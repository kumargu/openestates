use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::dag_config::search_resolution_config;
use crate::models::Property;
use crate::serving::{
    LoadedServingBundle, ServingEntityAliasIndex, ServingEntityRecord, TantivyRecallHit,
};
use crate::state::{SearchRuntimeSnapshot, SEARCH_ENGINE_VERSION};

use super::ast::{ConstraintExpr, ConstraintTerm, IntentAst, ResolvedEntityConstraint};
use super::compiled_plan::{
    CompiledSearchPlan, GeoCellSearchPolicy, GeoScope, ResolvedEntityHandle,
};
use super::geo;
use super::index::SearchIndex;
use super::intent::{SearchIntent, SourceSpan};
use super::query_plan::{self, QueryPlan};
use super::resolver::{is_resolvable_entity_name, slug};
use super::revision::PortableIntentAst;
use super::schema;
use super::text::SearchEvaluationContext;
use super::{
    CandidateEvaluationRequest, CandidateEvaluator, GeographyMatch, GeographyMatchKind,
    SearchResultCard, SearchResultSet,
};

const TANTIVY_RECALL_LIMIT: usize = 128;
const DIAGNOSTIC_ID_LIMIT: usize = 20;
const DIAGNOSTIC_SCORE_LIMIT: usize = 8;

pub struct SearchEngine<'a> {
    snapshot: &'a SearchRuntimeSnapshot,
}

