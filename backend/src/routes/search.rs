use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::knowledge::edge::Relation;
use crate::knowledge::fact::ScoringDirection;
use crate::knowledge::search_event::EnrichmentGap;
use crate::knowledge::{KnowledgeGraph, SearchEvent};
use crate::search::{
    guard_search_query, intent, no_results_guidance, schema, KnowledgeContext, SearchEngine,
    SearchEvidenceGap, SearchResponse, SearchResultCard, SourcedClaim,
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
            focus: None,
            knowledge_context: None,
            search_diagnostics: None,
            relaxations: Vec::new(),
            search_guidance: None,
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
                intent: guarded.intent,
                results: Vec::new(),
                area_context: None,
                total_results: 0,
                focus: None,
                knowledge_context: None,
                search_diagnostics: None,
                relaxations: Vec::new(),
                search_guidance: Some(guarded.guidance),
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
    let (engine_output, focus) = {
        let graph = state.knowledge.read().await;

        let engine_output = SearchEngine {
            properties: &snapshot.properties,
            search_index: &snapshot.search_index,
            serving_bundle: Some(snapshot.bundle.as_ref()),
            society_names: &snapshot.society_names,
            property_by_id: Some(&snapshot.property_by_id),
            societies: &snapshot.societies,
            graph: Some(&graph),
        }
        .search(&query);

        let focus = crate::search::build_search_result_focus(crate::search::FocusBuildInputs {
            query: &query,
            intent: &engine_output.intent,
            results: &engine_output.results,
            properties: &snapshot.properties,
            society_names: &snapshot.society_names,
            societies: &snapshot.societies,
            serving_facts,
            graph: Some(&graph),
        });

        (engine_output, focus)
    };
    let parsed_intent = engine_output.intent;
    let results = engine_output.results;
    let relaxations = engine_output.relaxations;
    let search_evidence_gaps = engine_output.evidence_gaps;

    // Look up area context if the intent identified an area.
    let area_context = parsed_intent.area.as_ref().and_then(|area_name| {
        snapshot
            .areas
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(area_name))
            .cloned()
    });

    let total_results = results.len();
    let evidence_claims = result_evidence_claims(&results);

    // --- Extract knowledge context from the graph ---
    let graph = state.knowledge.read().await;
    let (knowledge_context, graph_nodes_hit, enrichment_gaps, gap_candidate_society_ids) = {
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
    let mut event = SearchEvent::new(query.clone(), parsed_intent.clone(), total_results);
    event.graph_nodes_hit = graph_nodes_hit;
    event.enrichment_gaps = enrichment_gaps.clone();
    let mut log_messages = vec![SearchLogMessage::SearchEvent(event)];
    if !enrichment_gaps.is_empty() {
        log_messages.push(SearchLogMessage::PersistEnrichmentGaps(
            EnrichmentGapPersistence {
                gaps: enrichment_gaps.clone(),
                query: query.clone(),
                intent: parsed_intent.clone(),
                results_returned: total_results,
                top_candidate_society_ids: gap_candidate_society_ids.clone(),
            },
        ));
    }
    for message in log_messages.clone() {
        enqueue_search_log(&state, message);
    }

    let buyer_knowledge_context = KnowledgeContext {
        claims: knowledge_context.claims,
        nodes_consulted: knowledge_context.nodes_consulted,
        learning_gaps: Vec::new(),
    };

    let response = SearchResponse {
        query,
        intent: parsed_intent,
        results,
        area_context,
        total_results,
        focus,
        knowledge_context: Some(buyer_knowledge_context),
        search_diagnostics: None,
        relaxations,
        search_guidance: (total_results == 0).then(no_results_guidance),
    };
    if response.total_results > 0 {
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
    }

    Json(response)
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

    for constraint in &intent.hard_constraints {
        if let Some(schema) = schema::numeric_constraint_schema(&constraint.field) {
            let mut match_labels = vec![constraint.raw_text.clone(), schema.label.to_string()];
            match_labels.push(schema.dimension.replace('_', " "));
            prefs.push(GapPreference {
                label: constraint.raw_text.clone(),
                match_labels,
                candidate_fact_keys: schema.fact_keys.iter().map(|key| key.to_string()).collect(),
                gap_fact_keys: Vec::new(),
                reason: format!("Hard constraint: {}", constraint.raw_text),
            });
        }
    }

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
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::knowledge::fact::{ScoringDirection, ScoringHint};
    use crate::knowledge::FactValue;
    use std::sync::atomic::{AtomicU64, Ordering};

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
            intent: intent.clone(),
            results: Vec::new(),
            area_context: None,
            total_results: 2,
            focus: None,
            knowledge_context: None,
            search_diagnostics: None,
            relaxations: Vec::new(),
            search_guidance: None,
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
    !snapshot
        .search_index
        .recall_ids(query, &guarded.intent)
        .is_empty()
}
