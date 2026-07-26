use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionPoliciesFile {
    pub version: u32,
    #[serde(default)]
    pub default_strategy: Option<String>,
    #[serde(default)]
    pub source_tiers: Vec<String>,
    #[serde(default)]
    pub never_default_fact_prefixes: Vec<String>,
    #[serde(default)]
    pub source_caps: HashMap<String, f32>,
}

pub fn resolution_policies_path() -> std::path::PathBuf {
    dag_root().join("resolution_policies.json")
}

pub fn load_resolution_policies() -> Result<ResolutionPoliciesFile, DagConfigError> {
    load_json(&resolution_policies_path())
}

pub fn source_tier_rank(source_type: &str, policies: &ResolutionPoliciesFile) -> u32 {
    let normalized = normalize_source_type(source_type);
    policies
        .source_tiers
        .iter()
        .position(|tier| tier.eq_ignore_ascii_case(&normalized))
        .map(|index| index as u32)
        .unwrap_or(u32::MAX)
}

pub fn buyer_visible_fact(
    _fact_key: &str,
    _source_type: &str,
    _policies: &ResolutionPoliciesFile,
) -> bool {
    true
}

pub fn better_source_type(
    left: &str,
    right: &str,
    left_confidence: f32,
    right_confidence: f32,
    policies: &ResolutionPoliciesFile,
) -> bool {
    let left_tier = source_tier_rank(left, policies);
    let right_tier = source_tier_rank(right, policies);
    if left_tier != right_tier {
        return left_tier < right_tier;
    }
    if (left_confidence - right_confidence).abs() > f32::EPSILON {
        return left_confidence > right_confidence;
    }
    false
}

fn normalize_source_type(source_type: &str) -> String {
    source_type
        .trim()
        .trim_start_matches("SourceType::")
        .replace('_', "")
        .to_ascii_lowercase()
        .replace("reddittheme", "reddit_theme")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_sources_order_by_tier() {
        let policies = load_resolution_policies().expect("resolution policies load");
        assert!(better_source_type("Rera", "Google", 1.0, 0.8, &policies));
        assert!(!better_source_type(
            "RedditTheme",
            "Google",
            0.45,
            0.8,
            &policies
        ));
        assert!(better_source_type(
            "Google",
            "RedditTheme",
            0.8,
            0.45,
            &policies
        ));
        assert!(better_source_type(
            "Rera",
            "RedditTheme",
            0.9,
            0.45,
            &policies
        ));
    }

    #[test]
    fn buyer_visible_fact_allows_source_backed_facts() {
        let policies = load_resolution_policies().expect("resolution policies load");
        assert!(buyer_visible_fact(
            "market.price_per_sqft",
            "Rera",
            &policies
        ));
    }
}
