//! Graph edges — typed relationships between nodes.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::fact::FactSource;
use super::node::NodeId;

/// The type of relationship between two nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Relation {
    /// Property belongs to a society
    PropertyInSociety,
    /// Society is located in an area
    SocietyInArea,
    /// Society/property was built by a builder
    BuiltBy,
    /// Entity is near a metro station
    NearMetro,
    /// Entity is near a school
    NearSchool,
    /// Entity has a signal (waterlogging risk, metro access, etc.)
    HasSignal,
    /// Two societies are similar (computed from embeddings)
    SimilarTo,
    /// Two properties compete on price in the same area/BHK
    CompetesWithPrice,
    /// Fact is sourced from a URL/thread
    SourcedFrom,
}

/// A directed edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: Relation,
    /// Strength of relationship (0.0 - 1.0)
    pub weight: f32,
    /// Extra context (e.g. "distance_km": "2.5")
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
    pub source: FactSource,
}

impl Edge {
    /// Create a simple edge with manual source.
    #[cfg(test)]
    pub fn new(from: NodeId, to: NodeId, relation: Relation) -> Self {
        Self {
            from,
            to,
            relation,
            weight: 1.0,
            metadata: HashMap::new(),
            source: FactSource {
                source_type: super::fact::SourceType::Manual,
                url: None,
                model: None,
                skill_id: None,
                triggered_by: None,
            },
        }
    }
}
