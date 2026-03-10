//! Graph traversal queries — structured ways to explore the knowledge graph.
//!
//! These are the building blocks for API endpoints and internal graph operations:
//! - Neighbor traversal (1-hop, multi-hop)
//! - Path finding between nodes
//! - Subgraph extraction
//! - Fact aggregation across connected nodes

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use super::edge::Relation;
use super::graph::KnowledgeGraph;
use super::node::{Node, NodeId, NodeType};

// ---------------------------------------------------------------------------
// Query result types
// ---------------------------------------------------------------------------

/// A path between two nodes in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphPath {
    /// Ordered list of node IDs from source to target.
    pub nodes: Vec<NodeId>,
    /// Relations traversed at each hop.
    pub relations: Vec<Relation>,
    /// Total number of hops.
    pub hops: usize,
}

/// A subgraph extracted around a focal node.
#[derive(Debug, Clone, Serialize)]
pub struct Subgraph {
    pub focal_node: NodeId,
    pub nodes: Vec<SubgraphNode>,
    pub edges: Vec<SubgraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubgraphNode {
    pub id: NodeId,
    pub name: String,
    pub node_type: String,
    pub fact_count: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubgraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: String,
    pub weight: f32,
}

/// Aggregated facts across multiple nodes (e.g., all maintenance facts in an area).
#[derive(Debug, Clone, Serialize)]
pub struct FactAggregation {
    pub fact_key: String,
    pub entries: Vec<AggregatedFact>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregatedFact {
    pub node_id: NodeId,
    pub node_name: String,
    pub value: serde_json::Value,
    pub confidence: f32,
    pub source_type: String,
}

/// Comparison of two nodes on shared fact keys.
#[derive(Debug, Clone, Serialize)]
pub struct NodeComparison {
    pub node_a: NodeId,
    pub node_b: NodeId,
    pub shared_facts: Vec<ComparisonEntry>,
    pub unique_to_a: Vec<String>,
    pub unique_to_b: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparisonEntry {
    pub fact_key: String,
    pub value_a: serde_json::Value,
    pub confidence_a: f32,
    pub value_b: serde_json::Value,
    pub confidence_b: f32,
}

// ---------------------------------------------------------------------------
// Query implementations
// ---------------------------------------------------------------------------

impl KnowledgeGraph {
    /// Find the shortest path between two nodes using BFS.
    /// Returns None if no path exists.
    pub fn find_path(
        &self,
        from: &str,
        to: &str,
        max_hops: usize,
    ) -> Option<GraphPath> {
        if from == to {
            return Some(GraphPath {
                nodes: vec![from.to_string()],
                relations: Vec::new(),
                hops: 0,
            });
        }

        let mut visited: HashSet<&str> = HashSet::new();
        // (current_node, path_so_far, relations_so_far)
        let mut queue: VecDeque<(&str, Vec<NodeId>, Vec<Relation>)> = VecDeque::new();

        visited.insert(from);
        queue.push_back((from, vec![from.to_string()], Vec::new()));

        while let Some((current, path, rels)) = queue.pop_front() {
            if path.len() > max_hops + 1 {
                continue;
            }

            // Check outgoing edges
            for edge in self.edges_from(current) {
                let next = edge.to.as_str();
                if next == to {
                    let mut final_path = path.clone();
                    final_path.push(to.to_string());
                    let mut final_rels = rels.clone();
                    final_rels.push(edge.relation);
                    return Some(GraphPath {
                        hops: final_path.len() - 1,
                        nodes: final_path,
                        relations: final_rels,
                    });
                }
                if !visited.contains(next) {
                    visited.insert(next);
                    let mut new_path = path.clone();
                    new_path.push(next.to_string());
                    let mut new_rels = rels.clone();
                    new_rels.push(edge.relation);
                    queue.push_back((next, new_path, new_rels));
                }
            }

            // Check incoming edges (graph is directed but we traverse both ways)
            for edge in self.edges_to(current) {
                let next = edge.from.as_str();
                if next == to {
                    let mut final_path = path.clone();
                    final_path.push(to.to_string());
                    let mut final_rels = rels.clone();
                    final_rels.push(edge.relation);
                    return Some(GraphPath {
                        hops: final_path.len() - 1,
                        nodes: final_path,
                        relations: final_rels,
                    });
                }
                if !visited.contains(next) {
                    visited.insert(next);
                    let mut new_path = path.clone();
                    new_path.push(next.to_string());
                    let mut new_rels = rels.clone();
                    new_rels.push(edge.relation);
                    queue.push_back((next, new_path, new_rels));
                }
            }
        }

        None
    }

    /// Extract a subgraph around a focal node up to `depth` hops.
    pub fn subgraph(&self, focal_id: &str, depth: usize) -> Option<Subgraph> {
        self.get_node(focal_id)?;

        let mut visited: HashMap<&str, usize> = HashMap::new(); // node_id → depth
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
        let mut sub_edges: Vec<SubgraphEdge> = Vec::new();

        visited.insert(focal_id, 0);
        queue.push_back((focal_id, 0));

        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            for edge in self.edges_from(current) {
                let next = edge.to.as_str();
                sub_edges.push(SubgraphEdge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    relation: format!("{:?}", edge.relation),
                    weight: edge.weight,
                });
                if !visited.contains_key(next) {
                    visited.insert(next, current_depth + 1);
                    queue.push_back((next, current_depth + 1));
                }
            }

