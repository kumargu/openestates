use std::collections::HashMap;

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

    pub fn walk_out(
        &self,
        anchor: &str,
        hops: &[&str],
        max_depth: usize,
    ) -> Vec<WalkStep> {
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
}
