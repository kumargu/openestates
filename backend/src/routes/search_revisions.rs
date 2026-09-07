use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::dag_config::search_guardrail_config;
use crate::search::ast::{ConstraintExpr, ConstraintTerm};
use crate::search::{
    apply_typed_revision, compile_typed_revision, decode_signed_search_context,
    issue_signed_search_context, render_revision_active_query, BuyerIntentBranchProjection,
    CompiledSearchPlan, GeoCellSearchPolicy, GeoScope, ResolvedEntityHandle, SearchResponse,
    SearchRevisionDescriptor, SearchRevisionLimits, SearchRevisionOperation, SearchRevisionOutcome,
    SearchRuntimeVersion, SignedSearchContext, SourceSpan,
};
use crate::state::{AppState, CachedSearchOutput, RuntimeVersionKey};

use super::search::compute_search_plan;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRevisionRequest {
    pub parent_context: String,
    pub utterance: String,
    pub client_idempotency_key: String,
    #[serde(default)]
    pub undo_context: Option<String>,
    #[serde(default)]
    pub selected_property_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRevisionResponse {
    pub revision_id: String,
    pub operation: SearchRevisionOperation,
    pub outcome: SearchRevisionOutcome,
    pub active_query: String,
    pub active_branch_count: usize,
    pub runtime_version: SearchRuntimeVersion,
    pub ast_fingerprint: String,
    pub intent_projection: IntentProjection,
    pub active_revision: SearchRevisionDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempted_candidate: Option<SearchRevisionDescriptor>,
    pub candidate_count: usize,
    pub preserve_parent: bool,
    pub active_result_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_property_consequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchResponse>,
    pub result_delta: ResultDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<RevisionGuidance>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentProjection {
    pub branches: Vec<IntentBranchProjection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentBranchProjection {
    pub branch_id: String,
    pub geo_cluster_id: String,
    pub source_spans: Vec<SourceSpan>,
    pub geo_scope: GeoScope,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
    pub summary: String,
    pub constraints: Vec<IntentConstraintProjection>,
    pub spatial_entities: Vec<SpatialEntityHandle>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentConstraintProjection {
    pub dimension: String,
    pub operator: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpatialEntityHandle {
    pub entity_id: String,
    pub entity_type: String,
    pub label: String,
    pub relation: String,
    pub required: bool,
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

pub async fn revise_search(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRevisionRequest>,
) -> Response {
    let snapshot = state.search_runtime.load_full();
    let runtime_version = runtime_version(&snapshot.version_key);
    if request.parent_context.trim().is_empty()
        || request.utterance.trim().is_empty()
        || request.client_idempotency_key.trim().is_empty()
    {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "invalid_revision_request",
            "Parent context, utterance, and idempotency key are required.",
            runtime_version,
        );
    }
    let parent = match decode_signed_search_context(&request.parent_context) {
        Ok(context) => context,
        Err(_) => {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "invalid_parent_context",
                "The parent search context is invalid.",
                runtime_version,
            )
        }
    };
    if parent.runtime_version != runtime_version
        || parent.plan.snapshot_identity != runtime_version.serving_bundle_version
    {
        return revision_error(
            StatusCode::CONFLICT,
            "runtimeUnavailable",
            "The search snapshot for this revision is no longer available.",
            runtime_version,
        );
    }
    let request_fingerprint = revision_request_fingerprint(&request);
    match idempotency_lookup(
        &parent.revision_id,
        &request.client_idempotency_key,
        &request_fingerprint,
    ) {
        IdempotencyLookup::Hit(response) => return Json(response).into_response(),
        IdempotencyLookup::Conflict => {
            return revision_error(
                StatusCode::CONFLICT,
                "idempotency_key_conflict",
                "This idempotency key was already used for a different revision.",
                runtime_version,
            )
        }
        IdempotencyLookup::Miss => {}
    }
    if parent.depth >= search_guardrail_config().revisions.max_revision_depth {
        let response = inactive_revision_response(
            &parent,
            &request.parent_context,
            SearchRevisionOperation::Refine,
            SearchRevisionOutcome::RequireCheckpoint,
            runtime_version,
        );
        idempotency_insert(
            &parent.revision_id,
            &request.client_idempotency_key,
            request_fingerprint,
            response.clone(),
        );
        return Json(response).into_response();
    }

    let fragment_turn_id = format!("{}:turn", parent.revision_id);
    let mut fragment = crate::search::SearchEngine::new(&snapshot).compile_fragment(
        &request.utterance,
        &fragment_turn_id,
        &parent.plan,
    );
    let revision = compile_typed_revision(
        &parent.plan,
        &fragment,
        &request.utterance,
        &parent.revision_id,
        SearchRevisionLimits {
            max_active_branches: search_guardrail_config().revisions.max_active_branches,
        },
    );
    if revision.outcome != SearchRevisionOutcome::Candidate {
        let response = inactive_revision_response(
            &parent,
            &request.parent_context,
            revision.operation,
            revision.outcome,
            runtime_version,
        );
        idempotency_insert(
            &parent.revision_id,
            &request.client_idempotency_key,
            request_fingerprint,
            response.clone(),
        );
        return Json(response).into_response();
    }

    let mut undo_active_query = None;
    let mut undo_descriptor = None;
    if revision.operation == SearchRevisionOperation::Undo {
        let Some(undo_value) = request.undo_context.as_deref() else {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "undo_context_required",
                "Undo requires the signed direct-parent context.",
                runtime_version,
            );
        };
        let Ok(undo) = decode_signed_search_context(undo_value) else {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "invalid_undo_context",
                "The undo context is invalid.",
                runtime_version,
            );
        };
        if parent.parent_revision_id.as_deref() != Some(undo.revision_id.as_str())
            || undo.runtime_version != runtime_version
        {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "invalid_undo_parent",
                "Undo must target the signed direct parent.",
                runtime_version,
            );
        }
        undo_active_query = Some(undo.active_query.clone());
        undo_descriptor = Some(descriptor_from_context(&undo, undo_value.to_string()));
        fragment = undo.plan.clone();
    }

    let active_query = undo_active_query.unwrap_or_else(|| {
        render_revision_active_query(&parent.active_query, &request.utterance, revision.operation)
    });
    let candidate_plan = if revision.operation == SearchRevisionOperation::Undo {
        fragment.clone()
    } else {
        let Some(plan) = apply_typed_revision(
            &parent.plan,
            &fragment,
            &revision,
            &active_query,
            &snapshot.geo_topology,
            Some(&snapshot.bundle.spatial_index),
            GeoCellSearchPolicy {
                max_hops: snapshot.geo_cell_max_hops,
                max_distance_km: snapshot.geo_cell_max_distance_km,
            },
        ) else {
            return revision_error(
                StatusCode::BAD_REQUEST,
                "invalid_revision_patch",
                "The revision could not be applied to this search.",
                runtime_version,
            );
        };
        plan
    };
    let semantic_cache_key = format!(
        "{}:{}",
        candidate_plan.semantic_fingerprint,
        serde_json::to_string(&runtime_version).expect("runtime version serializes")
    );
    let candidate_output = if let Some(cached) = semantic_cache_get(&semantic_cache_key) {
        cached
    } else {
        let graph = state.knowledge.read().await.clone();
        let execution_snapshot = snapshot.clone();
        let execution_plan = candidate_plan.clone();
        let execution_query = active_query.clone();
        let execution = state
            .execution
            .run_customer_compute(move || {
                compute_search_plan(execution_snapshot, graph, execution_plan, execution_query)
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
        semantic_cache_insert(semantic_cache_key, output.clone());
        output
    };
    let candidate_ids = candidate_output.response.ordered_result_ids.clone();
    let candidate_descriptor = undo_descriptor.unwrap_or_else(|| {
        issue_signed_search_context(
            Some(parent.revision_id.clone()),
            revision.operation,
            parent.depth + 1,
            active_query.clone(),
            candidate_plan.clone(),
            runtime_version.clone(),
            candidate_ids.clone(),
        )
        .1
    });
    let candidate_count = candidate_ids.len();
    let preserve_parent = candidate_count == 0;
    let (active_descriptor, active_ids, intent_plan) = if preserve_parent {
        (
            descriptor_from_context(&parent, request.parent_context.clone()),
            parent.ordered_result_ids.clone(),
            parent.plan.clone(),
        )
    } else {
        (
            candidate_descriptor.clone(),
            candidate_ids.clone(),
            candidate_plan.clone(),
        )
    };
    let mut search = candidate_output.response.as_ref().clone();
    search.query.clone_from(&active_query);
    search.revision_id.clone_from(&active_descriptor.id);
    search.revision = Some(active_descriptor.clone());
    let selected_property_consequence = request.selected_property_id.as_ref().map(|selected| {
        if preserve_parent {
            "preserved".to_string()
        } else if candidate_ids.contains(selected) {
            "retained".to_string()
        } else {
            "removed".to_string()
        }
    });
    let response = SearchRevisionResponse {
        revision_id: active_descriptor.id.clone(),
        operation: revision.operation,
        outcome: revision.outcome,
        active_query: active_descriptor.active_query.clone(),
        active_branch_count: intent_plan.branches.len(),
        runtime_version: runtime_version.clone(),
        ast_fingerprint: intent_plan.semantic_fingerprint.clone(),
        intent_projection: project_intent(&intent_plan),
        active_revision: active_descriptor,
        attempted_candidate: preserve_parent.then_some(candidate_descriptor),
        candidate_count,
        preserve_parent,
        active_result_ids: active_ids.clone(),
        selected_property_consequence,
        search: (!preserve_parent).then_some(search),
        result_delta: result_delta(&parent.ordered_result_ids, &candidate_ids),
        guidance: preserve_parent.then(|| RevisionGuidance {
            code: "preserveParent".to_string(),
            message: "That change has no matching homes, so your current search is unchanged."
                .to_string(),
            suggested_action: "Remove one constraint or try a broader alternative.".to_string(),
        }),
    };
    idempotency_insert(
        &parent.revision_id,
        &request.client_idempotency_key,
        request_fingerprint,
        response.clone(),
    );
    Json(response).into_response()
}

const REVISION_CACHE_CAPACITY: usize = 128;

struct IdempotencyEntry {
    request_fingerprint: String,
    response: SearchRevisionResponse,
}

#[derive(Default)]
struct IdempotencyCache {
    entries: HashMap<String, IdempotencyEntry>,
    order: VecDeque<String>,
}

enum IdempotencyLookup {
    Hit(SearchRevisionResponse),
    Conflict,
    Miss,
}

#[derive(Default)]
struct SemanticRevisionCache {
    entries: HashMap<String, CachedSearchOutput>,
    order: VecDeque<String>,
}

fn idempotency_cache() -> &'static Mutex<IdempotencyCache> {
    static CACHE: OnceLock<Mutex<IdempotencyCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(IdempotencyCache::default()))
}

fn semantic_revision_cache() -> &'static Mutex<SemanticRevisionCache> {
    static CACHE: OnceLock<Mutex<SemanticRevisionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SemanticRevisionCache::default()))
}

