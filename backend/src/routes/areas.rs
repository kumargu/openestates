use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use crate::models::AreaProfile;
use crate::routes::properties::ErrorResponse;
use crate::state::AppState;

/// Lightweight area summary for list/card views.
#[derive(Serialize)]
pub struct AreaListItem {
    pub id: String,
    pub name: String,
    pub median_price_per_sqft: u64,
    pub trend_direction: String,
    pub primary_signal: String,
}

/// GET /api/areas — returns lightweight area list for homepage cards.
pub async fn list_areas(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AreaListItem>> {
    let items: Vec<AreaListItem> = state
        .areas
        .iter()
        .map(|a| {
            let primary_signal = a
                .externality_tags
                .first()
                .cloned()
                .unwrap_or_default();

            AreaListItem {
                id: a.id.clone(),
                name: a.name.clone(),
                median_price_per_sqft: a.median_price_per_sqft,
                trend_direction: a.trend_direction.clone(),
                primary_signal,
            }
        })
        .collect();

    Json(items)
}

/// GET /api/areas/:id — returns full area profile.
pub async fn get_area(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AreaProfile>, (StatusCode, Json<ErrorResponse>)> {
    let area = state
        .areas
        .iter()
        .find(|a| a.id == id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "area_not_found".to_string(),
                }),
            )
        })?;

    Ok(Json(area))
}
