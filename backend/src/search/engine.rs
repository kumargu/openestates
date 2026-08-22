use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::dag_config::{area_alias_entries, search_resolution_config};
use crate::knowledge::KnowledgeGraph;
use crate::models::{Property, Society};
use crate::serving::{
    LoadedServingBundle, ServingEntityAliasIndex, ServingEntityRecord, TantivyRecallHit,
};
use crate::state::SEARCH_ENGINE_VERSION;

use super::ast::{CompiledQuery, ResolvedEntityConstraint};
use super::geo;
use super::index::SearchIndex;
use super::intent::SearchIntent;
use super::query_plan::{self, QueryPlan};
use super::resolver::{is_resolvable_entity_name, query_contains_lower_text, slug};
use super::schema;
use super::{SearchResultCard, SearchResultSet, TextSearch, TextSearchRequest};

const TANTIVY_RECALL_LIMIT: usize = 128;
const UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT: usize = 16;
const DIAGNOSTIC_ID_LIMIT: usize = 20;
const DIAGNOSTIC_SCORE_LIMIT: usize = 8;

pub struct SearchEngine<'a> {
    pub properties: &'a [Property],
    pub search_index: &'a SearchIndex,
    pub serving_bundle: Option<&'a LoadedServingBundle>,
    pub society_names: &'a HashMap<String, String>,
    pub property_by_id: Option<&'a HashMap<String, usize>>,
    pub societies: &'a [Society],
    pub graph: Option<&'a KnowledgeGraph>,
}

#[derive(Debug, Clone)]
pub struct SearchEngineOutput {
    pub intent: SearchIntent,
    pub results: Vec<SearchResultCard>,
    pub result_sets: Vec<SearchResultSet>,
    pub eligible_result_count: usize,
    pub diagnostics: SearchDiagnostics,
    pub evidence_gaps: Vec<SearchEvidenceGap>,
}

#[derive(Debug, Clone)]
pub struct RecallSet {
    pub structured_candidate_ids: Vec<String>,
    pub structured_total_count: usize,
    pub tantivy_candidate_ids: Vec<String>,
    pub merged_extra_candidate_ids: Option<Vec<String>>,
    pub ranking_candidate_ids: Option<Vec<String>>,
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
    pub merged_extra_count: usize,
    pub structured_sample: Vec<String>,
    pub tantivy_sample: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tantivy_entity_sample: Vec<TantivyHitDiagnostic>,
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

impl<'a> SearchEngine<'a> {
    pub fn search(&self, query: &str) -> SearchEngineOutput {
        if let Some(branch_queries) = query_plan::paired_ordinal_branch_queries(query) {
            if branch_queries
                .iter()
                .all(|branch| self.branch_has_search_anchor(branch))
            {
                return self.search_independent_branches(query, &branch_queries);
            }
        }
        if let Some(branch_queries) = self.discourse_branch_queries(query) {
            return self.search_independent_branches(query, &branch_queries);
        }
        self.search_single(query)
    }

    fn search_independent_branches(
        &self,
        query: &str,
        branch_queries: &[String],
    ) -> SearchEngineOutput {
        let started_at = Instant::now();
        let outputs = branch_queries
            .iter()
            .map(|branch| self.search_single(branch))
            .collect::<Vec<_>>();
        combine_branch_outputs(query, outputs, started_at.elapsed().as_secs_f64() * 1000.0)
    }

    fn discourse_branch_queries(&self, query: &str) -> Option<Vec<String>> {
        let layout = query_plan::discourse_branch_layout(query)?;
        let shared_suffix = layout
            .shared_suffix
            .map(|suffix| query[suffix.start..suffix.end].trim());
        let segment_queries = layout
            .segments
            .iter()
            .map(|segment| {
                let branch = query[segment.start..segment.end].trim();
                shared_suffix
                    .map(|shared| format!("{branch} {shared}"))
                    .unwrap_or_else(|| branch.to_string())
            })
            .collect::<Vec<_>>();
        segment_queries
            .iter()
            .all(|branch| self.branch_has_search_anchor(branch))
            .then_some(segment_queries)
    }

    fn branch_has_search_anchor(&self, query: &str) -> bool {
        let plan = query_plan::compile_query_plan(query);
        if !plan.slots.bhks.is_empty()
            || !plan.slots.budgets.is_empty()
            || !plan.areas.is_empty()
            || !plan.clauses.is_empty()
            || !plan.evidence.is_empty()
        {
            return true;
        }
        let intent = query_plan::project_search_intent(query, &plan);
        !resolve_serving_query_entities(query, &plan, &intent, self.serving_bundle, self.properties)
            .is_empty()
    }

