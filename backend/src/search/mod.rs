pub mod intent;
pub mod semantic;
mod text;

pub use intent::SearchIntent;
pub use text::TextSearch;

use serde::Serialize;

use crate::models::{AreaProfile, PropertyCard};

/// A search result that includes full PropertyCard data plus match info.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResultCard {
    // Flatten PropertyCard fields
    #[serde(flatten)]
    pub card: PropertyCard,
    pub match_score: f64,
    pub match_label: String,
    pub match_reason: String,
    /// Cosine similarity score if this result was semantically boosted, None otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f64>,
}

/// Sourced claim — a piece of knowledge with provenance, shown alongside results.
#[derive(Debug, Clone, Serialize)]
pub struct SourcedClaim {
    pub entity_name: String,
    pub claim: String,
    pub confidence: f32,
    pub source_type: String,
}

/// Knowledge context for a search — what the graph knows about the matched entities.
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeContext {
    /// Sourced claims relevant to the search results
    pub claims: Vec<SourcedClaim>,
    /// How many graph nodes were consulted
    pub nodes_consulted: usize,
    /// Facts the graph is still missing for this query
    pub learning_gaps: Vec<String>,
}

/// The full search response for the upgraded endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub intent: SearchIntent,
    pub results: Vec<SearchResultCard>,
    pub area_context: Option<AreaProfile>,
    pub total_results: usize,
    /// Knowledge graph provenance for the results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_context: Option<KnowledgeContext>,
    /// Whether live discovery was triggered: "discovered_new", "from_cache", "rate_limited", "discovery_failed", or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_status: Option<String>,
    /// How many properties were discovered (if discovery happened)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_count: Option<usize>,
}