fn revision_request_fingerprint(request: &SearchRevisionRequest) -> String {
    serde_json::to_string(&serde_json::json!({
        "utterance": request.utterance,
        "undoContext": request.undo_context,
        "selectedPropertyId": request.selected_property_id,
    }))
    .expect("revision request fingerprint serializes")
}

fn idempotency_lookup(
    parent_revision_id: &str,
    client_key: &str,
    request_fingerprint: &str,
) -> IdempotencyLookup {
    let key = format!("{parent_revision_id}\0{client_key}");
    let cache = idempotency_cache()
        .lock()
        .expect("revision idempotency cache lock poisoned");
    match cache.entries.get(&key) {
        Some(entry) if entry.request_fingerprint == request_fingerprint => {
            IdempotencyLookup::Hit(entry.response.clone())
        }
        Some(_) => IdempotencyLookup::Conflict,
        None => IdempotencyLookup::Miss,
    }
}

fn idempotency_insert(
    parent_revision_id: &str,
    client_key: &str,
    request_fingerprint: String,
    response: SearchRevisionResponse,
) {
    let key = format!("{parent_revision_id}\0{client_key}");
    let mut cache = idempotency_cache()
        .lock()
        .expect("revision idempotency cache lock poisoned");
    if !cache.entries.contains_key(&key) {
        cache.order.push_back(key.clone());
    }
    cache.entries.insert(
        key,
        IdempotencyEntry {
            request_fingerprint,
            response,
        },
    );
    while cache.entries.len() > REVISION_CACHE_CAPACITY {
        if let Some(oldest) = cache.order.pop_front() {
            cache.entries.remove(&oldest);
        }
    }
}

