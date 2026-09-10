use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::dag_config::search_guardrail_config;
use crate::search::{
    apply_typed_revision, compile_typed_revision, decode_signed_search_context, intent_breakdown,
    issue_signed_search_context, render_revision_active_query, result_membership_fingerprint,
    BuyerIntentBranchProjection, CompiledSearchPlan, GeoCellSearchPolicy, GeoScope, SearchResponse,
    SearchRevisionLimits, SearchRevisionOperation, SearchRevisionOutcome, SearchRuntimeVersion,
    TypedSearchRevision,
};
use crate::state::{
    AppState, RevisionIdempotencyLookup, RevisionReservationUpdate, RuntimeVersionKey,
};

use super::search::{compute_search_plan, enqueue_cached_search_logs};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRevisionRequest {
    pub parent_token: String,
    pub parent_result_ids: Vec<String>,
    pub utterance: String,
    pub client_mutation_id: String,
    #[serde(default)]
    pub selected_property_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RevisionActivationOutcome {
    Activate,
    PreserveParent,
    ClarificationRequired,
    LimitReached,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRevisionResponse {
    pub operation: SearchRevisionOperation,
    pub outcome: RevisionActivationOutcome,
    pub catalog_rebased: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<SearchResponse>,
    pub result_delta: ResultDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_property_consequence: Option<String>,
    pub attempted_breakdown: Vec<BuyerIntentBranchProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<RevisionGuidance>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub retained: Vec<String>,
    pub reordered: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionGuidance {
    pub code: String,
    pub message: String,
    pub suggested_action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionError {
    code: String,
    message: String,
    runtime_version: SearchRuntimeVersion,
}

enum RevisionCompilation {
    RebaseUnavailable,
    Ready(Box<RevisionCompilationReady>),
}

struct RevisionCompilationReady {
    fragment: CompiledSearchPlan,
    revision: TypedSearchRevision,
    candidate_plan: Option<CompiledSearchPlan>,
}

pub async fn revise_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRevisionRequest>,
) -> Response {
    let snapshot = state.search_runtime.load_full();
    let runtime_version = runtime_version(&snapshot.version_key);
    if request.parent_token.trim().is_empty()
        || request.utterance.trim().is_empty()
        || request.client_mutation_id.trim().is_empty()
    {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "invalid_revision_request",
            "Parent token, utterance, and client mutation ID are required.",
            runtime_version,
        );
    }
    if request.utterance.len()
        > crate::security::security_tuning()
            .requests
            .max_search_query_bytes
    {
        return revision_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "revision_utterance_too_long",
            "The revision utterance is too long.",
            runtime_version,
        );
    }
    let parent = match decode_signed_search_context(&request.parent_token) {
        Ok(context) => context,
        Err(_) => {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "invalid_parent_token",
                "The parent revision token is invalid.",
                runtime_version,
            )
        }
    };
    if result_membership_fingerprint(&request.parent_result_ids) != parent.result_fingerprint {
        return revision_error(
            StatusCode::CONFLICT,
            "parent_results_mismatch",
            "The supplied parent results do not belong to this revision.",
            runtime_version,
        );
    }

    let request_fingerprint = revision_request_fingerprint(&request);
    let reservation = loop {
        match state.search_revision_caches.lookup_or_reserve(
            &parent.revision_id,
            &request.client_mutation_id,
            &request_fingerprint,
        ) {
            RevisionIdempotencyLookup::Hit(response) => return Json(response).into_response(),
            RevisionIdempotencyLookup::Conflict => {
                return revision_error(
                    StatusCode::CONFLICT,
                    "client_mutation_id_conflict",
                    "This client mutation ID was already used for a different revision.",
                    runtime_version,
                )
            }
            RevisionIdempotencyLookup::Leader(reservation) => break reservation,
            RevisionIdempotencyLookup::Waiter(mut receiver) => loop {
                match receiver.borrow().clone() {
                    RevisionReservationUpdate::Complete(response) => {
                        return Json(response).into_response()
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

    let catalog_rebased = parent.runtime_lineage != runtime_version;
    if parent.depth >= search_guardrail_config().revisions.max_revision_depth {
        let response = inactive_response(
            SearchRevisionOperation::Refine,
            RevisionActivationOutcome::LimitReached,
            catalog_rebased,
            Vec::new(),
            Some(limit_guidance()),
        );
        reservation.complete(response.clone());
        return Json(response).into_response();
    }

    let compilation_snapshot = snapshot.clone();
    let parent_intent_ast = parent.intent_ast.clone();
    let parent_revision_id = parent.revision_id.clone();
    let utterance = request.utterance.clone();
    let fragment_turn_id = format!("{}:turn", parent.revision_id);
    let max_active_branches = search_guardrail_config().revisions.max_active_branches;
    let compilation = state
        .execution
        .run_customer_compute(move || {
            let engine = crate::search::SearchEngine::new(&compilation_snapshot);
            let Ok(parent_plan) = engine.compile_intent_ast(&parent_intent_ast) else {
                return RevisionCompilation::RebaseUnavailable;
            };
            if has_unresolved_required_geography(&parent_plan) {
                return RevisionCompilation::RebaseUnavailable;
            }
            let fragment = engine.compile_fragment(&utterance, &fragment_turn_id, &parent_plan);
            let revision = compile_typed_revision(
                &parent_plan,
                &fragment,
                &utterance,
                &parent_revision_id,
                SearchRevisionLimits {
                    max_active_branches,
                },
            );
            let candidate_plan = (revision.outcome == SearchRevisionOutcome::Candidate)
                .then(|| {
                    apply_typed_revision(
                        &parent_plan,
                        &fragment,
                        &revision,
                        &compilation_snapshot.bundle.graph_index,
                        Some(&compilation_snapshot.bundle.spatial_index),
                        GeoCellSearchPolicy {
                            max_hops: compilation_snapshot.geo_cell_max_hops,
                            max_distance_km: compilation_snapshot.geo_cell_max_distance_km,
                        },
                    )
                })
                .flatten();
            RevisionCompilation::Ready(Box::new(RevisionCompilationReady {
                fragment,
                revision,
                candidate_plan,
            }))
        })
        .await;
    let (fragment, revision, candidate_plan) = match compilation {
        Ok(RevisionCompilation::RebaseUnavailable) => {
            let response = inactive_response(
                SearchRevisionOperation::Refine,
                RevisionActivationOutcome::PreserveParent,
                catalog_rebased,
                Vec::new(),
                Some(rebase_guidance()),
            );
            reservation.complete(response.clone());
            return Json(response).into_response();
        }
        Ok(RevisionCompilation::Ready(compilation)) => {
            let RevisionCompilationReady {
                fragment,
                revision,
                candidate_plan,
            } = *compilation;
            (fragment, revision, candidate_plan)
        }
        Err(_) => {
            return revision_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "search_unavailable",
                "Search is temporarily unavailable. Retry this revision.",
                runtime_version,
            )
        }
    };
    if revision.outcome != SearchRevisionOutcome::Candidate {
        let (outcome, guidance) = match revision.outcome {
            SearchRevisionOutcome::RequireClarification => (
                RevisionActivationOutcome::ClarificationRequired,
                clarification_guidance(),
            ),
            SearchRevisionOutcome::RequireCheckpoint => {
                (RevisionActivationOutcome::LimitReached, limit_guidance())
            }
            SearchRevisionOutcome::Candidate => unreachable!(),
        };
        let response = inactive_response(
            revision.operation,
            outcome,
            catalog_rebased,
            intent_breakdown(&fragment),
            Some(guidance),
        );
        reservation.complete(response.clone());
        return Json(response).into_response();
    }

    let Some(candidate_plan) = candidate_plan else {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "invalid_revision_patch",
            "The revision could not be applied to this search.",
            runtime_version,
        );
    };
    let attempted_breakdown = intent_breakdown(&candidate_plan);
    if has_unresolved_required_geography(&candidate_plan)
        || !candidate_plan
            .aggregate_intent
            .unsupported_inventory_types
            .is_empty()
    {
        let response = inactive_response(
            revision.operation,
            RevisionActivationOutcome::PreserveParent,
            catalog_rebased,
            attempted_breakdown,
            Some(required_evidence_guidance()),
        );
        reservation.complete(response.clone());
        return Json(response).into_response();
    }

    let active_query =
        render_revision_active_query(&parent.active_query, &request.utterance, revision.operation);
    let semantic_cache_key = format!(
        "{}:{}",
        candidate_plan.semantic_fingerprint,
        serde_json::to_string(&runtime_version).expect("runtime version serializes")
    );
    let candidate_output = if let Some(cached) = state
        .search_revision_caches
        .semantic_get(&semantic_cache_key)
    {
        cached
    } else {
        let execution_snapshot = snapshot.clone();
        let execution_plan = candidate_plan.clone();
        let execution_query = active_query.clone();
        let execution = state
            .execution
            .run_customer_compute(move || {
                compute_search_plan(execution_snapshot, execution_plan, execution_query)
            })
            .await;
        let Ok(Some(output)) = execution else {
            return revision_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "search_unavailable",
                "Search is temporarily unavailable. Retry this revision.",
                runtime_version,
            );
        };
        state
            .search_revision_caches
            .semantic_insert(semantic_cache_key, output.clone());
        output
    };
    enqueue_cached_search_logs(&state, &candidate_output, &active_query);
    let candidate_ids = candidate_output.response.ordered_result_ids.clone();
    let delta = result_delta(&request.parent_result_ids, &candidate_ids);
    let selected_property_consequence = request.selected_property_id.as_ref().map(|selected| {
        if candidate_ids.contains(selected) {
            "retained".to_string()
        } else {
            "excluded".to_string()
        }
    });
    if candidate_ids.is_empty() {
        let response = SearchRevisionResponse {
            operation: revision.operation,
            outcome: RevisionActivationOutcome::PreserveParent,
            catalog_rebased,
            candidate: None,
            result_delta: delta,
            selected_property_consequence,
            attempted_breakdown,
            guidance: Some(zero_results_guidance()),
        };
        reservation.complete(response.clone());
        return Json(response).into_response();
    }

    let (_, descriptor) = match issue_signed_search_context(
        Some(parent.revision_id.clone()),
        &request.client_mutation_id,
        revision.operation,
        parent.depth + 1,
        active_query.clone(),
        candidate_plan,
        runtime_version.clone(),
        candidate_ids,
    ) {
        Ok(issued) => issued,
        Err(_) => {
            let response = inactive_response(
                revision.operation,
                RevisionActivationOutcome::LimitReached,
                catalog_rebased,
                attempted_breakdown,
                Some(limit_guidance()),
            );
            reservation.complete(response.clone());
            return Json(response).into_response();
        }
    };
    let mut candidate = candidate_output.response.as_ref().clone();
    candidate.query = active_query;
    candidate.revision_id.clone_from(&descriptor.id);
    candidate.revision = Some(descriptor);
    let response = SearchRevisionResponse {
        operation: revision.operation,
        outcome: RevisionActivationOutcome::Activate,
        catalog_rebased,
        candidate: Some(candidate),
        result_delta: delta,
        selected_property_consequence,
        attempted_breakdown,
        guidance: None,
    };
    reservation.complete(response.clone());
    Json(response).into_response()
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

fn inactive_response(
    operation: SearchRevisionOperation,
    outcome: RevisionActivationOutcome,
    catalog_rebased: bool,
    attempted_breakdown: Vec<BuyerIntentBranchProjection>,
    guidance: Option<RevisionGuidance>,
) -> SearchRevisionResponse {
    SearchRevisionResponse {
        operation,
        outcome,
        catalog_rebased,
        candidate: None,
        result_delta: ResultDelta::default(),
        selected_property_consequence: None,
        attempted_breakdown,
        guidance,
    }
}

fn revision_request_fingerprint(request: &SearchRevisionRequest) -> String {
    serde_json::to_string(&serde_json::json!({
        "parentResultIds": request.parent_result_ids,
        "utterance": request.utterance,
        "selectedPropertyId": request.selected_property_id,
    }))
    .expect("revision request fingerprint serializes")
}

fn result_delta(parent: &[String], candidate: &[String]) -> ResultDelta {
    let parent_positions = parent
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let candidate_positions = candidate
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    ResultDelta {
        added: candidate
            .iter()
            .filter(|id| !parent_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        removed: parent
            .iter()
            .filter(|id| !candidate_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        retained: candidate
            .iter()
            .filter(|id| parent_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        reordered: candidate
            .iter()
            .enumerate()
            .filter(|(index, id)| {
                parent_positions
                    .get(id.as_str())
                    .is_some_and(|old| old != index)
            })
            .map(|(_, id)| id.clone())
            .collect(),
    }
}

fn clarification_guidance() -> RevisionGuidance {
    RevisionGuidance {
        code: "clarificationRequired".to_string(),
        message: "Add a named place or an exact constraint.".to_string(),
        suggested_action: "clarify".to_string(),
    }
}

fn limit_guidance() -> RevisionGuidance {
    RevisionGuidance {
        code: "limitReached".to_string(),
        message: "This search has reached its revision limit.".to_string(),
        suggested_action: "startInitialSearch".to_string(),
    }
}

fn rebase_guidance() -> RevisionGuidance {
    RevisionGuidance {
        code: "catalogRebaseUnresolved".to_string(),
        message: "The current catalog cannot resolve every required part of this search."
            .to_string(),
        suggested_action: "preserveParent".to_string(),
    }
}

fn required_evidence_guidance() -> RevisionGuidance {
    RevisionGuidance {
        code: "requiredCapabilityUnavailable".to_string(),
        message: "That required constraint is unavailable in the current catalog.".to_string(),
        suggested_action: "preserveParent".to_string(),
    }
}

fn zero_results_guidance() -> RevisionGuidance {
    RevisionGuidance {
        code: "preserveParent".to_string(),
        message: "That change has no matching homes, so the current search is unchanged."
            .to_string(),
        suggested_action: "broadenRevision".to_string(),
    }
}

fn runtime_version(version: &RuntimeVersionKey) -> SearchRuntimeVersion {
    SearchRuntimeVersion {
        serving_bundle_version: version.serving_bundle_version.clone(),
        scoring_policy_version: version.scoring_policy_version,
        search_engine_version: version.search_engine_version.clone(),
        semantic_contract_digest: version.semantic_contract_digest.clone(),
    }
}

fn revision_error(
    status: StatusCode,
    code: &str,
    message: &str,
    runtime_version: SearchRuntimeVersion,
) -> Response {
    (
        status,
        Json(RevisionError {
            code: code.to_string(),
            message: message.to_string(),
            runtime_version,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_delta_is_stable_and_marks_order_changes() {
        let parent = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let candidate = vec!["b".to_string(), "a".to_string(), "d".to_string()];
        let delta = result_delta(&parent, &candidate);
        assert_eq!(delta.added, ["d"]);
        assert_eq!(delta.removed, ["c"]);
        assert_eq!(delta.retained, ["b", "a"]);
        assert_eq!(delta.reordered, ["b", "a"]);
    }
}
