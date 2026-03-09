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
}
