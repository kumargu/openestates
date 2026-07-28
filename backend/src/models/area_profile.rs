use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRange {
    pub low: u64,
    pub high: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSignals {
    pub decision_drivers: Vec<String>,
    pub recurring_concerns: Vec<String>,
    pub sentiment_label: String,
    pub last_updated: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AreaTrackerMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listing_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_price_per_sqft: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_min: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bhks: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_inventory_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metro_supported_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_builder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub societies: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demand_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_searches: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_searched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_gap_count: Option<usize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_metrics: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaProfile {
    pub id: String,
    pub name: String,
    pub city: String,
    pub median_price_per_sqft: u64,
    pub price_range_per_sqft: PriceRange,
    pub trend_direction: String,
    pub trend_summary: String,
    pub metro_access_summary: String,
    pub airport_noise_summary: String,
    pub traffic_summary: String,
    pub waterlogging_summary: String,
    pub livability_summary: String,
    pub externality_tags: Vec<String>,
    pub infrastructure_tags: Vec<String>,
    pub reddit_signals: RedditSignals,
    pub community_notes: String,
    pub sample_size: u32,
    pub last_updated: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker_metrics: Option<AreaTrackerMetrics>,
}
