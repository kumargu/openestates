use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::dag_config::search_resolution_config;
use crate::knowledge::KnowledgeGraph;
use crate::models::{Property, Society};
use crate::routes::enrichment::society_node_id;
use crate::serving::{
    LoadedServingBundle, ServingEntityRecord, SpatialPoint, SpatialServingIndex, TantivyRecallHit,
};
use crate::state::SEARCH_ENGINE_VERSION;

use super::geo;
use super::index::SearchIndex;
use super::intent::{self, SearchIntent};
use super::parser;
use super::resolver::{is_resolvable_entity_name, query_contains_lower_text, slug};
use super::schema;
use super::{MatchExplanation, MatchReason, PreferenceCoverage, SearchResultCard, TextSearch};

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
    pub diagnostics: SearchDiagnostics,
    pub relaxations: Vec<SearchRelaxation>,
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
    pub relaxations: Vec<SearchRelaxation>,
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
pub struct SearchRelaxation {
    pub kind: String,
    pub from: String,
    pub to: String,
    pub reason: SearchRelaxationReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_anchor: Option<SearchRelaxationAreaAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_distance_km: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRelaxationReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRelaxationAreaAnchor {
    pub entity_id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
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
        let mut timer = SearchTimer::start();

        let parsed_intent = timer.measure("intent_parse", || intent::parse_intent(query));

        let geo_query = timer.measure("geo_resolve", || {
            self.serving_bundle
                .and_then(|bundle| bundle.geo_index.query(query))
        });
        let serving_resolved_entities = timer.measure("serving_entity_resolution", || {
            resolve_serving_query_entities(
                query,
                &parsed_intent,
                self.serving_bundle,
                self.properties,
            )
        });
        let intent = timer.measure("intent_constraints", || {
            apply_resolved_constraints(parsed_intent.clone(), &serving_resolved_entities)
        });
        let unresolved_entity_clause =
            unresolved_named_entity_clause(query, &serving_resolved_entities, geo_query.as_ref());

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

        let explicit_geo_distance_limit = geo_query
            .as_ref()
            .is_some_and(|query| query.max_distance_km().is_some());
        let extra_candidate_ids = if explicit_geo_distance_limit {
            optional_non_empty(geo_candidate_ids.clone())
        } else {
            merge_candidate_ids(
                optional_non_empty(tantivy_recall.property_ids.clone()),
                geo_candidate_ids.clone(),
            )
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
                || (explicit_geo_distance_limit
                    && recall_set
                        .ranking_candidate_ids
                        .as_ref()
                        .is_some_and(Vec::is_empty))
            {
                Vec::new()
            } else {
                TextSearch::search_with_candidate_property_indexes_serving_facts_and_intent(
                    self.properties,
                    None,
                    recall_set.ranking_candidate_ids.as_deref(),
                    ranking_candidate_indexes.clone(),
                    geo_query.as_ref(),
                    serving_facts,
                    self.society_names,
                    self.societies,
                    query,
                    &intent,
                    ranking_graph,
                )
            }
        });
        let mut relaxations = Vec::new();
        let mut evidence_gaps = Vec::new();
        let relaxation_target = schema::ranking_policy()
            .constraint_relaxation
            .target_result_count;
        if results.len() < relaxation_target
            && !explicit_geo_distance_limit
            && unresolved_entity_clause.is_none()
        {
            let relaxation_value = timer.measure_value("constraint_relaxation", || {
                self.relaxed_results(
                    query,
                    &intent,
                    recall_set.merged_extra_candidate_ids.as_deref(),
                    self.property_by_id,
                    geo_query.as_ref(),
                    serving_facts,
                    ranking_graph,
                    &serving_resolved_entities,
                    results.clone(),
                )
            });
            let (relaxed_match, relaxation_gaps) = relaxation_value.value;
            evidence_gaps = relaxation_gaps;
            if let Some((relaxed_results, applied)) = relaxed_match {
                relaxations = applied;
                results = relaxed_results;
            }
        }
        evidence_gaps.extend(unresolved_proximity_gaps(geo_query.as_ref()));