fn semantic_cache_get(key: &str) -> Option<CachedSearchOutput> {
    semantic_revision_cache()
        .lock()
        .expect("semantic revision cache lock poisoned")
        .entries
        .get(key)
        .cloned()
}

fn semantic_cache_insert(key: String, output: CachedSearchOutput) {
    let mut cache = semantic_revision_cache()
        .lock()
        .expect("semantic revision cache lock poisoned");
    if !cache.entries.contains_key(&key) {
        cache.order.push_back(key.clone());
    }
    cache.entries.insert(key, output);
    while cache.entries.len() > REVISION_CACHE_CAPACITY {
        if let Some(oldest) = cache.order.pop_front() {
            cache.entries.remove(&oldest);
        }
    }
}

fn descriptor_from_context(
    context: &SignedSearchContext,
    encoded: String,
) -> SearchRevisionDescriptor {
    SearchRevisionDescriptor {
        id: context.revision_id.clone(),
        parent_id: context.parent_revision_id.clone(),
        operation: context.operation,
        depth: context.depth,
        active_query: context.active_query.clone(),
        plan_fingerprint: context.plan_fingerprint.clone(),
        runtime_version: context.runtime_version.clone(),
        buyer_intent: context
            .plan
            .branches
            .iter()
            .map(|branch| BuyerIntentBranchProjection {
                branch_id: branch.branch_id.clone(),
                summary: branch.buyer_summary.clone(),
                source_spans: branch.source_spans.clone(),
            })
            .collect(),
        context: encoded,
    }
}

