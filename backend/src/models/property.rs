use serde::{Deserialize, Serialize};

use super::marketplace::{ListingStatus, PropertySource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub id: String,
    pub title: String,
    pub area: String,
    pub area_id: String,
    pub city: String,
    pub society_id: String,
    pub builder_name: String,
    pub property_type: String,
    pub listing_type: String,
    pub bhk: u32,
    pub price: u64,
    pub price_per_sqft: u64,
    pub carpet_area_sqft: u32,
    pub super_builtup_sqft: u32,
    pub floor: u32,
    pub total_floors: u32,
    pub facing: String,
    pub possession_status: String,
    pub metro_distance_mins: u32,
    pub maintenance_cost_monthly: u32,
    pub society_quality_score: f64,
    pub builder_quality_score: f64,
    pub document_completeness_score: f64,
    pub litigation_risk: f64,
    pub noise_score: f64,
    pub sunlight_score: f64,
    pub airport_noise_score: f64,
    pub waterlogging_risk_score: f64,
    pub traffic_score: f64,
    pub days_on_market: u32,
    #[serde(default)]
    pub greenery_score: Option<f64>,
    #[serde(default)]
    pub open_space_score: Option<f64>,
    #[serde(default)]
    pub resale_strength_score: Option<f64>,
    #[serde(default)]
    pub interest_level: Option<String>,
    #[serde(default)]
    pub saves_last_7d: Option<u32>,
    #[serde(default)]
    pub offers_last_7d: Option<u32>,
    pub images: Vec<String>,
    pub hero_image: String,
    pub description_summary: String,
    pub transparency_tags: Vec<String>,
    pub source_reference: String,
    /// Where this listing came from. Defaults to Seed for existing data.
    #[serde(default)]
    pub source: PropertySource,
    /// Set when source == SellerListed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seller_id: Option<String>,
    /// Current lifecycle state of this listing.
    #[serde(default)]
    pub listing_status: ListingStatus,
}

/// UI-ready property card for the results page.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyCard {
    pub id: String,
    pub title: String,
    pub area: String,
    pub price: u64,
    pub price_per_sqft: u64,
    pub bhk: u32,
    pub sqft: u32,
    pub society_name: String,
    pub builder_name: String,
    pub hero_image: String,
    pub transparency_tags: Vec<String>,
    pub description_summary: String,
    pub possession_status: String,
    pub metro_distance_mins: u32,
    pub floor: u32,
    pub total_floors: u32,
    pub facing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_review_count: Option<u32>,
}
