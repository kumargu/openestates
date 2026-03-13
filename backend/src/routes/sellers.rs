use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::models::{PropertyCard, SellerCard, MIN_COMPLETENESS_PCT};
use crate::state::AppState;

use super::enrichment::enrich_property_card;

/// GET /api/sellers — returns all sellers as SellerCards.
pub async fn list_sellers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<SellerCard>> {
    let sellers = state.sellers.read().await;
    let cards: Vec<SellerCard> = sellers.iter().map(|s| s.to_card()).collect();
    Json(cards)
}

#[derive(Serialize)]
pub struct SellerDetail {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub property_ids: Vec<String>,
    pub has_basic_info: bool,
    pub has_property_prompt: bool,
    pub property_prompt: Option<String>,
    pub has_details: bool,
    pub has_pricing: bool,
    pub has_documents: bool,
    pub has_photos: bool,
    pub has_society_info: bool,
    pub documents_provided: Vec<String>,
    pub verified: bool,
    pub created_at: String,
    pub updated_at: String,
    pub completeness_pct: u32,
    pub properties: Vec<PropertyCard>,
}

#[derive(Serialize)]
pub struct SellerError {
    pub error: String,
}

/// GET /api/sellers/{id} — returns full seller with linked property details.
pub async fn get_seller(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SellerDetail>, (StatusCode, Json<SellerError>)> {
    let sellers = state.sellers.read().await;
    let seller = sellers
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(SellerError {
                    error: format!("seller '{}' not found", id),
                }),
            )
        })?;

    let graph = state.knowledge.read().await;
    let properties_lock = state.properties.read().await;

    let linked_properties: Vec<PropertyCard> = seller
        .property_ids
        .iter()
        .filter_map(|pid| properties_lock.iter().find(|p| &p.id == pid))
        .map(|p| enrich_property_card(p, &state.societies, &graph))
        .collect();

    Ok(Json(SellerDetail {
        id: seller.id.clone(),
        name: seller.name.clone(),
        email: seller.email.clone(),
        phone: seller.phone.clone(),
        property_ids: seller.property_ids.clone(),
        has_basic_info: seller.has_basic_info,
        has_property_prompt: seller.has_property_prompt,
        property_prompt: seller.property_prompt.clone(),
        has_details: seller.has_details,
        has_pricing: seller.has_pricing,
        has_documents: seller.has_documents,
        has_photos: seller.has_photos,
        has_society_info: seller.has_society_info,
        documents_provided: seller.documents_provided.clone(),
        verified: seller.verified,
        created_at: seller.created_at.clone(),
        updated_at: seller.updated_at.clone(),
        completeness_pct: seller.completeness_pct(),
        properties: linked_properties,
    }))
}

// --- Dashboard types ---

#[derive(Serialize)]
pub struct InterestTimelineEntry {
    pub date: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct PropertyInterestSummary {
    pub property_id: String,
    pub property_title: String,
    pub interest_count: usize,
    pub last_interest_at: Option<String>,
    pub timeline: Vec<InterestTimelineEntry>,
}

#[derive(Serialize)]
pub struct CompletenessGuide {
    pub pct: u32,
    pub min_pct: u32,
    pub completed_steps: Vec<String>,
    pub next_steps: Vec<String>,
}

#[derive(Serialize)]
pub struct SellerDashboard {
    pub seller: SellerDetail,
    pub interest_summary: Vec<PropertyInterestSummary>,
    pub completeness_guide: CompletenessGuide,
}

/// Step name + accessor for the 7 completeness booleans on a Seller.
type CompletenessStep = (&'static str, fn(&crate::models::Seller) -> bool);

/// The 7 completeness steps and their corresponding `has_*` field accessors.
const COMPLETENESS_STEPS: &[CompletenessStep] = &[
    ("basic_info", |s| s.has_basic_info),
    ("property_prompt", |s| s.has_property_prompt),
    ("details", |s| s.has_details),
    ("pricing", |s| s.has_pricing),
    ("documents", |s| s.has_documents),
    ("photos", |s| s.has_photos),
    ("society_info", |s| s.has_society_info),
];

/// GET /api/sellers/{id}/dashboard — seller's own dashboard view.
/// Returns seller detail + per-property interest counts + completeness guide.
pub async fn get_seller_dashboard(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SellerDashboard>, (StatusCode, Json<SellerError>)> {
    let sellers = state.sellers.read().await;
    let seller = sellers
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(SellerError {
                    error: format!("seller '{}' not found", id),
                }),
            )
        })?;

