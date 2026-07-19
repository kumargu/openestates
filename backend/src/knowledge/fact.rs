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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum FactValue {
    Numeric(f64),
    Text(String),
    Bool(bool),
    Tags(Vec<String>),
    Score { value: f64, explanation: String },
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
    /// Legacy AI-generated synthesis. Retained so older KG facts still load.
    /// New enrichment code should not emit this source type.
    Llm,
    /// Bootstrap seed JSON imported at low confidence — superseded by source-backed facts.
    LegacySeed,
}

impl SourceType {
    /// Default confidence level for this source type.
    #[cfg(test)]
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
            Self::LegacySeed => 0.25,
        }
    }
}

impl SourcedFact {
    /// Create a manually-sourced fact (used by tests).
    #[cfg(test)]
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

pub fn google_reviews_url_from_facts(facts: &[SourcedFact], entity_name: &str) -> Option<String> {
    for key in [
        "google_reviews_url",
        "google_maps_url",
        "google_place_url",
        "google_review_url",
    ] {
        if let Some(url) = latest_text_fact(facts, key).filter(|url| is_web_url(url)) {
            return Some(url);
        }
    }

    for fact in facts {
        if is_google_review_link_fact(fact) {
            if let Some(url) = fact.source.url.as_deref().filter(|url| is_web_url(url)) {
                return Some(url.trim().to_string());
            }
        }
    }

    for key in ["google_place_id", "future_google_place_id"] {
        if let Some(place_id) = latest_text_fact(facts, key).filter(|value| !value.is_empty()) {
            return Some(google_maps_place_url(entity_name, &place_id));
        }
    }

    if facts.iter().any(is_google_review_link_fact) {
        return Some(google_maps_search_url(entity_name));
    }

    None
}

fn latest_text_fact(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts
        .iter()
        .filter(|fact| fact.key == key)
        .max_by_key(|fact| fact.version)
        .and_then(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            _ => None,
        })
}

fn is_web_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://") || url.starts_with("http://")
}

fn is_google_review_link_fact(fact: &SourcedFact) -> bool {
    if fact.source.source_type != SourceType::Google {
        return false;
    }

    if fact.source.skill_id.as_deref().is_some_and(|skill_id| {
        skill_id == "fetch_google_review_links" || skill_id == "fetch_google_reviews"
    }) {
        return true;
    }

    matches!(
        fact.key.as_str(),
        "google_reviews_url"
            | "google_maps_url"
            | "google_place_url"
            | "google_review_url"
            | "google_rating"
            | "google_review_count"
            | "google_sentiment"
            | "google_common_themes"
            | "google_top_positives"
            | "google_top_negatives"
    )
}

fn google_maps_place_url(entity_name: &str, place_id: &str) -> String {
    format!(
        "https://www.google.com/maps/search/?api=1&query={}&query_place_id={}",
        percent_encode_query(entity_name),
        percent_encode_query(place_id)
    )
}

fn google_maps_search_url(entity_name: &str) -> String {
    format!(
        "https://www.google.com/maps/search/?api=1&query={}",
        percent_encode_query(entity_name)
    )
}

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char)
            }
            b' ' => encoded.push_str("%20"),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod google_reviews_url_tests {
    use super::*;

    #[test]
    fn google_reviews_url_prefers_explicit_url_fact() {
        let mut fact = SourcedFact::manual(
            "google_reviews_url",
            FactValue::Text("https://maps.google.com/?cid=123".to_string()),
        );
        fact.source.source_type = SourceType::Google;

        assert_eq!(
            google_reviews_url_from_facts(&[fact], "Test Society").as_deref(),
            Some("https://maps.google.com/?cid=123")
        );
    }

    #[test]
    fn google_reviews_url_uses_google_source_url() {
        let mut fact = SourcedFact::manual("google_rating", FactValue::Numeric(4.3));
        fact.source.source_type = SourceType::Google;
        fact.source.skill_id = Some("fetch_google_review_links".to_string());
        fact.source.url = Some("https://www.google.com/maps/place/test".to_string());

        assert_eq!(
            google_reviews_url_from_facts(&[fact], "Test Society").as_deref(),
            Some("https://www.google.com/maps/place/test")
        );
    }

    #[test]
    fn google_reviews_url_ignores_unrelated_google_source_url() {
        let mut fact = SourcedFact::manual("pricing_3bhk", FactValue::Numeric(250.0));
        fact.source.source_type = SourceType::Google;
        fact.source.skill_id = Some("market_pricing_facts".to_string());
        fact.source.url = Some("https://www.magicbricks.com/test".to_string());

        assert_eq!(google_reviews_url_from_facts(&[fact], "Test Society"), None);
    }

    #[test]
    fn google_reviews_url_builds_maps_link_from_place_id() {
        let fact = SourcedFact::manual(
            "google_place_id",
            FactValue::Text("ChIJ_Test Place".to_string()),
        );

        assert_eq!(
            google_reviews_url_from_facts(&[fact], "Test Society").as_deref(),
            Some(
                "https://www.google.com/maps/search/?api=1&query=Test%20Society&query_place_id=ChIJ_Test%20Place"
            )
        );
    }

    #[test]
    fn google_reviews_url_builds_search_link_from_review_fact_without_url() {
        let mut fact = SourcedFact::manual(
            "google_sentiment",
            FactValue::Text("Reviews mention amenities".to_string()),
        );
        fact.source.source_type = SourceType::Google;

        assert_eq!(
            google_reviews_url_from_facts(&[fact], "Test Society").as_deref(),
            Some("https://www.google.com/maps/search/?api=1&query=Test%20Society")
        );
    }
}
