use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::knowledge::edge::Relation;
use crate::knowledge::fact::ScoringDirection;
use crate::knowledge::search_event::EnrichmentGap;
use crate::knowledge::{KnowledgeGraph, SearchEvent};
use crate::search::{
    intent, schema, KnowledgeContext, SearchIndex, SearchResponse, SearchResultCard, SourcedClaim,
    TextSearch,
};
use crate::serving::LoadedServingBundle;
use crate::state::AppState;

use super::enrichment::society_node_id;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

/// GET /api/search?q=... — local intent-based search over the knowledge graph.
pub async fn search_properties(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let query = params.q.unwrap_or_default();

    if query.trim().is_empty() {
        return Json(SearchResponse {
            query,
            intent: intent::SearchIntent {
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
            },
            results: Vec::new(),
            area_context: None,
            total_results: 0,
            knowledge_context: None,
            discovery_status: None,
            discovery_count: None,
        });
    }

    // Parse structured intent from the natural-language query.
    let parsed_intent = intent::parse_intent(&query);

    // Build society name lookup map.
    let society_names: HashMap<String, String> = state
        .societies
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();

    // Look up area context if the intent identified an area.
    let area_context = parsed_intent.area.as_ref().and_then(|area_name| {
        state
            .areas
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(area_name))
            .cloned()
    });

    let serving_bundle = state.serving_bundle.read().await.clone();
    let serving_facts = serving_bundle.as_ref().map(|bundle| &bundle.fact_index);
    let results = {
        let graph = state.knowledge.read().await;
        let properties = state.properties.read().await;
        let search_index = state.search_index.read().await;
        let semantic_index = state.semantic_index.read().await;
        let sellers = state.sellers.read().await;
        let serving_candidate_ids =
            serving_candidate_ids(serving_bundle.as_deref(), &query, &search_index);
        let semantic_hits = semantic_index.search(&query, state.semantic_embedder.as_ref(), 128);
        let semantic_scores = search_index.property_scores_for_semantic_hits(&semantic_hits);
        let semantic_candidate_ids = semantic_scores.keys().cloned().collect::<Vec<_>>();
        let extra_candidate_ids =
            merge_candidate_ids(serving_candidate_ids, semantic_candidate_ids);
        let semantic_scores = (!semantic_scores.is_empty()).then_some(&semantic_scores);
        let ranking_graph = if serving_facts.is_some() {
            None
        } else {
            Some(&*graph)
        };
        TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
            &properties,
            Some(&*search_index),
            extra_candidate_ids.as_deref(),
            semantic_scores,
            serving_facts,
            &society_names,
            &state.societies,
            &query,
            &parsed_intent,
            ranking_graph,
            &sellers,
        )
    };

    let total_results = results.len();
    let evidence_claims = result_evidence_claims(&results);

    // --- Extract knowledge context from the graph ---
    let graph = state.knowledge.read().await;
    let properties = state.properties.read().await;
    let (knowledge_context, graph_nodes_hit, enrichment_gaps) = {
        let matched_society_ids: Vec<String> = results
            .iter()
            .filter_map(|r| {
                properties
                    .iter()
                    .find(|p| p.id == r.card.id)
                    .map(|p| p.society_id.clone())
            })
            .collect();

        build_knowledge_context(
            &graph,
            serving_facts,
            &matched_society_ids,
            &parsed_intent,
            evidence_claims,
        )
    };
    drop(properties);
    drop(graph);

    // --- Log search event ---
    let mut event = SearchEvent::new(query.clone(), parsed_intent.clone(), total_results);
    event.graph_nodes_hit = graph_nodes_hit;
    event.enrichment_gaps = enrichment_gaps.clone();
    persist_enrichment_gaps(&enrichment_gaps);

    {
        let mut graph = state.knowledge.write().await;
        graph.log_search(event);
    }

    Json(SearchResponse {
        query,
        intent: parsed_intent,
        results,
        area_context,
        total_results,
        knowledge_context: Some(knowledge_context),
        discovery_status: None,
        discovery_count: None,
    })
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