fn inactive_revision_response(
    parent: &SignedSearchContext,
    parent_context: &str,
    operation: SearchRevisionOperation,
    outcome: SearchRevisionOutcome,
    runtime_version: SearchRuntimeVersion,
) -> SearchRevisionResponse {
    SearchRevisionResponse {
        revision_id: parent.revision_id.clone(),
        operation,
        outcome,
        active_query: parent.active_query.clone(),
        active_branch_count: parent.plan.branches.len(),
        runtime_version,
        ast_fingerprint: parent.plan_fingerprint.clone(),
        intent_projection: project_intent(&parent.plan),
        active_revision: descriptor_from_context(parent, parent_context.to_string()),
        attempted_candidate: None,
        candidate_count: 0,
        preserve_parent: true,
        active_result_ids: parent.ordered_result_ids.clone(),
        selected_property_consequence: None,
        search: None,
        result_delta: ResultDelta::default(),
        guidance: Some(guidance_for(outcome)),
    }
}

fn project_intent(compiled_plan: &CompiledSearchPlan) -> IntentProjection {
    let branches = compiled_plan
        .branches
        .iter()
        .map(|branch| {
            let mut constraints = Vec::new();
            let mut spatial_entities = Vec::new();
            let mut explicit_spatial_ids = Vec::new();
            collect_explicit_spatial_ids(&branch.predicates, &mut explicit_spatial_ids);
            collect_projection_terms(
                &branch.predicates,
                false,
                &explicit_spatial_ids,
                &mut constraints,
                &mut spatial_entities,
            );
            for preference in &branch.constraints.positive_preferences {
                for key in &preference.expanded_keys {
                    push_constraint(&mut constraints, "preference", "prefer", key.clone());
                }
            }
            for preference in &branch.constraints.negative_preferences {
                for key in &preference.expanded_keys {
                    push_constraint(&mut constraints, "preference", "avoid", key.clone());
                }
            }
            for priority in &branch.constraints.ranking_priorities {
                push_constraint(
                    &mut constraints,
                    "rankingPriority",
                    "ordered",
                    priority.clone(),
                );
            }
            constraints.sort_by(|left, right| {
                left.dimension
                    .cmp(&right.dimension)
                    .then(left.operator.cmp(&right.operator))
            });
            spatial_entities.sort_by(|left, right| {
                left.entity_id
                    .cmp(&right.entity_id)
                    .then(left.relation.cmp(&right.relation))
            });
            let summary = branch_summary(&constraints, &spatial_entities);
            IntentBranchProjection {
                branch_id: branch.branch_id.clone(),
                geo_cluster_id: branch.geo_cluster_id.clone(),
                source_spans: branch.source_spans.clone(),
                geo_scope: branch.geo_scope.clone(),
                resolved_entities: branch.resolved_entities.clone(),
                summary,
                constraints,
                spatial_entities,
            }
        })
        .collect();
    IntentProjection { branches }
}

