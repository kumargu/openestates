//! Semantic search boost: finds societies with high embedding similarity to the query.
//!
//! Embeddings enhance explainability, they don't replace it. Every embedding-boosted
//! result gets a transparent reason like "semantically similar to your search".

use std::collections::HashMap;

use crate::knowledge::KnowledgeGraph;
use crate::knowledge::embed_client::EmbedClient;
use crate::knowledge::node::NodeType;

/// Find societies with high embedding similarity to the query string.
/// Returns a map of society_node_id → similarity_score for boosting text search results.
///
/// Returns an empty map if embedding fails (graceful degradation).
pub async fn semantic_society_scores(
    embed_client: &EmbedClient,
    graph: &KnowledgeGraph,
    query: &str,
    top_n: usize,
) -> HashMap<String, f64> {
    // 1. Embed the query
    let query_emb = match embed_client.embed(query).await {
        Some(emb) => emb,
        None => return HashMap::new(), // Graceful degradation
    };

    // 2. Find similar society nodes via brute-force cosine similarity
    let similar = graph.similar_to_vector(&query_emb, top_n, Some(NodeType::Society));

    // 3. Build score map (only include scores above noise threshold)
    similar
        .into_iter()
        .filter(|s| s.similarity > 0.3) // Below 0.3 cosine = noise
        .map(|s| (s.node_id, s.similarity))
        .collect()
}
