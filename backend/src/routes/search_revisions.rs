use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::search_guardrail_config;
use crate::knowledge::search_event::EnrichmentGap;
use crate::knowledge::SearchEvent;
use crate::search::journey::{
    buyer_brief, present_intent, project_current_results, result_delta, JourneyClarification,
    ResultDelta, SearchJourneyActive, SearchJourneyAttempt, SearchJourneyAttemptKind,
    SearchJourneyEnvelope, SearchJourneyOutcome, SearchJourneyResults, SearchJourneyRevision,
    SearchRevisionTarget, SelectedPropertyConsequence, SEARCH_JOURNEY_CONTRACT_VERSION,
};
use crate::search::proof::{resolve_proof_token, ProofResolution, ProofResolutionError};
use crate::search::{
    apply_typed_revision, compile_typed_revision, decode_signed_search_context, guard_search_query,
    issue_signed_search_context, result_membership_fingerprint, CompiledSearchPlan,
    GeoCellSearchPolicy, GeoScope, SearchRevisionLimits, SearchRevisionOperation,
    SearchRevisionOutcome, SearchRuntimeVersion, SignedSearchContext, TypedSearchRevision,
    TypedSearchRevisionPatch,
};
use crate::state::{
    AppState, CachedSearchOutput, RevisionIdempotencyLookup, RevisionReservationUpdate,
    SearchCacheKey, SearchCacheLookup, SearchRuntimeSnapshot,
};

