use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use crate::models::PropertyCard;
use crate::state::AppState;

use super::enrichment::{enrich_area, enrich_property_card, enrich_society};

/// GET /api/properties — returns UI-ready property cards.
pub async fn list_properties(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PropertyCard>> {
    let graph = state.knowledge.read().await;
    let properties = state.properties.read().await;

    let cards: Vec<PropertyCard> = properties
        .iter()
        .map(|p| enrich_property_card(p, &state.societies, &graph))
        .collect();

    Json(cards)
}

#[derive(Serialize)]
pub struct PropertyDetail {
    pub property: crate::models::Property,
    pub society: Option<crate::models::Society>,
    pub area: Option<crate::models::AreaProfile>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/properties/:id — returns joined property + society + area,
/// enriched from the knowledge graph.
pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    let properties = state.properties.read().await;
    let property = properties
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

    let graph = state.knowledge.read().await;

    // Enrich society from KG
    let mut society = state
        .societies
        .iter()
        .find(|s| s.id == property.society_id)
        .cloned();
    if let Some(ref mut soc) = society {
        enrich_society(soc, &graph);
    }

    // Enrich area from KG
    let mut area = state
        .areas
        .iter()
        .find(|a| a.id == property.area_id)
        .cloned();
    if let Some(ref mut ap) = area {
        enrich_area(ap, &graph);
    }

    Ok(Json(PropertyDetail {
        property,
        society,
        area,
    }))
}
