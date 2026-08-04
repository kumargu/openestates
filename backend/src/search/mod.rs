pub mod analyzer;
pub mod engine;
pub mod focus;
pub mod geo;
pub mod guard;
pub mod index;
pub mod intent;
pub(crate) mod parser;
pub mod resolver;
pub mod schema;
pub mod text;

pub use engine::{
    CandidateScore, SearchDiagnostics, SearchEngine, SearchEvidenceGap, SearchLayerTiming,
    SearchRecallDiagnostics, SearchRelaxation,
};
pub use focus::{build_search_result_focus, FocusBuildInputs, SearchResultFocus};
pub use guard::{guard_search_query, no_results_guidance, SearchGuidance};
pub use index::SearchIndex;
pub use intent::SearchIntent;
pub use text::TextSearch;

use serde::Serialize;

use crate::models::{AreaProfile, PropertyCard};
use crate::proof_focus::ProofFocus;

/// One structured reason why a result matched a user preference.
#[derive(Debug, Clone, Serialize)]
pub struct MatchReason {
    /// The user preference this reason addresses, e.g. "quiet neighborhood"
    pub preference: String,
    /// The fact key that provided the answer, e.g. "noise_level"
    pub fact_key: String,
    /// Human-readable display from display_template, e.g. "Noise level is low"
    pub display: String,
    /// Score contribution (0.0-1.0 normalized)
    pub score: f64,
    /// Fact confidence (1.0 for RERA, 0.6 for Reddit, etc.)
    pub confidence: f32,
    /// Source type: "Reddit", "Rera", "Computed", "Manual", etc.
    pub source_type: String,
    /// "graph" or "local"
    pub scoring_method: String,
}

/// How a user preference was handled during scoring.
#[derive(Debug, Clone, Serialize)]
pub struct PreferenceCoverage {
    /// The user preference label from config or parsed intent.
    pub preference: String,
    /// "matched" (score > 0.5), "partial" (score > 0), "no_data"
    pub status: String,
    /// The fact key used, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
}

/// Full explanation of why a result was ranked where it is.
#[derive(Debug, Clone, Serialize)]
pub struct MatchExplanation {
    /// Per-fact reasons contributing to the score
    pub reasons: Vec<MatchReason>,
    /// Per-preference coverage status
    pub preference_coverage: Vec<PreferenceCoverage>,
    /// Percentage of score derived from graph facts vs local scoring (0-100)
    pub graph_driven_pct: f32,
    /// Total number of facts the scorer examined
    pub total_facts_consulted: usize,
}

/// One component of the confidence score, explaining a dimension.
#[derive(Debug, Clone, Serialize)]
pub struct ConfidenceComponent {
    /// Dimension name: "source_quality", "fact_coverage", "freshness", "match_quality"
    pub dimension: String,
    /// Score for this dimension (0.0 - 1.0)
    pub score: f64,
    /// Weight applied to this dimension
    pub weight: f64,
    /// Human-readable explanation
    pub explanation: String,
}

/// Overall confidence in a search result's data quality.
#[derive(Debug, Clone, Serialize)]
pub struct ConfidenceScore {
    /// Overall confidence (0.0 - 1.0)
    pub overall: f64,
    /// Human-readable label: "High", "Moderate", "Low"
    pub label: String,
    /// Per-dimension breakdown
    pub components: Vec<ConfidenceComponent>,
}

/// A search result that includes full PropertyCard data plus match info.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResultCard {
    // Flatten PropertyCard fields
    #[serde(flatten)]
    pub card: PropertyCard,
    pub match_score: f64,
    pub match_label: String,
    pub match_reason: String,
    /// Structured match explanation — present when query has preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_explanation: Option<MatchExplanation>,
    /// Generic detail-surface focus handles backed by the same proof reasons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_focuses: Vec<ProofFocus>,
    /// Data confidence score — how trustworthy is this result's data?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<ConfidenceScore>,
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
    /// Journey rails: named-society focus + siblings, or ranked matches + more homes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<SearchResultFocus>,
    /// Knowledge graph provenance for the results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_context: Option<KnowledgeContext>,
    /// Internal search diagnostics used by benchmarks and milestone validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_diagnostics: Option<SearchDiagnostics>,
    /// Deterministic relaxations applied after exact constraints produced no results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relaxations: Vec<SearchRelaxation>,
    /// Early guardrail guidance for vague, unsupported, or off-topic queries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_guidance: Option<SearchGuidance>,
}