use super::search::{
    compute_search, compute_search_plan, enqueue_cached_search_logs, enqueue_search_log,
    guarded_search_output, rebase_cached_response, search_runtime_version,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRevisionRequest {
    pub parent_token: String,
    pub parent_result_ids: Vec<String>,
    pub utterance: String,
    pub client_mutation_id: String,
    #[serde(default)]
    pub selected_property_id: Option<String>,
    #[serde(default)]
    pub target: Option<SearchRevisionTarget>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchResumeRequest {
    pub parent_token: String,
    pub known_result_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofResolveRequest {
    pub proof_token: String,
    #[serde(default)]
    pub property_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchJourneyErrorBody {
    code: String,
    message: String,
    runtime_version: Box<SearchRuntimeVersion>,
}

pub(crate) struct SearchJourneyHttpError {
    status: StatusCode,
    body: SearchJourneyErrorBody,
}

impl IntoResponse for SearchJourneyHttpError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

pub(crate) struct SearchJourneyService {
    state: Arc<AppState>,
    snapshot: Arc<SearchRuntimeSnapshot>,
    runtime_version: SearchRuntimeVersion,
}

impl SearchJourneyService {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        let snapshot = state.search_runtime.load_full();
        let runtime_version = search_runtime_version(&snapshot);
        Self {
            state,
            snapshot,
            runtime_version,
        }
    }

    pub(crate) async fn start(
        &self,
        query: String,
    ) -> Result<SearchJourneyEnvelope, SearchJourneyHttpError> {
        let output = if query.trim().is_empty() {
            guarded_search_output(&self.snapshot, query.clone(), None)
        } else if let Some(guarded) = guard_search_query(&query) {
            if guarded_search_has_local_recall(&self.snapshot, &query, &guarded) {
                self.cached_initial_search(&query).await?
            } else {
                let mut event = SearchEvent::new(query.clone(), guarded.intent, 0);
                event.enrichment_gaps.push(EnrichmentGap {
                    entity_id: "search:guardrail".to_string(),
                    missing_fact: guarded.guidance.mode.clone(),
                    reason: guarded.guidance.message.clone(),
                });
                enqueue_search_log(
                    &self.state,
                    crate::state::SearchLogMessage::SearchEvent(event),
                );
                guarded_search_output(&self.snapshot, query.clone(), Some(guarded.guidance))
            }
        } else {
            self.cached_initial_search(&query).await?
        };
        let active = self.issue_active(
            None,
            "initial",
            SearchRevisionOperation::Initial,
            1,
            output.compiled_plan.as_ref().clone(),
            output.response.as_ref(),
        )?;
        Ok(SearchJourneyEnvelope {
            contract_version: SEARCH_JOURNEY_CONTRACT_VERSION,
            runtime_version: self.runtime_version.clone(),
            active,
            attempt: SearchJourneyAttempt {
                kind: SearchJourneyAttemptKind::Initial,
                operation: SearchRevisionOperation::Initial,
                outcome: SearchJourneyOutcome::Activated,
                catalog_rebased: false,
                catalog_delta: None,
                intent_delta: None,
                attempted_intent: None,
                clarification: None,
                selected_property_consequence: None,
            },
        })
    }

    pub(crate) async fn revise(
        &self,
        request: SearchRevisionRequest,
    ) -> Result<SearchJourneyEnvelope, SearchJourneyHttpError> {
        validate_revision_request(&request, &self.runtime_version)?;
        let parent = self.decode_parent(&request.parent_token)?;
        self.validate_result_ids(&request.parent_result_ids, "invalid_revision_request")?;
        self.validate_result_fingerprint(&request.parent_result_ids, &parent)?;
        let request_fingerprint = revision_request_fingerprint(&request);
        let reservation = loop {
            match self.state.search_revision_caches.lookup_or_reserve(
                &parent.revision_id,
                &request.client_mutation_id,
                &request_fingerprint,
            ) {
                RevisionIdempotencyLookup::Hit(response) => {
                    return Ok(response.as_ref().clone());
                }
                RevisionIdempotencyLookup::Conflict => {
                    return Err(self.error(
                        StatusCode::CONFLICT,
                        "client_mutation_id_conflict",
                        "This client mutation ID was already used for a different revision.",
                    ));
                }
                RevisionIdempotencyLookup::Leader(reservation) => break reservation,
                RevisionIdempotencyLookup::Waiter(mut receiver) => loop {
                    match receiver.borrow().clone() {
                        RevisionReservationUpdate::Complete(response) => {
                            return Ok(response.as_ref().clone());
                        }
                        RevisionReservationUpdate::Abandoned => break,
                        RevisionReservationUpdate::Pending => {}
                    }
                    if receiver.changed().await.is_err() {
                        break;
                    }
                },
            }
        };
        let response = self.revise_uncached(&parent, &request).await?;
        reservation.complete(response.clone());
        Ok(response)
    }

    pub(crate) async fn resume(
        &self,
        request: SearchResumeRequest,
    ) -> Result<SearchJourneyEnvelope, SearchJourneyHttpError> {
        if request.parent_token.trim().is_empty() {
            return Err(self.error(
                StatusCode::BAD_REQUEST,
                "invalid_resume_request",
                "Parent token is required.",
            ));
        }
        let parent = self.decode_parent(&request.parent_token)?;
        self.validate_result_ids(&request.known_result_ids, "invalid_resume_request")?;
        self.validate_result_fingerprint(&request.known_result_ids, &parent)?;
        let catalog_rebased = parent.runtime_lineage != self.runtime_version;
        let Some(parent_plan) = self.rebind_parent(&parent).await? else {
            return Ok(self.retained_envelope(
                &parent,
                request.parent_token,
                request.known_result_ids,
                SearchJourneyAttemptKind::Resume,
                SearchRevisionOperation::Resume,
                catalog_rebased,
                "catalogRebaseUnresolved",
            ));
        };
        let output = self
            .execute_plan(parent_plan.clone(), parent.buyer_brief.clone())
            .await?;
        let active = self.issue_active(
            Some(parent.revision_id.clone()),
            "resume",
            SearchRevisionOperation::Resume,
            parent.depth,
            parent_plan,
            output.response.as_ref(),
        )?;
        let catalog_delta = result_delta(
            &request.known_result_ids,
            active.results.ordered_result_ids(),
        );
        Ok(SearchJourneyEnvelope {
            contract_version: SEARCH_JOURNEY_CONTRACT_VERSION,
            runtime_version: self.runtime_version.clone(),
            active,
            attempt: SearchJourneyAttempt {
                kind: SearchJourneyAttemptKind::Resume,
                operation: SearchRevisionOperation::Resume,
                outcome: SearchJourneyOutcome::Resumed,
                catalog_rebased,
                catalog_delta: Some(catalog_delta),
                intent_delta: None,
                attempted_intent: None,
                clarification: None,
                selected_property_consequence: None,
            },
        })
    }

    pub(crate) fn resolve_proof(
        &self,
        request: ProofResolveRequest,
    ) -> Result<ProofResolution, SearchJourneyHttpError> {
        if request.proof_token.trim().is_empty() {
            return Err(self.error(
                StatusCode::BAD_REQUEST,
                "invalid_proof_request",
                "Proof token is required.",
            ));
        }
        resolve_proof_token(
            &self.snapshot,
            &request.proof_token,
            request.property_id.as_deref(),
        )
        .map_err(|error| self.proof_error(error))
    }

    async fn revise_uncached(
        &self,
        parent: &SignedSearchContext,
        request: &SearchRevisionRequest,
    ) -> Result<SearchJourneyEnvelope, SearchJourneyHttpError> {
        let catalog_rebased = parent.runtime_lineage != self.runtime_version;
        let Some(parent_plan) = self.rebind_parent(parent).await? else {
            return Ok(self.retained_envelope(
                parent,
                request.parent_token.clone(),
                request.parent_result_ids.clone(),
                SearchJourneyAttemptKind::Revision,
                SearchRevisionOperation::Refine,
                catalog_rebased,
                "catalogRebaseUnresolved",
            ));
        };
        let refreshed_parent = self
            .execute_plan(parent_plan.clone(), parent.buyer_brief.clone())
            .await?;
        let refreshed_active = self.issue_active(
            Some(parent.revision_id.clone()),
            "catalog-refresh",
            parent.operation,
            parent.depth,
            parent_plan.clone(),
            refreshed_parent.response.as_ref(),
        )?;
        let catalog_delta = result_delta(
            &request.parent_result_ids,
            refreshed_active.results.ordered_result_ids(),
        );

        if parent.depth >= search_guardrail_config().revisions.max_revision_depth {
            return Ok(self.inactive_envelope(
                refreshed_active,
                SearchRevisionOperation::Refine,
                SearchJourneyOutcome::LimitReached,
                catalog_rebased,
                catalog_delta,
                None,
                "limitReached",
                "This search has reached its revision limit.",
                None,
            ));
        }

        let (fragment, mut revision) = self.compile_revision(parent, request, &parent_plan).await?;
        apply_explicit_target(
            &mut revision,
            request.target.as_ref(),
            &parent_plan,
            &fragment,
        )
        .map_err(|code| {
            self.error(
                StatusCode::CONFLICT,
                code,
                "The target does not belong to the signed parent intent.",
            )
        })?;
        if revision.outcome != SearchRevisionOutcome::Candidate {
            let (outcome, code, message) = match revision.outcome {
                SearchRevisionOutcome::RequireClarification => (
                    SearchJourneyOutcome::ClarificationRequired,
                    "clarificationRequired",
                    "Choose the branch or condition this change should update.",
                ),
                SearchRevisionOutcome::RequireCheckpoint => (
                    SearchJourneyOutcome::LimitReached,
                    "limitReached",
                    "This search has reached its revision limit.",
                ),
                SearchRevisionOutcome::Candidate => unreachable!(),
            };
            return Ok(self.inactive_envelope(
                refreshed_active,
                revision.operation,
                outcome,
                catalog_rebased,
                catalog_delta,
                None,
                code,
                message,
                None,
            ));
        }
        let candidate_plan = apply_typed_revision(
            &parent_plan,
            &fragment,
            &revision,
            &self.snapshot.bundle.graph_index,
            Some(&self.snapshot.bundle.spatial_index),
            GeoCellSearchPolicy {
                max_hops: self.snapshot.geo_cell_max_hops,
                max_distance_km: self.snapshot.geo_cell_max_distance_km,
            },
        )
        .ok_or_else(|| {
            self.error(
                StatusCode::BAD_REQUEST,
                "invalid_revision_patch",
                "The revision could not be applied to this search.",
            )
        })?;
        let attempted_intent = present_intent(&candidate_plan);
        if has_unresolved_required_geography(&candidate_plan)
            || !candidate_plan
                .aggregate_intent
                .unsupported_inventory_types
                .is_empty()
            || has_unsupported_required_preference(&self.snapshot, &candidate_plan)
        {
            return Ok(self.inactive_envelope(
                refreshed_active,
                revision.operation,
                SearchJourneyOutcome::PreservedParent,
                catalog_rebased,
                catalog_delta,
                Some(attempted_intent),
                "requiredCapabilityUnavailable",
                "That required condition is unavailable in the current catalog.",
                None,
            ));
        }

        let candidate_brief = buyer_brief(&attempted_intent);
        let candidate = self
            .execute_plan(candidate_plan.clone(), candidate_brief)
            .await?;
        let candidate_ids = candidate.response.ordered_result_ids.clone();
        let intent_delta = result_delta(
            refreshed_active.results.ordered_result_ids(),
            &candidate_ids,
        );
        let selected_property_consequence = request.selected_property_id.as_ref().map(|selected| {
            if candidate_ids.contains(selected) {
                SelectedPropertyConsequence::Retained
            } else {
                SelectedPropertyConsequence::Excluded
            }
        });
        if candidate_ids.is_empty() {
            return Ok(self
                .inactive_envelope(
                    refreshed_active,
                    revision.operation,
                    SearchJourneyOutcome::PreservedParent,
                    catalog_rebased,
                    catalog_delta,
                    Some(attempted_intent),
                    "zeroResults",
                    "That change has no matching homes, so the current search is unchanged.",
                    selected_property_consequence,
                )
                .with_intent_delta(intent_delta));
        }

        let active = self.issue_active(
            Some(parent.revision_id.clone()),
            &request.client_mutation_id,
            revision.operation,
            parent.depth + 1,
            candidate_plan,
            candidate.response.as_ref(),
        )?;
        Ok(SearchJourneyEnvelope {
            contract_version: SEARCH_JOURNEY_CONTRACT_VERSION,
            runtime_version: self.runtime_version.clone(),
            active,
            attempt: SearchJourneyAttempt {
                kind: SearchJourneyAttemptKind::Revision,
                operation: revision.operation,
                outcome: SearchJourneyOutcome::Activated,
                catalog_rebased,
                catalog_delta: Some(catalog_delta),
                intent_delta: Some(intent_delta),
                attempted_intent: Some(attempted_intent),
                clarification: None,
                selected_property_consequence,
            },
        })
    }

    async fn compile_revision(
        &self,
        parent: &SignedSearchContext,
        request: &SearchRevisionRequest,
        parent_plan: &CompiledSearchPlan,
    ) -> Result<(CompiledSearchPlan, TypedSearchRevision), SearchJourneyHttpError> {
        let snapshot = self.snapshot.clone();
        let parent_plan = parent_plan.clone();
        let utterance = request.utterance.clone();
        let parent_revision_id = parent.revision_id.clone();
        let target_branch_id = request.target.as_ref().map(|target| match target {
            SearchRevisionTarget::Branch { branch_id }
            | SearchRevisionTarget::Predicate { branch_id, .. } => branch_id.clone(),
        });
        let source_turn_id = format!("{}:turn", parent.revision_id);
        self.state
            .execution
            .run_customer_compute(move || {
                let engine = crate::search::SearchEngine::new(&snapshot);
                let fragment = engine.compile_fragment(&utterance, &source_turn_id, &parent_plan);
                let mut compilation_parent = parent_plan.clone();
                if let Some(target_branch_id) = target_branch_id {
                    compilation_parent
                        .branches
                        .retain(|branch| branch.branch_id == target_branch_id);
                    if compilation_parent.branches.len() == 1 {
                        compilation_parent.root = crate::search::BoolExpr::Leaf(target_branch_id);
                    }
                }
                let revision = compile_typed_revision(
                    &compilation_parent,
                    &fragment,
                    &utterance,
                    &parent_revision_id,
                    SearchRevisionLimits {
                        max_active_branches: search_guardrail_config()
                            .revisions
                            .max_active_branches,
                    },
                );
                (fragment, revision)
            })
            .await
            .map_err(|_| self.unavailable())
    }

    async fn rebind_parent(
        &self,
        parent: &SignedSearchContext,
    ) -> Result<Option<CompiledSearchPlan>, SearchJourneyHttpError> {
        let snapshot = self.snapshot.clone();
        let intent_ast = parent.intent_ast.clone();
        self.state
            .execution
            .run_customer_compute(move || {
                let plan = crate::search::SearchEngine::new(&snapshot)
                    .compile_intent_ast(&intent_ast)
                    .ok()?;
                (!has_unresolved_required_geography(&plan)).then_some(plan)
            })
            .await
            .map_err(|_| self.unavailable())
    }

    async fn execute_plan(
        &self,
        plan: CompiledSearchPlan,
        brief: String,
    ) -> Result<CachedSearchOutput, SearchJourneyHttpError> {
        let semantic_cache_key = execution_cache_key(&plan, &self.runtime_version);
        if let Some(cached) = self
            .state
            .search_revision_caches
            .semantic_get(&semantic_cache_key)
        {
            enqueue_cached_search_logs(&self.state, &cached, &brief);
            return Ok(rebase_cached_output(cached, &brief));
        }
        let snapshot = self.snapshot.clone();
        let output = self
            .state
            .execution
            .run_customer_compute(move || compute_search_plan(snapshot, plan, brief))
            .await
            .map_err(|_| self.unavailable())?
            .ok_or_else(|| self.unavailable())?;
        for message in output.log_messages.clone() {
            enqueue_search_log(&self.state, message);
        }
        self.state
            .search_revision_caches
            .semantic_insert(semantic_cache_key, output.clone());
        Ok(output)
    }

    async fn cached_initial_search(
        &self,
        query: &str,
    ) -> Result<CachedSearchOutput, SearchJourneyHttpError> {
        let cache_key = SearchCacheKey::new(query, &self.snapshot.version_key);
        let reservation = loop {
            match self.state.search_cache.lookup_or_reserve(&cache_key).await {
                SearchCacheLookup::Hit(cached) => {
                    enqueue_cached_search_logs(&self.state, &cached, query);
                    return Ok(rebase_cached_output(cached, query));
                }
                SearchCacheLookup::Waiter(mut receiver) => {
                    match receiver.wait_for(Option::is_some).await {
                        Ok(cached) => {
                            let cached = cached.as_ref().expect("watch value is present");
                            enqueue_cached_search_logs(&self.state, cached, query);
                            return Ok(rebase_cached_output(cached.clone(), query));
                        }
                        Err(_) => continue,
                    }
                }
                SearchCacheLookup::Leader(reservation) => break reservation,
                SearchCacheLookup::Overloaded => return Err(self.unavailable()),
            }
        };
        let snapshot = self.snapshot.clone();
        let work_query = query.to_string();
        let output = self
            .state
            .execution
            .run_customer_compute(move || compute_search(snapshot, work_query))
            .await
            .map_err(|_| self.unavailable())?;
        for message in output.log_messages.clone() {
            enqueue_search_log(&self.state, message);
        }
        reservation.complete(output.clone()).await;
        Ok(output)
    }

    fn issue_active(
        &self,
        parent_revision_id: Option<String>,
        mutation_id: &str,
        operation: SearchRevisionOperation,
        depth: usize,
        plan: CompiledSearchPlan,
        execution: &crate::search::SearchExecution,
    ) -> Result<SearchJourneyActive, SearchJourneyHttpError> {
        let intent = present_intent(&plan);
        let brief = buyer_brief(&intent);
        let issued = issue_signed_search_context(
            parent_revision_id,
            mutation_id,
            operation,
            depth,
            brief.clone(),
            plan.clone(),
            self.runtime_version.clone(),
            execution.ordered_result_ids.clone(),
        )
        .map_err(|_| {
            self.error(
                StatusCode::CONFLICT,
                "journey_limit_reached",
                "This search cannot fit in a portable journey token.",
            )
        })?;
        Ok(SearchJourneyActive {
            revision: SearchJourneyRevision {
                id: issued.context.revision_id.clone(),
                parent_id: issued.context.parent_revision_id.clone(),
                operation: issued.context.operation,
                depth: issued.context.depth,
                semantic_fingerprint: issued.context.semantic_fingerprint.clone(),
                result_fingerprint: issued.context.result_fingerprint.clone(),
                state_token: issued.state_token,
            },
            buyer_brief: brief,
            intent,
            results: project_current_results(&self.snapshot, execution, &plan),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn inactive_envelope(
        &self,
        active: SearchJourneyActive,
        operation: SearchRevisionOperation,
        outcome: SearchJourneyOutcome,
        catalog_rebased: bool,
        catalog_delta: ResultDelta,
        attempted_intent: Option<crate::search::journey::IntentPresentation>,
        code: &str,
        message: &str,
        selected_property_consequence: Option<SelectedPropertyConsequence>,
    ) -> SearchJourneyEnvelope {
        SearchJourneyEnvelope {
            contract_version: SEARCH_JOURNEY_CONTRACT_VERSION,
            runtime_version: self.runtime_version.clone(),
            active,
            attempt: SearchJourneyAttempt {
                kind: SearchJourneyAttemptKind::Revision,
                operation,
                outcome,
                catalog_rebased,
                catalog_delta: Some(catalog_delta),
                intent_delta: None,
                attempted_intent,
                clarification: Some(JourneyClarification {
                    code: code.to_string(),
                    message: message.to_string(),
                }),
                selected_property_consequence,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn retained_envelope(
        &self,
        parent: &SignedSearchContext,
        state_token: String,
        result_ids: Vec<String>,
        kind: SearchJourneyAttemptKind,
        operation: SearchRevisionOperation,
        catalog_rebased: bool,
        code: &str,
    ) -> SearchJourneyEnvelope {
        let intent = portable_intent_presentation(parent);
        SearchJourneyEnvelope {
            contract_version: SEARCH_JOURNEY_CONTRACT_VERSION,
            runtime_version: self.runtime_version.clone(),
            active: SearchJourneyActive {
                revision: SearchJourneyRevision {
                    id: parent.revision_id.clone(),
                    parent_id: parent.parent_revision_id.clone(),
                    operation: parent.operation,
                    depth: parent.depth,
                    semantic_fingerprint: parent.semantic_fingerprint.clone(),
                    result_fingerprint: parent.result_fingerprint.clone(),
                    state_token,
                },
                buyer_brief: parent.buyer_brief.clone(),
                intent,
                results: SearchJourneyResults::Retained {
                    ordered_result_ids: result_ids,
                    result_fingerprint: parent.result_fingerprint.clone(),
                },
            },
            attempt: SearchJourneyAttempt {
                kind,
                operation,
                outcome: SearchJourneyOutcome::PreservedParent,
                catalog_rebased,
                catalog_delta: None,
                intent_delta: None,
                attempted_intent: None,
                clarification: Some(JourneyClarification {
                    code: code.to_string(),
                    message: "The current catalog cannot rebind every required condition."
                        .to_string(),
                }),
                selected_property_consequence: None,
            },
        }
    }

    fn decode_parent(&self, token: &str) -> Result<SignedSearchContext, SearchJourneyHttpError> {
        decode_signed_search_context(token).map_err(|_| {
            self.error(
                StatusCode::BAD_REQUEST,
                "invalid_parent_token",
                "The parent journey token is invalid.",
            )
        })
    }

    fn validate_result_fingerprint(
        &self,
        result_ids: &[String],
        parent: &SignedSearchContext,
    ) -> Result<(), SearchJourneyHttpError> {
        if result_membership_fingerprint(result_ids) != parent.result_fingerprint {
            return Err(self.error(
                StatusCode::CONFLICT,
                "parent_results_mismatch",
                "The supplied results do not belong to this journey revision.",
            ));
        }
        Ok(())
    }

    fn validate_result_ids(
        &self,
        result_ids: &[String],
        code: &str,
    ) -> Result<(), SearchJourneyHttpError> {
        let max_ids = crate::search::schema::ranking_policy().result_limit;
        let max_id_bytes = crate::security::security_tuning()
            .search_journey
            .max_result_id_bytes;
        let unique = result_ids.iter().collect::<std::collections::HashSet<_>>();
        if result_ids.len() > max_ids
            || unique.len() != result_ids.len()
            || result_ids
                .iter()
                .any(|id| id.trim().is_empty() || id.len() > max_id_bytes)
        {
            return Err(self.error(
                StatusCode::BAD_REQUEST,
                code,
                "Result IDs exceed the search journey transport limits.",
            ));
        }
        Ok(())
    }

    fn proof_error(&self, error: ProofResolutionError) -> SearchJourneyHttpError {
        let status = match error {
            ProofResolutionError::InvalidToken => StatusCode::BAD_REQUEST,
            ProofResolutionError::StaleSnapshot
            | ProofResolutionError::WrongProperty
            | ProofResolutionError::WrongSubject
            | ProofResolutionError::DestinationMismatch => StatusCode::CONFLICT,
            ProofResolutionError::MissingEvidence => StatusCode::NOT_FOUND,
        };
        self.error(
            status,
            error.code(),
            "The requested proof could not be resolved.",
        )
    }

    fn unavailable(&self) -> SearchJourneyHttpError {
        self.error(
            StatusCode::SERVICE_UNAVAILABLE,
            "search_unavailable",
            "Search is temporarily unavailable.",
        )
    }

    fn error(&self, status: StatusCode, code: &str, message: &str) -> SearchJourneyHttpError {
        SearchJourneyHttpError {
            status,
            body: SearchJourneyErrorBody {
                code: code.to_string(),
                message: message.to_string(),
                runtime_version: Box::new(self.runtime_version.clone()),
            },
        }
    }
}

fn execution_cache_key(
    plan: &CompiledSearchPlan,
    runtime_version: &SearchRuntimeVersion,
) -> String {
    let identity = serde_json::json!({
        "runtimeVersion": runtime_version,
        "intentAst": crate::search::TypedIntentAst::from_plan(plan),
        "branchLabels": plan
            .branches
            .iter()
            .map(|branch| branch.buyer_summary.as_str())
            .collect::<Vec<_>>(),
    });
    let digest = Sha256::digest(
        serde_json::to_vec(&identity).expect("search execution cache identity serializes"),
    );
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

trait WithIntentDelta {
    fn with_intent_delta(self, delta: ResultDelta) -> Self;
}

impl WithIntentDelta for SearchJourneyEnvelope {
    fn with_intent_delta(mut self, delta: ResultDelta) -> Self {
        self.attempt.intent_delta = Some(delta);
        self
    }
}

pub async fn revise_search(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<SearchRevisionRequest>, JsonRejection>,
) -> Response {
    let service = SearchJourneyService::new(state);
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => return invalid_transport(&service, rejection).into_response(),
    };
    match service.revise(request).await {
        Ok(envelope) => Json(envelope).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn resume_search(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<SearchResumeRequest>, JsonRejection>,
) -> Response {
    let service = SearchJourneyService::new(state);
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => return invalid_transport(&service, rejection).into_response(),
    };
    match service.resume(request).await {
        Ok(envelope) => Json(envelope).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn resolve_search_proof(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<ProofResolveRequest>, JsonRejection>,
) -> Response {
    let service = SearchJourneyService::new(state);
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => return invalid_transport(&service, rejection).into_response(),
    };
    match service.resolve_proof(request) {
        Ok(resolution) => Json(resolution).into_response(),
        Err(error) => error.into_response(),
    }
}

fn invalid_transport(
    service: &SearchJourneyService,
    rejection: JsonRejection,
) -> SearchJourneyHttpError {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return service.error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "search_body_too_large",
            "The search request body exceeds the configured transport limit.",
        );
    }
    service.error(
        StatusCode::BAD_REQUEST,
        "invalid_transport_input",
        "The request body does not match the search journey contract.",
    )
}

fn validate_revision_request(
    request: &SearchRevisionRequest,
    runtime_version: &SearchRuntimeVersion,
) -> Result<(), SearchJourneyHttpError> {
    let error = |status, code: &str, message: &str| SearchJourneyHttpError {
        status,
        body: SearchJourneyErrorBody {
            code: code.to_string(),
            message: message.to_string(),
            runtime_version: Box::new(runtime_version.clone()),
        },
    };
    if request.parent_token.trim().is_empty()
        || request.utterance.trim().is_empty()
        || request.client_mutation_id.trim().is_empty()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_revision_request",
            "Parent token, utterance, and client mutation ID are required.",
        ));
    }
    if request.utterance.len()
        > crate::security::security_tuning()
            .requests
            .max_search_query_bytes
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "revision_utterance_too_long",
            "The revision utterance is too long.",
        ));
    }
    let journey_tuning = &crate::security::security_tuning().search_journey;
    let target_too_long = request.target.as_ref().is_some_and(|target| match target {
        SearchRevisionTarget::Branch { branch_id } => {
            branch_id.len() > journey_tuning.max_target_id_bytes
        }
        SearchRevisionTarget::Predicate {
            branch_id,
            predicate_id,
        } => {
            branch_id.len() > journey_tuning.max_target_id_bytes
                || predicate_id.len() > journey_tuning.max_target_id_bytes
        }
    });
    if request.client_mutation_id.len() > journey_tuning.max_client_mutation_id_bytes
        || request
            .selected_property_id
            .as_ref()
            .is_some_and(|id| id.len() > journey_tuning.max_result_id_bytes)
        || target_too_long
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "revision_identity_too_long",
            "The revision identity exceeds the transport limit.",
        ));
    }
    Ok(())
}