fn collect_explicit_spatial_ids(expression: &ConstraintExpr, ids: &mut Vec<String>) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_explicit_spatial_ids(clause, ids);
            }
        }
        ConstraintExpr::Not { clause } => collect_explicit_spatial_ids(clause, ids),
        ConstraintExpr::Term {
            term: ConstraintTerm::Spatial { entity_id, .. },
        } => push_unique(ids, entity_id),
        ConstraintExpr::Term { .. } => {}
    }
}

fn collect_projection_terms(
    expression: &ConstraintExpr,
    negated: bool,
    explicit_spatial_ids: &[String],
    constraints: &mut Vec<IntentConstraintProjection>,
    spatial: &mut Vec<SpatialEntityHandle>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_projection_terms(
                    clause,
                    negated,
                    explicit_spatial_ids,
                    constraints,
                    spatial,
                );
            }
        }
        ConstraintExpr::Not { clause } => {
            collect_projection_terms(clause, !negated, explicit_spatial_ids, constraints, spatial)
        }
        ConstraintExpr::Term { term } => match term {
            ConstraintTerm::Bhk { value, .. } => push_constraint(
                constraints,
                "bhk",
                if negated { "exclude" } else { "anyOf" },
                value.to_string(),
            ),
            ConstraintTerm::Budget { min, max, .. } => {
                if let Some(bound) = min {
                    push_constraint(
                        constraints,
                        "price",
                        if negated { "excludeAtLeast" } else { "atLeast" },
                        bound.value.to_string(),
                    );
                }
                if let Some(bound) = max {
                    push_constraint(
                        constraints,
                        "price",
                        if negated { "excludeAtMost" } else { "atMost" },
                        bound.value.to_string(),
                    );
                }
            }
            ConstraintTerm::Evidence { constraint, .. } => push_constraint(
                constraints,
                &constraint.field,
                if negated {
                    "exclude"
                } else {
                    match constraint.operator {
                        crate::search::intent::ConstraintOperator::Min => "atLeast",
                        crate::search::intent::ConstraintOperator::Max => "atMost",
                    }
                },
                format!("{} {}", constraint.value, constraint.unit),
            ),
            ConstraintTerm::Area {
                entity_id, value, ..
            } => {
                let is_relation_target = entity_id
                    .as_ref()
                    .is_some_and(|id| explicit_spatial_ids.contains(id));
                if negated || !is_relation_target {
                    push_constraint(
                        constraints,
                        "area",
                        if negated { "exclude" } else { "inside" },
                        value.clone(),
                    );
                    if let Some(entity_id) = entity_id {
                        push_spatial_handle(
                            spatial,
                            SpatialEntityHandle {
                                entity_id: entity_id.clone(),
                                entity_type: "area".to_string(),
                                label: value.clone(),
                                relation: if negated { "exclude" } else { "inside" }.to_string(),
                                required: true,
                            },
                        );
                    }
                }
            }
            ConstraintTerm::Society {
                entity_id,
                display_name,
                ..
            } => {
                if negated || !explicit_spatial_ids.contains(entity_id) {
                    push_constraint(
                        constraints,
                        "society",
                        if negated { "exclude" } else { "inside" },
                        display_name.clone(),
                    );
                    push_spatial_handle(
                        spatial,
                        SpatialEntityHandle {
                            entity_id: entity_id.clone(),
                            entity_type: "society".to_string(),
                            label: display_name.clone(),
                            relation: if negated { "exclude" } else { "inside" }.to_string(),
                            required: true,
                        },
                    );
                }
            }
            ConstraintTerm::Builder {
                entity_id,
                display_name,
                ..
            } => push_constraint(
                constraints,
                "builder",
                if negated { "exclude" } else { "is" },
                format!("{display_name} ({entity_id})"),
            ),
            ConstraintTerm::Spatial {
                relation,
                entity_id,
                display_name,
                required,
                ..
            } => push_spatial_handle(
                spatial,
                SpatialEntityHandle {
                    entity_id: entity_id.clone(),
                    entity_type: if entity_id.starts_with("area:") {
                        "area"
                    } else if entity_id.starts_with("society:") {
                        "society"
                    } else {
                        "place"
                    }
                    .to_string(),
                    label: display_name.clone(),
                    relation: if negated { "exclude" } else { relation }.to_ascii_lowercase(),
                    required: *required,
                },
            ),
        },
    }
}

