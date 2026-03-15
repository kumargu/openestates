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

/// Where a node's data originally came from — determines trust floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootSource {
    /// Government-verified (RERA), confidence floor = 1.0
    Rera,
    /// Self-reported by seller, enrichable but no legal proof
    Seller,
    /// Live discovery (Gemini), verification pending
    Discovered,
    /// Pre-Sprint 3 seed data, unclassified
    Legacy,
}

impl RootSource {
    /// Canonical string representation — single source of truth for serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rera => "rera",
            Self::Seller => "seller",
            Self::Discovered => "discovered",
            Self::Legacy => "legacy",
        }
    }
}

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

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub name: String,
    /// Structured facts with provenance
    pub facts: Vec<SourcedFact>,
    /// Where this node's data originally came from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_source: Option<RootSource>,
    /// Optional summary embedding for semantic search (768-dim)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_embedding: Option<Vec<f32>>,
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
            root_source: None,
            summary_embedding: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a fact, updating `updated_at` timestamp.
    pub fn add_fact(&mut self, fact: SourcedFact) {
        self.updated_at = Utc::now();
        self.facts.push(fact);
    }

    /// Add multiple facts at once.
    pub fn add_facts(&mut self, facts: Vec<SourcedFact>) {
        if !facts.is_empty() {
            self.updated_at = Utc::now();
            self.facts.extend(facts);
        }
    }

    /// Get the latest fact for a given key (highest version).
    pub fn get_fact(&self, key: &str) -> Option<&SourcedFact> {
        self.facts
            .iter()
            .filter(|f| f.key == key)
            .max_by_key(|f| f.version)
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