#[derive(Debug, Clone)]
pub struct SearchEngineOutput {
    pub compiled_plan: CompiledSearchPlan,
    pub intent: SearchIntent,
    pub results: Vec<SearchResultCard>,
    pub result_sets: Vec<SearchResultSet>,
    pub eligible_result_count: usize,
    pub diagnostics: SearchDiagnostics,
    pub evidence_gaps: Vec<SearchEvidenceGap>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchDiagnostics {
    pub layer_timings: Vec<SearchLayerTiming>,
    pub runtime: SearchRuntimeDiagnostics,
    pub resolved: SearchResolutionDiagnostics,
    pub recall: SearchRecallDiagnostics,
    pub top_candidate_scores: Vec<CandidateScore>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence_gaps: Vec<SearchEvidenceGap>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRuntimeDiagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_bundle_version: Option<String>,
    pub search_engine_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResolutionDiagnostics {
    pub entities: Vec<ResolvedSearchEntity>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSearchEntity {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub match_kind: String,
    pub match_source: String,
    pub matched_text: String,
    pub polarity: String,
    #[serde(skip)]
    pub source_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLayerTiming {
    pub layer: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRecallDiagnostics {
    pub structured_total_count: usize,
    pub structured_count: usize,
    pub tantivy_count: usize,
    pub tantivy_branch_additions: usize,
    pub merged_extra_count: usize,
    pub structured_sample: Vec<String>,
    pub tantivy_sample: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tantivy_entity_sample: Vec<TantivyHitDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<BranchRecallDiagnostics>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRecallDiagnostics {
    pub branch_id: String,
    pub structured_count: usize,
    pub tantivy_count: usize,
    pub spatial_count: usize,
    pub merged_count: usize,
    pub tantivy_additions: usize,
    pub spatial_additions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TantivyHitDiagnostic {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub score: f32,
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScore {
    pub property_id: String,
    pub rank: usize,
    pub final_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEvidenceGap {
    pub entity_id: String,
    pub missing_fact: String,
    pub reason: String,
}

#[derive(Debug)]
struct SearchTimer {
    started_at: Instant,
    timings: Vec<SearchLayerTiming>,
}

struct TimedValue<T> {
    value: T,
}

struct TantivyRecallResult {
    property_ids: Vec<String>,
    entity_hits: Vec<TantivyRecallHit>,
    warning: Option<String>,
}

struct PreparedSearchBranch {
    plan_compiled_query: IntentAst,
    serving_resolved_entities: Vec<ResolvedSearchEntity>,
    unresolved_entity_clause: Option<String>,
}

impl<'a> SearchEngine<'a> {
    pub fn new(snapshot: &'a SearchRuntimeSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn search(&self, query: &str) -> SearchEngineOutput {
        let plan = self.compile_initial(query, "root");
        self.execute_plan(plan)
            .expect("a freshly compiled plan must target the active snapshot")
    }

    pub fn compile_initial(&self, query: &str, source_turn_id: &str) -> CompiledSearchPlan {
        let top_level_plan = query_plan::compile_query_plan(query);
        let aggregate_intent = query_plan::project_search_intent(query, &top_level_plan);
        let prepared = self.prepare_branch_from_plan(query, top_level_plan, aggregate_intent, &[]);
        self.compile_prepared_plan(&prepared, source_turn_id)
    }

    /// Bind a portable intent tree to the active serving snapshot. This path
    /// deliberately never reparses the buyer-facing presentation string.
    pub fn compile_intent_ast(
        &self,
        ast: &PortableIntentAst,
    ) -> Result<CompiledSearchPlan, String> {
        if ast.version != 1 || ast.branches.is_empty() {
            return Err("unsupported or empty intent AST".to_string());
        }
        let branch_predicates = ast
            .branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let compiled_query = IntentAst {
            raw: String::new(),
            constraints: ConstraintExpr::any_of(branch_predicates.clone()),
            branches: branch_predicates,
            intent: ast.aggregate_intent.clone(),
        };
        let resolved_entities = ast
            .branches
            .iter()
            .flat_map(|branch| branch.resolved_entities.iter().cloned())
            .fold(
                Vec::<ResolvedEntityHandle>::new(),
                |mut entities, entity| {
                    if !entities.iter().any(|existing| {
                        existing.entity_id == entity.entity_id
                            && existing.source_span == entity.source_span
                    }) {
                        entities.push(entity);
                    }
                    entities
                },
            );
        let mut plan = CompiledSearchPlan::compile_for_snapshot(
            compiled_query,
            self.snapshot.version_key.serving_bundle_version.as_str(),
            &resolved_entities,
            &self.snapshot.bundle.graph_index,
            Some(&self.snapshot.bundle.spatial_index),
            GeoCellSearchPolicy {
                max_hops: self.snapshot.geo_cell_max_hops,
                max_distance_km: self.snapshot.geo_cell_max_distance_km,
            },
        );
        if plan.branches.len() != ast.branches.len() {
            return Err("intent AST branch count changed while binding".to_string());
        }
        for (branch, portable) in plan.branches.iter_mut().zip(&ast.branches) {
            branch.branch_id.clone_from(&portable.branch_id);
            branch
                .resolved_entities
                .clone_from(&portable.resolved_entities);
            for binding in &mut branch.predicate_bindings {
                if let Some(stable) = portable.predicate_bindings.iter().find(|stable| {
                    stable.family == binding.family
                        && stable.polarity == binding.polarity
                        && (stable.semantic_key == binding.semantic_key
                            || stable.path == binding.path)
                }) {
                    binding.predicate_id.clone_from(&stable.predicate_id);
                }
            }
        }
        plan.root = super::compiled_plan::BoolExpr::Any(
            plan.branches
                .iter()
                .map(|branch| super::compiled_plan::BoolExpr::Leaf(branch.branch_id.clone()))
                .collect(),
        );
        plan.refresh_semantic_fingerprint();
        Ok(plan)
    }

    pub fn compile_fragment(
        &self,
        fragment: &str,
        source_turn_id: &str,
        parent: &CompiledSearchPlan,
    ) -> CompiledSearchPlan {
        let trimmed = fragment.trim();
        let compiled_fragment = crate::dag_config::search_parser_config()
            .discourse
            .revision_continuity_prefixes
            .iter()
            .filter_map(|prefix| {
                trimmed
                    .get(..prefix.len())
                    .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
                    .and_then(|_| trimmed.get(prefix.len()..))
            })
            .map(|remainder| remainder.trim_start_matches([',', ':', '-', ' ']))
            .filter(|remainder| !remainder.is_empty())
            .min_by_key(|remainder| remainder.len())
            .unwrap_or(trimmed);
        let offset = compiled_fragment.as_ptr() as usize - fragment.as_ptr() as usize;
        let top_level_plan = query_plan::compile_query_plan(compiled_fragment);
        let aggregate_intent =
            query_plan::project_search_intent(compiled_fragment, &top_level_plan);
        let inherited_area_context_ids = parent
            .branches
            .iter()
            .flat_map(|branch| match &branch.geo_scope {
                GeoScope::Scoped {
                    market_locality_ids,
                    ..
                } => market_locality_ids.clone(),
                GeoScope::Unresolved { .. } => Vec::new(),
                GeoScope::BundleWide => Vec::new(),
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let prepared = self.prepare_branch_from_plan(
            compiled_fragment,
            top_level_plan.clone(),
            aggregate_intent,
            &inherited_area_context_ids,
        );
        let mut plan = self.compile_prepared_plan(&prepared, source_turn_id);
        if plan
            .branches
            .iter()
            .all(|branch| !branch.predicates.has_terms() && branch.resolved_entities.is_empty())
        {
            if let Some(relation_start) = first_relation_start(&top_level_plan) {
                let relation_fragment = &compiled_fragment[relation_start..];
                let relation_query_plan = query_plan::compile_query_plan(relation_fragment);
                let relation_intent =
                    query_plan::project_search_intent(relation_fragment, &relation_query_plan);
                let mut relation_prepared = self.prepare_branch_from_plan(
                    relation_fragment,
                    relation_query_plan,
                    relation_intent,
                    &inherited_area_context_ids,
                );
                let merged_intent = merge_fragment_intent(
                    &plan.aggregate_intent,
                    &relation_prepared.plan_compiled_query.intent,
                );
                relation_prepared.plan_compiled_query.intent = merged_intent;
                let mut relation_plan =
                    self.compile_prepared_plan(&relation_prepared, source_turn_id);
                if relation_plan.branches.iter().any(|branch| {
                    branch.predicates.has_terms() || !branch.resolved_entities.is_empty()
                }) {
                    relation_plan.shift_source_spans(offset + relation_start);
                    return relation_plan;
                }
            }
        }
        plan.shift_source_spans(offset);
        plan
    }

    /// Execute an already compiled, snapshot-pinned plan. No source query is
    /// parsed or resolved on this path.
    pub fn execute_plan(&self, plan: CompiledSearchPlan) -> Option<SearchEngineOutput> {
        if plan.snapshot_identity != self.snapshot.version_key.serving_bundle_version {
            return None;
        }
        Some(self.execute_compiled_plan(plan))
    }

    fn compile_prepared_plan(
        &self,
        prepared: &PreparedSearchBranch,
        source_turn_id: &str,
    ) -> CompiledSearchPlan {
        let resolved_entities = prepared
            .serving_resolved_entities
            .iter()
            .map(|entity| ResolvedEntityHandle {
                entity_id: entity.entity_id.clone(),
                entity_type: entity.entity_type.clone(),
                display_name: entity.name.clone(),
                source_span: entity.source_span.clone(),
            })
            .collect::<Vec<_>>();
        let mut plan = CompiledSearchPlan::compile_for_snapshot(
            prepared.plan_compiled_query.clone(),
            self.snapshot.version_key.serving_bundle_version.as_str(),
            &resolved_entities,
            &self.snapshot.bundle.graph_index,
            Some(&self.snapshot.bundle.spatial_index),
            GeoCellSearchPolicy {
                max_hops: self.snapshot.geo_cell_max_hops,
                max_distance_km: self.snapshot.geo_cell_max_distance_km,
            },
        );
        plan.assign_source_turn(source_turn_id);
        if prepared.unresolved_entity_clause.is_some() {
            for branch in &mut plan.branches {
                if branch.geo_scope.is_bundle_wide() {
                    branch.geo_scope = GeoScope::Unresolved {
                        anchors: Vec::new(),
                        requested: prepared.unresolved_entity_clause.iter().cloned().collect(),
                        reason:
                            super::compiled_plan::GeoScopeResolution::UnresolvedExplicitGeography,
                    };
                }
            }
        }
        plan.refresh_semantic_fingerprint();
        plan
    }

    fn prepare_branch_from_plan(
        &self,
        query: &str,
        query_plan: QueryPlan,
        parsed_intent: SearchIntent,
        inherited_area_context_ids: &[String],
    ) -> PreparedSearchBranch {
        let mut serving_resolved_entities = resolve_serving_query_entities(
            query,
            &query_plan,
            &parsed_intent,
            Some(self.snapshot.bundle.as_ref()),
            &self.snapshot.properties,
        );
        let mut area_context_ids =
            explicit_area_context_ids(query, &query_plan, &serving_resolved_entities);
        for area_id in inherited_area_context_ids {
            if !area_context_ids.contains(area_id) {
                area_context_ids.push(area_id.clone());
            }
        }
        let geo_query = self
            .snapshot
            .bundle
            .entity_index
            .query_with_plan_and_area_context(
                &query_plan,
                Some(&self.snapshot.bundle.spatial_index),
                &area_context_ids,
            );
        retain_relation_compatible_entities(
            query,
            &query_plan,
            geo_query.as_ref(),
            &mut serving_resolved_entities,
        );
        if let Some(spatial_query) = geo_query.as_ref() {
            for clause in spatial_query
                .resolved_clauses()
                .iter()
                .filter(|clause| clause.place_entity_ids.is_empty())
            {
                let normalized_target = normalized_place_family_target(&clause.target_text);
                let Some(family) =
                    search_resolution_config()
                        .place_families
                        .iter()
                        .find(|family| {
                            normalized_place_family_target(&family.label) == normalized_target
                                || family.aliases.iter().any(|alias| {
                                    normalized_place_family_target(alias) == normalized_target
                                })
                        })
                else {
                    continue;
                };
                let entity = ResolvedSearchEntity {
                    entity_id: format!("place_family:{}", family.id),
                    entity_type: "place_family".to_string(),
                    name: family.label.clone(),
                    match_kind: "configured_place_family".to_string(),
                    match_source: "compiled_plan".to_string(),
                    matched_text: clause.target_text.clone(),
                    polarity: "positive".to_string(),
                    source_span: Some(clause.target_span.clone()),
                };
                if !serving_resolved_entities.iter().any(|existing| {
                    existing.entity_id == entity.entity_id
                        && existing.source_span == entity.source_span
                }) {
                    serving_resolved_entities.push(entity);
                }
            }
        }
        let entity_constraints = resolved_entity_constraints(&serving_resolved_entities);
        let intent = apply_resolved_constraints(parsed_intent, &serving_resolved_entities);
        let mut compiled_query =
            IntentAst::compile(query, &query_plan, intent, &entity_constraints);
        if let Some(layout) = query_plan::paired_ordinal_branch_layout(&query_plan) {
            let segments = layout
                .segments
                .iter()
                .map(|span| (span.start, span.end))
                .collect::<Vec<_>>();
            let bhk_spans = layout
                .bhk_spans
                .iter()
                .map(|span| (span.start, span.end))
                .collect::<Vec<_>>();
            compiled_query.align_paired_ordinal_branches(&segments, &bhk_spans);
        }
        let discourse_layout = query_plan::discourse_branch_layout_with_plan(query, &query_plan);
        let discourse_segments = discourse_layout.as_ref().map(|layout| {
            layout
                .segments
                .iter()
                .map(|span| (span.start, span.end))
                .collect::<Vec<_>>()
        });
        if let Some(layout) = discourse_layout.as_ref() {
            compiled_query.align_discourse_branches(
                discourse_segments.as_deref().unwrap_or_default(),
                layout.shared_suffix.as_ref().map(|span| span.start),
            );
        }
        let mut plan_compiled_query = compiled_query;
        if let Some(spatial_query) = geo_query.as_ref() {
            let mut spatial_terms = spatial_query.ast_terms();
            for term in &mut spatial_terms {
                let ConstraintTerm::Spatial {
                    entity_id, span, ..
                } = term
                else {
                    continue;
                };
                if let Some(resolved_span) = serving_resolved_entities
                    .iter()
                    .find(|entity| entity.entity_id == *entity_id)
                    .and_then(|entity| entity.source_span.clone())
                {
                    *span = Some(resolved_span);
                }
            }
            // Keep optional named-place terms in the authoritative plan so
            // branch recall and proof projection retain the resolved target.
            // The execution query below drops optional spatial terms from
            // hard eligibility; lexical recall can never satisfy them.
            plan_compiled_query.add_spatial_plan_constraints(
                spatial_terms.clone(),
                discourse_segments.as_deref(),
                discourse_layout
                    .as_ref()
                    .and_then(|layout| layout.shared_suffix.as_ref().map(|span| span.start)),
            );
        }
        let unresolved_entity_clause = unsupported_qualifier_clause(query, &query_plan)
            .or_else(|| {
                unresolved_named_entity_clause(
                    query,
                    &query_plan,
                    &serving_resolved_entities,
                    geo_query.as_ref(),
                )
            })
            .filter(|clause| {
                !constraint_has_unbound_area(&plan_compiled_query.constraints, clause)
                    && !serving_resolved_entities.iter().any(|entity| {
                        entity.polarity != "exclusion"
                            && (entity.name.eq_ignore_ascii_case(clause)
                                || entity.matched_text.eq_ignore_ascii_case(clause))
                    })
            });
        PreparedSearchBranch {
            plan_compiled_query,
            serving_resolved_entities,
            unresolved_entity_clause,
        }
    }

    fn execute_compiled_plan(&self, compiled_plan: CompiledSearchPlan) -> SearchEngineOutput {
        let mut timer = SearchTimer::start();
        let snapshot_identity = self.snapshot.version_key.serving_bundle_version.as_str();
        let serving_facts = Some(&self.snapshot.bundle.fact_index);
        let mut structured_ids = Vec::new();
        let mut tantivy_ids = Vec::new();
        let mut merged_ids = Vec::new();
        let mut tantivy_hits = Vec::new();
        let mut geo_hit_samples = Vec::new();
        let mut warnings = Vec::new();
        let mut evidence_gaps = Vec::new();
        let mut branch_diagnostics = Vec::new();
        let mut branch_results = Vec::with_capacity(compiled_plan.branches.len());

        for branch in &compiled_plan.branches {
            let structured = timer.measure("structured_recall", || {
                self.snapshot
                    .search_index
                    .recall_plan_ids(&branch.scoring_query, &branch.eligibility_predicates)
            });
            let eligible_property_ids = branch.eligibility_predicates.has_terms().then(|| {
                self.snapshot
                    .search_index
                    .recall_constraint_expr_ids(&branch.eligibility_predicates)
                    .into_iter()
                    .collect::<HashSet<_>>()
            });
            let tantivy = timer.measure("tantivy_recall", || {
                tantivy_candidate_ids(
                    Some(self.snapshot.bundle.as_ref()),
                    &branch.recall_query,
                    &self.snapshot.search_index,
                )
            });
            let mut geo_query = self
                .snapshot
                .bundle
                .entity_index
                .bind_compiled_spatial_predicates(&branch.spatial_predicates);
            let spatial = timer.measure("geo_recall", || {
                let coordinate_candidates = geo_query
                    .as_ref()
                    .map(|query| {
                        query
                            .spatial_candidate_society_ids(
                                &self.snapshot.bundle.spatial_index,
                                &self.snapshot.search_index,
                                eligible_property_ids.as_ref(),
                            )
                            .into_iter()
                            .flat_map(|entity_id| {
                                self.snapshot
                                    .search_index
                                    .property_ids_for_entity_id(&entity_id)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let fact_candidates = geo_query
                    .as_ref()
                    .map(|query| {
                        query.serving_fact_candidate_property_ids(
                            &self.snapshot.search_index,
                            &self.snapshot.bundle.fact_index,
                            eligible_property_ids.as_ref(),
                        )
                    })
                    .unwrap_or_default();
                merge_candidate_ids(optional_non_empty(coordinate_candidates), fact_candidates)
                    .unwrap_or_default()
            });

            let mut verified_spatial_matches = HashMap::new();
            if let Some(query) = geo_query.as_ref() {
                for property_id in &spatial {
                    let Some(property) = self
                        .snapshot
                        .property_by_id
                        .get(property_id)
                        .and_then(|index| self.snapshot.properties.get(*index))
                    else {
                        continue;
                    };
                    let matches = query.verified_matches_for_property(
                        property,
                        &self.snapshot.search_index,
                        &self.snapshot.bundle.spatial_index,
                        &self.snapshot.bundle.fact_index,
                        snapshot_identity,
                    );
                    if !matches.is_empty() {
                        verified_spatial_matches.insert(property_id.clone(), matches);
                    }
                }
            }
            if let Some(query) = geo_query.as_mut() {
                query.restrict_evidence_to_properties(
                    &self.snapshot.properties,
                    &self.snapshot.search_index,
                    &spatial,
                );
                evidence_gaps.extend(unresolved_proximity_gaps(Some(query)));
                geo_hit_samples.extend(sample_geo_hits(Some(query)));
            }

            let mut candidates = structured.clone();
            let mut tantivy_additions = 0;
            let mut spatial_additions = 0;
            for property_id in &tantivy.property_ids {
                if !candidates.contains(property_id) {
                    candidates.push(property_id.clone());
                    tantivy_additions += 1;
                }
            }
            for property_id in &spatial {
                if !candidates.contains(property_id) {
                    candidates.push(property_id.clone());
                    spatial_additions += 1;
                }
            }
            if branch_has_required_spatial_predicate(&branch.predicates) {
                let spatial = spatial.iter().collect::<HashSet<_>>();
                candidates.retain(|property_id| spatial.contains(property_id));
            }

            let mut geography_matches = HashMap::new();
            if !branch.geo_scope.is_bundle_wide() {
                candidates.retain(|property_id| {
                    let Some(society_id) = self
                        .snapshot
                        .search_index
                        .society_entity_id_for_property(property_id)
                    else {
                        return false;
                    };
                    let Some(geography_match) = geography_match_for_property(
                        &branch.geo_scope,
                        society_id,
                        &self.snapshot.bundle.graph_index,
                        &self.snapshot.bundle.spatial_index,
                        snapshot_identity,
                    ) else {
                        return false;
                    };
                    geography_matches.insert(property_id.clone(), geography_match);
                    true
                });
            }

            let unavailable_capability = branch
                .ranking_intent
                .positive_preferences
                .iter()
                .chain(branch.ranking_intent.negative_preferences.iter())
                .filter(|preference| preference.required)
                .find(|preference| {
                    !self
                        .snapshot
                        .bundle
                        .search_capabilities
                        .supports_preference(preference)
                });
            let mut results = if let Some(capability) = unavailable_capability {
                warnings.push(format!(
                    "unavailable search capability: {}",
                    capability.raw_text
                ));
                Vec::new()
            } else {
                let candidate_indexes =
                    candidate_property_indexes(&candidates, Some(&self.snapshot.property_by_id));
                timer.measure("ranking", || {
                    CandidateEvaluator::search(CandidateEvaluationRequest {
                        properties: &self.snapshot.properties,
                        search_index: Some(&self.snapshot.search_index),
                        extra_candidate_ids: None,
                        candidate_property_indexes: candidate_indexes,
                        geo_query: geo_query.as_ref(),
                        serving_facts,
                        society_names: &self.snapshot.society_names,
                        query: &branch.scoring_query,
                        intent: &branch.ranking_intent,
                        constraints: &branch.eligibility_predicates,
                        evaluation: SearchEvaluationContext {
                            options: &self.snapshot.inventory_options,
                            spatial_matches: &verified_spatial_matches,
                            snapshot_identity,
                        },
                    })
                })
            };
            for result in &mut results {
                result.geography_match = geography_matches.get(&result.card.id).cloned();
            }
            sort_geography_cohorts(&mut results);

            extend_unique_strings(&mut structured_ids, &structured);
            extend_unique_strings(&mut tantivy_ids, &tantivy.property_ids);
            extend_unique_strings(&mut merged_ids, &candidates);
            tantivy_hits.extend(tantivy.entity_hits);
            if let Some(warning) = tantivy.warning {
                warnings.push(warning);
            }
            if let GeoScope::Unresolved {
                reason, requested, ..
            } = &branch.geo_scope
            {
                warnings.push(format!(
                    "unresolved geography scope: {reason:?}{}",
                    requested
                        .first()
                        .map(|value| format!(" ({value})"))
                        .unwrap_or_default()
                ));
            }
            branch_diagnostics.push(BranchRecallDiagnostics {
                branch_id: branch.branch_id.clone(),
                structured_count: structured.len(),
                tantivy_count: tantivy.property_ids.len(),
                spatial_count: spatial.len(),
                merged_count: candidates.len(),
                tantivy_additions,
                spatial_additions,
            });
            branch_results.push(results);
        }

        let eligible_result_count = branch_results
            .iter()
            .flatten()
            .map(|result| result.card.id.as_str())
            .collect::<HashSet<_>>()
            .len();
        let result_sets = build_result_sets(&compiled_plan, branch_results);
        let (result_sets, results) =
            limit_result_sets(result_sets, schema::ranking_policy().result_limit);

        let resolved_entities = compiled_plan
            .branches
            .iter()
            .flat_map(|branch| branch.resolved_entities.iter())
            .map(|entity| ResolvedSearchEntity {
                entity_id: entity.entity_id.clone(),
                entity_type: entity.entity_type.clone(),
                name: entity.display_name.clone(),
                match_kind: "compiled".to_string(),
                match_source: "compiled_plan".to_string(),
                matched_text: entity.display_name.clone(),
                polarity: "resolved".to_string(),
                source_span: entity.source_span.clone(),
            })
            .fold(Vec::new(), |mut values, entity| {
                if !values
                    .iter()
                    .any(|existing: &ResolvedSearchEntity| existing.entity_id == entity.entity_id)
                {
                    values.push(entity);
                }
                values
            });
        let total_duration_ms = timer.started_at.elapsed().as_secs_f64() * 1000.0;
        let mut diagnostics = SearchDiagnostics {
            layer_timings: timer.finish(),
            runtime: SearchRuntimeDiagnostics {
                serving_bundle_version: Some(
                    self.snapshot.version_key.serving_bundle_version.clone(),
                ),
                search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
            },
            resolved: SearchResolutionDiagnostics {
                entities: resolved_entities,
            },
            recall: SearchRecallDiagnostics {
                structured_total_count: structured_ids.len(),
                structured_count: structured_ids.len(),
                tantivy_count: tantivy_ids.len(),
                tantivy_branch_additions: branch_diagnostics
                    .iter()
                    .map(|branch| branch.tantivy_additions)
                    .sum(),
                merged_extra_count: merged_ids.len(),
                structured_sample: sample_ids(&structured_ids),
                tantivy_sample: sample_ids(&tantivy_ids),
                tantivy_entity_sample: sample_tantivy_hits(&tantivy_hits)
                    .into_iter()
                    .chain(geo_hit_samples)
                    .collect(),
                branches: branch_diagnostics,
            },
            top_candidate_scores: candidate_scores(&results),
            evidence_gaps: evidence_gaps.clone(),
            warnings,
        };
        diagnostics.layer_timings.push(SearchLayerTiming {
            layer: "total".to_string(),
            duration_ms: total_duration_ms,
        });

        let intent = compiled_plan.aggregate_intent.clone();
        SearchEngineOutput {
            compiled_plan,
            intent,
            results,
            result_sets,
            eligible_result_count,
            diagnostics,
            evidence_gaps,
        }
    }
}

fn normalized_place_family_target(value: &str) -> String {
    let resolution = search_resolution_config();
    super::parser::query_tokens(value)
        .into_iter()
        .filter(|token| {
            !resolution
                .ignored_entity_names
                .iter()
                .chain(resolution.generic_scope_nouns.iter())
                .any(|ignored| ignored.eq_ignore_ascii_case(token))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn constraint_has_unbound_area(expression: &ConstraintExpr, label: &str) -> bool {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| constraint_has_unbound_area(clause, label)),
        ConstraintExpr::Not { .. } => false,
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Area {
                    entity_id: None,
                    value,
                    ..
                },
        } => value.eq_ignore_ascii_case(label),
        ConstraintExpr::Term { .. } => false,
    }
}

fn first_relation_start(plan: &QueryPlan) -> Option<usize> {
    let relations = &crate::dag_config::search_parser_config().relations.aliases;
    plan.tokens.iter().enumerate().find_map(|(index, token)| {
        relations.iter().find_map(|relation| {
            let alias_tokens = super::parser::query_tokens(&relation.alias);
            (!alias_tokens.is_empty()
                && index + alias_tokens.len() <= plan.tokens.len()
                && plan.tokens[index..index + alias_tokens.len()]
                    .iter()
                    .zip(alias_tokens)
                    .all(|(token, alias)| token.text.eq_ignore_ascii_case(&alias)))
            .then_some(token.start)
        })
    })
}

fn merge_fragment_intent(left: &SearchIntent, right: &SearchIntent) -> SearchIntent {
    let mut merged = right.clone();
    for preference in &left.positive_preferences {
        if !merged.positive_preferences.contains(preference) {
            merged.positive_preferences.push(preference.clone());
        }
    }
    for preference in &left.negative_preferences {
        if !merged.negative_preferences.contains(preference) {
            merged.negative_preferences.push(preference.clone());
        }
    }
    for preference in &left.preferences {
        if !merged.preferences.contains(preference) {
            merged.preferences.push(preference.clone());
        }
    }
    merged.positive_preferences.sort_by_key(|preference| {
        schema::positive_preference_patterns()
            .iter()
            .position(|pattern| pattern.label.eq_ignore_ascii_case(&preference.raw_text))
            .unwrap_or(usize::MAX)
    });
    merged.negative_preferences.sort_by_key(|preference| {
        schema::negative_preference_patterns()
            .iter()
            .position(|pattern| pattern.label.eq_ignore_ascii_case(&preference.raw_text))
            .unwrap_or(usize::MAX)
    });
    merged
}

fn limit_result_sets(
    result_sets: Vec<SearchResultSet>,
    result_limit: usize,
) -> (Vec<SearchResultSet>, Vec<SearchResultCard>) {
    if result_limit == 0 || result_sets.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut positions = vec![0usize; result_sets.len()];
    let mut selected = Vec::with_capacity(result_limit);
    let mut selected_ids = HashSet::new();

    while selected.len() < result_limit {
        let mut made_progress = false;
        for (set_index, result_set) in result_sets.iter().enumerate() {
            while let Some(result) = result_set.results.get(positions[set_index]) {
                positions[set_index] += 1;
                if !selected_ids.insert(result.card.id.clone()) {
                    continue;
                }
                selected.push(result.clone());
                made_progress = true;
                break;
            }
            if selected.len() == result_limit {
                break;
            }
        }
        if !made_progress {
            break;
        }
    }

    let limited_sets = result_sets
        .into_iter()
        .filter_map(|mut result_set| {
            result_set
                .results
                .retain(|result| selected_ids.contains(&result.card.id));
            if result_set.results.is_empty() {
                return None;
            }
            Some(result_set)
        })
        .collect();
    (limited_sets, selected)
}

fn sort_geography_cohorts(results: &mut [SearchResultCard]) {
    results.sort_by_key(
        |result| match result.geography_match.as_ref().map(|value| value.kind) {
            Some(GeographyMatchKind::ExactSociety) => 0,
            Some(GeographyMatchKind::SameMarketLocality) => 1,
            Some(GeographyMatchKind::CellNearby) => 2,
            None => 3,
        },
    );
}

fn geography_match_for_property(
    scope: &GeoScope,
    society_id: &str,
    topology: &crate::graph::GraphIndex,
    spatial_index: &crate::serving::SpatialServingIndex,
    snapshot_identity: &str,
) -> Option<GeographyMatch> {
    if let GeoScope::Unresolved { anchors, .. } = scope {
        return anchors
            .iter()
            .any(|anchor| {
                anchor.entity_type.eq_ignore_ascii_case("society") && anchor.entity_id == society_id
            })
            .then(|| GeographyMatch {
                kind: GeographyMatchKind::ExactSociety,
                cell_path: Vec::new(),
                hops: 0,
                distance_km: None,
                evidence_refs: Vec::new(),
            });
    }
    let GeoScope::Scoped {
        anchors,
        market_locality_ids,
        seed_cells,
        expanded_cell_paths,
        max_distance_km,
        ..
    } = scope
    else {
        return None;
    };
    let exact_society = anchors.iter().any(|anchor| {
        anchor.entity_type.eq_ignore_ascii_case("society") && anchor.entity_id == society_id
    });

    let market_membership = topology
        .market_memberships(society_id)
        .iter()
        .find(|membership| market_locality_ids.contains(&membership.target_entity_id))
        .map(|membership| membership.evidence_refs.clone());
    let occupied_paths = topology
        .occupied_cells(society_id)
        .iter()
        .filter_map(|occupancy| {
            let path = expanded_cell_paths.iter().find(|path| {
                path.cell_ids
                    .last()
                    .is_some_and(|cell| cell == &occupancy.target_entity_id)
            })?;
            Some((path, occupancy.evidence_refs.clone()))
        })
        .collect::<Vec<_>>();
    let best_path = occupied_paths.iter().min_by(|(left, _), (right, _)| {
        left.hops
            .cmp(&right.hops)
            .then_with(|| left.distance_km.total_cmp(&right.distance_km))
            .then_with(|| left.cell_ids.cmp(&right.cell_ids))
    });
    let distance = geography_candidate_distance(
        anchors,
        seed_cells,
        society_id,
        spatial_index,
        snapshot_identity,
    );

    let (kind, path, hops, mut evidence_refs) = if exact_society {
        let (path, occupancy) = best_path?;
        (
            GeographyMatchKind::ExactSociety,
            path.cell_ids.clone(),
            path.hops,
            occupancy.clone(),
        )
    } else if let Some(membership) = market_membership {
        let (path, hops) = best_path
            .map(|(path, _)| (path.cell_ids.clone(), path.hops))
            .unwrap_or_default();
        (
            GeographyMatchKind::SameMarketLocality,
            path,
            hops,
            membership,
        )
    } else {
        let (path, occupancy) = best_path?;
        let distance = distance.as_ref()?;
        if distance.distance_km > *max_distance_km {
            return None;
        }
        (
            GeographyMatchKind::CellNearby,
            path.cell_ids.clone(),
            path.hops,
            occupancy.clone(),
        )
    };
    if let Some((path, _)) = best_path {
        extend_unique_refs(&mut evidence_refs, path.supporting_evidence.clone());
    }
    if let Some(distance) = &distance {
        extend_unique_refs(&mut evidence_refs, distance.evidence_refs.clone());
    }
    Some(GeographyMatch {
        kind,
        cell_path: path,
        hops,
        distance_km: distance.map(|distance| distance.distance_km),
        evidence_refs,
    })
}

fn extend_unique_strings(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn geography_candidate_distance(
    anchors: &[super::compiled_plan::GeoAnchor],
    seed_cells: &[super::compiled_plan::GeoCellSeed],
    society_id: &str,
    spatial_index: &crate::serving::SpatialServingIndex,
    snapshot_identity: &str,
) -> Option<crate::serving::SpatialDistance> {
    let non_area_anchors = anchors
        .iter()
        .filter(|anchor| !anchor.entity_type.eq_ignore_ascii_case("area"))
        .map(|anchor| anchor.entity_id.as_str())
        .collect::<Vec<_>>();
    if !non_area_anchors.is_empty() {
        return non_area_anchors
            .into_iter()
            .filter_map(|anchor| {
                spatial_index.distance_between(anchor, society_id, snapshot_identity)
            })
            .min_by(|left, right| left.distance_km.total_cmp(&right.distance_km));
    }
    seed_cells
        .iter()
        .filter_map(|seed| {
            spatial_index.distance_from_entity_to_area(society_id, &seed.cell_id, snapshot_identity)
        })
        .min_by(|left, right| left.distance_km.total_cmp(&right.distance_km))
}

fn extend_unique_refs(
    target: &mut Vec<crate::serving::EvidenceRef>,
    values: Vec<crate::serving::EvidenceRef>,
) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn branch_has_required_spatial_predicate(expression: &ConstraintExpr) -> bool {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            clauses.iter().any(branch_has_required_spatial_predicate)
        }
        ConstraintExpr::Not { clause } => branch_has_required_spatial_predicate(clause),
        ConstraintExpr::Term {
            term: ConstraintTerm::Spatial { required, .. },
        } => *required,
        ConstraintExpr::Term { .. } => false,
    }
}

fn unsupported_qualifier_clause(query: &str, plan: &QueryPlan) -> Option<String> {
    for (index, token) in plan.tokens.iter().enumerate() {
        if !matches!(token.text.to_ascii_lowercase().as_str(), "with" | "prefer") {
            continue;
        }
        let Some(first) = plan.tokens.get(index + 1) else {
            continue;
        };
        let end = plan.tokens[index + 1..]
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.text.to_ascii_lowercase().as_str(),
                    "and" | "or" | "but" | "in" | "near" | "under" | "below" | "above"
                )
            })
            .map_or(query.len(), |candidate| candidate.start);
        if end <= first.start {
            continue;
        }
        let clause = query[first.start..end].trim_matches(|character: char| {
            character.is_ascii_whitespace() || ",;".contains(character)
        });
        if clause.is_empty() {
            continue;
        }
        let intent = crate::search::intent::parse_intent(clause);
        if intent.positive_preferences.is_empty()
            && intent.negative_preferences.is_empty()
            && intent.hard_constraints.is_empty()
        {
            return Some(clause.to_string());
        }
    }
    None
}

fn build_result_sets(
    compiled_plan: &CompiledSearchPlan,
    branch_results: Vec<Vec<SearchResultCard>>,
) -> Vec<SearchResultSet> {
    compiled_plan
        .branches
        .iter()
        .zip(branch_results)
        .filter_map(|(branch, mut results)| {
            for result in &mut results {
                result.tradeoff_label = None;
            }
            if results.is_empty() {
                return None;
            }
            let mut label = branch.buyer_summary.clone();
            if label.is_empty() {
                label = "Matches".to_string();
            }
            Some(SearchResultSet {
                branch_id: branch.branch_id.clone(),
                label,
                results,
            })
        })
        .collect()
}

fn resolved_entity_constraints(
    resolved_entities: &[ResolvedSearchEntity],
) -> Vec<ResolvedEntityConstraint> {
    let mut constraints = Vec::new();
    for entity in resolved_entities.iter().filter(|entity| {
        ["area", "society", "builder"]
            .iter()
            .any(|entity_type| entity.entity_type.eq_ignore_ascii_case(entity_type))
    }) {
        let exclusion = entity.polarity == "exclusion";
        let Some(span) = entity.source_span.clone() else {
            continue;
        };
        let constraint = ResolvedEntityConstraint {
            entity_id: entity.entity_id.clone(),
            entity_type: entity.entity_type.clone(),
            display_name: entity.name.clone(),
            span,
            exclusion,
        };
        if !constraints.contains(&constraint) {
            constraints.push(constraint);
        }
    }
    constraints
}

fn apply_resolved_constraints(
    mut intent: SearchIntent,
    resolved_entities: &[ResolvedSearchEntity],
) -> SearchIntent {
    if let Some(area) = intent.area.clone() {
        push_unique_string(&mut intent.areas, &area);
    }
    for entity in resolved_entities {
        let entity_type = entity.entity_type.to_ascii_lowercase();
        if entity.polarity == "exclusion" {
            match entity_type.as_str() {
                "area" => {
                    push_unique_string(&mut intent.excluded_areas, &entity.name);
                }
                "society" => {
                    push_unique_string(&mut intent.excluded_societies, &entity.name);
                }
                "builder" => {
                    push_unique_string(&mut intent.excluded_builders, &entity.name);
                }
                _ => {}
            }
            continue;
        }
        if entity_type == "area" {
            push_unique_string(&mut intent.areas, &entity.name);
        }
    }
    sync_positive_areas(&mut intent);
    intent
}

fn sync_positive_areas(intent: &mut SearchIntent) {
    intent.areas.retain(|area| {
        !intent
            .excluded_areas
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(area))
    });
    match intent.areas.as_slice() {
        [] => intent.area = None,
        [only] => intent.area = Some(only.clone()),
        _ => intent.area = None,
    }
}

fn resolve_serving_query_entities(
    query: &str,
    plan: &QueryPlan,
    intent: &SearchIntent,
    serving_bundle: Option<&LoadedServingBundle>,
    _properties: &[Property],
) -> Vec<ResolvedSearchEntity> {
    let mut entities = Vec::new();
    let Some(bundle) = serving_bundle else {
        return entities;
    };
    entities.extend(
        resolve_serving_query_entities_from_records_with_alias_index(
            query,
            plan,
            intent,
            &bundle.entities,
            &bundle.entity_alias_index,
        ),
    );
    prefer_market_locality_area_matches(&bundle.edges, &mut entities);
    let bound_provider_ids = crate::serving::bound_provider_entity_ids(&bundle.edges);
    entities.retain(|entity| {
        !entity.entity_type.eq_ignore_ascii_case("place")
            || !bound_provider_ids.contains(entity.entity_id.as_str())
    });
    remove_entities_only_mentioned_inside_longer_match(query, &mut entities);
    entities
}

fn prefer_market_locality_area_matches(
    edges: &[crate::serving::ServingEdgeRecord],
    entities: &mut Vec<ResolvedSearchEntity>,
) {
    let market_area_ids = edges
        .iter()
        .filter(|edge| edge.edge_type.eq_ignore_ascii_case("in_market_locality"))
        .map(|edge| edge.to_entity_id.as_str())
        .collect::<HashSet<_>>();
    let preferred = entities
        .iter()
        .filter(|entity| {
            entity.entity_type.eq_ignore_ascii_case("area")
                && market_area_ids.contains(entity.entity_id.as_str())
        })
        .filter_map(|entity| {
            Some((
                entity.name.to_ascii_lowercase(),
                entity.polarity.clone(),
                entity.source_span.as_ref()?.start,
                entity.source_span.as_ref()?.end,
            ))
        })
        .collect::<HashSet<_>>();
    entities.retain(|entity| {
        !entity.entity_type.eq_ignore_ascii_case("area")
            || market_area_ids.contains(entity.entity_id.as_str())
            || entity.source_span.as_ref().is_none_or(|span| {
                !preferred.contains(&(
                    entity.name.to_ascii_lowercase(),
                    entity.polarity.clone(),
                    span.start,
                    span.end,
                ))
            })
    });
}

fn remove_entities_only_mentioned_inside_longer_match(
    _query: &str,
    entities: &mut Vec<ResolvedSearchEntity>,
) {
    let entity_ranges = entities
        .iter()
        .map(|entity| {
            entity
                .source_span
                .as_ref()
                .map(|span| (span.start, span.end))
        })
        .collect::<Vec<_>>();
    let keep = entities
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            let Some(candidate_range) = entity_ranges[candidate_index] else {
                return true;
            };
            !entities.iter().enumerate().any(|(other_index, other)| {
                other_index != candidate_index
                    && other.matched_text.len() > candidate.matched_text.len()
                    && entity_ranges[other_index].is_some_and(|other_range| {
                        other_range.0 <= candidate_range.0 && other_range.1 >= candidate_range.1
                    })
            })
        })
        .collect::<Vec<_>>();
    let mut index = 0;
    entities.retain(|_| {
        let retain = keep[index];
        index += 1;
        retain
    });
}

fn explicit_area_context_ids(
    query: &str,
    plan: &QueryPlan,
    entities: &[ResolvedSearchEntity],
) -> Vec<String> {
    let query_lower = query.to_ascii_lowercase();
    let mut ids = entities
        .iter()
        .filter(|entity| {
            entity.polarity != "exclusion"
                && entity.entity_type.eq_ignore_ascii_case("area")
                && entity.source_span.as_ref().is_some_and(|span| {
                    area_range_is_explicit_context(&query_lower, plan, (span.start, span.end))
                })
        })
        .map(|entity| entity.entity_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn area_range_is_explicit_context(
    query_lower: &str,
    plan: &QueryPlan,
    range: (usize, usize),
) -> bool {
    let containing_relations = plan
        .clauses
        .iter()
        .filter(|clause| {
            clause.place_family_id.is_some()
                && clause.target_span.start <= range.0
                && clause.target_span.end >= range.1
        })
        .collect::<Vec<_>>();
    if containing_relations.is_empty() {
        return true;
    }
    let prefix_text = query_lower[..range.0].trim_end();
    search_resolution_config()
        .named_entity_scope_prefixes
        .iter()
        .any(|prefix| {
            prefix_text.strip_suffix(prefix).is_some_and(|before| {
                before
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !ch.is_alphanumeric())
            })
        })
}

fn retain_relation_compatible_entities(
    query: &str,
    plan: &QueryPlan,
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
    entities: &mut Vec<ResolvedSearchEntity>,
) {
    let query_lower = query.to_ascii_lowercase();
    let resolved_place_ids = geo_query
        .into_iter()
        .flat_map(|query| query.resolved_places())
        .map(|place| place.entity_id.as_str())
        .collect::<HashSet<_>>();
    entities.retain(|entity| {
        let ranges = entity
            .source_span
            .as_ref()
            .map(|span| (span.start, span.end))
            .into_iter()
            .collect::<Vec<_>>();
        if entity.entity_type.eq_ignore_ascii_case("place") {
            let belongs_to_relation = ranges.iter().any(|range| {
                plan.clauses.iter().any(|clause| {
                    clause.target_span.start <= range.0 && clause.target_span.end >= range.1
                })
            });
            return !belongs_to_relation || resolved_place_ids.contains(entity.entity_id.as_str());
        }
        if entity.entity_type.eq_ignore_ascii_case("area") {
            return ranges
                .iter()
                .any(|range| area_range_is_explicit_context(&query_lower, plan, *range));
        }
        true
    });
}

fn unresolved_named_entity_clause(
    query: &str,
    plan: &QueryPlan,
    resolved_entities: &[ResolvedSearchEntity],
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
) -> Option<String> {
    for clause in &plan.clauses {
        if clause.requirement != query_plan::RelationRequirement::Hard {
            continue;
        }
        let resolved = geo_query.is_some_and(|geo_query| {
            geo_query.resolved_clauses().iter().any(|resolved| {
                resolved
                    .target_text
                    .eq_ignore_ascii_case(&clause.target_text)
            })
        });
        if !resolved {
            return Some(clause.target_text.clone());
        }
    }

    if let Some(target) = geo_query
        .and_then(|geo_query| geo_query.unresolved_targets().first())
        .filter(|target| !target.trim().is_empty())
    {
        return Some(target.clone());
    }

    query_plan::unresolved_named_entity_clause(
        query,
        plan,
        |clause| {
            geo_query.is_some_and(|geo_query| {
                geo_query.resolved_clauses().iter().any(|resolved| {
                    resolved
                        .target_text
                        .eq_ignore_ascii_case(&clause.target_text)
                })
            })
        },
        |span| entity_scope_is_fully_resolved(plan, resolved_entities, span),
    )
    .or_else(|| {
        query_plan::unresolved_residual_clause(query, plan, |span| {
            entity_scope_is_fully_resolved(plan, resolved_entities, span)
        })
    })
}

fn entity_scope_is_fully_resolved(
    plan: &QueryPlan,
    resolved_entities: &[ResolvedSearchEntity],
    span: query_plan::ByteSpan,
) -> bool {
    let resolved_ranges = plan
        .areas
        .iter()
        .map(|area| (area.span.start, area.span.end))
        .chain(resolved_entities.iter().filter_map(|entity| {
            entity
                .source_span
                .as_ref()
                .map(|span| (span.start, span.end))
        }))
        .collect::<Vec<_>>();
    let config = search_resolution_config();

    plan.tokens
        .iter()
        .filter(|token| token.start >= span.start && token.end <= span.end)
        .filter(|token| {
            token
                .text
                .chars()
                .any(|character| character.is_ascii_alphabetic())
        })
        .filter(|token| {
            !config
                .ignored_entity_names
                .iter()
                .chain(config.generic_scope_nouns.iter())
                .any(|ignored| ignored.eq_ignore_ascii_case(&token.text))
        })
        .all(|token| {
            resolved_ranges
                .iter()
                .any(|(start, end)| *start <= token.start && *end >= token.end)
        })
}

fn unresolved_proximity_gaps(
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
) -> Vec<SearchEvidenceGap> {
    let Some(geo_query) = geo_query else {
        return Vec::new();
    };
    geo_query
        .unresolved_targets()
        .iter()
        .map(|target| SearchEvidenceGap {
            entity_id: format!("query:proximity:{}", slug(target)),
            missing_fact: "geo.proximity_anchor".to_string(),
            reason: format!(
                "Proximity target '{target}' did not resolve to a serving entity with usable location evidence"
            ),
        })
        .collect()
}

fn resolve_serving_query_entities_from_records_with_alias_index(
    query: &str,
    plan: &QueryPlan,
    intent: &SearchIntent,
    entities_source: &[ServingEntityRecord],
    alias_index: &ServingEntityAliasIndex,
) -> Vec<ResolvedSearchEntity> {
    let resolution_config = search_resolution_config();
    let query_lower = query.to_ascii_lowercase();
    let mut entities = Vec::new();

    for entity in entities_source {
        if !entity.visibility.is_searchable()
            || !is_serving_resolvable_entity_type(&entity.entity_type)
            || !is_resolvable_entity_name(&entity.name, resolution_config)
        {
            continue;
        }
        for (start, end) in exact_entity_match_ranges(&query_lower, &entity.name) {
            let polarity = if match_has_exclusion_prefix(&query_lower, start) {
                "exclusion"
            } else {
                "positive"
            };
            if polarity == "positive"
                && intent
                    .excluded_areas
                    .iter()
                    .any(|area| area.eq_ignore_ascii_case(&entity.name))
            {
                continue;
            }
            entities.push(ResolvedSearchEntity {
                entity_id: entity.entity_id.clone(),
                entity_type: entity.entity_type.clone(),
                name: entity.name.clone(),
                match_kind: "serving_entity_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: query[start..end].to_string(),
                polarity: polarity.to_string(),
                source_span: Some(SourceSpan {
                    source_turn_id: String::new(),
                    start,
                    end,
                    raw_text: query[start..end].to_string(),
                }),
            });
        }
    }

    let exact_spans = entities
        .iter()
        .flat_map(|entity| exact_entity_match_ranges(&query_lower, &entity.matched_text))
        .collect::<Vec<_>>();
    for token_start in 0..plan.tokens.len() {
        let mut alias = String::new();
        for token_end in token_start
            ..plan
                .tokens
                .len()
                .min(token_start + alias_index.max_token_count())
        {
            if !alias.is_empty() {
                alias.push(' ');
            }
            alias.push_str(&plan.tokens[token_end].text);
            let Some(group) = alias_index.get(&alias) else {
                continue;
            };
            let start = plan.tokens[token_start].start;
            let end = plan.tokens[token_end].end;
            if exact_spans
                .iter()
                .any(|(exact_start, exact_end)| *exact_start <= start && *exact_end >= end)
            {
                continue;
            }
            let polarity = if match_has_exclusion_prefix(&query_lower, start) {
                "exclusion"
            } else {
                "positive"
            };
            for record in &group.members {
                entities.push(ResolvedSearchEntity {
                    entity_id: record.entity_id.clone(),
                    entity_type: record.entity_type.clone(),
                    name: record.entity_name.clone(),
                    match_kind: "serving_entity_materialized_alias".to_string(),
                    match_source: "serving_alias_index".to_string(),
                    matched_text: query[start..end].to_string(),
                    polarity: polarity.to_string(),
                    source_span: Some(SourceSpan {
                        source_turn_id: String::new(),
                        start,
                        end,
                        raw_text: query[start..end].to_string(),
                    }),
                });
            }
        }
    }

    for fuzzy in fuzzy_society_name_matches(query, plan, entities_source, &entities) {
        entities.push(fuzzy);
    }

    entities
}

fn fuzzy_society_name_matches(
    query: &str,
    plan: &QueryPlan,
    entities_source: &[ServingEntityRecord],
    exact_matches: &[ResolvedSearchEntity],
) -> Vec<ResolvedSearchEntity> {
    let resolution_config = search_resolution_config();
    let mut candidates_by_span =
        std::collections::BTreeMap::<(usize, usize), Vec<(usize, &ServingEntityRecord)>>::new();

    for entity in entities_source.iter().filter(|entity| {
        entity.visibility.is_searchable()
            && entity.entity_type.eq_ignore_ascii_case("society")
            && is_resolvable_entity_name(&entity.name, resolution_config)
    }) {
        let name_tokens = entity_name_tokens(&entity.name);
        if name_tokens.len() < 2 || name_tokens.len() > plan.tokens.len() {
            continue;
        }
        let normalized_name = name_tokens.join(" ");
        let max_distance = if normalized_name.len() >= 8 { 2 } else { 1 };

        for window in plan.tokens.windows(name_tokens.len()) {
            let start = window[0].start;
            let end = window[window.len() - 1].end;
            if exact_matches.iter().any(|exact| {
                exact.entity_id == entity.entity_id
                    && exact_entity_match_ranges(&query.to_ascii_lowercase(), &exact.matched_text)
                        .iter()
                        .any(|range| range.0 <= start && range.1 >= end)
            }) {
                continue;
            }
            let query_tokens = window
                .iter()
                .map(|token| token.text.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if !query_tokens
                .iter()
                .zip(&name_tokens)
                .any(|(query_token, name_token)| query_token == name_token)
            {
                continue;
            }
            let normalized_query = query_tokens.join(" ");
            if normalized_query == normalized_name
                || normalized_query
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count()
                    < resolution_config.min_partial_entity_name_chars
            {
                continue;
            }
            let distance = super::index::levenshtein_distance(&normalized_query, &normalized_name);
            if distance == 0 || distance > max_distance {
                continue;
            }
            candidates_by_span
                .entry((start, end))
                .or_default()
                .push((distance, entity));
        }
    }

    let query_lower = query.to_ascii_lowercase();
    let mut resolved = Vec::new();
    for ((start, end), mut candidates) in candidates_by_span {
        candidates.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.entity_id.cmp(&right.1.entity_id))
        });
        let Some((best_distance, best)) = candidates.first().copied() else {
            continue;
        };
        let best_ids = candidates
            .iter()
            .filter(|(distance, _)| *distance == best_distance)
            .map(|(_, entity)| entity.entity_id.as_str())
            .collect::<HashSet<_>>();
        if best_ids.len() != 1 {
            continue;
        }
        resolved.push(ResolvedSearchEntity {
            entity_id: best.entity_id.clone(),
            entity_type: best.entity_type.clone(),
            name: best.name.clone(),
            match_kind: "serving_entity_name_typo".to_string(),
            match_source: "serving_entity".to_string(),
            matched_text: query[start..end].to_string(),
            polarity: if match_has_exclusion_prefix(&query_lower, start) {
                "exclusion".to_string()
            } else {
                "positive".to_string()
            },
            source_span: Some(SourceSpan {
                source_turn_id: String::new(),
                start,
                end,
                raw_text: query[start..end].to_string(),
            }),
        });
    }
    resolved
}

fn entity_name_tokens(name: &str) -> Vec<String> {
    name.split(|character: char| !character.is_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (!token.is_empty()).then_some(token)
        })
        .collect()
}

#[cfg(test)]
fn resolve_serving_query_entities_from_records(
    query: &str,
    intent: &SearchIntent,
    entities_source: &[ServingEntityRecord],
) -> Vec<ResolvedSearchEntity> {
    let plan = query_plan::compile_query_plan(query);
    resolve_serving_query_entities_from_records_with_alias_index(
        query,
        &plan,
        intent,
        entities_source,
        &ServingEntityAliasIndex::default(),
    )
}

fn is_serving_resolvable_entity_type(entity_type: &str) -> bool {
    let config = search_resolution_config();
    let configured_types = if config.resolvable_entity_types.is_empty() {
        default_resolvable_entity_types()
    } else {
        config.resolvable_entity_types.as_slice()
    };
    configured_types
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(entity_type))
}

fn exact_entity_match_ranges(query_lower: &str, name: &str) -> Vec<(usize, usize)> {
    let name = name.trim().to_ascii_lowercase();
    let mut ranges = Vec::new();
    if name.is_empty() {
        return ranges;
    }
    let mut search_start = 0;
    while let Some(relative_pos) = query_lower[search_start..].find(&name) {
        let start = search_start + relative_pos;
        let end = start + name.len();
        let before_ok = query_lower[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        let after_ok = query_lower[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        if before_ok && after_ok {
            ranges.push((start, end));
        }
        search_start = end;
        if search_start >= query_lower.len() {
            break;
        }
    }
    ranges
}

fn match_has_exclusion_prefix(query_lower: &str, start: usize) -> bool {
    let config = search_resolution_config();
    let configured_prefixes = if config.exclusion_prefixes.is_empty() {
        default_resolution_exclusion_prefixes()
    } else {
        config.exclusion_prefixes.as_slice()
    };
    let prefix =
        query_lower[..start].trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',');
    configured_prefixes
        .iter()
        .any(|phrase| prefix_ends_with_phrase(prefix, phrase))
}

fn default_resolvable_entity_types() -> &'static [String] {
    static TYPES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        ["area", "place", "society", "builder"]
            .iter()
            .map(|value| value.to_string())
            .collect()
    })
}

fn default_resolution_exclusion_prefixes() -> &'static [String] {
    static PREFIXES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    PREFIXES.get_or_init(|| {
        [
            "not interested in",
            "not looking in",
            "not looking for",
            "do not want",
            "don't want",
            "dont want",
            "avoid",
            "exclude",
            "excluding",
            "except",
            "outside",
            "not in",
            "not",
        ]
        .iter()
        .map(|value| value.to_string())
        .collect()
    })
}

fn prefix_ends_with_phrase(prefix: &str, phrase: &str) -> bool {
    let Some(before_phrase) = prefix.strip_suffix(phrase) else {
        return false;
    };
    before_phrase
        .chars()
        .next_back()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric())
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

impl SearchTimer {
    fn start() -> Self {
        Self {
            started_at: Instant::now(),
            timings: Vec::new(),
        }
    }

    fn measure<T>(&mut self, layer: &str, f: impl FnOnce() -> T) -> T {
        self.measure_value(layer, f).value
    }

    fn measure_value<T>(&mut self, layer: &str, f: impl FnOnce() -> T) -> TimedValue<T> {
        let started_at = Instant::now();
        let value = f();
        let duration_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        self.timings.push(SearchLayerTiming {
            layer: layer.to_string(),
            duration_ms,
        });
        TimedValue { value }
    }

    fn finish(self) -> Vec<SearchLayerTiming> {
        self.timings
    }
}

fn tantivy_candidate_ids(
    serving_bundle: Option<&LoadedServingBundle>,
    query: &str,
    search_index: &SearchIndex,
) -> TantivyRecallResult {
    let Some(serving_bundle) = serving_bundle else {
        return TantivyRecallResult {
            property_ids: Vec::new(),
            entity_hits: Vec::new(),
            warning: None,
        };
    };
    let recall_query = super::analyzer::search_tokens(query, super::schema::query_stopwords())
        .into_iter()
        .map(|term| format!("{term:?}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    if recall_query.is_empty() {
        return TantivyRecallResult {
            property_ids: Vec::new(),
            entity_hits: Vec::new(),
            warning: None,
        };
    }
    let hits = match serving_bundle
        .recall_index
        .search(&recall_query, TANTIVY_RECALL_LIMIT)
    {
        Ok(hits) => hits,
        Err(err) => {
            let warning = format!("Serving bundle Tantivy recall failed: {err}");
            eprintln!("WARN: {warning}");
            return TantivyRecallResult {
                property_ids: Vec::new(),
                entity_hits: Vec::new(),
                warning: Some(warning),
            };
        }
    };
    let property_ids = search_index.property_ids_for_entity_hits(&hits);
    TantivyRecallResult {
        property_ids,
        entity_hits: hits,
        warning: None,
    }
}

fn merge_candidate_ids(mut left: Option<Vec<String>>, right: Vec<String>) -> Option<Vec<String>> {
    let ids = left.get_or_insert_with(Vec::new);
    let mut seen = ids.iter().cloned().collect::<HashSet<_>>();
    for id in right {
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    left.filter(|ids| !ids.is_empty())
}

#[cfg(test)]
fn intersect_candidate_ids(left: Option<Vec<String>>, right: &[String]) -> Vec<String> {
    let Some(left) = left else {
        return right.to_vec();
    };
    let right = right.iter().collect::<HashSet<_>>();
    left.into_iter().filter(|id| right.contains(id)).collect()
}

fn candidate_property_indexes(
    candidate_ids: &[String],
    property_by_id: Option<&HashMap<String, usize>>,
) -> Option<Vec<usize>> {
    let property_by_id = property_by_id?;
    let mut indexes = Vec::new();
    let mut seen = HashSet::new();
    for id in candidate_ids {
        let Some(index) = property_by_id.get(id).copied() else {
            continue;
        };
        if seen.insert(index) {
            indexes.push(index);
        }
    }
    Some(indexes)
}

fn optional_non_empty(ids: Vec<String>) -> Option<Vec<String>> {
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

fn sample_ids(ids: &[String]) -> Vec<String> {
    ids.iter().take(DIAGNOSTIC_ID_LIMIT).cloned().collect()
}

fn sample_tantivy_hits(hits: &[TantivyRecallHit]) -> Vec<TantivyHitDiagnostic> {
    hits.iter()
        .take(DIAGNOSTIC_ID_LIMIT)
        .map(|hit| TantivyHitDiagnostic {
            entity_id: hit.entity_id.clone(),
            entity_type: hit.entity_type.clone(),
            name: hit.name.clone(),
            score: hit.score,
            matched_fields: hit.matched_fields.clone(),
        })
        .collect()
}

fn sample_geo_hits(geo_query: Option<&geo::GeoSearchQuery<'_>>) -> Vec<TantivyHitDiagnostic> {
    geo_query.map_or_else(Vec::new, |query| {
        query
            .resolved_places()
            .iter()
            .take(DIAGNOSTIC_ID_LIMIT)
            .map(|place| TantivyHitDiagnostic {
                entity_id: place.entity_id.clone(),
                entity_type: "place".to_string(),
                name: place.name.clone(),
                score: place.match_score as f32,
                matched_fields: vec!["geo_place_name".to_string()],
            })
            .collect()
    })
}

fn candidate_scores(results: &[SearchResultCard]) -> Vec<CandidateScore> {
    results
        .iter()
        .take(DIAGNOSTIC_SCORE_LIMIT)
        .enumerate()
        .map(|(index, result)| CandidateScore {
            property_id: result.card.id.clone(),
            rank: index + 1,
            final_score: result.match_score,
            confidence_score: result.confidence_score.as_ref().map(|score| score.overall),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::dag_config::SearchResolutionConfig;
    use crate::graph::GraphIndex;
    use crate::knowledge::FactValue;
    use crate::models::Society;
    use crate::search::evaluation::{EvaluationState, InventoryOption};
    use crate::search::geo::SpatialEntityIndex;
    use crate::search::intent::SearchIntent;
    use crate::search::SearchCapabilityIndex;
    use crate::serving::{
        materialize_society_aliases, normalize_alias, EvidenceRef, LoadedServingBundle,
        ReraEvidenceIndex, ServingBundleManifest, ServingEdgeRecord, ServingEntityAliasIndex,
        ServingEntityAliasRecord, ServingEntityRecord, ServingFactIndex, ServingFactRecord,
        SourceObservation, SpatialServingIndex, TantivyRecallIndex,
    };

    use super::*;

    #[test]
    fn candidate_vector_operations_stay_bounded_at_ten_thousand_ids() {
        let left = (0..10_000)
            .map(|index| format!("property-{index}"))
            .collect::<Vec<_>>();
        let right = (5_000..15_000)
            .map(|index| format!("property-{index}"))
            .collect::<Vec<_>>();
        let positions = (0..15_000)
            .map(|index| (format!("property-{index}"), index))
            .collect::<HashMap<_, _>>();

        let started = Instant::now();
        let merged = merge_candidate_ids(Some(left), right.clone()).expect("merged candidates");
        let intersection = intersect_candidate_ids(Some(merged.clone()), &right);
        let indexes =
            candidate_property_indexes(&merged, Some(&positions)).expect("candidate indexes");
        let elapsed = started.elapsed();

        assert_eq!(merged.len(), 15_000);
        assert_eq!(intersection.len(), 10_000);
        assert_eq!(indexes.len(), 15_000);
        assert!(
            elapsed < Duration::from_millis(250),
            "hash-indexed candidate operations took {elapsed:?}"
        );
    }

    fn empty_intent() -> SearchIntent {
        SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: None,
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            ranking_priorities: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        }
    }

    fn serving_entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            visibility: Default::default(),
            searchable_text: name.to_string(),
        }
    }

    fn society_alias_index(aliases: &[(&str, &str, &str)]) -> ServingEntityAliasIndex {
        ServingEntityAliasIndex::from_records(
            aliases
                .iter()
                .map(|(alias, entity_id, entity_name)| ServingEntityAliasRecord {
                    alias: (*alias).to_string(),
                    normalized_alias: normalize_alias(alias),
                    entity_id: (*entity_id).to_string(),
                    entity_type: "society".to_string(),
                    entity_name: (*entity_name).to_string(),
                    source: if entity_name.to_ascii_lowercase().contains(" by ") {
                        "builder_byline"
                    } else {
                        "test"
                    }
                    .to_string(),
                })
                .collect(),
        )
        .unwrap()
    }

    fn built_by_edge(society_id: &str, builder_id: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: society_id.to_string(),
            edge_type: "built_by".to_string(),
            to_entity_id: builder_id.to_string(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
            derivation: None,
        }
    }

    fn test_property(id: &str, area: &str) -> Property {
        Property {
            id: id.to_string(),
            title: id.to_string(),
            area: area.to_string(),
            area_id: slug(area),
            city: "Bengaluru".to_string(),
            society_id: format!("soc-{id}"),
            builder_name: "Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 10_000_000,
            price_min: None,
            price_max: None,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_000,
            super_builtup_sqft: 1_200,
            floor: 1,
            total_floors: 10,
            facing: "East".to_string(),
            possession_status: "Ready".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 5_000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 1,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: "test".to_string(),
        }
    }

    fn coordinate_fact(entity_id: &str, fact_key: &str, value: f64) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "numeric".to_string(),
            value_text: Some(value.to_string()),
            value: FactValue::Numeric(value),
            confidence: 0.9,
            source_type: "source_entity_seed".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            observation: None,
        }
    }

    #[test]
    fn inventory_predicates_share_one_validated_evidence_reference() {
        let subject = "society:one";
        let snapshot = "bundle:v9";
        let observation = SourceObservation::new(
            "FixtureProvider",
            "listing-one",
            subject,
            Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            Some("https://example.test/listing-one".to_string()),
            vec!["asset:external-listings/v1".to_string()],
        )
        .unwrap();
        let evidence = EvidenceRef::for_observation(snapshot, &observation);
        let option = InventoryOption {
            property_id: "property:one".to_string(),
            society_id: subject.to_string(),
            bhk: Some(3),
            price_min: Some(10_000_000),
            price_max: Some(10_000_000),
            size_sqft: Some(1_000),
            evidence_reference: Some(evidence.clone()),
        };

        let bhk_evaluation = option.evaluate_bhk("property:one", subject, 3, snapshot);
        let price_evaluation =
            option.evaluate_budget("property:one", subject, None, Some(10_000_000), snapshot);
        let bhk = &bhk_evaluation.verified_matches[0];
        let price = &price_evaluation.verified_matches[0];

        assert_eq!(bhk.evidence_refs, vec![evidence.clone()]);
        assert_eq!(price.evidence_refs, vec![evidence]);
        assert_eq!(
            option
                .evaluate_bhk("property:one", subject, 3, "bundle:other")
                .state,
            EvaluationState::Unknown
        );
    }

    fn run_search_for_test(query: &str, properties: &[Property]) -> SearchEngineOutput {
        let snapshot = test_runtime_snapshot(properties);
        SearchEngine::new(&snapshot).search(query)
    }

    fn test_runtime_snapshot(properties: &[Property]) -> SearchRuntimeSnapshot {
        let entities = properties
            .iter()
            .map(|property| ServingEntityRecord {
                entity_id: format!("society:{}", property.society_id),
                entity_type: "society".to_string(),
                name: property.society_id.clone(),
                root_source: Some("engine_test".to_string()),
                visibility: Default::default(),
                searchable_text: property.society_id.clone(),
            })
            .collect::<Vec<_>>();
        let facts = Vec::new();
        let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
        let cache_dir = tempdir().expect("temporary engine test bundle").keep();
        let recall_index = TantivyRecallIndex::build_in_dir(&cache_dir, &entities, &facts, &[])
            .expect("engine test recall index");
        let entity_index = SpatialEntityIndex::from_serving_bundle(&entities, &fact_index);
        let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        let search_index = SearchIndex::build_with_serving_entities(properties, &entities);
        let bundle = LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "engine-unit-test".to_string(),
                format_version: 1,
                created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                entity_count: entities.len() as u64,
                entity_alias_count: 0,
                fact_count: 0,
                search_metadata_count: 0,
                rera_evidence_count: 0,
                excluded_rera_evidence_society_ids: Vec::new(),
                edge_count: 0,
                eligibility_policy_version: 0,
                quarantined_society_count: 0,
                quarantine_reason_counts: Default::default(),
                entity_parquet_key: "entities.parquet".to_string(),
                entity_alias_parquet_key: "aliases.parquet".to_string(),
                fact_parquet_key: "facts.parquet".to_string(),
                search_metadata_parquet_key: "search.parquet".to_string(),
                rera_evidence_parquet_key: "rera.parquet".to_string(),
                edge_parquet_key: "edges.parquet".to_string(),
                quarantine_report_key: "quarantine.json".to_string(),
                schema_key: "schema.json".to_string(),
                tantivy_index_prefix: "tantivy".to_string(),
                artifacts: Vec::new(),
            },
            entities,
            entity_alias_index: ServingEntityAliasIndex::default(),
            edges: Vec::new(),
            graph_index: GraphIndex::default(),
            recall_index,
            fact_index,
            rera_evidence_index: ReraEvidenceIndex::default(),
            entity_index,
            spatial_index,
            search_capabilities: SearchCapabilityIndex::default(),
            cache_dir,
        };
        let mut snapshot = SearchRuntimeSnapshot::new(
            Arc::new(bundle),
            properties.to_vec(),
            properties
                .iter()
                .map(|property| Society {
                    id: property.society_id.clone(),
                    name: property.society_id.clone(),
                    area: property.area.clone(),
                    city: property.city.clone(),
                    builder_name: property.builder_name.clone(),
                    year_built: 0,
                    total_units: 0,
                    summary: String::new(),
                    maintenance_sentiment: String::new(),
                    livability_sentiment: String::new(),
                    common_positives: Vec::new(),
                    common_complaints: Vec::new(),
                    review_summary: String::new(),
                    google_reviews_url: None,
                    future_google_place_name: String::new(),
                    future_google_place_id: None,
                    future_review_enrichment_status: String::new(),
                })
                .collect(),
            Vec::new(),
            search_index,
        );
        snapshot.inventory_options = properties
            .iter()
            .map(|property| {
                let society_id = snapshot
                    .search_index
                    .society_entity_id_for_property(&property.id)
                    .expect("fixture society identity")
                    .to_string();
                let observation = SourceObservation::new(
                    "SearchEngineFixture",
                    property.id.clone(),
                    society_id.clone(),
                    Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    Some(format!("https://example.test/{}", property.id)),
                    vec!["asset:search-engine-fixture/v1".to_string()],
                )
                .unwrap();
                let exact_price = (property.price > 0).then_some(property.price);
                (
                    property.id.clone(),
                    InventoryOption {
                        property_id: property.id.clone(),
                        society_id,
                        bhk: (property.bhk > 0).then_some(property.bhk),
                        price_min: property.price_min.or(exact_price),
                        price_max: property.price_max.or(exact_price),
                        size_sqft: (property.super_builtup_sqft > 0)
                            .then_some(property.super_builtup_sqft),
                        evidence_reference: Some(EvidenceRef::for_observation(
                            &snapshot.version_key.serving_bundle_version,
                            &observation,
                        )),
                    },
                )
            })
            .collect();
        snapshot
    }