fn revision_request_fingerprint(request: &SearchRevisionRequest) -> String {
    let payload = serde_json::to_vec(&serde_json::json!({
        "parentResultIds": request.parent_result_ids,
        "utterance": request.utterance,
        "selectedPropertyId": request.selected_property_id,
        "target": request.target,
    }))
    .expect("revision request fingerprint serializes");
    let digest = Sha256::digest(payload);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn rebase_cached_output(mut cached: CachedSearchOutput, query: &str) -> CachedSearchOutput {
    cached.response = Arc::new(rebase_cached_response(cached.response.as_ref(), query));
    cached
}

fn guarded_search_has_local_recall(
    snapshot: &SearchRuntimeSnapshot,
    query: &str,
    guarded: &crate::search::guard::GuardedSearch,
) -> bool {
    if !matches!(
        guarded.guidance.mode.as_str(),
        "too_short" | "out_of_scope" | "needs_home_anchor"
    ) {
        return false;
    }
    let compiled = crate::search::IntentAst::from_text(query);
    !snapshot
        .search_index
        .recall_named_entity_ids(&compiled)
        .is_empty()
}

fn apply_explicit_target(
    revision: &mut TypedSearchRevision,
    target: Option<&SearchRevisionTarget>,
    parent: &CompiledSearchPlan,
    fragment: &CompiledSearchPlan,
) -> Result<(), &'static str> {
    let Some(target) = target else {
        return Ok(());
    };
    let expression = fragment
        .branches
        .first()
        .map(|branch| branch.predicates.clone())
        .ok_or("revision_target_mismatch")?;
    match target {
        SearchRevisionTarget::Branch { branch_id } => {
            if !parent
                .branches
                .iter()
                .any(|branch| branch.branch_id == *branch_id)
            {
                return Err("revision_target_mismatch");
            }
            for patch in &mut revision.patches {
                match patch {
                    TypedSearchRevisionPatch::AddPredicate { branch_ids, .. }
                    | TypedSearchRevisionPatch::ReplacePredicate { branch_ids, .. }
                    | TypedSearchRevisionPatch::ReplacePreference { branch_ids, .. } => {
                        *branch_ids = vec![branch_id.clone()];
                    }
                    TypedSearchRevisionPatch::AddAlternative { .. }
                    | TypedSearchRevisionPatch::ReplaceIntent => {}
                }
            }
        }
        SearchRevisionTarget::Predicate {
            branch_id,
            predicate_id,
        } => {
            let Some(branch) = parent
                .branches
                .iter()
                .find(|branch| branch.branch_id == *branch_id)
            else {
                return Err("revision_target_mismatch");
            };
            if branch
                .predicate_bindings
                .iter()
                .any(|binding| binding.predicate_id == *predicate_id)
            {
                revision.outcome = SearchRevisionOutcome::Candidate;
                revision.patches = vec![TypedSearchRevisionPatch::ReplacePredicate {
                    branch_ids: vec![branch_id.clone()],
                    families: Vec::new(),
                    predicate_ids: vec![predicate_id.clone()],
                    expression,
                }];
                return Ok(());
            }
            let targeted_preference = branch
                .ranking_intent
                .positive_preferences
                .iter()
                .chain(branch.ranking_intent.negative_preferences.iter())
                .find(|preference| {
                    crate::search::revision::ranking_preference_id(branch_id, preference)
                        == *predicate_id
                });
            if targeted_preference.is_none() {
                return Err("revision_target_mismatch");
            }
            let preferences = fragment
                .branches
                .first()
                .map(|branch| {
                    branch
                        .ranking_intent
                        .positive_preferences
                        .iter()
                        .chain(branch.ranking_intent.negative_preferences.iter())
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if preferences.is_empty() {
                return Err("revision_target_mismatch");
            }
            revision.outcome = SearchRevisionOutcome::Candidate;
            revision.patches = vec![TypedSearchRevisionPatch::ReplacePreference {
                branch_ids: vec![branch_id.clone()],
                preference_ids: vec![predicate_id.clone()],
                preferences,
            }];
        }
    }
    Ok(())
}

fn has_unsupported_required_preference(
    snapshot: &SearchRuntimeSnapshot,
    plan: &CompiledSearchPlan,
) -> bool {
    plan.branches.iter().any(|branch| {
        branch
            .ranking_intent
            .positive_preferences
            .iter()
            .chain(branch.ranking_intent.negative_preferences.iter())
            .any(|preference| {
                preference.required
                    && !snapshot
                        .bundle
                        .search_capabilities
                        .supports_preference(preference)
            })
    })
}

fn has_unresolved_required_geography(plan: &CompiledSearchPlan) -> bool {
    plan.branches.iter().any(|branch| {
        let GeoScope::Unresolved { anchors, .. } = &branch.geo_scope else {
            return false;
        };
        anchors.is_empty()
            || anchors
                .iter()
                .any(|anchor| !anchor.entity_type.eq_ignore_ascii_case("society"))
            || has_positive_non_society_geography(&branch.predicates, false)
    })
}

fn has_positive_non_society_geography(
    expression: &crate::search::ConstraintExpr,
    negated: bool,
) -> bool {
    match expression {
        crate::search::ConstraintExpr::And { clauses }
        | crate::search::ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| has_positive_non_society_geography(clause, negated)),
        crate::search::ConstraintExpr::Not { clause } => {
            has_positive_non_society_geography(clause, !negated)
        }
        crate::search::ConstraintExpr::Term {
            term:
                crate::search::ConstraintTerm::Area {
                    entity_id: Some(_), ..
                },
        } => !negated,
        crate::search::ConstraintExpr::Term {
            term:
                crate::search::ConstraintTerm::Spatial {
                    entity_id,
                    required: true,
                    ..
                },
        } => !negated && !entity_id.is_empty(),
        crate::search::ConstraintExpr::Term { .. } => false,
    }
}

fn portable_intent_presentation(
    parent: &SignedSearchContext,
) -> crate::search::journey::IntentPresentation {
    let branches = parent
        .intent_ast
        .branches
        .iter()
        .map(|branch| crate::search::compiled_plan::GeoBranch {
            branch_id: branch.branch_id.clone(),
            geo_cluster_id: branch.branch_id.clone(),
            source_spans: Vec::new(),
            geo_scope: GeoScope::BundleWide,
            predicates: branch.predicates.clone(),
            eligibility_predicates: branch.predicates.clone(),
            spatial_predicates: Vec::new(),
            predicate_bindings: branch.predicate_bindings.clone(),
            resolved_entities: branch.resolved_entities.clone(),
            ranking_intent: branch.ranking_intent.clone(),
            buyer_summary: String::new(),
            recall_query: String::new(),
            scoring_query: String::new(),
            fallback_text: None,
        })
        .collect::<Vec<_>>();
    present_intent(&CompiledSearchPlan {
        root: parent.intent_ast.root.clone(),
        branches,
        resolution_gaps: Vec::new(),
        aggregate_intent: parent.intent_ast.aggregate_intent.clone(),
        semantic_fingerprint: parent.semantic_fingerprint.clone(),
        snapshot_identity: parent.runtime_lineage.serving_bundle_version.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_cache_identity_keeps_stable_branch_and_predicate_ids_isolated() {
        let plan = CompiledSearchPlan::compile_for_snapshot(
            crate::search::IntentAst::from_text("3bhk under 2.4cr"),
            "cache-test-bundle",
            &[],
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        );
        let mut renamed = plan.clone();
        renamed.branches[0].branch_id = "branch:another-journey".to_string();
        for binding in &mut renamed.branches[0].predicate_bindings {
            binding.predicate_id = format!("another: {}", binding.predicate_id);
        }
        renamed.root = crate::search::BoolExpr::Leaf("branch:another-journey".to_string());
        renamed.refresh_semantic_fingerprint();
        assert_eq!(plan.semantic_fingerprint, renamed.semantic_fingerprint);

        let runtime = SearchRuntimeVersion {
            serving_bundle_version: "cache-test-bundle".to_string(),
            scoring_policy_version: 1,
            search_engine_version: "cache-test-engine".to_string(),
            semantic_contract_digest: "sha256:cache-test".to_string(),
        };
        assert_ne!(
            execution_cache_key(&plan, &runtime),
            execution_cache_key(&renamed, &runtime)
        );
        assert_eq!(
            execution_cache_key(&plan, &runtime),
            execution_cache_key(&plan.clone(), &runtime)
        );
    }
}
