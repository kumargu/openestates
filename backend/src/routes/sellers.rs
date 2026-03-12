use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::models::{PropertyCard, SellerCard};
use crate::state::AppState;

use super::enrichment::enrich_property_card;

/// GET /api/sellers — returns all sellers as SellerCards.
pub async fn list_sellers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<SellerCard>> {
    let cards: Vec<SellerCard> = state.sellers.iter().map(|s| s.to_card()).collect();
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
    let seller = state
        .sellers
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
