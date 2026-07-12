//! KnowledgeGraph — the in-memory graph with indexes for fast lookup.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::edge::{Edge, Relation};
use super::fact::SourcedFact;
use super::node::{Node, NodeId, NodeType};
use super::search_event::SearchEvent;

/// The main knowledge graph. Holds all nodes, edges, and indexes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<Edge>,

    // --- Indexes (rebuilt on load, not persisted separately) ---
    /// Edges originating from a node → vec of edge indexes
    #[serde(skip)]
    edges_by_from: HashMap<NodeId, Vec<usize>>,
    /// Edges pointing to a node → vec of edge indexes
    #[serde(skip)]
    edges_by_to: HashMap<NodeId, Vec<usize>>,
    /// Nodes grouped by type
    #[serde(skip)]
    nodes_by_type: HashMap<NodeType, Vec<NodeId>>,

    /// Search event log (append-only)
    pub search_log: Vec<SearchEvent>,

    /// Enrichment queue
    pub enrichment_queue: Vec<EnrichmentTask>,
}

/// A task queued for background enrichment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentTask {
    pub entity_id: NodeId,
    pub skill_needed: String,
    /// Higher = more queries asked about this gap
    pub priority: f32,
    /// Which search queries triggered this task
    pub triggered_by: Vec<String>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            edges_by_from: HashMap::new(),
            edges_by_to: HashMap::new(),
            nodes_by_type: HashMap::new(),
            search_log: Vec::new(),
            enrichment_queue: Vec::new(),
        }
    }

    /// Rebuild all indexes from the current nodes and edges.
    /// Call after loading from disk.
    pub fn rebuild_indexes(&mut self) {
        self.edges_by_from.clear();
        self.edges_by_to.clear();
        self.nodes_by_type.clear();

        for (id, node) in &self.nodes {
            self.nodes_by_type
                .entry(node.node_type)
                .or_default()
                .push(id.clone());
        }

        for (idx, edge) in self.edges.iter().enumerate() {
            self.edges_by_from
                .entry(edge.from.clone())
                .or_default()
                .push(idx);
            self.edges_by_to
                .entry(edge.to.clone())
                .or_default()
                .push(idx);
        }
    }

    // --- Node operations ---

    pub fn add_node(&mut self, node: Node) {
        let id = node.id.clone();
        let node_type = node.node_type;
        self.nodes.insert(id.clone(), node);
        self.nodes_by_type.entry(node_type).or_default().push(id);
    }

    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Get all nodes of a given type.
    pub fn nodes_of_type(&self, node_type: NodeType) -> Vec<&Node> {
        self.nodes_by_type
            .get(&node_type)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    /// Add a fact to an existing node.
    pub fn add_fact_to_node(&mut self, node_id: &str, fact: SourcedFact) -> bool {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.add_fact(fact);
            true
        } else {
            false
        }
    }

    // --- Edge operations ---

    pub fn add_edge(&mut self, edge: Edge) {
        let idx = self.edges.len();
        self.edges_by_from
            .entry(edge.from.clone())
            .or_default()
            .push(idx);
        self.edges_by_to
            .entry(edge.to.clone())
            .or_default()
            .push(idx);
        self.edges.push(edge);
    }

    /// Get all edges originating from a node.
    pub fn edges_from(&self, node_id: &str) -> Vec<&Edge> {
        self.edges_by_from
            .get(node_id)
            .map(|idxs| idxs.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    /// Get all edges pointing to a node.
    pub fn edges_to(&self, node_id: &str) -> Vec<&Edge> {
        self.edges_by_to
            .get(node_id)
            .map(|idxs| idxs.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    /// Get all neighbors of a node (both directions), optionally filtered by relation.
    pub fn neighbors(&self, node_id: &str, relation: Option<Relation>) -> Vec<&Node> {
        let mut neighbor_ids: Vec<&NodeId> = Vec::new();

        if let Some(idxs) = self.edges_by_from.get(node_id) {
            for &idx in idxs {
                if let Some(edge) = self.edges.get(idx) {
                    if relation.is_none() || relation == Some(edge.relation) {
                        neighbor_ids.push(&edge.to);
                    }
                }
            }
        }

        if let Some(idxs) = self.edges_by_to.get(node_id) {
            for &idx in idxs {
                if let Some(edge) = self.edges.get(idx) {
                    if relation.is_none() || relation == Some(edge.relation) {
                        neighbor_ids.push(&edge.from);
                    }
                }
            }
        }

        neighbor_ids
            .into_iter()
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    // --- Search event logging ---

    /// Maximum number of search events to keep in memory.
    /// Older events are still persisted to JSONL on disk.
    const MAX_SEARCH_LOG: usize = 500;

    pub fn log_search(&mut self, event: SearchEvent) {
        self.search_log.push(event);
        // Trim oldest entries to prevent unbounded memory growth.
        // The full history lives in the JSONL files on disk.
        if self.search_log.len() > Self::MAX_SEARCH_LOG {
            let drain_count = self.search_log.len() - Self::MAX_SEARCH_LOG;
            self.search_log.drain(..drain_count);
        }
    }

    // --- Enrichment queue ---

    /// Get pending enrichment tasks sorted by priority (highest first).
    pub fn pending_enrichments(&self) -> Vec<&EnrichmentTask> {
        let mut tasks: Vec<_> = self
            .enrichment_queue
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect();
        tasks.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        tasks
    }

    // --- Stats ---

    pub fn stats(&self) -> GraphStats {
        let mut facts_count = 0;
        for node in self.nodes.values() {
            facts_count += node.facts.len();
        }

        let mut nodes_by_type = HashMap::new();
        for node in self.nodes.values() {
            *nodes_by_type
                .entry(format!("{}", node.node_type))
                .or_insert(0usize) += 1;
        }

        GraphStats {
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            total_facts: facts_count,
            nodes_by_type,
            search_events: self.search_log.len(),
            pending_enrichments: self
                .enrichment_queue
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
        }
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_facts: usize,
    pub nodes_by_type: HashMap<String, usize>,
    pub search_events: usize,
    pub pending_enrichments: usize,
}