        let resolved_entities = timer.measure("entity_resolution", || {
            resolve_query_entities(
                query,
                &intent,
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
            relaxations: relaxations.clone(),
            evidence_gaps: evidence_gaps.clone(),
            warnings,
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
            evidence_gaps,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn relaxed_results(
        &self,
        query: &str,
        intent: &SearchIntent,
        extra_candidate_ids: Option<&[String]>,
        property_by_id: Option<&HashMap<String, usize>>,
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
        serving_facts: Option<&crate::serving::ServingFactIndex>,
        ranking_graph: Option<&KnowledgeGraph>,
        resolved_entities: &[ResolvedSearchEntity],
        initial_results: Vec<SearchResultCard>,
    ) -> (
        Option<(Vec<SearchResultCard>, Vec<SearchRelaxation>)>,
        Vec<SearchEvidenceGap>,
    ) {
        self.run_relaxation_sequence(
            query,
            intent,
            extra_candidate_ids,
            property_by_id,
            geo_query,
            serving_facts,
            ranking_graph,
            resolved_entities,
            initial_results,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_relaxation_sequence(
        &self,
        query: &str,
        intent: &SearchIntent,
        extra_candidate_ids: Option<&[String]>,
        property_by_id: Option<&HashMap<String, usize>>,
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
        serving_facts: Option<&crate::serving::ServingFactIndex>,
        ranking_graph: Option<&KnowledgeGraph>,
        resolved_entities: &[ResolvedSearchEntity],
        initial_results: Vec<SearchResultCard>,
    ) -> (
        Option<(Vec<SearchResultCard>, Vec<SearchRelaxation>)>,
        Vec<SearchEvidenceGap>,
    ) {
        if !intent.unsupported_inventory_types.is_empty() {
            return (None, Vec::new());
        }

        let rank = |relaxed_intent: &SearchIntent, candidate_ids: Option<Vec<String>>| {
            let ranking_candidate_ids = candidate_ids.or_else(|| {
                merge_candidate_ids(
                    optional_non_empty(self.search_index.recall_ids(query, relaxed_intent)),
                    extra_candidate_ids.unwrap_or_default().to_vec(),
                )
            });
            let ranking_candidate_indexes = ranking_candidate_ids
                .as_ref()
                .and_then(|ids| candidate_property_indexes(ids, property_by_id));
            let mut results =
                TextSearch::search_with_candidate_property_indexes_serving_facts_and_intent(
                    self.properties,
                    None,
                    ranking_candidate_ids.as_deref(),
                    ranking_candidate_indexes,
                    geo_query,
                    serving_facts,
                    self.society_names,
                    self.societies,
                    query,
                    relaxed_intent,
                    ranking_graph,
                );
            if intent.budget_max.is_some() && relaxed_intent.budget_max.is_none() {
                results.retain(|result| result.card.price > 0);
            }
            results
        };

        let policy = &schema::ranking_policy().constraint_relaxation;
        let initial_result_count = initial_results.len();
        let mut accumulated_results = initial_results;
        let mut seen_property_ids = accumulated_results
            .iter()
            .map(|result| result.card.id.clone())
            .collect::<HashSet<_>>();
        let mut applied_relaxations = Vec::new();
        let mut relaxed_intent = intent.clone();
        let mut cumulative = Vec::new();
        let mut evidence_gaps = Vec::new();
        for step in &policy.order {
            match step.as_str() {
                "budget_tolerance" => {
                    let Some(budget_max) = relaxed_intent.budget_max else {
                        continue;
                    };
                    for multiplier in &policy.budget_multipliers {
                        let relaxed_budget = ((budget_max as f64) * multiplier).round() as u64;
                        let mut attempt_intent = relaxed_intent.clone();
                        attempt_intent.budget_max = Some(relaxed_budget);
                        let applied = vec![budget_tolerance_relaxation(budget_max, relaxed_budget)];
                        let results = rank(&attempt_intent, None);
                        if accumulate_relaxed_results(
                            &mut accumulated_results,
                            &mut seen_property_ids,
                            results,
                            &applied,
                            policy.target_result_count,
                        ) > 0
                        {
                            extend_unique_relaxations(&mut applied_relaxations, &applied);
                        }
                        if accumulated_results.len() >= policy.target_result_count {
                            return (
                                Some((accumulated_results, applied_relaxations)),
                                evidence_gaps,
                            );
                        }
                    }
                }
                "budget_cap" => {
                    let Some(budget_max) = relaxed_intent.budget_max else {
                        continue;
                    };
                    relaxed_intent.budget_max = None;
                    cumulative.push(budget_cap_relaxation(budget_max));
                    let results = rank(&relaxed_intent, None);
                    if accumulate_relaxed_results(
                        &mut accumulated_results,
                        &mut seen_property_ids,
                        results,
                        &cumulative,
                        policy.target_result_count,
                    ) > 0
                    {
                        extend_unique_relaxations(&mut applied_relaxations, &cumulative);
                    }
                    if accumulated_results.len() >= policy.target_result_count {
                        return (
                            Some((accumulated_results, applied_relaxations)),
                            evidence_gaps,
                        );
                    }
                }
                "bhk" => {
                    let Some(bhk) = relaxed_intent.bhk else {
                        continue;
                    };
                    relaxed_intent.bhk = None;
                    remove_bhk_preference_signals(&mut relaxed_intent, bhk);
                    cumulative.push(bhk_relaxation(bhk));
                    let results = rank(&relaxed_intent, None);
                    if accumulate_relaxed_results(
                        &mut accumulated_results,
                        &mut seen_property_ids,
                        results,
                        &cumulative,
                        policy.target_result_count,
                    ) > 0
                    {
                        extend_unique_relaxations(&mut applied_relaxations, &cumulative);
                    }
                    if accumulated_results.len() >= policy.target_result_count {
                        return (
                            Some((accumulated_results, applied_relaxations)),
                            evidence_gaps,
                        );
                    }
                }
                "area_radius" => {
                    let Some(area_name) = relaxed_intent.area.as_deref() else {
                        continue;
                    };
                    let Some((anchor, anchor_name)) = area_relaxation_anchor(
                        area_name,
                        resolved_entities,
                        self.serving_bundle.map(|bundle| &bundle.spatial_index),
                        &mut evidence_gaps,
                    ) else {
                        break;
                    };
                    let mut radius_intent = relaxed_intent.clone();
                    radius_intent.area = None;
                    for radius_km in &policy.area_radii_km {
                        let (candidate_ids, distances) = area_radius_candidate_ids(
                            self.properties,
                            self.society_names,
                            self.serving_bundle.map(|bundle| &bundle.spatial_index),
                            anchor,
                            *radius_km,
                        );
                        if candidate_ids.is_empty() {
                            continue;
                        }
                        let results = rank(&radius_intent, Some(candidate_ids));
                        if results.is_empty() {
                            continue;
                        }
                        let mut added = 0;
                        for mut result in results {
                            if accumulated_results.len() >= policy.target_result_count {
                                break;
                            }
                            if !seen_property_ids.insert(result.card.id.clone()) {
                                continue;
                            }
                            let mut applied = cumulative.clone();
                            applied.push(area_radius_relaxation(
                                area_name,
                                anchor,
                                &anchor_name,
                                *radius_km,
                                distances.get(&result.card.id).copied(),
                            ));
                            annotate_relaxed_results(std::slice::from_mut(&mut result), &applied);
                            extend_unique_relaxations(&mut applied_relaxations, &applied);
                            accumulated_results.push(result);
                            added += 1;
                        }
                        if added > 0 && accumulated_results.len() >= policy.target_result_count {
                            return (
                                Some((accumulated_results, applied_relaxations)),
                                evidence_gaps,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        if accumulated_results.len() > initial_result_count {
            (
                Some((accumulated_results, applied_relaxations)),
                evidence_gaps,
            )
        } else {
            (None, evidence_gaps)
        }
    }
}

fn accumulate_relaxed_results(
    accumulated: &mut Vec<SearchResultCard>,
    seen_property_ids: &mut HashSet<String>,
    candidates: Vec<SearchResultCard>,
    relaxations: &[SearchRelaxation],
    target_result_count: usize,
) -> usize {
    let mut added = 0;
    for mut candidate in candidates {
        if accumulated.len() >= target_result_count {
            break;
        }
        if !seen_property_ids.insert(candidate.card.id.clone()) {
            continue;
        }
        annotate_relaxed_results(std::slice::from_mut(&mut candidate), relaxations);
        accumulated.push(candidate);
        added += 1;
    }
    added
}

fn extend_unique_relaxations(
    accumulated: &mut Vec<SearchRelaxation>,
    relaxations: &[SearchRelaxation],
) {
    for relaxation in relaxations {
        let duplicate = accumulated.iter().any(|existing| {
            existing.reason.code == relaxation.reason.code
                && existing.from == relaxation.from
                && existing.to == relaxation.to
                && existing.radius_km == relaxation.radius_km
        });
        if !duplicate {
            accumulated.push(relaxation.clone());
        }
    }
}

fn relaxation_reason(code: &str, message: &str) -> SearchRelaxationReason {
    SearchRelaxationReason {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn budget_tolerance_relaxation(budget_max: u64, relaxed_budget: u64) -> SearchRelaxation {
    SearchRelaxation {
        kind: "budget".to_string(),
        from: budget_display(budget_max),
        to: budget_display(relaxed_budget),
        reason: relaxation_reason(
            "budget_tolerance",
            "No exact budget match; widened budget tolerance deterministically.",
        ),
        area_anchor: None,
        radius_km: None,
        candidate_distance_km: None,
    }
}

fn budget_cap_relaxation(budget_max: u64) -> SearchRelaxation {
    SearchRelaxation {
        kind: "budget".to_string(),
        from: budget_display(budget_max),
        to: "available market".to_string(),
        reason: relaxation_reason(
            "budget_cap_removed",
            "No result within budget tolerance; removed the budget cap.",
        ),
        area_anchor: None,
        radius_km: None,
        candidate_distance_km: None,
    }
}

fn bhk_relaxation(bhk: u32) -> SearchRelaxation {
    SearchRelaxation {
        kind: "bhk".to_string(),
        from: format!("{bhk} BHK"),
        to: "available configurations".to_string(),
        reason: relaxation_reason(
            "bhk_removed",
            "No matching configuration after budget relaxation; widened configuration while preserving area.",
        ),
        area_anchor: None,
        radius_km: None,
        candidate_distance_km: None,
    }
}

fn area_radius_relaxation(
    area_name: &str,
    anchor: &SpatialPoint,
    anchor_name: &str,
    radius_km: f64,
    candidate_distance_km: Option<f64>,
) -> SearchRelaxation {
    SearchRelaxation {
        kind: "area".to_string(),
        from: area_name.to_string(),
        to: format!("within {radius_km:.0} km"),
        reason: relaxation_reason(
            "area_radius_expanded",
            "No exact-area candidate remained after earlier relaxations; expanded around the resolved area anchor.",
        ),
        area_anchor: Some(SearchRelaxationAreaAnchor {
            entity_id: anchor.entity_id.clone(),
            name: anchor_name.to_string(),
            latitude: anchor.latitude,
            longitude: anchor.longitude,
        }),
        radius_km: Some(radius_km),
        candidate_distance_km,
    }
}

fn area_relaxation_anchor<'a>(
    area_name: &str,
    resolved_entities: &[ResolvedSearchEntity],
    spatial_index: Option<&'a SpatialServingIndex>,
    evidence_gaps: &mut Vec<SearchEvidenceGap>,
) -> Option<(&'a SpatialPoint, String)> {
    let resolved_area = resolved_entities.iter().find(|entity| {
        entity.entity_type.eq_ignore_ascii_case("area")
            && entity.polarity == "positive"
            && entity.name.eq_ignore_ascii_case(area_name)
    });
    let entity_id = resolved_area
        .map(|entity| entity.entity_id.clone())
        .unwrap_or_else(|| format!("area:{}", slug(area_name)));
    let area_label = resolved_area
        .map(|entity| entity.name.clone())
        .unwrap_or_else(|| area_name.to_string());
    let anchor = spatial_index.and_then(|index| index.point_for_entity(&entity_id));
    if let Some(anchor) = anchor {
        return Some((anchor, area_label));
    }

    for missing_fact in ["geo.latitude", "geo.longitude"] {
        evidence_gaps.push(SearchEvidenceGap {
            entity_id: entity_id.clone(),
            missing_fact: missing_fact.to_string(),
            reason: format!(
                "Area radius relaxation requested for {area_label}, but the promoted serving bundle has no resolved area coordinates"
            ),
        });
    }
    None
}

fn area_radius_candidate_ids(
    properties: &[Property],
    society_names: &HashMap<String, String>,
    spatial_index: Option<&SpatialServingIndex>,
    anchor: &SpatialPoint,
    radius_km: f64,
) -> (Vec<String>, HashMap<String, f64>) {
    let Some(spatial_index) = spatial_index else {
        return (Vec::new(), HashMap::new());
    };
    let nearby_societies = spatial_index
        .points_within_radius(anchor.latitude, anchor.longitude, radius_km)
        .into_iter()
        .filter(|(point, _)| point.entity_type.eq_ignore_ascii_case("society"))
        .collect::<Vec<_>>();
    let mut ids = Vec::new();
    let mut distances = HashMap::new();
    for property in properties.iter().filter(|property| property.is_listable()) {
        let society_name = society_names
            .get(&property.society_id)
            .map(String::as_str)
            .unwrap_or_default();
        let Some((_, distance_km)) = nearby_societies.iter().find(|(point, _)| {
            point.name.eq_ignore_ascii_case(society_name)
                || point
                    .entity_id
                    .eq_ignore_ascii_case(&society_node_id(&property.society_id))
        }) else {
            continue;
        };
        if !ids.iter().any(|id| id == &property.id) {
            ids.push(property.id.clone());
        }
        distances.insert(property.id.clone(), *distance_km);
    }
    (ids, distances)
}

fn remove_bhk_preference_signals(intent: &mut SearchIntent, relaxed_bhk: u32) {
    let mentions_relaxed_bhk = |value: &str| {
        parser::parse_query_slots(value)
            .bhk
            .is_some_and(|constraint| constraint.value == relaxed_bhk)
    };
    intent
        .preferences
        .retain(|preference| !mentions_relaxed_bhk(preference));
    intent
        .positive_preferences
        .retain(|preference| !mentions_relaxed_bhk(&preference.raw_text));
    intent
        .negative_preferences
        .retain(|preference| !mentions_relaxed_bhk(&preference.raw_text));
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
                    "{}: {} -> {} ({})",
                    relaxation.kind, relaxation.from, relaxation.to, relaxation.reason.code
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

fn apply_resolved_constraints(
    mut intent: SearchIntent,
    resolved_entities: &[ResolvedSearchEntity],
) -> SearchIntent {
    for entity in resolved_entities {
        if !entity.entity_type.eq_ignore_ascii_case("area") {
            continue;
        }
        if entity.polarity == "exclusion" {
            push_unique_string(&mut intent.excluded_areas, &entity.name);
            if intent
                .area
                .as_deref()
                .is_some_and(|area| area.eq_ignore_ascii_case(&entity.name))
            {
                intent.area = None;
            }
        } else if intent.area.is_none() {
            intent.area = Some(entity.name.clone());
        }
    }
    intent
}

fn resolve_serving_query_entities(
    query: &str,
    intent: &SearchIntent,
    serving_bundle: Option<&LoadedServingBundle>,
    properties: &[Property],
) -> Vec<ResolvedSearchEntity> {
    let mut entities = resolve_runtime_area_query_entities(query, intent, properties);
    let Some(bundle) = serving_bundle else {
        return entities;
    };
    for entity in resolve_serving_query_entities_from_records(query, intent, &bundle.entities) {
        push_resolved(&mut entities, entity);
    }
    remove_entities_only_mentioned_inside_longer_match(query, &mut entities);
    entities.truncate(DIAGNOSTIC_ID_LIMIT);
    entities
}

fn remove_entities_only_mentioned_inside_longer_match(
    query: &str,
    entities: &mut Vec<ResolvedSearchEntity>,
) {
    let query_lower = query.to_ascii_lowercase();
    let entity_ranges = entities
        .iter()
        .map(|entity| exact_entity_match_ranges(&query_lower, &entity.name))
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
                            && other.name.len() > candidate.name.len()
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
    resolved_entities: &[ResolvedSearchEntity],
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
) -> Option<String> {
    let query_lower = query.to_ascii_lowercase();
    let parsed_slots = parser::parse_query_slots(query);
    let budget_start = parsed_slots.budget_max.as_ref().and_then(|budget| {
        exact_entity_match_ranges(&query_lower, &budget.raw_text)
            .into_iter()
            .map(|(start, _)| start)
            .next()
    });
    let relation_range = parsed_slots.relations.first().and_then(|relation| {
        exact_entity_match_ranges(&query_lower, &relation.raw_text)
            .into_iter()
            .next()
    });

    for prefix in &search_resolution_config().named_entity_scope_prefixes {
        for (_, prefix_end) in scope_prefix_match_ranges(&query_lower, prefix) {
            if relation_range.is_some_and(|(relation_start, _)| prefix_end > relation_start) {
                continue;
            }
            let clause_end = [budget_start, relation_range.map(|(start, _)| start)]
                .into_iter()
                .flatten()
                .filter(|end| *end > prefix_end)
                .min()
                .unwrap_or(query.len());
            if unresolved_clause(query, prefix_end, clause_end, resolved_entities, false) {
                return Some(query[prefix_end..clause_end].trim().to_string());
            }
        }
    }

    if let Some((_, relation_end)) = relation_range {
        let clause_end = budget_start
            .filter(|start| *start > relation_end)
            .unwrap_or(query.len());
        if geo_query.is_none()
            && unresolved_clause(query, relation_end, clause_end, resolved_entities, true)
        {
            return Some(query[relation_end..clause_end].trim().to_string());
        }
    }

    None
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

fn scope_prefix_match_ranges(query_lower: &str, prefix: &str) -> Vec<(usize, usize)> {
    exact_entity_match_ranges(query_lower, prefix)
        .into_iter()
        .filter(|(start, end)| {
            query_lower[..*start].chars().next_back() != Some('-')
                && query_lower[*end..].chars().next() != Some('-')
        })
        .collect()
}

fn unresolved_clause(
    query: &str,
    start: usize,
    end: usize,
    resolved_entities: &[ResolvedSearchEntity],
    allow_place_family: bool,
) -> bool {
    let clause = query[start..end].trim();
    if clause.is_empty()
        || !clause
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    if !allow_place_family && is_generic_scope_clause(clause) {
        return false;
    }
    let query_lower = query.to_ascii_lowercase();
    let has_resolved_entity = resolved_entities.iter().any(|entity| {
        exact_entity_match_ranges(&query_lower, &entity.name)
            .iter()
            .any(|(entity_start, entity_end)| *entity_start >= start && *entity_end <= end)
    });
    if has_resolved_entity {
        return false;
    }
    if allow_place_family
        && search_resolution_config()
            .place_families
            .iter()
            .flat_map(|family| family.aliases.iter())
            .any(|alias| {
                exact_entity_match_ranges(&query_lower, alias)
                    .iter()
                    .any(|(alias_start, alias_end)| *alias_start >= start && *alias_end <= end)
            })
    {
        return false;
    }
    true
}

fn is_generic_scope_clause(clause: &str) -> bool {
    let config = search_resolution_config();
    parser::query_tokens(clause)
        .into_iter()
        .find(|token| {
            !config
                .ignored_entity_names
                .iter()
                .any(|ignored| ignored.eq_ignore_ascii_case(token))
        })
        .is_some_and(|token| {
            config
                .generic_scope_nouns
                .iter()
                .any(|noun| noun.eq_ignore_ascii_case(&token))
        })
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
            push_resolved(
                &mut entities,
                ResolvedSearchEntity {
                    entity_id: format!("area:{}", slug(area)),
                    entity_type: "area".to_string(),
                    name: area.to_string(),
                    match_kind: "runtime_area_name".to_string(),
                    match_source: "serving_entity".to_string(),
                    matched_text: query[start..end].to_string(),
                    polarity: polarity.to_string(),
                },
            );
            if entities.len() >= DIAGNOSTIC_ID_LIMIT {
                return entities;
            }
        }
    }

    entities
}

fn resolve_serving_query_entities_from_records(
    query: &str,
    intent: &SearchIntent,
    entities_source: &[ServingEntityRecord],
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
            push_resolved(
                &mut entities,
                ResolvedSearchEntity {
                    entity_id: entity.entity_id.clone(),
                    entity_type: entity.entity_type.clone(),
                    name: entity.name.clone(),
                    match_kind: "serving_entity_name".to_string(),
                    match_source: "serving_entity".to_string(),
                    matched_text: query[start..end].to_string(),
                    polarity: polarity.to_string(),
                },
            );
            if entities.len() >= DIAGNOSTIC_ID_LIMIT {
                return entities;
            }
        }
    }

    entities
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

    if let Some(area) = intent.area.as_deref() {
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "area_alias".to_string(),
                match_source: "parser_broad_region".to_string(),
                matched_text: area.to_string(),
                polarity: "positive".to_string(),
            },
        );
    }
    for area in &intent.excluded_areas {
        push_resolved(
            &mut entities,
            ResolvedSearchEntity {
                entity_id: format!("area:{}", slug(area)),
                entity_type: "area".to_string(),
                name: area.to_string(),
                match_kind: "area_alias".to_string(),
                match_source: "parser_broad_region".to_string(),
                matched_text: area.to_string(),
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

fn candidate_property_indexes(
    candidate_ids: &[String],
    property_by_id: Option<&HashMap<String, usize>>,
) -> Option<Vec<usize>> {
    let property_by_id = property_by_id?;
    let mut indexes = Vec::new();
    for id in candidate_ids {
        let Some(index) = property_by_id.get(id).copied() else {
            continue;
        };
        if !indexes.iter().any(|existing| *existing == index) {
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
    use chrono::{TimeZone, Utc};

    use crate::dag_config::SearchResolutionConfig;
    use crate::knowledge::FactValue;
    use crate::search::intent::{Polarity, PreferenceSignal, SearchIntent};
    use crate::serving::{ServingEntityRecord, ServingFactIndex, ServingFactRecord};

    use super::*;

    fn intent_without_soft_signals() -> SearchIntent {
        SearchIntent {
            area: Some("Whitefield".to_string()),
            excluded_areas: Vec::new(),
            bhk: Some(3),
            budget_max: Some(25_000_000),
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        }
    }

    fn empty_intent() -> SearchIntent {
        SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            bhk: None,
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

    fn run_relaxation_for_test(
        query: &str,
        intent: &SearchIntent,
        properties: &[Property],
    ) -> (
        Option<(Vec<SearchResultCard>, Vec<SearchRelaxation>)>,
        Vec<SearchEvidenceGap>,
    ) {
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
        .run_relaxation_sequence(
            query,
            intent,
            None,
            Some(&property_by_id),
            None,
            None,
            None,
            &[],
            Vec::new(),
        )
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
    fn resolver_rejects_junk_tiny_entity_names() {
        let config = SearchResolutionConfig {
            min_resolvable_entity_name_chars: 3,
            ignored_entity_names: vec!["a".to_string(), "in".to_string()],
            resolvable_entity_types: Vec::new(),
            named_entity_scope_prefixes: Vec::new(),
            generic_scope_nouns: Vec::new(),
            exclusion_prefixes: Vec::new(),
            place_families: Vec::new(),
        };

        assert!(!is_resolvable_entity_name("a", &config));
        assert!(!is_resolvable_entity_name("in", &config));
        assert!(!is_resolvable_entity_name("  ", &config));
        assert!(is_resolvable_entity_name("Forum", &config));
        assert!(is_resolvable_entity_name("DSR", &config));
    }

    #[test]
    fn active_relaxation_policy_does_not_weaken_bhk() {
        let mut intent = intent_without_soft_signals();
        intent.bhk = Some(2);
        intent.budget_max = Some(5_000_000);
        intent.positive_preferences.push(PreferenceSignal {
            raw_text: "2bhk configuration".to_string(),
            polarity: Polarity::Positive,
            expanded_keys: vec!["has_2bhk".to_string()],
            gap_keys: Vec::new(),
            weight: 1.0,
            missing_evidence_neutral: false,
        });
        let properties = vec![test_property("exact-area-home", "Whitefield")];

        let (matched, gaps) = run_relaxation_for_test("Whitefield", &intent, &properties);
        assert!(matched.is_none());
        assert!(gaps
            .iter()
            .all(|gap| ["geo.latitude", "geo.longitude"].contains(&gap.missing_fact.as_str())));
    }

    #[test]
    fn budget_relaxation_accumulates_unique_results_across_bands() {
        let mut intent = intent_without_soft_signals();
        intent.area = None;
        intent.bhk = None;
        intent.budget_max = Some(100_000_000);
        let properties = [
            ("ten-percent", 105_000_000),
            ("twenty-five-percent", 120_000_000),
            ("fifty-percent", 140_000_000),
        ]
        .into_iter()
        .map(|(id, price)| {
            let mut property = test_property(id, "Whitefield");
            property.price = price;
            property
        })
        .collect::<Vec<_>>();

        let (matched, gaps) = run_relaxation_for_test("homes", &intent, &properties);
        let (results, applied) = matched.expect("three budget bands should fill the target");

        assert_eq!(
            results
                .iter()
                .map(|result| result.card.id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["ten-percent", "twenty-five-percent", "fifty-percent"])
        );
        assert_eq!(results.len(), 3);
        assert_eq!(
            applied
                .iter()
                .map(|relaxation| relaxation.to.as_str())
                .collect::<Vec<_>>(),
            ["11.00Cr", "12.50Cr", "15.00Cr"]
        );
        for result in &results {
            let relaxation_reason_count = result
                .match_explanation
                .as_ref()
                .expect("relaxed result should explain its relaxation")
                .reasons
                .iter()
                .filter(|reason| reason.fact_key == "search.constraint_relaxation")
                .count();
            assert_eq!(relaxation_reason_count, 1);
        }
        assert!(gaps.is_empty());
    }

    #[test]
    fn exact_results_are_kept_while_relaxation_fills_the_target() {
        let properties = [
            ("strict", 100_000_000),
            ("relaxed-one", 105_000_000),
            ("relaxed-two", 120_000_000),
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

        assert_eq!(output.results.len(), 3);
        assert_eq!(output.results[0].card.id, "strict");
        assert!(output.results[0]
            .match_explanation
            .as_ref()
            .is_none_or(|explanation| explanation
                .reasons
                .iter()
                .all(|reason| reason.fact_key != "search.constraint_relaxation")));
        assert!(output
            .results
            .iter()
            .skip(1)
            .all(|result| result.match_reason.contains("Relaxed budget")));
        assert!(!output
            .results
            .iter()
            .any(|result| result.card.id == "beyond-target"));
    }

    #[test]
    fn removing_budget_cap_does_not_admit_unknown_prices() {
        let mut intent = intent_without_soft_signals();
        intent.area = None;
        intent.bhk = None;
        intent.budget_max = Some(100_000_000);
        let mut property = test_property("unknown-price", "Whitefield");
        property.price = 0;

        let (matched, gaps) = run_relaxation_for_test("homes", &intent, &[property]);

        assert!(matched.is_none());
        assert!(gaps.is_empty());
    }

    #[test]
    fn missing_area_coordinates_abstain_and_emit_coordinate_gaps() {
        let mut intent = empty_intent();
        intent.area = Some("Whitefield".to_string());
        let properties = vec![test_property("distant-home", "Indiranagar")];

        let (matched, gaps) = run_relaxation_for_test("Whitefield", &intent, &properties);

        assert!(matched.is_none());
        assert_eq!(
            gaps.iter()
                .map(|gap| gap.missing_fact.as_str())
                .collect::<Vec<_>>(),
            vec!["geo.latitude", "geo.longitude"]
        );
        assert!(gaps.iter().all(|gap| gap.entity_id == "area:whitefield"));
    }

    #[test]
    fn area_radius_candidates_follow_policy_radii_and_exclude_distant_societies() {
        let entities = vec![
            serving_entity("area:anchor", "area", "Anchor"),
            serving_entity("society:near", "society", "Near"),
            serving_entity("society:mid", "society", "Mid"),
            serving_entity("society:wide", "society", "Wide"),
            serving_entity("society:distant", "society", "Distant"),
        ];
        let coordinates = [
            ("area:anchor", 12.98),
            ("society:near", 12.99),
            ("society:mid", 13.01),
            ("society:wide", 13.06),
            ("society:distant", 13.10),
        ];
        let facts = coordinates
            .into_iter()
            .flat_map(|(entity_id, latitude)| {
                [
                    coordinate_fact(entity_id, "geo.latitude", latitude),
                    coordinate_fact(entity_id, "geo.longitude", 77.75),
                ]
            })
            .collect::<Vec<_>>();
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        let anchor = spatial_index
            .point_for_entity("area:anchor")
            .expect("area anchor should be indexed");
        let properties = ["near", "mid", "wide", "distant"]
            .into_iter()
            .map(|id| {
                let mut property = test_property(id, "Elsewhere");
                property.society_id = id.to_string();
                property
            })
            .collect::<Vec<_>>();
        let society_names = HashMap::new();

        let ids_by_radius = schema::ranking_policy()
            .constraint_relaxation
            .area_radii_km
            .iter()
            .map(|radius| {
                area_radius_candidate_ids(
                    &properties,
                    &society_names,
                    Some(&spatial_index),
                    anchor,
                    *radius,
                )
                .0
            })
            .collect::<Vec<_>>();

        assert_eq!(
            schema::ranking_policy().constraint_relaxation.area_radii_km,
            vec![2.0, 5.0, 10.0]
        );
        assert_eq!(ids_by_radius[0], vec!["near"]);
        assert_eq!(ids_by_radius[1], vec!["near", "mid"]);
        assert_eq!(ids_by_radius[2], vec!["near", "mid", "wide"]);
        assert!(ids_by_radius
            .iter()
            .all(|ids| !ids.iter().any(|id| id == "distant")));

        let relaxation = area_radius_relaxation("Anchor", anchor, "Anchor", 2.0, Some(1.1));
        assert_eq!(relaxation.radius_km, Some(2.0));
        assert_eq!(relaxation.candidate_distance_km, Some(1.1));
        assert_eq!(
            relaxation
                .area_anchor
                .as_ref()
                .map(|area| area.entity_id.as_str()),
            Some("area:anchor")
        );
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
            unresolved_named_entity_clause("3BHK in Atlantis Enclave", &[], None),
            Some("Atlantis Enclave".to_string())
        );
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
            unresolved_named_entity_clause("3BHK in Whitefield under 2cr", &resolved, None),
            None
        );
    }

    #[test]
    fn unsupported_proximity_family_abstains_without_stealing_generic_suffix() {
        assert_eq!(
            unresolved_named_entity_clause("3BHK near a police station", &[], None),
            Some("a police station".to_string())
        );
        assert_eq!(
            unresolved_named_entity_clause("3BHK near metro", &[], None),
            None
        );
    }

    #[test]
    fn unresolved_secondary_personal_anchor_records_gap_without_discarding_resolved_clauses() {
        let entities = vec![serving_entity("area:whitefield", "area", "Whitefield")];
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

        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].missing_fact, "geo.proximity_anchor");
        assert!(gaps[0].reason.contains("my wife office in marathahalli"));
        assert_eq!(
            unresolved_named_entity_clause(query, &resolved, Some(&geo_query)),
            None
        );
    }

    #[test]
    fn hyphenated_status_words_do_not_create_named_entity_scopes() {
        assert_eq!(
            unresolved_named_entity_clause(
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
            unresolved_named_entity_clause(
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
            unresolved_named_entity_clause(
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
}
