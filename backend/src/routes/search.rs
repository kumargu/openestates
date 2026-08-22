use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::knowledge::edge::Relation;
use crate::knowledge::search_event::EnrichmentGap;
use crate::knowledge::{KnowledgeGraph, SearchEvent};
use crate::search::{
    guard_search_query, intent, schema, KnowledgeContext, SearchEngine, SearchEvidenceGap,
    SearchResponse, SearchResultCard, SearchResultSet, SourcedClaim,
};
use crate::state::{
    AppState, CachedSearchOutput, EnrichmentGapPersistence, SearchCacheKey, SearchLogMessage,
};

use super::enrichment::society_node_id;

const MAX_GAP_ENTITIES_PER_SEARCH: usize = 5;
const MAX_LEARNING_GAPS_PER_SEARCH: usize = 20;

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
            result_sets: Vec::new(),
            total_matches: 0,
            area_context: None,
            state: "no_matches".to_string(),
        });
    }

    if let Some(guarded) = guard_search_query(&query) {
        if guarded_search_has_local_recall(&state, &query, &guarded).await {
            // A loaded project/society name can look like a bare noun phrase to
            // the guardrail. If deterministic local recall has a concrete
            // candidate, let ranking handle the query instead of rejecting it.
        } else {
            let mut event = SearchEvent::new(query.clone(), guarded.intent.clone(), 0);
            event.enrichment_gaps.push(EnrichmentGap {
                entity_id: "search:guardrail".to_string(),
                missing_fact: guarded.guidance.mode.clone(),
                reason: guarded.guidance.message.clone(),
            });
            enqueue_search_log(&state, SearchLogMessage::SearchEvent(event));

            return Json(SearchResponse {
                query,
                result_sets: Vec::new(),
                total_matches: 0,
                area_context: None,
                state: "no_matches".to_string(),
            });
        }
    }

    let snapshot = state.search_runtime.load_full();
    let cache_key = SearchCacheKey::new(&query, &snapshot.version_key);
    if let Some(cached) = state.search_cache.get(&cache_key).await {
        for message in rebase_cached_log_messages(cached.log_messages, &query) {
            enqueue_search_log(&state, message);
        }
        return Json(rebase_cached_response(cached.response.as_ref(), &query));
    }

    let serving_facts = Some(&snapshot.bundle.fact_index);
    let engine_output = {
        let graph = state.knowledge.read().await;

        SearchEngine {
            properties: &snapshot.properties,
            search_index: &snapshot.search_index,
            serving_bundle: Some(snapshot.bundle.as_ref()),
            society_names: &snapshot.society_names,
            property_by_id: Some(&snapshot.property_by_id),
            societies: &snapshot.societies,
            graph: Some(&graph),
        }
        .search(&query)
    };
    let parsed_intent = engine_output.intent;
    let results = engine_output.results;
    let result_sets = engine_output.result_sets;
    let search_evidence_gaps = engine_output.evidence_gaps;

    // Look up area context if the intent identified an area.
    let area_context = parsed_intent.area.as_ref().and_then(|area_name| {
        snapshot
            .areas
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(area_name))
            .cloned()
    });

    let results_returned = results.len();
    let evidence_claims = result_evidence_claims(&results);

    // --- Extract knowledge context from the graph ---
    let graph = state.knowledge.read().await;
    let (_knowledge_context, graph_nodes_hit, enrichment_gaps, gap_candidate_society_ids) = {
        let mut matched_society_ids: Vec<String> = Vec::new();
        for result in &results {
            if let Some(society_id) = snapshot
                .properties
                .iter()
                .find(|p| p.id == result.card.id)
                .map(|p| p.society_id.clone())
            {
                push_unique_string(&mut matched_society_ids, society_id);
                if matched_society_ids.len() >= MAX_GAP_ENTITIES_PER_SEARCH {
                    break;
                }
            }
        }

        let (mut knowledge_context, graph_nodes_hit, mut enrichment_gaps) = build_knowledge_context(
            &graph,
            serving_facts,
            &matched_society_ids,
            &parsed_intent,
            evidence_claims,
        );
        merge_search_evidence_gaps(
            &mut knowledge_context,
            &mut enrichment_gaps,
            &search_evidence_gaps,
        );
        (
            knowledge_context,
            graph_nodes_hit,
            enrichment_gaps,
            matched_society_ids,
        )
    };
    drop(graph);

    // --- Log search event ---
    let mut event = SearchEvent::new(query.clone(), parsed_intent.clone(), results_returned);
    event.graph_nodes_hit = graph_nodes_hit;
    event.enrichment_gaps = enrichment_gaps.clone();
    let mut log_messages = vec![SearchLogMessage::SearchEvent(event)];
    if !enrichment_gaps.is_empty() {
        log_messages.push(SearchLogMessage::PersistEnrichmentGaps(
            EnrichmentGapPersistence {
                gaps: enrichment_gaps.clone(),
                query: query.clone(),
                intent: parsed_intent.clone(),
                results_returned,
                top_candidate_society_ids: gap_candidate_society_ids.clone(),
            },
        ));
    }
    for message in log_messages.clone() {
        enqueue_search_log(&state, message);
    }

    let total_matches = unique_result_count(&result_sets);
    let response = SearchResponse {
        query,
        result_sets,
        total_matches,
        area_context,
        state: if total_matches == 0 {
            "no_matches".to_string()
        } else {
            "results".to_string()
        },
    };
    state
        .search_cache
        .put(
            cache_key,
            CachedSearchOutput {
                response: Arc::new(response.clone()),
                log_messages,
            },
        )
        .await;

    Json(response)
}

