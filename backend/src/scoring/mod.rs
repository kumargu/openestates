//! Scoring module.
//!
//! Buyer-facing quality signals now come from DAG-backed evidence folds and the
//! livability brief (see `crate::livability_brief`). This module keeps:
//! - `transparency`: an internal composite trust score used for ranking/decisions,
//!   not rendered as a raw number to buyers.
//!
//! The old hand-written `CompareThemes`/`compute_tradeoffs` heuristics over seed
//! Property fields were removed in favor of source-backed facts.

mod policy;
mod transparency;

pub use policy::{
    area_tracker_policy, score_property_for_surface, scoring_policy, search_ranking_policy,
    signal_score, AreaTrackerPolicy, BestEffortRankingTier, CandidateScore, FactAvailability,
    RecommendationBranchPolicy, RecommendationEligibilityPolicy,
    RecommendationFallbackBranchPolicy, RecommendationRecallChannelPolicy,
    RecommendationRecallOperator, RecommendationRecallPolicy, ScoredSignal, ScoringPolicyFile,
    SearchRankingPolicy,
};
pub use transparency::{compute_transparency_score, TransparencyScore};
