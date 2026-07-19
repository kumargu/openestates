use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const DISCOVERY_CONFIG_PATHS: &[&str] = &[
    "app/config/product/discovery_home.json",
    "data/product/discovery_home.json",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub product_promise: String,
    pub quotes: Vec<DiscoveryQuoteConfig>,
    pub shelves: Vec<DiscoveryShelfConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryQuoteConfig {
    pub text: String,
    pub tone: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryShelfConfig {
    pub id: String,
    pub title: String,
    pub quote: String,
    pub description: String,
    pub search_query: String,
    pub proof_label: String,
}

pub fn load_discovery_config(project_root: &Path) -> DiscoveryConfig {
    for relative in DISCOVERY_CONFIG_PATHS {
        let path = project_root.join(relative);
        if !path.exists() {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => return config,
                Err(err) => {
                    eprintln!(
                        "WARN: Failed to parse discovery config at {}: {err}; trying next path",
                        path.display()
                    );
                }
            },
            Err(err) => {
                eprintln!(
                    "WARN: Failed to read discovery config at {}: {err}; trying next path",
                    path.display()
                );
            }
        }
    }

    eprintln!("WARN: No discovery config found; using defaults");
    DiscoveryConfig::default()
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            product_promise: "Tell us the life you want. We'll show homes with receipts."
                .to_string(),
            quotes: vec![
                DiscoveryQuoteConfig {
                    text: "Fewer homes. Better reasons.".to_string(),
                    tone: "proof".to_string(),
                },
                DiscoveryQuoteConfig {
                    text: "Search by tradeoff, not checkbox.".to_string(),
                    tone: "intent".to_string(),
                },
                DiscoveryQuoteConfig {
                    text: "Receipts before recommendations.".to_string(),
                    tone: "trust".to_string(),
                },
            ],
            shelves: vec![
                DiscoveryShelfConfig {
                    id: "verified_value".to_string(),
                    title: "Value with receipts".to_string(),
                    quote: "Good price, proof attached.".to_string(),
                    description: "Lower per-sqft options with visible source signals.".to_string(),
                    search_query: "good value with proof".to_string(),
                    proof_label: "Price + source facts".to_string(),
                },
                DiscoveryShelfConfig {
                    id: "low_commute_pain".to_string(),
                    title: "Low commute pain".to_string(),
                    quote: "Shorter commute, cleaner proof.".to_string(),
                    description: "Homes with closer metro access or stronger traffic signals."
                        .to_string(),
                    search_query: "near metro low traffic".to_string(),
                    proof_label: "Access facts".to_string(),
                },
                DiscoveryShelfConfig {
                    id: "family_ready".to_string(),
                    title: "Family-ready societies".to_string(),
                    quote: "More life-fit, less guesswork.".to_string(),
                    description: "3BHK+ homes with society, risk, and review signals.".to_string(),
                    search_query: "family friendly 3BHK".to_string(),
                    proof_label: "Society + risk facts".to_string(),
                },
                DiscoveryShelfConfig {
                    id: "premium_explainable".to_string(),
                    title: "Premium but explainable".to_string(),
                    quote: "If it's expensive, it should explain itself.".to_string(),
                    description: "Higher-ticket homes with stronger proof or brand signals."
                        .to_string(),
                    search_query: "premium explainable homes".to_string(),
                    proof_label: "Price + trust facts".to_string(),
                },
                DiscoveryShelfConfig {
                    id: "area_tracker_picks".to_string(),
                    title: "Area Tracker picks".to_string(),
                    quote: "Area signals first.".to_string(),
                    description: "Homes from active micro-markets with enough local context."
                        .to_string(),
                    search_query: "area tracker picks".to_string(),
                    proof_label: "Area facts".to_string(),
                },
            ],
        }
    }
}