    let graph = state.knowledge.read().await;
    let properties_lock = state.properties.read().await;

    // Build linked property cards
    let linked_properties: Vec<PropertyCard> = seller
        .property_ids
        .iter()
        .filter_map(|pid| properties_lock.iter().find(|p| &p.id == pid))
        .map(|p| enrich_property_card(p, &state.societies, &graph))
        .collect();

    // Build interest summary per property (with daily timeline for last 30 days)
    let interests_dir = state.project_root.join("data").join("interests");
    let mut interest_summary = Vec::new();
    for prop in &linked_properties {
        let file_path = interests_dir.join(format!("{}.jsonl", prop.id));
        let (interest_count, last_interest_at, timeline) =
            build_interest_timeline(&file_path).await;

        interest_summary.push(PropertyInterestSummary {
            property_id: prop.id.clone(),
            property_title: prop.title.clone(),
            interest_count,
            last_interest_at,
            timeline,
        });
    }

    // Build completeness guide
    let mut completed_steps = Vec::new();
    let mut next_steps = Vec::new();
    for &(step_name, accessor) in COMPLETENESS_STEPS {
        if accessor(seller) {
            completed_steps.push(step_name.to_string());
        } else {
            next_steps.push(step_name.to_string());
        }
    }
    let completeness_guide = CompletenessGuide {
        pct: seller.completeness_pct(),
        min_pct: MIN_COMPLETENESS_PCT,
        completed_steps,
        next_steps,
    };

    let seller_detail = SellerDetail {
        id: seller.id.clone(),
        name: seller.name.clone(),
        email: seller.email.clone(),
        phone: seller.phone.clone(),
        property_ids: seller.property_ids.clone(),
        has_basic_info: seller.has_basic_info,
        has_property_prompt: seller.has_property_prompt,
        property_prompt: seller.property_prompt.clone(),
        has_details: seller.has_details,
        has_pricing: seller.has_pricing,
        has_documents: seller.has_documents,
        has_photos: seller.has_photos,
        has_society_info: seller.has_society_info,
        documents_provided: seller.documents_provided.clone(),
        verified: seller.verified,
        created_at: seller.created_at.clone(),
        updated_at: seller.updated_at.clone(),
        completeness_pct: seller.completeness_pct(),
        properties: linked_properties,
    };

    Ok(Json(SellerDashboard {
        seller: seller_detail,
        interest_summary,
        completeness_guide,
    }))
}

/// Parse a JSONL interest file and return (total_count, last_timestamp, daily_timeline).
/// The timeline covers the last 30 days, including zero-count days.
async fn build_interest_timeline(
    path: &std::path::Path,
) -> (usize, Option<String>, Vec<InterestTimelineEntry>) {
    let contents = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => return (0, None, Vec::new()),
    };

    let lines: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
    let total_count = lines.len();

    if total_count == 0 {
        return (0, None, Vec::new());
    }

    // Parse created_at dates from each line, bucket by date string (YYYY-MM-DD)
    let mut date_counts: HashMap<String, usize> = HashMap::new();
    let mut last_timestamp: Option<String> = None;

    for line in &lines {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(created_at) = val.get("created_at").and_then(|v| v.as_str()) {
                // Track chronologically latest timestamp (not file-order last)
                let is_newer = match &last_timestamp {
                    None => true,
                    Some(prev) => created_at > prev.as_str(),
                };
                if is_newer {
                    last_timestamp = Some(created_at.to_string());
                }
                // Extract YYYY-MM-DD from RFC3339 timestamp
                if let Some(date_part) = created_at.get(..10) {
                    *date_counts.entry(date_part.to_string()).or_insert(0) += 1;
                }
            }
        }
        // Skip malformed lines silently
    }

    // Build timeline for last 30 days (including zero-count days)
    let today = chrono::Utc::now().date_naive();
    let mut timeline = Vec::with_capacity(30);
    for days_ago in (0..30).rev() {
        let date = today - chrono::Duration::days(days_ago);
        let date_str = date.format("%Y-%m-%d").to_string();
        let count = date_counts.get(&date_str).copied().unwrap_or(0);
        timeline.push(InterestTimelineEntry {
            date: date_str,
            count,
        });
    }

    (total_count, last_timestamp, timeline)
}