fn serving_candidate_ids(
    serving_bundle: Option<&LoadedServingBundle>,
    query: &str,
    search_index: &SearchIndex,
) -> Option<Vec<String>> {
    let serving_bundle = serving_bundle?;
    let hits = match serving_bundle.recall_index.search(query, 128) {
        Ok(hits) => hits,
        Err(err) => {
            eprintln!("WARN: Serving bundle recall failed; using local recall only: {err}");
            return None;
        }
    };
    let ids = search_index.property_ids_for_entity_hits(&hits);
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

// ---------------------------------------------------------------------------
// Knowledge context builder — graph-first
// ---------------------------------------------------------------------------

/// Build knowledge context from the graph for matched results.
/// Returns (KnowledgeContext, graph_nodes_hit, enrichment_gaps).
fn build_knowledge_context(
    graph: &KnowledgeGraph,
    serving_facts: Option<&crate::serving::ServingFactIndex>,
    society_ids: &[String],
    intent: &intent::SearchIntent,
    claims: Vec<SourcedClaim>,
) -> (KnowledgeContext, Vec<String>, Vec<EnrichmentGap>) {
    let mut nodes_consulted = 0;
    let mut learning_gaps = Vec::new();
    let mut graph_nodes_hit = Vec::new();
    let mut enrichment_gaps = Vec::new();

    for society_id in society_ids {
        let node_id = society_node_id(society_id);

        if let Some(node) = graph.get_node(&node_id) {
            nodes_consulted += 1;
            graph_nodes_hit.push(node_id.clone());

            // Record related nodes consulted while evaluating query evidence gaps.
            for edge in graph.edges_from(&node_id) {
                if !matches!(edge.relation, Relation::BuiltBy | Relation::SocietyInArea) {
                    continue;
                }
                if graph.get_node(&edge.to).is_some() {
                    graph_nodes_hit.push(edge.to.clone());
                }
            }

            // --- Detect learning gaps ---
            // Structured intent already knows which fact keys answer a preference.
            // Use that before falling back to the legacy preference->fact map.
            for pref in gap_preferences(intent) {
                if serving_facts
                    .is_some_and(|facts| serving_has_gap_evidence(facts, &node_id, &pref))
                    || node_has_gap_evidence(node, &pref)
                    || related_node_has_gap_evidence(graph, &node_id, Relation::BuiltBy, &pref)
                    || related_node_has_gap_evidence(
                        graph,
                        &node_id,
                        Relation::SocietyInArea,
                        &pref,
                    )
                {
                    continue;
                }

                if let Some(needed_fact) = pref.candidate_fact_keys.first() {
                    learning_gaps.push(format!("{}: missing {} data", node.name, needed_fact));
                    enrichment_gaps.push(EnrichmentGap {
                        entity_id: node_id.clone(),
                        missing_fact: needed_fact.clone(),
                        reason: pref.reason,
                    });
                } else {
                    let missing_fact =
                        format!("unknown:{}", pref.label.to_lowercase().replace(' ', "_"));
                    learning_gaps.push(format!(
                        "{}: no knowledge about '{}'",
                        node.name, pref.label
                    ));
                    enrichment_gaps.push(EnrichmentGap {
                        entity_id: node_id.clone(),
                        missing_fact,
                        reason: format!("Unknown preference: {}", pref.label),
                    });
                }
            }
        }
    }

    for inventory_type in &intent.unsupported_inventory_types {
        learning_gaps.push(format!(
            "Unsupported inventory request: {} inventory is not in the current apartment corpus",
            inventory_type
        ));
        enrichment_gaps.push(EnrichmentGap {
            entity_id: format!("inventory:{}", inventory_type.replace(' ', "-")),
            missing_fact: "inventory_type".to_string(),
            reason: format!("User asked for unsupported inventory type: {inventory_type}"),
        });
    }

    let context = KnowledgeContext {
        claims,
        nodes_consulted,
        learning_gaps,
    };

    (context, graph_nodes_hit, enrichment_gaps)
}

fn result_evidence_claims(results: &[SearchResultCard]) -> Vec<SourcedClaim> {
    const MAX_KNOWLEDGE_CLAIMS: usize = 12;

    let mut claims = Vec::new();
    for result in results {
        let Some(explanation) = &result.match_explanation else {
            continue;
        };
        let entity_name = if result.card.society_name.trim().is_empty() {
            result.card.title.clone()
        } else {
            result.card.society_name.clone()
        };
        for reason in &explanation.reasons {
            let claim = SourcedClaim {
                entity_name: entity_name.clone(),
                claim: reason.display.clone(),
                confidence: reason.confidence,
                source_type: reason.source_type.clone(),
            };
            if !claims.iter().any(|existing: &SourcedClaim| {
                existing.entity_name == claim.entity_name && existing.claim == claim.claim
            }) {
                claims.push(claim);
                if claims.len() == MAX_KNOWLEDGE_CLAIMS {
                    return claims;
                }
            }
        }
    }
    claims
}

struct GapPreference {
    label: String,
    match_labels: Vec<String>,
    candidate_fact_keys: Vec<String>,
    reason: String,
}

fn gap_preferences(intent: &intent::SearchIntent) -> Vec<GapPreference> {
    let mut prefs = Vec::new();

    for constraint in &intent.hard_constraints {
        if let Some(schema) = schema::numeric_constraint_schema(&constraint.field) {
            let mut match_labels = vec![constraint.raw_text.clone(), schema.label.to_string()];
            match_labels.push(schema.dimension.replace('_', " "));
            prefs.push(GapPreference {
                label: constraint.raw_text.clone(),
                match_labels,
                candidate_fact_keys: schema.fact_keys.iter().map(|key| key.to_string()).collect(),
                reason: format!("Hard constraint: {}", constraint.raw_text),
            });
        }
    }

    for pref in &intent.positive_preferences {
        prefs.push(GapPreference {
            label: pref.raw_text.clone(),
            match_labels: vec![pref.raw_text.clone()],
            candidate_fact_keys: pref.expanded_keys.clone(),
            reason: format!("User preference: {}", pref.raw_text),
        });
    }

    for pref in &intent.negative_preferences {
        prefs.push(GapPreference {
            label: format!("avoid {}", pref.raw_text),
            match_labels: vec![pref.raw_text.clone(), format!("avoid {}", pref.raw_text)],
            candidate_fact_keys: pref.expanded_keys.clone(),
            reason: format!("Avoid preference: {}", pref.raw_text),
        });
    }

    if prefs.is_empty() {
        for pref in &intent.preferences {
            let is_negative = pref.starts_with("avoid ");
            let normalized = pref
                .strip_prefix("avoid ")
                .unwrap_or(pref.as_str())
                .to_string();
            let candidate_fact_keys =
                crate::search::schema::expanded_keys_for_preference_label(&normalized, is_negative);
            prefs.push(GapPreference {
                label: pref.clone(),
                match_labels: vec![pref.clone(), normalized.clone()],
                candidate_fact_keys,
                reason: format!("User preference: {}", pref),
            });
        }
    }

    prefs
}

fn node_has_gap_evidence(node: &crate::knowledge::node::Node, pref: &GapPreference) -> bool {
    node.facts
        .iter()
        .any(|fact| fact_matches_gap_preference(fact, pref))
}

fn serving_has_gap_evidence(
    serving_facts: &crate::serving::ServingFactIndex,
    entity_id: &str,
    pref: &GapPreference,
) -> bool {
    let Some(rows) = serving_facts.entity(entity_id) else {
        return false;
    };

    rows.facts.iter().any(|fact| {
        serving_fact_value_is_usable(&fact.value)
            && (pref
                .candidate_fact_keys
                .iter()
                .any(|key| fact.fact_key.eq_ignore_ascii_case(key))
                || rows.search_metadata_for_fact_key(&fact.fact_key)
                    .any(|metadata| {
                        metadata.answers_preferences.iter().any(|answer| {
                            pref.match_labels
                                .iter()
                                .any(|label| fuzzy_preference_match(answer, label))
                        })
                    }))
    })
}

fn serving_fact_value_is_usable(value: &crate::knowledge::FactValue) -> bool {
    match value {
        crate::knowledge::FactValue::Numeric(value) => value.is_finite(),
        crate::knowledge::FactValue::Text(value) => !value.trim().is_empty(),
        crate::knowledge::FactValue::Bool(_) => true,
        crate::knowledge::FactValue::Tags(values) => {
            values.iter().any(|value| !value.trim().is_empty())
        }
        crate::knowledge::FactValue::Score { value, .. } => value.is_finite(),
    }
}

fn related_node_has_gap_evidence(
    graph: &KnowledgeGraph,
    node_id: &str,
    relation: Relation,
    pref: &GapPreference,
) -> bool {
    graph.edges_from(node_id).iter().any(|edge| {
        edge.relation == relation
            && graph
                .get_node(&edge.to)
                .is_some_and(|related| node_has_gap_evidence(related, pref))
    })
}

fn fact_matches_gap_preference(fact: &crate::knowledge::SourcedFact, pref: &GapPreference) -> bool {
    pref.candidate_fact_keys
        .iter()
        .any(|key| fact.key.eq_ignore_ascii_case(key))
        || fact.answers_preferences.iter().any(|answer| {
            pref.match_labels
                .iter()
                .any(|label| fuzzy_preference_match(answer, label))
        })
}

fn fuzzy_preference_match(left: &str, right: &str) -> bool {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    left == right || left.contains(&right) || right.contains(&left)
}

// ---------------------------------------------------------------------------
// Graph-driven preference scoring (used by text.rs)
// ---------------------------------------------------------------------------

/// Score how well a node's facts match a user preference, using the graph's own
/// scoring_hint metadata. Returns a score 0.0-2.0 and fact detail for MatchReason.
pub fn graph_preference_score_detailed(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
) -> Option<(f64, GraphFactDetail)> {
    graph_preference_score_for_keys(graph, society_id, preference, &[])
}

pub fn graph_preference_score_for_keys(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
    candidate_fact_keys: &[String],
) -> Option<(f64, GraphFactDetail)> {
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;
    let pref_lower = preference.to_lowercase();

    // Find any fact that declares it answers this preference AND has a scoring_hint.
    // Uses contains-based fuzzy matching so "ready to move" matches
    // "ready to move", "ready possession", etc. and vice versa.
    for fact in &node.facts {
        let answers = fact.answers_preferences.iter().any(|ap| {
            let ap_lower = ap.to_lowercase();
            ap_lower == pref_lower
                || ap_lower.contains(&pref_lower)
                || pref_lower.contains(&ap_lower)
        });

        let key_matches = candidate_fact_keys
            .iter()
            .any(|key| key.eq_ignore_ascii_case(&fact.key));
        let keyed_match = answers
            || (key_matches
                && fact
                    .scoring_hint
                    .as_ref()
                    .is_some_and(|hint| !matches!(hint.direction, ScoringDirection::TextMatch)));
        if (!candidate_fact_keys.is_empty() && !keyed_match)
            || (candidate_fact_keys.is_empty() && !answers)
        {
            continue;
        }

        let score = if let Some(ref hint) = fact.scoring_hint {
            score_fact_with_hint(&fact.value, hint)
        } else {
            1.0
        };
        if score <= 0.0 {
            continue;
        }

        let display = render_template(
            fact.display_template.as_deref().unwrap_or("{value}"),
            &fact.value,
        );

        let detail = GraphFactDetail {
            fact_key: fact.key.clone(),
            display,
            confidence: fact.confidence,
            source_type: format!("{:?}", fact.source.source_type),
        };

        return Some((score, detail));
    }

    if let Some(schema) = schema::numeric_evidence_schema(preference) {
        for fact in &node.facts {
            let key_matches = schema
                .fact_keys
                .iter()
                .any(|key| key.eq_ignore_ascii_case(&fact.key))
                || candidate_fact_keys
                    .iter()
                    .any(|key| key.eq_ignore_ascii_case(&fact.key));
            if !key_matches {
                continue;
            }
            let Some(score) = score_fact_with_numeric_schema(&fact.value, schema) else {
                continue;
            };
            if score <= 0.0 {
                continue;
            }
            let value = render_template("{value}", &fact.value);
            return Some((
                score,
                GraphFactDetail {
                    fact_key: fact.key.clone(),
                    display: format!("{}: {}", schema.display_label, value),
                    confidence: fact.confidence,
                    source_type: format!("{:?}", fact.source.source_type),
                },
            ));
        }
    }

    // --- Cross-node scoring: traverse BuiltBy edge to check builder facts ---
    if let Some(result) = check_builder_facts(graph, &node_id, &pref_lower, candidate_fact_keys) {
        return Some(result);
    }

    None // No fact answers this preference
}

/// Traverse BuiltBy edges from a society node to its builder node and check
/// builder-level facts for preference matches. Returns the first match found.
fn check_builder_facts(
    graph: &KnowledgeGraph,
    society_node_id: &str,
    pref_lower: &str,
    candidate_fact_keys: &[String],
) -> Option<(f64, GraphFactDetail)> {
    for edge in graph.edges_from(society_node_id) {
        if edge.relation != Relation::BuiltBy {
            continue;
        }
        let builder_node = graph.get_node(&edge.to)?;
        for fact in &builder_node.facts {
            let answers = fact.answers_preferences.iter().any(|ap| {
                let ap_lower = ap.to_lowercase();
                ap_lower == *pref_lower
                    || ap_lower.contains(pref_lower)
                    || pref_lower.contains(&ap_lower)
            });

            let key_matches = candidate_fact_keys
                .iter()
                .any(|key| key.eq_ignore_ascii_case(&fact.key));
            let keyed_match = answers
                || (key_matches
                    && fact.scoring_hint.as_ref().is_some_and(|hint| {
                        !matches!(hint.direction, ScoringDirection::TextMatch)
                    }));
            if (!candidate_fact_keys.is_empty() && !keyed_match)
                || (candidate_fact_keys.is_empty() && !answers)
            {
                continue;
            }

            let score = if let Some(ref hint) = fact.scoring_hint {
                score_fact_with_hint(&fact.value, hint)
            } else {
                1.0
            };
            if score <= 0.0 {
                continue;
            }

            let display = render_template(
                fact.display_template.as_deref().unwrap_or("{value}"),
                &fact.value,
            );

            let detail = GraphFactDetail {
                fact_key: fact.key.clone(),
                display,
                confidence: fact.confidence,
                source_type: format!("{:?}", fact.source.source_type),
            };

            return Some((score, detail));
        }
    }
    None
}

/// Metadata from a graph fact, used to build MatchReason.
pub struct GraphFactDetail {
    pub fact_key: String,
    pub display: String,
    pub confidence: f32,
    pub source_type: String,
}

/// Apply a scoring hint to a fact value. Returns 0.0 - weight (typically 0-2).
fn score_fact_with_hint(
    value: &crate::knowledge::FactValue,
    hint: &crate::knowledge::fact::ScoringHint,
) -> f64 {
    let weight = hint.weight as f64;

    match &hint.direction {
        ScoringDirection::HigherIsBetter => {
            let num = fact_to_numeric(value).unwrap_or(0.0);
            if hint.thresholds.len() >= 2 {
                // thresholds: [good, ok] e.g. [0.8, 0.5]
                if num >= hint.thresholds[0] {
                    weight // full score
                } else if num >= hint.thresholds[1] {
                    weight * 0.5 // partial
                } else {
                    0.0
                }
            } else {
                // No thresholds: linear scale assuming 0-1 range
                num.clamp(0.0, 1.0) * weight
            }
        }
        ScoringDirection::LowerIsBetter => {
            let num = fact_to_numeric(value).unwrap_or(f64::MAX);
            if hint.thresholds.len() >= 2 {
                // thresholds: [good, ok] e.g. [10.0, 20.0] for metro_distance
                if num <= hint.thresholds[0] {
                    weight
                } else if num <= hint.thresholds[1] {
                    weight * 0.5
                } else {
                    0.0
                }
            } else {
                // No thresholds: inverse scale
                let score = (1.0 - num.clamp(0.0, 1.0)) * weight;
                score.max(0.0)
            }
        }
        ScoringDirection::TextMatch => {
            // For text facts: if matched via answers_preferences, the match already
            // proves relevance — score at full weight unless the value is explicitly
            // negative. This handles category values like "ready_to_move",
            // "under_construction" etc. that aren't sentiment words.
            let text = fact_to_search_text(value).to_lowercase();
            let negative = [
                "poor",
                "bad",
                "low",
                "terrible",
                "worst",
                "dangerous",
                "unsafe",
            ];
            let partial = ["average", "moderate", "mixed", "ok"];

            if negative.iter().any(|n| text.contains(n)) {
                0.0
            } else if partial.iter().any(|p| text.contains(p)) {
                weight * 0.5
            } else if !text.is_empty() {
                // Non-empty, non-negative text: score at full weight.
                // This covers both sentiment words ("good", "high") and
                // category values ("ready_to_move", "new_launch") that were
                // matched via answers_preferences.
                weight
            } else {
                0.0
            }
        }
    }
}

fn score_fact_with_numeric_schema(
    value: &crate::knowledge::FactValue,
    schema: &schema::NumericEvidenceSchema,
) -> Option<f64> {
    let num = fact_to_numeric(value)?;
    if !num.is_finite() {
        return None;
    }
    let weight = schema.score_delta.clamp(0.0, 2.0);
    if schema.direction.eq_ignore_ascii_case("HigherIsBetter")
        || schema.direction.eq_ignore_ascii_case("higher_is_better")
    {
        if schema.thresholds.len() >= 2 {
            if num >= schema.thresholds[0] {
                Some(weight)
            } else if num >= schema.thresholds[1] {
                Some(weight * 0.5)
            } else {
                None
            }
        } else {
            Some(num.clamp(0.0, 1.0) * weight).filter(|score| *score > 0.0)
        }
    } else if schema.direction.eq_ignore_ascii_case("LowerIsBetter")
        || schema.direction.eq_ignore_ascii_case("lower_is_better")
    {
        if schema.thresholds.len() >= 2 {
            if num <= schema.thresholds[0] {
                Some(weight)
            } else if num <= schema.thresholds[1] {
                Some(weight * 0.5)
            } else {
                None
            }
        } else {
            Some((1.0 - num.clamp(0.0, 1.0)) * weight).filter(|score| *score > 0.0)
        }
    } else {
        None
    }
}

fn fact_to_numeric(value: &crate::knowledge::FactValue) -> Option<f64> {
    match value {
        crate::knowledge::FactValue::Numeric(n) => Some(*n),
        crate::knowledge::FactValue::Score { value: v, .. } => Some(*v),
        _ => None,
    }
}

fn fact_to_search_text(value: &crate::knowledge::FactValue) -> String {
    match value {
        crate::knowledge::FactValue::Text(s) => s.clone(),
        crate::knowledge::FactValue::Tags(tags) => tags.join(" "),
        crate::knowledge::FactValue::Score { explanation, .. } => explanation.clone(),
        crate::knowledge::FactValue::Bool(value) => value.to_string(),
        crate::knowledge::FactValue::Numeric(value) => value.to_string(),
    }
}

/// Render a display template by replacing `{value}` with the fact's value.
fn render_template(template: &str, value: &crate::knowledge::FactValue) -> String {
    let value_str = match value {
        crate::knowledge::FactValue::Text(s) => s.clone(),
        crate::knowledge::FactValue::Numeric(n) => {
            if *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{:.1}", n)
            }
        }
        crate::knowledge::FactValue::Bool(b) => {
            if *b {
                "yes".to_string()
            } else {
                "no".to_string()
            }
        }
        crate::knowledge::FactValue::Tags(tags) => tags.join(", "),
        crate::knowledge::FactValue::Score {
            value: v,
            explanation: _,
        } => format!("{:.1}", v),
    };
    template.replace("{value}", &value_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::fact::{ScoringDirection, ScoringHint};
    use crate::knowledge::FactValue;

    // --- Day 62: TextMatch scoring fix tests ---

    #[test]
    fn test_textmatch_scores_category_values() {
        // "ready_to_move" is a category value, not a sentiment word.
        // It should score at full weight because it's non-empty and non-negative.
        let value = FactValue::Text("ready_to_move".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(
            score, 3.0,
            "ready_to_move should score at full weight (3.0), got {}",
            score
        );
    }

    #[test]
    fn test_textmatch_scores_under_construction() {
        let value = FactValue::Text("under_construction".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(score, 3.0, "under_construction should score at full weight");
    }

    #[test]
    fn test_textmatch_scores_new_launch() {
        let value = FactValue::Text("new_launch".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(score, 3.0, "new_launch should score at full weight");
    }

    #[test]
    fn test_textmatch_zero_for_negative_values() {
        let value = FactValue::Text("poor".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(score, 0.0, "negative value 'poor' should score 0.0");
    }

    #[test]
    fn test_textmatch_zero_for_empty_string() {
        let value = FactValue::Text("".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(score, 0.0, "empty text should score 0.0");
    }

    #[test]
    fn test_textmatch_partial_for_mixed() {
        let value = FactValue::Text("mixed".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 3.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(
            score, 1.5,
            "partial value 'mixed' should score 0.5 * weight"
        );
    }

    #[test]
    fn test_textmatch_still_works_for_sentiment_words() {
        // Existing behavior must be preserved: "good" still scores full weight
        let value = FactValue::Text("good".to_string());
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 2.0,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert_eq!(
            score, 2.0,
            "sentiment word 'good' should still score at full weight"
        );
    }

    #[test]
    fn test_textmatch_scores_non_empty_tags() {
        let value = FactValue::Tags(vec!["Greenwood High (1.2 km, 4.3 rating)".to_string()]);
        let hint = ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 0.8,
            thresholds: vec![],
        };
        let score = score_fact_with_hint(&value, &hint);
        assert!(
            (score - 0.8).abs() < 0.00001,
            "non-empty tag evidence should score, got {score}"
        );
    }

    #[test]
    fn test_negative_preference_gap_uses_structured_fact_key() {
        use crate::knowledge::node::{Node, NodeType};

        let mut graph = crate::knowledge::KnowledgeGraph::new();
        graph.add_node(Node::new(
            "society:test-society",
            NodeType::Society,
            "Test Society",
        ));

        let intent = crate::search::intent::parse_intent("3bhk whitefield no waterlogging");
        let (_, _, enrichment_gaps) = build_knowledge_context(
            &graph,
            None,
            &["test-society".to_string()],
            &intent,
            Vec::new(),
        );

        assert!(
            enrichment_gaps
                .iter()
                .any(|gap| gap.missing_fact == "waterlogging_risk_score"),
            "Expected waterlogging_risk_score gap, got {:?}",
            enrichment_gaps
        );
        assert!(
            !enrichment_gaps
                .iter()
                .any(|gap| gap.missing_fact.starts_with("unknown:")),
            "Structured negative preferences should not be logged as unknown gaps"
        );
    }

    #[test]
    fn serving_gap_evidence_requires_a_usable_value() {
        use crate::serving::{ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord};
        use chrono::Utc;

        let pref = GapPreference {
            label: "greenery".to_string(),
            match_labels: vec!["greenery".to_string()],
            candidate_fact_keys: vec!["resident_greenery_signal".to_string()],
            reason: "User preference: greenery".to_string(),
        };
        let metadata = ServingSearchMetadataRecord {
            entity_id: "society:test-society".to_string(),
            fact_key: "resident_greenery_signal".to_string(),
            display_template: Some("Residents report {value}".to_string()),
            answers_preferences: vec!["greenery".to_string()],
            scoring_direction: Some("TextMatch".to_string()),
            scoring_weight: Some(1.0),
            scoring_thresholds: Vec::new(),
        };
        let fact = |value| ServingFactRecord {
            entity_id: "society:test-society".to_string(),
            fact_key: "resident_greenery_signal".to_string(),
            value_type: "text".to_string(),
            value_text: None,
            value,
            confidence: 0.7,
            source_type: "Reddit".to_string(),
            source_url: None,
            model: None,
            skill_id: Some("reddit_resident_facts".to_string()),
            learned_at: Utc::now(),
        };

        let empty = ServingFactIndex::from_records(
            vec![fact(FactValue::Text("   ".to_string()))],
            vec![metadata.clone()],
        );
        assert!(!serving_has_gap_evidence(
            &empty,
            "society:test-society",
            &pref
        ));

        let usable = ServingFactIndex::from_records(
            vec![fact(FactValue::Text("mature trees".to_string()))],
            vec![metadata],
        );
        assert!(serving_has_gap_evidence(
            &usable,
            "society:test-society",
            &pref
        ));
    }

    // --- Day 62: Graph scoring integration tests ---

    fn make_test_fact(status: &str, answers: Vec<&str>) -> crate::knowledge::fact::SourcedFact {
        use crate::knowledge::fact::{FactSource, SourceType};
        use chrono::Utc;

        crate::knowledge::fact::SourcedFact {
            key: "project_status".to_string(),
            value: FactValue::Text(status.to_string()),
            confidence: 1.0,
            source: FactSource {
                source_type: SourceType::Computed,
                url: None,
                model: None,
                skill_id: Some("classify_project_status".to_string()),
                triggered_by: None,
            },
            learned_at: Utc::now(),
            version: 1,
            display_template: Some(status.replace('_', " ").to_string()),
            answers_preferences: answers.into_iter().map(String::from).collect(),
            scoring_hint: Some(ScoringHint {
                direction: ScoringDirection::TextMatch,
                weight: 3.0,
                thresholds: vec![],
            }),
        }
    }

    fn make_test_node(
        graph: &mut crate::knowledge::KnowledgeGraph,
        slug: &str,
        name: &str,
        fact: crate::knowledge::fact::SourcedFact,
    ) {
        use crate::knowledge::node::{Node, NodeType};

        let mut node = Node::new(format!("society:{}", slug), NodeType::Society, name);
        node.add_fact(fact);
        graph.add_node(node);
    }

    #[test]
    fn test_graph_preference_score_with_project_status() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();

        let fact = make_test_fact(
            "ready_to_move",
            vec![
                "ready to move",
                "ready possession",
                "completed project",
                "immediate possession",
            ],
        );
        make_test_node(&mut graph, "prestige-test", "Prestige Test", fact);

        // Test: "ready to move" preference should match and score 3.0
        let result = graph_preference_score_detailed(&graph, "prestige-test", "ready to move");
        assert!(
            result.is_some(),
            "Should find matching fact for 'ready to move'"
        );
        let (score, detail) = result.unwrap();
        assert_eq!(
            score, 3.0,
            "ready_to_move fact should score 3.0, got {}",
            score
        );
        assert_eq!(detail.fact_key, "project_status");

        // Test: "under construction" should NOT match this society
        let result = graph_preference_score_detailed(&graph, "prestige-test", "under construction");
        assert!(
            result.is_none(),
            "Should NOT match 'under construction' for a ready_to_move society"
        );
    }

    #[test]
    fn structured_candidate_keys_reject_unrelated_preference_facts() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();
        let mut google_fact = make_test_fact("positive", vec!["resident feedback"]);
        google_fact.key = "google_sentiment".to_string();
        make_test_node(
            &mut graph,
            "source-specific",
            "Source Specific Society",
            google_fact,
        );

        let reddit_keys = vec!["reddit_thread_count".to_string()];
        assert!(graph_preference_score_for_keys(
            &graph,
            "source-specific",
            "reddit discussions",
            &reddit_keys,
        )
        .is_none());
    }

    #[test]
    fn zero_scored_fact_is_not_positive_preference_evidence() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();
        let mut reddit_fact = make_test_fact("unused", vec!["resident feedback"]);
        reddit_fact.key = "reddit_thread_count".to_string();
        reddit_fact.value = FactValue::Numeric(0.0);
        reddit_fact.scoring_hint = Some(ScoringHint {
            direction: ScoringDirection::HigherIsBetter,
            weight: 1.0,
            thresholds: vec![5.0, 2.0],
        });
        make_test_node(
            &mut graph,
            "no-reddit-evidence",
            "No Reddit Evidence Society",
            reddit_fact,
        );

        let reddit_keys = vec!["reddit_thread_count".to_string()];
        assert!(graph_preference_score_for_keys(
            &graph,
            "no-reddit-evidence",
            "reddit discussions",
            &reddit_keys,
        )
        .is_none());
    }

    #[test]
    fn numeric_schema_scores_premium_without_per_fact_hint() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();
        let mut premium_fact = crate::knowledge::fact::SourcedFact::manual(
            "price_per_sqft",
            FactValue::Numeric(26_500.0),
        );
        premium_fact.confidence = 0.7;
        premium_fact.source.source_type = crate::knowledge::fact::SourceType::Google;
        premium_fact.source.url = Some("https://maps.google.com/?cid=test".to_string());
        premium_fact.display_template = Some("{value}/sqft".to_string());
        premium_fact.answers_preferences = Vec::new();
        premium_fact.scoring_hint = None;
        make_test_node(
            &mut graph,
            "k-raheja-vivarea",
            "K Raheja Vivarea",
            premium_fact,
        );

        let premium_keys = vec!["price_per_sqft".to_string()];
        let (score, detail) =
            graph_preference_score_for_keys(&graph, "k-raheja-vivarea", "premium", &premium_keys)
                .expect("registry numeric evidence should score premium price signals");

        assert_eq!(score, 2.0);
        assert_eq!(detail.fact_key, "price_per_sqft");
        assert_eq!(detail.display, "Premium price signal: 26500");
        assert_eq!(detail.source_type, "Google");
    }

    #[test]
    fn test_graph_fuzzy_matching_answers_preferences() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();

        let fact = make_test_fact(
            "under_construction",
            vec!["under construction", "ongoing project", "in progress"],
        );
        make_test_node(&mut graph, "test-society", "Test Society", fact);

        // Exact match
        let result = graph_preference_score_detailed(&graph, "test-society", "under construction");
        assert!(
            result.is_some(),
            "Exact match 'under construction' should work"
        );
        assert_eq!(result.unwrap().0, 3.0);

        // Fuzzy: preference contains answers_preference substring
        let result = graph_preference_score_detailed(&graph, "test-society", "ongoing");
        // "ongoing" is contained in "ongoing project" → should match
        assert!(
            result.is_some(),
            "Fuzzy match: 'ongoing' should match 'ongoing project'"
        );
    }

    // --- Day 63: Cross-node builder scoring tests ---

    fn make_builder_fact(
        key: &str,
        value: FactValue,
        answers: Vec<&str>,
        direction: ScoringDirection,
        weight: f32,
    ) -> crate::knowledge::fact::SourcedFact {
        use crate::knowledge::fact::{FactSource, SourceType};
        use chrono::Utc;

        crate::knowledge::fact::SourcedFact {
            key: key.to_string(),
            value,
            confidence: 0.9,
            source: FactSource {
                source_type: SourceType::Computed,
                url: None,
                model: None,
                skill_id: Some("compute_builder_delivery_rate".to_string()),
                triggered_by: None,
            },
            learned_at: Utc::now(),
            version: 1,
            display_template: Some("Builder delivers on time: 80% of projects".to_string()),
            answers_preferences: answers.into_iter().map(String::from).collect(),
            scoring_hint: Some(ScoringHint {
                direction,
                weight,
                thresholds: vec![],
            }),
        }
    }

    #[test]
    fn test_cross_node_builder_scoring() {
        use crate::knowledge::edge::Edge;
        use crate::knowledge::node::{Node, NodeType};

        let mut graph = crate::knowledge::KnowledgeGraph::new();

        // Create society node (no builder-related facts)
        let society_node = Node::new(
            "society:test-society".to_string(),
            NodeType::Society,
            "Test Society",
        );
        graph.add_node(society_node);

        // Create builder node with delivery_rate fact
        let mut builder_node = Node::new(
            "builder:test-builder".to_string(),
            NodeType::Builder,
            "Test Builder",
        );
        builder_node.add_fact(make_builder_fact(
            "builder_delivery_rate",
            FactValue::Numeric(0.8),
            vec!["reliable builder", "trusted builder", "on time delivery"],
            ScoringDirection::HigherIsBetter,
            2.5,
        ));
        graph.add_node(builder_node);

        // Create BuiltBy edge: society -> builder
        let edge = Edge::new(
            "society:test-society".to_string(),
            "builder:test-builder".to_string(),
            Relation::BuiltBy,
        );
        graph.add_edge(edge);

        // Test: "reliable builder" should score via cross-node traversal
        let result = graph_preference_score_detailed(&graph, "test-society", "reliable builder");
        assert!(
            result.is_some(),
            "Should find builder fact via BuiltBy edge for 'reliable builder'"
        );
        let (score, detail) = result.unwrap();
        assert!(score > 0.0, "Score should be positive, got {}", score);
        assert_eq!(detail.fact_key, "builder_delivery_rate");
    }

    #[test]
    fn test_no_cross_node_scoring_without_edge() {
        use crate::knowledge::node::{Node, NodeType};

        let mut graph = crate::knowledge::KnowledgeGraph::new();

        // Create society node (no facts, no edges)
        let society_node = Node::new(
            "society:orphan-society".to_string(),
            NodeType::Society,
            "Orphan Society",
        );
        graph.add_node(society_node);

        // Create builder node with facts but NO edge connecting them
        let mut builder_node = Node::new(
            "builder:unlinked-builder".to_string(),
            NodeType::Builder,
            "Unlinked Builder",
        );
        builder_node.add_fact(make_builder_fact(
            "builder_delivery_rate",
            FactValue::Numeric(1.0),
            vec!["reliable builder"],
            ScoringDirection::HigherIsBetter,
            2.5,
        ));
        graph.add_node(builder_node);

        // Test: should NOT score because there is no BuiltBy edge
        let result = graph_preference_score_detailed(&graph, "orphan-society", "reliable builder");
        assert!(
            result.is_none(),
            "Should NOT match builder fact without BuiltBy edge"
        );
    }

    #[test]
    fn test_builder_zero_revocations_textmatch() {
        use crate::knowledge::edge::Edge;
        use crate::knowledge::node::{Node, NodeType};

        let mut graph = crate::knowledge::KnowledgeGraph::new();

        let society_node = Node::new(
            "society:revoc-test".to_string(),
            NodeType::Society,
            "Revoc Test Society",
        );
        graph.add_node(society_node);

        let mut builder_node = Node::new(
            "builder:clean-builder".to_string(),
            NodeType::Builder,
            "Clean Builder",
        );
        builder_node.add_fact(make_builder_fact(
            "builder_zero_revocations",
            FactValue::Text("true".to_string()),
            vec!["reliable builder", "trusted builder", "no delays"],
            ScoringDirection::TextMatch,
            2.0,
        ));
        graph.add_node(builder_node);

        let edge = Edge::new(
            "society:revoc-test".to_string(),
            "builder:clean-builder".to_string(),
            Relation::BuiltBy,
        );
        graph.add_edge(edge);

        let result = graph_preference_score_detailed(&graph, "revoc-test", "trusted builder");
        assert!(
            result.is_some(),
            "Should match builder_zero_revocations for 'trusted builder'"
        );
        let (score, _) = result.unwrap();
        assert_eq!(
            score, 2.0,
            "TextMatch 'true' should score at full weight 2.0"
        );
    }

    #[test]
    fn test_no_graph_node_returns_none_for_preferences() {
        // When a society has no KG node, graph_preference_score_detailed returns None.
        // Search should rely on serving/graph evidence only — no seed-field fallback.
        let graph = crate::knowledge::KnowledgeGraph::new();

        let result =
            graph_preference_score_detailed(&graph, "nonexistent-society", "reliable builder");
        assert!(
            result.is_none(),
            "Should return None for non-existent society"
        );

        let result = graph_preference_score_detailed(&graph, "nonexistent-society", "metro access");
        assert!(
            result.is_none(),
            "Should return None for non-existent society on standard preferences too"
        );
    }

    #[test]
    fn test_no_kg_node_still_matches_hard_constraints() {
        // A property whose society has no KG node should still match hard constraints
        // (area, BHK) but not receive legacy preference scoring.
        use crate::search::TextSearch;

        let graph = crate::knowledge::KnowledgeGraph::new();

        // Create a property with fields that match legacy preferences
        let prop = crate::models::Property {
            id: "no-kg-prop".into(),
            title: "Test Property No KG".into(),
            area: "TestArea".into(),
            area_id: "test-area".into(),
            city: "Bengaluru".into(),
            society_id: "no-kg-society".into(),
            builder_name: "Test Builder".into(),
            property_type: "Apartment".into(),
            listing_type: "Resale".into(),
            bhk: 3,
            price: 10_000_000,
            price_per_sqft: 7500,
            carpet_area_sqft: 1200,
            super_builtup_sqft: 1500,
            floor: 5,
            total_floors: 20,
            facing: "East".into(),
            possession_status: "Ready to Move".into(),
            metro_distance_mins: 5,
            maintenance_cost_monthly: 5000,
            society_quality_score: Some(0.8),
            builder_quality_score: Some(0.7),
            document_completeness_score: Some(0.9),
            litigation_risk: Some(0.1),
            noise_score: Some(0.2),
            sunlight_score: Some(0.7),
            airport_noise_score: Some(0.1),
            waterlogging_risk_score: Some(0.1),
            traffic_score: Some(0.3),
            days_on_market: 30,
            greenery_score: Some(0.6),
            open_space_score: Some(0.5),
            resale_strength_score: Some(0.7),
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: vec![],
            hero_image: String::new(),
            description_summary: "Test property".into(),
            transparency_tags: vec![],
            source_reference: "test".into(),
            seller_id: None,
        };

        let properties = vec![prop];
        let societies: Vec<crate::models::Society> = vec![];
        let mut society_names = std::collections::HashMap::new();
        society_names.insert("no-kg-society".to_string(), "No KG Society".to_string());

        let intent = crate::search::SearchIntent {
            area: Some("TestArea".into()),
            excluded_areas: Vec::new(),
            bhk: Some(3),
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: vec!["metro access".into(), "quiet neighborhood".into()],
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &societies,
            "3bhk TestArea metro access quiet",
            &intent,
            Some(&graph),
        );

        assert_eq!(
            results.len(),
            1,
            "Should return 1 result even without KG node"
        );
        let result = &results[0];
        assert!(
            result.match_score > 0.0,
            "Should have positive score from hard-constraint floor"
        );

        // Confidence should still be computed (low, since no KG node)
        assert!(
            result.confidence_score.is_some(),
            "Should have confidence score"
        );
        let conf = result.confidence_score.as_ref().unwrap();
        // With no KG node, source_quality=0.3, coverage=0.0, freshness=0.3, match=0.0
        // Weighted: 0.3*0.4 + 0.0*0.2 + 0.3*0.2 + 0.0*0.2 = 0.18
        assert!(
            conf.overall < 0.4,
            "Confidence should be low without KG data, got {}",
            conf.overall
        );
        assert_eq!(conf.label, "Low");
    }
}

fn persist_enrichment_gaps(gaps: &[EnrichmentGap]) {
    if gaps.is_empty() {
        return;
    }

    let path = enrichment_gaps_output_path();
    let mut entries: Vec<serde_json::Value> = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|payload| serde_json::from_str(&payload).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let recorded_at = Utc::now().to_rfc3339();
    for gap in gaps {
        entries.push(serde_json::json!({
            "entity_id": gap.entity_id,
            "missing_fact": gap.missing_fact,
            "reason": gap.reason,
            "recorded_at": recorded_at,
        }));
    }

    if entries.len() > 500 {
        let start = entries.len() - 500;
        entries = entries.split_off(start);
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(payload) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(path, payload);
    }
}

fn enrichment_gaps_output_path() -> PathBuf {
    if let Ok(path) = std::env::var("OPENESTATES_ENRICHMENT_GAPS_PATH") {
        return PathBuf::from(path);
    }
    PathBuf::from("data/validation/enrichment_gaps.json")
}
