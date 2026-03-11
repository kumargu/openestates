//! Graph nodes — entities in the knowledge graph.
//!
//! Each node represents a real-world entity: a property, society, area, builder,
//! metro station, school, or reusable signal. Nodes carry typed facts with provenance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::fact::SourcedFact;

/// Unique identifier for a node. Format: "{type}:{slug}" e.g. "society:prestige-lakeside-habitat"
pub type NodeId = String;

/// What kind of entity this node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    Property,
    Society,
    Area,
    Builder,
    Metro,
    School,
    /// Reusable signal like "waterlogging_risk", "metro_access"
    Signal,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Property => write!(f, "property"),
            Self::Society => write!(f, "society"),
            Self::Area => write!(f, "area"),
            Self::Builder => write!(f, "builder"),
            Self::Metro => write!(f, "metro"),
            Self::School => write!(f, "school"),
            Self::Signal => write!(f, "signal"),
        }
    }
}

/// The 6 scoring dimensions used for aspect embeddings.
pub const ASPECT_NAMES: &[&str] = &[
    "livability", "family", "risk", "investment", "environment", "infrastructure",
];

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub name: String,
    /// Structured facts with provenance
    pub facts: Vec<SourcedFact>,
    /// Summary embedding for backward-compat semantic search (768-dim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_embedding: Option<Vec<f32>>,
    /// Per-aspect embeddings: aspect_name → 768-dim vector.
    /// When present, used instead of summary_embedding for aspect-aware recall.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_embeddings: Option<std::collections::HashMap<String, Vec<f32>>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Node {
    pub fn new(id: impl Into<String>, node_type: NodeType, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            node_type,
            name: name.into(),
            facts: Vec::new(),
            summary_embedding: None,
            aspect_embeddings: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a fact, replacing any existing fact with the same key and source skill.
    /// This prevents duplicate facts from accumulating on re-enrichment.
    pub fn add_fact(&mut self, fact: SourcedFact) {
        self.updated_at = Utc::now();
        let skill_id = fact.source.skill_id.as_deref().unwrap_or("");
        // Remove existing facts with same key from same skill
        if !skill_id.is_empty() {
            self.facts.retain(|f| {
                !(f.key == fact.key
                    && f.source.skill_id.as_deref().unwrap_or("") == skill_id)
            });
        }
        self.facts.push(fact);
    }

    /// Add multiple facts at once, deduplicating by key+skill.
    pub fn add_facts(&mut self, facts: Vec<SourcedFact>) {
        if !facts.is_empty() {
            self.updated_at = Utc::now();
            for fact in facts {
                let skill_id = fact.source.skill_id.as_deref().unwrap_or("");
                if !skill_id.is_empty() {
                    self.facts.retain(|f| {
                        !(f.key == fact.key
                            && f.source.skill_id.as_deref().unwrap_or("") == skill_id)
                    });
                }
                self.facts.push(fact);
            }
        }
    }

    /// Get the latest fact for a given key (highest version).
    pub fn get_fact(&self, key: &str) -> Option<&SourcedFact> {
        self.facts
            .iter()
            .filter(|f| f.key == key)
            .max_by_key(|f| f.version)
    }

    /// Get all facts for a given key (all versions).
    pub fn get_fact_history(&self, key: &str) -> Vec<&SourcedFact> {
        let mut facts: Vec<_> = self.facts.iter().filter(|f| f.key == key).collect();
        facts.sort_by_key(|f| f.version);
        facts
    }

    /// List all unique fact keys on this node.
    pub fn fact_keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.facts.iter().map(|f| f.key.as_str()).collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Check if this node has a fact for the given key.
    pub fn has_fact(&self, key: &str) -> bool {
        self.facts.iter().any(|f| f.key == key)
    }
}

/// Helper to build a NodeId from type and slug.
pub fn node_id(node_type: NodeType, slug: &str) -> NodeId {
    format!("{}:{}", node_type, slug)
}
