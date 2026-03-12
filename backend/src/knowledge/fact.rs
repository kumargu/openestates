//! SourcedFact — the atomic unit of knowledge.
//!
//! Every fact in the knowledge graph has provenance: who said it, how confident
//! we are, when we learned it, and what triggered learning it. This is the core
//! transparency primitive.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The atomic unit of knowledge. Every piece of information has provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcedFact {
    /// What this fact is about: "maintenance_quality", "family_friendly", "metro_distance"
    pub key: String,
    /// The actual value
    pub value: FactValue,
    /// How confident we are (0.0 - 1.0)
    pub confidence: f32,
    /// Where this fact came from
    pub source: FactSource,
    /// When the system learned this
    pub learned_at: DateTime<Utc>,
    /// Facts can be updated; newer version wins
    pub version: u32,
    /// How to display this fact to users, e.g. "Maintenance is {value}".
    /// The `{value}` placeholder is replaced with the fact's value at render time.
    /// Skills set this at enrichment time so Rust doesn't need to know every fact key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_template: Option<String>,

    /// Which user preference keywords this fact answers.
    /// e.g. maintenance_quality answers ["good society", "well maintained", "maintenance"].
    /// Skills declare this so the system learns new preference→fact mappings automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers_preferences: Vec<String>,

    /// How this fact should influence ranking when a matching preference is active.
    /// Skills declare scoring semantics so ranking adapts as new fact types appear.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoring_hint: Option<ScoringHint>,
}

/// How a fact should influence property ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringHint {
    /// How to interpret the value for ranking.
    pub direction: ScoringDirection,
    /// How much weight to give this fact when the user's preference matches (0.0-5.0).
    pub weight: f32,
    /// For numeric/score facts: thresholds for good/ok/poor.
    /// e.g. [0.8, 0.5] means >=0.8 is good, >=0.5 is ok, <0.5 is poor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thresholds: Vec<f64>,
}

/// Which direction is "better" for a fact value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScoringDirection {
    /// Higher numeric value or "good"/"high" text = better (e.g. society_quality_score)
    HigherIsBetter,
    /// Lower numeric value or "low"/"quiet" text = better (e.g. noise_level, metro_distance)
    LowerIsBetter,
    /// Text value: match against positive_values list (e.g. "good", "positive")
    TextMatch,
}

/// What kind of value a fact holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FactValue {
    Numeric(f64),
    Text(String),
    Bool(bool),
    Tags(Vec<String>),
    Score {
        value: f64,
        explanation: String,
    },
}

/// Where a fact came from — the provenance chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactSource {
    pub source_type: SourceType,
    /// Link to original source (URL, file path, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Which LLM model produced this (if AI-generated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Which skill produced this fact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Which search query triggered learning this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_by: Option<String>,
}

/// The type of source a fact comes from. Each has different trust and refresh characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceType {
    /// r/bangalore thread — dynamic, refresh weekly
    Reddit,
    /// Google Reviews / Places — dynamic, refresh weekly
    Google,
    /// Karnataka RERA registry — static, fetch once (confidence: 1.0)
    Rera,
    /// BBMP property tax / records — static, refresh yearly
    Bbmp,
    /// News article — dynamic, on-demand
    News,
    /// Derived from other facts — recompute when inputs change
    Computed,
    /// Seed data, hand-curated — static until manually updated
    Manual,
    /// AI-generated synthesis — dynamic, refresh with new data
    Llm,
}

impl SourceType {
    /// Default confidence level for this source type.
    pub fn default_confidence(&self) -> f32 {
        match self {
            Self::Rera => 1.0,
            Self::Bbmp => 0.9,
            Self::Manual => 0.8,
            Self::Google => 0.8,
            Self::Reddit => 0.7,
            Self::News => 0.7,
            Self::Computed => 0.6,
            Self::Llm => 0.5,
        }
    }
}

impl SourcedFact {
    /// Create a manually-sourced fact (from seed data).
    pub fn manual(key: impl Into<String>, value: FactValue) -> Self {
        Self {
            key: key.into(),
            value,
            confidence: SourceType::Manual.default_confidence(),
            source: FactSource {
                source_type: SourceType::Manual,
                url: None,
                model: None,
                skill_id: None,
                triggered_by: None,
            },
            learned_at: Utc::now(),
            version: 1,
            display_template: None,
            answers_preferences: Vec::new(),
            scoring_hint: None,
        }
    }

}
