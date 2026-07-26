use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;

use crate::dag_config::search_resolution_config;
use crate::knowledge::KnowledgeGraph;
use crate::models::{Property, Seller, Society};
use crate::serving::{LoadedServingBundle, TantivyRecallHit};

use super::geo;
use super::index::SearchIndex;
use super::intent::{self, SearchIntent};
use super::resolver::{is_resolvable_entity_name, query_contains_lower_text, slug};
use super::semantic::{SemanticEmbedder, SemanticSearchIndex};
use super::{MatchExplanation, MatchReason, PreferenceCoverage, SearchResultCard, TextSearch};

const SEMANTIC_RECALL_LIMIT: usize = 16;
const TANTIVY_RECALL_LIMIT: usize = 128;
const UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT: usize = 16;
const STRUCTURED_LOCAL_RECALL_SKIP_LIMIT: usize = 4;
const DIAGNOSTIC_ID_LIMIT: usize = 12;
const DIAGNOSTIC_SCORE_LIMIT: usize = 8;

pub struct SearchEngine<'a> {
    pub properties: &'a [Property],
    pub search_index: &'a SearchIndex,
    pub serving_bundle: Option<&'a LoadedServingBundle>,
    pub semantic_index: &'a SemanticSearchIndex,
    pub semantic_embedder: &'a dyn SemanticEmbedder,
    pub society_names: &'a HashMap<String, String>,
    pub societies: &'a [Society],
    pub graph: Option<&'a KnowledgeGraph>,
    pub sellers: &'a [Seller],
}

#[derive(Debug, Clone)]
pub struct SearchEngineOutput {
    pub intent: SearchIntent,
    pub results: Vec<SearchResultCard>,
    pub diagnostics: SearchDiagnostics,
    pub relaxations: Vec<SearchRelaxation>,
}

