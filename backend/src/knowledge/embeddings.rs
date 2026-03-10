//! Vector index over entity summaries — enables semantic similarity search.
//!
//! Phase 1: brute-force cosine similarity over 768-dim embeddings.
//! Phase 2: FAISS IVF index when >5000 vectors.
//! Phase 3: Rust-native HNSW or pgvector.
//!
//! Embeddings are computed by the Python `embed_entity` skill and stored
//! on each Node's `summary_embedding` field. This module provides in-memory
//! search over those embeddings.

use serde::Serialize;

use super::graph::KnowledgeGraph;
use super::node::{Node, NodeType};

/// Result of a similarity search.
#[derive(Debug, Clone, Serialize)]
pub struct SimilarEntity {
    pub node_id: String,
    pub name: String,
    pub node_type: String,
    pub similarity: f64,
}

impl KnowledgeGraph {
    /// Find the top-N most similar nodes to a given node, based on embedding cosine similarity.
    /// Only considers nodes that have embeddings.
    pub fn similar_to(
        &self,
        node_id: &str,
        top_n: usize,
        node_type_filter: Option<NodeType>,
    ) -> Vec<SimilarEntity> {
        let target = match self.get_node(node_id) {
            Some(n) => n,
            None => return Vec::new(),
        };

        let target_emb = match &target.summary_embedding {
            Some(e) => e,
            None => return Vec::new(),
        };

        let mut scored: Vec<SimilarEntity> = self
            .nodes
            .values()
            .filter(|n| {
                n.id != node_id
                    && n.summary_embedding.is_some()
                    && node_type_filter.map_or(true, |nt| n.node_type == nt)
            })
            .filter_map(|n| {
                let emb = n.summary_embedding.as_ref()?;
                let sim = cosine_similarity(target_emb, emb);
                Some(SimilarEntity {
                    node_id: n.id.clone(),
                    name: n.name.clone(),
                    node_type: format!("{}", n.node_type),
                    similarity: sim,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_n);
        scored
    }

    /// Find the top-N most similar nodes to a raw embedding vector.
    /// Useful for query embedding → similar entity search.
    pub fn similar_to_vector(
        &self,
        query_embedding: &[f32],
        top_n: usize,
        node_type_filter: Option<NodeType>,
    ) -> Vec<SimilarEntity> {
        let mut scored: Vec<SimilarEntity> = self
            .nodes
            .values()
            .filter(|n| {
                n.summary_embedding.is_some()
                    && node_type_filter.map_or(true, |nt| n.node_type == nt)
            })
            .filter_map(|n| {
                let emb = n.summary_embedding.as_ref()?;
                let sim = cosine_similarity(query_embedding, emb);
                Some(SimilarEntity {
                    node_id: n.id.clone(),
                    name: n.name.clone(),
                    node_type: format!("{}", n.node_type),
                    similarity: sim,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_n);
        scored
    }

    /// Count how many nodes have embeddings, by type.
    pub fn embedding_stats(&self) -> Vec<(String, usize, usize)> {
        let mut stats: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();

        for node in self.nodes.values() {
            let key = format!("{}", node.node_type);
            let entry = stats.entry(key).or_insert((0, 0));
            entry.1 += 1; // total
            if node.summary_embedding.is_some() {
                entry.0 += 1; // with embedding
            }
        }

        stats
            .into_iter()
            .map(|(k, (has, total))| (k, has, total))
            .collect()
    }
}

/// Cosine similarity between two vectors. Returns value in [-1, 1].
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let v = vec![1.0f32, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }
}
