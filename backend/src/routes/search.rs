use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::discovery;
use crate::discovery::gemini::DiscoveryConstraints;
use crate::knowledge::{KnowledgeGraph, SearchEvent, store as kg_store};
use crate::knowledge::edge::Relation;
use crate::knowledge::fact::ScoringDirection;
use crate::knowledge::search_event::EnrichmentGap;
use crate::search::{KnowledgeContext, SearchResponse, SourcedClaim, TextSearch, intent};
use crate::state::AppState;

use super::enrichment::society_node_id;

/// Score threshold below which we trigger live discovery.
const DISCOVERY_THRESHOLD: f64 = 0.15;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

/// GET /api/search?q=... — intent-based search with live discovery fallback.
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
                bhk: None,
                budget_max: None,
                preferences: Vec::new(),
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

    // Run text search and query embedding in parallel for minimal latency.
    let (mut results, semantic_scores) = {
        let text_search_fut = async {
            let graph = state.knowledge.read().await;
            let properties = state.properties.read().await;
            let sellers = state.sellers.read().await;
            TextSearch::search_with_intent_and_sellers(
                &properties,
                &society_names,
                &state.societies,
                &query,
                &parsed_intent,
                Some(&graph),
                &sellers,
            )
        };

        let semantic_fut = async {
            if let Some(ref ec) = state.embed_client {
                let graph = state.knowledge.read().await;
                crate::search::semantic::semantic_society_scores(ec, &graph, &query, 20).await
            } else {
                HashMap::new()
            }
        };

        tokio::join!(text_search_fut, semantic_fut)
    };

    // --- Apply semantic boost to text search results ---
    if !semantic_scores.is_empty() {
        let properties = state.properties.read().await;

        // Boost existing results that have high semantic similarity
        for result in &mut results {
            if let Some(prop) = properties.iter().find(|p| p.id == result.card.id) {
                let node_id = society_node_id(&prop.society_id);
                if let Some(&sim) = semantic_scores.get(&node_id) {
                    // Boost: add up to 0.15 to normalized score (max 15% boost)
                    let boost = (sim - 0.3) * 0.2; // Maps 0.3-1.0 → 0.0-0.14
                    result.match_score = (result.match_score + boost).min(1.0);
                    result.match_reason = format!("{} + semantically relevant", result.match_reason);
                    result.semantic_score = Some((sim * 100.0).round() / 100.0);
                }
            }
        }

        // Inject semantic-only matches: high similarity but not in text results
        let existing_ids: Vec<String> = results.iter().map(|r| r.card.id.clone()).collect();
        for (node_id, sim) in &semantic_scores {
            if *sim <= 0.5 {
                continue; // Only inject high-confidence semantic matches
            }
            // Find properties in this society that aren't already in results
            for prop in properties.iter() {
                let prop_node_id = society_node_id(&prop.society_id);
                if prop_node_id != *node_id || existing_ids.contains(&prop.id) {
                    continue;
                }
                // Check hard constraints still apply
                if let Some(bhk) = parsed_intent.bhk {
                    if prop.bhk != bhk { continue; }
                }
                if let Some(budget) = parsed_intent.budget_max {
                    if prop.price > budget { continue; }
                }

                let graph = state.knowledge.read().await;
                let sellers = state.sellers.read().await;
                let card = crate::routes::enrichment::enrich_property_card_with_sellers(
                    prop, &state.societies, &graph, &sellers,
                );
                drop(sellers);
                drop(graph);

                results.push(crate::search::SearchResultCard {
                    card,
                    match_score: (*sim * 0.5).min(0.6), // Semantic-only gets moderate score
                    match_label: "Semantic match".to_string(),
                    match_reason: "Semantically similar to your search".to_string(),
                    match_explanation: None,
                    semantic_score: Some((*sim * 100.0).round() / 100.0),
                    confidence_score: None,
                });
                break; // One property per society for injection
            }
        }

        drop(properties);

        // Re-sort by updated scores
        results.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
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
                        let sellers = state.sellers.read().await;
                        results = TextSearch::search_with_intent_and_sellers(
                            &properties,
                            &society_names,
                            &state.societies,
                            &query,
                            &parsed_intent,
                            Some(&graph),
                            &sellers,
                        );
                        drop(sellers);
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

    Json(SearchResponse {
        query,
        intent: parsed_intent,
        results,
        area_context,
        total_results,
        knowledge_context: Some(knowledge_context),
        discovery_status,
        discovery_count,
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

            // --- Extract claims from builder nodes via BuiltBy edges ---
            for edge in graph.edges_from(&node_id) {
                if edge.relation != Relation::BuiltBy {
                    continue;
                }
                if let Some(builder_node) = graph.get_node(&edge.to) {
                    graph_nodes_hit.push(edge.to.clone());
                    for fact in &builder_node.facts {
                        let claim_text = if let Some(ref template) = fact.display_template {
                            let rendered = render_template(template, &fact.value);
                            if rendered.is_empty() { None } else { Some(rendered) }
                        } else {
                            None
                        };

                        if let Some(claim) = claim_text {
                            claims.push(SourcedClaim {
                                entity_name: builder_node.name.clone(),
                                claim,
                                confidence: fact.confidence,
                                source_type: format!("{:?}", fact.source.source_type),
                            });
                        }
                    }
                }
            }

            // --- Extract claims from area nodes via SocietyInArea edges ---
            for edge in graph.edges_from(&node_id) {
                if edge.relation != Relation::SocietyInArea {
                    continue;
                }
                if let Some(area_node) = graph.get_node(&edge.to) {
                    graph_nodes_hit.push(edge.to.clone());
                    for fact in &area_node.facts {
                        let claim_text = if let Some(ref template) = fact.display_template {
                            let rendered = render_template(template, &fact.value);
                            if rendered.is_empty() { None } else { Some(rendered) }
                        } else {
                            None
                        };

                        if let Some(claim) = claim_text {
                            claims.push(SourcedClaim {
                                entity_name: area_node.name.clone(),
                                claim,
                                confidence: fact.confidence,
                                source_type: format!("{:?}", fact.source.source_type),
                            });
                        }
                    }
                }
            }

            // --- Detect learning gaps ---
            // For each user preference, check if ANY fact on this node answers it.
            // Graph-first: scan facts' answers_preferences fields.
            // Legacy fallback: hardcoded preference→fact_key map.
            for pref in &intent.preferences {
                let pref_lower = pref.to_lowercase();

                // Graph-first: does any fact on this node declare it answers this preference?
                // Uses same fuzzy matching as graph_preference_score_detailed.
                let answered_by_graph = node.facts.iter().any(|f| {
                    f.answers_preferences.iter().any(|ap| {
                        let ap_lower = ap.to_lowercase();
                        ap_lower == pref_lower
                            || ap_lower.contains(&pref_lower)
                            || pref_lower.contains(&ap_lower)
                    })
                });

                // Also check builder node facts via BuiltBy edges
                let answered_by_builder = if !answered_by_graph {
                    graph.edges_from(&node_id).iter().any(|edge| {
                        edge.relation == Relation::BuiltBy
                            && graph.get_node(&edge.to).map_or(false, |builder| {
                                builder.facts.iter().any(|f| {
                                    f.answers_preferences.iter().any(|ap| {
                                        let ap_lower = ap.to_lowercase();
                                        ap_lower == pref_lower
                                            || ap_lower.contains(&pref_lower)
                                            || pref_lower.contains(&ap_lower)
                                    })
                                })
                            })
                    })
                } else {
                    false
                };

                if answered_by_graph || answered_by_builder {
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
/// scoring_hint metadata. Returns a score 0.0-2.0 and fact detail for MatchReason.
pub fn graph_preference_score_detailed(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
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

    // --- Cross-node scoring: traverse BuiltBy edge to check builder facts ---
    if let Some(result) = check_builder_facts(graph, &node_id, &pref_lower) {
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
            let text = fact_to_text(value).unwrap_or_default().to_lowercase();
            let negative = ["poor", "bad", "low", "terrible", "worst", "dangerous", "unsafe"];
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
        assert_eq!(score, 3.0, "ready_to_move should score at full weight (3.0), got {}", score);
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
        assert_eq!(score, 1.5, "partial value 'mixed' should score 0.5 * weight");
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
        assert_eq!(score, 2.0, "sentiment word 'good' should still score at full weight");
    }

    // --- Day 62: Graph scoring integration tests ---

    fn make_test_fact(
        status: &str,
        answers: Vec<&str>,
    ) -> crate::knowledge::fact::SourcedFact {
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

        let fact = make_test_fact("ready_to_move", vec![
            "ready to move", "ready possession", "completed project", "immediate possession",
        ]);
        make_test_node(&mut graph, "prestige-test", "Prestige Test", fact);

        // Test: "ready to move" preference should match and score 3.0
        let result = graph_preference_score_detailed(&graph, "prestige-test", "ready to move");
        assert!(result.is_some(), "Should find matching fact for 'ready to move'");
        let (score, detail) = result.unwrap();
        assert_eq!(score, 3.0, "ready_to_move fact should score 3.0, got {}", score);
        assert_eq!(detail.fact_key, "project_status");

        // Test: "under construction" should NOT match this society
        let result = graph_preference_score_detailed(&graph, "prestige-test", "under construction");
        assert!(result.is_none(), "Should NOT match 'under construction' for a ready_to_move society");
    }

    #[test]
    fn test_graph_fuzzy_matching_answers_preferences() {
        let mut graph = crate::knowledge::KnowledgeGraph::new();

        let fact = make_test_fact("under_construction", vec![
            "under construction", "ongoing project", "in progress",
        ]);
        make_test_node(&mut graph, "test-society", "Test Society", fact);

        // Exact match
        let result = graph_preference_score_detailed(&graph, "test-society", "under construction");
        assert!(result.is_some(), "Exact match 'under construction' should work");
        assert_eq!(result.unwrap().0, 3.0);

        // Fuzzy: preference contains answers_preference substring
        let result = graph_preference_score_detailed(&graph, "test-society", "ongoing");
        // "ongoing" is contained in "ongoing project" → should match
        assert!(result.is_some(), "Fuzzy match: 'ongoing' should match 'ongoing project'");
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
        assert!(result.is_some(), "Should match builder_zero_revocations for 'trusted builder'");
        let (score, _) = result.unwrap();
        assert_eq!(score, 2.0, "TextMatch 'true' should score at full weight 2.0");
    }

    #[test]
    fn test_legacy_fallback_for_nonexistent_society() {
        // When a society has no KG node, graph_preference_score_detailed returns None.
        // The text search layer then falls back to legacy_preference_score, which
        // handles standard preferences but returns 0.0 for builder-specific ones.
        // This test ensures no panic and correct None return for unknown societies.
        let graph = crate::knowledge::KnowledgeGraph::new();

        // Society doesn't exist in the graph at all
        let result = graph_preference_score_detailed(&graph, "nonexistent-society", "reliable builder");
        assert!(
            result.is_none(),
            "Should return None for non-existent society, allowing legacy fallback"
        );

        // Also verify standard preferences return None (triggering legacy path)
        let result = graph_preference_score_detailed(&graph, "nonexistent-society", "metro access");
        assert!(
            result.is_none(),
            "Should return None for non-existent society on standard preferences too"
        );
    }

    #[test]
    fn test_legacy_fallback_no_kg_node_scores_via_legacy() {
        // A property whose society has no KG node at all should still score
        // through legacy scoring (based on property fields like metro_distance,
        // noise_score, etc.) rather than panicking or returning zero results.
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
            society_quality_score: 0.8,
            builder_quality_score: 0.7,
            document_completeness_score: 0.9,
            litigation_risk: 0.1,
            noise_score: 0.2,
            sunlight_score: 0.7,
            airport_noise_score: 0.1,
            waterlogging_risk_score: 0.1,
            traffic_score: 0.3,
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
            bhk: Some(3),
            budget_max: None,
            preferences: vec!["metro access".into(), "quiet neighborhood".into()],
        };

        let results = TextSearch::search_with_intent(
            &properties,
            &society_names,
            &societies,
            "3bhk TestArea metro access quiet",
            &intent,
            Some(&graph),
        );

        assert_eq!(results.len(), 1, "Should return 1 result even without KG node");
        let result = &results[0];
        assert!(result.match_score > 0.0, "Should have positive score from legacy scoring");

        // Confidence should still be computed (low, since no KG node)
        assert!(result.confidence_score.is_some(), "Should have confidence score");
        let conf = result.confidence_score.as_ref().unwrap();
        // With no KG node, source_quality=0.3, coverage=0.0, freshness=0.3, match=0.0
        // Weighted: 0.3*0.4 + 0.0*0.2 + 0.3*0.2 + 0.0*0.2 = 0.18
        assert!(conf.overall < 0.4, "Confidence should be low without KG data, got {}", conf.overall);
        assert_eq!(conf.label, "Low");
    }
}