    #[test]
    fn response_cap_is_applied_after_all_eligible_properties_are_ranked() {
        let properties = (0..48)
            .map(|index| test_property(&format!("eligible-{index:02}"), "Whitefield"))
            .collect::<Vec<_>>();

        let output = run_search_for_test("3bhk homes under 2cr", &properties);
        assert_eq!(output.eligible_result_count, 48);
        assert_eq!(output.results.len(), 32);
        assert!(output
            .results
            .iter()
            .all(|result| result.card.bhk == 3 && result.card.price <= 20_000_000));
    }

    #[test]
    fn deduped_results_retain_membership_in_every_matching_branch() {
        let property = test_property("shared", "Whitefield");
        let output = run_search_for_test("3bhk", std::slice::from_ref(&property));
        let result = output.results[0].clone();
        let result_sets = vec![
            SearchResultSet {
                branch_id: "branch-1".to_string(),
                label: "First".to_string(),
                results: vec![result.clone()],
            },
            SearchResultSet {
                branch_id: "branch-2".to_string(),
                label: "Second".to_string(),
                results: vec![result],
            },
        ];

        let (limited_sets, unique_results) = limit_result_sets(result_sets, 1);

        assert_eq!(unique_results.len(), 1);
        assert_eq!(limited_sets.len(), 2);
        assert!(limited_sets
            .iter()
            .all(|set| set.results[0].card.id == "shared"));
    }

