//! Debug API endpoints — score a specific society against a specific query.
//!
//! Gated by ENABLE_DEBUG_API=true env flag. Not exposed in production.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::search::{
    SocietyDebugScore, PreferenceDebugScore, intent as intent_mod, score_all_societies,
};
use crate::state::AppState;

use super::enrichment::society_node_id;

#[derive(Deserialize)]
pub struct ScoreDebugQuery {
    /// Natural-language search query
    pub q: String,
    /// Society node ID to score, e.g. "society:prestige-lakeside"
    pub society: String,
}

#[derive(Serialize)]
pub struct ScoreDebugResponse {
    pub query: String,
    pub society_id: String,
    pub intent_parsed: crate::search::SearchIntent,
    pub score_breakdown: Option<SocietyDebugScore>,
    pub error: Option<String>,
}

/// GET /api/debug/score?q=...&society=society:xyz
///
/// Scores a specific society against a specific query and returns the full debug breakdown.
/// Requires ENABLE_DEBUG_API=true environment variable.
pub async fn score_debug(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ScoreDebugQuery>,
) -> Result<Json<ScoreDebugResponse>, (StatusCode, &'static str)> {
    // Gate behind env flag
    if std::env::var("ENABLE_DEBUG_API").as_deref() != Ok("true") {
        return Err((StatusCode::FORBIDDEN, "Debug API not enabled (set ENABLE_DEBUG_API=true)"));
    }

    let query = params.q;
    let society_id = params.society;

    let parsed_intent = intent_mod::parse_intent(&query);

    let graph = state.knowledge.read().await;

    let node_id = if society_id.starts_with("society:") {
        society_id.clone()
    } else {
        society_node_id(&society_id)
    };

    // Check node exists
    let Some(node) = graph.get_node(&node_id) else {
        return Ok(Json(ScoreDebugResponse {
            query,
            society_id,
            intent_parsed: parsed_intent,
            score_breakdown: None,
            error: Some(format!("Node not found: {}", node_id)),
        }));
    };

    let _ = node; // suppress unused warning
    let ids = vec![society_id.trim_start_matches("society:").to_string()];
    let scores = score_all_societies(&graph, &ids, &parsed_intent);
    drop(graph);

    let debug_score = scores.get(&node_id).map(|ss| {
        let preference_scores: Vec<PreferenceDebugScore> = parsed_intent
            .positive_preferences.iter()
            .chain(parsed_intent.negative_preferences.iter())
            .map(|pref| {
                let matched = ss.matched_reasons.iter().find(|r| r.preference == pref.raw_text);
                PreferenceDebugScore {
                    preference: pref.raw_text.clone(),
                    polarity: format!("{:?}", pref.polarity),
                    matched_fact_key: matched.map(|r| r.fact_key.clone()),
                    matched_fact_value: matched.map(|r| r.display.clone()),
                    raw_score: matched.map(|r| r.score).unwrap_or(0.0),
                    weighted_score: matched.map(|r| r.score * pref.weight).unwrap_or(0.0),
                    source: matched.map(|r| r.source_level.clone()),
                    confidence: matched.map(|r| r.confidence).unwrap_or(0.0),
                }
            })
            .collect();

        SocietyDebugScore {
            society_id: ss.society_id.clone(),
            society_name: ss.society_id.clone(), // name not in SocietyScore, use id
            final_score: ss.score,
            confidence: ss.confidence,
            archetype_modifier: if parsed_intent.buyer_archetype.is_some() { 1.0 } else { 0.0 },
            area_signals_used: Vec::new(),
            facts_consulted: preference_scores.iter().filter(|p| p.matched_fact_key.is_some()).count(),
            preference_scores,
        }
    });

    Ok(Json(ScoreDebugResponse {
        query,
        society_id,
        intent_parsed: parsed_intent,
        score_breakdown: debug_score,
        error: None,
    }))
}
