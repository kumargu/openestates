use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seller {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub property_ids: Vec<String>,
    #[serde(default)]
    pub has_basic_info: bool,
    #[serde(default)]
    pub has_property_prompt: bool,
    #[serde(default)]
    pub property_prompt: Option<String>,
    #[serde(default)]
    pub has_details: bool,
    #[serde(default)]
    pub has_pricing: bool,
    #[serde(default)]
    pub has_documents: bool,
    #[serde(default)]
    pub has_photos: bool,
    #[serde(default)]
    pub has_society_info: bool,
    #[serde(default)]
    pub documents_provided: Vec<String>,
    #[serde(default)]
    pub verified: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Seller {
    /// Compute completeness percentage from the 7 has_* fields.
    /// Each field is ~14.3%, 7 fields = 100%.
    pub fn completeness_pct(&self) -> u32 {
        let steps = [
            self.has_basic_info,
            self.has_property_prompt,
            self.has_details,
            self.has_pricing,
            self.has_documents,
            self.has_photos,
            self.has_society_info,
        ];
        let count = steps.iter().filter(|&&v| v).count() as u32;
        (count * 100) / 7
    }

    /// Convert to a SellerCard for list views.
    pub fn to_card(&self) -> SellerCard {
        SellerCard {
            id: self.id.clone(),
            name: self.name.clone(),
            property_count: self.property_ids.len() as u32,
            completeness_pct: self.completeness_pct(),
            verified: self.verified,
        }
    }
}

/// Lightweight seller representation for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SellerCard {
    pub id: String,
    pub name: String,
    pub property_count: u32,
    pub completeness_pct: u32,
    pub verified: bool,
}

/// Buyer-facing seller summary for property detail pages.
/// Intentionally omits email/phone — those are not exposed to buyers.
#[derive(Debug, Clone, Serialize)]
pub struct SellerSummary {
    pub name: String,
    pub verified: bool,
    pub completeness_pct: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_prompt: Option<String>,
    pub documents_provided: Vec<String>,
}

impl Seller {
    /// Convert to a buyer-facing summary (no email/phone).
    pub fn to_summary(&self) -> SellerSummary {
        SellerSummary {
            name: self.name.clone(),
            verified: self.verified,
            completeness_pct: self.completeness_pct(),
            property_prompt: self.property_prompt.clone(),
            documents_provided: self.documents_provided.clone(),
        }
    }
}