    #[test]
    fn resolver_rejects_junk_tiny_entity_names() {
        let config = SearchResolutionConfig {
            min_resolvable_entity_name_chars: 3,
            ignored_entity_names: vec!["a".to_string(), "in".to_string()],
            ..SearchResolutionConfig::default()
        };

        assert!(!is_resolvable_entity_name("a", &config));
        assert!(!is_resolvable_entity_name("in", &config));
        assert!(!is_resolvable_entity_name("  ", &config));
        assert!(is_resolvable_entity_name("Forum", &config));
        assert!(is_resolvable_entity_name("DSR", &config));
    }

    #[test]
    fn grouped_budgets_remain_hard_across_branches() {
        let properties = [
            ("three-bed", 3, 21_000_000),
            ("four-bed-one", 4, 41_000_000),
            ("four-bed-two", 4, 42_000_000),
        ]
        .into_iter()
        .map(|(id, bhk, price)| {
            let mut property = test_property(id, "Whitefield");
            property.bhk = bhk;
            property.price = price;
            property
        })
        .collect::<Vec<_>>();

        let output = run_search_for_test("3BHK under 2Cr or 4BHK under 4Cr", &properties);

        assert_eq!(output.eligible_result_count, 0);
        assert!(output.results.is_empty());
        assert!(output.result_sets.is_empty());
    }

