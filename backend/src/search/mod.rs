pub mod analyzer;
pub mod ast;
pub mod capabilities;
pub mod compiled_plan;
pub mod engine;
pub mod evaluation;
pub mod geo;
pub mod guard;
pub mod index;
pub mod intent;
pub mod journey;
pub(crate) mod parser;
pub mod proof;
pub(crate) mod query_plan;
pub mod resolver;
pub mod revision;
pub mod schema;
pub mod text;
mod tokens;

pub use ast::{ConstraintExpr, ConstraintTerm, IntentAst, PredicateFamily, PredicatePolarity};
pub use capabilities::SearchCapabilityIndex;
pub use compiled_plan::{
    BoolExpr, BranchId, CompiledSearchPlan, GeoAnchor, GeoBranch, GeoCellPath, GeoCellSearchPolicy,
    GeoCellSeed, GeoScope, GeoScopeResolution, ResolvedEntityHandle,
};
pub use engine::{
    CandidateScore, SearchDiagnostics, SearchEngine, SearchEvidenceGap, SearchLayerTiming,
    SearchRecallDiagnostics,
};
pub use evaluation::{
    BooleanEvaluation, EvaluationEvidence, EvaluationState, EvidenceGap, InventoryOption,
    PredicateEvaluation, VerifiedMatch,
};
pub use guard::{
    guard_search_query, named_society_alternatives_guidance, no_results_guidance, SearchGuidance,
};
pub use index::SearchIndex;
pub use intent::{SearchIntent, SourceSpan};
pub use revision::{
    apply_typed_revision, compile_typed_revision, decode_signed_search_context,
    issue_signed_search_context, reissue_signed_search_context, result_membership_fingerprint,
    IssuedSearchContext, SearchRevisionLimits, SearchRevisionOperation, SearchRevisionOutcome,
    SignedSearchContext, TypedIntentAst, TypedIntentAstBranch, TypedSearchRevision,
    TypedSearchRevisionPatch,
};
pub use text::{CandidateEvaluationRequest, CandidateEvaluator, SearchEvaluationContext};

use serde::{Deserialize, Serialize};

use crate::models::{AreaProfile, PropertyCard};
use crate::serving::EvidenceId;

/// Exact serving identity selected by ranking before a snapshot-qualified
/// proof reference is projected. This stays internal to search execution.
#[derive(Debug, Clone)]
pub struct MatchEvidenceIdentity {
    pub subject_entity_id: String,
    pub evidence_id: EvidenceId,
}

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
    /// Exact observation chosen by evaluation. Public journey reasons replace
    /// this with a signed, snapshot-qualified proof token.
    #[serde(skip)]
    pub evidence_identity: Option<MatchEvidenceIdentity>,
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

/// How a branch's evidenced geography admitted and ordered a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyMatchKind {
    ExactSociety,
    SameMarketLocality,
    CellNearby,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeographyMatch {
    pub kind: GeographyMatchKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cell_path: Vec<String>,
    pub hops: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<crate::serving::EvidenceRef>,
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
    pub match_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeoff_label: Option<String>,
    #[serde(rename = "geographyMatch", skip_serializing_if = "Option::is_none")]
    pub geography_match: Option<GeographyMatch>,
    /// Structured match explanation — present when query has preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_explanation: Option<MatchExplanation>,
    /// Exact predicate observations used for hard eligibility. These are the
    /// machine-readable receipts behind ranking and proof projections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_matches: Vec<VerifiedMatch>,
    /// Data confidence score — how trustworthy is this result's data?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<ConfidenceScore>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultSet {
    pub branch_id: String,
    pub label: String,
    pub results: Vec<SearchResultCard>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRuntimeVersion {
    pub serving_bundle_version: String,
    pub scoring_policy_version: u32,
    pub search_engine_version: String,
    pub semantic_contract_digest: String,
}

/// Internal result of executing one snapshot-bound plan. Public search routes
/// project this through `SearchJourneyEnvelope`; this shape is never a wire
/// contract of its own.
#[derive(Debug, Clone, Serialize)]
pub struct SearchExecution {
    pub query: String,
    pub ast_fingerprint: String,
    pub result_sets: Vec<SearchResultSet>,
    pub ordered_result_ids: Vec<String>,
    pub total_matches: usize,
    pub area_context: Option<AreaProfile>,
    pub state: String,
    pub search_guidance: Option<SearchGuidance>,
}
