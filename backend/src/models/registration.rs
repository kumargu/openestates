use serde::{Deserialize, Serialize};

/// A registration draft represents a seller's in-progress registration.
/// Persisted per-draft at `data/registrations/{id}.json`.
/// Not visible to buyers until explicitly published (future step).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationDraft {
    pub id: String,
    /// 0-7: the last completed step number.
    pub current_step: u8,
    pub created_at: String,
    pub updated_at: String,
    // Step 1: Basic Info
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    // Step 2: Property Prompt
    #[serde(default)]
    pub property_prompt: Option<String>,
    // Steps 3-7: placeholders for future days
    #[serde(default)]
    pub property_details: Option<serde_json::Value>,
    #[serde(default)]
    pub pricing: Option<serde_json::Value>,
    #[serde(default)]
    pub documents: Option<serde_json::Value>,
    #[serde(default)]
    pub photos: Option<serde_json::Value>,
    #[serde(default)]
    pub society_info: Option<serde_json::Value>,
    /// Set after publish — prevents double-publish. Contains the seller ID.
    #[serde(default)]
    pub published_seller_id: Option<String>,
}

impl RegistrationDraft {
    /// Create a new blank draft with the given ID.
    pub fn new(id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            current_step: 0,
            created_at: now.clone(),
            updated_at: now,
            name: None,
            email: None,
            phone: None,
            property_prompt: None,
            property_details: None,
            pricing: None,
            documents: None,
            photos: None,
            society_info: None,
            published_seller_id: None,
        }
    }

    /// Compute completeness as a percentage (7 steps total, each ~14%).
    pub fn completeness_pct(&self) -> u32 {
        let steps = [
            self.name.is_some(),             // Step 1
            self.property_prompt.is_some(),  // Step 2
            self.property_details.is_some(), // Step 3
            self.pricing.is_some(),          // Step 4
            self.documents.is_some(),        // Step 5
            self.photos.is_some(),           // Step 6
            self.society_info.is_some(),     // Step 7
        ];
        let count = steps.iter().filter(|&&v| v).count() as u32;
        (count * 100) / 7
    }
}
