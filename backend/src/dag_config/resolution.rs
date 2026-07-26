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
    #[serde(default)]
    pub overrides: HashMap<String, ResolutionOverride>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResolutionOverride {
    #[serde(default)]
    pub source_priority: Vec<String>,
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
    fact_key: &str,
    source_type: &str,
    policies: &ResolutionPoliciesFile,
) -> bool {
    let is_never_default = policies
        .never_default_fact_prefixes
        .iter()
        .any(|prefix| fact_key.starts_with(prefix));
    if !is_never_default {
        return true;
    }

    let normalized = normalize_source_type(source_type);
    let source_cap = policies
        .source_caps
        .get(&normalized)
        .copied()
        .unwrap_or(1.0);
    if source_cap < 0.5 {
        return false;
    }

    true
}

pub fn better_source_type(
    left: &str,
    right: &str,
    left_confidence: f32,
    right_confidence: f32,
    policies: &ResolutionPoliciesFile,
) -> bool {
    better_source_type_for_fact(
        None,
        left,
        right,
        left_confidence,
        right_confidence,
        policies,
    )
}

pub fn better_source_type_for_fact(
    fact_key: Option<&str>,
    left: &str,
    right: &str,
    left_confidence: f32,
    right_confidence: f32,
    policies: &ResolutionPoliciesFile,
) -> bool {
    let left_tier = source_rank_for_fact(fact_key, left, policies);
    let right_tier = source_rank_for_fact(fact_key, right, policies);
    if left_tier != right_tier {
        return left_tier < right_tier;
    }
    let left_effective_confidence = capped_confidence(left, left_confidence, policies);
    let right_effective_confidence = capped_confidence(right, right_confidence, policies);
    if (left_effective_confidence - right_effective_confidence).abs() > f32::EPSILON {
        return left_effective_confidence > right_effective_confidence;
    }
    false
}

fn source_rank_for_fact(
    fact_key: Option<&str>,
    source_type: &str,
    policies: &ResolutionPoliciesFile,
) -> u32 {
    let normalized = normalize_source_type(source_type);
    if let Some(fact_key) = fact_key {
        if let Some(source_priority) = policies
            .overrides
            .get(fact_key)
            .map(|override_policy| &override_policy.source_priority)
        {
            if let Some(index) = source_priority
                .iter()
                .position(|source| normalize_source_type(source) == normalized)
            {
                return index as u32;
            }
        }
    }

    source_tier_rank(source_type, policies).saturating_add(policies.source_tiers.len() as u32)
}

fn capped_confidence(source_type: &str, confidence: f32, policies: &ResolutionPoliciesFile) -> f32 {
    let normalized = normalize_source_type(source_type);
    policies
        .source_caps
        .get(&normalized)
        .map(|cap| confidence.min(*cap))
        .unwrap_or(confidence)
}

fn normalize_source_type(source_type: &str) -> String {
    let compact = source_type
        .trim()
        .trim_start_matches("SourceType::")
        .replace(['_', '-', ' '], "")
        .to_ascii_lowercase()
        .replace("::", "");
    match compact.as_str() {
        "registeredtransactions" => "registered_transactions".to_string(),
        "opencity" => "opencity".to_string(),
        "openstreetmap" => "openstreetmap".to_string(),
        "reddittheme" => "reddit_theme".to_string(),
        "sellerclaim" => "seller_claim".to_string(),
        _ => compact,
    }
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

    #[test]
    fn buyer_visible_fact_blocks_capped_never_default_facts() {
        let policies = load_resolution_policies().expect("resolution policies load");
        assert!(!buyer_visible_fact(
            "risk.flooding",
            "seller_claim",
            &policies
        ));
    }

    #[test]
    fn source_caps_bound_effective_confidence_within_same_tier() {
        let mut policies = load_resolution_policies().expect("resolution policies load");
        policies.source_tiers = vec!["seller_claim".to_string(), "seller_claim".to_string()];
        assert!(!better_source_type(
            "seller_claim",
            "seller_claim",
            0.9,
            0.5,
            &policies
        ));
    }

    #[test]
    fn fact_overrides_can_prefer_lower_default_tier_sources() {
        let policies = load_resolution_policies().expect("resolution policies load");

        assert!(better_source_type_for_fact(
            Some("market.transaction.price_per_sqft"),
            "registered_transactions",
            "Rera",
            0.8,
            1.0,
            &policies
        ));
        assert!(!better_source_type_for_fact(
            Some("market.transaction.price_per_sqft"),
            "Rera",
            "registered_transactions",
            1.0,
            0.8,
            &policies
        ));
    }
}