    #[test]
    fn hard_budget_returns_only_exact_matches() {
        let properties = [
            ("strict", 100_000_000),
            ("over-budget-one", 105_000_000),
            ("over-budget-two", 120_000_000),
            ("beyond-target", 140_000_000),
        ]
        .into_iter()
        .map(|(id, price)| {
            let mut property = test_property(id, "Whitefield");
            property.price = price;
            property
        })
        .collect::<Vec<_>>();

        let output = run_search_for_test("3 BHK homes under 10 crore", &properties);

        assert_eq!(output.results.len(), 1);
        assert_eq!(output.results[0].card.id, "strict");
    }

    #[test]
    fn shared_area_suffix_keeps_grouped_bhk_budget_pairs() {
        let mut east_three = test_property("east-three", "East Bengaluru");
        east_three.bhk = 3;
        east_three.price = 19_000_000;
        let mut east_four = test_property("east-four", "East Bengaluru");
        east_four.bhk = 4;
        east_four.price = 39_000_000;
        let mut crossed = test_property("crossed", "East Bengaluru");
        crossed.bhk = 3;
        crossed.price = 39_000_000;
        let mut wrong_area = test_property("wrong-area", "South Bengaluru");
        wrong_area.bhk = 4;
        wrong_area.price = 39_000_000;

        let output = run_search_for_test(
            "3BHK under 2Cr or 4BHK under 4Cr in East Bangalore",
            &[east_three, east_four, crossed, wrong_area],
        );
        let ids = output
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<HashSet<_>>();

        assert_eq!(
            ids,
            HashSet::from(["east-three", "east-four"]),
            "diagnostics={:#?}",
            output.diagnostics
        );
    }

