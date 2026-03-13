//! Semantic recall: finds societies with high embedding similarity to the query.
//!
//! Embeddings serve recall (finding candidates), NOT ranking. Graph-fact scoring ranks.
//! The semantic recall populates the "also_consider" section — societies the user may
//! not have explicitly asked for but that match their underlying preferences.

use std::collections::HashMap;

use crate::knowledge::KnowledgeGraph;
use crate::knowledge::embed_client::EmbedClient;
use crate::knowledge::node::NodeType;
use crate::search::intent::{SearchIntent, PreferenceSignal};

/// Build an enriched embed text from query + structured intent.
/// Gives the embedding model more signal than the raw query alone.
pub fn build_embed_text(query: &str, intent: &SearchIntent) -> String {
    let mut parts = vec![query.to_string()];
    if !intent.positive_preferences.is_empty() {
        let wants = intent.positive_preferences.iter()
            .map(|p| p.raw_text.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Looking for: {}", wants));
    }
    if !intent.negative_preferences.is_empty() {
        let avoids = intent.negative_preferences.iter()
            .map(|p| p.raw_text.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Avoiding: {}", avoids));
    }
    parts.join(". ")
}

/// Map intent preferences to aspect names for aspect-aware recall.
/// Returns the set of relevant aspects based on the user's expanded fact keys.
pub fn intent_to_aspects(intent: &SearchIntent) -> Vec<String> {
    let mut aspects: std::collections::HashSet<String> = Default::default();

    // Map expanded KG fact keys → aspects
    let key_to_aspect: &[(&str, &str)] = &[
        ("maintenance_quality", "livability"), ("community_vibe", "livability"),
        ("livability_score", "livability"), ("society_management", "livability"),
        ("family_friendly", "family"), ("child_safety", "family"),
        ("school_nearby", "family"), ("calm_environment", "family"), ("playground", "family"),
        ("water_supply", "risk"), ("waterlogging_risk", "risk"), ("litigation_risk", "risk"),
        ("builder_reputation", "risk"), ("rera_status", "risk"), ("delivery_track_record", "risk"),
        ("resale_strength", "investment"), ("market_activity", "investment"),
        ("rental_yield", "investment"), ("area_trend", "investment"),
        ("greenery_score", "environment"), ("noise_score", "environment"),
        ("open_space_score", "environment"), ("density", "environment"), ("air_quality", "environment"),
        ("metro_distance", "infrastructure"), ("traffic_score", "infrastructure"),
        ("road_quality", "infrastructure"), ("commute_score", "infrastructure"),
    ];

    let all_prefs: Vec<&PreferenceSignal> = intent.positive_preferences.iter()
        .chain(intent.negative_preferences.iter())
        .collect();

    for pref in all_prefs {
        for expanded_key in &pref.expanded_keys {
            for (fact_key, aspect) in key_to_aspect {
                if expanded_key == fact_key {
                    aspects.insert(aspect.to_string());
                }
            }
        }
    }

    aspects.into_iter().collect()
}

/// Find societies with high embedding similarity to the query.
/// Returns a map of society_node_id → similarity_score.
/// Used for "also_consider" recall — NOT for boosting primary scores.
///
/// Uses aspect-aware embeddings when available, falls back to summary embeddings.
/// Returns an empty map if embedding fails (graceful degradation).
pub async fn semantic_society_scores(
    embed_client: &EmbedClient,
    graph: &KnowledgeGraph,
    query: &str,
    intent: &SearchIntent,
    top_n: usize,
) -> HashMap<String, f64> {
    // Embed the enriched query text (query + expanded preferences)
    let embed_text = build_embed_text(query, intent);
    let query_emb = match embed_client.embed(&embed_text).await {
        Some(emb) => emb,
        None => return HashMap::new(), // Graceful degradation
    };

    let aspects = intent_to_aspects(intent);

    // Use aspect-aware search if aspects are detected, fall back to summary search
    let similar = if aspects.is_empty() {
        graph.similar_to_vector(&query_emb, top_n, Some(NodeType::Society))
    } else {
        graph.similar_to_vector_by_aspect(&query_emb, &aspects, top_n, Some(NodeType::Society))
    };

    // Only include scores above noise threshold (0.4 for recall, stricter than before)
    similar
        .into_iter()
        .filter(|s| s.similarity > 0.4)
        .map(|s| (s.node_id, s.similarity))
        .collect()
}
