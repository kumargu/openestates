use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::dag_config::{
    nearby_place_category_for_fact_key, requested_nearby_place_categories,
    search_resolution_config, ui_surfaces_config,
};
use crate::knowledge::node::RootSource;
use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::models::{KgEntityRefs, Property, Society};
use crate::proof_focus::ProofFocus;
use crate::routes::enrichment::{
    area_node_id, enrich_property_card, property_node_id, society_node_id,
};
use crate::routes::search::graph_preference_score_for_keys;
use crate::serving::{
    GoogleReviewEvidence, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
    SocietyFactProjection,
};

use super::analyzer;
use super::geo;
use super::index::{price_satisfies_budget, SearchIndex};
use super::intent::{ConstraintOperator, HardConstraint, SearchIntent};
use super::resolver::{is_resolvable_entity_name, query_contains_lower_text};
use super::schema::{self, NumericConstraintSchema, NumericEvidenceSchema, TextEvidenceSchema};
use super::{
    ConfidenceComponent, ConfidenceScore, MatchExplanation, MatchReason, PreferenceCoverage,
    SearchResultCard,
};

/// Simple text-matching search engine.
///
/// Designed to be swappable with a vector search backend later — the interface
/// (query in, scored results out) stays the same.
pub struct TextSearch;

const FIELD_ONLY_STRUCTURED_RESULT_LIMIT: usize = 400;

impl TextSearch {
    /// Intent-based search: filters by hard constraints, scores by relevance,
    /// and returns full PropertyCard data with match info.
    ///
    /// When `graph` is provided, preference scoring uses the graph's self-describing
    /// `answers_preferences` + `scoring_hint` metadata. Falls back to hardcoded
    /// scoring when the graph doesn't have relevant facts.
    #[allow(dead_code)] // Convenience wrapper used by tests — prod code calls search_with_intent.
    pub fn search_with_intent(
        properties: &[Property],
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_and_intent(
            properties,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
        )
    }