fn unique_result_count(result_sets: &[SearchResultSet]) -> usize {
    result_sets
        .iter()
        .flat_map(|set| set.results.iter().map(|result| result.card.id.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn merge_search_evidence_gaps(
    knowledge_context: &mut KnowledgeContext,
    enrichment_gaps: &mut Vec<EnrichmentGap>,
    search_gaps: &[SearchEvidenceGap],
) {
    for gap in search_gaps {
        if enrichment_gaps.iter().any(|existing| {
            existing.entity_id == gap.entity_id
                && existing
                    .missing_fact
                    .eq_ignore_ascii_case(&gap.missing_fact)
        }) {
            continue;
        }
        if knowledge_context.learning_gaps.len() < MAX_LEARNING_GAPS_PER_SEARCH {
            knowledge_context.learning_gaps.push(format!(
                "{}: missing {} data",
                gap.entity_id, gap.missing_fact
            ));
        }
        enrichment_gaps.push(EnrichmentGap {
            entity_id: gap.entity_id.clone(),
            missing_fact: gap.missing_fact.clone(),
            reason: gap.reason.clone(),
        });
    }
}

fn enqueue_search_log(state: &AppState, message: SearchLogMessage) {
    try_enqueue_search_log(
        &state.search_event_tx,
        &state.search_log_dropped_count,
        message,
    );
}

fn try_enqueue_search_log(
    tx: &tokio::sync::mpsc::Sender<SearchLogMessage>,
    dropped_count: &std::sync::atomic::AtomicU64,
    message: SearchLogMessage,
) {
    if tx.try_send(message).is_err() {
        dropped_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn rebase_cached_response(response: &SearchResponse, query: &str) -> SearchResponse {
    let mut response = response.clone();
    response.query = query.to_string();
    response
}

fn rebase_cached_log_messages(
    messages: Vec<SearchLogMessage>,
    query: &str,
) -> Vec<SearchLogMessage> {
    messages
        .into_iter()
        .map(|message| match message {
            SearchLogMessage::SearchEvent(mut event) => {
                event.query = query.to_string();
                event.timestamp = chrono::Utc::now();
                SearchLogMessage::SearchEvent(event)
            }
            SearchLogMessage::PersistEnrichmentGaps(mut payload) => {
                payload.query = query.to_string();
                SearchLogMessage::PersistEnrichmentGaps(payload)
            }
        })
        .collect()
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
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
        if learning_gaps.len() >= MAX_LEARNING_GAPS_PER_SEARCH {
            break;
        }
        let node_id = society_node_id(society_id);
        let node = graph.get_node(&node_id);
        if node.is_some()
            || serving_facts
                .and_then(|facts| facts.entity(&node_id))
                .is_some()
        {
            nodes_consulted += 1;
        }

        if node.is_some() {
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
        }

        let entity_name = node
            .map(|node| node.name.as_str())
            .unwrap_or_else(|| fallback_entity_name(&node_id));

        for pref in gap_preferences(intent) {
            for needed_fact in missing_gap_fact_keys(graph, serving_facts, &node_id, node, &pref) {
                push_learning_gap(
                    &mut learning_gaps,
                    &mut enrichment_gaps,
                    entity_name,
                    &node_id,
                    &needed_fact,
                    &pref.reason,
                );
                if learning_gaps.len() >= MAX_LEARNING_GAPS_PER_SEARCH {
                    break;
                }
            }
            if learning_gaps.len() >= MAX_LEARNING_GAPS_PER_SEARCH {
                break;
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
    gap_fact_keys: Vec<String>,
    reason: String,
}

fn gap_preferences(intent: &intent::SearchIntent) -> Vec<GapPreference> {
    let mut prefs = Vec::new();

    for pref in &intent.positive_preferences {
        prefs.push(GapPreference {
            label: pref.raw_text.clone(),
            match_labels: vec![pref.raw_text.clone()],
            candidate_fact_keys: pref.expanded_keys.clone(),
            gap_fact_keys: pref.gap_keys.clone(),
            reason: format!("User preference: {}", pref.raw_text),
        });
    }

    for pref in &intent.negative_preferences {
        prefs.push(GapPreference {
            label: format!("avoid {}", pref.raw_text),
            match_labels: vec![pref.raw_text.clone(), format!("avoid {}", pref.raw_text)],
            candidate_fact_keys: pref.expanded_keys.clone(),
            gap_fact_keys: pref.gap_keys.clone(),
            reason: format!("Avoid preference: {}", pref.raw_text),
        });
    }

    if prefs.is_empty() {
        for pref in &intent.preferences {
            let signal = crate::search::schema::legacy_display_preference_signal(pref);
            let label = match signal.polarity {
                crate::search::intent::Polarity::Positive => signal.raw_text.clone(),
                crate::search::intent::Polarity::Negative => format!("avoid {}", signal.raw_text),
            };
            let mut match_labels = vec![label.clone()];
            if signal.polarity == crate::search::intent::Polarity::Negative {
                match_labels.push(signal.raw_text.clone());
            }
            prefs.push(GapPreference {
                label: label.clone(),
                match_labels,
                candidate_fact_keys: signal.expanded_keys,
                gap_fact_keys: signal.gap_keys,
                reason: format!("User preference: {}", label),
            });
        }
    }

    prefs
}

fn missing_gap_fact_keys(
    graph: &KnowledgeGraph,
    serving_facts: Option<&crate::serving::ServingFactIndex>,
    node_id: &str,
    node: Option<&crate::knowledge::node::Node>,
    pref: &GapPreference,
) -> Vec<String> {
    if !pref.gap_fact_keys.is_empty() {
        return pref
            .gap_fact_keys
            .iter()
            .filter(|fact_key| {
                !has_exact_gap_evidence(graph, serving_facts, node_id, node, fact_key)
            })
            .cloned()
            .collect();
    }

    if serving_facts.is_some_and(|facts| serving_has_gap_evidence(facts, node_id, pref))
        || node.is_some_and(|node| node_has_gap_evidence(node, pref))
        || related_node_has_gap_evidence(graph, node_id, Relation::BuiltBy, pref)
        || related_node_has_gap_evidence(graph, node_id, Relation::SocietyInArea, pref)
    {
        return Vec::new();
    }

    if let Some(needed_fact) = pref.candidate_fact_keys.first() {
        return vec![needed_fact.clone()];
    }

    vec![format!(
        "unknown:{}",
        pref.label.to_lowercase().replace(' ', "_")
    )]
}

fn has_exact_gap_evidence(
    graph: &KnowledgeGraph,
    serving_facts: Option<&crate::serving::ServingFactIndex>,
    node_id: &str,
    node: Option<&crate::knowledge::node::Node>,
    fact_key: &str,
) -> bool {
    serving_facts.is_some_and(|facts| serving_has_exact_fact(facts, node_id, fact_key))
        || node.is_some_and(|node| node_has_exact_fact(node, fact_key))
        || related_node_has_exact_fact(graph, node_id, Relation::BuiltBy, fact_key)
        || related_node_has_exact_fact(graph, node_id, Relation::SocietyInArea, fact_key)
}

fn serving_has_exact_fact(
    serving_facts: &crate::serving::ServingFactIndex,
    entity_id: &str,
    fact_key: &str,
) -> bool {
    serving_facts.entity(entity_id).is_some_and(|rows| {
        rows.facts.iter().any(|fact| {
            fact.fact_key.eq_ignore_ascii_case(fact_key)
                && fact.confidence >= schema::ranking_policy().min_support_evidence_confidence
                && serving_fact_value_is_usable(&fact.value)
        })
    })
}

fn node_has_exact_fact(node: &crate::knowledge::node::Node, fact_key: &str) -> bool {
    node.facts.iter().any(|fact| {
        fact.key.eq_ignore_ascii_case(fact_key)
            && fact.confidence >= schema::ranking_policy().min_support_evidence_confidence
            && sourced_fact_value_is_usable(&fact.value)
    })
}

fn related_node_has_exact_fact(
    graph: &KnowledgeGraph,
    node_id: &str,
    relation: Relation,
    fact_key: &str,
) -> bool {
    graph.edges_from(node_id).iter().any(|edge| {
        edge.relation == relation
            && graph
                .get_node(&edge.to)
                .is_some_and(|related| node_has_exact_fact(related, fact_key))
    })
}

fn push_learning_gap(
    learning_gaps: &mut Vec<String>,
    enrichment_gaps: &mut Vec<EnrichmentGap>,
    entity_name: &str,
    entity_id: &str,
    missing_fact: &str,
    reason: &str,
) {
    if enrichment_gaps.iter().any(|gap| {
        gap.entity_id == entity_id && gap.missing_fact.eq_ignore_ascii_case(missing_fact)
    }) {
        return;
    }

    learning_gaps.push(format!("{entity_name}: missing {missing_fact} data"));
    enrichment_gaps.push(EnrichmentGap {
        entity_id: entity_id.to_string(),
        missing_fact: missing_fact.to_string(),
        reason: reason.to_string(),
    });
}

fn fallback_entity_name(node_id: &str) -> &str {
    node_id.strip_prefix("society:").unwrap_or(node_id)
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
            && fact.confidence >= schema::ranking_policy().min_support_evidence_confidence
            && (pref
                .candidate_fact_keys
                .iter()
                .any(|key| fact.fact_key.eq_ignore_ascii_case(key))
                || rows
                    .search_metadata_for_fact_key(&fact.fact_key)
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

fn sourced_fact_value_is_usable(value: &crate::knowledge::FactValue) -> bool {
    serving_fact_value_is_usable(value)
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
    fact.confidence >= schema::ranking_policy().min_support_evidence_confidence
        && sourced_fact_value_is_usable(&fact.value)
        && (pref
            .candidate_fact_keys
            .iter()
            .any(|key| fact.key.eq_ignore_ascii_case(key))
            || fact.answers_preferences.iter().any(|answer| {
                pref.match_labels
                    .iter()
                    .any(|label| fuzzy_preference_match(answer, label))
            }))
}

fn fuzzy_preference_match(left: &str, right: &str) -> bool {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    left == right || left.contains(&right) || right.contains(&left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::FactValue;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn buyer_response_exposes_result_sets_without_internal_search_state() {
        let response = SearchResponse {
            query: "3bhk whitefield".to_string(),
            result_sets: Vec::new(),
            total_matches: 0,
            area_context: None,
            state: "no_matches".to_string(),
        };

        let value = serde_json::to_value(response).expect("search response should serialize");
        assert_eq!(value["query"], "3bhk whitefield");
        assert_eq!(value["resultSets"], serde_json::json!([]));
        assert_eq!(value["totalMatches"], 0);
        assert_eq!(value["state"], "no_matches");
        for internal in [
            "intent",
            "results",
            "searchDiagnostics",
            "knowledgeContext",
            "relaxations",
        ] {
            assert!(value.get(internal).is_none(), "leaked {internal}");
        }
    }

    #[test]
    fn search_evidence_gaps_are_persisted_and_available_only_for_debug_context() {
        let mut context = KnowledgeContext {
            claims: Vec::new(),
            nodes_consulted: 0,
            learning_gaps: Vec::new(),
        };
        let mut enrichment_gaps = Vec::new();
        let search_gaps = vec![
            SearchEvidenceGap {
                entity_id: "area:whitefield".to_string(),
                missing_fact: "geo.latitude".to_string(),
                reason: "area radius requires an anchor".to_string(),
            },
            SearchEvidenceGap {
                entity_id: "area:whitefield".to_string(),
                missing_fact: "geo.longitude".to_string(),
                reason: "area radius requires an anchor".to_string(),
            },
        ];

        merge_search_evidence_gaps(&mut context, &mut enrichment_gaps, &search_gaps);

        assert_eq!(context.learning_gaps.len(), 2);
        assert_eq!(enrichment_gaps.len(), 2);
        assert_eq!(enrichment_gaps[0].missing_fact, "geo.latitude");

        merge_search_evidence_gaps(&mut context, &mut enrichment_gaps, &search_gaps);
        assert_eq!(
            context.learning_gaps.len(),
            2,
            "gaps should be deduplicated"
        );
        assert_eq!(enrichment_gaps.len(), 2, "gaps should be deduplicated");
    }

    #[test]
    fn hard_constraints_are_not_flattened_into_result_gap_preferences() {
        let parsed = intent::parse_intent("3BHK above 10 acres under 2Cr or 4BHK under 4Cr");

        assert!(!parsed.hard_constraints.is_empty());
        assert!(
            gap_preferences(&parsed)
                .iter()
                .all(|preference| !preference.label.contains("acres")),
            "branch-local hard constraints must not become global result gaps"
        );
    }

    #[test]
    fn cached_response_and_log_messages_are_rebased_to_current_query() {
        let intent = intent::parse_intent("3bhk whitefield");
        let mut event = SearchEvent::new("3bhk whitefield".to_string(), intent.clone(), 2);
        let original_timestamp = event.timestamp;
        event.enrichment_gaps.push(EnrichmentGap {
            entity_id: "society:test".to_string(),
            missing_fact: "nearby_schools".to_string(),
            reason: "school access requested".to_string(),
        });
        let response = SearchResponse {
            query: "3bhk whitefield".to_string(),
            result_sets: Vec::new(),
            total_matches: 0,
            area_context: None,
            state: "no_matches".to_string(),
        };
        let messages = vec![
            SearchLogMessage::SearchEvent(event),
            SearchLogMessage::PersistEnrichmentGaps(EnrichmentGapPersistence {
                gaps: Vec::new(),
                query: "3bhk whitefield".to_string(),
                intent,
                results_returned: 2,
                top_candidate_society_ids: Vec::new(),
            }),
        ];

        let rebased_response = rebase_cached_response(&response, "  3BHK   Whitefield  ");
        let rebased_messages = rebase_cached_log_messages(messages, "  3BHK   Whitefield  ");

        assert_eq!(rebased_response.query, "  3BHK   Whitefield  ");
        match &rebased_messages[0] {
            SearchLogMessage::SearchEvent(rebased_event) => {
                assert_eq!(rebased_event.query, "  3BHK   Whitefield  ");
                assert!(
                    rebased_event.timestamp >= original_timestamp,
                    "cache-hit log event should get a fresh timestamp"
                );
            }
            SearchLogMessage::PersistEnrichmentGaps(_) => {
                panic!("first message should remain search event")
            }
        }
        match &rebased_messages[1] {
            SearchLogMessage::PersistEnrichmentGaps(payload) => {
                assert_eq!(payload.query, "  3BHK   Whitefield  ");
            }
            SearchLogMessage::SearchEvent(_) => {
                panic!("second message should remain enrichment-gap payload")
            }
        }
    }

    #[test]
    fn full_search_log_queue_increments_drop_counter() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let dropped = AtomicU64::new(0);
        let intent = intent::parse_intent("3bhk whitefield");

        try_enqueue_search_log(
            &tx,
            &dropped,
            SearchLogMessage::SearchEvent(SearchEvent::new(
                "3bhk whitefield".to_string(),
                intent.clone(),
                1,
            )),
        );
        try_enqueue_search_log(
            &tx,
            &dropped,
            SearchLogMessage::SearchEvent(SearchEvent::new(
                "3bhk whitefield".to_string(),
                intent,
                1,
            )),
        );

        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert!(rx.try_recv().is_ok());
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
    fn serving_only_runtime_emits_configured_gap_keys() {
        let graph = crate::knowledge::KnowledgeGraph::new();
        let serving_facts = crate::serving::ServingFactIndex::from_records(Vec::new(), Vec::new());
        let intent = crate::search::intent::parse_intent(
            "Need 3BHK Whitefield under 2.4Cr. no tanker dependency or daily water stress",
        );

        let (_, _, enrichment_gaps) = build_knowledge_context(
            &graph,
            Some(&serving_facts),
            &["soc-test-society".to_string()],
            &intent,
            Vec::new(),
        );

        assert!(
            enrichment_gaps
                .iter()
                .any(|gap| gap.missing_fact == "operating.tanker_dependence"),
            "Expected tanker gap, got {:?}",
            enrichment_gaps
        );
        assert!(
            enrichment_gaps
                .iter()
                .any(|gap| gap.missing_fact == "water_supply_risk"),
            "Expected water supply gap, got {:?}",
            enrichment_gaps
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
            gap_fact_keys: Vec::new(),
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

        let weak = ServingFactIndex::from_records(
            vec![ServingFactRecord {
                confidence: 0.4,
                ..fact(FactValue::Text("mature trees".to_string()))
            }],
            vec![metadata.clone()],
        );
        assert!(!serving_has_gap_evidence(
            &weak,
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

    #[test]
    fn test_no_kg_node_still_matches_hard_constraints() {
        // A property whose society has no KG node should still match hard constraints
        // (area, BHK) but not receive legacy preference scoring.
        use crate::search::{CompiledQuery, TextSearch, TextSearchRequest};

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
            price_min: None,
            price_max: None,
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
        };

        let properties = vec![prop];
        let societies: Vec<crate::models::Society> = vec![];
        let mut society_names = std::collections::HashMap::new();
        society_names.insert("no-kg-society".to_string(), "No KG Society".to_string());

        let intent = crate::search::SearchIntent {
            area: Some("TestArea".into()),
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: Some(3),
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: vec!["metro access".into(), "quiet neighborhood".into()],
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };

        let compiled_query =
            CompiledQuery::from_text_with_intent("3bhk TestArea metro access quiet", intent);
        let results = TextSearch::search(TextSearchRequest {
            properties: &properties,
            search_index: None,
            extra_candidate_ids: None,
            candidate_property_indexes: None,
            geo_query: None,
            serving_facts: None,
            society_names: &society_names,
            societies: &societies,
            compiled_query: &compiled_query,
            graph: Some(&graph),
        });

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

async fn guarded_search_has_local_recall(
    state: &AppState,
    query: &str,
    guarded: &crate::search::guard::GuardedSearch,
) -> bool {
    if !matches!(
        guarded.guidance.mode.as_str(),
        "too_short" | "out_of_scope" | "needs_home_anchor"
    ) {
        return false;
    }

    let snapshot = state.search_runtime.load_full();
    let compiled = crate::search::CompiledQuery::from_text(query);
    !snapshot.search_index.recall_ids(&compiled).is_empty()
}
