pub mod intent;
pub mod scoring;
pub mod semantic;
mod text;

pub use intent::{BuyerArchetype, Polarity, PreferenceSignal, SearchIntent};
pub use scoring::{
    Concern, ExplanationCard, ExplanationConcern, ExplanationReason, EvidenceSummary,
    MatchReason as SocietyMatchReason, SocietyScore, score_all_societies, synthesize_explanation,
};
pub use text::TextSearch;

use serde::Serialize;

use crate::models::{AreaProfile, PropertyCard};

/// One structured reason why a result matched a user preference.
#[derive(Debug, Clone, Serialize)]
pub struct MatchReason {
    pub preference: String,
    pub fact_key: String,
    pub display: String,
    pub score: f64,
    pub confidence: f32,
    pub source_type: String,
    /// "graph" or "legacy"
    pub scoring_method: String,
}

/// How a user preference was handled during scoring.
#[derive(Debug, Clone, Serialize)]
pub struct PreferenceCoverage {
    pub preference: String,
    /// "matched" (score > 0.5), "partial" (score > 0), "no_data"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
}

/// Full explanation of why a result was ranked where it is.
#[derive(Debug, Clone, Serialize)]
pub struct MatchExplanation {
    pub reasons: Vec<MatchReason>,
    pub preference_coverage: Vec<PreferenceCoverage>,
    pub graph_driven_pct: f32,
    pub total_facts_consulted: usize,
}

/// A search result that includes full PropertyCard data plus match info.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResultCard {
    #[serde(flatten)]
    pub card: PropertyCard,
    pub match_score: f64,
    pub match_label: String,
    pub match_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_explanation: Option<MatchExplanation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f64>,
    /// Society-level score from graph-fact scoring (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub society_score: Option<f32>,
    /// Evidence confidence: "high", "medium", or "low".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub society_confidence: Option<String>,
    /// Concerns raised for negative preferences.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<Concern>,
    /// Preferences for which no data was found.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unmatched_preferences: Vec<String>,
    /// Rich explanation card — why this matched, concerns, gaps, evidence.
    /// Present when query has structured preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation_card: Option<ExplanationCard>,
    /// Number of registered sellers actively listing this property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_seller_count: Option<u32>,
}

/// Sourced claim — a piece of knowledge with provenance, shown alongside results.
#[derive(Debug, Clone, Serialize)]
pub struct SourcedClaim {
    pub entity_name: String,
    pub claim: String,
    pub confidence: f32,
    pub source_type: String,
}

/// Knowledge context for a search.
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeContext {
    pub claims: Vec<SourcedClaim>,
    pub nodes_consulted: usize,
    pub learning_gaps: Vec<String>,
}

/// The full search response.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub intent: SearchIntent,
    pub results: Vec<SearchResultCard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_context: Option<AreaProfile>,
    pub total_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_context: Option<KnowledgeContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_count: Option<usize>,
    /// Results found through semantic recall (not in primary hard-constraint results).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_consider: Vec<SearchResultCard>,
    /// Debug trace — only present when ?debug=true is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<SearchDebugTrace>,
}

// ---------------------------------------------------------------------------
// Debug trace structs (Day 39)
// ---------------------------------------------------------------------------

/// Full debug trace for a search — only populated when ?debug=true.
/// Adds <2ms overhead when enabled; zero cost when disabled.
#[derive(Debug, Clone, Serialize)]
pub struct SearchDebugTrace {
    pub timestamp: String,
    pub query: String,
    pub candidate_count: usize,
    pub also_consider_triggered: bool,
    pub also_consider_reason: Option<String>,
    pub discovery_triggered: bool,
    /// Total end-to-end latency in ms
    pub total_latency_ms: u64,
    /// Intent parsing latency
    pub intent_parse_ms: u64,
    /// Society-first scoring latency
    pub scoring_ms: u64,
    /// Embedding similarity latency (0 if no embed client)
    pub embedding_ms: u64,
    /// Per-society score breakdown (top results only)
    pub societies_scored: Vec<SocietyDebugScore>,
}

/// Per-society score breakdown for debug mode.
#[derive(Debug, Clone, Serialize)]
pub struct SocietyDebugScore {
    pub society_id: String,
    pub society_name: String,
    pub final_score: f32,
    pub confidence: f32,
    pub archetype_modifier: f32,
    pub area_signals_used: Vec<String>,
    pub facts_consulted: usize,
    pub preference_scores: Vec<PreferenceDebugScore>,
}

/// Per-preference score breakdown for a society.
#[derive(Debug, Clone, Serialize)]
pub struct PreferenceDebugScore {
    pub preference: String,
    pub polarity: String,
    pub matched_fact_key: Option<String>,
    pub matched_fact_value: Option<String>,
    pub raw_score: f32,
    pub weighted_score: f32,
    pub source: Option<String>,
    pub confidence: f32,
}