    /// Indexed local recall followed by deterministic ranking and explanation.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_and_intent(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_and_extra_recall_and_intent(
            properties,
            search_index,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
        )
    }

    /// Indexed local recall plus optional serving-bundle recall, followed by deterministic ranking.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_and_extra_recall_and_intent(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_extra_recall_serving_facts_and_intent(
            properties,
            search_index,
            extra_candidate_ids,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
        )
    }

    /// Indexed local recall plus optional serving-bundle recall/facts.
    ///
    /// The in-memory KG remains the first ranking source. Serving facts are a
    /// read-optimized overlay for recently materialized DAG facts that have not
    /// yet been folded back into per-entity KG JSON files.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_extra_recall_serving_facts_and_intent(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        serving_facts: Option<&ServingFactIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_extra_recall_geo_serving_facts_and_intent(
            properties,
            search_index,
            extra_candidate_ids,
            None,
            serving_facts,
            society_names,
            societies,
            query,
            intent,
            graph,
        )
    }

    /// Indexed local/Tantivy/geo recall plus deterministic fact-first ranking.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_extra_recall_geo_serving_facts_and_intent(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
        serving_facts: Option<&ServingFactIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        let merged_ids = merged_candidate_ids(
            search_index.map(|index| index.recall_ids(query, intent)),
            extra_candidate_ids,
        );
        let candidate_property_indexes = search_index.and_then(|index| {
            merged_ids
                .as_deref()
                .and_then(|ids| {
                    let indexes = index.property_indexes_for_ids(ids);
                    (indexes.len() == ids.len()).then_some(indexes)
                })
                .filter(|indexes| !indexes.is_empty())
        });
        Self::search_with_candidate_property_indexes_serving_facts_and_intent(
            properties,
            search_index,
            merged_ids.as_deref(),
            candidate_property_indexes,
            geo_query,
            serving_facts,
            society_names,
            societies,
            query,
            intent,
            graph,
        )
    }

    /// Candidate-index ranking entry point for the live snapshot path.
    ///
    /// When `candidate_property_indexes` is present, ranking iterates only those
    /// corpus indexes in caller-provided order and keeps the original corpus
    /// ordinal for final tie-breaking.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_candidate_property_indexes_serving_facts_and_intent(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        candidate_property_indexes: Option<Vec<usize>>,
        geo_query: Option<&geo::GeoSearchQuery<'_>>,
        serving_facts: Option<&ServingFactIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        if !intent.unsupported_inventory_types.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let terms = scoring_query_terms(&query_lower);
        let structured_terms = structured_intent_terms(intent);
        let geo_place_terms = geo_query
            .map(|query| query.resolved_place_terms())
            .unwrap_or_default();
        let scoring_terms = terms
            .iter()
            .map(String::as_str)
            .filter(|term| !structured_terms.iter().any(|structured| structured == term))
            .filter(|term| !geo_place_terms.iter().any(|place_term| place_term == term))
            .collect::<Vec<_>>();
        let positive_preferences = positive_preference_labels(intent);
        let negative_preferences = negative_preference_labels(intent);
        let prefix_terms = if scoring_terms.is_empty() {
            Vec::new()
        } else {
            terms.iter().map(String::as_str).collect::<Vec<_>>()
        };
        let has_geo_query = geo_query.is_some_and(|query| !query.is_empty());
        let has_explainable_signals = !positive_preferences.is_empty()
            || !negative_preferences.is_empty()
            || !intent.hard_constraints.is_empty()
            || has_geo_query;
        let proof_focus_targets = proof_focus_targets();
        let candidate_ids = if candidate_property_indexes.is_some() {
            None
        } else {
            merged_candidate_ids(
                search_index.map(|index| index.recall_ids(query, intent)),
                extra_candidate_ids,
            )
        };
        let mut candidate_properties = candidate_property_refs(
            properties,
            candidate_property_indexes.as_deref(),
            candidate_ids.as_deref(),
        );
        if should_limit_field_only_structured_results(intent, geo_query, serving_facts, graph) {
            candidate_properties.truncate(FIELD_ONLY_STRUCTURED_RESULT_LIMIT);
        }

        let mut results: Vec<RankedSearchResult> = candidate_properties
            .into_iter()
            .filter_map(|(ordinal, p)| {
                if !p.is_listable() {
                    return None;
                }

                if intent
                    .excluded_areas
                    .iter()
                    .any(|area| property_matches_area(p, area, graph))
                {
                    return None;
                }

                // Hard constraint: BHK
                if let Some(bhk) = intent.bhk {
                    if p.bhk != bhk {
                        return None;
                    }
                }

                // Hard constraint: budget
                if let Some(budget_max) = intent.budget_max {
                    if !price_satisfies_budget(p.price, budget_max) {
                        return None;
                    }
                }

                let named_place_evidence = geo_query
                    .map(|query| {
                        let mut evidence = serving_facts
                            .map(|facts| serving_named_place_evidence(facts, &p.society_id, query))
                            .unwrap_or_default();
                        for fallback in query.evidence_for_society(&p.society_id) {
                            if !evidence.iter().any(|existing| {
                                existing.place_entity_id == fallback.place_entity_id
                            }) {
                                evidence.push(fallback.into());
                            }
                        }
                        evidence
                    })
                    .unwrap_or_default();

                // Soft constraint: area — exact match keeps full score,
                // nearby/sub-area match gets a penalty instead of exclusion.
                let (area_penalty, area_match_kind): (f64, Option<AreaMatchKind>) =
                    if let Some(ref area) = intent.area {
                        if p.area.eq_ignore_ascii_case(area) {
                            (0.0, Some(AreaMatchKind::Exact))
                        } else if area_is_nearby(&p.area, area) {
                            (
                                schema::ranking_policy().nearby_area_score_penalty,
                                Some(AreaMatchKind::Nearby),
                            )
                        } else if graph_area_match(&p.society_id, area, graph) {
                            (
                                schema::ranking_policy().graph_area_score_penalty,
                                Some(AreaMatchKind::Graph),
                            )
                        } else {
                            return None; // unrelated area: exclude
                        }
                    } else {
                        (0.0, None)
                    };

                let hard_constraint_matches =
                    match_hard_constraints(intent, graph, serving_facts, &p.society_id)?;

                let society_name = society_names
                    .get(&p.society_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let named_society_match =
                    query_mentions_resolvable_society(&query_lower, society_name);
                let name_prefix_score =
                    query_name_prefix_score(&prefix_terms, &p.title, society_name);

                // Base text score
                let (mut score, mut reasons) = if terms.is_empty() {
                    (1.0, Vec::new())
                } else {
                    score_property(p, society_name, &scoring_terms)
                };
                score += area_penalty;

                // Boost for preference alignment — collect structured reasons
                let mut match_reasons: Vec<MatchReason> = Vec::new();
                let mut proof_focuses: Vec<ProofFocus> = Vec::new();
                let mut pref_coverage: Vec<PreferenceCoverage> = Vec::new();
                let mut graph_count: usize = 0;
                let mut total_facts_consulted: usize = 0;
                let mut positive_evidence_score = 0.0;
                let mut primary_intent_score: f64 = 0.0;
                let mut best_fact_key_rank = usize::MAX;

                for evidence in hard_constraint_matches {
                    best_fact_key_rank = best_fact_key_rank.min(evidence.fact_key_rank);
                    total_facts_consulted += 1;
                    graph_count += 1;
                    score += evidence.score_delta;
                    primary_intent_score += evidence_intent_score(&evidence);
                    reasons.push(evidence.reason.clone());
                    match_reasons.push(MatchReason {
                        preference: evidence.preference.clone(),
                        fact_key: evidence.fact_key.clone(),
                        display: evidence.display.clone(),
                        score: evidence.normalized_score,
                        confidence: evidence.confidence,
                        source_type: evidence.source_type.clone(),
                        scoring_method: evidence.scoring_method.clone(),
                    });
                    push_proof_focus(
                        &mut proof_focuses,
                        &proof_focus_targets,
                        ProofFocusCandidate {
                            fact_key: &evidence.fact_key,
                            matched_label: None,
                            matched_value: Some(&evidence.display),
                            requested_constraint: Some(&evidence.preference),
                            entity_id: None,
                            distance_m: None,
                            reason: &evidence.reason,
                        },
                    );
                    pref_coverage.push(PreferenceCoverage {
                        preference: evidence.preference,
                        status: "matched".into(),
                        fact_key: Some(evidence.fact_key),
                    });
                }

                let named_place_fact_keys = named_place_evidence
                    .iter()
                    .map(|evidence| evidence.fact_key.clone())
                    .collect::<Vec<_>>();
                for evidence in named_place_evidence {
                    total_facts_consulted += 1;
                    graph_count += 1;
                    score += evidence.score_delta;
                    positive_evidence_score += evidence.score_delta.max(0.0);
                    primary_intent_score += named_place_intent_score(&evidence);
                    let preference = format!("near {}", evidence.place_name);
                    let focus_reason = format!("matched {}", preference);
                    reasons.push(format!("{}: {}", preference, evidence.display));
                    match_reasons.push(MatchReason {
                        preference: preference.clone(),
                        fact_key: evidence.fact_key.clone(),
                        display: evidence.display.clone(),
                        score: evidence.normalized_score,
                        confidence: evidence.confidence,
                        source_type: evidence.source_type.clone(),
                        scoring_method: evidence.scoring_method.clone(),
                    });
                    push_proof_focus(
                        &mut proof_focuses,
                        &proof_focus_targets,
                        ProofFocusCandidate {
                            fact_key: &evidence.fact_key,
                            matched_label: Some(&evidence.place_name),
                            matched_value: Some(&evidence.display),
                            requested_constraint: Some(&preference),
                            entity_id: Some(&evidence.place_entity_id),
                            distance_m: distance_m(evidence.distance_km),
                            reason: &focus_reason,
                        },
                    );
                    pref_coverage.push(PreferenceCoverage {
                        preference,
                        status: if evidence.normalized_score > 0.5 {
                            "matched"
                        } else {
                            "partial"
                        }
                        .to_string(),
                        fact_key: Some(evidence.fact_key),
                    });
                }

                for pref in &positive_preferences {
                    let candidate_fact_keys = positive_preference_keys(intent, pref);
                    if positive_preference_covered_by_named_place(
                        &candidate_fact_keys,
                        &named_place_fact_keys,
                    ) {
                        continue;
                    }
                    // Graph-first: check if the society's facts declare scoring for this preference
                    if let Some(g) = graph {
                        if let Some((gs, detail)) = graph_preference_score_for_keys(
                            g,
                            &p.society_id,
                            pref,
                            &candidate_fact_keys,
                        ) {
                            if !evidence_is_confident_enough(
                                &detail.source_type,
                                detail.confidence,
                                "graph",
                            ) {
                                continue;
                            }
                            total_facts_consulted += 1;
                            score += gs;
                            positive_evidence_score += gs.max(0.0);
                            best_fact_key_rank = best_fact_key_rank.min(candidate_fact_key_rank(
                                &candidate_fact_keys,
                                &detail.fact_key,
                            ));
                            reasons.push(format!("matches preference: {}", pref));

                            // Normalize score to 0-1 range (graph scores are 0-2)
                            let norm_score = (gs / 2.0).min(1.0);
                            primary_intent_score += norm_score * f64::from(detail.confidence);
                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: detail.fact_key.clone(),
                                display: detail.display.clone(),
                                score: norm_score,
                                confidence: detail.confidence,
                                source_type: detail.source_type.clone(),
                                scoring_method: "graph".into(),
                            });
                            let focus_reason = format!("matches preference: {}", pref);
                            push_proof_focus(
                                &mut proof_focuses,
                                &proof_focus_targets,
                                ProofFocusCandidate {
                                    fact_key: &detail.fact_key,
                                    matched_label: None,
                                    matched_value: Some(&detail.display),
                                    requested_constraint: Some(pref),
                                    entity_id: None,
                                    distance_m: None,
                                    reason: &focus_reason,
                                },
                            );
                            pref_coverage.push(PreferenceCoverage {
                                preference: pref.clone(),
                                status: if norm_score > 0.5 {
                                    "matched"
                                } else {
                                    "partial"
                                }
                                .into(),
                                fact_key: Some(detail.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    if let Some(serving_facts) = serving_facts {
                        if let Some(evidence) = serving_preference_evidence(
                            serving_facts,
                            &p.society_id,
                            pref,
                            &candidate_fact_keys,
                            &query_lower,
                        ) {
                            best_fact_key_rank = best_fact_key_rank.min(evidence.fact_key_rank);
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            positive_evidence_score += evidence.score_delta.max(0.0);
                            primary_intent_score += evidence_intent_score(&evidence);
                            reasons.push(evidence.reason.clone());

                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display.clone(),
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type.clone(),
                                scoring_method: evidence.scoring_method.clone(),
                            });
                            push_proof_focus(
                                &mut proof_focuses,
                                &proof_focus_targets,
                                ProofFocusCandidate {
                                    fact_key: &evidence.fact_key,
                                    matched_label: None,
                                    matched_value: Some(&evidence.display),
                                    requested_constraint: Some(pref),
                                    entity_id: None,
                                    distance_m: None,
                                    reason: &evidence.reason,
                                },
                            );
                            pref_coverage.push(PreferenceCoverage {
                                preference: pref.clone(),
                                status: if evidence.normalized_score > 0.5 {
                                    "matched"
                                } else {
                                    "partial"
                                }
                                .into(),
                                fact_key: Some(evidence.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    if let Some(g) = graph {
                        if let Some(evidence) = graph_textual_preference_evidence(
                            g,
                            &p.society_id,
                            pref,
                            &candidate_fact_keys,
                        ) {
                            best_fact_key_rank = best_fact_key_rank.min(evidence.fact_key_rank);
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            positive_evidence_score += evidence.score_delta.max(0.0);
                            primary_intent_score += evidence_intent_score(&evidence);
                            reasons.push(evidence.reason.clone());

                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display.clone(),
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type.clone(),
                                scoring_method: evidence.scoring_method.clone(),
                            });
                            push_proof_focus(
                                &mut proof_focuses,
                                &proof_focus_targets,
                                ProofFocusCandidate {
                                    fact_key: &evidence.fact_key,
                                    matched_label: None,
                                    matched_value: Some(&evidence.display),
                                    requested_constraint: Some(pref),
                                    entity_id: None,
                                    distance_m: None,
                                    reason: &evidence.reason,
                                },
                            );
                            pref_coverage.push(PreferenceCoverage {
                                preference: pref.clone(),
                                status: if evidence.normalized_score > 0.5 {
                                    "matched"
                                } else {
                                    "partial"
                                }
                                .into(),
                                fact_key: Some(evidence.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    pref_coverage.push(PreferenceCoverage {
                        preference: pref.clone(),
                        status: "no_data".into(),
                        fact_key: None,
                    });
                }

                for pref in &negative_preferences {
                    let candidate_fact_keys = negative_preference_keys(intent, pref);
                    if let Some(serving_facts) = serving_facts {
                        if let Some(evidence) = serving_negative_preference_evidence(
                            serving_facts,
                            &p.society_id,
                            pref,
                            &candidate_fact_keys,
                        ) {
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            reasons.push(evidence.reason.clone());
                            let coverage_status = negative_coverage_status(&evidence);

                            match_reasons.push(MatchReason {
                                preference: format!("avoid {}", pref),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display.clone(),
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type.clone(),
                                scoring_method: evidence.scoring_method.clone(),
                            });
                            let requested_constraint = format!("avoid {}", pref);
                            push_proof_focus(
                                &mut proof_focuses,
                                &proof_focus_targets,
                                ProofFocusCandidate {
                                    fact_key: &evidence.fact_key,
                                    matched_label: None,
                                    matched_value: Some(&evidence.display),
                                    requested_constraint: Some(&requested_constraint),
                                    entity_id: None,
                                    distance_m: None,
                                    reason: &evidence.reason,
                                },
                            );
                            pref_coverage.push(PreferenceCoverage {
                                preference: requested_constraint,
                                status: coverage_status.to_string(),
                                fact_key: Some(evidence.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    if let Some(g) = graph {
                        if let Some(evidence) = graph_negative_preference_evidence(
                            g,
                            &p.society_id,
                            pref,
                            &candidate_fact_keys,
                        ) {
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            reasons.push(evidence.reason.clone());
                            let coverage_status = negative_coverage_status(&evidence);

                            match_reasons.push(MatchReason {
                                preference: format!("avoid {}", pref),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display.clone(),
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type.clone(),
                                scoring_method: evidence.scoring_method.clone(),
                            });
                            let requested_constraint = format!("avoid {}", pref);
                            push_proof_focus(
                                &mut proof_focuses,
                                &proof_focus_targets,
                                ProofFocusCandidate {
                                    fact_key: &evidence.fact_key,
                                    matched_label: None,
                                    matched_value: Some(&evidence.display),
                                    requested_constraint: Some(&requested_constraint),
                                    entity_id: None,
                                    distance_m: None,
                                    reason: &evidence.reason,
                                },
                            );
                            pref_coverage.push(PreferenceCoverage {
                                preference: requested_constraint,
                                status: coverage_status.to_string(),
                                fact_key: Some(evidence.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    score -= negative_no_data_penalty(intent, pref);
                    pref_coverage.push(PreferenceCoverage {
                        preference: format!("avoid {}", pref),
                        status: "no_data".into(),
                        fact_key: None,
                    });
                }

                // Build explanation only when the query asked for constraints/preferences.
                let has_positive_evidence =
                    has_positive_preference_evidence(&pref_coverage, &positive_preferences);
                let match_explanation = if has_explainable_signals {
                    let graph_pct = if graph_count > 0 { 100.0 } else { 0.0 };
                    Some(MatchExplanation {
                        reasons: match_reasons,
                        preference_coverage: pref_coverage,
                        graph_driven_pct: graph_pct,
                        total_facts_consulted,
                    })
                } else {
                    None
                };

                // Structured preference queries should still return local candidates
                // with no-data coverage instead of disappearing behind a penalty.
                let has_constraints = intent.area.is_some()
                    || intent.bhk.is_some()
                    || intent.budget_max.is_some()
                    || !intent.hard_constraints.is_empty();
                let has_preferences =
                    !positive_preferences.is_empty() || !negative_preferences.is_empty();
                if score <= 0.0 && (has_constraints || has_preferences) {
                    if has_preferences {
                        score =
                            score.max(minimum_evidence_floor(positive_evidence_score, graph_count));
                    } else {
                        score = 1.0;
                        reasons.push("matches search criteria".to_string());
                    }
                }

                if !positive_preferences.is_empty() && !has_positive_evidence {
                    score *= schema::ranking_policy().no_positive_evidence_score_multiplier;
                }

                if score <= 0.0 {
                    return None;
                }

                // Use shared enrichment — same PropertyCard as /api/properties.
                // graph is always Some in practice (search always has KG access).
                let mut card = if let Some(g) = graph {
                    enrich_property_card(p, societies, g)
                } else {
                    // Fallback without graph — build minimal card
                    crate::models::PropertyCard {
                        id: p.id.clone(),
                        kg_entity_refs: KgEntityRefs {
                            property_entity_id: property_node_id(&p.id),
                            society_entity_id: society_node_id(&p.society_id),
                            area_entity_id: area_node_id(&p.area),
                            builder_entity_id: None,
                            source_entity_ids: Vec::new(),
                        },
                        title: p.title.clone(),
                        area: p.area.clone(),
                        price: p.price,
                        price_per_sqft: p.price_per_sqft,
                        bhk: p.bhk,
                        sqft: p.carpet_area_sqft,
                        carpet_area_sqft: p.carpet_area_sqft,
                        super_builtup_sqft: p.super_builtup_sqft,
                        society_name: society_name.to_string(),
                        builder_name: p.builder_name.clone(),
                        images: p.images.clone(),
                        hero_image: p.hero_image.clone(),
                        transparency_tags: crate::routes::enrichment::compact_transparency_tags(
                            &p.transparency_tags,
                        ),
                        description_summary: p.description_summary.clone(),
                        possession_status: p.possession_status.clone(),
                        metro_distance_mins: p.metro_distance_mins,
                        floor: p.floor,
                        total_floors: p.total_floors,
                        facing: p.facing.clone(),
                        google_rating: None,
                        google_review_count: None,
                        google_reviews_url: None,
                        society_land_acres: None,
                        open_space_pct: None,
                        units_per_acre: None,
                        root_source: None,
                        project_status: None,
                        project_status_display: None,
                        home_state_display: None,
                        builder_delivery_display: None,
                        data_freshness: None,
                        floor_plan_preview_url: None,
                        plan_carpet_area_sqft: None,
                        plan_sale_area_sqft: None,
                        plan_configuration_type: None,
                        decision_labels: Vec::new(),
                        decision_check_summary: None,
                    }
                };
                if let Some(serving_facts) = serving_facts {
                    enrich_card_from_serving_facts(&mut card, serving_facts, &p.society_id);
                }
                sanitize_card_display_placeholders(&mut card);

                // Normalize score to 0.0–1.0 range (rough normalization)
                let max_possible = 15.0; // approximate ceiling
                let normalized = (score / max_possible).min(1.0);

                let evidence_strength = evidence_strength(match_explanation.as_ref());
                let mut display_score = (normalized * 100.0).round() / 100.0;
                if normalized > 0.0 && display_score == 0.0 {
                    display_score = 0.01;
                }
                let match_label = match_label_from_score_and_coverage(
                    normalized,
                    match_explanation.as_ref(),
                    has_preferences,
                );
                let match_reason = build_match_reason(intent, &p.area, area_match_kind, &reasons);

                // Compute confidence score for this result
                let gdp = match_explanation
                    .as_ref()
                    .map(|e| e.graph_driven_pct)
                    .unwrap_or(0.0);
                let confidence_score =
                    compute_confidence(graph, &p.society_id, gdp).or_else(|| {
                        serving_facts.and_then(|facts| {
                            compute_confidence_from_serving_facts(facts, &p.society_id, gdp)
                        })
                    });
                let review_quality_score = review_quality_score(&card);

                Some(RankedSearchResult {
                    ranking_score: normalized,
                    primary_intent_score,
                    best_fact_key_rank,
                    name_prefix_score,
                    evidence_strength,
                    review_quality_score,
                    named_society_match,
                    ordinal,
                    result: SearchResultCard {
                        card,
                        match_score: display_score,
                        match_label,
                        match_reason,
                        match_explanation,
                        proof_focuses,
                        confidence_score,
                    },
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.named_society_match
                .cmp(&a.named_society_match)
                .then_with(|| {
                    b.result
                        .match_score
                        .partial_cmp(&a.result.match_score)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    b.primary_intent_score
                        .partial_cmp(&a.primary_intent_score)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.best_fact_key_rank.cmp(&b.best_fact_key_rank))
                .then_with(|| b.named_society_match.cmp(&a.named_society_match))
                .then_with(|| {
                    b.name_prefix_score
                        .partial_cmp(&a.name_prefix_score)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    b.evidence_strength
                        .partial_cmp(&a.evidence_strength)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| compare_score_and_review(a, b))
                .then_with(|| a.ordinal.cmp(&b.ordinal))
        });
        results.into_iter().map(|ranked| ranked.result).collect()
    }
}

struct RankedSearchResult {
    result: SearchResultCard,
    ranking_score: f64,
    primary_intent_score: f64,
    best_fact_key_rank: usize,
    name_prefix_score: f64,
    evidence_strength: f64,
    review_quality_score: f64,
    named_society_match: bool,
    ordinal: usize,
}

fn compare_score_and_review(a: &RankedSearchResult, b: &RankedSearchResult) -> Ordering {
    b.ranking_score
        .partial_cmp(&a.ranking_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            b.review_quality_score
                .partial_cmp(&a.review_quality_score)
                .unwrap_or(Ordering::Equal)
        })
}

#[derive(Debug, Clone)]
struct ProofFocusTarget {
    surface_id: String,
    layer_id: String,
    fact_key: String,
}

struct ProofFocusCandidate<'a> {
    fact_key: &'a str,
    matched_label: Option<&'a str>,
    matched_value: Option<&'a str>,
    requested_constraint: Option<&'a str>,
    entity_id: Option<&'a str>,
    distance_m: Option<u32>,
    reason: &'a str,
}

fn proof_focus_targets() -> Vec<ProofFocusTarget> {
    let Ok(config) = ui_surfaces_config() else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for surface in &config.surfaces {
        let Some(scene) = surface.scene.as_ref() else {
            continue;
        };
        for layer in &scene.layers {
            for fact_key in layer
                .fact_keys
                .iter()
                .chain(layer.linked_entity_fact_keys.iter())
            {
                if targets.iter().any(|target: &ProofFocusTarget| {
                    target.surface_id == surface.id
                        && target.layer_id == layer.id
                        && target.fact_key.eq_ignore_ascii_case(fact_key)
                }) {
                    continue;
                }
                targets.push(ProofFocusTarget {
                    surface_id: surface.id.clone(),
                    layer_id: layer.id.clone(),
                    fact_key: fact_key.to_string(),
                });
            }
        }
    }
    targets
}

fn push_proof_focus(
    focuses: &mut Vec<ProofFocus>,
    targets: &[ProofFocusTarget],
    candidate: ProofFocusCandidate<'_>,
) {
    for target in targets
        .iter()
        .filter(|target| target.fact_key.eq_ignore_ascii_case(candidate.fact_key))
    {
        if focuses.iter().any(|existing| {
            existing.surface_id == target.surface_id
                && existing.layer_id == target.layer_id
                && existing.fact_key.eq_ignore_ascii_case(candidate.fact_key)
                && existing.entity_id.as_deref() == candidate.entity_id
                && existing.matched_label.as_deref() == candidate.matched_label
                && existing.requested_constraint.as_deref() == candidate.requested_constraint
        }) {
            continue;
        }

        focuses.push(ProofFocus {
            surface_id: target.surface_id.clone(),
            layer_id: target.layer_id.clone(),
            fact_key: candidate.fact_key.to_string(),
            entity_id: candidate.entity_id.map(str::to_string),
            feature_id: None,
            receipt_id: None,
            matched_label: candidate
                .matched_label
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            matched_value: candidate
                .matched_value
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            requested_constraint: candidate
                .requested_constraint
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            distance_m: candidate.distance_m,
            reason: candidate.reason.to_string(),
        });
    }
}

fn distance_m(distance_km: f64) -> Option<u32> {
    distance_km
        .is_finite()
        .then(|| (distance_km * 1000.0).round())
        .filter(|meters| *meters >= 0.0 && *meters <= u32::MAX as f64)
        .map(|meters| meters as u32)
}

impl From<geo::HaversineEvidence> for NamedPlaceEvidence {
    fn from(evidence: geo::HaversineEvidence) -> Self {
        Self {
            place_entity_id: evidence.place_entity_id,
            place_name: evidence.place_name,
            distance_km: evidence.distance_km,
            fact_key: geo::DISTANCE_TO_PLACE_FACT_KEY.to_string(),
            display: evidence.display,
            normalized_score: evidence.normalized_score,
            score_delta: evidence.score_delta,
            confidence: evidence.confidence,
            source_type: "Computed".to_string(),
            scoring_method: geo::HAVERSINE_SCORING_METHOD.to_string(),
        }
    }
}

pub(crate) fn enrich_card_from_serving_facts(
    card: &mut crate::models::PropertyCard,
    serving_facts: &ServingFactIndex,
    society_id: &str,
) {
    let projected = SocietyFactProjection::from_index(serving_facts, society_id)
        .project_google_reviews(GoogleReviewEvidence {
            rating: card.google_rating,
            review_count: card.google_review_count,
            reviews_url: card.google_reviews_url.clone(),
        });
    card.google_rating = projected.rating;
    card.google_review_count = projected.review_count;
    card.google_reviews_url = projected.reviews_url;

    let status_projection = SocietyFactProjection::from_index(serving_facts, society_id)
        .project_status(
            card.project_status.clone(),
            card.project_status_display.clone(),
        );
    card.project_status = status_projection.status;
    card.project_status_display = status_projection.display;
    card.home_state_display = SocietyFactProjection::from_index(serving_facts, society_id)
        .project_home_state()
        .display;
    crate::routes::enrichment::overlay_project_scale_facts(card, serving_facts, society_id);
    crate::plans::overlay_project_plans_on_card(card, society_id, Some(serving_facts));
    card.decision_labels =
        crate::decision_labels::rera_decision_labels_for_society(serving_facts, society_id);
    card.decision_check_summary =
        crate::decision_labels::rera_decision_check_summary_for_society(serving_facts, society_id);
}

fn sanitize_card_display_placeholders(card: &mut crate::models::PropertyCard) {
    clean_display_string(&mut card.possession_status);
    clean_display_string(&mut card.facing);
    clean_optional_display_string(&mut card.project_status_display);
    clean_optional_display_string(&mut card.home_state_display);
    clean_optional_display_string(&mut card.builder_delivery_display);
}

fn clean_display_string(value: &mut String) {
    if is_placeholder_display(value) {
        value.clear();
    }
}

fn clean_optional_display_string(value: &mut Option<String>) {
    if value.as_deref().is_some_and(is_placeholder_display) {
        *value = None;
    }
}

fn is_placeholder_display(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    schema::placeholder_display_values()
        .iter()
        .any(|placeholder| placeholder.eq_ignore_ascii_case(&normalized))
}

fn merged_candidate_ids(
    local_candidate_ids: Option<Vec<String>>,
    extra_candidate_ids: Option<&[String]>,
) -> Option<Vec<String>> {
    let mut merged = local_candidate_ids.unwrap_or_default();
    if let Some(extra_candidate_ids) = extra_candidate_ids {
        if should_prefer_extra_candidate_ids(merged.len(), extra_candidate_ids.len()) {
            merged.clear();
        }
        for id in extra_candidate_ids {
            if !merged.iter().any(|existing| existing == id) {
                merged.push(id.clone());
            }
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn should_limit_field_only_structured_results(
    intent: &SearchIntent,
    geo_query: Option<&geo::GeoSearchQuery<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph: Option<&KnowledgeGraph>,
) -> bool {
    graph.is_none()
        && serving_facts.is_none()
        && geo_query.is_none_or(geo::GeoSearchQuery::is_empty)
        && intent.excluded_areas.is_empty()
        && (intent.area.is_some() || intent.bhk.is_some() || intent.budget_max.is_some())
}

fn candidate_property_refs<'a>(
    properties: &'a [Property],
    candidate_property_indexes: Option<&[usize]>,
    candidate_ids: Option<&[String]>,
) -> Vec<(usize, &'a Property)> {
    if let Some(indexes) = candidate_property_indexes {
        let mut seen = HashSet::with_capacity(indexes.len());
        return indexes
            .iter()
            .filter_map(|index| {
                if !seen.insert(*index) {
                    return None;
                }
                properties.get(*index).map(|property| (*index, property))
            })
            .collect();
    }

    properties
        .iter()
        .enumerate()
        .filter(|(_, property)| {
            candidate_ids
                .map(|ids| ids.iter().any(|id| id == &property.id))
                .unwrap_or(true)
        })
        .collect()
}

fn should_prefer_extra_candidate_ids(local_len: usize, extra_len: usize) -> bool {
    let ranking = schema::ranking_policy();
    extra_len >= ranking.broad_local_recall_min_extra
        && local_len > extra_len.saturating_mul(ranking.broad_local_recall_multiplier)
}

struct EvidenceMatch {
    preference: String,
    fact_key: String,
    fact_key_rank: usize,
    display: String,
    normalized_score: f64,
    score_delta: f64,
    confidence: f32,
    source_type: String,
    scoring_method: String,
    reason: String,
}

struct NamedPlaceEvidence {
    place_entity_id: String,
    place_name: String,
    distance_km: f64,
    fact_key: String,
    display: String,
    normalized_score: f64,
    score_delta: f64,
    confidence: f32,
    source_type: String,
    scoring_method: String,
}

fn serving_named_place_evidence(
    serving_facts: &ServingFactIndex,
    society_id: &str,
    geo_query: &geo::GeoSearchQuery<'_>,
) -> Vec<NamedPlaceEvidence> {
    let node_id = society_node_id(society_id);
    let Some(rows) = serving_facts.entity(&node_id) else {
        return Vec::new();
    };
    let mut matches = Vec::new();

    for clause in geo_query.resolved_clauses() {
        let mut best: Option<NamedPlaceEvidence> = None;
        for fact in &rows.facts {
            if !evidence_is_confident_enough(
                &fact.source_type,
                fact.confidence,
                geo::NAMED_PLACE_FACT_SCORING_METHOD,
            ) {
                continue;
            }
            let metadata = rows.search_metadata_for_fact_key(&fact.fact_key).next();
            for snippet in serving_fact_text_snippets(fact) {
                for place in geo_query.places_for_clause(clause) {
                    if !geo_query.fact_key_matches_resolved_place(&fact.fact_key, place)
                        || !nearby_fact_mentions_place(&snippet, &place.name)
                    {
                        continue;
                    }
                    let Some(distance_km) =
                        distance_for_nearby_place_snippet(&snippet, &place.name)
                    else {
                        continue;
                    };
                    if geo_query
                        .clause_distance_limit_km(clause)
                        .is_some_and(|max_distance| distance_km > max_distance)
                    {
                        continue;
                    }
                    let Some(evidence) =
                        named_place_serving_fact_evidence(fact, metadata, place, distance_km)
                    else {
                        continue;
                    };
                    if best.as_ref().is_none_or(|current| {
                        evidence.score_delta > current.score_delta
                            || ((evidence.score_delta - current.score_delta).abs() < f64::EPSILON
                                && evidence.confidence > current.confidence)
                    }) {
                        best = Some(evidence);
                    }
                }
            }
        }
        if let Some(best) = best {
            matches.push(best);
        }
    }

    matches
}

fn named_place_serving_fact_evidence(
    fact: &ServingFactRecord,
    metadata: Option<&ServingSearchMetadataRecord>,
    place: &geo::ResolvedGeoPlace,
    distance_km: f64,
) -> Option<NamedPlaceEvidence> {
    if !distance_km.is_finite() || distance_km < 0.0 {
        return None;
    }
    let policy = schema::ranking_policy();
    let normalized_score = geo::normalized_distance_score(
        distance_km,
        policy.nearby_distance_full_score_km,
        policy.nearby_distance_zero_score_km,
    )?;
    if normalized_score <= 0.0 {
        return None;
    }

    let weight = metadata
        .and_then(|metadata| metadata.scoring_weight)
        .map(f64::from)
        .unwrap_or(1.0)
        .clamp(0.0, 2.0);
    let score_delta = (weight
        + normalized_score * policy.nearby_distance_bonus_cap.max(0.0)
        + normalized_score * policy.named_place_score_weight.max(0.0))
    .clamp(0.0, 3.0);
    if score_delta <= 0.0 {
        return None;
    }

    Some(NamedPlaceEvidence {
        place_entity_id: place.entity_id.clone(),
        place_name: place.name.clone(),
        distance_km,
        fact_key: fact.fact_key.clone(),
        display: format!("{distance_km:.1} km from {}", place.name),
        normalized_score,
        score_delta,
        confidence: fact.confidence.min(place.confidence),
        source_type: fact.source_type.clone(),
        scoring_method: geo::NAMED_PLACE_FACT_SCORING_METHOD.to_string(),
    })
}

fn serving_fact_text_snippets(fact: &ServingFactRecord) -> Vec<String> {
    let mut snippets = fact_text_snippets(&fact.value);
    if let Some(value_text) = fact.value_text.as_deref() {
        if !snippets.iter().any(|snippet| snippet == value_text) {
            snippets.push(value_text.to_string());
        }
    }
    snippets
}

fn nearby_fact_mentions_place(snippet: &str, place_name: &str) -> bool {
    let snippet_lower = snippet.to_ascii_lowercase();
    if query_contains_lower_text(&snippet_lower, place_name) {
        return true;
    }

    let place_tokens = named_place_identity_tokens(place_name);
    if place_tokens.is_empty() {
        return false;
    }
    let snippet_tokens = analyzer::stemmed_tokens(snippet);
    place_tokens
        .iter()
        .all(|token| snippet_tokens.iter().any(|candidate| candidate == token))
}

fn named_place_identity_tokens(place_name: &str) -> Vec<String> {
    analyzer::stemmed_tokens(place_name)
        .into_iter()
        .filter(|token| !is_nearby_place_generic_token(token))
        .collect()
}

fn is_nearby_place_generic_token(token: &str) -> bool {
    configured_named_place_generic_tokens().contains(token)
}

fn configured_named_place_generic_tokens() -> &'static HashSet<String> {
    static TOKENS: OnceLock<HashSet<String>> = OnceLock::new();
    TOKENS.get_or_init(|| {
        schema::ranking_policy()
            .named_place_generic_tokens
            .iter()
            .flat_map(|term| analyzer::stemmed_tokens(term))
            .collect()
    })
}

fn distance_for_nearby_place_snippet(snippet: &str, place_name: &str) -> Option<f64> {
    snippet
        .split(['\n', ';', '|'])
        .find_map(|segment| {
            nearby_fact_mentions_place(segment, place_name)
                .then(|| geo::extract_first_distance_km(segment))
                .flatten()
        })
        .or_else(|| geo::extract_first_distance_km(snippet))
}

fn minimum_evidence_floor(positive_evidence_score: f64, evidence_count: usize) -> f64 {
    let ranking = schema::ranking_policy();
    if positive_evidence_score > 0.0 {
        (positive_evidence_score * ranking.positive_evidence_floor_ratio).clamp(
            ranking.min_score_with_positive_evidence,
            ranking.max_score_with_positive_evidence,
        )
    } else if evidence_count > 0 {
        ranking.min_score_with_risk_only_evidence
    } else {
        ranking.min_score_with_constraint_only
    }
}

fn evidence_strength(explanation: Option<&MatchExplanation>) -> f64 {
    explanation.map_or(0.0, |explanation| {
        explanation
            .reasons
            .iter()
            .map(|reason| reason.score * f64::from(reason.confidence))
            .sum()
    })
}

fn evidence_intent_score(evidence: &EvidenceMatch) -> f64 {
    evidence.normalized_score.clamp(0.0, 1.0) * f64::from(evidence.confidence.clamp(0.0, 1.0))
}

fn named_place_intent_score(evidence: &NamedPlaceEvidence) -> f64 {
    evidence.normalized_score.clamp(0.0, 1.0) * f64::from(evidence.confidence.clamp(0.0, 1.0))
}

fn positive_preference_covered_by_named_place(
    candidate_fact_keys: &[String],
    named_place_fact_keys: &[String],
) -> bool {
    candidate_fact_keys.iter().any(|candidate| {
        named_place_fact_keys
            .iter()
            .any(|named| candidate.eq_ignore_ascii_case(named))
    })
}

fn review_quality_score(card: &crate::models::PropertyCard) -> f64 {
    let policy = schema::ranking_policy();
    let total_weight = policy.review_rating_weight.max(0.0) + policy.review_count_weight.max(0.0);
    if total_weight <= 0.0 {
        return 0.0;
    }

    let rating_score = card
        .google_rating
        .filter(|rating| rating.is_finite())
        .map(|rating| (rating / 5.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let count_divisor = policy.review_count_log_divisor.max(1.0);
    let review_count_score = card
        .google_review_count
        .map(|count| (f64::from(count).ln_1p() / count_divisor).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    (rating_score * policy.review_rating_weight.max(0.0)
        + review_count_score * policy.review_count_weight.max(0.0))
        / total_weight
}

fn query_name_prefix_score(query_terms: &[&str], title: &str, society_name: &str) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let title_tokens = analyzer::search_tokens(title, schema::scoring_stopwords());
    let society_tokens = analyzer::search_tokens(society_name, schema::scoring_stopwords());
    name_prefix_score_for_tokens(query_terms, &society_tokens)
        .max(name_prefix_score_for_tokens(query_terms, &title_tokens))
}

fn name_prefix_score_for_tokens(query_terms: &[&str], candidate_tokens: &[String]) -> f64 {
    if query_terms.is_empty() || candidate_tokens.is_empty() {
        return 0.0;
    }

    let mut best: f64 = 0.0;
    for start in 0..candidate_tokens.len() {
        let mut matched = 0usize;
        for (offset, query_term) in query_terms.iter().enumerate() {
            let Some(candidate_token) = candidate_tokens.get(start + offset) else {
                break;
            };
            if *query_term == candidate_token.as_str()
                || candidate_token.starts_with(*query_term)
                || query_term.starts_with(candidate_token.as_str())
            {
                matched += 1;
                continue;
            }
            break;
        }
        best = best.max(matched as f64 / query_terms.len() as f64);
    }
    best
}

fn structured_intent_terms(intent: &SearchIntent) -> Vec<String> {
    let mut terms = Vec::new();
    if let Some(area) = intent.area.as_deref() {
        push_intent_text_terms(&mut terms, area);
    }
    if let Some(bhk) = intent.bhk {
        push_unique_string(&mut terms, &format!("{bhk}bhk"));
        push_unique_string(&mut terms, &bhk.to_string());
    }
    for pref in intent
        .positive_preferences
        .iter()
        .chain(intent.negative_preferences.iter())
    {
        push_intent_text_terms(&mut terms, &pref.raw_text);
        for key in &pref.expanded_keys {
            push_intent_text_terms(&mut terms, key);
        }
    }
    for tradeoff in &intent.accepted_tradeoffs {
        push_intent_text_terms(&mut terms, tradeoff);
    }
    for constraint in &intent.hard_constraints {
        push_intent_text_terms(&mut terms, &constraint.raw_text);
    }
    terms
}

fn push_intent_text_terms(values: &mut Vec<String>, text: &str) {
    for term in analyzer::search_tokens(text, schema::scoring_stopwords()) {
        push_unique_string(values, &term);
    }
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn scoring_query_terms(query_lower: &str) -> Vec<String> {
    analyzer::search_tokens(query_lower, schema::scoring_stopwords())
}

fn match_hard_constraints(
    intent: &SearchIntent,
    graph: Option<&KnowledgeGraph>,
    serving_facts: Option<&ServingFactIndex>,
    society_id: &str,
) -> Option<Vec<EvidenceMatch>> {
    if intent.hard_constraints.is_empty() {
        return Some(Vec::new());
    }

    let mut matches = Vec::new();

    for constraint in &intent.hard_constraints {
        let schema = schema::numeric_constraint_schema(&constraint.field)?;
        let serving_evaluation = serving_facts
            .map(|index| serving_numeric_constraint_evidence(index, society_id, schema, constraint))
            .unwrap_or(ConstraintEvaluation::Missing);
        match serving_evaluation {
            ConstraintEvaluation::Matched(evidence) => matches.push(evidence),
            ConstraintEvaluation::Failed => return None,
            ConstraintEvaluation::Missing => {
                let graph = graph?;
                match numeric_constraint_evidence(graph, society_id, schema, constraint) {
                    ConstraintEvaluation::Matched(evidence) => matches.push(evidence),
                    ConstraintEvaluation::Failed | ConstraintEvaluation::Missing => return None,
                }
            }
        }
    }

    Some(matches)
}

fn numeric_constraint_evidence(
    graph: &KnowledgeGraph,
    society_id: &str,
    schema: &NumericConstraintSchema,
    constraint: &HardConstraint,
) -> ConstraintEvaluation {
    let node_id = society_node_id(society_id);
    let Some(node) = graph.get_node(&node_id) else {
        return ConstraintEvaluation::Missing;
    };
    let Some(query_unit) = schema
        .query_units
        .iter()
        .find(|unit| unit.unit.eq_ignore_ascii_case(&constraint.unit))
    else {
        return ConstraintEvaluation::Missing;
    };
    let threshold = constraint.value * query_unit.to_canonical;

    for fact_key in &schema.fact_keys {
        let Some(fact) = node.facts.iter().find(|fact| {
            fact.key.eq_ignore_ascii_case(fact_key)
                && schema
                    .proof_sources
                    .iter()
                    .any(|source| source == &fact.source.source_type)
        }) else {
            continue;
        };

        let FactValue::Numeric(canonical_value) = &fact.value else {
            continue;
        };
        if !canonical_value.is_finite() {
            continue;
        }

        match constraint.operator {
            ConstraintOperator::Min => {
                if canonical_value + 0.001 < threshold {
                    return ConstraintEvaluation::Failed;
                }
            }
            ConstraintOperator::Max => {
                if canonical_value - 0.001 > threshold {
                    return ConstraintEvaluation::Failed;
                }
            }
        }

        let display_value = canonical_value / query_unit.to_canonical;
        return ConstraintEvaluation::Matched(EvidenceMatch {
            preference: constraint.raw_text.clone(),
            fact_key: fact.key.clone(),
            fact_key_rank: usize::MAX,
            display: format!(
                "{}: {} {}",
                schema.label,
                format_measurement(display_value),
                query_unit.unit
            ),
            normalized_score: 1.0,
            score_delta: 2.0,
            confidence: fact.confidence,
            source_type: format!("{:?}", fact.source.source_type),
            scoring_method: schema.scoring_method.clone(),
            reason: format!("proved constraint: {}", constraint.raw_text),
        });
    }

    ConstraintEvaluation::Missing
}

fn serving_numeric_constraint_evidence(
    serving_facts: &ServingFactIndex,
    society_id: &str,
    schema: &NumericConstraintSchema,
    constraint: &HardConstraint,
) -> ConstraintEvaluation {
    let node_id = society_node_id(society_id);
    let Some(rows) = serving_facts.entity(&node_id) else {
        return ConstraintEvaluation::Missing;
    };
    let Some(query_unit) = schema
        .query_units
        .iter()
        .find(|unit| unit.unit.eq_ignore_ascii_case(&constraint.unit))
    else {
        return ConstraintEvaluation::Missing;
    };
    let threshold = constraint.value * query_unit.to_canonical;

    for fact_key in &schema.fact_keys {
        let Some(fact) = rows.facts.iter().find(|fact| {
            fact.fact_key.eq_ignore_ascii_case(fact_key)
                && schema.proof_sources.iter().any(|source| {
                    fact.source_type
                        .eq_ignore_ascii_case(&format!("{source:?}"))
                })
        }) else {
            continue;
        };
        let FactValue::Numeric(canonical_value) = fact.value else {
            continue;
        };
        if !canonical_value.is_finite() {
            continue;
        }
        match constraint.operator {
            ConstraintOperator::Min if canonical_value + 0.001 < threshold => {
                return ConstraintEvaluation::Failed;
            }
            ConstraintOperator::Max if canonical_value - 0.001 > threshold => {
                return ConstraintEvaluation::Failed;
            }
            ConstraintOperator::Min => {}
            ConstraintOperator::Max => {}
        }
        let display_value = canonical_value / query_unit.to_canonical;
        return ConstraintEvaluation::Matched(EvidenceMatch {
            preference: constraint.raw_text.clone(),
            fact_key: fact.fact_key.clone(),
            fact_key_rank: usize::MAX,
            display: format!(
                "{}: {} {}",
                schema.label,
                format_measurement(display_value),
                query_unit.unit
            ),
            normalized_score: 1.0,
            score_delta: 2.0,
            confidence: fact.confidence,
            source_type: fact.source_type.clone(),
            scoring_method: schema.scoring_method.clone(),
            reason: format!("proved constraint: {}", constraint.raw_text),
        });
    }

    ConstraintEvaluation::Missing
}

enum ConstraintEvaluation {
    Missing,
    Failed,
    Matched(EvidenceMatch),
}

fn graph_textual_preference_evidence(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
    candidate_fact_keys: &[String],
) -> Option<EvidenceMatch> {
    let schema = schema::text_evidence_schema(preference)?;
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;

    for fact in &node.facts {
        if schema::search_excludes_fact_key(&fact.key) {
            continue;
        }
        if !candidate_fact_keys.is_empty()
            && !candidate_fact_keys
                .iter()
                .any(|key| key.eq_ignore_ascii_case(&fact.key))
        {
            continue;
        }
        if !schema::fact_answers_text_schema(&fact.key, &fact.answers_preferences, schema) {
            continue;
        }

        if let Some(snippet) = schema::text_support_snippet(&fact.value, schema) {
            let source_type = format!("{:?}", fact.source.source_type);
            if !evidence_is_confident_enough(&source_type, fact.confidence, "graph-text") {
                continue;
            }
            return Some(EvidenceMatch {
                preference: preference.to_string(),
                fact_key: fact.key.clone(),
                fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, &fact.key),
                display: format!("{}: {}", schema.display_label, snippet),
                normalized_score: 0.7,
                score_delta: schema.score_delta,
                confidence: fact.confidence,
                source_type,
                scoring_method: "graph-text".into(),
                reason: format!("matches preference: {}", preference),
            });
        }
    }

    None
}

fn serving_preference_evidence(
    serving_facts: &ServingFactIndex,
    society_id: &str,
    preference: &str,
    candidate_fact_keys: &[String],
    query_lower: &str,
) -> Option<EvidenceMatch> {
    let node_id = society_node_id(society_id);
    let rows = serving_facts.entity(&node_id)?;
    let source_priority = schema::source_priority_for_preference(preference);

    let mut best_structured: Option<RankedEvidence> = None;
    for fact in &rows.facts {
        if schema::search_excludes_fact_key(&fact.fact_key) {
            continue;
        }
        let Some(metadata) = rows
            .search_metadata_for_fact_key(&fact.fact_key)
            .find(|metadata| {
                let answers_preference = metadata_answers_preference(metadata, preference);
                let key_matches = candidate_fact_keys
                    .iter()
                    .any(|key| key.eq_ignore_ascii_case(&fact.fact_key));
                let metadata_can_expand = answers_preference
                    && !preference_requires_registry_fact_key(preference)
                    && fact_key_can_self_describe_preference(&fact.fact_key);
                let annotated_support_fact_matches_dimension =
                    metadata_can_expand && fact_key_mentions_preference(&fact.fact_key, preference);
                if candidate_fact_keys.is_empty() {
                    metadata_can_expand
                } else {
                    key_matches || annotated_support_fact_matches_dimension
                }
            })
        else {
            continue;
        };

        if fact_is_negative_support_for_positive_preference(&fact.fact_key, preference) {
            continue;
        }
        if place_fact_conflicts_with_explicit_query_family(query_lower, &fact.fact_key) {
            continue;
        }
        if !lifecycle_preference_value_compatible(preference, &fact.fact_key, &fact.value) {
            continue;
        }

        let Some((score_delta, scoring_method)) = serving_fact_score(fact, metadata) else {
            continue;
        };
        if !evidence_is_confident_enough(&fact.source_type, fact.confidence, &scoring_method) {
            continue;
        }
        let normalized_score = (score_delta / 2.0).min(1.0);
        let fact_key_rank = candidate_fact_key_rank(candidate_fact_keys, &fact.fact_key);
        let ranked = RankedEvidence {
            source_rank: source_rank(&source_priority, &fact.source_type),
            fact_key_rank,
            normalized_score,
            confidence: fact.confidence,
            evidence: EvidenceMatch {
                preference: preference.to_string(),
                fact_key: fact.fact_key.clone(),
                fact_key_rank,
                display: render_serving_fact_display(fact, metadata, &fact.value),
                normalized_score,
                score_delta,
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
                scoring_method,
                reason: format!("matches preference: {}", preference),
            },
        };
        if best_structured
            .as_ref()
            .is_none_or(|current| ranked.is_better_than(current))
        {
            best_structured = Some(ranked);
        }
    }

    if let Some(ranked) = best_structured {
        return Some(ranked.evidence);
    }

    let schema = schema::text_evidence_schema(preference)?;
    let mut best_text: Option<RankedEvidence> = None;
    for fact in &rows.facts {
        if schema::search_excludes_fact_key(&fact.fact_key) {
            continue;
        }
        if !candidate_fact_keys.is_empty()
            && !candidate_fact_keys
                .iter()
                .any(|key| key.eq_ignore_ascii_case(&fact.fact_key))
        {
            continue;
        }
        if geo::is_geo_distance_fact_key(&fact.fact_key) {
            continue;
        }
        if !schema::fact_answers_text_schema(&fact.fact_key, &[], schema) {
            continue;
        }
        if fact_is_negative_support_for_positive_preference(&fact.fact_key, preference) {
            continue;
        }
        if place_fact_conflicts_with_explicit_query_family(query_lower, &fact.fact_key) {
            continue;
        }

        if let Some(snippet) = schema::text_support_snippet(&fact.value, schema) {
            if !evidence_is_confident_enough(&fact.source_type, fact.confidence, "serving-text") {
                continue;
            }
            let ranked = RankedEvidence {
                source_rank: source_rank(&source_priority, &fact.source_type),
                fact_key_rank: usize::MAX,
                normalized_score: 0.7,
                confidence: fact.confidence,
                evidence: EvidenceMatch {
                    preference: preference.to_string(),
                    fact_key: fact.fact_key.clone(),
                    fact_key_rank: usize::MAX,
                    display: format!("{}: {}", schema.display_label, snippet),
                    normalized_score: 0.7,
                    score_delta: schema.score_delta,
                    confidence: fact.confidence,
                    source_type: fact.source_type.clone(),
                    scoring_method: "serving-text".into(),
                    reason: format!("matches preference: {}", preference),
                },
            };
            if best_text
                .as_ref()
                .is_none_or(|current| ranked.is_better_than(current))
            {
                best_text = Some(ranked);
            }
        }
    }

    best_text.map(|ranked| ranked.evidence)
}

fn place_fact_conflicts_with_explicit_query_family(query_lower: &str, fact_key: &str) -> bool {
    let Some(fact_category) = nearby_place_category_for_fact_key(fact_key) else {
        return false;
    };
    let requested = requested_nearby_place_categories(query_lower);
    !requested.is_empty()
        && !requested
            .iter()
            .any(|category| category.eq_ignore_ascii_case(fact_category))
}

fn graph_negative_preference_evidence(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
    candidate_fact_keys: &[String],
) -> Option<EvidenceMatch> {
    let node_id = society_node_id(society_id);
    let source_priority = schema::source_priority_for_preference(preference);
    let mut candidates = Vec::new();
    if let Some(node) = graph.get_node(&node_id) {
        candidates.push(node);
    }
    for area_node in graph.neighbors(
        &node_id,
        Some(crate::knowledge::edge::Relation::SocietyInArea),
    ) {
        candidates.push(area_node);
    }

    let numeric_schema = schema::numeric_evidence_schema(preference);
    let text_schema = schema::text_evidence_schema(preference);
    let mut best: Option<RankedEvidence> = None;
    for node in candidates {
        for fact in &node.facts {
            if schema::search_excludes_fact_key(&fact.key) {
                continue;
            }
            let Some(evidence) = negative_evidence_from_fact(
                &fact.key,
                &fact.value,
                format!("{:?}", fact.source.source_type),
                fact.confidence,
                fact.display_template.as_deref(),
                preference,
                candidate_fact_keys,
                None,
                numeric_schema,
                text_schema,
            ) else {
                continue;
            };
            let ranked = RankedEvidence {
                source_rank: source_rank(&source_priority, &evidence.source_type),
                fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, &evidence.fact_key),
                normalized_score: evidence.normalized_score,
                confidence: evidence.confidence,
                evidence,
            };
            if best
                .as_ref()
                .is_none_or(|current| ranked.is_better_than(current))
            {
                best = Some(ranked);
            }
        }
    }

    best.map(|ranked| ranked.evidence)
}

fn serving_negative_preference_evidence(
    serving_facts: &ServingFactIndex,
    society_id: &str,
    preference: &str,
    candidate_fact_keys: &[String],
) -> Option<EvidenceMatch> {
    let node_id = society_node_id(society_id);
    let rows = serving_facts.entity(&node_id)?;
    let source_priority = schema::source_priority_for_preference(preference);
    let numeric_schema = schema::numeric_evidence_schema(preference);
    let text_schema = schema::text_evidence_schema(preference);
    let mut best: Option<RankedEvidence> = None;

    for fact in &rows.facts {
        if schema::search_excludes_fact_key(&fact.fact_key) {
            continue;
        }
        let metadata = rows.search_metadata_for_fact_key(&fact.fact_key).next();
        let Some(evidence) = negative_evidence_from_fact(
            &fact.fact_key,
            &fact.value,
            fact.source_type.clone(),
            fact.confidence,
            metadata.and_then(|metadata| metadata.display_template.as_deref()),
            preference,
            candidate_fact_keys,
            metadata,
            numeric_schema,
            text_schema,
        ) else {
            continue;
        };
        let ranked = RankedEvidence {
            source_rank: source_rank(&source_priority, &evidence.source_type),
            fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, &evidence.fact_key),
            normalized_score: evidence.normalized_score,
            confidence: evidence.confidence,
            evidence,
        };
        if best
            .as_ref()
            .is_none_or(|current| ranked.is_better_than(current))
        {
            best = Some(ranked);
        }
    }

    best.map(|ranked| ranked.evidence)
}

#[allow(clippy::too_many_arguments)]
fn negative_evidence_from_fact(
    fact_key: &str,
    value: &FactValue,
    source_type: String,
    confidence: f32,
    display_template: Option<&str>,
    preference: &str,
    candidate_fact_keys: &[String],
    metadata: Option<&ServingSearchMetadataRecord>,
    numeric_schema: Option<&NumericEvidenceSchema>,
    text_schema: Option<&TextEvidenceSchema>,
) -> Option<EvidenceMatch> {
    if !evidence_is_confident_enough(&source_type, confidence, "risk") {
        return None;
    }

    if candidate_fact_keys
        .iter()
        .any(|key| key.eq_ignore_ascii_case(fact_key))
        && metadata.is_some_and(|metadata| {
            metadata
                .scoring_direction
                .as_deref()
                .is_some_and(|direction| direction.eq_ignore_ascii_case("Concern"))
        })
    {
        let metadata = metadata.expect("checked above");
        let snippet = fact_text_snippets(value)
            .into_iter()
            .find(|snippet| !snippet.trim().is_empty())?;
        let score_delta = -f64::from(metadata.scoring_weight.unwrap_or(1.0)).clamp(0.0, 2.0);
        let display = display_template
            .unwrap_or("{value}")
            .replace("{value}", &truncate_snippet(&snippet, 150));
        return Some(EvidenceMatch {
            preference: preference.to_string(),
            fact_key: fact_key.to_string(),
            fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, fact_key),
            display,
            normalized_score: 0.0,
            score_delta,
            confidence,
            source_type,
            scoring_method: "serving-concern".to_string(),
            reason: negative_reason(preference, score_delta),
        });
    }

    if let Some(schema) = numeric_schema {
        if schema_key_matches(fact_key, candidate_fact_keys, &schema.fact_keys) {
            if let Some((score_delta, normalized_score, risk_label)) =
                negative_numeric_score(value, schema)
            {
                let display_value = fact_value_display(value);
                return Some(EvidenceMatch {
                    preference: preference.to_string(),
                    fact_key: fact_key.to_string(),
                    fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, fact_key),
                    display: format!("{}: {}", risk_label, display_value),
                    normalized_score,
                    score_delta,
                    confidence,
                    source_type,
                    scoring_method: "graph-risk-numeric".to_string(),
                    reason: negative_reason(preference, score_delta),
                });
            }
        }
    }

    let schema = text_schema?;
    if !schema_key_matches(fact_key, candidate_fact_keys, &schema.fact_keys) {
        return None;
    }
    let (score_delta, normalized_score, risk_label, snippet) = negative_text_score(value, schema)?;
    let display = display_template
        .unwrap_or("{value}")
        .replace("{value}", &snippet);
    Some(EvidenceMatch {
        preference: preference.to_string(),
        fact_key: fact_key.to_string(),
        fact_key_rank: candidate_fact_key_rank(candidate_fact_keys, fact_key),
        display: format!("{}: {}", risk_label, display),
        normalized_score,
        score_delta,
        confidence,
        source_type,
        scoring_method: "graph-risk-text".to_string(),
        reason: negative_reason(preference, score_delta),
    })
}

fn schema_key_matches(
    fact_key: &str,
    candidate_fact_keys: &[String],
    schema_keys: &[String],
) -> bool {
    candidate_fact_keys
        .iter()
        .any(|key| key.eq_ignore_ascii_case(fact_key))
        || schema_keys
            .iter()
            .any(|key| key.eq_ignore_ascii_case(fact_key))
}

fn negative_numeric_score(
    value: &FactValue,
    schema: &NumericEvidenceSchema,
) -> Option<(f64, f64, String)> {
    let value = fact_value_numeric(value)?;
    if !value.is_finite() {
        return None;
    }
    let weight = schema.score_delta.clamp(0.0, 3.0);
    let lower_is_better = schema.direction.eq_ignore_ascii_case("LowerIsBetter")
        || schema.direction.eq_ignore_ascii_case("lower_is_better");
    let higher_is_better = schema.direction.eq_ignore_ascii_case("HigherIsBetter")
        || schema.direction.eq_ignore_ascii_case("higher_is_better");
    if schema.thresholds.len() < 2 {
        return None;
    }
    let label = schema.display_label.as_str();
    if lower_is_better {
        if value <= schema.thresholds[0] {
            Some((weight, 1.0, format!("Low {label}")))
        } else if value <= schema.thresholds[1] {
            Some((weight * 0.5, 0.5, format!("Moderate {label}")))
        } else {
            Some((-weight, 0.0, format!("High {label}")))
        }
    } else if higher_is_better {
        if value >= schema.thresholds[0] {
            Some((-weight, 0.0, format!("High {label}")))
        } else if value >= schema.thresholds[1] {
            Some((weight * 0.5, 0.5, format!("Moderate {label}")))
        } else {
            Some((weight, 1.0, format!("Low {label}")))
        }
    } else {
        None
    }
}

fn negative_text_score(
    value: &FactValue,
    schema: &TextEvidenceSchema,
) -> Option<(f64, f64, &'static str, String)> {
    for snippet in fact_text_snippets(value) {
        if schema
            .negative_terms
            .iter()
            .any(|term| analyzer::contains_stemmed_phrase(&snippet, term))
        {
            return Some((
                -schema.score_delta,
                0.0,
                "Risk signal",
                truncate_snippet(&snippet, 150),
            ));
        }
        if schema
            .positive_terms
            .iter()
            .any(|term| analyzer::contains_stemmed_phrase(&snippet, term))
        {
            return Some((
                schema.score_delta,
                1.0,
                "Lower risk signal",
                truncate_snippet(&snippet, 150),
            ));
        }
    }
    None
}

fn fact_text_snippets(value: &FactValue) -> Vec<String> {
    match value {
        FactValue::Text(value) => vec![value.clone()],
        FactValue::Tags(values) => values.clone(),
        FactValue::Score { explanation, .. } => vec![explanation.clone()],
        FactValue::Bool(_) | FactValue::Numeric(_) => Vec::new(),
    }
}

fn truncate_snippet(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out = trimmed.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn negative_reason(preference: &str, score_delta: f64) -> String {
    if score_delta >= 0.0 {
        format!("avoids {}", preference)
    } else {
        format!("risk: {}", preference)
    }
}

fn negative_coverage_status(evidence: &EvidenceMatch) -> &'static str {
    if evidence.score_delta < 0.0 {
        "risk"
    } else if evidence.normalized_score > 0.5 {
        "matched"
    } else {
        "partial"
    }
}

fn evidence_is_confident_enough(source_type: &str, confidence: f32, scoring_method: &str) -> bool {
    let source = source_type.to_lowercase();
    if source == "rera" || source == "computed" {
        return confidence >= 0.50;
    }
    if source == "llm" {
        return confidence >= schema::ranking_policy().min_llm_evidence_confidence;
    }
    if scoring_method == "local" || scoring_method == "local-risk" {
        return false;
    }
    confidence >= schema::ranking_policy().min_support_evidence_confidence
}

fn negative_no_data_penalty(intent: &SearchIntent, preference: &str) -> f64 {
    intent
        .negative_preferences
        .iter()
        .find(|signal| signal.raw_text.eq_ignore_ascii_case(preference))
        .map(|signal| {
            if signal.missing_evidence_neutral {
                0.0
            } else {
                f64::from(signal.weight).clamp(0.5, 2.0)
            }
        })
        .unwrap_or(1.0)
        * schema::ranking_policy().negative_no_data_penalty_multiplier
}

struct RankedEvidence {
    source_rank: usize,
    fact_key_rank: usize,
    normalized_score: f64,
    confidence: f32,
    evidence: EvidenceMatch,
}

impl RankedEvidence {
    fn is_better_than(&self, other: &Self) -> bool {
        self.source_rank < other.source_rank
            || (self.source_rank == other.source_rank && self.fact_key_rank < other.fact_key_rank)
            || (self.source_rank == other.source_rank
                && self.fact_key_rank == other.fact_key_rank
                && self.normalized_score > other.normalized_score)
            || (self.source_rank == other.source_rank
                && self.fact_key_rank == other.fact_key_rank
                && (self.normalized_score - other.normalized_score).abs() < f64::EPSILON
                && (self.confidence > other.confidence
                    || ((self.confidence - other.confidence).abs() < f32::EPSILON
                        && (self.evidence.fact_key < other.evidence.fact_key
                            || (self.evidence.fact_key == other.evidence.fact_key
                                && self.evidence.display < other.evidence.display)))))
    }
}

fn candidate_fact_key_rank(candidate_fact_keys: &[String], fact_key: &str) -> usize {
    candidate_fact_keys
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(fact_key))
        .unwrap_or(usize::MAX)
}

fn serving_fact_score(
    fact: &ServingFactRecord,
    metadata: &ServingSearchMetadataRecord,
) -> Option<(f64, String)> {
    let weight = f64::from(metadata.scoring_weight.unwrap_or(1.0)).clamp(0.0, 2.0);
    if weight <= 0.0 {
        return None;
    }
    let direction = metadata
        .scoring_direction
        .as_deref()
        .unwrap_or("TextMatch")
        .to_lowercase();
    let thresholds = metadata.scoring_thresholds.as_slice();

    if geo::is_geo_distance_fact_key(&fact.fact_key) {
        let score = geo::score_serving_geo_distance(fact, metadata)?;
        return Some((
            score.score_delta,
            geo::GEO_DISTANCE_SCORING_METHOD.to_string(),
        ));
    }

    if direction == "higherisbetter" || direction == "higher_is_better" {
        let value = fact_value_numeric(&fact.value)?;
        if !value.is_finite() {
            return None;
        }
        let score = if thresholds.len() >= 2 {
            if value >= thresholds[0] {
                weight
            } else if value >= thresholds[1] {
                weight * 0.5
            } else {
                0.0
            }
        } else if value > 0.0 {
            value.clamp(0.0, 1.0) * weight
        } else {
            0.0
        };
        return (score > 0.0).then(|| (score, "serving-numeric".to_string()));
    }

    if direction == "lowerisbetter" || direction == "lower_is_better" {
        let value = fact_value_numeric(&fact.value)?;
        if !value.is_finite() {
            return None;
        }
        let score = if thresholds.len() >= 2 {
            if value <= thresholds[0] {
                weight
            } else if value <= thresholds[1] {
                weight * 0.5
            } else {
                0.0
            }
        } else {
            let score = (1.0 - value.clamp(0.0, 1.0)) * weight;
            score.max(0.0)
        };
        return (score > 0.0).then(|| (score, "serving-numeric".to_string()));
    }

    if !metadata_supports_text_match(metadata) {
        return None;
    }
    if !meaningful_fact_value(&fact.value) {
        return None;
    }
    Some((weight, "serving-fact".to_string()))
}

fn fact_value_numeric(value: &FactValue) -> Option<f64> {
    match value {
        FactValue::Numeric(value) => Some(*value),
        FactValue::Score { value, .. } => Some(*value),
        _ => None,
    }
}

fn meaningful_fact_value(value: &FactValue) -> bool {
    match value {
        FactValue::Text(value) => !value.trim().is_empty(),
        FactValue::Tags(values) => values.iter().any(|value| !value.trim().is_empty()),
        FactValue::Bool(_) | FactValue::Numeric(_) | FactValue::Score { .. } => true,
    }
}

fn source_rank(source_priority: &[String], source_type: &str) -> usize {
    let source_type = source_type.to_lowercase();
    source_priority
        .iter()
        .position(|source| {
            let source = source.to_lowercase();
            source == source_type || source.contains(&source_type) || source_type.contains(&source)
        })
        .unwrap_or(source_priority.len() + 1)
}

fn fact_is_negative_support_for_positive_preference(fact_key: &str, preference: &str) -> bool {
    let fact_key = fact_key.to_lowercase();
    let preference = preference.to_lowercase();
    let negative_fact = schema::negative_fact_key_terms()
        .iter()
        .any(|term| fact_key.contains(&term.to_ascii_lowercase()));
    if !negative_fact {
        return false;
    }

    !schema::negative_preference_allow_terms()
        .iter()
        .any(|term| preference.contains(&term.to_ascii_lowercase()))
}

fn metadata_supports_text_match(metadata: &ServingSearchMetadataRecord) -> bool {
    metadata
        .scoring_direction
        .as_deref()
        .is_none_or(|direction| {
            direction.eq_ignore_ascii_case("TextMatch")
                || direction.eq_ignore_ascii_case("text_match")
        })
}

fn fact_key_can_self_describe_preference(fact_key: &str) -> bool {
    let key = fact_key.to_ascii_lowercase();
    !schema::fact_key_self_describe_excluded_suffixes()
        .iter()
        .any(|suffix| key.ends_with(&suffix.to_ascii_lowercase()))
        && !schema::fact_key_self_describe_excluded_exact()
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(&key))
}

fn fact_key_mentions_preference(fact_key: &str, preference: &str) -> bool {
    let key = fact_key.to_ascii_lowercase();
    preference
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| term.len() >= 4)
        .any(|term| key.contains(term))
}

fn preference_requires_registry_fact_key(preference: &str) -> bool {
    schema::registry_fact_key_required_preferences()
        .iter()
        .any(|required| required.eq_ignore_ascii_case(preference))
}

fn lifecycle_preference_value_compatible(
    preference: &str,
    fact_key: &str,
    value: &FactValue,
) -> bool {
    if !preference_requires_registry_fact_key(preference) {
        return true;
    }
    let Some(text) = fact_value_search_text(value) else {
        return true;
    };
    let Some(rule) = schema::lifecycle_compatibility_rule(preference) else {
        return true;
    };
    if rule
        .reject_any_groups
        .iter()
        .any(|group| lifecycle_group_matches(group, &text))
    {
        return false;
    }
    if rule
        .require_any_groups
        .iter()
        .any(|group| lifecycle_group_matches(group, &text))
    {
        return true;
    }
    rule.require_fact_key_any_groups.iter().any(|allowance| {
        allowance.fact_key.eq_ignore_ascii_case(fact_key)
            && allowance
                .groups
                .iter()
                .any(|group| lifecycle_group_matches(group, &text))
    })
}

fn lifecycle_group_matches(group: &str, text: &str) -> bool {
    let terms = &schema::runtime_policy().lifecycle_value_terms;
    let values = match group {
        "ready" => &terms.ready,
        "under_construction" => &terms.under_construction,
        "delay" => &terms.delay,
        "new_age" => &terms.new_age,
        "established_age" => &terms.established_age,
        _ => return false,
    };
    values
        .iter()
        .any(|term| text.contains(&term.to_ascii_lowercase()))
}

fn fact_value_search_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(value) => Some(value.to_ascii_lowercase()),
        FactValue::Tags(values) => Some(values.join(" ").to_ascii_lowercase()),
        FactValue::Score { explanation, .. } => Some(explanation.to_ascii_lowercase()),
        FactValue::Bool(_) | FactValue::Numeric(_) => None,
    }
}

fn metadata_answers_preference(metadata: &ServingSearchMetadataRecord, preference: &str) -> bool {
    let preference = preference.to_lowercase();
    metadata.answers_preferences.iter().any(|answer| {
        let answer = answer.to_lowercase();
        answer == preference || answer.contains(&preference) || preference.contains(&answer)
    })
}

fn render_serving_fact_display(
    fact: &ServingFactRecord,
    metadata: &ServingSearchMetadataRecord,
    value: &FactValue,
) -> String {
    let value = compact_serving_fact_display_value(fact, value);
    metadata
        .display_template
        .as_deref()
        .unwrap_or("{value}")
        .replace("{value}", &value)
}

fn compact_serving_fact_display_value(fact: &ServingFactRecord, value: &FactValue) -> String {
    const MAX_DISPLAY_CHARS: usize = 180;

    if let Some(value_text) = fact.value_text.as_deref() {
        return truncate_snippet(value_text, MAX_DISPLAY_CHARS);
    }

    match value {
        FactValue::Text(value) => truncate_snippet(value, MAX_DISPLAY_CHARS),
        FactValue::Tags(values) => compact_tag_display(values, MAX_DISPLAY_CHARS),
        FactValue::Score { value, explanation } => truncate_snippet(
            &format!("{}: {}", format_measurement(*value), explanation),
            MAX_DISPLAY_CHARS,
        ),
        FactValue::Numeric(_) | FactValue::Bool(_) => fact_value_display(value),
    }
}

fn compact_tag_display(values: &[String], max_chars: usize) -> String {
    let mut visible = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let first = visible.next().unwrap_or_default();
    let Some(second) = visible.next() else {
        return truncate_snippet(first, max_chars);
    };
    let remaining = visible.count();
    let mut display = format!("{}, {}", first, second);
    if remaining > 0 {
        display.push_str(&format!(" +{} more", remaining));
    }
    truncate_snippet(&display, max_chars)
}

fn fact_value_display(value: &FactValue) -> String {
    match value {
        FactValue::Numeric(value) => format_measurement(*value),
        FactValue::Text(value) => value.clone(),
        FactValue::Bool(value) => value.to_string(),
        FactValue::Tags(values) => values.join(", "),
        FactValue::Score { value, explanation } => {
            format!("{}: {}", format_measurement(*value), explanation)
        }
    }
}

fn format_measurement(value: f64) -> String {
    if value >= 100.0 {
        format!("{:.0}", value)
    } else if value >= 10.0 {
        format!("{:.1}", value)
    } else {
        format!("{:.2}", value)
    }
}

/// Compute a confidence score for a property based on data quality dimensions.
/// Used by search results (with graph_driven_pct from match explanation).
pub fn compute_confidence(
    graph: Option<&KnowledgeGraph>,
    society_id: &str,
    graph_driven_pct: f32,
) -> Option<ConfidenceScore> {
    let graph = graph?;
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id);

    // Source quality: RERA=1.0, Discovered=0.5, Legacy/None=0.3
    let (source_score, source_explanation) = compute_source_quality(&node);

    // Fact coverage: min(fact_count/configured full-coverage threshold, 1.0)
    let (coverage_score, coverage_explanation) = compute_fact_coverage(&node);

    // Freshness with bulk-creation cap
    let (freshness_score, freshness_explanation) = compute_freshness(&node);

    // Match quality: graph_driven_pct / 100.0
    let match_score = (graph_driven_pct / 100.0) as f64;
    let match_explanation = format!(
        "{}% of scoring from verified graph data",
        graph_driven_pct.round() as u32
    );

    // Weighted average: source 0.4, coverage 0.2, freshness 0.2, match 0.2
    let overall =
        source_score * 0.4 + coverage_score * 0.2 + freshness_score * 0.2 + match_score * 0.2;

    let label = confidence_label(overall);

    let components = vec![
        ConfidenceComponent {
            dimension: "source_quality".to_string(),
            score: source_score,
            weight: 0.4,
            explanation: source_explanation,
        },
        ConfidenceComponent {
            dimension: "fact_coverage".to_string(),
            score: coverage_score,
            weight: 0.2,
            explanation: coverage_explanation,
        },
        ConfidenceComponent {
            dimension: "freshness".to_string(),
            score: freshness_score,
            weight: 0.2,
            explanation: freshness_explanation,
        },
        ConfidenceComponent {
            dimension: "match_quality".to_string(),
            score: match_score,
            weight: 0.2,
            explanation: match_explanation,
        },
    ];

    Some(ConfidenceScore {
        overall: (overall * 100.0).round() / 100.0,
        label,
        components,
    })
}

fn compute_confidence_from_serving_facts(
    serving_facts: &ServingFactIndex,
    society_id: &str,
    graph_driven_pct: f32,
) -> Option<ConfidenceScore> {
    let rows = serving_facts.entity(&society_node_id(society_id))?;
    let fact_count = rows.facts.len();
    if fact_count == 0 {
        return None;
    }

    let source_score = if rows
        .facts
        .iter()
        .any(|fact| fact.source_type.eq_ignore_ascii_case("rera"))
    {
        1.0
    } else if rows
        .facts
        .iter()
        .any(|fact| fact.source_type.eq_ignore_ascii_case("seller"))
    {
        0.6
    } else {
        0.5
    };
    let source_explanation = format!(
        "Serving bundle evidence from {} source{}",
        rows.facts
            .iter()
            .map(|fact| fact.source_type.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        if fact_count == 1 { "" } else { "s" }
    );

    let threshold = schema::ranking_policy().fact_coverage_threshold;
    let coverage_score = (fact_count as f64 / threshold).min(1.0);
    let coverage_explanation = format!(
        "{fact_count} serving facts available ({} = full coverage)",
        threshold as u32
    );

    let newest = rows.facts.iter().map(|fact| fact.learned_at).max()?;
    let days_ago = (chrono::Utc::now() - newest).num_days().max(0) as u32;
    let freshness_score = if days_ago < 7 {
        1.0
    } else if days_ago < 30 {
        0.8
    } else if days_ago < 90 {
        0.5
    } else {
        0.2
    };
    let freshness_explanation = format!("Newest serving fact learned {days_ago} days ago");

    let match_score = (graph_driven_pct / 100.0) as f64;
    let match_explanation = format!(
        "{}% of scoring from serving/graph evidence",
        graph_driven_pct.round() as u32
    );
    let overall =
        source_score * 0.4 + coverage_score * 0.2 + freshness_score * 0.2 + match_score * 0.2;

    Some(ConfidenceScore {
        overall: (overall * 100.0).round() / 100.0,
        label: confidence_label(overall),
        components: vec![
            ConfidenceComponent {
                dimension: "source_quality".to_string(),
                score: source_score,
                weight: 0.4,
                explanation: source_explanation,
            },
            ConfidenceComponent {
                dimension: "fact_coverage".to_string(),
                score: coverage_score,
                weight: 0.2,
                explanation: coverage_explanation,
            },
            ConfidenceComponent {
                dimension: "freshness".to_string(),
                score: freshness_score,
                weight: 0.2,
                explanation: freshness_explanation,
            },
            ConfidenceComponent {
                dimension: "match_quality".to_string(),
                score: match_score,
                weight: 0.2,
                explanation: match_explanation,
            },
        ],
    })
}

/// Compute a confidence score for the detail page, replacing match_quality
/// (which is meaningless outside search context) with fact_source_quality
/// (average confidence of the node's facts).
pub fn compute_confidence_for_detail(
    graph: Option<&KnowledgeGraph>,
    society_id: &str,
) -> Option<ConfidenceScore> {
    let graph = graph?;
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id);

    let (source_score, source_explanation) = compute_source_quality(&node);
    let (coverage_score, coverage_explanation) = compute_fact_coverage(&node);
    let (freshness_score, freshness_explanation) = compute_freshness(&node);

    // Fact source quality: average confidence of all facts on this node.
    // This replaces match_quality (graph_driven_pct) which is 0.0 on detail pages.
    let (fact_quality_score, fact_quality_explanation) = if let Some(n) = &node {
        if n.facts.is_empty() {
            (0.0, "No facts available".to_string())
        } else {
            let avg: f64 =
                n.facts.iter().map(|f| f.confidence as f64).sum::<f64>() / n.facts.len() as f64;
            (
                avg,
                format!(
                    "Average fact confidence: {:.0}% across {} facts",
                    avg * 100.0,
                    n.facts.len()
                ),
            )
        }
    } else {
        (0.0, "No knowledge graph data".to_string())
    };

    // Weighted average: source 0.4, coverage 0.2, freshness 0.2, fact_quality 0.2
    let overall = source_score * 0.4
        + coverage_score * 0.2
        + freshness_score * 0.2
        + fact_quality_score * 0.2;

    let label = confidence_label(overall);

    let components = vec![
        ConfidenceComponent {
            dimension: "source_quality".to_string(),
            score: source_score,
            weight: 0.4,
            explanation: source_explanation,
        },
        ConfidenceComponent {
            dimension: "fact_coverage".to_string(),
            score: coverage_score,
            weight: 0.2,
            explanation: coverage_explanation,
        },
        ConfidenceComponent {
            dimension: "freshness".to_string(),
            score: freshness_score,
            weight: 0.2,
            explanation: freshness_explanation,
        },
        ConfidenceComponent {
            dimension: "fact_source_quality".to_string(),
            score: fact_quality_score,
            weight: 0.2,
            explanation: fact_quality_explanation,
        },
    ];

    Some(ConfidenceScore {
        overall: (overall * 100.0).round() / 100.0,
        label,
        components,
    })
}

// ---------------------------------------------------------------------------
// Confidence scoring helpers — shared between search and detail variants
// ---------------------------------------------------------------------------

use crate::knowledge::node::Node;

fn compute_source_quality(node: &Option<&Node>) -> (f64, String) {
    if let Some(n) = node {
        match n.root_source {
            Some(RootSource::Rera) => (1.0, "RERA verified source".to_string()),
            Some(RootSource::Seller) => (0.6, "Self-reported source".to_string()),
            Some(RootSource::Discovered) => (
                0.5,
                "Discovered via search, verification pending".to_string(),
            ),
            Some(RootSource::Legacy) | None => (0.3, "Legacy/unclassified source".to_string()),
        }
    } else {
        (0.3, "No knowledge graph data".to_string())
    }
}

fn compute_fact_coverage(node: &Option<&Node>) -> (f64, String) {
    let fact_count = node.as_ref().map(|n| n.facts.len()).unwrap_or(0);
    let threshold = schema::ranking_policy().fact_coverage_threshold;
    let score = (fact_count as f64 / threshold).min(1.0);
    let explanation = format!(
        "{} facts available ({} = full coverage)",
        fact_count, threshold as u32
    );
    (score, explanation)
}

/// Compute freshness score with a cap for bulk-created nodes.
/// If all facts share the same learned_at timestamp (within 1 second), freshness
/// is capped at 0.5 to distinguish "freshly enriched" from "bulk-seeded".
fn compute_freshness(node: &Option<&Node>) -> (f64, String) {
    if let Some(n) = node {
        let most_recent_fact_ts = n.facts.iter().map(|f| f.learned_at).max();
        let effective_ts = most_recent_fact_ts.unwrap_or(n.updated_at);
        let days_ago = (chrono::Utc::now() - effective_ts).num_days().max(0) as u32;

        let raw_score: f64 = if days_ago < 7 {
            1.0
        } else if days_ago < 30 {
            0.8
        } else if days_ago < 90 {
            0.5
        } else {
            0.3
        };

        // Cap freshness at 0.5 if all facts have the same timestamp (within 1s).
        // This catches bulk-imported and newly discovered nodes where all facts
        // were created in a single batch, vs genuinely enriched nodes where facts
        // were added over time by different skills.
        let capped = if n.facts.len() >= 2 && all_facts_same_timestamp(&n.facts) {
            raw_score.min(0.5)
        } else {
            raw_score
        };

        let suffix = if capped < raw_score {
            " (bulk-created cap)"
        } else {
            ""
        };
        let label = match days_ago {
            0..=6 => "fresh",
            7..=29 => "recent",
            30..=89 => "aging",
            _ => "stale",
        };

        (
            capped,
            format!("Updated {} days ago ({}){}", days_ago, label, suffix),
        )
    } else {
        (0.3, "No update timestamp available".to_string())
    }
}

/// Check if all facts in a slice share the same learned_at timestamp within 1 second.
fn all_facts_same_timestamp(facts: &[crate::knowledge::SourcedFact]) -> bool {
    if facts.is_empty() {
        return true;
    }
    let first = facts[0].learned_at;
    facts.iter().all(|f| {
        let diff = (f.learned_at - first).num_seconds().abs();
        diff <= 1
    })
}

fn confidence_label(overall: f64) -> String {
    if overall >= 0.7 {
        "High".to_string()
    } else if overall >= 0.4 {
        "Moderate".to_string()
    } else {
        "Low".to_string()
    }
}

/// Score a property against search terms. Returns (score, match_reasons).
fn score_property(property: &Property, society_name: &str, terms: &[&str]) -> (f64, Vec<String>) {
    let fields: Vec<(&str, f64, &str)> = vec![
        (&property.title, 3.0, "title"),
        (&property.area, 2.5, "area"),
        (&property.builder_name, 2.0, "builder"),
        (society_name, 2.0, "society"),
        (&property.description_summary, 1.0, "description"),
        (&property.property_type, 1.5, "type"),
        (&property.possession_status, 1.0, "status"),
        (&property.facing, 0.5, "facing"),
        (&property.city, 1.5, "city"),
    ];

    let mut total_score = 0.0;
    let mut reasons = Vec::new();

    for term in terms {
        let mut term_matched = false;

        for (field_value, weight, field_name) in &fields {
            let field_lower = field_value.to_lowercase();
            if crate::search::index::text_field_matches_term(&field_lower, term) {
                total_score += weight;
                if !term_matched {
                    reasons.push(format!("matched '{}' in {}", term, field_name));
                    term_matched = true;
                }
            }
        }

        // Also check transparency tags.
        for tag in &property.transparency_tags {
            if crate::search::index::text_field_matches_term(&tag.to_lowercase(), term) {
                total_score += 1.0;
                if !term_matched {
                    reasons.push(format!("matched '{}' in tags", term));
                    term_matched = true;
                }
            }
        }
    }

    (total_score, reasons)
}

fn query_mentions_resolvable_society(query_lower: &str, society_name: &str) -> bool {
    let resolution_config = search_resolution_config();
    is_resolvable_entity_name(society_name, resolution_config)
        && query_contains_lower_text(query_lower, society_name)
}

/// Check if a property's area is "nearby" the canonical search area.
/// This catches sub-areas, micro-markets, and externally assigned areas that
/// belong to the same macro area but don't exactly match the canonical name.
///
/// Checks: alias list membership, substring containment, and same-city
/// knowledge graph edges (future). Does NOT check exact match — caller does that.
fn area_is_nearby(property_area: &str, canonical_area: &str) -> bool {
    use crate::dag_config::area_alias_entries;

    let prop_lower = property_area.trim().to_lowercase();
    let canon_lower = canonical_area.trim().to_lowercase();
    if prop_lower.is_empty() || canon_lower.is_empty() {
        return false;
    }

    // 1. Property area is a known alias of the canonical area
    for entry in area_alias_entries() {
        if !entry.canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        for alias in &entry.aliases {
            if prop_lower.contains(alias) || alias.contains(prop_lower.as_str()) {
                return true;
            }
        }
        break;
    }

    // 2. Property area maps to the same canonical area via its own aliases
    //    e.g. property area "Varthur" → canonical "Whitefield", search area is "Whitefield"
    for entry in area_alias_entries() {
        if !entry.canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        // Check if any word in the property area matches an alias
        for word in prop_lower.split_whitespace() {
            for alias in &entry.aliases {
                if alias == word {
                    return true;
                }
            }
        }
        break;
    }

    // 3. Substring containment (handles "East Whitefield" matching "Whitefield")
    if prop_lower.contains(&canon_lower) || canon_lower.contains(&prop_lower) {
        return true;
    }

    false
}

fn property_matches_area(
    property: &crate::models::Property,
    area: &str,
    graph: Option<&KnowledgeGraph>,
) -> bool {
    property.area.eq_ignore_ascii_case(area)
        || area_is_nearby(&property.area, area)
        || graph_area_match(&property.society_id, area, graph)
}

fn match_label_from_score_and_coverage(
    normalized: f64,
    explanation: Option<&MatchExplanation>,
    has_preferences: bool,
) -> String {
    if has_preferences {
        if let Some(explanation) = explanation {
            let has_coverage = !explanation.preference_coverage.is_empty();
            let all_matched = has_coverage
                && explanation
                    .preference_coverage
                    .iter()
                    .all(|coverage| coverage.status == "matched");
            let any_matched = explanation
                .preference_coverage
                .iter()
                .any(|coverage| coverage.status == "matched");
            if all_matched {
                return if normalized >= 0.75 {
                    "Strong match".to_string()
                } else {
                    "Good match".to_string()
                };
            }
            if any_matched && normalized < 0.25 {
                return "Partial match".to_string();
            }
        }
    }

    if normalized >= 0.75 {
        "Strong match".to_string()
    } else if normalized >= 0.5 {
        "Good match".to_string()
    } else if normalized >= 0.25 {
        "Partial match".to_string()
    } else {
        "Weak match".to_string()
    }
}

#[derive(Clone, Copy)]
enum AreaMatchKind {
    Exact,
    Nearby,
    Graph,
}

fn build_match_reason(
    intent: &SearchIntent,
    property_area: &str,
    area_match_kind: Option<AreaMatchKind>,
    reasons: &[String],
) -> String {
    let mut parts = Vec::new();

    parts.extend(named_place_match_reasons(reasons));

    if let Some(ref area) = intent.area {
        match area_match_kind {
            Some(AreaMatchKind::Nearby) => {
                parts.push(format!("Near {} ({})", area, property_area));
            }
            Some(AreaMatchKind::Exact | AreaMatchKind::Graph) | None => {
                parts.push(format!("Matches {}", area));
            }
        }
    }
    if let Some(bhk) = intent.bhk {
        parts.push(format!("{} BHK", bhk));
    }
    if let Some(budget) = intent.budget_max {
        let budget_str = if budget >= 10_000_000 {
            format!("{:.1} Cr", budget as f64 / 10_000_000.0)
        } else {
            format!("{:.0} L", budget as f64 / 100_000.0)
        };
        parts.push(format!("under {}", budget_str));
    }

    for constraint in &intent.hard_constraints {
        parts.push(constraint.raw_text.clone());
    }

    for pref in positive_preference_labels(intent) {
        if preference_was_matched(reasons, &pref) {
            parts.push(pref);
        }
    }
    for pref in negative_preference_labels(intent) {
        if negative_preference_was_avoided(reasons, &pref) {
            parts.push(format!("avoid {}", pref));
        }
    }

    if parts.is_empty() {
        // Fall back to raw match reasons
        reasons
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        parts.join(", ")
    }
}

fn named_place_match_reasons(reasons: &[String]) -> Vec<String> {
    let mut named_places = Vec::new();
    for reason in reasons {
        let Some((preference, _)) = reason.split_once(':') else {
            continue;
        };
        let preference = preference.trim();
        let Some(place) = preference.strip_prefix("near ") else {
            continue;
        };
        let place = place.trim();
        if place.is_empty()
            || named_places
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(place))
        {
            continue;
        }
        named_places.push(place.to_string());
    }

    named_places
        .into_iter()
        .map(|place| format!("Near {place}"))
        .collect()
}

fn preference_was_matched(reasons: &[String], preference: &str) -> bool {
    let expected = format!("matches preference: {}", preference);
    reasons.iter().any(|reason| reason == &expected)
}

fn has_positive_preference_evidence(
    coverage: &[PreferenceCoverage],
    positive_preferences: &[String],
) -> bool {
    coverage.iter().any(|coverage| {
        positive_preferences
            .iter()
            .any(|preference| preference == &coverage.preference)
            && (coverage.status == "matched" || coverage.status == "partial")
    })
}

fn negative_preference_was_avoided(reasons: &[String], preference: &str) -> bool {
    let expected = format!("avoids {}", preference);
    reasons.iter().any(|reason| reason == &expected)
}

fn positive_preference_labels(intent: &SearchIntent) -> Vec<String> {
    if !intent.positive_preferences.is_empty() {
        intent
            .positive_preferences
            .iter()
            .map(|pref| pref.raw_text.clone())
            .collect()
    } else {
        intent
            .preferences
            .iter()
            .map(|pref| schema::legacy_display_preference_signal(pref))
            .filter(|signal| signal.polarity == crate::search::intent::Polarity::Positive)
            .map(|signal| signal.raw_text)
            .collect()
    }
}

fn positive_preference_keys(intent: &SearchIntent, preference: &str) -> Vec<String> {
    intent
        .positive_preferences
        .iter()
        .find(|signal| signal.raw_text == preference)
        .map(|signal| signal.expanded_keys.clone())
        .unwrap_or_else(|| {
            let signal = schema::preference_signal_for_label(
                preference,
                crate::search::intent::Polarity::Positive,
            );
            signal.expanded_keys
        })
}

fn negative_preference_labels(intent: &SearchIntent) -> Vec<String> {
    if !intent.negative_preferences.is_empty() {
        intent
            .negative_preferences
            .iter()
            .map(|pref| pref.raw_text.clone())
            .collect()
    } else {
        intent
            .preferences
            .iter()
            .map(|pref| schema::legacy_display_preference_signal(pref))
            .filter(|signal| signal.polarity == crate::search::intent::Polarity::Negative)
            .map(|signal| signal.raw_text)
            .collect()
    }
}

fn negative_preference_keys(intent: &SearchIntent, preference: &str) -> Vec<String> {
    intent
        .negative_preferences
        .iter()
        .find(|signal| signal.raw_text == preference)
        .map(|signal| signal.expanded_keys.clone())
        .unwrap_or_else(|| {
            let signal = schema::preference_signal_for_label(
                preference,
                crate::search::intent::Polarity::Negative,
            );
            signal.expanded_keys
        })
}

/// Check if a property's society has a SocietyInArea edge to an area node
/// whose name matches the intent area. This catches societies whose area
/// fact doesn't exactly match the property's area field.
fn graph_area_match(society_id: &str, intent_area: &str, graph: Option<&KnowledgeGraph>) -> bool {
    use crate::knowledge::edge::Relation;
    use crate::routes::enrichment::society_node_id;

    let g = match graph {
        Some(g) => g,
        None => return false,
    };

    let node_id = society_node_id(society_id);
    let intent_lower = intent_area.to_lowercase();

    // Check all SocietyInArea neighbors of this society node.
    // Minimum length guard: only do substring containment when BOTH strings
    // are >= 4 chars to prevent false positives with short area names (e.g. "jp").
    // Exact matches always work regardless of length.
    for area_node in g.neighbors(&node_id, Some(Relation::SocietyInArea)) {
        let area_lower = area_node.name.to_lowercase();
        if area_lower == intent_lower {
            return true;
        }
        if area_lower.len() >= 4
            && intent_lower.len() >= 4
            && (area_lower.contains(&intent_lower) || intent_lower.contains(&area_lower))
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::edge::{Edge, Relation};
    use crate::knowledge::fact::{FactValue, ScoringDirection, SourceType, SourcedFact};
    use crate::knowledge::graph::KnowledgeGraph;
    use crate::knowledge::node::{Node, NodeType, RootSource};
    use crate::search::schema::SQM_PER_ACRE;
    use crate::serving::{
        ServingEntityRecord, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn structured_preference_terms_are_removed_from_free_text_scoring() {
        let intent =
            crate::search::intent::parse_intent("avoid water issues, no tanker dependency");
        let structured_terms = structured_intent_terms(&intent);

        for term in ["water", "issu", "tanker", "depend"] {
            assert!(
                structured_terms.iter().any(|structured| structured == term),
                "expected structured intent terms to include {term}: {structured_terms:?}"
            );
        }
    }

    /// Helper: build a minimal KnowledgeGraph with a society node, an area node,
    /// and a SocietyInArea edge between them.
    fn graph_with_society_in_area(
        society_slug: &str,
        area_slug: &str,
        area_name: &str,
    ) -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        let society_id = format!("society:{}", society_slug);
        let area_id = format!("area:{}", area_slug);

        g.add_node(Node::new(&society_id, NodeType::Society, society_slug));
        g.add_node(Node::new(&area_id, NodeType::Area, area_name));
        g.add_edge(Edge::new(society_id, area_id, Relation::SocietyInArea));
        g
    }

    // ---------------------------------------------------------------
    // graph_area_match tests
    // ---------------------------------------------------------------

    #[test]
    fn test_graph_area_match_exact() {
        let g = graph_with_society_in_area("prestige-lakeside", "whitefield", "Whitefield");
        assert!(graph_area_match(
            "prestige-lakeside",
            "Whitefield",
            Some(&g)
        ));
    }

    #[test]
    fn test_graph_area_match_case_insensitive() {
        let g = graph_with_society_in_area("prestige-lakeside", "whitefield", "Whitefield");
        assert!(graph_area_match(
            "prestige-lakeside",
            "whitefield",
            Some(&g)
        ));
        assert!(graph_area_match(
            "prestige-lakeside",
            "WHITEFIELD",
            Some(&g)
        ));
    }

    #[test]
    fn test_graph_area_match_substring_containment() {
        // Area "Sarjapur Road" should match intent "Sarjapur" (both >= 4 chars)
        let g = graph_with_society_in_area("sobha-neopolis", "sarjapur-road", "Sarjapur Road");
        assert!(graph_area_match("sobha-neopolis", "Sarjapur", Some(&g)));
    }

    #[test]
    fn test_graph_area_match_substring_reverse() {
        // Intent "Sarjapur Road" should match area "Sarjapur" (intent contains area)
        let g = graph_with_society_in_area("sobha-neopolis", "sarjapur", "Sarjapur");
        assert!(graph_area_match(
            "sobha-neopolis",
            "Sarjapur Road",
            Some(&g)
        ));
    }

    #[test]
    fn test_graph_area_match_short_name_blocked() {
        // Area "JP" (< 4 chars) — substring match should be blocked
        let g = graph_with_society_in_area("some-society", "jp", "JP");
        // Not an exact match either (intent is longer), so should be false
        assert!(!graph_area_match("some-society", "JP Nagar", Some(&g)));
    }

    #[test]
    fn test_graph_area_match_short_intent_blocked() {
        // Intent "HSR" (3 chars) — substring match should be blocked even if area is long
        let g = graph_with_society_in_area("some-society", "hsr-layout", "HSR Layout");
        assert!(!graph_area_match("some-society", "HSR", Some(&g)));
    }

    #[test]
    fn test_graph_area_match_short_exact_still_works() {
        // Exact match works even for short names
        let g = graph_with_society_in_area("some-society", "jp", "JP");
        assert!(graph_area_match("some-society", "JP", Some(&g)));
    }

    #[test]
    fn test_graph_area_match_no_edge() {
        // Society exists but has no SocietyInArea edge
        let mut g = KnowledgeGraph::new();
        g.add_node(Node::new(
            "society:lonely-society",
            NodeType::Society,
            "Lonely Society",
        ));
        assert!(!graph_area_match("lonely-society", "Whitefield", Some(&g)));
    }

    #[test]
    fn test_graph_area_match_no_graph() {
        assert!(!graph_area_match("prestige-lakeside", "Whitefield", None));
    }

    // ---------------------------------------------------------------
    // compute_confidence tests
    // ---------------------------------------------------------------

    /// Helper: create a SourcedFact with a given key for padding fact counts.
    fn make_fact(key: &str) -> SourcedFact {
        SourcedFact::manual(key, FactValue::Text("test".into()))
    }

    /// Helper: build a graph with a society node having given root_source and fact count.
    fn graph_with_society_node(
        slug: &str,
        root_source: Option<RootSource>,
        fact_count: usize,
    ) -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        let node_id = format!("society:{}", slug);
        let mut node = Node::new(&node_id, NodeType::Society, slug);
        node.root_source = root_source;
        for i in 0..fact_count {
            node.add_fact(make_fact(&format!("fact_{}", i)));
        }
        g.add_node(node);
        g
    }

    #[test]
    fn test_confidence_rera_many_facts_is_high() {
        let g = graph_with_society_node("well-known", Some(RootSource::Rera), 30);
        let score = compute_confidence(Some(&g), "well-known", 80.0).unwrap();
        assert_eq!(score.label, "High");
        // source=1.0*0.4 + coverage=1.0*0.2 + freshness~1.0*0.2 + match=0.8*0.2 = 0.96
        assert!(
            score.overall >= 0.7,
            "Expected High, got overall={}",
            score.overall
        );
    }

    #[test]
    fn test_confidence_discovered_few_facts_bulk_created_is_low() {
        // Discovered source (0.5) with only 2 facts and 0% graph-driven scoring.
        // All facts have same timestamp (bulk-created), so freshness capped at 0.5.
        // source=0.5*0.4=0.20 + coverage=(2/25)*0.2=0.016 + freshness=0.5*0.2=0.10 + match=0.0*0.2=0.0
        // total ~ 0.316 => "Low" (< 0.4)
        let g = graph_with_society_node("unknown", Some(RootSource::Discovered), 2);
        let score = compute_confidence(Some(&g), "unknown", 0.0).unwrap();
        assert_eq!(score.label, "Low");
        assert!(score.overall < 0.4, "Expected < 0.4, got {}", score.overall);

        // Compare: Legacy source (0.3) with 1 fact also Low
        let g2 = graph_with_society_node("legacy-sparse", Some(RootSource::Legacy), 1);
        let score2 = compute_confidence(Some(&g2), "legacy-sparse", 0.0).unwrap();
        assert_eq!(score2.label, "Low");
    }

    #[test]
    fn test_confidence_threshold_calibration() {
        // At exactly the configured coverage threshold, coverage should be 1.0.
        let fact_coverage_threshold = schema::ranking_policy().fact_coverage_threshold as usize;
        let g = graph_with_society_node(
            "calibrated",
            Some(RootSource::Legacy),
            fact_coverage_threshold,
        );
        let score = compute_confidence(Some(&g), "calibrated", 0.0).unwrap();
        let coverage_component = score
            .components
            .iter()
            .find(|c| c.dimension == "fact_coverage")
            .unwrap();
        assert!(
            (coverage_component.score - 1.0).abs() < 0.001,
            "Expected coverage=1.0 at threshold, got {}",
            coverage_component.score
        );
    }

    #[test]
    fn test_confidence_no_graph_returns_none() {
        let result = compute_confidence(None, "any-society", 0.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_confidence_unknown_society_still_returns() {
        // Society not in graph — should still return a score (low)
        let g = KnowledgeGraph::new();
        let score = compute_confidence(Some(&g), "nonexistent", 0.0).unwrap();
        assert_eq!(score.label, "Low");
    }

    #[test]
    fn test_confidence_components_sum_weights() {
        let g = graph_with_society_node("test", Some(RootSource::Rera), 10);
        let score = compute_confidence(Some(&g), "test", 50.0).unwrap();
        let total_weight: f64 = score.components.iter().map(|c| c.weight).sum();
        assert!(
            (total_weight - 1.0).abs() < 0.001,
            "Component weights should sum to 1.0, got {}",
            total_weight
        );
    }

    // ---------------------------------------------------------------
    // compute_confidence_for_detail tests (Day 71)
    // ---------------------------------------------------------------

    #[test]
    fn test_confidence_detail_uses_fact_quality() {
        // Detail page confidence uses average fact.confidence instead of graph_driven_pct.
        // Create a node with high-confidence facts (RERA facts default to 0.9 confidence).
        let mut g = KnowledgeGraph::new();
        let node_id = "society:well-enriched";
        let mut node = Node::new(node_id, NodeType::Society, "well-enriched");
        node.root_source = Some(RootSource::Rera);
        // Add facts with varying confidence
        for i in 0..10 {
            let mut fact = make_fact(&format!("fact_{}", i));
            fact.confidence = 0.8;
            // Space out timestamps so freshness cap doesn't kick in
            fact.learned_at = chrono::Utc::now() - chrono::Duration::hours(i as i64);
            node.add_fact(fact);
        }
        g.add_node(node);

        let score = compute_confidence_for_detail(Some(&g), "well-enriched").unwrap();

        // Should have "fact_source_quality" component instead of "match_quality"
        let fsq = score
            .components
            .iter()
            .find(|c| c.dimension == "fact_source_quality");
        assert!(fsq.is_some(), "Should have fact_source_quality component");

        let mq = score
            .components
            .iter()
            .find(|c| c.dimension == "match_quality");
        assert!(mq.is_none(), "Should NOT have match_quality component");

        // fact_source_quality should be ~0.8 (all facts have 0.8 confidence)
        let fsq = fsq.unwrap();
        assert!(
            (fsq.score - 0.8).abs() < 0.01,
            "Expected ~0.8, got {}",
            fsq.score
        );

        // Overall should be High (RERA + good coverage + fresh + high fact quality)
        assert_eq!(score.label, "High");
    }

    #[test]
    fn test_freshness_capped_for_bulk_created_nodes() {
        // All facts created at the same timestamp — freshness should be capped at 0.5
        let g = graph_with_society_node("bulk-created", Some(RootSource::Discovered), 5);
        let score = compute_confidence_for_detail(Some(&g), "bulk-created").unwrap();

        let freshness = score
            .components
            .iter()
            .find(|c| c.dimension == "freshness")
            .unwrap();
        assert!(
            freshness.score <= 0.5,
            "Bulk-created freshness should be capped at 0.5, got {}",
            freshness.score
        );
        assert!(
            freshness.explanation.contains("bulk-created cap"),
            "Explanation should mention bulk-created cap: {}",
            freshness.explanation
        );
    }

    #[test]
    fn test_freshness_not_capped_after_enrichment() {
        // Facts with different timestamps (spread over hours) — freshness should NOT be capped
        let mut g = KnowledgeGraph::new();
        let node_id = "society:enriched-over-time";
        let mut node = Node::new(node_id, NodeType::Society, "enriched-over-time");
        node.root_source = Some(RootSource::Rera);
        for i in 0..5 {
            let mut fact = make_fact(&format!("fact_{}", i));
            // Space facts apart by 2 seconds each
            fact.learned_at = chrono::Utc::now() - chrono::Duration::seconds(i as i64 * 2);
            node.add_fact(fact);
        }
        g.add_node(node);

        let score = compute_confidence_for_detail(Some(&g), "enriched-over-time").unwrap();

        let freshness = score
            .components
            .iter()
            .find(|c| c.dimension == "freshness")
            .unwrap();
        assert!(
            freshness.score > 0.5,
            "Enriched-over-time freshness should NOT be capped, got {}",
            freshness.score
        );
        // Should be 1.0 since they're all within the last 7 days
        assert!(
            (freshness.score - 1.0).abs() < 0.01,
            "Expected freshness ~1.0 for recent diverse timestamps, got {}",
            freshness.score
        );
    }

    #[test]
    fn test_freshness_single_fact_not_capped() {
        // A node with only 1 fact should NOT be capped (need >= 2 facts for bulk detection)
        let g = graph_with_society_node("single-fact", Some(RootSource::Discovered), 1);
        let score = compute_confidence_for_detail(Some(&g), "single-fact").unwrap();

        let freshness = score
            .components
            .iter()
            .find(|c| c.dimension == "freshness")
            .unwrap();
        // Single fact => all_facts_same_timestamp returns true, but n.facts.len() < 2 guard prevents cap
        assert!(
            freshness.score > 0.5,
            "Single-fact node should not be capped, got {}",
            freshness.score
        );
    }

    fn local_property(
        id: &str,
        area: &str,
        society_id: &str,
        bhk: u32,
        price: u64,
        metro_distance_mins: u32,
        noise_score: f64,
    ) -> crate::models::Property {
        crate::models::Property {
            id: id.to_string(),
            title: format!("{} BHK apartment", bhk),
            area: area.to_string(),
            area_id: area.to_lowercase().replace(' ', "-"),
            city: "Bengaluru".to_string(),
            society_id: society_id.to_string(),
            builder_name: "Test Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk,
            price,
            price_per_sqft: 12_000,
            carpet_area_sqft: 1_200,
            super_builtup_sqft: 1_550,
            floor: 8,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready to Move".to_string(),
            metro_distance_mins,
            maintenance_cost_monthly: 6_000,
            society_quality_score: Some(0.7),
            builder_quality_score: Some(0.7),
            document_completeness_score: Some(0.8),
            litigation_risk: Some(0.1),
            noise_score: Some(noise_score),
            sunlight_score: Some(0.7),
            airport_noise_score: Some(0.1),
            waterlogging_risk_score: Some(0.1),
            traffic_score: Some(0.6),
            days_on_market: 20,
            greenery_score: Some(0.6),
            open_space_score: Some(0.6),
            resale_strength_score: Some(0.7),
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: "Local test listing".to_string(),
            transparency_tags: Vec::new(),
            source_reference: "unit-test".to_string(),
        }
    }

    fn local_society_names(
        properties: &[crate::models::Property],
    ) -> std::collections::HashMap<String, String> {
        properties
            .iter()
            .map(|p| (p.society_id.clone(), format!("{} Society", p.society_id)))
            .collect()
    }

    fn graph_with_society_facts(slug: &str, facts: Vec<SourcedFact>) -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();
        let mut node = Node::new(format!("society:{}", slug), NodeType::Society, slug);
        node.add_facts(facts);
        graph.add_node(node);
        graph
    }

    fn add_society_facts(graph: &mut KnowledgeGraph, slug: &str, facts: Vec<SourcedFact>) {
        let mut node = Node::new(format!("society:{}", slug), NodeType::Society, slug);
        node.add_facts(facts);
        graph.add_node(node);
    }

    fn numeric_fact(key: &str, value: f64) -> SourcedFact {
        SourcedFact::manual(key, FactValue::Numeric(value))
    }

    fn rera_numeric_fact(key: &str, value: f64) -> SourcedFact {
        let mut fact = numeric_fact(key, value);
        fact.source.source_type = SourceType::Rera;
        fact.confidence = 1.0;
        fact
    }

    fn tags_fact(key: &str, tags: Vec<&str>) -> SourcedFact {
        SourcedFact::manual(
            key,
            FactValue::Tags(tags.into_iter().map(str::to_string).collect()),
        )
    }

    fn google_text_fact(key: &str, value: &str) -> SourcedFact {
        let mut fact = SourcedFact::manual(key, FactValue::Text(value.to_string()));
        fact.source.source_type = SourceType::Google;
        fact.confidence = 0.7;
        fact.display_template = Some("{value}".to_string());
        fact
    }

    fn preference_fact(
        key: &str,
        value: FactValue,
        answers: Vec<&str>,
        direction: ScoringDirection,
        weight: f32,
        thresholds: Vec<f64>,
    ) -> SourcedFact {
        use crate::knowledge::fact::ScoringHint;

        let mut fact = SourcedFact::manual(key, value);
        fact.confidence = 0.85;
        fact.display_template = Some("{value}".to_string());
        fact.answers_preferences = answers.into_iter().map(str::to_string).collect();
        fact.scoring_hint = Some(ScoringHint {
            direction,
            weight,
            thresholds,
        });
        fact
    }

    #[allow(clippy::too_many_arguments)]
    fn serving_fact(
        society_id: &str,
        fact_key: &str,
        value: FactValue,
        source_type: &str,
        confidence: f32,
    ) -> ServingFactRecord {
        let value_type = match value {
            FactValue::Numeric(_) => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
            FactValue::Score { .. } => "score",
        };
        ServingFactRecord {
            entity_id: format!("society:{society_id}"),
            fact_key: fact_key.to_string(),
            value_type: value_type.to_string(),
            value_text: None,
            value,
            confidence,
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: Some("unit-test".to_string()),
            learned_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
        }
    }

    fn serving_metadata(
        society_id: &str,
        fact_key: &str,
        answers_preferences: Vec<&str>,
        scoring_direction: &str,
        scoring_weight: f32,
        scoring_thresholds: Vec<f64>,
    ) -> ServingSearchMetadataRecord {
        ServingSearchMetadataRecord {
            entity_id: format!("society:{society_id}"),
            fact_key: fact_key.to_string(),
            display_template: Some("{value}".to_string()),
            answers_preferences: answers_preferences
                .into_iter()
                .map(str::to_string)
                .collect(),
            scoring_direction: Some(scoring_direction.to_string()),
            scoring_weight: Some(scoring_weight),
            scoring_thresholds,
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

    fn serving_entity_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        source_type: &str,
        confidence: f32,
    ) -> ServingFactRecord {
        let value_type = match value {
            FactValue::Numeric(_) => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
            FactValue::Score { .. } => "score",
        };
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: value_type.to_string(),
            value_text: None,
            value,
            confidence,
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: Some("unit-test".to_string()),
            learned_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn test_search_index_recalls_hard_constraint_candidates_without_network() {
        let properties = vec![
            local_property(
                "whitefield-fit",
                "Whitefield",
                "whitefield-fit",
                3,
                19_000_000,
                8,
                0.2,
            ),
            local_property(
                "bellandur-leak",
                "Bellandur",
                "bellandur-leak",
                3,
                18_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-over-budget",
                "Whitefield",
                "whitefield-over-budget",
                3,
                21_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-wrong-bhk",
                "Whitefield",
                "whitefield-wrong-bhk",
                2,
                15_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-unknown-price",
                "Whitefield",
                "whitefield-unknown-price",
                3,
                0,
                8,
                0.2,
            ),
        ];
        let index = crate::search::SearchIndex::build(&properties);
        let mut intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");
        intent.area = Some("Whitefield".to_string());

        let ids = index.recall_ids("3BHK Whitefield under 2Cr", &intent);

        assert_eq!(ids, vec!["whitefield-fit"]);
    }

    #[test]
    fn test_search_hard_filters_area_bhk_and_budget_without_network() {
        let properties = vec![
            local_property(
                "whitefield-fit",
                "Whitefield",
                "whitefield-fit",
                3,
                19_000_000,
                8,
                0.2,
            ),
            local_property(
                "bellandur-leak",
                "Bellandur",
                "bellandur-leak",
                3,
                18_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-over-budget",
                "Whitefield",
                "whitefield-over-budget",
                3,
                21_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-wrong-bhk",
                "Whitefield",
                "whitefield-wrong-bhk",
                2,
                15_000_000,
                8,
                0.2,
            ),
            local_property(
                "whitefield-unknown-price",
                "Whitefield",
                "whitefield-unknown-price",
                3,
                0,
                8,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let mut intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");
        intent.area = Some("Whitefield".to_string());

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
        );

        let ids: Vec<&str> = results.iter().map(|r| r.card.id.as_str()).collect();
        assert_eq!(ids, vec!["whitefield-fit"]);
    }

    #[test]
    fn test_budget_filter_treats_zero_price_as_unknown_not_free() {
        let properties = vec![
            local_property(
                "priced-fit",
                "Whitefield",
                "priced-fit",
                3,
                19_000_000,
                8,
                0.2,
            ),
            local_property("unknown-price", "Whitefield", "unknown-price", 3, 0, 8, 0.2),
        ];
        let society_names = local_society_names(&properties);
        let index = crate::search::SearchIndex::build(&properties);
        let mut intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");
        intent.area = Some("Whitefield".to_string());

        let results = TextSearch::search_with_index_and_intent(
            &properties,
            Some(&index),
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
        );

        let ids: Vec<&str> = results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect();
        assert_eq!(ids, vec!["priced-fit"]);
    }

    #[test]
    fn test_indexed_search_filters_candidates_before_ranking() {
        let mut noisy_unrelated = local_property(
            "bellandur-keyword-spam",
            "Bellandur",
            "bellandur-keyword-spam",
            3,
            18_000_000,
            8,
            0.2,
        );
        noisy_unrelated.title = "Whitefield 3 BHK keyword-heavy decoy".to_string();

        let properties = vec![
            noisy_unrelated,
            local_property(
                "whitefield-fit",
                "Whitefield",
                "whitefield-fit",
                3,
                19_000_000,
                8,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let index = crate::search::SearchIndex::build(&properties);
        let mut intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");
        intent.area = Some("Whitefield".to_string());

        let results = TextSearch::search_with_index_and_intent(
            &properties,
            Some(&index),
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
        );

        let ids: Vec<&str> = results.iter().map(|r| r.card.id.as_str()).collect();
        assert_eq!(ids, vec!["whitefield-fit"]);
    }

    #[test]
    fn test_serving_recall_expands_candidates_but_keeps_hard_filters() {
        let indexed_fit = local_property(
            "indexed-fit",
            "Whitefield",
            "indexed-fit",
            3,
            19_000_000,
            8,
            0.2,
        );
        let bundle_fit = local_property(
            "bundle-fit",
            "Whitefield",
            "bundle-fit",
            3,
            18_000_000,
            8,
            0.2,
        );
        let bundle_over_budget = local_property(
            "bundle-over-budget",
            "Whitefield",
            "bundle-over-budget",
            3,
            21_000_000,
            8,
            0.2,
        );
        let bundle_unknown_price = local_property(
            "bundle-unknown-price",
            "Whitefield",
            "bundle-unknown-price",
            3,
            0,
            8,
            0.2,
        );
        let properties = vec![
            indexed_fit.clone(),
            bundle_fit,
            bundle_over_budget,
            bundle_unknown_price,
        ];
        let society_names = local_society_names(&properties);
        let stale_local_index = crate::search::SearchIndex::build(&[indexed_fit]);
        let serving_candidate_ids = vec![
            "bundle-fit".to_string(),
            "bundle-over-budget".to_string(),
            "bundle-unknown-price".to_string(),
        ];
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");

        let results = TextSearch::search_with_index_and_extra_recall_and_intent(
            &properties,
            Some(&stale_local_index),
            Some(&serving_candidate_ids),
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
        );

        let ids: Vec<&str> = results.iter().map(|r| r.card.id.as_str()).collect();
        assert_eq!(ids, vec!["indexed-fit", "bundle-fit"]);
    }

    #[test]
    fn test_search_ranks_graph_preference_fit_before_weaker_match() {
        let properties = vec![
            local_property(
                "quiet-near-metro",
                "Whitefield",
                "quiet-near-metro",
                3,
                19_000_000,
                5,
                0.2,
            ),
            local_property(
                "noisy-far-from-metro",
                "Whitefield",
                "noisy-far-from-metro",
                3,
                19_000_000,
                25,
                0.7,
            ),
        ];
        let society_names = local_society_names(&properties);
        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "quiet-near-metro",
            vec![
                preference_fact(
                    "metro_distance_mins",
                    FactValue::Numeric(5.0),
                    vec!["metro access"],
                    ScoringDirection::LowerIsBetter,
                    2.0,
                    vec![10.0, 20.0],
                ),
                preference_fact(
                    "noise_score",
                    FactValue::Numeric(0.2),
                    vec!["quiet neighborhood"],
                    ScoringDirection::LowerIsBetter,
                    2.0,
                    vec![0.3, 0.5],
                ),
            ],
        );
        add_society_facts(
            &mut graph,
            "noisy-far-from-metro",
            vec![
                preference_fact(
                    "metro_distance_mins",
                    FactValue::Numeric(25.0),
                    vec!["metro access"],
                    ScoringDirection::LowerIsBetter,
                    2.0,
                    vec![10.0, 20.0],
                ),
                preference_fact(
                    "noise_score",
                    FactValue::Numeric(0.7),
                    vec!["quiet neighborhood"],
                    ScoringDirection::LowerIsBetter,
                    2.0,
                    vec![0.3, 0.5],
                ),
            ],
        );
        let intent =
            crate::search::intent::parse_intent("quiet 3BHK near metro in Whitefield under 2Cr");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "quiet 3BHK near metro in Whitefield under 2Cr",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "quiet-near-metro");
        assert!(
            results[0].match_score > results[1].match_score,
            "preference-fit property should score higher than weaker match"
        );

        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("preference query should include structured explanation");
        assert!(
            explanation
                .reasons
                .iter()
                .any(|r| r.preference == "metro access" && r.scoring_method == "graph"),
            "metro preference should be explained by graph scoring"
        );
        assert!(
            explanation
                .reasons
                .iter()
                .any(|r| r.preference == "quiet neighborhood" && r.scoring_method == "graph"),
            "quiet preference should be explained by graph scoring"
        );
    }

    #[test]
    fn test_zero_minute_metro_seed_value_is_not_treated_as_evidence() {
        let properties = vec![local_property(
            "dummy-metro",
            "Whitefield",
            "dummy-metro",
            3,
            19_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let intent = crate::search::intent::parse_intent("3bhk whitefield near metro");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3bhk whitefield near metro",
            &intent,
            None,
        );

        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("preference query should include structured explanation");

        assert!(
            !explanation
                .reasons
                .iter()
                .any(|reason| reason.fact_key == "metro_distance_mins"),
            "0-minute seed value should be no-data, not metro evidence: {:?}",
            explanation.reasons
        );
        assert!(
            explanation
                .preference_coverage
                .iter()
                .any(|coverage| coverage.preference == "metro access"
                    && coverage.status == "no_data"),
            "metro preference should be marked no_data when only seed value is zero: {:?}",
            explanation.preference_coverage
        );
    }

    #[test]
    fn specific_nearby_intent_requires_matching_category_evidence() {
        let properties = vec![
            local_property(
                "nearby-metro-fit",
                "Pattandur Agrahara",
                "nearby-metro-fit",
                3,
                32_000_000,
                0,
                0.2,
            ),
            local_property(
                "exact-area-wrong-category",
                "Whitefield",
                "exact-area-wrong-category",
                3,
                18_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "nearby-metro-fit",
                    "nearby_metro_stations",
                    FactValue::Text(
                        "Nearby metro: Kadugodi Tree Park (0.7 km, 4.5 rating, 1509 reviews)"
                            .to_string(),
                    ),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "exact-area-wrong-category",
                    "nearby_schools",
                    FactValue::Text("Nearby schools: Example School (0.1 km)".to_string()),
                    "Google",
                    0.9,
                ),
            ],
            vec![
                serving_metadata(
                    "nearby-metro-fit",
                    "nearby_metro_stations",
                    vec!["near metro", "metro access", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
                serving_metadata(
                    "exact-area-wrong-category",
                    "nearby_schools",
                    vec!["schools", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
            ],
        );
        let intent = crate::search::intent::parse_intent("near metro whitefield");
        let social_keys = intent
            .positive_preferences
            .iter()
            .find(|signal| signal.raw_text == "social infrastructure")
            .map(|signal| signal.expanded_keys.as_slice())
            .unwrap_or(&[]);
        assert!(
            social_keys.iter().any(|key| key == "nearby_metro_stations"),
            "metro-specific social infra keys should include nearby metro: {:?}",
            social_keys
        );
        assert!(
            !social_keys.iter().any(|key| key == "nearby_schools"),
            "metro-specific social infra keys should not accept school evidence: {:?}",
            social_keys
        );

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            "near metro whitefield",
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "nearby-metro-fit");
        assert_eq!(results[0].match_label, "Good match");
        let top_reasons = &results[0].match_explanation.as_ref().unwrap().reasons;
        assert!(
            top_reasons
                .iter()
                .any(|reason| reason.fact_key == "nearby_metro_stations"
                    && reason.scoring_method == geo::GEO_DISTANCE_SCORING_METHOD),
            "top result should be backed by metro evidence: {:?}",
            top_reasons
        );
        let wrong_category = results
            .iter()
            .find(|result| result.card.id == "exact-area-wrong-category")
            .expect("wrong-category result should still be returned as area match");
        let wrong_reasons = wrong_category
            .match_explanation
            .as_ref()
            .map(|explanation| explanation.reasons.as_slice())
            .unwrap_or(&[]);
        assert!(
            !wrong_reasons
                .iter()
                .any(|reason| reason.fact_key == "nearby_schools"),
            "school evidence should not prove a metro query: {:?}",
            wrong_reasons
        );
    }

    #[test]
    fn multi_place_family_query_keeps_each_requested_fact_family() {
        let query = "near metro and schools in whitefield";

        assert!(
            !place_fact_conflicts_with_explicit_query_family(query, "nearby_metro_stations"),
            "explicit metro request should keep metro evidence"
        );
        assert!(
            !place_fact_conflicts_with_explicit_query_family(query, "nearby_schools"),
            "explicit school request should keep school evidence"
        );
        assert!(
            place_fact_conflicts_with_explicit_query_family(query, "nearby_hospitals"),
            "unrequested sibling place category should still be suppressed"
        );
    }

    #[test]
    fn nearby_distance_scoring_orders_closer_place_evidence() {
        let properties = vec![
            local_property(
                "close-metro",
                "Whitefield",
                "close-metro",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "farther-metro",
                "Whitefield",
                "farther-metro",
                3,
                18_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "close-metro",
                    "nearby_metro_stations",
                    FactValue::Text(
                        "Nearby metro: Kadugodi Tree Park (0.6 km, 4.5 rating, 1509 reviews)"
                            .to_string(),
                    ),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "farther-metro",
                    "nearby_metro_stations",
                    FactValue::Text(
                        "Nearby metro: Kadugodi Tree Park (2.8 km, 4.5 rating, 1509 reviews)"
                            .to_string(),
                    ),
                    "Google",
                    0.9,
                ),
            ],
            vec![
                serving_metadata(
                    "close-metro",
                    "nearby_metro_stations",
                    vec!["near metro", "metro access", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
                serving_metadata(
                    "farther-metro",
                    "nearby_metro_stations",
                    vec!["near metro", "metro access", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
            ],
        );
        let intent = crate::search::intent::parse_intent("near metro whitefield");

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            "near metro whitefield",
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "close-metro");
        assert!(
            results[0].match_score > results[1].match_score,
            "closer distance should produce stronger score: {:?}",
            results
                .iter()
                .map(|result| (&result.card.id, result.match_score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn named_place_haversine_search_orders_societies_by_computed_distance() {
        let properties = vec![
            local_property(
                "close-to-kadugodi",
                "Whitefield",
                "close-to-kadugodi",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "farther-from-kadugodi",
                "Whitefield",
                "farther-from-kadugodi",
                3,
                18_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![serving_entity(
            "place:google:kadugodi-tree-park",
            "place",
            "Kadugodi Tree Park Metro Station",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    "society:close-to-kadugodi",
                    "geo.latitude",
                    FactValue::Numeric(12.9857),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:close-to-kadugodi",
                    "geo.longitude",
                    FactValue::Numeric(77.7468),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:farther-from-kadugodi",
                    "geo.latitude",
                    FactValue::Numeric(13.0050),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:farther-from-kadugodi",
                    "geo.longitude",
                    FactValue::Numeric(77.7700),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "place.name",
                    FactValue::Text("Kadugodi Tree Park Metro Station".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "geo.latitude",
                    FactValue::Numeric(12.985711),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "geo.longitude",
                    FactValue::Numeric(77.746842),
                    "Google",
                    0.9,
                ),
            ],
            Vec::new(),
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("3bhk near Kadugodi Tree Park")
            .expect("query should resolve the named place");
        let geo_candidate_ids = geo_query.candidate_property_ids(&properties);
        let intent = crate::search::intent::parse_intent("3bhk near Kadugodi Tree Park");

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&geo_candidate_ids),
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            "3bhk near Kadugodi Tree Park",
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "close-to-kadugodi");
        assert!(
            results[0].match_score > results[1].match_score,
            "closer society should score higher: {:?}",
            results
                .iter()
                .map(|result| (&result.card.id, result.match_score))
                .collect::<Vec<_>>()
        );
        let reasons = &results[0]
            .match_explanation
            .as_ref()
            .expect("named-place query should produce computed evidence")
            .reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.fact_key == geo::DISTANCE_TO_PLACE_FACT_KEY
                    && reason.scoring_method == geo::HAVERSINE_SCORING_METHOD
                    && reason.display.contains("Kadugodi Tree Park")
            }),
            "top result should include Haversine distance evidence: {:?}",
            reasons
        );
    }

    #[test]
    fn named_place_terms_do_not_keyword_boost_society_names() {
        let properties = vec![
            local_property(
                "close-home",
                "Whitefield",
                "soc-close-home",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "far-kadugodi-tree-park-view",
                "Whitefield",
                "soc-kadugodi-tree-park-view",
                3,
                18_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![serving_entity(
            "place:google:kadugodi-tree-park",
            "place",
            "Kadugodi Tree Park",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    "society:close-home",
                    "geo.latitude",
                    FactValue::Numeric(12.9857),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:close-home",
                    "geo.longitude",
                    FactValue::Numeric(77.7468),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:kadugodi-tree-park-view",
                    "geo.latitude",
                    FactValue::Numeric(13.0050),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:kadugodi-tree-park-view",
                    "geo.longitude",
                    FactValue::Numeric(77.7700),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "place.name",
                    FactValue::Text("Kadugodi Tree Park".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "geo.latitude",
                    FactValue::Numeric(12.985711),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:kadugodi-tree-park",
                    "geo.longitude",
                    FactValue::Numeric(77.746842),
                    "Google",
                    0.9,
                ),
            ],
            Vec::new(),
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        for query in [
            "3bhk near Kadugodi Tree Park",
            "3bhk near Kadugodi, Tree-Park",
        ] {
            let geo_query = geo_index
                .query(query)
                .expect("query should resolve the named place");
            let geo_candidate_ids = geo_query.candidate_property_ids(&properties);
            let intent = crate::search::intent::parse_intent(query);

            let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
                &properties,
                None,
                Some(&geo_candidate_ids),
                Some(&geo_query),
                Some(&serving_facts),
                &society_names,
                &[],
                query,
                &intent,
                None,
            );

            assert_eq!(results[0].card.id, "close-home", "query: {query}");
            assert!(
                results[0]
                    .match_explanation
                    .as_ref()
                    .expect("close result should explain geo evidence")
                    .reasons
                    .iter()
                    .any(|reason| reason.scoring_method == geo::HAVERSINE_SCORING_METHOD),
                "close result should include Haversine evidence for {query}: {:?}",
                results[0].match_explanation
            );
        }
    }

    #[test]
    fn named_place_query_prefers_exact_serving_nearby_fact_over_coordinate_only_match() {
        let properties = vec![
            local_property(
                "waterford",
                "Whitefield",
                "waterford",
                3,
                26_000_000,
                0,
                0.2,
            ),
            local_property(
                "coordinate-only",
                "Whitefield",
                "coordinate-only",
                3,
                26_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![serving_entity(
            "place:google:deens-academy",
            "place",
            "The Deens Academy",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    "society:waterford",
                    "geo.latitude",
                    FactValue::Numeric(12.9900),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:waterford",
                    "geo.longitude",
                    FactValue::Numeric(77.7500),
                    "Google",
                    1.0,
                ),
                serving_fact(
                    "waterford",
                    "nearby_schools",
                    FactValue::Text("Nearby schools: The Deens Academy (0.7 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "society:coordinate-only",
                    "geo.latitude",
                    FactValue::Numeric(12.9858),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:coordinate-only",
                    "geo.longitude",
                    FactValue::Numeric(77.7469),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "place:google:deens-academy",
                    "geo.latitude",
                    FactValue::Numeric(12.9857),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:deens-academy",
                    "geo.longitude",
                    FactValue::Numeric(77.7468),
                    "Google",
                    0.9,
                ),
            ],
            vec![serving_metadata(
                "waterford",
                "nearby_schools",
                vec!["schools", "school access", "social infrastructure"],
                "TextMatch",
                1.2,
                Vec::new(),
            )],
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("whitefield home near deens academy")
            .expect("query should resolve Deens Academy");
        let geo_candidate_ids = geo_query.candidate_property_ids(&properties);
        let intent = crate::search::intent::parse_intent("whitefield home near deens academy");

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&geo_candidate_ids),
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            "whitefield home near deens academy",
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "waterford");
        let reasons = &results[0]
            .match_explanation
            .as_ref()
            .expect("named-place query should explain exact nearby fact")
            .reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.fact_key == "nearby_schools"
                    && reason.scoring_method == geo::NAMED_PLACE_FACT_SCORING_METHOD
                    && reason.display.contains("The Deens Academy")
            }),
            "top result should be backed by exact nearby school fact: {:?}",
            reasons
        );
        assert!(
            !reasons.iter().any(|reason| {
                reason.fact_key == "nearby_schools"
                    && reason.scoring_method == geo::GEO_DISTANCE_SCORING_METHOD
            }),
            "named-place proof should not be duplicated by generic preference proof for the same fact key: {:?}",
            reasons
        );
    }

    #[test]
    fn named_place_distance_limit_filters_serving_nearby_fact() {
        let entities = vec![serving_entity(
            "place:google:deens-academy",
            "place",
            "The Deens Academy",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "outside-limit",
                    "nearby_schools",
                    FactValue::Text("Nearby schools: The Deens Academy (0.7 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:deens-academy",
                    "geo.latitude",
                    FactValue::Numeric(12.9857),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:deens-academy",
                    "geo.longitude",
                    FactValue::Numeric(77.7468),
                    "Google",
                    0.9,
                ),
            ],
            vec![serving_metadata(
                "outside-limit",
                "nearby_schools",
                vec!["schools", "school access", "social infrastructure"],
                "TextMatch",
                1.2,
                Vec::new(),
            )],
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let tight_query = geo_index
            .query("homes within 500m of deens academy")
            .expect("query should resolve Deens Academy");
        let loose_query = geo_index
            .query("homes within 1 km of deens academy")
            .expect("query should resolve Deens Academy");
        let postposed_tight_query = geo_index
            .query("homes near deens academy within 500m")
            .expect("postposed distance should stay attached to Deens Academy");

        assert!(
            serving_named_place_evidence(&serving_facts, "outside-limit", &tight_query).is_empty(),
            "0.7 km serving fact should not satisfy a 500m query"
        );
        assert!(
            !serving_named_place_evidence(&serving_facts, "outside-limit", &loose_query).is_empty(),
            "0.7 km serving fact should satisfy a 1 km query"
        );
        assert!(
            serving_named_place_evidence(&serving_facts, "outside-limit", &postposed_tight_query,)
                .is_empty(),
            "0.7 km serving fact should not satisfy a postposed 500m query"
        );
    }

    #[test]
    fn named_lake_uses_lake_fact_instead_of_same_name_park_fact() {
        let place_id = "place:osm:begur-lake";
        let entities = vec![serving_entity(place_id, "place", "Begur Lake")];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    place_id,
                    "place.category",
                    FactValue::Text("lake".to_string()),
                    "OpenStreetMap",
                    1.0,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.latitude",
                    FactValue::Numeric(12.88),
                    "OpenStreetMap",
                    1.0,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.longitude",
                    FactValue::Numeric(77.62),
                    "OpenStreetMap",
                    1.0,
                ),
                serving_fact(
                    "lake-home",
                    "nearby_public_parks",
                    FactValue::Text("Nearby parks: Begur Lake (0.1 km)".to_string()),
                    "Google",
                    0.99,
                ),
                serving_fact(
                    "lake-home",
                    "nearby_lakes",
                    FactValue::Text("Nearby lakes: Begur Lake (0.8 km)".to_string()),
                    "OpenStreetMap",
                    0.9,
                ),
            ],
            Vec::new(),
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("homes near Begur Lake")
            .expect("named lake should resolve");

        let evidence = serving_named_place_evidence(&serving_facts, "lake-home", &geo_query)
            .into_iter()
            .next()
            .expect("direct lake fact should provide proof");

        assert_eq!(evidence.fact_key, "nearby_lakes");
        assert_eq!(evidence.place_entity_id, place_id);
    }

    #[test]
    fn named_stormwater_drain_uses_direct_risk_fact() {
        let place_id = "place:stormwater-drain:way-1";
        let entities = vec![serving_entity(place_id, "place", "drain")];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    place_id,
                    "place.category",
                    FactValue::Text("stormwater_drain".to_string()),
                    "OpenStreetMap",
                    0.74,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.latitude",
                    FactValue::Numeric(12.87),
                    "OpenStreetMap",
                    0.74,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.longitude",
                    FactValue::Numeric(77.62),
                    "OpenStreetMap",
                    0.74,
                ),
                serving_fact(
                    "southern-star",
                    "stormwater_drain_nearby",
                    FactValue::Text("drain (237 m, stormwater_drain, severity: info)".to_string()),
                    "OpenStreetMap",
                    0.74,
                ),
            ],
            Vec::new(),
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("Prestige Southern Star near stormwater drain")
            .expect("named stormwater drain should resolve");

        let evidence = serving_named_place_evidence(&serving_facts, "southern-star", &geo_query)
            .into_iter()
            .next()
            .expect("direct stormwater fact should provide proof");

        assert_eq!(evidence.fact_key, "stormwater_drain_nearby");
        assert_eq!(evidence.place_entity_id, place_id);
        assert_eq!(distance_m(evidence.distance_km), Some(237));
    }

    #[test]
    fn named_metro_ignores_hospital_fact_with_same_locality_token() {
        let place_id = "place:google:bommanahalli-metro";
        let entities = vec![serving_entity(
            place_id,
            "place",
            "Bommanahalli Metro Station",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    place_id,
                    "place.category",
                    FactValue::Text("metro_station".to_string()),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.latitude",
                    FactValue::Numeric(12.9),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    place_id,
                    "geo.longitude",
                    FactValue::Numeric(77.61),
                    "Google",
                    1.0,
                ),
                serving_fact(
                    "metro-home",
                    "nearby_hospitals",
                    FactValue::Text("Nearby hospital: Bommanahalli Hospital (0.1 km)".to_string()),
                    "Google",
                    0.99,
                ),
                serving_fact(
                    "metro-home",
                    "nearby_metro_stations",
                    FactValue::Text(
                        "Nearby metro: Bommanahalli Metro Station (1.0 km)".to_string(),
                    ),
                    "Google",
                    0.9,
                ),
            ],
            Vec::new(),
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("homes near Bommanahalli Metro Station")
            .expect("named metro should resolve");

        let evidence = serving_named_place_evidence(&serving_facts, "metro-home", &geo_query)
            .into_iter()
            .next()
            .expect("direct metro fact should provide proof");

        assert_eq!(evidence.fact_key, "nearby_metro_stations");
        assert_eq!(evidence.place_entity_id, place_id);
    }

    #[test]
    fn specific_school_fact_beats_context_locality_distance_for_multi_place_query() {
        let properties = vec![
            local_property("school-fit", "Hoodi", "school-fit", 3, 20_000_000, 0, 0.2),
            local_property("hoodi-only", "Hoodi", "hoodi-only", 3, 20_000_000, 0, 0.2),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![
            serving_entity(
                "place:google:gopalan-national-school",
                "place",
                "Gopalan National School",
            ),
            serving_entity("place:google:hoodi", "place", "Hoodi"),
        ];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_entity_fact(
                    "society:school-fit",
                    "geo.latitude",
                    FactValue::Numeric(12.9960),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:school-fit",
                    "geo.longitude",
                    FactValue::Numeric(77.7200),
                    "Google",
                    1.0,
                ),
                serving_fact(
                    "school-fit",
                    "nearby_schools",
                    FactValue::Text("Nearby schools: Gopalan National School (0.5 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "society:hoodi-only",
                    "geo.latitude",
                    FactValue::Numeric(12.9919),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "society:hoodi-only",
                    "geo.longitude",
                    FactValue::Numeric(77.7152),
                    "Google",
                    1.0,
                ),
                serving_entity_fact(
                    "place:google:gopalan-national-school",
                    "geo.latitude",
                    FactValue::Numeric(12.9961),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:gopalan-national-school",
                    "geo.longitude",
                    FactValue::Numeric(77.7201),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:hoodi",
                    "geo.latitude",
                    FactValue::Numeric(12.9918),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:hoodi",
                    "geo.longitude",
                    FactValue::Numeric(77.7151),
                    "Google",
                    0.9,
                ),
            ],
            vec![serving_metadata(
                "school-fit",
                "nearby_schools",
                vec!["schools", "school access", "social infrastructure"],
                "TextMatch",
                1.2,
                Vec::new(),
            )],
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("apartment near gopalan national school hoodi")
            .expect("query should resolve school and Hoodi");
        let geo_candidate_ids = geo_query.candidate_property_ids(&properties);
        let intent =
            crate::search::intent::parse_intent("apartment near gopalan national school hoodi");

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&geo_candidate_ids),
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            "apartment near gopalan national school hoodi",
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "school-fit");
        let reasons = &results[0]
            .match_explanation
            .as_ref()
            .expect("specific school query should explain exact nearby fact")
            .reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.fact_key == "nearby_schools"
                    && reason.scoring_method == geo::NAMED_PLACE_FACT_SCORING_METHOD
                    && reason.display.contains("Gopalan National School")
            }),
            "specific school fact should be used ahead of locality distance: {:?}",
            reasons
        );
    }

    #[test]
    fn multiple_named_anchors_emit_independent_reasons_and_proof_focuses() {
        let properties = vec![
            local_property("both", "Whitefield", "both", 3, 20_000_000, 0, 0.2),
            local_property(
                "hospital-only",
                "Whitefield",
                "hospital-only",
                3,
                20_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let hospital_id = "place:google:manipal";
        let office_id = "place:google:itpb";
        let entities = vec![
            serving_entity(hospital_id, "place", "Manipal Hospital Whitefield"),
            serving_entity(office_id, "place", "International Tech Park Bengaluru ITPB"),
        ];
        let mut facts = vec![
            serving_entity_fact(
                hospital_id,
                "place.category",
                FactValue::Text("hospital".to_string()),
                "Google",
                1.0,
            ),
            serving_entity_fact(
                hospital_id,
                "geo.latitude",
                FactValue::Numeric(12.99),
                "Google",
                1.0,
            ),
            serving_entity_fact(
                hospital_id,
                "geo.longitude",
                FactValue::Numeric(77.72),
                "Google",
                1.0,
            ),
            serving_entity_fact(
                office_id,
                "place.category",
                FactValue::Text("tech_park".to_string()),
                "Google",
                1.0,
            ),
            serving_entity_fact(
                office_id,
                "geo.latitude",
                FactValue::Numeric(12.98),
                "Google",
                1.0,
            ),
            serving_entity_fact(
                office_id,
                "geo.longitude",
                FactValue::Numeric(77.73),
                "Google",
                1.0,
            ),
            serving_fact(
                "both",
                "nearby_hospitals",
                FactValue::Text(
                    "Nearby hospitals: Manipal Hospital Whitefield (1.6 km)".to_string(),
                ),
                "Google",
                0.95,
            ),
            serving_fact(
                "both",
                "nearby_tech_parks",
                FactValue::Text(
                    "Nearby tech parks: International Tech Park Bengaluru ITPB (0.8 km)"
                        .to_string(),
                ),
                "Google",
                0.95,
            ),
            serving_fact(
                "hospital-only",
                "nearby_hospitals",
                FactValue::Text(
                    "Nearby hospitals: Manipal Hospital Whitefield (1.0 km)".to_string(),
                ),
                "Google",
                0.95,
            ),
        ];
        facts.push(serving_entity_fact(
            "society:both",
            "geo.latitude",
            FactValue::Numeric(12.98),
            "Google",
            1.0,
        ));
        facts.push(serving_entity_fact(
            "society:both",
            "geo.longitude",
            FactValue::Numeric(77.73),
            "Google",
            1.0,
        ));
        let serving_facts = ServingFactIndex::from_records(facts, Vec::new());
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let query =
            "3BHK near Manipal Hospital Whitefield and near International Tech Park Bengaluru ITPB";
        let geo_query = geo_index.query(query).expect("both anchors should resolve");
        let intent = crate::search::intent::parse_intent(query);
        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&["both".to_string(), "hospital-only".to_string()]),
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            query,
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "both");
        assert!(
            results[0]
                .match_reason
                .contains("Near Manipal Hospital Whitefield"),
            "card reason should retain the hospital anchor: {}",
            results[0].match_reason
        );
        assert!(
            results[0]
                .match_reason
                .contains("Near International Tech Park Bengaluru ITPB"),
            "card reason should retain the tech-park anchor: {}",
            results[0].match_reason
        );
        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("multi-anchor result should explain both matches");
        let reason_keys = explanation
            .reasons
            .iter()
            .map(|reason| reason.fact_key.as_str())
            .collect::<Vec<_>>();
        assert!(reason_keys.contains(&"nearby_hospitals"));
        assert!(reason_keys.contains(&"nearby_tech_parks"));
        let focus_entity_ids = results[0]
            .proof_focuses
            .iter()
            .filter_map(|focus| focus.entity_id.as_deref())
            .collect::<Vec<_>>();
        assert!(focus_entity_ids.contains(&hospital_id));
        assert!(focus_entity_ids.contains(&office_id));
        let hospital_only = results
            .iter()
            .find(|result| result.card.id == "hospital-only")
            .expect("single-anchor candidate should remain in the comparison");
        assert!(hospital_only
            .match_explanation
            .as_ref()
            .is_some_and(|explanation| explanation
                .reasons
                .iter()
                .all(|reason| reason.fact_key != "nearby_tech_parks")));
        assert!(hospital_only
            .proof_focuses
            .iter()
            .all(|focus| focus.entity_id.as_deref() != Some(office_id)));
    }

    #[test]
    fn named_place_intent_expands_area_recall_and_ranks_proximity_first() {
        let properties = vec![
            local_property(
                "whitefield-keyword",
                "Whitefield",
                "whitefield-keyword",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "near-office-park",
                "Hoodi",
                "near-office-park",
                3,
                20_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![serving_entity(
            "place:google:example-office-park",
            "place",
            "Example Office Park",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "near-office-park",
                    "nearby_tech_parks",
                    FactValue::Text("Nearby tech parks: Example Office Park (0.6 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:example-office-park",
                    "geo.latitude",
                    FactValue::Numeric(12.99),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:example-office-park",
                    "geo.longitude",
                    FactValue::Numeric(77.71),
                    "Google",
                    0.9,
                ),
            ],
            vec![serving_metadata(
                "near-office-park",
                "nearby_tech_parks",
                vec!["tech parks", "office access", "social infrastructure"],
                "TextMatch",
                1.2,
                Vec::new(),
            )],
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("3bhk near example office park whitefield")
            .expect("query should resolve the named office park");
        let intent =
            crate::search::intent::parse_intent("3bhk near example office park whitefield");

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            "3bhk near example office park whitefield",
            &intent,
            None,
        );

        assert_eq!(
            results[0].card.id, "near-office-park",
            "named-place proximity should outrank exact locality text matches"
        );
        assert!(
            results[0].match_reason.contains("Near Example Office Park"),
            "card reason should surface the primary named-place intent: {}",
            results[0].match_reason
        );
        assert!(
            !results[0].match_reason.contains("Near Whitefield"),
            "named-place recall should not invent an area-proximity claim: {}",
            results[0].match_reason
        );
        let reasons = &results[0].match_explanation.as_ref().unwrap().reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.preference == "near Example Office Park"
                    && reason.fact_key == "nearby_tech_parks"
                    && reason.display.contains("Example Office Park")
            }),
            "top result should explain the named-place evidence: {:?}",
            reasons
        );
    }

    #[test]
    fn review_quality_breaks_ties_after_named_place_intent_fit() {
        let properties = vec![
            local_property(
                "lower-reviewed-fit",
                "Whitefield",
                "lower-reviewed-fit",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "better-reviewed-fit",
                "Whitefield",
                "better-reviewed-fit",
                3,
                20_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let entities = vec![serving_entity(
            "place:google:example-office-park",
            "place",
            "Example Office Park",
        )];
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "lower-reviewed-fit",
                    "nearby_tech_parks",
                    FactValue::Text("Nearby tech parks: Example Office Park (0.8 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "lower-reviewed-fit",
                    "google_rating",
                    FactValue::Numeric(3.8),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "lower-reviewed-fit",
                    "google_review_count",
                    FactValue::Numeric(80.0),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "better-reviewed-fit",
                    "nearby_tech_parks",
                    FactValue::Text("Nearby tech parks: Example Office Park (0.8 km)".to_string()),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "better-reviewed-fit",
                    "google_rating",
                    FactValue::Numeric(4.6),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "better-reviewed-fit",
                    "google_review_count",
                    FactValue::Numeric(500.0),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:example-office-park",
                    "geo.latitude",
                    FactValue::Numeric(12.99),
                    "Google",
                    0.9,
                ),
                serving_entity_fact(
                    "place:google:example-office-park",
                    "geo.longitude",
                    FactValue::Numeric(77.71),
                    "Google",
                    0.9,
                ),
            ],
            vec![
                serving_metadata(
                    "lower-reviewed-fit",
                    "nearby_tech_parks",
                    vec!["tech parks", "office access", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
                serving_metadata(
                    "better-reviewed-fit",
                    "nearby_tech_parks",
                    vec!["tech parks", "office access", "social infrastructure"],
                    "TextMatch",
                    1.2,
                    Vec::new(),
                ),
            ],
        );
        let geo_index = geo::GeoSearchIndex::from_serving_bundle(&entities, &serving_facts);
        let geo_query = geo_index
            .query("3bhk near example office park whitefield")
            .expect("query should resolve the named office park");
        let intent =
            crate::search::intent::parse_intent("3bhk near example office park whitefield");

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&geo_query),
            Some(&serving_facts),
            &society_names,
            &[],
            "3bhk near example office park whitefield",
            &intent,
            None,
        );

        assert_eq!(
            results[0].card.id, "better-reviewed-fit",
            "review quality should break ties once named-place intent fit is equal"
        );
        assert_eq!(results[0].card.google_rating, Some(4.6));
        assert_eq!(results[0].card.google_review_count, Some(500));
    }

    #[test]
    fn broad_text_queries_rank_text_fit_before_review_quality() {
        let properties = vec![
            local_property(
                "keyword-fit",
                "Whitefield",
                "keyword-fit",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "highly-reviewed",
                "Whitefield",
                "highly-reviewed",
                3,
                20_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "keyword-fit",
                    "google_rating",
                    FactValue::Numeric(3.6),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "keyword-fit",
                    "google_review_count",
                    FactValue::Numeric(30.0),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "highly-reviewed",
                    "google_rating",
                    FactValue::Numeric(4.8),
                    "Google",
                    0.9,
                ),
                serving_fact(
                    "highly-reviewed",
                    "google_review_count",
                    FactValue::Numeric(600.0),
                    "Google",
                    0.9,
                ),
            ],
            Vec::new(),
        );
        let intent = crate::search::intent::parse_intent("keyword 3bhk whitefield");

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            "keyword 3bhk whitefield",
            &intent,
            None,
        );

        assert_eq!(
            results[0].card.id, "keyword-fit",
            "broad text fit should rank before review quality when no primary intent evidence exists"
        );
        assert_eq!(results[1].card.id, "highly-reviewed");
    }

    #[test]
    fn nearby_place_fact_without_distance_does_not_prove_proximity() {
        let properties = vec![local_property(
            "undistanced-metro",
            "Whitefield",
            "undistanced-metro",
            3,
            18_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let serving_facts = ServingFactIndex::from_records(
            vec![serving_fact(
                "undistanced-metro",
                "nearby_metro_stations",
                FactValue::Text("Nearby metro: Kadugodi Tree Park station".to_string()),
                "Google",
                0.9,
            )],
            vec![serving_metadata(
                "undistanced-metro",
                "nearby_metro_stations",
                vec!["near metro", "metro access", "social infrastructure"],
                "TextMatch",
                1.2,
                Vec::new(),
            )],
        );
        let intent = crate::search::intent::parse_intent("near metro whitefield");

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            "near metro whitefield",
            &intent,
            None,
        );

        let reasons = results[0]
            .match_explanation
            .as_ref()
            .map(|explanation| explanation.reasons.as_slice())
            .unwrap_or(&[]);
        assert!(
            !reasons
                .iter()
                .any(|reason| reason.fact_key == "nearby_metro_stations"),
            "undistanced nearby facts should not prove proximity: {:?}",
            reasons
        );
    }

    #[test]
    fn good_reviews_preference_uses_google_rating_evidence() {
        let properties = vec![local_property(
            "reviewed-society",
            "Whitefield",
            "reviewed-society",
            3,
            19_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "reviewed-society",
                    "google_top_negatives",
                    FactValue::Text("traffic complaints".to_string()),
                    "Google",
                    0.95,
                ),
                serving_fact(
                    "reviewed-society",
                    "google_rating",
                    FactValue::Numeric(4.3),
                    "Google",
                    0.85,
                ),
            ],
            vec![
                serving_metadata(
                    "reviewed-society",
                    "google_top_negatives",
                    vec!["google reviews", "good reviews"],
                    "TextMatch",
                    1.5,
                    Vec::new(),
                ),
                serving_metadata(
                    "reviewed-society",
                    "google_rating",
                    vec!["high rating", "good reviews", "google rating"],
                    "HigherIsBetter",
                    1.0,
                    vec![4.2, 3.8],
                ),
            ],
        );
        let intent = crate::search::intent::parse_intent("3bhk whitefield with good reviews");

        assert!(
            intent
                .positive_preferences
                .iter()
                .any(|preference| preference.raw_text == "review quality"),
            "good reviews should be parsed as a review quality intent: {:?}",
            intent.positive_preferences
        );

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            "3bhk whitefield with good reviews",
            &intent,
            None,
        );

        let reasons = &results[0].match_explanation.as_ref().unwrap().reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.preference == "review quality"
                    && reason.fact_key == "google_rating"
                    && reason.source_type == "Google"
                    && reason.scoring_method == "serving-numeric"
            }),
            "review quality should be backed by Google rating evidence, got {:?}",
            reasons
        );
        assert!(
            !reasons
                .iter()
                .any(|reason| reason.fact_key == "google_top_negatives"),
            "negative snippets should not prove a positive reviews query: {:?}",
            reasons
        );
    }

    #[test]
    fn negative_risk_query_uses_area_graph_evidence_before_local_fallback() {
        let properties = vec![local_property(
            "whitefield-risk",
            "Whitefield",
            "whitefield-risk",
            3,
            18_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let mut graph = graph_with_society_in_area("whitefield-risk", "whitefield", "Whitefield");
        graph.add_fact_to_node(
            "area:whitefield",
            google_text_fact(
                "waterlogging_detail",
                "Whitefield experiences waterlogging during heavy rainfall and underpass flooding.",
            ),
        );
        graph.add_fact_to_node(
            "area:whitefield",
            google_text_fact(
                "traffic_reality",
                "Whitefield has severe traffic congestion during peak commute hours.",
            ),
        );
        let intent =
            crate::search::intent::parse_intent("3bhk whitefield avoid waterlogging and traffic");
        assert!(
            intent
                .negative_preferences
                .iter()
                .any(|preference| preference.raw_text == "waterlogging risk"),
            "waterlogging should be parsed as a negative preference: {:?}",
            intent.negative_preferences
        );
        assert!(
            intent
                .negative_preferences
                .iter()
                .any(|preference| preference.raw_text == "traffic"),
            "traffic should be parsed as a negative preference: {:?}",
            intent.negative_preferences
        );

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            None,
            &society_names,
            &[],
            "3bhk whitefield avoid waterlogging and traffic",
            &intent,
            Some(&graph),
        );

        let explanation = results[0].match_explanation.as_ref().unwrap();
        assert!(
            explanation.reasons.iter().any(|reason| {
                reason.preference == "avoid waterlogging risk"
                    && reason.fact_key == "waterlogging_detail"
                    && reason.scoring_method == "graph-risk-text"
                    && reason.source_type == "Google"
            }),
            "waterlogging risk should be sourced from area KG facts: {:?}",
            explanation.reasons
        );
        assert!(
            explanation.reasons.iter().any(|reason| {
                reason.preference == "avoid traffic"
                    && reason.fact_key == "traffic_reality"
                    && reason.scoring_method == "graph-risk-text"
                    && reason.source_type == "Google"
            }),
            "traffic risk should be sourced from area KG facts: {:?}",
            explanation.reasons
        );
        assert!(
            explanation.preference_coverage.iter().any(|coverage| {
                coverage.preference == "avoid waterlogging risk" && coverage.status == "risk"
            }),
            "known waterlogging risk should be explicit, not shown as avoided: {:?}",
            explanation.preference_coverage
        );
        assert_eq!(explanation.graph_driven_pct, 100.0);
    }

    #[test]
    fn negative_risk_query_ranks_sourced_risk_above_unknown_risk() {
        let known_risk = local_property(
            "known-risk",
            "Whitefield",
            "known-risk",
            3,
            18_000_000,
            0,
            0.2,
        );
        let unknown_risk = local_property(
            "unknown-risk",
            "Whitefield",
            "unknown-risk",
            3,
            18_000_000,
            0,
            0.2,
        );
        let properties = vec![unknown_risk, known_risk];
        let society_names = local_society_names(&properties);
        let mut graph = graph_with_society_in_area("known-risk", "whitefield", "Whitefield");
        add_society_facts(&mut graph, "unknown-risk", Vec::new());
        graph.add_fact_to_node(
            "area:whitefield",
            google_text_fact(
                "waterlogging_detail",
                "Whitefield experiences waterlogging during heavy rainfall and underpass flooding.",
            ),
        );
        graph.add_fact_to_node(
            "area:whitefield",
            google_text_fact(
                "traffic_reality",
                "Whitefield has severe traffic congestion during peak commute hours.",
            ),
        );
        let intent =
            crate::search::intent::parse_intent("3bhk whitefield avoid waterlogging and traffic");

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            None,
            &society_names,
            &[],
            "3bhk whitefield avoid waterlogging and traffic",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].card.id, "known-risk",
            "sourced risk evidence should outrank unknown risk so the UI can warn clearly"
        );
        let explanation = results[0].match_explanation.as_ref().unwrap();
        assert!(
            explanation
                .preference_coverage
                .iter()
                .any(|coverage| coverage.status == "risk"),
            "top result should carry explicit risk coverage: {:?}",
            explanation.preference_coverage
        );
    }

    #[test]
    fn negative_preference_no_data_query_keeps_candidates_with_no_data_coverage() {
        let properties = vec![
            local_property(
                "unknown-water-a",
                "Whitefield",
                "unknown-water-a",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "unknown-water-b",
                "Whitefield",
                "unknown-water-b",
                3,
                19_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let intent =
            crate::search::intent::parse_intent("avoid water issues, no tanker dependency");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "avoid water issues, no tanker dependency",
            &intent,
            None,
        );

        assert_eq!(results.len(), 2);
        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("negative preference query should include no-data coverage");
        assert!(
            explanation.preference_coverage.iter().any(|coverage| {
                coverage.preference == "avoid water issues" && coverage.status == "no_data"
            }),
            "expected water issues no_data coverage, got {:?}",
            explanation.preference_coverage
        );
        assert!(
            !results[0].match_reason.contains("avoid water issues"),
            "match reason should not claim avoided risk without evidence: {}",
            results[0].match_reason
        );
    }

    #[test]
    fn curated_review_concerns_are_structural_neutral_when_missing_and_do_not_stack() {
        let properties = vec![
            local_property(
                "curated-water-concern",
                "Whitefield",
                "curated-water-concern",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "raw-review-only",
                "Whitefield",
                "raw-review-only",
                3,
                18_000_000,
                0,
                0.2,
            ),
            local_property(
                "no-review-evidence",
                "Whitefield",
                "no-review-evidence",
                3,
                18_000_000,
                0,
                0.2,
            ),
        ];
        let society_names = local_society_names(&properties);
        let curated_fact_key = "review_observation.water_reliability.concern";
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "curated-water-concern",
                    curated_fact_key,
                    FactValue::Tags(vec![
                        "Horrible water issues".to_string(),
                        "Water outages happen repeatedly".to_string(),
                    ]),
                    "Google",
                    0.95,
                ),
                serving_fact(
                    "raw-review-only",
                    "google_review_snippets",
                    FactValue::Tags(vec!["Horrible water issues".to_string()]),
                    "Google",
                    0.95,
                ),
            ],
            vec![
                serving_metadata(
                    "curated-water-concern",
                    curated_fact_key,
                    vec!["water issues"],
                    "Concern",
                    1.2,
                    Vec::new(),
                ),
                serving_metadata(
                    "raw-review-only",
                    "google_review_snippets",
                    vec!["water issues"],
                    "Concern",
                    2.0,
                    Vec::new(),
                ),
            ],
        );
        let query = "avoid water issues in Whitefield";
        let intent = crate::search::intent::parse_intent(query);
        let review_preference = intent
            .negative_preferences
            .iter()
            .find(|preference| preference.raw_text == "water issues")
            .expect("review water concern should be config-resolved");
        assert!(review_preference.missing_evidence_neutral);
        assert!(review_preference
            .expanded_keys
            .iter()
            .any(|key| key == curated_fact_key));

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            query,
            &intent,
            None,
        );
        let result_for = |id: &str| {
            results
                .iter()
                .find(|result| result.card.id == id)
                .unwrap_or_else(|| panic!("missing result {id}"))
        };

        let concern = result_for("curated-water-concern");
        let concern_explanation = concern.match_explanation.as_ref().unwrap();
        let concern_reasons = concern_explanation
            .reasons
            .iter()
            .filter(|reason| reason.fact_key == curated_fact_key)
            .collect::<Vec<_>>();
        assert_eq!(
            concern_reasons.len(),
            1,
            "multiple excerpts for one society/concept must contribute once"
        );
        assert_eq!(concern_reasons[0].scoring_method, "serving-concern");
        assert_eq!(concern_reasons[0].score, 0.0);
        assert!(concern_explanation
            .preference_coverage
            .iter()
            .any(|coverage| {
                coverage.preference == "avoid water issues" && coverage.status == "risk"
            }));

        for id in ["raw-review-only", "no-review-evidence"] {
            let explanation = result_for(id).match_explanation.as_ref().unwrap();
            assert!(
                explanation.reasons.iter().all(|reason| {
                    reason.fact_key != "google_review_snippets"
                        && reason.fact_key != curated_fact_key
                }),
                "raw or absent review evidence must not create proof: {:?}",
                explanation.reasons
            );
            assert!(explanation.preference_coverage.iter().any(|coverage| {
                coverage.preference == "avoid water issues" && coverage.status == "no_data"
            }));
        }
        assert_eq!(
            result_for("raw-review-only").match_score,
            result_for("no-review-evidence").match_score,
            "missing curated review evidence must remain neutral"
        );
    }

    #[test]
    fn curated_review_facts_cover_each_conjunct_from_shared_config_labels() {
        let properties = vec![local_property(
            "curated-conjunction",
            "Whitefield",
            "curated-conjunction",
            3,
            18_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let maintenance_key = "review_observation.maintenance_quality.positive";
        let quiet_key = "review_observation.noise_exposure.positive";
        let serving_facts = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "curated-conjunction",
                    maintenance_key,
                    FactValue::Tags(vec!["The society is well maintained".to_string()]),
                    "Google",
                    0.95,
                ),
                serving_fact(
                    "curated-conjunction",
                    quiet_key,
                    FactValue::Tags(vec!["No sound of traffic inside the society".to_string()]),
                    "Google",
                    0.95,
                ),
            ],
            vec![
                serving_metadata(
                    "curated-conjunction",
                    maintenance_key,
                    vec!["maintenance"],
                    "TextMatch",
                    1.1,
                    Vec::new(),
                ),
                serving_metadata(
                    "curated-conjunction",
                    quiet_key,
                    vec!["quiet neighborhood"],
                    "TextMatch",
                    1.1,
                    Vec::new(),
                ),
            ],
        );
        let query = "quiet well maintained society in Whitefield";
        let intent = crate::search::intent::parse_intent(query);
        for (preference, fact_key) in [
            ("maintenance", maintenance_key),
            ("quiet neighborhood", quiet_key),
        ] {
            assert!(intent.positive_preferences.iter().any(|signal| {
                signal.raw_text == preference
                    && signal.expanded_keys.iter().any(|key| key == fact_key)
            }));
        }

        let results = TextSearch::search_with_index_extra_recall_serving_facts_and_intent(
            &properties,
            None,
            None,
            Some(&serving_facts),
            &society_names,
            &[],
            query,
            &intent,
            None,
        );
        let reasons = &results[0].match_explanation.as_ref().unwrap().reasons;
        assert!(reasons
            .iter()
            .any(|reason| reason.fact_key == maintenance_key));
        assert!(reasons.iter().any(|reason| reason.fact_key == quiet_key));
    }

    #[test]
    fn builder_trust_negative_query_uses_rera_revocation_evidence() {
        let properties = vec![local_property(
            "clean-builder",
            "Whitefield",
            "clean-builder",
            3,
            18_000_000,
            0,
            0.2,
        )];
        let society_names = local_society_names(&properties);
        let graph = graph_with_society_facts(
            "clean-builder",
            vec![rera_numeric_fact("rera_builder_revocations", 0.0)],
        );
        let intent = crate::search::intent::parse_intent("avoid shady builder");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "avoid shady builder",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 1);
        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("builder trust preference should include RERA coverage");
        assert!(
            explanation.reasons.iter().any(|reason| {
                reason.preference == "avoid builder trust"
                    && reason.fact_key == "rera_builder_revocations"
                    && reason.scoring_method == "graph-risk-numeric"
                    && reason.source_type == "Rera"
            }),
            "builder trust should be explained by RERA revocation evidence: {:?}",
            explanation.reasons
        );
        assert!(
            explanation.preference_coverage.iter().any(|coverage| {
                coverage.preference == "avoid builder trust" && coverage.status == "matched"
            }),
            "builder trust coverage should be matched: {:?}",
            explanation.preference_coverage
        );
    }

    #[test]
    fn test_search_requires_rera_land_area_for_min_acres_constraint() {
        let mut large_green = local_property(
            "large-green",
            "Whitefield",
            "large-green",
            3,
            19_000_000,
            8,
            0.2,
        );
        large_green.greenery_score = None;

        let mut small_green = local_property(
            "small-green",
            "Whitefield",
            "small-green",
            3,
            18_000_000,
            8,
            0.2,
        );
        small_green.greenery_score = None;

        let mut unknown_area = local_property(
            "unknown-area",
            "Whitefield",
            "unknown-area",
            3,
            17_000_000,
            8,
            0.2,
        );
        unknown_area.greenery_score = None;

        let properties = vec![small_green, unknown_area, large_green];
        let society_names = local_society_names(&properties);

        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "large-green",
            vec![
                rera_numeric_fact("rera_total_land_area_sqm", 12.0 * SQM_PER_ACRE),
                tags_fact(
                    "google_top_positives",
                    vec!["wide open spaces and green cover across the campus"],
                ),
            ],
        );
        add_society_facts(
            &mut graph,
            "small-green",
            vec![
                rera_numeric_fact("rera_total_land_area_sqm", 5.0 * SQM_PER_ACRE),
                tags_fact(
                    "google_top_positives",
                    vec!["landscaped open spaces and greenery"],
                ),
            ],
        );

        let intent =
            crate::search::intent::parse_intent("3bhk with greenery in whitefield above 10 acres");
        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3bhk with greenery in whitefield above 10 acres",
            &intent,
            Some(&graph),
        );

        let ids: Vec<&str> = results.iter().map(|r| r.card.id.as_str()).collect();
        assert_eq!(ids, vec!["large-green"]);
        assert!(
            results[0].match_reason.contains("above 10 acres"),
            "hard constraint should be visible in match reason: {}",
            results[0].match_reason
        );
        assert!(
            results[0].match_reason.contains("greenery"),
            "greenery should be visible only because graph text supports it: {}",
            results[0].match_reason
        );
        let explanation = results[0].match_explanation.as_ref().unwrap();
        assert!(explanation
            .reasons
            .iter()
            .any(|reason| reason.scoring_method == "rera-proof"
                && reason.fact_key == "rera_total_land_area_sqm"));
    }

    #[test]
    fn multi_intent_query_ranks_candidates_that_satisfy_more_structured_intents() {
        let acreage_only = local_property(
            "acreage-only",
            "Whitefield",
            "acreage-only",
            3,
            18_000_000,
            8,
            0.2,
        );
        let open_space_fit = local_property(
            "open-space-fit",
            "Whitefield",
            "open-space-fit",
            3,
            20_000_000,
            8,
            0.2,
        );
        let properties = vec![acreage_only, open_space_fit];
        let society_names = local_society_names(&properties);
        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "acreage-only",
            vec![
                rera_numeric_fact("rera_total_land_area_sqm", 12.0 * SQM_PER_ACRE),
                rera_numeric_fact("project_open_area_pct", 70.0),
            ],
        );
        add_society_facts(
            &mut graph,
            "open-space-fit",
            vec![
                rera_numeric_fact("rera_total_land_area_sqm", 12.0 * SQM_PER_ACRE),
                rera_numeric_fact("project_open_area_pct", 85.0),
                preference_fact(
                    "open_space_score",
                    FactValue::Numeric(0.85),
                    vec!["greenery", "open space"],
                    ScoringDirection::HigherIsBetter,
                    1.2,
                    vec![0.8, 0.6],
                ),
            ],
        );

        let query = "3bhk whitefield above 10 acres with at least 80% open space";
        let intent = crate::search::intent::parse_intent(query);
        assert!(
            intent
                .positive_preferences
                .iter()
                .any(|preference| preference.raw_text == "greenery"),
            "open-space wording should map to the generic greenery/open-space preference: {:?}",
            intent.positive_preferences
        );

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            query,
            &intent,
            Some(&graph),
        );

        assert_eq!(
            results[0].card.id, "open-space-fit",
            "candidate satisfying both acreage and numeric open-space intent should rank first"
        );
        assert_eq!(
            results.len(),
            1,
            "projects below the requested open-space percentage should be filtered out"
        );
        let reasons = &results[0].match_explanation.as_ref().unwrap().reasons;
        assert!(
            reasons.iter().any(|reason| {
                reason.preference == "above 10 acres"
                    && reason.fact_key == "rera_total_land_area_sqm"
            }) && reasons.iter().any(|reason| {
                reason.preference == "above 80 percent"
                    && reason.fact_key == "project_open_area_pct"
            }) && reasons.iter().any(|reason| {
                reason.preference == "greenery" && reason.fact_key == "open_space_score"
            }),
            "top result should explain both structured intents: {:?}",
            reasons
        );
    }

    #[test]
    fn test_no_data_preference_is_not_advertised_in_match_reason() {
        let mut property = local_property(
            "no-greenery-data",
            "Whitefield",
            "no-greenery-data",
            3,
            19_000_000,
            8,
            0.2,
        );
        property.greenery_score = None;
        let properties = vec![property];
        let society_names = local_society_names(&properties);
        let graph = graph_with_society_facts("no-greenery-data", Vec::new());
        let intent = crate::search::intent::parse_intent("3bhk whitefield greenery");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3bhk whitefield greenery",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].match_reason.contains("greenery"),
            "match reason should not claim no-data greenery: {}",
            results[0].match_reason
        );
        let explanation = results[0].match_explanation.as_ref().unwrap();
        assert!(
            explanation
                .preference_coverage
                .iter()
                .any(|coverage| coverage.preference == "greenery" && coverage.status == "no_data"),
            "expected greenery no_data coverage, got {:?}",
            explanation.preference_coverage
        );
    }

    #[test]
    fn nearby_area_result_does_not_claim_an_exact_area_match() {
        let property = local_property(
            "nearby-area",
            "East Bangalore",
            "nearby-area",
            3,
            19_000_000,
            8,
            0.2,
        );
        let properties = vec![property];
        let society_names = local_society_names(&properties);
        let intent = crate::search::intent::parse_intent("3bhk in east bengaluru");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3bhk in east bengaluru",
            &intent,
            None,
        );

        assert_eq!(results.len(), 1);
        assert!(results[0]
            .match_reason
            .contains("Near East Bengaluru (East Bangalore)"));
        assert!(!results[0].match_reason.contains("Matches East Bengaluru"));
    }

    #[test]
    fn blank_property_area_is_not_treated_as_nearby() {
        assert!(!area_is_nearby("", "Whitefield"));
        assert!(!area_is_nearby("   ", "Whitefield"));
        assert!(!area_is_nearby("Varthur", ""));
    }

    #[test]
    fn graph_verified_area_result_is_not_labeled_as_nearby() {
        let property = local_property(
            "graph-area",
            "Legacy East Zone",
            "graph-area",
            3,
            19_000_000,
            8,
            0.2,
        );
        let properties = vec![property];
        let society_names = local_society_names(&properties);
        let graph = graph_with_society_in_area("graph-area", "whitefield", "Whitefield");
        let query = "3bhk apartment in whitefield";
        let mut intent = crate::search::intent::parse_intent(query);
        intent.area = Some("Whitefield".to_string());

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            query,
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].match_reason.contains("Matches Whitefield"));
        assert!(!results[0].match_reason.contains("Near Whitefield"));
    }

    #[test]
    fn test_graph_text_greenery_evidence_supports_preference() {
        let mut property = local_property(
            "graph-greenery",
            "Whitefield",
            "graph-greenery",
            3,
            19_000_000,
            8,
            0.2,
        );
        property.greenery_score = None;
        let properties = vec![property];
        let society_names = local_society_names(&properties);
        let graph = graph_with_society_facts(
            "graph-greenery",
            vec![tags_fact(
                "google_top_positives",
                vec!["abundant green spaces, landscaped common areas, and open spaces"],
            )],
        );
        let intent = crate::search::intent::parse_intent("3bhk whitefield greenery");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3bhk whitefield greenery",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].match_reason.contains("greenery"));
        let explanation = results[0].match_explanation.as_ref().unwrap();
        assert!(
            explanation
                .reasons
                .iter()
                .any(|reason| reason.preference == "greenery"
                    && reason.scoring_method == "graph-text"
                    && reason.fact_key == "google_top_positives"),
            "expected graph-text greenery reason, got {:?}",
            explanation.reasons
        );
    }

    #[test]
    fn test_search_ranks_low_waterlogging_before_high_risk_when_avoiding_waterlogging() {
        let mut low_risk = local_property(
            "low-waterlogging",
            "Whitefield",
            "low-waterlogging",
            3,
            19_000_000,
            8,
            0.2,
        );
        low_risk.waterlogging_risk_score = Some(0.1);

        let mut high_risk = local_property(
            "high-waterlogging",
            "Whitefield",
            "high-waterlogging",
            3,
            19_000_000,
            8,
            0.2,
        );
        high_risk.waterlogging_risk_score = Some(0.85);

        let properties = vec![high_risk, low_risk];
        let society_names = local_society_names(&properties);
        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "low-waterlogging",
            vec![numeric_fact("waterlogging_risk_score", 0.1)],
        );
        add_society_facts(
            &mut graph,
            "high-waterlogging",
            vec![numeric_fact("waterlogging_risk_score", 0.85)],
        );
        let intent =
            crate::search::intent::parse_intent("3BHK Whitefield under 2Cr avoid waterlogging");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr avoid waterlogging",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "low-waterlogging");
        assert!(
            results[0].match_score > results[1].match_score,
            "low waterlogging risk should outrank high waterlogging risk"
        );
        let explanation = results[0]
            .match_explanation
            .as_ref()
            .expect("negative-preference query should include explanation");
        assert!(
            explanation
                .reasons
                .iter()
                .any(|r| r.preference == "avoid waterlogging risk"
                    && r.scoring_method == "graph-risk-numeric"),
            "avoid-waterlogging should be explained by graph risk scoring"
        );
    }

    #[test]
    fn test_search_penalizes_heavy_traffic_when_user_requests_less_traffic() {
        let mut low_traffic = local_property(
            "low-traffic",
            "Whitefield",
            "low-traffic",
            3,
            19_000_000,
            8,
            0.2,
        );
        low_traffic.traffic_score = Some(0.2);

        let mut heavy_traffic = local_property(
            "heavy-traffic",
            "Whitefield",
            "heavy-traffic",
            3,
            19_000_000,
            8,
            0.2,
        );
        heavy_traffic.traffic_score = Some(0.9);

        let properties = vec![heavy_traffic, low_traffic];
        let society_names = local_society_names(&properties);
        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "low-traffic",
            vec![numeric_fact("traffic_score", 0.2)],
        );
        add_society_facts(
            &mut graph,
            "heavy-traffic",
            vec![numeric_fact("traffic_score", 0.9)],
        );
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr less traffic");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr less traffic",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "low-traffic");
        assert!(results[0].match_score > results[1].match_score);
    }

    #[test]
    fn test_search_filters_explicitly_excluded_area() {
        let electronic_city = local_property(
            "electronic-city-fit",
            "Electronic City",
            "electronic-city-fit",
            3,
            19_000_000,
            8,
            0.2,
        );
        let whitefield = local_property(
            "whitefield-fit",
            "Whitefield",
            "whitefield-fit",
            3,
            19_000_000,
            8,
            0.2,
        );
        let properties = vec![electronic_city, whitefield];
        let society_names = local_society_names(&properties);
        let index = SearchIndex::build(&properties);
        let query = "near tech parks but quiet not electronic city 3bhk";
        let mut intent = crate::search::intent::parse_intent(query);
        intent.excluded_areas = vec!["Electronic City".to_string()];

        assert_eq!(intent.excluded_areas, vec!["Electronic City".to_string()]);

        let results = TextSearch::search_with_index_and_intent(
            &properties,
            Some(&index),
            &society_names,
            &[],
            query,
            &intent,
            None,
        );

        assert!(
            !results.is_empty(),
            "excluded-area query should still return non-excluded candidates"
        );
        assert!(
            results
                .iter()
                .all(|result| result.card.area != "Electronic City"),
            "excluded area leaked into results: {:?}",
            results.iter().map(|r| &r.card.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_search_does_not_fake_unsupported_inventory_matches() {
        let apartment = local_property(
            "apartment-fit",
            "Whitefield",
            "apartment-fit",
            3,
            19_000_000,
            8,
            0.2,
        );
        let properties = vec![apartment];
        let society_names = local_society_names(&properties);
        let index = SearchIndex::build(&properties);
        let query = "plot or villa style calm layout near Bagalur metro";
        let intent = crate::search::intent::parse_intent(query);

        assert_eq!(
            intent.unsupported_inventory_types,
            vec!["plot".to_string(), "villa".to_string()]
        );

        let results = TextSearch::search_with_index_and_intent(
            &properties,
            Some(&index),
            &society_names,
            &[],
            query,
            &intent,
            None,
        );

        assert!(
            results.is_empty(),
            "unsupported plot/villa query should not return apartment matches"
        );
    }

    #[test]
    fn test_search_ranks_reliable_builder_from_graph_facts() {
        let mut stronger_builder = local_property(
            "stronger-builder",
            "Whitefield",
            "stronger-builder",
            3,
            19_000_000,
            8,
            0.2,
        );
        stronger_builder.builder_quality_score = Some(0.9);

        let mut weaker_builder = local_property(
            "weaker-builder",
            "Whitefield",
            "weaker-builder",
            3,
            19_000_000,
            8,
            0.2,
        );
        weaker_builder.builder_quality_score = Some(0.35);

        let properties = vec![weaker_builder, stronger_builder];
        let society_names = local_society_names(&properties);
        let mut graph = KnowledgeGraph::new();
        add_society_facts(
            &mut graph,
            "stronger-builder",
            vec![preference_fact(
                "builder_quality_score",
                FactValue::Numeric(0.9),
                vec!["reliable builder"],
                ScoringDirection::HigherIsBetter,
                2.0,
                vec![0.8, 0.6],
            )],
        );
        add_society_facts(
            &mut graph,
            "weaker-builder",
            vec![preference_fact(
                "builder_quality_score",
                FactValue::Numeric(0.35),
                vec!["reliable builder"],
                ScoringDirection::HigherIsBetter,
                2.0,
                vec![0.8, 0.6],
            )],
        );
        let intent =
            crate::search::intent::parse_intent("3BHK Whitefield under 2Cr reliable builder");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr reliable builder",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "stronger-builder");
        assert!(results[0]
            .match_explanation
            .as_ref()
            .expect("builder preference should have explanation")
            .reasons
            .iter()
            .any(|r| r.preference == "reliable builder" && r.fact_key == "builder_quality_score"));
    }

    #[test]
    fn named_society_match_is_boundary_aware() {
        assert!(query_mentions_resolvable_society(
            "prestige waterford 3bhk",
            "Prestige Waterford"
        ));
        assert!(!query_mentions_resolvable_society(
            "prestige waterforded 3bhk",
            "Prestige Waterford"
        ));
        assert!(!query_mentions_resolvable_society(
            "3bhk in whitefield",
            "in"
        ));
    }

    #[test]
    fn project_name_prefix_match_ranks_before_looser_brand_match() {
        let waterford = local_property(
            "prestige-waterford-3bhk",
            "Whitefield",
            "prestige-waterford",
            3,
            25_000_000,
            8,
            0.2,
        );
        let lakeside = local_property(
            "prestige-lakeside-3bhk",
            "Whitefield",
            "prestige-lakeside-habitat",
            3,
            25_000_000,
            8,
            0.2,
        );
        let properties = vec![lakeside, waterford];
        let mut society_names = std::collections::HashMap::new();
        society_names.insert(
            "prestige-lakeside-habitat".to_string(),
            "Prestige Lakeside Habitat".to_string(),
        );
        society_names.insert(
            "prestige-waterford".to_string(),
            "Prestige Waterford".to_string(),
        );
        let extra_candidate_ids = vec![
            "prestige-lakeside-3bhk".to_string(),
            "prestige-waterford-3bhk".to_string(),
        ];
        let query = "prestige water";
        let intent = crate::search::intent::parse_intent(query);

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&extra_candidate_ids),
            None,
            None,
            &society_names,
            &[],
            query,
            &intent,
            None,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "prestige-waterford-3bhk");
        assert_eq!(results[1].card.id, "prestige-lakeside-3bhk");
    }

    #[test]
    fn exact_society_match_ranks_before_loose_text_match() {
        let target = local_property(
            "target-3bhk",
            "South Bengaluru",
            "snn-raj-serenity",
            3,
            20_000_000,
            8,
            0.2,
        );
        let decoy = local_property(
            "decoy-3bhk",
            "South Bengaluru",
            "snn-raj-grandeur",
            3,
            20_000_000,
            8,
            0.2,
        );
        let properties = vec![decoy, target];
        let society_names = std::collections::HashMap::from([
            (
                "snn-raj-serenity".to_string(),
                "SNN Raj Serenity".to_string(),
            ),
            (
                "snn-raj-grandeur".to_string(),
                "SNN Raj Grandeur".to_string(),
            ),
        ]);
        let candidate_ids = vec!["decoy-3bhk".to_string(), "target-3bhk".to_string()];
        let query = "SNN Raj Serenity trusted builder";
        let intent = crate::search::intent::parse_intent(query);

        let results = TextSearch::search_with_index_extra_recall_geo_serving_facts_and_intent(
            &properties,
            None,
            Some(&candidate_ids),
            None,
            None,
            &society_names,
            &[],
            query,
            &intent,
            None,
        );

        assert_eq!(results[0].card.id, "target-3bhk");
    }
}
