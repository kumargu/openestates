use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::search::ast::{ConstraintExpr, ConstraintTerm};
use crate::search::{
    compile_search_revision_with_plan, revision_id_for_query, validated_revision_depth,
    CompiledSearchPlan, GeoScope, ResolvedEntityHandle, SearchResponse, SearchRevisionLimits,
    SearchRevisionOperation, SearchRevisionOutcome, SearchRuntimeVersion, SourceSpan,
};
use crate::state::{AppState, RuntimeVersionKey};

use super::search::compute_search;

const MAX_ACTIVE_BRANCHES: usize = 8;
const MAX_REVISION_DEPTH: usize = 12;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRevisionRequest {
    pub parent_revision_id: String,
    pub parent_query: String,
    pub parent_branch_count: usize,
    pub expected_runtime_version: SearchRuntimeVersion,
    pub utterance: String,
    pub client_idempotency_key: String,
}

#[derive(Debug, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchResponse>,
    pub result_delta: ResultDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<RevisionGuidance>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentProjection {
    pub branches: Vec<IntentBranchProjection>,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub retained: Vec<String>,
    pub reordered: Vec<String>,
}

#[derive(Debug, Serialize)]
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
    if request.expected_runtime_version != runtime_version {
        return revision_error(
            StatusCode::CONFLICT,
            "runtime_version_changed",
            "Search data or ranking changed. Refresh the parent search before revising it.",
            runtime_version,
        );
    }
    if request.parent_query.trim().is_empty()
        || request.parent_revision_id.trim().is_empty()
        || request.utterance.trim().is_empty()
        || request.client_idempotency_key.trim().is_empty()
    {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "invalid_revision_request",
            "Parent query, parent revision, utterance key, and utterance are required.",
            runtime_version,
        );
    }

    let Some(parent_depth) = validated_revision_depth(
        &request.parent_revision_id,
        &request.parent_query,
        &runtime_version,
    ) else {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "invalid_parent_revision",
            "The parent revision does not match this query and runtime.",
            runtime_version,
        );
    };

    let graph = state.knowledge.read().await.clone();
    let parent_query = request.parent_query.clone();
    let parent_snapshot = snapshot.clone();
    let parent_execution = state
        .execution
        .run_customer_compute(move || compute_search(parent_snapshot, graph, parent_query))
        .await;
    let Ok(parent_output) = parent_execution else {
        return revision_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "search_unavailable",
            "Search is temporarily unavailable. Retry this revision.",
            runtime_version,
        );
    };
    let derived_parent_count = parent_output.compiled_plan.branches.len().max(1);
    if request.parent_branch_count != derived_parent_count {
        return revision_error(
            StatusCode::BAD_REQUEST,
            "parent_branch_count_mismatch",
            "The parent branch count does not match the reparsed parent query.",
            runtime_version,
        );
    }

    let area_only_alternative = crate::search::revision::revision_expansion_fragment(
        &request.utterance,
    )
    .and_then(|fragment| {
        crate::search::revision::resolve_area_only_alternative(
            fragment,
            &snapshot.bundle.entities,
            &snapshot.bundle.entity_alias_index,
        )
    });
    let revision = compile_search_revision_with_plan(
        &request.parent_query,
        &request.utterance,
        derived_parent_count,
        SearchRevisionLimits {
            max_active_branches: MAX_ACTIVE_BRANCHES,
        },
        Some(&parent_output.compiled_plan),
        area_only_alternative.as_deref(),
    );
    let depth_checkpoint = parent_depth >= MAX_REVISION_DEPTH;
    let outcome = if depth_checkpoint {
        SearchRevisionOutcome::RequireCheckpoint
    } else {
        revision.outcome
    };
    let active_query = if outcome == SearchRevisionOutcome::Candidate {
        revision
            .candidate_query
            .clone()
            .unwrap_or_else(|| request.parent_query.clone())
    } else {
        request.parent_query.clone()
    };
    if outcome != SearchRevisionOutcome::Candidate {
        let revision_id = revision_id_for_query(
            &active_query,
            &runtime_version,
            parent_depth.saturating_add(1),
        );
        let intent_projection = project_intent(&parent_output.compiled_plan);
        return Json(SearchRevisionResponse {
            revision_id,
            operation: revision.operation,
            outcome,
            active_query,
            active_branch_count: derived_parent_count,
            runtime_version,
            ast_fingerprint: parent_output.response.ast_fingerprint.clone(),
            intent_projection,
            search: None,
            result_delta: ResultDelta::default(),
            guidance: Some(guidance_for(outcome)),
        })
        .into_response();
    }

    let graph = state.knowledge.read().await.clone();
    let candidate_query = active_query.clone();
    let execution_snapshot = snapshot.clone();
    let execution = state
        .execution
        .run_customer_compute(move || compute_search(execution_snapshot, graph, candidate_query))
        .await;
    let Ok(candidate_output) = execution else {
        return revision_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "search_unavailable",
            "Search is temporarily unavailable. Retry this revision.",
            runtime_version,
        );
    };
    let active_branch_count = candidate_output.compiled_plan.branches.len().max(1);
    let revision_id = revision_id_for_query(
        &active_query,
        &runtime_version,
        parent_depth.saturating_add(1),
    );
    let intent_projection = project_intent(&candidate_output.compiled_plan);
    let mut search = candidate_output.response.as_ref().clone();
    search.revision_id.clone_from(&revision_id);
    let result_delta = result_delta(
        &parent_output.response.ordered_result_ids,
        &search.ordered_result_ids,
    );

    Json(SearchRevisionResponse {
        revision_id,
        operation: revision.operation,
        outcome,
        active_query,
        active_branch_count,
        runtime_version,
        ast_fingerprint: search.ast_fingerprint.clone(),
        intent_projection,
        search: Some(search),
        result_delta,
        guidance: None,
    })
    .into_response()
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
                span: None,
            }),
        ]);

        let mut plan = CompiledSearchPlan::compile(
            crate::search::ast::CompiledQuery::with_constraints(
                "near Fixture Locality",
                ast,
                crate::search::SearchIntent::default(),
            ),
            "fixture-snapshot",
        );
        let source_span = SourceSpan {
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