            for edge in self.edges_to(current) {
                let next = edge.from.as_str();
                sub_edges.push(SubgraphEdge {
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    relation: format!("{:?}", edge.relation),
                    weight: edge.weight,
                });
                if !visited.contains_key(next) {
                    visited.insert(next, current_depth + 1);
                    queue.push_back((next, current_depth + 1));
                }
            }
        }

        let mut sub_nodes: Vec<SubgraphNode> = visited
            .iter()
            .filter_map(|(&id, &d)| {
                self.get_node(id).map(|n| SubgraphNode {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    node_type: format!("{}", n.node_type),
                    fact_count: n.facts.len(),
                    depth: d,
                })
            })
            .collect();
        sub_nodes.sort_by_key(|n| (n.depth, n.id.clone()));

        // Deduplicate edges
        sub_edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.relation == b.relation);

        Some(Subgraph {
            focal_node: focal_id.to_string(),
            nodes: sub_nodes,
            edges: sub_edges,
        })
    }

    /// Aggregate a specific fact key across all neighbors of a node.
    /// Useful for: "What do all societies in Whitefield say about maintenance?"
    pub fn aggregate_fact(
        &self,
        node_id: &str,
        fact_key: &str,
        relation_filter: Option<Relation>,
    ) -> FactAggregation {
        let neighbors = self.neighbors(node_id, relation_filter);

        let entries: Vec<AggregatedFact> = neighbors
            .iter()
            .filter_map(|n| {
                n.get_fact(fact_key).map(|f| AggregatedFact {
                    node_id: n.id.clone(),
                    node_name: n.name.clone(),
                    value: serde_json::to_value(&f.value).unwrap_or_default(),
                    confidence: f.confidence,
                    source_type: format!("{:?}", f.source.source_type),
                })
            })
            .collect();

        let count = entries.len();
        FactAggregation {
            fact_key: fact_key.to_string(),
            entries,
            count,
        }
    }

    /// Compare two nodes side-by-side on their shared fact keys.
    pub fn compare_nodes(&self, id_a: &str, id_b: &str) -> Option<NodeComparison> {
        let node_a = self.get_node(id_a)?;
        let node_b = self.get_node(id_b)?;

        let keys_a: HashSet<&str> = node_a.facts.iter().map(|f| f.key.as_str()).collect();
        let keys_b: HashSet<&str> = node_b.facts.iter().map(|f| f.key.as_str()).collect();

        let shared: Vec<ComparisonEntry> = keys_a
            .intersection(&keys_b)
            .filter_map(|&key| {
                let fa = node_a.get_fact(key)?;
                let fb = node_b.get_fact(key)?;
                Some(ComparisonEntry {
                    fact_key: key.to_string(),
                    value_a: serde_json::to_value(&fa.value).unwrap_or_default(),
                    confidence_a: fa.confidence,
                    value_b: serde_json::to_value(&fb.value).unwrap_or_default(),
                    confidence_b: fb.confidence,
                })
            })
            .collect();

        let unique_to_a: Vec<String> = keys_a
            .difference(&keys_b)
            .map(|k| k.to_string())
            .collect();

        let unique_to_b: Vec<String> = keys_b
            .difference(&keys_a)
            .map(|k| k.to_string())
            .collect();

        Some(NodeComparison {
            node_a: id_a.to_string(),
            node_b: id_b.to_string(),
            shared_facts: shared,
            unique_to_a,
            unique_to_b,
        })
    }

    /// Find all nodes that have a specific fact key, optionally filtered by type.
    pub fn nodes_with_fact(
        &self,
        fact_key: &str,
        node_type_filter: Option<NodeType>,
    ) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| {
                if let Some(nt) = node_type_filter {
                    if n.node_type != nt {
                        return false;
                    }
                }
                n.has_fact(fact_key)
            })
            .collect()
    }

    /// Find nodes that are missing a specific fact (enrichment gap detection).
    pub fn nodes_missing_fact(
        &self,
        fact_key: &str,
        node_type_filter: Option<NodeType>,
    ) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| {
                if let Some(nt) = node_type_filter {
                    if n.node_type != nt {
                        return false;
                    }
                }
                !n.has_fact(fact_key)
            })
            .collect()
    }

    /// Get all societies in an area (convenience query).
    pub fn societies_in_area(&self, area_id: &str) -> Vec<&Node> {
        self.neighbors(area_id, Some(Relation::SocietyInArea))
            .into_iter()
            .filter(|n| n.node_type == NodeType::Society)
            .collect()
    }

    /// Get all properties in a society (convenience query).
    pub fn properties_in_society(&self, society_id: &str) -> Vec<&Node> {
        self.neighbors(society_id, Some(Relation::PropertyInSociety))
            .into_iter()
            .filter(|n| n.node_type == NodeType::Property)
            .collect()
    }

    /// Coverage report: for each fact key, how many nodes of a type have it?
    pub fn fact_coverage(&self, node_type: NodeType) -> HashMap<String, (usize, usize)> {
        let nodes = self.nodes_of_type(node_type);
        let total = nodes.len();

        let mut all_keys: HashSet<String> = HashSet::new();
        for node in &nodes {
            for fact in &node.facts {
                all_keys.insert(fact.key.clone());
            }
        }

        let mut coverage: HashMap<String, (usize, usize)> = HashMap::new();
        for key in all_keys {
            let has_it = nodes.iter().filter(|n| n.has_fact(&key)).count();
            coverage.insert(key, (has_it, total));
        }

        coverage
    }
}
