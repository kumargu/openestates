use crate::knowledge::node::RootSource;
use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::models::{Property, Seller, Society};
use crate::routes::enrichment::{enrich_property_card_with_sellers, society_node_id};
use crate::routes::search::graph_preference_score_detailed;
use crate::serving::{ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord};

use super::index::SearchIndex;
use super::intent::{ConstraintOperator, HardConstraint, SearchIntent};
use super::schema::{self, NumericConstraintSchema};
use super::{
    ConfidenceComponent, ConfidenceScore, MatchExplanation, MatchReason, PreferenceCoverage,
    SearchResultCard,
};

/// Simple text-matching search engine.
///
/// Designed to be swappable with a vector search backend later — the interface
/// (query in, scored results out) stays the same.
pub struct TextSearch;

impl TextSearch {
    /// Intent-based search: filters by hard constraints, scores by relevance,
    /// and returns full PropertyCard data with match info.
    ///
    /// When `graph` is provided, preference scoring uses the graph's self-describing
    /// `answers_preferences` + `scoring_hint` metadata. Falls back to hardcoded
    /// scoring when the graph doesn't have relevant facts.
    #[allow(dead_code)] // Convenience wrapper used by tests — prod code calls search_with_intent_and_sellers
    pub fn search_with_intent(
        properties: &[Property],
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        Self::search_with_intent_and_sellers(
            properties,
            society_names,
            societies,
            query,
            intent,
            graph,
            &[],
        )
    }

    /// Intent-based search with seller trust data for completeness boost and card enrichment.
    pub fn search_with_intent_and_sellers(
        properties: &[Property],
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
        sellers: &[Seller],
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_and_intent_and_sellers(
            properties,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
            sellers,
        )
    }

