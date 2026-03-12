use serde::{Deserialize, Serialize};

/// An interest event — a buyer expressing interest in a property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interest {
    pub id: String,
    pub property_id: String,
    #[serde(default)]
    pub buyer_name: Option<String>,
    #[serde(default)]
    pub buyer_contact: Option<String>,
    pub created_at: String,
}

/// POST response after expressing interest.
#[derive(Debug, Clone, Serialize)]
pub struct InterestResponse {
    pub id: String,
    pub status: &'static str,
    pub property_id: String,
}

/// Aggregated interest count for a property.
#[derive(Debug, Clone, Serialize)]
pub struct InterestCount {
    pub property_id: String,
    pub count: usize,
}
