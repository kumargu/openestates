use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Society {
    pub id: String,
    pub name: String,
    pub area: String,
    pub city: String,
    pub builder_name: String,
    pub year_built: u32,
    pub total_units: u32,
    pub summary: String,
    pub maintenance_sentiment: String,
    pub livability_sentiment: String,
    pub common_positives: Vec<String>,
    pub common_complaints: Vec<String>,
    pub review_summary: String,
    pub future_google_place_name: String,
    pub future_google_place_id: Option<String>,
    pub future_review_enrichment_status: String,
}
