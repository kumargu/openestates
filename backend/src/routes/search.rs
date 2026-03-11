use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::discovery;
use crate::discovery::gemini::DiscoveryConstraints;
use crate::knowledge::{KnowledgeGraph, SearchEvent, store as kg_store};
use crate::knowledge::fact::ScoringDirection;
use crate::knowledge::search_event::EnrichmentGap;
use crate::search::{
    KnowledgeContext, SearchDebugTrace, SearchResponse, SocietyDebugScore,
    PreferenceDebugScore, SourcedClaim, TextSearch, intent, score_all_societies, synthesize_explanation,
};
use crate::state::AppState;

use super::enrichment::society_node_id;

/// Score threshold below which we trigger live discovery.
const DISCOVERY_THRESHOLD: f64 = 0.15;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub debug: Option<bool>,
}

/// GET /api/search?q=... — intent-based search with live discovery fallback.
pub async fn search_properties(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let query = params.q.unwrap_or_default();
    let debug_mode = params.debug.unwrap_or(false);
    let request_start = std::time::Instant::now();

    if query.trim().is_empty() {
        return Json(SearchResponse {
            query,
            intent: intent::SearchIntent {
                area: None,
                bhk: None,
                budget_max: None,
                preferences: Vec::new(),
                positive_preferences: Vec::new(),
                negative_preferences: Vec::new(),
                buyer_archetype: None,
            },
            results: Vec::new(),
            area_context: None,
            total_results: 0,
            knowledge_context: None,
            discovery_status: None,
            discovery_count: None,
            also_consider: Vec::new(),
            debug: None,
        });
    }

    // Parse structured intent from the natural-language query.
    let intent_start = std::time::Instant::now();
    let parsed_intent = intent::parse_intent(&query);
    let intent_parse_ms = intent_start.elapsed().as_millis() as u64;

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

    // Run text search and query embedding in parallel for minimal latency.
    let embedding_start = std::time::Instant::now();
    let (mut results, semantic_scores) = {
        let text_search_fut = async {
            let graph = state.knowledge.read().await;
            let properties = state.properties.read().await;
            TextSearch::search_with_intent(
                &properties,
                &society_names,
                &state.societies,
                &query,
                &parsed_intent,
                Some(&graph),
            )
        };

        let semantic_fut = async {
            if let Some(ref ec) = state.embed_client {
                let graph = state.knowledge.read().await;
                crate::search::semantic::semantic_society_scores(ec, &graph, &query, &parsed_intent, 20).await
            } else {
                HashMap::new()
            }
        };

        tokio::join!(text_search_fut, semantic_fut)
    };
    let embedding_ms = embedding_start.elapsed().as_millis() as u64;

    // --- Society-first scoring: re-rank using KG graph facts ---
    // Score all unique societies from candidates, then blend into match_score.
    // This runs in-memory (<1ms) and is always applied.
    let scoring_start = std::time::Instant::now();
    {
        let properties = state.properties.read().await;
        let graph = state.knowledge.read().await;

        // Collect unique society IDs from candidate results
        let society_ids: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            results.iter().filter_map(|r| {
                properties.iter()
                    .find(|p| p.id == r.card.id)
                    .map(|p| p.society_id.clone())
            })
            .filter(|id| seen.insert(id.clone()))
            .collect()
        };

        if !society_ids.is_empty() && (!parsed_intent.positive_preferences.is_empty()
            || !parsed_intent.negative_preferences.is_empty()
            || parsed_intent.buyer_archetype.is_some())
        {
            let society_scores = score_all_societies(&graph, &society_ids, &parsed_intent);

            // Attach society scores to results
            for result in &mut results {
                let Some(prop) = properties.iter().find(|p| p.id == result.card.id) else { continue };
                let node_id = society_node_id(&prop.society_id);
                let Some(ss) = society_scores.get(&node_id) else { continue };

                // Normalize society score (0–5 range typically) to 0–1
                let normalized = (ss.score / 5.0).min(1.0);

                // Blend: 60% society score + 40% existing text score
                result.match_score = (normalized as f64 * 0.6 + result.match_score * 0.4).min(1.0);
                result.match_label = match_label_from_score(result.match_score);
                result.society_score = Some(normalized);
                result.society_confidence = Some(confidence_label(ss.confidence));
                result.concerns = ss.concerns.clone();
                result.unmatched_preferences = ss.unmatched_preferences.clone();

                // Synthesize explanation card from society score
                let facts_consulted = graph
                    .get_node(&node_id)
                    .map(|n| n.facts.len())
                    .unwrap_or(0);
                let source_types: Vec<String> = graph
                    .get_node(&node_id)
                    .map(|n| {
                        n.facts.iter()
                            .map(|f| format!("{:?}", f.source.source_type))
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default();
                result.explanation_card = Some(synthesize_explanation(ss, facts_consulted, &source_types));
            }

            // Re-sort by updated scores
            results.sort_by(|a, b| {
                b.match_score
                    .partial_cmp(&a.match_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        drop(graph);
        drop(properties);
    }
    let scoring_ms = scoring_start.elapsed().as_millis() as u64;

    // --- Semantic recall → also_consider ---
    // Embeddings serve recall only; primary ranking is pure graph-fact scoring.
    // Trigger when results are sparse or weak (not when query has strong matches).
    let mut also_consider: Vec<crate::search::SearchResultCard> = Vec::new();

    let primary_strong = results.len() >= 5
        && results.iter().map(|r| r.match_score).fold(0.0f64, f64::max) >= 0.6;

    if !primary_strong && !semantic_scores.is_empty() {
        let properties = state.properties.read().await;
        let graph = state.knowledge.read().await;

        let already_shown: std::collections::HashSet<String> = results
            .iter()
            .filter_map(|r| {
                properties.iter()
                    .find(|p| p.id == r.card.id)
                    .map(|p| society_node_id(&p.society_id))
            })
            .collect();

        // Sort by similarity score descending, take top candidates
        let mut sim_candidates: Vec<(&String, f64)> = semantic_scores
            .iter()
            .filter(|(node_id, _)| !already_shown.contains(*node_id))
            .map(|(id, &sim)| (id, sim))
            .collect();
        sim_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (node_id, sim) in sim_candidates.iter().take(10) {
            if also_consider.len() >= 5 { break; }

            // Find a representative property for this society that passes hard constraints
            let Some(prop) = properties.iter().find(|p| {
                let prop_node_id = society_node_id(&p.society_id);
                if prop_node_id != **node_id { return false; }
                if let Some(bhk) = parsed_intent.bhk {
                    if p.bhk != bhk { return false; }
                }
                if let Some(budget) = parsed_intent.budget_max {
                    if p.price > budget { return false; }
                }
                true
            }) else { continue };

            let card = crate::routes::enrichment::enrich_property_card(prop, &state.societies, &graph);

            // Score via graph facts so the result has proper explanation
            let society_id_str = prop.society_id.clone();
            let society_ids_slice = vec![society_id_str];
            let ss_map = score_all_societies(&graph, &society_ids_slice, &parsed_intent);
            let ss = ss_map.get(node_id.as_str());

            let (society_score, society_confidence, concerns, unmatched, explanation_card) = if let Some(ss) = ss {
                let normalized = (ss.score / 5.0).min(1.0);
                let facts_consulted = graph.get_node(node_id).map(|n| n.facts.len()).unwrap_or(0);
                let source_types: Vec<String> = graph.get_node(node_id)
                    .map(|n| n.facts.iter()
                        .map(|f| format!("{:?}", f.source.source_type))
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter().collect())
                    .unwrap_or_default();
                let exp = synthesize_explanation(ss, facts_consulted, &source_types);
                (
                    Some(normalized as f32),
                    Some(confidence_label(ss.confidence)),
                    ss.concerns.clone(),
                    ss.unmatched_preferences.clone(),
                    Some(exp),
                )
            } else {
                (None, None, Vec::new(), Vec::new(), None)
            };

            let score = (sim * 0.5) as f64; // Recall score, not ranking signal
            also_consider.push(crate::search::SearchResultCard {
                card,
                match_score: score,
                match_label: "Similar profile".to_string(),
                match_reason: "Semantically similar to your search preferences".to_string(),
                match_explanation: None,
                semantic_score: Some((*sim * 100.0).round() / 100.0),
                society_score,
                society_confidence,
                concerns,
                unmatched_preferences: unmatched,
                explanation_card,
                active_seller_count: None,
            });
        }

        drop(graph);
        drop(properties);
    }

    // --- Live Discovery: if results are poor, discover on-the-fly ---
    let mut discovery_status: Option<String> = None;
    let mut discovery_count: Option<usize> = None;

    let max_score = results.iter().map(|r| r.match_score).fold(0.0f64, f64::max);
    let should_discover = (results.is_empty() || max_score < DISCOVERY_THRESHOLD)
        && parsed_intent.area.is_some()
        && state.gemini.is_some();

    if should_discover {
        let area = parsed_intent.area.as_ref().unwrap();
        let cache_key = discovery::DiscoveryCache::cache_key(
            area,
            parsed_intent.bhk,
            parsed_intent.budget_max,
        );

        // Check discovery cache first
        let mut dc = state.discovery_cache.lock().await;
        if let Some(cached) = dc.get(&cache_key) {
            // We have cached discoveries but they might not be in the property list yet.
            // The properties were already ingested on the first discovery call.
            discovery_status = Some("from_cache".to_string());
            discovery_count = Some(cached.len());
            drop(dc);
        } else if dc.can_discover() {
            drop(dc); // Release lock before async Gemini call

            let gemini = state.gemini.as_ref().unwrap();
            let constraints = DiscoveryConstraints {
                bhk: parsed_intent.bhk,
                budget_max: parsed_intent.budget_max,
            };

            match gemini.discover_properties(area, "Bangalore", &constraints).await {
                Ok((discoveries, area_identified)) => {
                    let area_canonical = area_identified.as_deref().unwrap_or(area);
                    let disc_count = discoveries.len();

                    // Cache the raw discoveries
                    {
                        let mut dc = state.discovery_cache.lock().await;
                        dc.put(cache_key, discoveries.clone());
                    }

                    // Ingest into knowledge graph
                    let new_properties = {
                        let mut graph = state.knowledge.write().await;
                        discovery::ingest::ingest_discoveries(
                            &discoveries,
                            &mut graph,
                            area_canonical,
                            &query,
                        )
                    };

                    if !new_properties.is_empty() {
                        // Persist to seed data
                        let existing_props = state.properties.read().await;
                        if let Err(e) = discovery::ingest::persist_to_seed(
                            &state.project_root,
                            &existing_props,
                            &new_properties,
                        ) {
                            eprintln!("WARN: Failed to persist discovered properties: {}", e);
                        }
                        drop(existing_props);

                        // Add to in-memory property list
                        {
                            let mut props = state.properties.write().await;
                            for p in &new_properties {
                                if !props.iter().any(|ep| ep.id == p.id) {
                                    props.push(p.clone());
                                }
                            }
                        }

                        // Persist knowledge graph
                        {
                            let graph = state.knowledge.read().await;
                            let kg_dir = kg_store::knowledge_dir(&state.project_root);
                            if let Err(e) = kg_store::save_graph(&kg_dir, &graph) {
                                eprintln!("WARN: Failed to persist knowledge graph: {}", e);
                            }
                        }

                        // Re-run search with expanded corpus
                        let graph = state.knowledge.read().await;
                        let properties = state.properties.read().await;
                        results = TextSearch::search_with_intent(
                            &properties,
                            &society_names,
                            &state.societies,
                            &query,
                            &parsed_intent,
                            Some(&graph),
                        );
                        drop(properties);
                        drop(graph);
                    }

                    discovery_status = Some("discovered_new".to_string());
                    discovery_count = Some(disc_count);
                }
                Err(e) => {
                    eprintln!("WARN: Live discovery failed: {}", e);
                    discovery_status = Some("discovery_failed".to_string());
                }
            }
        } else {
            drop(dc);
            discovery_status = Some("rate_limited".to_string());
        }
    }

    let total_results = results.len();

    // --- Attach seller counts to results ---
    {
        let sellers = state.sellers.read().await;
        for result in &mut results {
            let count = sellers
                .values()
                .filter(|s| s.listing_ids.contains(&result.card.id))
                .count() as u32;
            if count > 0 {
                result.active_seller_count = Some(count);
            }
        }
    }

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

        build_knowledge_context(&graph, &matched_society_ids, &parsed_intent)
    };
    drop(properties);
    drop(graph);

    // --- Log search event ---
    let mut event = SearchEvent::new(query.clone(), parsed_intent.clone(), total_results);
    event.graph_nodes_hit = graph_nodes_hit;
    event.enrichment_gaps = enrichment_gaps;

    let kg_dir = kg_store::knowledge_dir(&state.project_root);
    if let Err(e) = kg_store::append_search_log(&kg_dir, &event) {
        eprintln!("WARN: Failed to append search log: {}", e);
    }

    {
        let mut graph = state.knowledge.write().await;
        graph.log_search(event);
    }

    // --- Fire-and-forget: queue enrichment request for the searched area ---
    if let Some(ref area) = parsed_intent.area {
        let entity_ids: Vec<String> = results
            .iter()
            .filter_map(|r| {
                // Extract society node IDs from matched results
                let properties = state.properties.try_read().ok()?;
                properties
                    .iter()
                    .find(|p| p.id == r.card.id)
                    .map(|p| society_node_id(&p.society_id))
            })
            .collect();
        let root = state.project_root.clone();
        let area = area.clone();
        let prefs = parsed_intent.preferences.clone();
        let q = query.clone();
        tokio::spawn(async move {
            crate::enrichment_queue::append_enrichment_request(
                &root, &area, entity_ids, prefs, &q,
            );
        });
    }

    let total_latency_ms = request_start.elapsed().as_millis() as u64;

    // Build debug trace if requested
    let debug = if debug_mode {
        let societies_debug: Vec<SocietyDebugScore> = results.iter().take(10).map(|r| {
            let preference_scores: Vec<PreferenceDebugScore> = parsed_intent
                .positive_preferences.iter()
                .chain(parsed_intent.negative_preferences.iter())
                .map(|pref| {
                    let matched = r.explanation_card.as_ref()
                        .and_then(|ec| ec.why_matches.iter().find(|wm| wm.preference == pref.raw_text));
                    PreferenceDebugScore {
                        preference: pref.raw_text.clone(),
                        polarity: format!("{:?}", pref.polarity),
                        matched_fact_key: matched.map(|_| pref.canonical_key.clone()),
                        matched_fact_value: matched.map(|wm| wm.text.clone()),
                        raw_score: r.society_score.unwrap_or(0.0),
                        weighted_score: r.society_score.unwrap_or(0.0) * pref.weight,
                        source: r.society_confidence.clone(),
                        confidence: r.society_score.unwrap_or(0.0),
                    }
                })
                .collect();

            SocietyDebugScore {
                society_id: r.card.id.clone(),
                society_name: r.card.title.clone(),
                final_score: r.society_score.unwrap_or(0.0),
                confidence: r.society_score.unwrap_or(0.0),
                archetype_modifier: if parsed_intent.buyer_archetype.is_some() { 1.0 } else { 0.0 },
                area_signals_used: Vec::new(),
                facts_consulted: r.explanation_card.as_ref()
                    .map(|ec| ec.evidence_summary.facts_consulted)
                    .unwrap_or(0),
                preference_scores,
            }
        }).collect();

        Some(SearchDebugTrace {
            timestamp: chrono::Utc::now().to_rfc3339(),
            query: query.clone(),
            candidate_count: total_results,
            also_consider_triggered: !also_consider.is_empty(),
            also_consider_reason: if !also_consider.is_empty() {
                Some("Sparse or weak primary results".to_string())
            } else {
                None
            },
            discovery_triggered: discovery_status.as_deref() == Some("discovered_new"),
            total_latency_ms,
            intent_parse_ms,
            scoring_ms,
            embedding_ms,
            societies_scored: societies_debug,
        })
    } else {
        None
    };

    Json(SearchResponse {
        query,
        intent: parsed_intent,
        results,
        area_context,
        total_results,
        knowledge_context: Some(knowledge_context),
        discovery_status,
        discovery_count,
        also_consider,
        debug,
    })
}

// ---------------------------------------------------------------------------
// Legacy fallback maps — used only for seed facts without self-describing metadata.
// As skills populate display_template + answers_preferences, these shrink to nothing.
// ---------------------------------------------------------------------------

/// Legacy: maps a fact key to display format (for facts without display_template).
fn claim_format(fact_key: &str) -> Option<ClaimFormat> {
    match fact_key {
        "maintenance_sentiment" | "maintenance_quality" => {
            Some(ClaimFormat::Text("Maintenance is"))
        }
        "reddit_thread_count" => Some(ClaimFormat::NumericInt("{} Reddit discussions found")),
        "livability_sentiment" | "family_suitability" => {
            Some(ClaimFormat::Text("Family suitability:"))
        }
        "resident_sentiment" => Some(ClaimFormat::Text("Resident sentiment:")),
        _ => None,
    }
}

enum ClaimFormat {
    Text(&'static str),
    NumericInt(&'static str),
}

/// Legacy: maps a preference to a fact key (for nodes without answers_preferences).
fn legacy_preference_to_fact_key(preference: &str) -> Option<&'static str> {
    match preference {
        "metro access" | "metro" | "near metro" => Some("metro_distance"),
        "quiet neighborhood" | "quiet" | "peaceful" => Some("noise_level"),
        "greenery" | "green" | "parks" => Some("greenery_score"),
        "good society" | "well maintained" | "maintenance" => Some("maintenance_quality"),
        "safe" | "safety" | "secure" => Some("safety_rating"),
        "water supply" | "water" => Some("water_supply"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Knowledge context builder — graph-first, legacy fallback
// ---------------------------------------------------------------------------

/// Build knowledge context from the graph for matched results.
/// Returns (KnowledgeContext, graph_nodes_hit, enrichment_gaps).
fn build_knowledge_context(
    graph: &KnowledgeGraph,
    society_ids: &[String],
    intent: &intent::SearchIntent,
) -> (KnowledgeContext, Vec<String>, Vec<EnrichmentGap>) {
    let mut claims = Vec::new();
    let mut nodes_consulted = 0;
    let mut learning_gaps = Vec::new();
    let mut graph_nodes_hit = Vec::new();
    let mut enrichment_gaps = Vec::new();

    for society_id in society_ids {
        let node_id = society_node_id(society_id);

        if let Some(node) = graph.get_node(&node_id) {
            nodes_consulted += 1;
            graph_nodes_hit.push(node_id.clone());

            // --- Extract claims ---
            // Priority: fact's own display_template > legacy fallback map.
            for fact in &node.facts {
                let claim_text = if let Some(ref template) = fact.display_template {
                    let rendered = render_template(template, &fact.value);
                    if rendered.is_empty() { None } else { Some(rendered) }
                } else {
                    match claim_format(&fact.key) {
                        Some(ClaimFormat::Text(prefix)) => {
                            extract_text_value(&fact.value)
                                .map(|val| format!("{} {}", prefix, val))
                        }
                        Some(ClaimFormat::NumericInt(template)) => {
                            extract_numeric_value(&fact.value)
                                .map(|val| template.replace("{}", &(val as u32).to_string()))
                        }
                        None => None,
                    }
                };

                if let Some(claim) = claim_text {
                    claims.push(SourcedClaim {
                        entity_name: node.name.clone(),
                        claim,
                        confidence: fact.confidence,
                        source_type: format!("{:?}", fact.source.source_type),
                    });
                }
            }

            // --- Detect learning gaps ---
            // For each user preference, check if ANY fact on this node answers it.
            // Graph-first: scan facts' answers_preferences fields.
            // Legacy fallback: hardcoded preference→fact_key map.
            for pref in &intent.preferences {
                let pref_lower = pref.to_lowercase();

                // Graph-first: does any fact on this node declare it answers this preference?
                let answered_by_graph = node.facts.iter().any(|f| {
                    f.answers_preferences
                        .iter()
                        .any(|ap| ap.to_lowercase() == pref_lower)
                });

                if answered_by_graph {
                    continue; // Graph knows this — no gap
                }

                // Legacy fallback: check hardcoded map
                if let Some(needed_fact) = legacy_preference_to_fact_key(pref) {
                    if node.facts.iter().any(|f| f.key == needed_fact) {
                        continue; // Old-style fact exists — no gap
                    }
                    // Gap: neither graph-declared nor legacy fact found
                    learning_gaps.push(format!(
                        "{}: missing {} data",
                        node.name, needed_fact
                    ));
                    enrichment_gaps.push(EnrichmentGap {
                        entity_id: node_id.clone(),
                        missing_fact: needed_fact.to_string(),
                        reason: format!("User preference: {}", pref),
                    });
                } else {
                    // Completely unknown preference — still a gap, but we don't know
                    // which fact key to ask for. Log it as a generic gap.
                    learning_gaps.push(format!(
                        "{}: no knowledge about '{}'",
                        node.name, pref
                    ));
                    enrichment_gaps.push(EnrichmentGap {
                        entity_id: node_id.clone(),
                        missing_fact: format!("unknown:{}", pref_lower.replace(' ', "_")),
                        reason: format!("Unknown preference: {}", pref),
                    });
                }
            }
        }
    }

    claims.dedup_by(|a, b| a.entity_name == b.entity_name && a.claim == b.claim);

    let context = KnowledgeContext {
        claims,
        nodes_consulted,
        learning_gaps,
    };

    (context, graph_nodes_hit, enrichment_gaps)
}

// ---------------------------------------------------------------------------
// Graph-driven preference scoring (used by text.rs)
// ---------------------------------------------------------------------------

/// Score how well a node's facts match a user preference, using the graph's own
/// scoring_hint metadata. Returns a score 0.0-2.0 (same scale as the old hardcoded fn).
///
/// Called from text.rs as a graph-aware alternative to the hardcoded preference_score.
pub fn graph_preference_score(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
) -> Option<f64> {
    graph_preference_score_detailed(graph, society_id, preference).map(|(score, _)| score)
}

/// Like `graph_preference_score`, but also returns the matching fact's metadata
/// so the caller can build a structured MatchReason.
pub fn graph_preference_score_detailed(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
) -> Option<(f64, GraphFactDetail)> {
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;
    let pref_lower = preference.to_lowercase();

    // Find any fact that declares it answers this preference AND has a scoring_hint
    for fact in &node.facts {
        let answers = fact
            .answers_preferences
            .iter()
            .any(|ap| ap.to_lowercase() == pref_lower);

        if !answers {
            continue;
        }

        let score = if let Some(ref hint) = fact.scoring_hint {
            score_fact_with_hint(&fact.value, hint)
        } else {
            1.0
        };

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

    None // No fact answers this preference
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
    hint: &crate::knowledge::ScoringHint,
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
            // For text facts: positive values score full, negative score zero
            let text = fact_to_text(value).unwrap_or_default().to_lowercase();
            let positive = ["good", "high", "positive", "quiet", "safe", "yes", "excellent"];
            let partial = ["average", "moderate", "mixed", "ok"];
            if positive.iter().any(|p| text.contains(p)) {
                weight
            } else if partial.iter().any(|p| text.contains(p)) {
                weight * 0.5
            } else {
                0.0
            }
        }
    }
}

fn fact_to_numeric(value: &crate::knowledge::FactValue) -> Option<f64> {
    match value {
        crate::knowledge::FactValue::Numeric(n) => Some(*n),
        crate::knowledge::FactValue::Score { value: v, .. } => Some(*v),
        _ => None,
    }
}

fn fact_to_text(value: &crate::knowledge::FactValue) -> Option<&str> {
    match value {
        crate::knowledge::FactValue::Text(s) => Some(s.as_str()),
        _ => None,
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
            if *b { "yes".to_string() } else { "no".to_string() }
        }
        crate::knowledge::FactValue::Tags(tags) => tags.join(", "),
        crate::knowledge::FactValue::Score { value: v, explanation: _ } => format!("{:.1}", v),
    };
    template.replace("{value}", &value_str)
}

fn extract_text_value(value: &crate::knowledge::FactValue) -> Option<&str> {
    match value {
        crate::knowledge::FactValue::Text(s) => Some(s.as_str()),
        _ => None,
    }
}

fn extract_numeric_value(value: &crate::knowledge::FactValue) -> Option<f64> {
    match value {
        crate::knowledge::FactValue::Numeric(n) => Some(*n),
        _ => None,
    }
}

fn match_label_from_score(score: f64) -> String {
    if score >= 0.75 { "Strong match".to_string() }
    else if score >= 0.5 { "Good match".to_string() }
    else if score >= 0.25 { "Partial match".to_string() }
    else { "Weak match".to_string() }
}

fn confidence_label(confidence: f32) -> String {
    if confidence >= 0.7 { "high".to_string() }
    else if confidence >= 0.4 { "medium".to_string() }
    else { "low".to_string() }
}
