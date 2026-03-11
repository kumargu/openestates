use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::models::{Bid, BidStats, BidStatus, ListingStatus, Seller, SellerVerificationTier};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a time-based unique ID. Collision-safe for our write rate.
fn new_id(prefix: &str) -> String {
    format!("{}-{}", prefix, chrono::Utc::now().timestamp_millis())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Persist seller to data/marketplace/sellers/{id}.json (atomic write via .tmp).
fn save_seller(root: &std::path::Path, seller: &Seller) -> std::io::Result<()> {
    let dir = root.join("sellers");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{}.json.tmp", seller.id));
    let dst = dir.join(format!("{}.json", seller.id));
    std::fs::write(&tmp, serde_json::to_vec_pretty(seller).unwrap())?;
    std::fs::rename(tmp, dst)
}

/// Append a bid to data/marketplace/bids/{property_id}.jsonl.
fn append_bid(root: &std::path::Path, bid: &Bid) -> std::io::Result<()> {
    let dir = root.join("bids");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.jsonl", bid.property_id));
    let line = format!("{}\n", serde_json::to_string(bid).unwrap());
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Write bid stats to data/marketplace/bid_stats/{property_id}.json (atomic).
fn save_bid_stats(root: &std::path::Path, stats: &BidStats) -> std::io::Result<()> {
    let dir = root.join("bid_stats");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{}.json.tmp", stats.property_id));
    let dst = dir.join(format!("{}.json", stats.property_id));
    std::fs::write(&tmp, serde_json::to_vec_pretty(stats).unwrap())?;
    std::fs::rename(tmp, dst)
}

// ---------------------------------------------------------------------------
// Seller registration
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSellerRequest {
    pub name: String,
    pub phone: String,
    pub rera_number: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSellerResponse {
    pub seller: Seller,
}

/// POST /api/sellers/register
pub async fn register_seller(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterSellerRequest>,
) -> Result<Json<RegisterSellerResponse>, (StatusCode, Json<serde_json::Value>)> {
    if body.name.trim().is_empty() || body.phone.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "name and phone are required" })),
        ));
    }

    let tier = if body.rera_number.is_some() {
        SellerVerificationTier::ReraVerified
    } else {
        SellerVerificationTier::IdentityVerified
    };

    let seller = Seller {
        id: new_id("seller"),
        name: body.name.trim().to_string(),
        phone: body.phone.trim().to_string(),
        verification_tier: tier,
        rera_number: body.rera_number,
        listing_ids: Vec::new(),
        registered_at: now_iso(),
    };

    // Persist to disk
    if let Err(e) = save_seller(&state.marketplace_root, &seller) {
        eprintln!("WARN: Failed to persist seller {}: {}", seller.id, e);
    }

    // Add to in-memory state
    {
        let mut sellers = state.sellers.write().await;
        sellers.insert(seller.id.clone(), seller.clone());
    }

    Ok(Json(RegisterSellerResponse { seller }))
}

// ---------------------------------------------------------------------------
// Seller profile + listings
// ---------------------------------------------------------------------------