fn push_constraint(
    constraints: &mut Vec<IntentConstraintProjection>,
    dimension: &str,
    operator: &str,
    value: String,
) {
    if let Some(existing) = constraints
        .iter_mut()
        .find(|constraint| constraint.dimension == dimension && constraint.operator == operator)
    {
        push_unique(&mut existing.values, &value);
        existing.values.sort();
    } else {
        constraints.push(IntentConstraintProjection {
            dimension: dimension.to_string(),
            operator: operator.to_string(),
            values: vec![value],
        });
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn push_spatial_handle(handles: &mut Vec<SpatialEntityHandle>, handle: SpatialEntityHandle) {
    if !handles.iter().any(|existing| {
        existing.entity_id == handle.entity_id && existing.relation == handle.relation
    }) {
        handles.push(handle);
    }
}

fn branch_summary(
    constraints: &[IntentConstraintProjection],
    spatial: &[SpatialEntityHandle],
) -> String {
    let mut parts = Vec::new();
    if let Some(bhk) = constraints
        .iter()
        .find(|constraint| constraint.dimension == "bhk" && constraint.operator != "exclude")
        .and_then(|constraint| constraint.values.first())
    {
        parts.push(format!("{bhk} BHK"));
    }
    parts.extend(spatial.iter().take(2).map(|entity| entity.label.clone()));
    if let Some(max) = constraints
        .iter()
        .find(|constraint| constraint.dimension == "price" && constraint.operator == "atMost")
        .and_then(|constraint| constraint.values.first())
        .and_then(|value| value.parse::<f64>().ok())
    {
        parts.push(format!("under ₹{:.2}Cr", max / 10_000_000.0));
    }
    if parts.is_empty() {
        "Search option".to_string()
    } else {
        parts.join(" · ")
    }
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

fn guidance_for(outcome: SearchRevisionOutcome) -> RevisionGuidance {
    match outcome {
        SearchRevisionOutcome::RequireClarification => RevisionGuidance {
            code: "clarify_spatial_constraint".to_string(),
            message: "Add a named place or a distance so the change stays deterministic."
                .to_string(),
            suggested_action: "clarify".to_string(),
        },
        SearchRevisionOutcome::RequireCheckpoint => RevisionGuidance {
            code: "checkpoint_required".to_string(),
            message: "Confirm or remove an existing option before expanding this search."
                .to_string(),
            suggested_action: "checkpoint".to_string(),
        },
        SearchRevisionOutcome::Candidate => unreachable!("candidate outcomes have no guidance"),
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
    use crate::search::{revision_id_for_query, validated_revision_depth};

    #[test]
    fn result_delta_is_stable_and_marks_order_changes() {
        let parent = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let candidate = vec!["b".to_string(), "c".to_string(), "d".to_string()];
        let delta = result_delta(&parent, &candidate);
        assert_eq!(delta.added, ["d"]);
        assert_eq!(delta.removed, ["a"]);
        assert_eq!(delta.retained, ["b", "c"]);
        assert_eq!(delta.reordered, ["b", "c"]);
    }

    #[test]
    fn revision_ids_are_bound_to_the_parent_query_and_runtime() {
        let runtime = SearchRuntimeVersion {
            serving_bundle_version: "bundle-v1".to_string(),
            scoring_policy_version: 1,
            search_engine_version: "engine-v1".to_string(),
            semantic_contract_digest: "sha256:test".to_string(),
        };
        let revision_id = revision_id_for_query("3BHK in Whitefield", &runtime, 12);
        assert_eq!(
            validated_revision_depth(&revision_id, "3BHK in Whitefield", &runtime),
            Some(12)
        );
        assert_eq!(
            validated_revision_depth(&revision_id, "3BHK in Hoodi", &runtime),
            None
        );
        assert_eq!(
            validated_revision_depth("rev-000-deadbeef", "3BHK in Whitefield", &runtime),
            None
        );

        let mut public_hash = sha2::Sha256::new();
        use sha2::Digest;
        public_hash.update(b"3BHK in Whitefield");
        public_hash.update([0]);
        public_hash.update(serde_json::to_vec(&runtime).unwrap());
        let forged = format!(
            "rev-012-{}",
            public_hash
                .finalize()
                .iter()
                .take(8)
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert_eq!(
            validated_revision_depth(&forged, "3BHK in Whitefield", &runtime),
            None,
            "a caller-computable hash must not authenticate revision depth"
        );
    }

    #[test]
    fn projection_uses_the_executed_spatial_relation_without_inventing_inside() {
        let area_id = "area:osm:relation-123";
        let ast = ConstraintExpr::and(vec![
            ConstraintExpr::term(ConstraintTerm::Area {
                entity_id: Some(area_id.to_string()),
                value: "Fixture Locality".to_string(),
                span: None,
            }),
            ConstraintExpr::term(ConstraintTerm::Spatial {
                relation: "near".to_string(),
                entity_id: area_id.to_string(),
                display_name: "Fixture Locality".to_string(),
                required: true,
                category_fact_keys: Vec::new(),
                distance_limit_km: None,
                span: None,
            }),
        ]);

        let mut plan = CompiledSearchPlan::compile_for_snapshot(
            crate::search::ast::CompiledQuery::with_constraints(
                "near Fixture Locality",
                ast,
                crate::search::SearchIntent::default(),
            ),
            "fixture-snapshot",
            &[],
            &crate::search::GeoTopologyIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        );
        let source_span = SourceSpan {
            source_turn_id: String::new(),
            start: 5,
            end: 21,
            raw_text: "Fixture Locality".to_string(),
        };
        plan.branches[0].source_spans = vec![source_span.clone()];
        plan.branches[0].geo_scope = GeoScope::Scoped {
            anchors: vec![crate::search::GeoAnchor {
                entity_id: "place:fixture".to_string(),
                entity_type: "place".to_string(),
            }],
            market_locality_ids: vec![area_id.to_string()],
            seed_cells: Vec::new(),
            expanded_cell_paths: Vec::new(),
            supporting_evidence: Vec::new(),
            max_distance_km: 4.0,
        };
        plan.branches[0].resolved_entities = vec![ResolvedEntityHandle {
            entity_id: "place:fixture".to_string(),
            entity_type: "place".to_string(),
            display_name: "Fixture Locality".to_string(),
            source_span: Some(source_span.clone()),
        }];
        let projection = project_intent(&plan);
        assert_eq!(projection.branches.len(), 1);
        assert_eq!(projection.branches[0].branch_id, "branch-1");
        assert_eq!(projection.branches[0].source_spans, [source_span]);
        assert_eq!(projection.branches[0].geo_scope, plan.branches[0].geo_scope);
        assert_eq!(
            projection.branches[0].resolved_entities,
            plan.branches[0].resolved_entities
        );
        assert_eq!(projection.branches[0].spatial_entities.len(), 1);
        assert_eq!(projection.branches[0].spatial_entities[0].relation, "near");
        assert!(
            projection.branches[0]
                .constraints
                .iter()
                .all(|constraint| !(constraint.dimension == "area"
                    && constraint.operator == "inside"))
        );
    }
}
