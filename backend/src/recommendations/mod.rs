mod branch;
mod builder;
mod snapshot;

pub use branch::{BranchLens, EvidenceDelta, RecommendationBranch};
pub use builder::build_recommendation_branches;
pub use snapshot::{summarize_evidence_sections, EvidenceSnapshot};