/// GET /api/sellers/{id}
pub async fn get_seller(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Seller>, StatusCode> {
    let sellers = state.sellers.read().await;
    sellers
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SellerListingItem {
    pub property_id: String,
    pub listing_status: ListingStatus,
    pub bid_stats: BidStats,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SellerListingsResponse {
    pub seller: Seller,
    pub listings: Vec<SellerListingItem>,
}

/// GET /api/sellers/{id}/listings
pub async fn get_seller_listings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SellerListingsResponse>, StatusCode> {
    let seller = {
        let sellers = state.sellers.read().await;
        sellers.get(&id).cloned().ok_or(StatusCode::NOT_FOUND)?
    };

    let bid_stats_map = state.bid_stats.read().await;
    let properties = state.properties.read().await;

    let listings: Vec<SellerListingItem> = seller
        .listing_ids
        .iter()
        .filter_map(|pid| {
            let prop = properties.iter().find(|p| p.id == *pid)?;
            let stats = bid_stats_map
                .get(pid)
                .cloned()
                .unwrap_or_else(|| BidStats {
                    property_id: pid.clone(),
                    ..Default::default()
                });
            Some(SellerListingItem {
                property_id: pid.clone(),
                listing_status: prop.listing_status.clone(),
                bid_stats: stats,
            })
        })
        .collect();

    Ok(Json(SellerListingsResponse { seller, listings }))
}

// ---------------------------------------------------------------------------
// Bids
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceBidRequest {
    pub bidder_name: String,
    pub bidder_phone: String,
    /// Bid amount in rupees.
    pub amount: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceBidResponse {
    pub bid: Bid,
    pub bid_stats: BidStats,
}

/// POST /api/properties/{id}/bids
pub async fn place_bid(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
    Json(body): Json<PlaceBidRequest>,
) -> Result<Json<PlaceBidResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Verify property exists
    {
        let props = state.properties.read().await;
        if !props.iter().any(|p| p.id == property_id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Property not found" })),
            ));
        }
    }

    if body.bidder_name.trim().is_empty() || body.bidder_phone.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "bidderName and bidderPhone are required" })),
        ));
    }
    if body.amount == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "amount must be greater than 0" })),
        ));
    }

    let bid = Bid {
        id: new_id("bid"),
        property_id: property_id.clone(),
        bidder_name: body.bidder_name.trim().to_string(),
        bidder_phone: body.bidder_phone.trim().to_string(),
        amount: body.amount,
        created_at: now_iso(),
        status: BidStatus::Pending,
    };

    // Persist bid to JSONL
    if let Err(e) = append_bid(&state.marketplace_root, &bid) {
        eprintln!("WARN: Failed to persist bid {}: {}", bid.id, e);
    }

    // Update in-memory bids and recompute stats
    let bid_stats = {
        let mut bids_map = state.bids.write().await;
        let bids = bids_map.entry(property_id.clone()).or_default();
        bids.push(bid.clone());
        BidStats::compute(&property_id, bids)
    };

    // Persist stats
    if let Err(e) = save_bid_stats(&state.marketplace_root, &bid_stats) {
        eprintln!("WARN: Failed to persist bid stats for {}: {}", property_id, e);
    }

    // Update in-memory stats
    {
        let mut stats_map = state.bid_stats.write().await;
        stats_map.insert(property_id.clone(), bid_stats.clone());
    }

    Ok(Json(PlaceBidResponse { bid, bid_stats }))
}

/// GET /api/properties/{id}/bids
///
/// Returns public bid list (bidder_phone redacted).
pub async fn list_bids(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
) -> Json<serde_json::Value> {
    let bids_map = state.bids.read().await;
    let bids: Vec<serde_json::Value> = bids_map
        .get(&property_id)
        .map(|bids| {
            bids.iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id,
                        "amount": b.amount,
                        "createdAt": b.created_at,
                        "status": b.status,
                        // Phone redacted from public listing
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Json(serde_json::json!({ "bids": bids, "count": bids.len() }))
}

/// GET /api/properties/{id}/bid-stats
pub async fn get_bid_stats(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
) -> Json<BidStats> {
    let stats_map = state.bid_stats.read().await;
    Json(stats_map.get(&property_id).cloned().unwrap_or_else(|| BidStats {
        property_id: property_id.clone(),
        ..Default::default()
    }))
}

// ---------------------------------------------------------------------------
// Bid accept / reject
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBidStatusRequest {
    pub status: String,
}

/// PUT /api/properties/{id}/bids/{bid_id}/status
pub async fn update_bid_status(
    State(state): State<Arc<AppState>>,
    Path((property_id, bid_id)): Path<(String, String)>,
    Json(body): Json<UpdateBidStatusRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let new_status = match body.status.as_str() {
        "accepted" => BidStatus::Accepted,
        "rejected" => BidStatus::Rejected,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("invalid status: {}", other) })),
            ));
        }
    };

    // Update in-memory bids
    let (found, recomputed_stats) = {
        let mut bids_map = state.bids.write().await;
        let bids = bids_map.entry(property_id.clone()).or_default();
        let found = bids
            .iter_mut()
            .find(|b| b.id == bid_id)
            .map(|b| { b.status = new_status.clone(); true })
            .unwrap_or(false);
        let stats = BidStats::compute(&property_id, bids);
        (found, stats)
    };

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Bid not found" })),
        ));
    }

    // Persist updated stats
    if let Err(e) = save_bid_stats(&state.marketplace_root, &recomputed_stats) {
        eprintln!("WARN: Failed to persist bid stats: {}", e);
    }
    {
        let mut stats_map = state.bid_stats.write().await;
        stats_map.insert(property_id.clone(), recomputed_stats.clone());
    }

    // If accepted, move property to UnderOffer
    if new_status == BidStatus::Accepted {
        let mut props = state.properties.write().await;
        if let Some(prop) = props.iter_mut().find(|p| p.id == property_id) {
            prop.listing_status = ListingStatus::UnderOffer;
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "bid_id": bid_id,
        "status": body.status,
        "bid_stats": recomputed_stats,
    })))
}
