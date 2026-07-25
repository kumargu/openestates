mod branch;
mod builder;
mod snapshot;

pub use branch::{
    BranchLens, EvidenceDelta, RecallChannelHit, RecommendationBranch, RecommendationEnvelope,
    RecommendationResponse, RecommendationStatus, RECOMMENDATION_ENGINE_VERSION,
};
pub use builder::{build_recommendation_branches, RecommendationBranchInputs};
pub use snapshot::{summarize_evidence_sections, EvidenceSnapshot};
