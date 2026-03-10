//! Search event logging — every search builds the graph's understanding.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::node::NodeId;
use crate::search::intent::SearchIntent;

/// A logged search event. Captures query, intent, results, and gaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEvent {
    pub query: String,
    pub intent: SearchIntent,
    pub results_returned: usize,
    /// Which graph nodes were relevant to this search
    pub graph_nodes_hit: Vec<NodeId>,
    /// What we didn't know — enrichment gaps discovered
    pub enrichment_gaps: Vec<EnrichmentGap>,
    pub timestamp: DateTime<Utc>,
}

/// A gap in knowledge discovered during a search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentGap {
    pub entity_id: NodeId,
    /// What fact key was needed but missing
    pub missing_fact: String,
    /// Why this was needed (e.g. "user asked for family-friendly")
    pub reason: String,
}

impl SearchEvent {
    pub fn new(query: String, intent: SearchIntent, results_returned: usize) -> Self {
        Self {
            query,
            intent,
            results_returned,
            graph_nodes_hit: Vec::new(),
            enrichment_gaps: Vec::new(),
            timestamp: Utc::now(),
        }
    }
}
