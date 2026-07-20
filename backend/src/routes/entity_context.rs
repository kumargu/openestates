use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::entity_context::{
    compose_entity_context, society_anchor_for_property_slug, EntityContextResponse,
};
use crate::state::AppState;

pub async fn entity_context(
    State(state): State<Arc<AppState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<EntityContextResponse>, StatusCode> {
    let bundle = state.serving_bundle.read().await;
    let loaded = bundle.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let context = compose_entity_context(&entity_id, loaded).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(context))
}

pub async fn property_context(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
) -> Result<Json<EntityContextResponse>, StatusCode> {
    let bundle = state.serving_bundle.read().await;
    let loaded = bundle.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let anchor = {
        let properties = state.properties.read().await;
        properties
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| crate::routes::enrichment::society_node_id(&property.society_id))
            .or_else(|| society_anchor_for_property_slug(&property_id, loaded))
            .unwrap_or_else(|| {
                if property_id.starts_with("property:") {
                    property_id.clone()
                } else {
                    format!("property:{property_id}")
                }
            })
    };

    let context = compose_entity_context(&anchor, loaded).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(context))
}