    fn search_single(&self, query: &str) -> SearchEngineOutput {
        let mut timer = SearchTimer::start();

        let query_plan = timer.measure("query_plan_compile", || {
            query_plan::compile_query_plan(query)
        });
        let parsed_intent = timer.measure("intent_parse", || {
            query_plan::project_search_intent(query, &query_plan)
        });

        let mut geo_query = timer.measure("geo_resolve", || {
            self.serving_bundle
                .and_then(|bundle| bundle.geo_index.query_with_plan(&query_plan))
        });
        let serving_resolved_entities = timer.measure("serving_entity_resolution", || {
            resolve_serving_query_entities(
                query,
                &query_plan,
                &parsed_intent,
                self.serving_bundle,
                self.properties,
            )
        });
        let requested_societies = serving_resolved_entities
            .iter()
            .filter(|entity| {
                entity.polarity != "exclusion" && entity.entity_type.eq_ignore_ascii_case("society")
            })
            .map(|entity| entity.name.clone())
            .collect::<Vec<_>>();
        let entity_constraints = resolved_entity_constraints(query, &serving_resolved_entities);
        let compiled_query = timer.measure("intent_constraints", || {
            let intent =
                apply_resolved_constraints(parsed_intent.clone(), &serving_resolved_entities);
            CompiledQuery::compile(query, &query_plan, intent, &entity_constraints)
        });
        let intent = &compiled_query.intent;
        let unresolved_entity_clause =
            unsupported_qualifier_clause(query, &query_plan).or_else(|| {
                unresolved_named_entity_clause(
                    query,
                    &query_plan,
                    &serving_resolved_entities,
                    geo_query.as_ref(),
                )
            });
        let unavailable_required_capability = self.serving_bundle.and_then(|bundle| {
            intent
                .positive_preferences
                .iter()
                .chain(intent.negative_preferences.iter())
                .filter(|preference| preference.required)
                .find(|preference| !bundle.search_capabilities.supports_preference(preference))
                .map(|preference| preference.raw_text.clone())
        });

        let mut structured_candidate_ids = timer.measure("structured_recall", || {
            self.search_index.recall_ids(&compiled_query)
        });
        let structured_total_count = structured_candidate_ids.len();
        if !has_filter_intent(&compiled_query)
            && structured_candidate_ids.len() > UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT
        {
            structured_candidate_ids.truncate(UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT);
        }
        let eligible_property_ids =
            (has_filter_intent(&compiled_query) || !requested_societies.is_empty()).then(|| {
                self.search_index
                    .recall_constraint_ids(&compiled_query)
                    .into_iter()
                    .collect::<HashSet<_>>()
            });

        let tantivy_recall = timer.measure("tantivy_recall", || {
            tantivy_candidate_ids(self.serving_bundle, query, self.search_index)
        });

        let geo_candidate_ids = timer.measure("geo_recall", || {
            let coordinate_candidates = geo_query
                .as_ref()
                .zip(self.serving_bundle)
                .map(|(query, bundle)| {
                    query
                        .spatial_candidate_society_ids(
                            &bundle.spatial_index,
                            self.search_index,
                            eligible_property_ids.as_ref(),
                        )
                        .into_iter()
                        .flat_map(|entity_id| {
                            self.search_index.property_ids_for_entity_id(&entity_id)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let serving_fact_candidates = geo_query
                .as_ref()
                .and_then(|query| {
                    self.serving_bundle.map(|bundle| {
                        query.serving_fact_candidate_property_ids(
                            self.search_index,
                            &bundle.fact_index,
                            eligible_property_ids.as_ref(),
                        )
                    })
                })
                .unwrap_or_default();
            merge_candidate_ids(
                optional_non_empty(coordinate_candidates),
                serving_fact_candidates,
            )
            .unwrap_or_default()
        });
        if let Some(query) = geo_query.as_mut() {
            query.restrict_evidence_to_properties(
                self.properties,
                self.search_index,
                &geo_candidate_ids,
            );
        }

        let resolved_geo_constraint = geo_query.as_ref().is_some_and(|query| !query.is_empty());
        let extra_candidate_ids = if resolved_geo_constraint {
            optional_non_empty(geo_candidate_ids.clone())
        } else {
            merge_candidate_ids(
                optional_non_empty(tantivy_recall.property_ids.clone()),
                geo_candidate_ids.clone(),
            )
        };
        let ranking_candidate_ids = if resolved_geo_constraint {
            Some(
                geo_candidate_ids
                    .iter()
                    .filter(|property_id| {
                        eligible_property_ids
                            .as_ref()
                            .is_none_or(|eligible| eligible.contains(*property_id))
                    })
                    .cloned()
                    .collect(),
            )
        } else {
            merge_candidate_ids(
                optional_non_empty(structured_candidate_ids.clone()),
                extra_candidate_ids.clone().unwrap_or_default(),
            )
        };
        let recall_set = RecallSet {
            structured_candidate_ids,
            structured_total_count,
            tantivy_candidate_ids: tantivy_recall.property_ids.clone(),
            merged_extra_candidate_ids: extra_candidate_ids,
            ranking_candidate_ids,
        };
        let serving_facts = self.serving_bundle.map(|bundle| &bundle.fact_index);
        let ranking_graph = if serving_facts.is_some() {
            None
        } else {
            self.graph
        };
        let ranking_candidate_indexes = recall_set
            .ranking_candidate_ids
            .as_ref()
            .and_then(|ids| candidate_property_indexes(ids, self.property_by_id));

        let mut results = timer.measure("ranking", || {
            if unresolved_entity_clause.is_some()
                || unavailable_required_capability.is_some()
                || (resolved_geo_constraint
                    && recall_set
                        .ranking_candidate_ids
                        .as_ref()
                        .is_some_and(Vec::is_empty))
            {
                Vec::new()
            } else {
                TextSearch::search(TextSearchRequest {
                    properties: self.properties,
                    search_index: Some(self.search_index),
                    extra_candidate_ids: recall_set.ranking_candidate_ids.as_deref(),
                    candidate_property_indexes: ranking_candidate_indexes.clone(),
                    geo_query: geo_query.as_ref(),
                    serving_facts,
                    society_names: self.society_names,
                    societies: self.societies,
                    compiled_query: &compiled_query,
                    graph: ranking_graph,
                })
            }
        });
        let eligible_result_count = results.len();
        let mut evidence_gaps = Vec::new();
        evidence_gaps.extend(unresolved_proximity_gaps(geo_query.as_ref()));
        results.truncate(schema::ranking_policy().result_limit);
        let result_sets = build_result_sets(
            &compiled_query,
            &results,
            self.properties,
            self.property_by_id,
            self.search_index,
            serving_facts,
        );

        let resolved_entities = timer.measure("entity_resolution", || {
            resolve_query_entities(
                query,
                intent,
                &serving_resolved_entities,
                &results,
                geo_query.as_ref(),
            )
        });
        let total_duration_ms = timer.started_at.elapsed().as_secs_f64() * 1000.0;
        let mut warnings = tantivy_recall.warning.into_iter().collect::<Vec<_>>();
        if let Some(clause) = unresolved_entity_clause {
            warnings.push(format!("unresolved named entity clause: {clause}"));
        }
        if let Some(capability) = unavailable_required_capability {
            warnings.push(format!("unavailable search capability: {capability}"));
        }
        let mut diagnostics = SearchDiagnostics {
            layer_timings: timer.finish(),
            runtime: SearchRuntimeDiagnostics {
                serving_bundle_version: self
                    .serving_bundle
                    .map(|bundle| bundle.manifest.bundle_version.clone()),
                search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
            },
            resolved: SearchResolutionDiagnostics {
                entities: resolved_entities,
            },
            recall: SearchRecallDiagnostics {
                structured_total_count: recall_set.structured_total_count,
                structured_count: recall_set.structured_candidate_ids.len(),
                tantivy_count: recall_set.tantivy_candidate_ids.len(),
                merged_extra_count: recall_set
                    .merged_extra_candidate_ids
                    .as_ref()
                    .map_or(0, Vec::len),
                structured_sample: sample_ids(&recall_set.structured_candidate_ids),
                tantivy_sample: sample_ids(&recall_set.tantivy_candidate_ids),
                tantivy_entity_sample: sample_tantivy_hits(&tantivy_recall.entity_hits)
                    .into_iter()
                    .chain(sample_geo_hits(geo_query.as_ref()))
                    .collect(),
            },
            top_candidate_scores: candidate_scores(&results),
            evidence_gaps: evidence_gaps.clone(),
            warnings,
        };
        diagnostics.layer_timings.push(SearchLayerTiming {
            layer: "total".to_string(),
            duration_ms: total_duration_ms,
        });

        SearchEngineOutput {
            intent: intent.clone(),
            results,
            result_sets,
            eligible_result_count,
            diagnostics,
            evidence_gaps,
        }
    }
}

fn combine_branch_outputs(
    query: &str,
    outputs: Vec<SearchEngineOutput>,
    total_duration_ms: f64,
) -> SearchEngineOutput {
    let mut intent =
        query_plan::project_search_intent(query, &query_plan::compile_query_plan(query));
    let mut result_sets = Vec::new();
    let mut results = Vec::new();
    let mut result_ids = HashSet::new();
    let mut evidence_gaps = Vec::new();
    for (index, output) in outputs.iter().enumerate() {
        merge_branch_intent_resolution(&mut intent, &output.intent);
        for gap in &output.evidence_gaps {
            if !evidence_gaps.iter().any(|existing: &SearchEvidenceGap| {
                existing.entity_id == gap.entity_id && existing.missing_fact == gap.missing_fact
            }) {
                evidence_gaps.push(gap.clone());
            }
        }
        let branch_results = output
            .result_sets
            .iter()
            .flat_map(|set| set.results.iter().cloned())
            .collect::<Vec<_>>();
        if branch_results.is_empty() {
            continue;
        }
        for result in &branch_results {
            if result_ids.insert(result.card.id.clone()) {
                results.push(result.clone());
            }
        }
        let label = output
            .result_sets
            .first()
            .map(|set| set.label.clone())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| format!("Option {}", index + 1));
        result_sets.push(SearchResultSet {
            branch_id: format!("branch-{}", index + 1),
            label,
            results: branch_results,
        });
    }
    let eligible_result_count = outputs
        .iter()
        .map(|output| output.eligible_result_count)
        .sum();
    results.truncate(schema::ranking_policy().result_limit);
    let diagnostics =
        combine_branch_diagnostics(&outputs, &results, &evidence_gaps, total_duration_ms);
    SearchEngineOutput {
        intent,
        results,
        result_sets,
        eligible_result_count,
        diagnostics,
        evidence_gaps,
    }
}

fn merge_branch_intent_resolution(intent: &mut SearchIntent, branch: &SearchIntent) {
    for area in branch.requested_areas() {
        push_unique_string(&mut intent.areas, area);
    }
    intent.area = (intent.areas.len() == 1).then(|| intent.areas[0].clone());
    for bhk in branch.requested_bhks() {
        if !intent.bhks.contains(&bhk) {
            intent.bhks.push(bhk);
        }
    }
    intent.bhk = (intent.bhks.len() == 1).then(|| intent.bhks[0]);
    for area in &branch.excluded_areas {
        push_unique_string(&mut intent.excluded_areas, area);
    }
    for society in &branch.excluded_societies {
        push_unique_string(&mut intent.excluded_societies, society);
    }
    for builder in &branch.excluded_builders {
        push_unique_string(&mut intent.excluded_builders, builder);
    }
}

fn combine_branch_diagnostics(
    outputs: &[SearchEngineOutput],
    results: &[SearchResultCard],
    evidence_gaps: &[SearchEvidenceGap],
    total_duration_ms: f64,
) -> SearchDiagnostics {
    let mut diagnostics = outputs
        .first()
        .map(|output| output.diagnostics.clone())
        .unwrap_or_else(|| SearchDiagnostics {
            layer_timings: Vec::new(),
            runtime: SearchRuntimeDiagnostics {
                serving_bundle_version: None,
                search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
            },
            resolved: SearchResolutionDiagnostics {
                entities: Vec::new(),
            },
            recall: SearchRecallDiagnostics {
                structured_total_count: 0,
                structured_count: 0,
                tantivy_count: 0,
                merged_extra_count: 0,
                structured_sample: Vec::new(),
                tantivy_sample: Vec::new(),
                tantivy_entity_sample: Vec::new(),
            },
            top_candidate_scores: Vec::new(),
            evidence_gaps: Vec::new(),
            warnings: Vec::new(),
        });
    diagnostics.layer_timings.clear();
    diagnostics.resolved.entities.clear();
    diagnostics.recall.structured_total_count = 0;
    diagnostics.recall.structured_count = 0;
    diagnostics.recall.tantivy_count = 0;
    diagnostics.recall.merged_extra_count = 0;
    diagnostics.recall.structured_sample.clear();
    diagnostics.recall.tantivy_sample.clear();
    diagnostics.recall.tantivy_entity_sample.clear();
    diagnostics.warnings.clear();
    for output in outputs {
        for timing in &output.diagnostics.layer_timings {
            if timing.layer == "total" {
                continue;
            }
            if let Some(existing) = diagnostics
                .layer_timings
                .iter_mut()
                .find(|existing| existing.layer == timing.layer)
            {
                existing.duration_ms += timing.duration_ms;
            } else {
                diagnostics.layer_timings.push(timing.clone());
            }
        }
        for entity in &output.diagnostics.resolved.entities {
            if !diagnostics.resolved.entities.iter().any(|existing| {
                existing.entity_id == entity.entity_id
                    && existing.matched_text == entity.matched_text
                    && existing.polarity == entity.polarity
            }) {
                diagnostics.resolved.entities.push(entity.clone());
            }
        }
        diagnostics.recall.structured_total_count +=
            output.diagnostics.recall.structured_total_count;
        diagnostics.recall.structured_count += output.diagnostics.recall.structured_count;
        diagnostics.recall.tantivy_count += output.diagnostics.recall.tantivy_count;
        diagnostics.recall.merged_extra_count += output.diagnostics.recall.merged_extra_count;
        for id in &output.diagnostics.recall.structured_sample {
            push_unique_string(&mut diagnostics.recall.structured_sample, id);
        }
        for id in &output.diagnostics.recall.tantivy_sample {
            push_unique_string(&mut diagnostics.recall.tantivy_sample, id);
        }
        for hit in &output.diagnostics.recall.tantivy_entity_sample {
            if !diagnostics
                .recall
                .tantivy_entity_sample
                .iter()
                .any(|existing| existing.entity_id == hit.entity_id)
            {
                diagnostics.recall.tantivy_entity_sample.push(hit.clone());
            }
        }
        for warning in &output.diagnostics.warnings {
            push_unique_string(&mut diagnostics.warnings, warning);
        }
    }
    diagnostics.layer_timings.push(SearchLayerTiming {
        layer: "total".to_string(),
        duration_ms: total_duration_ms,
    });
    diagnostics.top_candidate_scores = candidate_scores(results);
    diagnostics.evidence_gaps = evidence_gaps.to_vec();
    diagnostics
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
    compiled_query: &CompiledQuery,
    results: &[SearchResultCard],
    properties: &[Property],
    property_by_id: Option<&HashMap<String, usize>>,
    search_index: &SearchIndex,
    serving_facts: Option<&crate::serving::ServingFactIndex>,
) -> Vec<SearchResultSet> {
    let branches = compiled_query.constraints.flat_branches();
    let branch_count = branches.len();
    branches
        .into_iter()
        .enumerate()
        .filter_map(|(index, branch)| {
            let mut branch_results = Vec::new();
            for result in results {
                let property = property_by_id
                    .and_then(|by_id| by_id.get(&result.card.id))
                    .and_then(|property_index| properties.get(*property_index))
                    .or_else(|| {
                        properties
                            .iter()
                            .find(|property| property.id == result.card.id)
                    });
                let Some(property) = property else {
                    continue;
                };
                let exact = branch.evaluate(&mut |term| {
                    super::text::property_matches_constraint_term_with_index(
                        property,
                        term,
                        Some(search_index),
                        serving_facts,
                    )
                });
                if !exact {
                    continue;
                }
                let mut result = result.clone();
                result.match_tier = "exact".to_string();
                result.tradeoff_label = None;
                branch_results.push(result);
            }
            if branch_results.is_empty() {
                return None;
            }
            let mut label = branch.buyer_label();
            if label.is_empty() {
                label = if branch_count == 1 {
                    "Matches".to_string()
                } else {
                    format!("Option {}", index + 1)
                };
            }
            Some(SearchResultSet {
                branch_id: format!("branch-{}", index + 1),
                label,
                results: branch_results,
            })
        })
        .collect()
}

fn resolved_entity_constraints(
    query: &str,
    resolved_entities: &[ResolvedSearchEntity],
) -> Vec<ResolvedEntityConstraint> {
    let query_lower = query.to_ascii_lowercase();
    let mut constraints = Vec::new();
    for entity in resolved_entities.iter().filter(|entity| {
        ["area", "society", "builder"]
            .iter()
            .any(|entity_type| entity.entity_type.eq_ignore_ascii_case(entity_type))
    }) {
        let exclusion = entity.polarity == "exclusion";
        for (start, end) in resolved_occurrence_ranges(&query_lower, entity) {
            let constraint = ResolvedEntityConstraint {
                entity_id: entity.entity_id.clone(),
                entity_type: entity.entity_type.clone(),
                display_name: entity.name.clone(),
                span: crate::search::intent::SourceSpan {
                    start,
                    end,
                    raw_text: query[start..end].to_string(),
                },
                exclusion,
            };
            if !constraints.contains(&constraint) {
                constraints.push(constraint);
            }
        }
    }
    constraints
}

fn resolved_occurrence_ranges(
    query_lower: &str,
    entity: &ResolvedSearchEntity,
) -> Vec<(usize, usize)> {
    let exclusion = entity.polarity == "exclusion";
    exact_entity_match_ranges(query_lower, &entity.matched_text)
        .into_iter()
        .filter(|(start, _)| match_has_exclusion_prefix(query_lower, *start) == exclusion)
        .collect()
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
    properties: &[Property],
) -> Vec<ResolvedSearchEntity> {
    let mut entities = resolve_runtime_area_query_entities(query, intent, properties);
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
    remove_entities_only_mentioned_inside_longer_match(query, &mut entities);
    entities
}

fn remove_entities_only_mentioned_inside_longer_match(
    query: &str,
    entities: &mut Vec<ResolvedSearchEntity>,
) {
    let query_lower = query.to_ascii_lowercase();
    let entity_ranges = entities
        .iter()
        .map(|entity| exact_entity_match_ranges(&query_lower, &entity.matched_text))
        .collect::<Vec<_>>();
    let keep = entities
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            entity_ranges[candidate_index]
                .iter()
                .any(|candidate_range| {
                    !entities.iter().enumerate().any(|(other_index, other)| {
                        other_index != candidate_index
                            && other.matched_text.len() > candidate.matched_text.len()
                            && entity_ranges[other_index].iter().any(|other_range| {
                                other_range.0 <= candidate_range.0
                                    && other_range.1 >= candidate_range.1
                            })
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

    let query_lower = query.to_ascii_lowercase();
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
        |span| entity_scope_is_fully_resolved(&query_lower, plan, resolved_entities, span),
    )
    .or_else(|| {
        query_plan::unresolved_residual_clause(query, plan, |span| {
            entity_scope_is_fully_resolved(&query_lower, plan, resolved_entities, span)
        })
    })
}

fn entity_scope_is_fully_resolved(
    query_lower: &str,
    plan: &QueryPlan,
    resolved_entities: &[ResolvedSearchEntity],
    span: query_plan::ByteSpan,
) -> bool {
    let resolved_ranges = plan
        .areas
        .iter()
        .map(|area| (area.span.start, area.span.end))
        .chain(
            resolved_entities
                .iter()
                .flat_map(|entity| exact_entity_match_ranges(query_lower, &entity.matched_text)),
        )
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

fn resolve_runtime_area_query_entities(
    query: &str,
    intent: &SearchIntent,
    properties: &[Property],
) -> Vec<ResolvedSearchEntity> {
    let resolution_config = search_resolution_config();
    let query_lower = query.to_ascii_lowercase();
    let mut entities = Vec::new();
    let mut area_names = properties
        .iter()
        .filter_map(|property| {
            let area = property.area.trim();
            (!area.is_empty()).then_some(area)
        })
        .collect::<Vec<_>>();
    area_names.sort_unstable_by_key(|area| area.to_ascii_lowercase());
    area_names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    for area in area_names {
        if !is_resolvable_entity_name(area, resolution_config) {
            continue;
        }
        for (start, end) in exact_entity_match_ranges(&query_lower, area) {
            let polarity = if match_has_exclusion_prefix(&query_lower, start) {
                "exclusion"
            } else {
                "positive"
            };
            if polarity == "positive"
                && intent
                    .excluded_areas
                    .iter()
                    .any(|excluded| excluded.eq_ignore_ascii_case(area))
            {
                continue;
            }
            entities.push(ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "runtime_area_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: query[start..end].to_string(),
                polarity: polarity.to_string(),
            });
        }
    }

    entities
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
        if !is_serving_resolvable_entity_type(&entity.entity_type)
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
        entity.entity_type.eq_ignore_ascii_case("society")
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

fn resolve_query_entities(
    query: &str,
    intent: &SearchIntent,
    serving_resolved_entities: &[ResolvedSearchEntity],
    results: &[SearchResultCard],
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
) -> Vec<ResolvedSearchEntity> {
    let resolution_config = search_resolution_config();
    let query_lower = query.to_ascii_lowercase();
    let mut entities = Vec::new();
    for entity in serving_resolved_entities {
        push_resolved(&mut entities, entity.clone());
        if entities.len() >= DIAGNOSTIC_ID_LIMIT {
            return entities;
        }
    }

    for area in intent.requested_areas() {
        let matched_text = matched_area_query_text(&query_lower, area);
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "area_alias".to_string(),
                match_source: "parser_broad_region".to_string(),
                matched_text,
                polarity: "positive".to_string(),
            },
        );
    }
    for area in &intent.excluded_areas {
        let matched_text = matched_area_query_text(&query_lower, area);
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "area_alias".to_string(),
                match_source: "parser_broad_region".to_string(),
                matched_text,
                polarity: "exclusion".to_string(),
            },
        );
    }

    if let Some(geo_query) = geo_query {
        for place in geo_query.resolved_places() {
            push_resolved(
                &mut entities,
                ResolvedSearchEntity {
                    entity_id: place.entity_id.clone(),
                    entity_type: "place".to_string(),
                    name: place.name.clone(),
                    match_kind: "place_name".to_string(),
                    match_source: "geo_place".to_string(),
                    matched_text: place.name.clone(),
                    polarity: "positive".to_string(),
                },
            );
            if entities.len() >= DIAGNOSTIC_ID_LIMIT {
                return entities;
            }
        }
    }

    for result in results {
        if is_resolvable_entity_name(&result.card.society_name, resolution_config)
            && query_contains_lower_text(&query_lower, &result.card.society_name)
        {
            push_resolved(
                &mut entities,
                ResolvedSearchEntity {
                    entity_id: result.card.kg_entity_refs.society_entity_id.clone(),
                    entity_type: "society".to_string(),
                    name: result.card.society_name.clone(),
                    match_kind: "result_society_name".to_string(),
                    match_source: "result_entity".to_string(),
                    matched_text: result.card.society_name.clone(),
                    polarity: "positive".to_string(),
                },
            );
        }

        if is_resolvable_entity_name(&result.card.builder_name, resolution_config)
            && query_contains_lower_text(&query_lower, &result.card.builder_name)
        {
            if let Some(builder_entity_id) = result.card.kg_entity_refs.builder_entity_id.as_ref() {
                push_resolved(
                    &mut entities,
                    ResolvedSearchEntity {
                        entity_id: builder_entity_id.clone(),
                        entity_type: "builder".to_string(),
                        name: result.card.builder_name.clone(),
                        match_kind: "result_builder_name".to_string(),
                        match_source: "result_entity".to_string(),
                        matched_text: result.card.builder_name.clone(),
                        polarity: "positive".to_string(),
                    },
                );
            }
        }

        if entities.len() >= DIAGNOSTIC_ID_LIMIT {
            return entities;
        }
    }

    for family in &resolution_config.place_families {
        if !family
            .aliases
            .iter()
            .any(|pattern| query_contains_lower_text(&query_lower, pattern))
        {
            continue;
        }
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("place_family:{}", family.id),
                entity_type: "place_family".to_string(),
                name: family.label.clone(),
                match_kind: "place_family_alias".to_string(),
                match_source: "parser_broad_region".to_string(),
                matched_text: family
                    .aliases
                    .iter()
                    .find(|pattern| query_contains_lower_text(&query_lower, pattern))
                    .cloned()
                    .unwrap_or_else(|| family.id.clone()),
                polarity: "positive".to_string(),
            },
        );
    }

    entities
}

fn matched_area_query_text(query_lower: &str, canonical: &str) -> String {
    if query_contains_lower_text(query_lower, canonical) {
        return canonical.to_string();
    }
    area_alias_entries()
        .iter()
        .filter(|entry| entry.canonical.eq_ignore_ascii_case(canonical))
        .flat_map(|entry| entry.aliases.iter())
        .find(|alias| query_contains_lower_text(query_lower, alias))
        .cloned()
        .unwrap_or_else(|| canonical.to_string())
}

fn push_resolved(entities: &mut Vec<ResolvedSearchEntity>, entity: ResolvedSearchEntity) {
    if entities.iter().any(|existing| {
        existing.entity_id == entity.entity_id && existing.polarity == entity.polarity
    }) {
        return;
    }
    entities.push(entity);
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

fn has_filter_intent(query: &CompiledQuery) -> bool {
    query.constraints.has_terms()
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
    let hits = match serving_bundle
        .recall_index
        .search(query, TANTIVY_RECALL_LIMIT)
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
    use std::time::Duration;

    use chrono::{TimeZone, Utc};

    use crate::dag_config::SearchResolutionConfig;
    use crate::knowledge::FactValue;
    use crate::search::intent::SearchIntent;
    use crate::serving::{
        materialize_society_aliases, normalize_alias, ServingEdgeRecord, ServingEntityAliasIndex,
        ServingEntityAliasRecord, ServingEntityRecord, ServingFactIndex, ServingFactRecord,
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
        }
    }

    fn run_search_for_test(query: &str, properties: &[Property]) -> SearchEngineOutput {
        let search_index = SearchIndex::build(properties);
        let society_names = properties
            .iter()
            .map(|property| (property.society_id.clone(), property.society_id.clone()))
            .collect::<HashMap<_, _>>();
        let property_by_id = properties
            .iter()
            .enumerate()
            .map(|(index, property)| (property.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let societies = Vec::new();
        SearchEngine {
            properties,
            search_index: &search_index,
            serving_bundle: None,
            society_names: &society_names,
            property_by_id: Some(&property_by_id),
            societies: &societies,
            graph: None,
        }
        .search(query)
    }

    #[test]
    fn response_cap_is_applied_after_all_eligible_properties_are_ranked() {
        let properties = (0..48)
            .map(|index| test_property(&format!("eligible-{index:02}"), "Whitefield"))
            .collect::<Vec<_>>();

        let output = run_search_for_test("3bhk in Whitefield under 2cr", &properties);

        assert_eq!(output.eligible_result_count, 48);
        assert_eq!(output.results.len(), 32);
        assert!(output
            .results
            .iter()
            .all(|result| result.card.bhk == 3 && result.card.price <= 20_000_000));
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
        let entities = resolved_entity_constraints(query, &resolved);
        let compiled = CompiledQuery::compile(query, &plan, intent, &entities);
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
        let properties = vec![test_property("one", "Banashankari")];
        let bundle_entities = vec![
            serving_entity("area:banashankari", "area", "Banashankari"),
            serving_entity(
                "place:sri-banashankari-hospital",
                "place",
                "Sri Banashankari Hospital",
            ),
        ];
        let mut resolved = resolve_runtime_area_query_entities(
            "homes near Sri Banashankari Hospital",
            &intent,
            &properties,
        );
        for entity in resolve_serving_query_entities_from_records(
            "homes near Sri Banashankari Hospital",
            &intent,
            &bundle_entities,
        ) {
            push_resolved(&mut resolved, entity);
        }

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
            },
            ResolvedSearchEntity {
                entity_id: "place:manipal-hospital-whitefield".to_string(),
                entity_type: "place".to_string(),
                name: "Manipal Hospital Whitefield".to_string(),
                match_kind: "serving_entity_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: "Manipal Hospital Whitefield".to_string(),
                polarity: "positive".to_string(),
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
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &facts);
        let query =
            "3bhk near Whitefield close to kids school and near my wife office in Marathahalli";
        let geo_query = geo_index
            .query(query)
            .expect("Whitefield and school clauses should remain usable");
        let resolved = ["Whitefield", "Marathahalli"]
            .into_iter()
            .map(|name| ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(name)),
                entity_type: "area".to_string(),
                name: name.to_string(),
                match_kind: "serving_entity_name".to_string(),
                match_source: "serving_entity".to_string(),
                matched_text: name.to_string(),
                polarity: "positive".to_string(),
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

    #[test]
    fn runtime_area_resolution_uses_serving_derived_property_areas() {
        let intent = empty_intent();
        let properties = vec![
            test_property("one", "Whitefield"),
            test_property("two", "Electronic City"),
        ];

        let positive = resolve_runtime_area_query_entities(
            "Whitefield 2BHK under 1.5cr",
            &intent,
            &properties,
        );
        let effective = apply_resolved_constraints(intent.clone(), &positive);
        assert_eq!(effective.area.as_deref(), Some("Whitefield"));
        assert_eq!(positive[0].match_source, "serving_entity");

        let negative =
            resolve_runtime_area_query_entities("not Electronic City 3BHK", &intent, &properties);
        let effective = apply_resolved_constraints(intent, &negative);
        assert_eq!(effective.area, None);
        assert_eq!(
            effective.excluded_areas,
            vec!["Electronic City".to_string()]
        );
        assert_eq!(negative[0].polarity, "exclusion");
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
