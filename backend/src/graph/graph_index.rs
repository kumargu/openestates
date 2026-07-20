use std::collections::{HashMap, HashSet, VecDeque};

use crate::serving::ServingEdgeRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkStep {
    pub from_entity_id: String,
    pub edge_type: String,
    pub to_entity_id: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphIndex {
    edges_from: HashMap<(String, String), Vec<String>>,
    edges_to: HashMap<(String, String), Vec<String>>,
}

impl GraphIndex {
    pub fn from_serving_edges(edges: &[ServingEdgeRecord]) -> Self {
        let mut index = Self::default();
        for edge in edges {
            index
                .edges_from
                .entry((edge.from_entity_id.clone(), edge.edge_type.clone()))
                .or_default()
                .push(edge.to_entity_id.clone());
            index
                .edges_to
                .entry((edge.to_entity_id.clone(), edge.edge_type.clone()))
                .or_default()
                .push(edge.from_entity_id.clone());
        }
        index
    }

    pub fn walk_out(&self, anchor: &str, hops: &[&str], max_depth: usize) -> Vec<WalkStep> {
        if hops.is_empty() || max_depth == 0 {
            return Vec::new();
        }

        let mut steps = Vec::new();
        let mut frontier = vec![anchor.to_string()];

        for edge_type in hops.iter().take(max_depth) {
            let mut next = Vec::new();
            for from_id in &frontier {
                let key = (from_id.clone(), (*edge_type).to_string());
                if let Some(targets) = self.edges_from.get(&key) {
                    for to_id in targets {
                        steps.push(WalkStep {
                            from_entity_id: from_id.clone(),
                            edge_type: (*edge_type).to_string(),
                            to_entity_id: to_id.clone(),
                        });
                        next.push(to_id.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }

        steps
    }

    pub fn walk_bfs(
        &self,
        anchor: &str,
        allowed_edges: &[&str],
        max_depth: usize,
    ) -> Vec<WalkStep> {
        if allowed_edges.is_empty() || max_depth == 0 {
            return Vec::new();
        }

        let mut steps = Vec::new();
        let mut frontier = VecDeque::from([(anchor.to_string(), 0usize)]);
        let mut visited_nodes = HashSet::from([anchor.to_string()]);
        let mut visited_edges = HashSet::<(String, String, String)>::new();

        while let Some((from_id, depth)) = frontier.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for edge_type in allowed_edges {
                let key = (from_id.clone(), (*edge_type).to_string());
                let Some(targets) = self.edges_from.get(&key) else {
                    continue;
                };
                for to_id in targets {
                    let edge_key = (from_id.clone(), (*edge_type).to_string(), to_id.clone());
                    if !visited_edges.insert(edge_key) {
                        continue;
                    }
                    steps.push(WalkStep {
                        from_entity_id: from_id.clone(),
                        edge_type: (*edge_type).to_string(),
                        to_entity_id: to_id.clone(),
                    });
                    if visited_nodes.insert(to_id.clone()) {
                        frontier.push_back((to_id.clone(), depth + 1));
                    }
                }
            }
        }

        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_out_follows_served_by_road_chain() {
        let edges = vec![
            ServingEdgeRecord {
                from_entity_id: "society:prestige-waterford".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road:ecc-road".to_string(),
                confidence: 0.9,
                source_type: "LegacySeed".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:prestige-waterford".to_string(),
                edge_type: "in_area".to_string(),
                to_entity_id: "area:whitefield".to_string(),
                confidence: 0.8,
                source_type: "LegacySeed".to_string(),
            },
        ];
        let index = GraphIndex::from_serving_edges(&edges);
        let steps = index.walk_out("society:prestige-waterford", &["served_by_road"], 2);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].to_entity_id, "road:ecc-road");
    }

    #[test]
    fn walk_bfs_respects_allowed_edges_and_depth() {
        let edges = vec![
            ServingEdgeRecord {
                from_entity_id: "society:one".to_string(),
                edge_type: "in_area".to_string(),
                to_entity_id: "area:whitefield".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "area:whitefield".to_string(),
                edge_type: "near_place".to_string(),
                to_entity_id: "place:metro".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:one".to_string(),
                edge_type: "built_by".to_string(),
                to_entity_id: "builder:x".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
        ];
        let index = GraphIndex::from_serving_edges(&edges);
        let steps = index.walk_bfs("society:one", &["in_area", "near_place"], 2);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].to_entity_id, "area:whitefield");
        assert_eq!(steps[1].to_entity_id, "place:metro");
        assert!(steps.iter().all(|step| step.edge_type != "built_by"));
    }
}