    #[test]
    fn over_budget_home_is_not_returned() {
        let mut property = test_property("over-budget-only", "Whitefield");
        property.price = 105_000_000;

        let output = run_search_for_test("3 BHK homes under 10 crore", &[property]);

        assert_eq!(output.eligible_result_count, 0);
        assert!(output.results.is_empty());
    }

    #[test]
    fn serving_area_resolution_promotes_named_area_to_structured_constraint() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity("area:whitefield", "area", "Whitefield"),
            serving_entity("area:marathahalli", "area", "Marathahalli"),
        ];

        let resolved = resolve_serving_query_entities_from_records(
            "Whitefield 2BHK under 1.5cr near metro",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert_eq!(effective.area.as_deref(), Some("Whitefield"));
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entity_id, "area:whitefield");
        assert_eq!(resolved[0].match_source, "serving_entity");
        assert_eq!(resolved[0].polarity, "positive");
        assert_eq!(effective.areas, vec!["Whitefield".to_string()]);
    }

    #[test]
    fn serving_area_resolution_keeps_all_positive_area_alternatives() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity("area:whitefield", "area", "Whitefield"),
            serving_entity("area:sarjapur", "area", "Sarjapur"),
        ];

        let resolved = resolve_serving_query_entities_from_records(
            "Whitefield or Sarjapur under 2.2Cr",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert_eq!(effective.area, None);
        assert_eq!(effective.areas.len(), 2);
        assert!(effective.areas.iter().any(|area| area == "Whitefield"));
        assert!(effective.areas.iter().any(|area| area == "Sarjapur"));
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn serving_area_resolution_keeps_negative_area_as_exclusion_only() {
        let intent = empty_intent();
        let entities = vec![serving_entity(
            "area:electronic-city",
            "area",
            "Electronic City",
        )];

        let resolved = resolve_serving_query_entities_from_records(
            "3BHK not Electronic City under 1.5cr",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert_eq!(effective.area, None);
        assert_eq!(
            effective.excluded_areas,
            vec!["Electronic City".to_string()]
        );
        assert_eq!(resolved[0].polarity, "exclusion");
    }

    #[test]
    fn unique_society_name_part_resolves_to_canonical_serving_entity() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity(
                "society:prestige-waterford",
                "society",
                "Prestige Waterford",
            ),
            serving_entity(
                "society:prestige-lakeside",
                "society",
                "Prestige Lakeside Habitat",
            ),
        ];
        let aliases = society_alias_index(&[(
            "Waterford",
            "society:prestige-waterford",
            "Prestige Waterford",
        )]);
        let plan = query_plan::compile_query_plan("Waterford 4BHK");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "Waterford 4BHK",
            &plan,
            &intent,
            &entities,
            &aliases,
        );

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entity_id, "society:prestige-waterford");
        assert_eq!(resolved[0].name, "Prestige Waterford");
        assert_eq!(resolved[0].matched_text, "Waterford");
        assert_eq!(resolved[0].match_kind, "serving_entity_materialized_alias");
    }

    #[test]
    fn minor_multi_token_society_typo_resolves_to_serving_entity() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity("society:godrej-air", "society", "Godrej Air"),
            serving_entity("society:godrej-splendour", "society", "Godrej Splendour"),
        ];
        let plan = query_plan::compile_query_plan("Godrej Ari 3BHK");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "Godrej Ari 3BHK",
            &plan,
            &intent,
            &entities,
            &ServingEntityAliasIndex::default(),
        );

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entity_id, "society:godrej-air");
        assert_eq!(resolved[0].matched_text, "Godrej Ari");
        assert_eq!(resolved[0].match_kind, "serving_entity_name_typo");
    }

    #[test]
    fn ambiguous_multi_token_society_typo_does_not_hard_anchor() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity("society:alpha-one", "society", "Alpha One"),
            serving_entity("society:alpha-owe", "society", "Alpha Owe"),
        ];
        let plan = query_plan::compile_query_plan("Alpha Oze 3BHK");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "Alpha Oze 3BHK",
            &plan,
            &intent,
            &entities,
            &ServingEntityAliasIndex::default(),
        );

        assert!(resolved.is_empty());
    }

    #[test]
    fn phase_family_alias_resolves_all_materialized_members() {
        let intent = empty_intent();
        let entities = ["I", "II", "III", "IV"]
            .iter()
            .map(|phase| {
                serving_entity(
                    &format!("society:folium-{}", phase.to_ascii_lowercase()),
                    "society",
                    &format!("FOLIUM BY SUMADHURA PHASE-{phase}"),
                )
            })
            .collect::<Vec<_>>();
        let aliases = society_alias_index(&[
            ("Folium", "society:folium-i", "FOLIUM BY SUMADHURA PHASE-I"),
            (
                "Folium",
                "society:folium-ii",
                "FOLIUM BY SUMADHURA PHASE-II",
            ),
            (
                "Folium",
                "society:folium-iii",
                "FOLIUM BY SUMADHURA PHASE-III",
            ),
            (
                "Folium",
                "society:folium-iv",
                "FOLIUM BY SUMADHURA PHASE-IV",
            ),
        ]);
        let plan = query_plan::compile_query_plan("Folium 3BHK");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "Folium 3BHK",
            &plan,
            &intent,
            &entities,
            &aliases,
        );

        assert_eq!(resolved.len(), 4);
        assert!(resolved
            .iter()
            .all(|entity| entity.matched_text == "Folium"));
    }

    #[test]
    fn materialized_alias_resolution_preserves_each_query_occurrence() {
        let query = "Waterford 3BHK or Waterford 4BHK";
        let plan = query_plan::compile_query_plan(query);
        let aliases = society_alias_index(&[(
            "Waterford",
            "society:prestige-waterford",
            "Prestige Waterford",
        )]);

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            query,
            &plan,
            &empty_intent(),
            &[],
            &aliases,
        );

        assert_eq!(resolved.len(), 2);
        assert!(resolved
            .iter()
            .all(|entity| entity.entity_id == "society:prestige-waterford"));
    }

    #[test]
    fn semantic_entity_resolution_is_not_limited_by_diagnostic_sample_size() {
        let entities = (0..25)
            .map(|index| {
                serving_entity(
                    &format!("society:project-{index}"),
                    "society",
                    &format!("Project {index}"),
                )
            })
            .collect::<Vec<_>>();
        let query = (0..25)
            .map(|index| format!("Project {index}"))
            .collect::<Vec<_>>()
            .join(" or ");

        let resolved =
            resolve_serving_query_entities_from_records(&query, &empty_intent(), &entities);

        assert_eq!(resolved.len(), 25);
    }

    #[test]
    fn incidental_canonical_tokens_do_not_hard_anchor() {
        let entities = vec![
            serving_entity(
                "society:sumadhura-capital-residency",
                "society",
                "Sumadhura Capital Residency",
            ),
            serving_entity(
                "society:sumadhura-sunshine",
                "society",
                "Sumadhura Sunshine",
            ),
            serving_entity("builder:sumadhura", "builder", "Sumadhura Infracon"),
        ];
        let aliases = ServingEntityAliasIndex::from_records(
            materialize_society_aliases(
                &entities,
                &[
                    built_by_edge("society:sumadhura-capital-residency", "builder:sumadhura"),
                    built_by_edge("society:sumadhura-sunshine", "builder:sumadhura"),
                ],
            )
            .unwrap(),
        )
        .unwrap();

        let bhk_plan = query_plan::compile_query_plan("3BHK capital appreciation");
        let bhk_intent = query_plan::project_search_intent("3BHK capital appreciation", &bhk_plan);
        let bhk_query = resolve_serving_query_entities_from_records_with_alias_index(
            "3BHK capital appreciation",
            &bhk_plan,
            &bhk_intent,
            &entities,
            &aliases,
        );
        let home_plan = query_plan::compile_query_plan("family home capital appreciation");
        let home_intent =
            query_plan::project_search_intent("family home capital appreciation", &home_plan);
        let home_query = resolve_serving_query_entities_from_records_with_alias_index(
            "family home capital appreciation",
            &home_plan,
            &home_intent,
            &entities,
            &aliases,
        );

        assert!(bhk_query.is_empty());
        assert!(home_query.is_empty());
    }

    #[test]
    fn distinctive_society_alias_supports_exclusion() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity(
                "society:prestige-waterford",
                "society",
                "Prestige Waterford",
            ),
            serving_entity(
                "society:prestige-lakeside",
                "society",
                "Prestige Lakeside Habitat",
            ),
        ];
        let aliases = society_alias_index(&[(
            "Waterford",
            "society:prestige-waterford",
            "Prestige Waterford",
        )]);
        let plan = query_plan::compile_query_plan("avoid Waterford");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "avoid Waterford",
            &plan,
            &intent,
            &entities,
            &aliases,
        );

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entity_id, "society:prestige-waterford");
        assert_eq!(resolved[0].polarity, "exclusion");
    }

    #[test]
    fn ambiguous_society_name_part_does_not_hard_anchor() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity(
                "society:prestige-waterford",
                "society",
                "Prestige Waterford",
            ),
            serving_entity(
                "society:prestige-lakeside",
                "society",
                "Prestige Lakeside Habitat",
            ),
        ];

        let resolved =
            resolve_serving_query_entities_from_records("Prestige 4BHK", &intent, &entities);

        assert!(resolved.is_empty());
    }

    #[test]
    fn central_area_language_does_not_hard_anchor_society() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity("society:century-central", "society", "Century Central"),
            serving_entity("builder:century", "builder", "Century Real Estate"),
        ];
        let aliases = ServingEntityAliasIndex::from_records(
            materialize_society_aliases(
                &entities,
                &[built_by_edge("society:century-central", "builder:century")],
            )
            .unwrap(),
        )
        .unwrap();
        let plan = query_plan::compile_query_plan("3BHK central Bangalore");

        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "3BHK central Bangalore",
            &plan,
            &intent,
            &entities,
            &aliases,
        );

        assert!(aliases.get("Central").is_none());
        assert!(resolved
            .iter()
            .all(|entity| entity.entity_id != "society:century-central"));
    }

    #[test]
    fn request_path_does_not_synthesize_aliases_from_entities() {
        let intent = empty_intent();
        let entities = (0..5_000)
            .map(|index| {
                let brand = index / 2;
                let alias = if index % 2 == 0 {
                    "Waterford".to_string()
                } else {
                    format!("Other {brand}")
                };
                serving_entity(
                    &format!("society:brand-{index}"),
                    "society",
                    &format!("Brand{brand} {alias}"),
                )
            })
            .collect::<Vec<_>>();

        let aliases = ServingEntityAliasIndex::default();
        let plan = query_plan::compile_query_plan("Waterford 4BHK");
        let resolved = resolve_serving_query_entities_from_records_with_alias_index(
            "Waterford 4BHK",
            &plan,
            &intent,
            &entities,
            &aliases,
        );

        assert!(resolved.is_empty());
    }

    #[test]
    fn serving_society_exclusion_is_applied_to_intent() {
        let intent = empty_intent();
        let entities = vec![
            serving_entity(
                "society:prestige-waterford",
                "society",
                "Prestige Waterford",
            ),
            serving_entity("society:prestige-elysian", "society", "Prestige Elysian"),
        ];

        let resolved = resolve_serving_query_entities_from_records(
            "3BHK under 4Cr, avoid Prestige Waterford",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entity_id, "society:prestige-waterford");
        assert_eq!(resolved[0].polarity, "exclusion");
        assert_eq!(
            effective.excluded_societies,
            vec!["Prestige Waterford".to_string()]
        );
        assert!(effective.excluded_areas.is_empty());
        assert!(effective.excluded_builders.is_empty());
    }

    #[test]
    fn serving_builder_exclusion_is_applied_to_intent() {
        let intent = empty_intent();
        let entities = vec![serving_entity("builder:prestige", "builder", "Prestige")];

        let resolved = resolve_serving_query_entities_from_records(
            "3BHK under 4Cr, avoid Prestige",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert_eq!(resolved[0].polarity, "exclusion");
        assert_eq!(effective.excluded_builders, vec!["Prestige".to_string()]);
        assert!(effective.excluded_societies.is_empty());
    }

    #[test]
    fn serving_builder_resolution_compiles_as_a_positive_filter() {
        let query = "Prestige under 2Cr";
        let entities = vec![serving_entity("builder:prestige", "builder", "Prestige")];
        let plan = query_plan::compile_query_plan(query);
        let intent = query_plan::project_search_intent(query, &plan);
        let resolved = resolve_serving_query_entities_from_records(query, &intent, &entities);
        let entities = resolved_entity_constraints(&resolved);
        let compiled = IntentAst::compile(query, &plan, intent, &entities);
        let matches = |builder: &str, price: u64| {
            compiled.constraints.evaluate(&mut |term| match term {
                crate::search::ast::ConstraintTerm::Builder { display_name, .. } => {
                    display_name.eq_ignore_ascii_case(builder)
                }
                crate::search::ast::ConstraintTerm::Budget { min, max, .. } => {
                    min.as_ref().is_none_or(|bound| price >= bound.value)
                        && max.as_ref().is_none_or(|bound| price <= bound.value)
                }
                _ => true,
            })
        };

        assert!(matches("Prestige", 19_000_000));
        assert!(!matches("Other Builder", 19_000_000));
        assert!(!matches("Prestige", 21_000_000));
    }

    #[test]
    fn serving_resolution_suppresses_area_only_mentioned_inside_place_name() {
        let intent = empty_intent();
        let bundle_entities = vec![
            serving_entity("area:banashankari", "area", "Banashankari"),
            serving_entity(
                "place:sri-banashankari-hospital",
                "place",
                "Sri Banashankari Hospital",
            ),
        ];
        let mut resolved = resolve_serving_query_entities_from_records(
            "homes near Sri Banashankari Hospital",
            &intent,
            &bundle_entities,
        );

        remove_entities_only_mentioned_inside_longer_match(
            "homes near Sri Banashankari Hospital",
            &mut resolved,
        );

        assert!(resolved
            .iter()
            .any(|entity| { entity.entity_id == "place:sri-banashankari-hospital" }));
        assert!(!resolved
            .iter()
            .any(|entity| entity.entity_type.eq_ignore_ascii_case("area")));
    }

    #[test]
    fn serving_resolution_keeps_area_with_separate_query_mention() {
        let mut resolved = vec![
            ResolvedSearchEntity {
                entity_id: "area:whitefield".to_string(),
                entity_type: "area".to_string(),
                name: "Whitefield".to_string(),
                match_kind: "serving_entity_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: "Whitefield".to_string(),
                polarity: "positive".to_string(),
                source_span: Some(SourceSpan {
                    source_turn_id: String::new(),
                    start: 8,
                    end: 18,
                    raw_text: "Whitefield".to_string(),
                }),
            },
            ResolvedSearchEntity {
                entity_id: "place:manipal-hospital-whitefield".to_string(),
                entity_type: "place".to_string(),
                name: "Manipal Hospital Whitefield".to_string(),
                match_kind: "serving_entity_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: "Manipal Hospital Whitefield".to_string(),
                polarity: "positive".to_string(),
                source_span: Some(SourceSpan {
                    source_turn_id: String::new(),
                    start: 24,
                    end: 51,
                    raw_text: "Manipal Hospital Whitefield".to_string(),
                }),
            },
        ];

        remove_entities_only_mentioned_inside_longer_match(
            "3BHK in Whitefield near Manipal Hospital Whitefield",
            &mut resolved,
        );

        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn unresolved_named_area_clause_abstains() {
        assert_eq!(
            test_unresolved_named_entity_clause("3BHK in Atlantis Enclave", &[], None),
            Some("Atlantis Enclave".to_string())
        );
    }

    #[test]
    fn unresolved_direct_project_name_abstains_instead_of_becoming_free_text() {
        for query in [
            "Ajmera Nucleus 2BHK under 1.5cr",
            "Foo Bar Residency 2BHK under 1.5cr",
            "Unknown Heights",
        ] {
            assert!(
                test_unresolved_named_entity_clause(query, &[], None).is_some(),
                "expected unresolved residual clause for {query}"
            );
        }
    }

    #[test]
    fn configured_preferences_do_not_become_unresolved_project_names() {
        for query in [
            "quiet family 2BHK under 1.5cr",
            "good reviews 2BHK under 1.5cr",
            "ready to move 2BHK under 1.5cr",
            "low traffic 2BHK under 1.5cr",
        ] {
            assert_eq!(
                test_unresolved_named_entity_clause(query, &[], None),
                None,
                "configured preference was treated as an entity in {query}"
            );
        }
    }

    #[test]
    fn resolved_named_area_clause_does_not_abstain() {
        let resolved = vec![ResolvedSearchEntity {
            entity_id: "area:whitefield".to_string(),
            entity_type: "area".to_string(),
            name: "Whitefield".to_string(),
            match_kind: "serving_entity_name".to_string(),
            match_source: "serving_entity".to_string(),
            matched_text: "Whitefield".to_string(),
            polarity: "positive".to_string(),
            source_span: Some(SourceSpan {
                source_turn_id: String::new(),
                start: 8,
                end: 18,
                raw_text: "Whitefield".to_string(),
            }),
        }];

        assert_eq!(
            test_unresolved_named_entity_clause("3BHK in Whitefield under 2cr", &resolved, None),
            None
        );
    }

    #[test]
    fn resolved_project_prefix_before_numeric_evidence_does_not_abstain() {
        let resolved = vec![ResolvedSearchEntity {
            entity_id: "society:godrej-air".to_string(),
            entity_type: "society".to_string(),
            name: "Godrej Air".to_string(),
            match_kind: "serving_entity_name".to_string(),
            match_source: "serving_entity".to_string(),
            matched_text: "Godrej Air".to_string(),
            polarity: "positive".to_string(),
            source_span: Some(SourceSpan {
                source_turn_id: String::new(),
                start: 0,
                end: 10,
                raw_text: "Godrej Air".to_string(),
            }),
        }];

        assert_eq!(
            test_unresolved_named_entity_clause(
                "Godrej Air with at least 5 acres",
                &resolved,
                None,
            ),
            None
        );
    }

    #[test]
    fn unsupported_proximity_family_abstains_without_stealing_generic_suffix() {
        assert_eq!(
            test_unresolved_named_entity_clause("3BHK near a police station", &[], None),
            Some("a police station".to_string())
        );
        assert_eq!(
            test_unresolved_named_entity_clause("3BHK near metro", &[], None),
            None
        );
    }

    #[test]
    fn contextual_personal_anchor_resolves_without_discarding_other_clauses() {
        let entities = vec![
            serving_entity("area:whitefield", "area", "Whitefield"),
            serving_entity("area:marathahalli", "area", "Marathahalli"),
        ];
        let facts = ServingFactIndex::from_records(
            vec![
                coordinate_fact("area:whitefield", "geo.latitude", 12.9698),
                coordinate_fact("area:whitefield", "geo.longitude", 77.75),
                coordinate_fact("area:marathahalli", "geo.latitude", 12.9569),
                coordinate_fact("area:marathahalli", "geo.longitude", 77.7011),
            ],
            Vec::new(),
        );
        let entity_index = geo::SpatialEntityIndex::from_serving_bundle(&entities, &facts);
        let query =
            "3bhk near Whitefield close to kids school and near my wife office in Marathahalli";
        let geo_query = entity_index
            .query(query)
            .expect("Whitefield and school clauses should remain usable");
        let resolved = ["Whitefield", "Marathahalli"]
            .into_iter()
            .map(|name| {
                let start = query.find(name).expect("resolved area occurs in query");
                ResolvedSearchEntity {
                    entity_id: format!("area:{}", slug(name)),
                    entity_type: "area".to_string(),
                    name: name.to_string(),
                    match_kind: "serving_entity_name".to_string(),
                    match_source: "serving_entity".to_string(),
                    matched_text: name.to_string(),
                    polarity: "positive".to_string(),
                    source_span: Some(SourceSpan {
                        source_turn_id: String::new(),
                        start,
                        end: start + name.len(),
                        raw_text: name.to_string(),
                    }),
                }
            })
            .collect::<Vec<_>>();

        let gaps = unresolved_proximity_gaps(Some(&geo_query));

        assert!(gaps.is_empty());
        assert_eq!(
            test_unresolved_named_entity_clause(query, &resolved, Some(&geo_query)),
            None
        );
    }

    #[test]
    fn hyphenated_status_words_do_not_create_named_entity_scopes() {
        assert_eq!(
            test_unresolved_named_entity_clause(
                "move-in-ready 3 BHK backed by a current listing price",
                &[],
                None,
            ),
            None
        );
    }

    #[test]
    fn generic_project_phrases_do_not_create_named_entity_scopes() {
        assert_eq!(
            test_unresolved_named_entity_clause(
                "three-bedroom inventory with asking-price proof in a project whose RERA complaint count is zero",
                &[],
                None,
            ),
            None
        );
    }

    #[test]
    fn generic_community_phrases_do_not_create_named_entity_scopes() {
        assert_eq!(
            test_unresolved_named_entity_clause(
                "Whitefield homes with a clearly stated total number of homes in the community",
                &[],
                None,
            ),
            None
        );
    }

    #[test]
    fn missing_named_area_abstains_instead_of_guessing() {
        let intent = empty_intent();
        let entities = vec![serving_entity("area:whitefield", "area", "Whitefield")];

        let resolved = resolve_serving_query_entities_from_records(
            "3BHK in Atlantis Heights under 2cr",
            &intent,
            &entities,
        );
        let effective = apply_resolved_constraints(intent, &resolved);

        assert!(resolved.is_empty());
        assert_eq!(effective.area, None);
        assert!(effective.excluded_areas.is_empty());
    }

    fn test_unresolved_named_entity_clause(
        query: &str,
        resolved_entities: &[ResolvedSearchEntity],
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
    ) -> Option<String> {
        let plan = query_plan::compile_query_plan(query);
        unresolved_named_entity_clause(query, &plan, resolved_entities, geo_query)
    }
}
