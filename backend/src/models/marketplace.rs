use serde::{Deserialize, Serialize};

/// Where a property listing originated.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PropertySource {
    /// Original seed / bootstrap data.
    #[default]
    Seed,
    /// Listed directly by a registered seller.
    SellerListed,
    /// Ingested from an external crawler (MagicBricks etc.).
    Crawled,
    /// Discovered via live Gemini discovery.
    Discovered,
}

/// Lifecycle status of a listing.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    #[default]
    Active,
    /// Seller has accepted a bid — deal intent confirmed.
    UnderOffer,
    /// Bid matched — contact exchange done.
    Matched,
    Sold,
}

/// How much a seller's identity has been verified.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SellerVerificationTier {
    /// Phone number provided at registration.
    #[default]
    IdentityVerified,
    /// RERA registration number cross-referenced.
    ReraVerified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seller {
    pub id: String,
    pub name: String,
    pub phone: String,
    pub verification_tier: SellerVerificationTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera_number: Option<String>,
    /// Property IDs this seller has listed.
    pub listing_ids: Vec<String>,
    pub registered_at: String,
}

/// Status of a single bid on a property.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BidStatus {
    #[default]
    Pending,
    Accepted,
    Rejected,
}

/// A buyer's bid on a property. Stored as append-only JSONL per property.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bid {
    pub id: String,
    pub property_id: String,
    pub bidder_name: String,
    /// Stored for seller contact; not exposed in public listing. Optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder_phone: Option<String>,
    /// Bid amount in rupees (consistent with Property.price).
    pub amount: u64,
    pub created_at: String,
    pub status: BidStatus,
}

/// Computed aggregate stats for a property's bids.
/// Written to data/marketplace/bid_stats/{property_id}.json on each bid event.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BidStats {
    pub property_id: String,
    /// Total non-rejected bids.
    pub count: u32,
    /// Average amount across non-rejected bids. 0 if no bids.
    pub average: u64,
    /// Highest bid amount. 0 if no bids.
    pub p100: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_bid_at: Option<String>,
}

impl BidStats {
    pub fn compute(property_id: &str, bids: &[Bid]) -> Self {
        let active: Vec<&Bid> = bids
            .iter()
            .filter(|b| b.status != BidStatus::Rejected)
            .collect();

        let count = active.len() as u32;
        if count == 0 {
            return BidStats {
                property_id: property_id.to_string(),
                ..Default::default()
            };
        }

        let sum: u64 = active.iter().map(|b| b.amount).sum();
        let average = sum / count as u64;
        let p100 = active.iter().map(|b| b.amount).max().unwrap_or(0);
        let last_bid_at = active
            .iter()
            .map(|b| b.created_at.clone())
            .max();

        BidStats {
            property_id: property_id.to_string(),
            count,
            average,
            p100,
            last_bid_at,
        }
    }
}
