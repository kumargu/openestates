//! Semantic recall: finds societies with high embedding similarity to the query.
//!
//! Embeddings serve recall (finding candidates), NOT ranking. Graph-fact scoring ranks.
//! The semantic recall populates the "also_consider" section — societies the user may
//! not have explicitly asked for but that match their underlying preferences.

use std::collections::HashMap;

use crate::knowledge::KnowledgeGraph;
use crate::knowledge::embed_client::EmbedClient;
use crate::knowledge::node::NodeType;

/// Find societies with high embedding similarity to the query.
/// Returns a map of society_node_id → similarity_score.
/// Used for "also_consider" recall — NOT for boosting primary scores.
///
/// Returns an empty map if embedding fails (graceful degradation).
pub async fn semantic_society_scores(
    embed_client: &EmbedClient,
    graph: &KnowledgeGraph,
    query: &str,
    top_n: usize,
) -> HashMap<String, f64> {
    let query_emb = match embed_client.embed(query).await {
        Some(emb) => emb,
        None => return HashMap::new(), // Graceful degradation
    };

    let similar = graph.similar_to_vector(&query_emb, top_n, Some(NodeType::Society));

    // Only include scores above noise threshold (0.4 for recall, stricter than before)
    similar
        .into_iter()
        .filter(|s| s.similarity > 0.4)
        .map(|s| (s.node_id, s.similarity))
        .collect()
}