#[derive(Debug, Clone)]
pub struct RecallSet {
    pub structured_candidate_ids: Vec<String>,
    pub structured_total_count: usize,
    pub tantivy_candidate_ids: Vec<String>,
    pub semantic_candidate_ids: Vec<String>,
    pub merged_extra_candidate_ids: Option<Vec<String>>,
    pub ranking_candidate_ids: Option<Vec<String>>,
    pub semantic_skipped: bool,
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
    pub relaxations: Vec<SearchRelaxation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRuntimeDiagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_bundle_version: Option<String>,
    pub semantic_embedder_model_id: String,
    pub semantic_index_model_id: String,
    pub semantic_index_document_count: usize,
    pub semantic_index_empty: bool,
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
    pub matched_text: String,
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
    pub semantic_count: usize,
    pub merged_extra_count: usize,
    pub semantic_skipped: bool,
    pub structured_sample: Vec<String>,
    pub tantivy_sample: Vec<String>,
    pub semantic_sample: Vec<String>,
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
    pub semantic_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRelaxation {
    pub kind: String,
    pub from: String,
    pub to: String,
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
        let mut timer = SearchTimer::start();

        let intent = timer.measure("intent_parse", || intent::parse_intent(query));

        let mut structured_candidate_ids = timer.measure("structured_recall", || {
            self.search_index.recall_ids(query, &intent)
        });
        let structured_total_count = structured_candidate_ids.len();
        if !has_filter_intent(&intent)
            && structured_candidate_ids.len() > UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT
        {
            structured_candidate_ids.truncate(UNSTRUCTURED_LOCAL_CANDIDATE_LIMIT);
        }

        let tantivy_recall = timer.measure("tantivy_recall", || {
            tantivy_candidate_ids(self.serving_bundle, query, self.search_index)
        });

        let geo_query = timer.measure("geo_resolve", || {
            self.serving_bundle
                .and_then(|bundle| bundle.geo_index.query(query))
        });
        let geo_candidate_ids = timer.measure("geo_recall", || {
            let coordinate_candidates = geo_query
                .as_ref()
                .map(|query| query.candidate_property_ids(self.properties))
                .unwrap_or_default();
            let serving_fact_candidates = geo_query
                .as_ref()
                .and_then(|query| {
                    self.serving_bundle.map(|bundle| {
                        query.serving_fact_candidate_property_ids(
                            self.properties,
                            &bundle.fact_index,
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

        let semantic_skipped = should_skip_semantic_recall(&intent, &structured_candidate_ids);
        let semantic_value = timer.measure_value("semantic_recall", || {
            if semantic_skipped {
                return (Vec::new(), HashMap::new());
            }
            let hits =
                self.semantic_index
                    .search(query, self.semantic_embedder, SEMANTIC_RECALL_LIMIT);
            let scores = self.search_index.property_scores_for_semantic_hits(&hits);
            let candidate_ids = scores.keys().cloned().collect::<Vec<_>>();
            (candidate_ids, scores)
        });
        let (semantic_candidate_ids, semantic_scores) = semantic_value.value;

        let explicit_geo_distance_limit = geo_query
            .as_ref()
            .is_some_and(|query| query.max_distance_km().is_some());
        let extra_candidate_ids = if explicit_geo_distance_limit {
            optional_non_empty(geo_candidate_ids.clone())
        } else {
            let extra_candidate_ids = merge_candidate_ids(
                optional_non_empty(tantivy_recall.property_ids.clone()),
                semantic_candidate_ids.clone(),
            );
            merge_candidate_ids(extra_candidate_ids, geo_candidate_ids.clone())
        };
        let ranking_candidate_ids = if explicit_geo_distance_limit {
            Some(intersect_candidate_ids(
                optional_non_empty(structured_candidate_ids.clone()),
                &geo_candidate_ids,
            ))
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
            semantic_candidate_ids,
            merged_extra_candidate_ids: extra_candidate_ids,
            ranking_candidate_ids,
            semantic_skipped,
        };
        let semantic_scores_ref = (!semantic_scores.is_empty()).then_some(&semantic_scores);
        let serving_facts = self.serving_bundle.map(|bundle| &bundle.fact_index);
        let ranking_graph = if serving_facts.is_some() {
            None
        } else {
            self.graph
        };

        let mut results = timer.measure("ranking", || {
            if explicit_geo_distance_limit
                && recall_set
                    .ranking_candidate_ids
                    .as_ref()
                    .is_some_and(Vec::is_empty)
            {
                Vec::new()
            } else {
                TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
                    self.properties,
                    None,
                    recall_set.ranking_candidate_ids.as_deref(),
                    semantic_scores_ref,
                    geo_query.as_ref(),
                    serving_facts,
                    self.society_names,
                    self.societies,
                    query,
                    &intent,
                    ranking_graph,
                    self.sellers,
                )
            }
        });
        let mut relaxations = Vec::new();
        if results.is_empty() && !explicit_geo_distance_limit {
            let relaxation_value = timer.measure_value("constraint_relaxation", || {
                self.relaxed_results(
                    query,
                    &intent,
                    recall_set.merged_extra_candidate_ids.as_deref(),
                    semantic_scores_ref,
                    geo_query.as_ref(),
                    serving_facts,
                    ranking_graph,
                )
            });
            if let Some((mut relaxed_results, applied)) = relaxation_value.value {
                annotate_relaxed_results(&mut relaxed_results, &applied);
                relaxations = applied;
                results = relaxed_results;
            }
        }

        let resolved_entities = timer.measure("entity_resolution", || {
            resolve_query_entities(query, &intent, &results, geo_query.as_ref())
        });
        let total_duration_ms = timer.started_at.elapsed().as_secs_f64() * 1000.0;
        let mut diagnostics = SearchDiagnostics {
            layer_timings: timer.finish(),
            runtime: SearchRuntimeDiagnostics {
                serving_bundle_version: self
                    .serving_bundle
                    .map(|bundle| bundle.manifest.bundle_version.clone()),
                semantic_embedder_model_id: self.semantic_embedder.model_id().to_string(),
                semantic_index_model_id: self.semantic_index.model_id().to_string(),
                semantic_index_document_count: self.semantic_index.len(),
                semantic_index_empty: self.semantic_index.is_empty(),
            },
            resolved: SearchResolutionDiagnostics {
                entities: resolved_entities,
            },
            recall: SearchRecallDiagnostics {
                structured_total_count: recall_set.structured_total_count,
                structured_count: recall_set.structured_candidate_ids.len(),
                tantivy_count: recall_set.tantivy_candidate_ids.len(),
                semantic_count: recall_set.semantic_candidate_ids.len(),
                merged_extra_count: recall_set
                    .merged_extra_candidate_ids
                    .as_ref()
                    .map_or(0, Vec::len),
                semantic_skipped: recall_set.semantic_skipped,
                structured_sample: sample_ids(&recall_set.structured_candidate_ids),
                tantivy_sample: sample_ids(&recall_set.tantivy_candidate_ids),
                semantic_sample: sample_ids(&recall_set.semantic_candidate_ids),
                tantivy_entity_sample: sample_tantivy_hits(&tantivy_recall.entity_hits)
                    .into_iter()
                    .chain(sample_geo_hits(geo_query.as_ref()))
                    .collect(),
            },
            top_candidate_scores: candidate_scores(&results),
            relaxations: relaxations.clone(),
            warnings: tantivy_recall.warning.into_iter().collect(),
        };
        diagnostics.layer_timings.push(SearchLayerTiming {
            layer: "total".to_string(),
            duration_ms: total_duration_ms,
        });

        SearchEngineOutput {
            intent,
            results,
            diagnostics,
            relaxations,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn relaxed_results(
        &self,
        query: &str,
        intent: &SearchIntent,
        extra_candidate_ids: Option<&[String]>,
        semantic_scores: Option<&HashMap<String, f64>>,
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
        serving_facts: Option<&crate::serving::ServingFactIndex>,
        ranking_graph: Option<&KnowledgeGraph>,
    ) -> Option<(Vec<SearchResultCard>, Vec<SearchRelaxation>)> {
        for (relaxed_intent, applied) in relaxation_attempts(intent) {
            let structured_ids = self.search_index.recall_ids(query, &relaxed_intent);
            let ranking_candidate_ids = merge_candidate_ids(
                optional_non_empty(structured_ids),
                extra_candidate_ids.unwrap_or_default().to_vec(),
            );
            let results =
                TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
                    self.properties,
                    None,
                    ranking_candidate_ids.as_deref(),
                    semantic_scores,
                    geo_query,
                    serving_facts,
                    self.society_names,
                    self.societies,
                    query,
                    &relaxed_intent,
                    ranking_graph,
                    self.sellers,
                );
            if !results.is_empty() {
                return Some((results, applied));
            }
        }

        None
    }
}

fn relaxation_attempts(intent: &SearchIntent) -> Vec<(SearchIntent, Vec<SearchRelaxation>)> {
    let mut attempts = Vec::new();
    if !intent.unsupported_inventory_types.is_empty() {
        return attempts;
    }

    if let Some(budget_max) = intent.budget_max {
        for multiplier in [1.10, 1.25, 1.50] {
            let relaxed_budget = ((budget_max as f64) * multiplier).round() as u64;
            let mut relaxed = intent.clone();
            relaxed.budget_max = Some(relaxed_budget);
            attempts.push((
                relaxed,
                vec![SearchRelaxation {
                    kind: "budget".to_string(),
                    from: budget_display(budget_max),
                    to: budget_display(relaxed_budget),
                    reason: "No exact budget match; widened budget tolerance deterministically."
                        .to_string(),
                }],
            ));
        }

        let mut relaxed = intent.clone();
        relaxed.budget_max = None;
        attempts.push((
            relaxed,
            vec![SearchRelaxation {
                kind: "budget".to_string(),
                from: budget_display(budget_max),
                to: "available market".to_string(),
                reason: "No result within budget tolerance; removed budget cap as the last budget relaxation."
                    .to_string(),
            }],
        ));
    }

    if let Some(area) = intent.area.as_deref() {
        let mut relaxed = intent.clone();
        relaxed.area = None;
        attempts.push((
            relaxed,
            vec![SearchRelaxation {
                kind: "area".to_string(),
                from: area.to_string(),
                to: "any indexed area".to_string(),
                reason: "No exact area match; widened area after stricter filters failed."
                    .to_string(),
            }],
        ));
    }

    if let (Some(area), Some(budget_max)) = (intent.area.as_deref(), intent.budget_max) {
        let mut relaxed = intent.clone();
        let relaxed_budget = ((budget_max as f64) * 1.25).round() as u64;
        relaxed.area = None;
        relaxed.budget_max = Some(relaxed_budget);
        attempts.push((
            relaxed,
            vec![
                SearchRelaxation {
                    kind: "budget".to_string(),
                    from: budget_display(budget_max),
                    to: budget_display(relaxed_budget),
                    reason: "No exact budget match; widened budget tolerance deterministically."
                        .to_string(),
                },
                SearchRelaxation {
                    kind: "area".to_string(),
                    from: area.to_string(),
                    to: "any indexed area".to_string(),
                    reason: "No exact area match; widened area after stricter filters failed."
                        .to_string(),
                },
            ],
        ));

        let mut relaxed = intent.clone();
        relaxed.area = None;
        relaxed.budget_max = None;
        attempts.push((
            relaxed,
            vec![
                SearchRelaxation {
                    kind: "budget".to_string(),
                    from: budget_display(budget_max),
                    to: "available market".to_string(),
                    reason: "No result within budget tolerance; removed budget cap as the last budget relaxation."
                        .to_string(),
                },
                SearchRelaxation {
                    kind: "area".to_string(),
                    from: area.to_string(),
                    to: "any indexed area".to_string(),
                    reason: "No exact area match; widened area after stricter filters failed."
                        .to_string(),
                },
            ],
        ));
    }

    attempts
}

fn annotate_relaxed_results(results: &mut [SearchResultCard], relaxations: &[SearchRelaxation]) {
    if relaxations.is_empty() {
        return;
    }
    let summary = relaxation_summary(relaxations);
    for result in results {
        if result.match_reason.trim().is_empty() {
            result.match_reason = summary.clone();
        } else if !result.match_reason.contains(&summary) {
            result.match_reason = format!("{}; {}", result.match_reason, summary);
        }
        let explanation = result
            .match_explanation
            .get_or_insert_with(|| MatchExplanation {
                reasons: Vec::new(),
                preference_coverage: Vec::new(),
                graph_driven_pct: 0.0,
                total_facts_consulted: 0,
            });
        for relaxation in relaxations {
            let preference = format!("relaxed {}", relaxation.kind);
            explanation.reasons.push(MatchReason {
                preference: preference.clone(),
                fact_key: "search.constraint_relaxation".to_string(),
                display: format!(
                    "{}: {} -> {}",
                    relaxation.kind, relaxation.from, relaxation.to
                ),
                score: 0.0,
                confidence: 1.0,
                source_type: "Computed".to_string(),
                scoring_method: "constraint-relaxation".to_string(),
            });
            explanation.preference_coverage.push(PreferenceCoverage {
                preference,
                status: "relaxed".to_string(),
                fact_key: Some("search.constraint_relaxation".to_string()),
            });
        }
    }
}

fn relaxation_summary(relaxations: &[SearchRelaxation]) -> String {
    let labels = relaxations
        .iter()
        .map(|relaxation| {
            format!(
                "{} {} -> {}",
                relaxation.kind, relaxation.from, relaxation.to
            )
        })
        .collect::<Vec<_>>();
    format!("Relaxed {}", labels.join(", "))
}

fn budget_display(value: u64) -> String {
    if value >= 10_000_000 {
        let cr = value as f64 / 10_000_000.0;
        format!("{cr:.2}Cr")
    } else if value >= 100_000 {
        let lakh = value as f64 / 100_000.0;
        format!("{lakh:.0}L")
    } else {
        value.to_string()
    }
}

fn resolve_query_entities(
    query: &str,
    intent: &SearchIntent,
    results: &[SearchResultCard],
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
) -> Vec<ResolvedSearchEntity> {
    let resolution_config = search_resolution_config();
    let query_lower = query.to_ascii_lowercase();
    let mut entities = Vec::new();
    if let Some(area) = intent.area.as_deref() {
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "area_alias".to_string(),
                matched_text: area.to_string(),
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
                    matched_text: place.name.clone(),
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
                    matched_text: result.card.society_name.clone(),
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
                        matched_text: result.card.builder_name.clone(),
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
                matched_text: family
                    .aliases
                    .iter()
                    .find(|pattern| query_contains_lower_text(&query_lower, pattern))
                    .cloned()
                    .unwrap_or_else(|| family.id.clone()),
            },
        );
    }

    entities
}

fn push_resolved(entities: &mut Vec<ResolvedSearchEntity>, entity: ResolvedSearchEntity) {
    if entities.iter().any(|existing| {
        existing.entity_id == entity.entity_id && existing.match_kind == entity.match_kind
    }) {
        return;
    }
    entities.push(entity);
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

fn should_skip_semantic_recall(intent: &SearchIntent, local_candidate_ids: &[String]) -> bool {
    if local_candidate_ids.is_empty() {
        return false;
    }
    !has_soft_intent(intent)
        || has_filter_intent(intent)
        || local_candidate_ids.len() >= STRUCTURED_LOCAL_RECALL_SKIP_LIMIT
}

fn has_soft_intent(intent: &SearchIntent) -> bool {
    !intent.preferences.is_empty()
        || !intent.positive_preferences.is_empty()
        || !intent.negative_preferences.is_empty()
        || intent.buyer_archetype.is_some()
}

fn has_filter_intent(intent: &SearchIntent) -> bool {
    !intent.hard_constraints.is_empty()
        || !intent.excluded_areas.is_empty()
        || intent.area.is_some()
        || intent.bhk.is_some()
        || intent.budget_max.is_some()
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
    for id in right {
        if !ids.iter().any(|existing| existing == &id) {
            ids.push(id);
        }
    }
    left.filter(|ids| !ids.is_empty())
}

fn intersect_candidate_ids(left: Option<Vec<String>>, right: &[String]) -> Vec<String> {
    let Some(left) = left else {
        return right.to_vec();
    };
    left.into_iter()
        .filter(|id| right.iter().any(|candidate| candidate == id))
        .collect()
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
            semantic_score: result.semantic_score,
            confidence_score: result.confidence_score.as_ref().map(|score| score.overall),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::dag_config::SearchResolutionConfig;

    use super::*;

    #[test]
    fn resolver_rejects_junk_tiny_entity_names() {
        let config = SearchResolutionConfig {
            min_resolvable_entity_name_chars: 3,
            ignored_entity_names: vec!["a".to_string(), "in".to_string()],
            place_families: Vec::new(),
        };

        assert!(!is_resolvable_entity_name("a", &config));
        assert!(!is_resolvable_entity_name("in", &config));
        assert!(!is_resolvable_entity_name("  ", &config));
        assert!(is_resolvable_entity_name("Forum", &config));
        assert!(is_resolvable_entity_name("DSR", &config));
    }
}
