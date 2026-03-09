use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use crate::models::{AreaProfile, PropertyCard, Society};
use crate::state::AppState;

/// GET /api/properties — returns UI-ready property cards.
pub async fn list_properties(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PropertyCard>> {
    let cards: Vec<PropertyCard> = state
        .properties
        .iter()
        .map(|p| {
            let society_name = state
                .societies
                .iter()
                .find(|s| s.id == p.society_id)
                .map(|s| s.name.clone())
                .unwrap_or_default();

            PropertyCard {
                id: p.id.clone(),
                title: p.title.clone(),
                area: p.area.clone(),
                price: p.price,
                price_per_sqft: p.price_per_sqft,
                bhk: p.bhk,
                sqft: p.carpet_area_sqft,
                society_name,
                hero_image: p.hero_image.clone(),
                transparency_tags: p.transparency_tags.iter().take(3).cloned().collect(),
            }
        })
        .collect();

    Json(cards)
}

#[derive(Serialize)]
pub struct PropertyDetail {
    pub property: crate::models::Property,
    pub society: Option<Society>,
    pub area: Option<AreaProfile>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/properties/:id — returns joined property + society + area.
pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    let property = state
        .properties
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "property_not_found".to_string(),
                }),
            )
        })?;

    let society = state
        .societies
        .iter()
        .find(|s| s.id == property.society_id)
        .cloned();

    let area = state
        .areas
        .iter()
        .find(|a| a.id == property.area_id)
        .cloned();

    Ok(Json(PropertyDetail {
        property,
        society,
        area,
    }))
}