    /// Indexed local recall followed by deterministic ranking and explanation.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_and_intent_and_sellers(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
        sellers: &[Seller],
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_and_extra_recall_and_intent_and_sellers(
            properties,
            search_index,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
            sellers,
        )
    }

    /// Indexed local recall plus optional serving-bundle recall, followed by deterministic ranking.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_and_extra_recall_and_intent_and_sellers(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
        sellers: &[Seller],
    ) -> Vec<SearchResultCard> {
        Self::search_with_index_extra_recall_serving_facts_and_intent_and_sellers(
            properties,
            search_index,
            extra_candidate_ids,
            None,
            society_names,
            societies,
            query,
            intent,
            graph,
            sellers,
        )
    }

    /// Indexed local recall plus optional serving-bundle recall/facts.
    ///
    /// The in-memory KG remains the first ranking source. Serving facts are a
    /// read-optimized overlay for recently materialized DAG facts that have not
    /// yet been folded back into per-entity KG JSON files.
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_index_extra_recall_serving_facts_and_intent_and_sellers(
        properties: &[Property],
        search_index: Option<&SearchIndex>,
        extra_candidate_ids: Option<&[String]>,
        serving_facts: Option<&ServingFactIndex>,
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
        sellers: &[Seller],
    ) -> Vec<SearchResultCard> {
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();
        let positive_preferences = positive_preference_labels(intent);
        let negative_preferences = negative_preference_labels(intent);
        let has_explainable_signals = !positive_preferences.is_empty()
            || !negative_preferences.is_empty()
            || !intent.hard_constraints.is_empty();
        let candidate_ids = merged_candidate_ids(
            search_index.map(|index| index.recall_ids(query, intent)),
            extra_candidate_ids,
        );

        let mut results: Vec<SearchResultCard> = properties
            .iter()
            .filter_map(|p| {
                if let Some(ids) = candidate_ids.as_ref() {
                    if !ids.iter().any(|id| id == &p.id) {
                        return None;
                    }
                }

                // Hard constraint: BHK
                if let Some(bhk) = intent.bhk {
                    if p.bhk != bhk {
                        return None;
                    }
                }

                // Hard constraint: budget
                if let Some(budget_max) = intent.budget_max {
                    if p.price > budget_max {
                        return None;
                    }
                }

                // Soft constraint: area — exact match keeps full score,
                // nearby/sub-area match gets a penalty instead of exclusion.
                let area_penalty: f64 = if let Some(ref area) = intent.area {
                    if p.area.eq_ignore_ascii_case(area) {
                        0.0 // exact match
                    } else if area_is_nearby(&p.area, area) {
                        -2.0 // nearby: include but rank lower
                    } else if graph_area_match(&p.society_id, area, graph) {
                        -1.0 // graph SocietyInArea edge match: between exact and nearby
                    } else {
                        return None; // unrelated area: exclude
                    }
                } else {
                    0.0
                };

                let hard_constraint_matches =
                    match_hard_constraints(intent, graph, serving_facts, &p.society_id)?;

                let society_name = society_names
                    .get(&p.society_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");

                // Base text score
                let (mut score, mut reasons) = if terms.is_empty() {
                    (1.0, Vec::new())
                } else {
                    score_property(p, society_name, &terms)
                };
                score += area_penalty;

                // Boost for preference alignment — collect structured reasons
                let mut match_reasons: Vec<MatchReason> = Vec::new();
                let mut pref_coverage: Vec<PreferenceCoverage> = Vec::new();
                let mut graph_count: usize = 0;
                let mut legacy_count: usize = 0;
                let mut total_facts_consulted: usize = 0;

                for evidence in hard_constraint_matches {
                    total_facts_consulted += 1;
                    graph_count += 1;
                    score += evidence.score_delta;
                    reasons.push(evidence.reason.clone());
                    match_reasons.push(MatchReason {
                        preference: evidence.preference.clone(),
                        fact_key: evidence.fact_key.clone(),
                        display: evidence.display,
                        score: evidence.normalized_score,
                        confidence: evidence.confidence,
                        source_type: evidence.source_type,
                        scoring_method: evidence.scoring_method,
                    });
                    pref_coverage.push(PreferenceCoverage {
                        preference: evidence.preference,
                        status: "matched".into(),
                        fact_key: Some(evidence.fact_key),
                    });
                }

                for pref in &positive_preferences {
                    // Graph-first: check if the society's facts declare scoring for this preference
                    if let Some(g) = graph {
                        if let Some((gs, detail)) =
                            graph_preference_score_detailed(g, &p.society_id, pref)
                        {
                            total_facts_consulted += 1;
                            score += gs;
                            reasons.push(format!("matches preference: {}", pref));

                            // Normalize score to 0-1 range (graph scores are 0-2)
                            let norm_score = (gs / 2.0).min(1.0);
                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: detail.fact_key.clone(),
                                display: detail.display,
                                score: norm_score,
                                confidence: detail.confidence,
                                source_type: detail.source_type,
                                scoring_method: "graph".into(),
                            });
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

                        if let Some(evidence) =
                            graph_textual_preference_evidence(g, &p.society_id, pref)
                        {
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            reasons.push(evidence.reason.clone());

                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display,
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type,
                                scoring_method: evidence.scoring_method,
                            });
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

                    if let Some(serving_facts) = serving_facts {
                        if let Some(evidence) =
                            serving_preference_evidence(serving_facts, &p.society_id, pref)
                        {
                            total_facts_consulted += 1;
                            score += evidence.score_delta;
                            reasons.push(evidence.reason.clone());

                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: evidence.fact_key.clone(),
                                display: evidence.display,
                                score: evidence.normalized_score,
                                confidence: evidence.confidence,
                                source_type: evidence.source_type,
                                scoring_method: evidence.scoring_method,
                            });
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

                    // Legacy fallback
                    let legacy = legacy_preference_score(p, pref);
                    if legacy > 0.0 {
                        score += legacy;
                        reasons.push(format!("matches preference: {}", pref));

                        let norm_score = (legacy / 2.0).min(1.0);
                        let fact_key = legacy_fact_key_for_preference(pref);
                        match_reasons.push(MatchReason {
                            preference: pref.clone(),
                            fact_key: fact_key.clone(),
                            display: format_legacy_display(pref, p),
                            score: norm_score,
                            confidence: 0.5,
                            source_type: "Seed".into(),
                            scoring_method: "legacy".into(),
                        });
                        pref_coverage.push(PreferenceCoverage {
                            preference: pref.clone(),
                            status: if norm_score > 0.5 {
                                "matched"
                            } else {
                                "partial"
                            }
                            .into(),
                            fact_key: Some(fact_key),
                        });
                        legacy_count += 1;
                    } else {
                        pref_coverage.push(PreferenceCoverage {
                            preference: pref.clone(),
                            status: "no_data".into(),
                            fact_key: None,
                        });
                    }
                }

                for pref in &negative_preferences {
                    if let Some(evaluation) = legacy_negative_preference_evaluation(p, pref) {
                        score += evaluation.score_delta;
                        if evaluation.score_delta >= 0.0 {
                            reasons.push(format!("avoids {}", pref));
                        } else {
                            reasons.push(format!("risk: {}", pref));
                        }

                        match_reasons.push(MatchReason {
                            preference: format!("avoid {}", pref),
                            fact_key: evaluation.fact_key.to_string(),
                            display: evaluation.display,
                            score: evaluation.score,
                            confidence: 0.5,
                            source_type: "Seed".into(),
                            scoring_method: "local-risk".into(),
                        });
                        pref_coverage.push(PreferenceCoverage {
                            preference: format!("avoid {}", pref),
                            status: evaluation.status.to_string(),
                            fact_key: Some(evaluation.fact_key.to_string()),
                        });
                        legacy_count += 1;
                    } else {
                        pref_coverage.push(PreferenceCoverage {
                            preference: format!("avoid {}", pref),
                            status: "no_data".into(),
                            fact_key: None,
                        });
                    }
                }

                // Build explanation only when the query asked for constraints/preferences.
                let match_explanation = if has_explainable_signals {
                    let total = graph_count + legacy_count;
                    let graph_pct = if total > 0 {
                        (graph_count as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    Some(MatchExplanation {
                        reasons: match_reasons,
                        preference_coverage: pref_coverage,
                        graph_driven_pct: graph_pct,
                        total_facts_consulted,
                    })
                } else {
                    None
                };

                // If we had hard constraints that passed, give a base score even if
                // text matching scored zero.
                let has_constraints = intent.area.is_some()
                    || intent.bhk.is_some()
                    || intent.budget_max.is_some()
                    || !intent.hard_constraints.is_empty();
                if score <= 0.0 && has_constraints {
                    score = 1.0;
                    reasons.push("matches search criteria".to_string());
                }

                if score <= 0.0 {
                    return None;
                }

                // Use shared enrichment — same PropertyCard as /api/properties.
                // graph is always Some in practice (search always has KG access).
                let card = if let Some(g) = graph {
                    enrich_property_card_with_sellers(p, societies, g, sellers)
                } else {
                    // Fallback without graph — build minimal card
                    crate::models::PropertyCard {
                        id: p.id.clone(),
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
                        hero_image: p.hero_image.clone(),
                        transparency_tags: p.transparency_tags.iter().take(3).cloned().collect(),
                        description_summary: p.description_summary.clone(),
                        possession_status: p.possession_status.clone(),
                        metro_distance_mins: p.metro_distance_mins,
                        floor: p.floor,
                        total_floors: p.total_floors,
                        facing: p.facing.clone(),
                        google_rating: None,
                        google_review_count: None,
                        google_reviews_url: None,
                        seller_id: p.seller_id.clone(),
                        seller_completeness_pct: None,
                        documents_provided: Vec::new(),
                        seller_verified: None,
                        root_source: None,
                        project_status: None,
                        project_status_display: None,
                        builder_delivery_display: None,
                        data_freshness: None,
                    }
                };

                // Normalize score to 0.0–1.0 range (rough normalization)
                let max_possible = 15.0; // approximate ceiling
                let mut normalized = (score / max_possible).min(1.0);

                // Seller completeness boost — multiplicative so low-score properties
                // don't get disproportionately lifted (0.10 → 0.105, not 0.15).
                if let Some(pct) = card.seller_completeness_pct {
                    if pct >= 70 {
                        normalized = (normalized * 1.05).min(1.0);
                    } else if pct >= 42 {
                        normalized = (normalized * 1.02).min(1.0);
                    }
                }
                let match_label = match_label_from_score(normalized);
                let match_reason = build_match_reason(intent, &reasons);

                // Compute confidence score for this result
                let gdp = match_explanation
                    .as_ref()
                    .map(|e| e.graph_driven_pct)
                    .unwrap_or(0.0);
                let confidence_score = compute_confidence(graph, &p.society_id, gdp);

                Some(SearchResultCard {
                    card,
                    match_score: (normalized * 100.0).round() / 100.0,
                    match_label,
                    match_reason,
                    match_explanation,
                    semantic_score: None,
                    confidence_score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

fn merged_candidate_ids(
    local_candidate_ids: Option<Vec<String>>,
    extra_candidate_ids: Option<&[String]>,
) -> Option<Vec<String>> {
    let mut merged = local_candidate_ids.unwrap_or_default();
    if let Some(extra_candidate_ids) = extra_candidate_ids {
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

struct EvidenceMatch {
    preference: String,
    fact_key: String,
    display: String,
    normalized_score: f64,
    score_delta: f64,
    confidence: f32,
    source_type: String,
    scoring_method: String,
    reason: String,
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
        if *canonical_value <= 0.0 {
            continue;
        }

        match constraint.operator {
            ConstraintOperator::Min => {
                if canonical_value + 0.001 < threshold {
                    return ConstraintEvaluation::Failed;
                }
            }
        }

        let display_value = canonical_value / query_unit.to_canonical;
        return ConstraintEvaluation::Matched(EvidenceMatch {
            preference: constraint.raw_text.clone(),
            fact_key: fact.key.clone(),
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
        if !canonical_value.is_finite() || canonical_value <= 0.0 {
            continue;
        }
        match constraint.operator {
            ConstraintOperator::Min if canonical_value + 0.001 < threshold => {
                return ConstraintEvaluation::Failed;
            }
            ConstraintOperator::Min => {}
        }
        let display_value = canonical_value / query_unit.to_canonical;
        return ConstraintEvaluation::Matched(EvidenceMatch {
            preference: constraint.raw_text.clone(),
            fact_key: fact.fact_key.clone(),
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
) -> Option<EvidenceMatch> {
    let schema = schema::text_evidence_schema(preference)?;
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;

    for fact in &node.facts {
        if !schema::fact_answers_text_schema(&fact.key, &fact.answers_preferences, schema) {
            continue;
        }

        if let Some(snippet) = schema::text_support_snippet(&fact.value, schema) {
            return Some(EvidenceMatch {
                preference: preference.to_string(),
                fact_key: fact.key.clone(),
                display: format!("{}: {}", schema.display_label, snippet),
                normalized_score: 0.7,
                score_delta: schema.score_delta,
                confidence: fact.confidence,
                source_type: format!("{:?}", fact.source.source_type),
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
) -> Option<EvidenceMatch> {
    let node_id = society_node_id(society_id);
    let rows = serving_facts.entity(&node_id)?;

    for fact in &rows.facts {
        let Some(metadata) = rows.search_metadata.iter().find(|metadata| {
            metadata.fact_key.eq_ignore_ascii_case(&fact.fact_key)
                && metadata_supports_text_match(metadata)
                && metadata_answers_preference(metadata, preference)
        }) else {
            continue;
        };

        let score_delta = f64::from(metadata.scoring_weight.unwrap_or(1.0)).clamp(0.0, 2.0);
        return Some(EvidenceMatch {
            preference: preference.to_string(),
            fact_key: fact.fact_key.clone(),
            display: render_serving_fact_display(fact, metadata, &fact.value),
            normalized_score: (score_delta / 2.0).min(1.0),
            score_delta,
            confidence: fact.confidence,
            source_type: fact.source_type.clone(),
            scoring_method: "serving-fact".into(),
            reason: format!("matches preference: {}", preference),
        });
    }

    let schema = schema::text_evidence_schema(preference)?;
    for fact in &rows.facts {
        if !schema::fact_answers_text_schema(&fact.fact_key, &[], schema) {
            continue;
        }

        if let Some(snippet) = schema::text_support_snippet(&fact.value, schema) {
            return Some(EvidenceMatch {
                preference: preference.to_string(),
                fact_key: fact.fact_key.clone(),
                display: format!("{}: {}", schema.display_label, snippet),
                normalized_score: 0.7,
                score_delta: schema.score_delta,
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
                scoring_method: "serving-text".into(),
                reason: format!("matches preference: {}", preference),
            });
        }
    }

    None
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
    let value = fact
        .value_text
        .clone()
        .unwrap_or_else(|| fact_value_display(value));
    metadata
        .display_template
        .as_deref()
        .unwrap_or("{value}")
        .replace("{value}", &value)
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

/// Number of facts considered "full coverage" for confidence scoring.
/// Calibrated to ~p25 of enriched societies (median=49, p25≈25).
const FACT_COVERAGE_THRESHOLD: f64 = 25.0;

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

    // Fact coverage: min(fact_count/FACT_COVERAGE_THRESHOLD, 1.0)
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
            Some(RootSource::Seller) => (0.6, "Seller-listed data".to_string()),
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
    let score = (fact_count as f64 / FACT_COVERAGE_THRESHOLD).min(1.0);
    let explanation = format!(
        "{} facts available ({} = full coverage)",
        fact_count, FACT_COVERAGE_THRESHOLD as u32
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
        if is_scoring_stopword(term) {
            continue;
        }

        let mut term_matched = false;

        for (field_value, weight, field_name) in &fields {
            let field_lower = field_value.to_lowercase();
            if field_lower.contains(term) {
                total_score += weight;
                if !term_matched {
                    reasons.push(format!("matched '{}' in {}", term, field_name));
                    term_matched = true;
                }
            }
        }

        // Also check transparency tags.
        for tag in &property.transparency_tags {
            if tag.to_lowercase().contains(term) {
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

fn is_scoring_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "at"
            | "above"
            | "below"
            | "by"
            | "for"
            | "from"
            | "in"
            | "is"
            | "near"
            | "no"
            | "not"
            | "of"
            | "or"
            | "over"
            | "the"
            | "to"
            | "under"
            | "with"
            | "without"
            | "acre"
            | "acres"
            | "bhk"
            | "cr"
            | "crore"
            | "lakh"
            | "lakhs"
    )
}

/// Legacy hardcoded preference scoring — used when the graph doesn't have
/// self-describing scoring_hint metadata for this preference.
///
/// TODO(Phase 2): Remove once KG property nodes carry `answers_preferences` and
/// `scoring_hint` on their facts. Currently KG property nodes have only 7-8 bare
/// fact keys (area, city, bhk, price, etc.) with no scoring metadata, so this
/// fallback is actively needed for all preference-based ranking.
fn legacy_preference_score(property: &Property, preference: &str) -> f64 {
    match preference {
        "metro access" => {
            if property.metro_distance_mins <= 10 {
                2.0
            } else if property.metro_distance_mins <= 20 {
                1.0
            } else {
                0.0
            }
        }
        "quiet neighborhood" => {
            if property.noise_score < 0.3 {
                2.0
            } else if property.noise_score < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        "value for money" => {
            if property.price_per_sqft < 8000 {
                2.0
            } else if property.price_per_sqft < 10000 {
                1.0
            } else {
                0.0
            }
        }
        "premium" => {
            if property.price_per_sqft >= 12000 {
                2.0
            } else if property.price_per_sqft >= 10000 {
                1.0
            } else {
                0.0
            }
        }
        "good society" => {
            if property.society_quality_score >= 0.8 {
                2.0
            } else if property.society_quality_score >= 0.6 {
                1.0
            } else {
                0.0
            }
        }
        "reliable builder" | "trusted builder" => {
            if property.builder_quality_score >= 0.8 {
                2.0
            } else if property.builder_quality_score >= 0.6 {
                1.0
            } else {
                0.0
            }
        }
        "on time delivery" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("delay") || status.contains("behind") {
                0.0
            } else if status.contains("ready")
                || status.contains("delivered")
                || status.contains("completed")
            {
                2.0
            } else if property.builder_quality_score >= 0.75 {
                1.0
            } else {
                0.0
            }
        }
        "greenery" => property.greenery_score.unwrap_or(0.0) * 2.0,
        "new construction" | "under construction" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("under") || status.contains("construction") || status.contains("new")
            {
                2.0
            } else {
                0.0
            }
        }
        "ready to move" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("ready")
                || status.contains("delivered")
                || status.contains("completed")
            {
                2.0
            } else {
                0.0
            }
        }
        "new launch" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("new") || status.contains("launch") {
                2.0
            } else {
                0.0
            }
        }
        "delayed" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("delay") || status.contains("behind") {
                2.0
            } else {
                0.0
            }
        }
        "upcoming" => {
            let status = property.possession_status.to_lowercase();
            if status.contains("upcoming")
                || status.contains("pre-launch")
                || status.contains("future")
            {
                2.0
            } else {
                0.0
            }
        }
        "high floor" => {
            if property.total_floors > 0 && property.floor >= property.total_floors - 2 {
                2.0
            } else if property.floor >= 10 {
                1.0
            } else {
                0.0
            }
        }
        "east facing" => {
            if property.facing.to_lowercase().contains("east") {
                2.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Check if a property's area is "nearby" the canonical search area.
/// This catches sub-areas, micro-markets, and externally assigned areas that
/// belong to the same macro area but don't exactly match the canonical name.
///
/// Checks: alias list membership, substring containment, and same-city
/// knowledge graph edges (future). Does NOT check exact match — caller does that.
fn area_is_nearby(property_area: &str, canonical_area: &str) -> bool {
    use super::intent::AREA_ALIASES;

    let prop_lower = property_area.to_lowercase();
    let canon_lower = canonical_area.to_lowercase();

    // 1. Property area is a known alias of the canonical area
    for (aliases, canonical) in AREA_ALIASES {
        if !canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        for alias in *aliases {
            if prop_lower.contains(alias) || alias.contains(prop_lower.as_str()) {
                return true;
            }
        }
        break;
    }

    // 2. Property area maps to the same canonical area via its own aliases
    //    e.g. property area "Varthur" → canonical "Whitefield", search area is "Whitefield"
    for (aliases, canonical) in AREA_ALIASES {
        if !canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        // Check if any word in the property area matches an alias
        for word in prop_lower.split_whitespace() {
            for alias in *aliases {
                if *alias == word {
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

fn match_label_from_score(normalized: f64) -> String {
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

/// Map a preference to its corresponding legacy fact key.
fn legacy_fact_key_for_preference(preference: &str) -> String {
    match preference {
        "metro access" => "metro_distance_mins",
        "quiet neighborhood" => "noise_score",
        "value for money" => "price_per_sqft",
        "premium" => "price_per_sqft",
        "good society" => "society_quality_score",
        "reliable builder" | "trusted builder" => "builder_quality_score",
        "on time delivery" => "possession_status",
        "greenery" => "greenery_score",
        "new construction" | "under construction" | "ready to move" | "new launch" | "delayed"
        | "upcoming" => "possession_status",
        "high floor" => "floor",
        "east facing" => "facing",
        _ => "unknown",
    }
    .to_string()
}

/// Build a human-readable display string for a legacy preference match.
fn format_legacy_display(preference: &str, property: &Property) -> String {
    match preference {
        "metro access" => format!("{} min to metro", property.metro_distance_mins),
        "quiet neighborhood" => {
            if property.noise_score < 0.3 {
                "Quiet neighborhood".into()
            } else {
                "Moderately quiet area".into()
            }
        }
        "value for money" => format!("{}/sqft — good value", property.price_per_sqft),
        "premium" => format!("{}/sqft — premium segment", property.price_per_sqft),
        "good society" => {
            if property.society_quality_score >= 0.8 {
                "Strong society quality".into()
            } else {
                "Decent society quality".into()
            }
        }
        "reliable builder" | "trusted builder" => {
            if property.builder_quality_score >= 0.8 {
                "Strong builder quality".into()
            } else {
                "Acceptable builder quality".into()
            }
        }
        "on time delivery" => format!("Status: {}", property.possession_status),
        "greenery" => "Green surroundings".into(),
        "new construction" | "under construction" | "new launch" | "delayed" | "upcoming" => {
            format!("Status: {}", property.possession_status)
        }
        "ready to move" => format!("Status: {}", property.possession_status),
        "high floor" => format!("Floor {}/{}", property.floor, property.total_floors),
        "east facing" => format!("Facing: {}", property.facing),
        _ => format!("Matches {}", preference),
    }
}

fn build_match_reason(intent: &SearchIntent, reasons: &[String]) -> String {
    let mut parts = Vec::new();

    if let Some(ref area) = intent.area {
        parts.push(format!("Matches {}", area));
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

fn preference_was_matched(reasons: &[String], preference: &str) -> bool {
    let expected = format!("matches preference: {}", preference);
    reasons.iter().any(|reason| reason == &expected)
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
            .filter(|pref| !pref.starts_with("avoid "))
            .cloned()
            .collect()
    }
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
            .filter_map(|pref| pref.strip_prefix("avoid ").map(str::to_string))
            .collect()
    }
}

struct LocalPreferenceEvaluation {
    score_delta: f64,
    fact_key: &'static str,
    display: String,
    status: &'static str,
    score: f64,
}

fn legacy_negative_preference_evaluation(
    property: &Property,
    preference: &str,
) -> Option<LocalPreferenceEvaluation> {
    match preference {
        "waterlogging risk" => Some(lower_risk_evaluation(
            property.waterlogging_risk_score,
            "waterlogging_risk_score",
            "waterlogging",
            0.2,
            0.5,
            1.4,
        )),
        "traffic" => Some(lower_risk_evaluation(
            property.traffic_score,
            "traffic_score",
            "traffic",
            0.35,
            0.6,
            1.3,
        )),
        "noise" => Some(lower_risk_evaluation(
            property.noise_score,
            "noise_score",
            "noise",
            0.3,
            0.5,
            1.0,
        )),
        "legal risk" => Some(lower_risk_evaluation(
            property.litigation_risk,
            "litigation_risk",
            "legal",
            0.15,
            0.3,
            1.5,
        )),
        "delay risk" => Some(delay_risk_evaluation(property)),
        _ => None,
    }
}

fn lower_risk_evaluation(
    value: f64,
    fact_key: &'static str,
    label: &'static str,
    good_max: f64,
    ok_max: f64,
    weight: f64,
) -> LocalPreferenceEvaluation {
    if value <= good_max {
        LocalPreferenceEvaluation {
            score_delta: 2.0 * weight,
            fact_key,
            display: format!("Low {} risk", label),
            status: "matched",
            score: 1.0,
        }
    } else if value <= ok_max {
        LocalPreferenceEvaluation {
            score_delta: 0.7 * weight,
            fact_key,
            display: format!("Moderate {} risk", label),
            status: "partial",
            score: 0.5,
        }
    } else {
        LocalPreferenceEvaluation {
            score_delta: -2.0 * weight,
            fact_key,
            display: format!("High {} risk", label),
            status: "partial",
            score: 0.0,
        }
    }
}

fn delay_risk_evaluation(property: &Property) -> LocalPreferenceEvaluation {
    let status = property.possession_status.to_lowercase();
    if status.contains("delay") || status.contains("behind") {
        LocalPreferenceEvaluation {
            score_delta: -2.8,
            fact_key: "possession_status",
            display: format!("Delay risk in status: {}", property.possession_status),
            status: "partial",
            score: 0.0,
        }
    } else if status.contains("under") || status.contains("construction") {
        LocalPreferenceEvaluation {
            score_delta: -0.8,
            fact_key: "possession_status",
            display: format!(
                "Handover still depends on status: {}",
                property.possession_status
            ),
            status: "partial",
            score: 0.4,
        }
    } else {
        LocalPreferenceEvaluation {
            score_delta: 2.0,
            fact_key: "possession_status",
            display: format!("No delay signal in status: {}", property.possession_status),
            status: "matched",
            score: 1.0,
        }
    }
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
    use crate::knowledge::fact::{FactValue, SourceType, SourcedFact};
    use crate::knowledge::graph::KnowledgeGraph;
    use crate::knowledge::node::{Node, NodeType, RootSource};
    use crate::search::schema::SQM_PER_ACRE;

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
        // At exactly FACT_COVERAGE_THRESHOLD facts, coverage should be 1.0
        let g = graph_with_society_node(
            "calibrated",
            Some(RootSource::Legacy),
            FACT_COVERAGE_THRESHOLD as usize,
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
            society_quality_score: 0.7,
            builder_quality_score: 0.7,
            document_completeness_score: 0.8,
            litigation_risk: 0.1,
            noise_score,
            sunlight_score: 0.7,
            airport_noise_score: 0.1,
            waterlogging_risk_score: 0.1,
            traffic_score: 0.6,
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
            seller_id: None,
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
        ];
        let index = crate::search::SearchIndex::build(&properties);
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");

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
        ];
        let society_names = local_society_names(&properties);
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");

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
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");

        let results = TextSearch::search_with_index_and_intent_and_sellers(
            &properties,
            Some(&index),
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
            &[],
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
        let properties = vec![indexed_fit.clone(), bundle_fit, bundle_over_budget];
        let society_names = local_society_names(&properties);
        let stale_local_index = crate::search::SearchIndex::build(&[indexed_fit]);
        let serving_candidate_ids =
            vec!["bundle-fit".to_string(), "bundle-over-budget".to_string()];
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr");

        let results = TextSearch::search_with_index_and_extra_recall_and_intent_and_sellers(
            &properties,
            Some(&stale_local_index),
            Some(&serving_candidate_ids),
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr",
            &intent,
            None,
            &[],
        );

        let ids: Vec<&str> = results.iter().map(|r| r.card.id.as_str()).collect();
        assert_eq!(ids, vec!["indexed-fit", "bundle-fit"]);
    }

    #[test]
    fn test_search_ranks_local_preference_fit_before_weaker_match() {
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
        let intent =
            crate::search::intent::parse_intent("quiet 3BHK near metro in Whitefield under 2Cr");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "quiet 3BHK near metro in Whitefield under 2Cr",
            &intent,
            None,
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
                .any(|r| r.preference == "metro access" && r.scoring_method == "legacy"),
            "metro preference should be explained by local legacy scoring when no graph fact is present"
        );
        assert!(
            explanation
                .reasons
                .iter()
                .any(|r| r.preference == "quiet neighborhood" && r.scoring_method == "legacy"),
            "quiet preference should be explained by local legacy scoring when no graph fact is present"
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
        low_risk.waterlogging_risk_score = 0.1;

        let mut high_risk = local_property(
            "high-waterlogging",
            "Whitefield",
            "high-waterlogging",
            3,
            19_000_000,
            8,
            0.2,
        );
        high_risk.waterlogging_risk_score = 0.85;

        let properties = vec![high_risk, low_risk];
        let society_names = local_society_names(&properties);
        let intent =
            crate::search::intent::parse_intent("3BHK Whitefield under 2Cr avoid waterlogging");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr avoid waterlogging",
            &intent,
            None,
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
                    && r.scoring_method == "local-risk"),
            "avoid-waterlogging should be explained by local risk scoring"
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
        low_traffic.traffic_score = 0.2;

        let mut heavy_traffic = local_property(
            "heavy-traffic",
            "Whitefield",
            "heavy-traffic",
            3,
            19_000_000,
            8,
            0.2,
        );
        heavy_traffic.traffic_score = 0.9;

        let properties = vec![heavy_traffic, low_traffic];
        let society_names = local_society_names(&properties);
        let intent = crate::search::intent::parse_intent("3BHK Whitefield under 2Cr less traffic");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr less traffic",
            &intent,
            None,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].card.id, "low-traffic");
        assert!(results[0].match_score > results[1].match_score);
    }

    #[test]
    fn test_search_ranks_reliable_builder_locally() {
        let mut stronger_builder = local_property(
            "stronger-builder",
            "Whitefield",
            "stronger-builder",
            3,
            19_000_000,
            8,
            0.2,
        );
        stronger_builder.builder_quality_score = 0.9;

        let mut weaker_builder = local_property(
            "weaker-builder",
            "Whitefield",
            "weaker-builder",
            3,
            19_000_000,
            8,
            0.2,
        );
        weaker_builder.builder_quality_score = 0.35;

        let properties = vec![weaker_builder, stronger_builder];
        let society_names = local_society_names(&properties);
        let intent =
            crate::search::intent::parse_intent("3BHK Whitefield under 2Cr reliable builder");

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &[],
            "3BHK Whitefield under 2Cr reliable builder",
            &intent,
            None,
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
}
