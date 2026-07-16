use serde::Serialize;

use crate::models::PropertyCard;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchLens {
    Proof,
    Value,
    Trust,
    Commute,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceDelta {
    pub fact_count: usize,
    pub gap_count: usize,
    pub confidence_pct: u8,
    pub fact_delta: i32,
    pub gap_delta: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecommendationBranch {
    pub lens: BranchLens,
    pub headline: String,
    pub property: PropertyCard,
    pub contrast: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradeoff: Option<String>,
    pub evidence_delta: EvidenceDelta,
}

impl BranchLens {
    pub fn headline(self) -> &'static str {
        match self {
            Self::Proof => "Better proof",
            Self::Value => "Better value",
            Self::Trust => "Lower risk",
            Self::Commute => "Easier commute",
        }
    }
}
