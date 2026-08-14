use std::sync::Arc;

use axum::extract::{Json as RequestJson, Query};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::dag_config::ui_surfaces_config;
use crate::proof_focus::ProofFocus;
use crate::routes::enrichment::kg_entity_refs_for_property;
use crate::state::AppState;
use crate::surfaces::{build_surface_scene_with_focus, SurfaceSceneResponse};

const MAX_SURFACE_BATCH_PROPERTIES: usize = 24;
const MAX_SURFACE_IDS: usize = 8;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

#[derive(Deserialize)]
pub struct SurfaceListQuery {
    pub ids: Option<String>,
    pub focus: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceBatchRequest {
    pub property_ids: Vec<String>,
    pub surface_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertySurfacesResponse {
    pub contract_version: u32,
    pub property_id: String,
    pub scenes: Vec<SurfaceSceneResponse>,
    pub missing: Vec<SurfaceSceneMissing>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSceneMissing {
    pub surface_id: String,
    pub reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceBatchResponse {
    pub contract_version: u32,
    pub items: Vec<PropertySurfacesResponse>,
}

/// GET /api/properties/{id}/surfaces/{surface_id}
///
/// Returns a backend-projected, receipt-backed scene for one buyer-facing
/// property surface. The UI renders this contract; it must not reconstruct
/// proximity, traversal, ranking, or evidence validity on its own.
pub async fn get_property_surface(
    State(state): State<Arc<AppState>>,
    Path((property_id, surface_id)): Path<(String, String)>,
    Query(query): Query<SurfaceListQuery>,
) -> Result<Json<SurfaceSceneResponse>, (StatusCode, Json<ErrorResponse>)> {
    let focus = parse_focus(query.focus.as_deref())?;
    let response = build_property_surfaces_response(
        &state,
        &property_id,
        std::slice::from_ref(&surface_id),
        focus.as_ref(),
    )
    .await
    .map_err(route_error)?;
    if let Some(scene) = response.scenes.into_iter().next() {
        return Ok(Json(scene));
    }
    let reason = response
        .missing
        .first()
        .map(|missing| missing.reason.as_str())
        .unwrap_or("surface_scene_empty");
    Err(route_error(SurfaceRouteError::not_found(reason)))
}

/// GET /api/properties/{id}/surfaces?ids=around_this_home,water_context
pub async fn list_property_surfaces(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
    Query(query): Query<SurfaceListQuery>,
) -> Result<Json<PropertySurfacesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let surface_ids = parse_surface_ids(query.ids.as_deref())?;
    let focus = parse_focus(query.focus.as_deref())?;
    build_property_surfaces_response(&state, &property_id, &surface_ids, focus.as_ref())
        .await
        .map(Json)
        .map_err(route_error)
}

/// POST /api/properties/surfaces/batch
pub async fn get_property_surfaces_batch(
    State(state): State<Arc<AppState>>,
    RequestJson(request): RequestJson<SurfaceBatchRequest>,
) -> Result<Json<SurfaceBatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    if request.property_ids.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "property_ids_required"));
    }
    if request.property_ids.len() > MAX_SURFACE_BATCH_PROPERTIES {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "too_many_property_ids_requested",
        ));
    }
    let surface_ids = validate_surface_ids(request.surface_ids).map_err(route_error)?;
    let mut items = Vec::new();
    for property_id in request.property_ids {
        items.push(
            build_property_surfaces_response(&state, &property_id, &surface_ids, None)
                .await
                .map_err(route_error)?,
        );
    }
    Ok(Json(SurfaceBatchResponse {
        contract_version: crate::surfaces::SURFACE_SCENE_CONTRACT_VERSION,
        items,
    }))
}

async fn build_property_surfaces_response(
    state: &Arc<AppState>,
    property_id: &str,
    surface_ids: &[String],
    proof_focus: Option<&ProofFocus>,
) -> Result<PropertySurfacesResponse, SurfaceRouteError> {
    let properties = state.properties.read().await;
    let property = properties
        .iter()
        .find(|property| property.id == *property_id)
        .cloned()
        .ok_or_else(|| SurfaceRouteError::not_found("property_not_found"))?;
    if !property.is_eligible_for(crate::buyer_eligibility::DETAIL_SURFACE) {
        return Err(SurfaceRouteError::not_ready(&property));
    }
    drop(properties);

    let config = ui_surfaces_config()
        .map_err(|err| SurfaceRouteError::internal(format!("surface_config_invalid: {err}")))?;

    let serving_bundle = state.serving_bundle.read().await.clone().ok_or_else(|| {
        SurfaceRouteError::status(
            StatusCode::SERVICE_UNAVAILABLE,
            "serving_bundle_unavailable",
        )
    })?;
    let graph = state.knowledge.read().await;
    let entity_refs = kg_entity_refs_for_property(&property, &graph);
    drop(graph);
    let societies = state.societies.read().await;
    let society_name = societies
        .iter()
        .find(|society| society.id == property.society_id)
        .map(|society| society.name.as_str());
    let mut scenes = Vec::new();
    let mut missing = Vec::new();
    for surface_id in surface_ids {
        let Some(surface) = config
            .surfaces
            .iter()
            .find(|surface| surface.id == *surface_id)
        else {
            missing.push(SurfaceSceneMissing {
                surface_id: surface_id.clone(),
                reason: "surface_not_found".to_string(),
            });
            continue;
        };
        if surface.scene.is_none() {
            missing.push(SurfaceSceneMissing {
                surface_id: surface_id.clone(),
                reason: "surface_scene_not_configured".to_string(),
            });
            continue;
        }
        let surface_focus = proof_focus.filter(|focus| focus.surface_id == surface.id);
        match build_surface_scene_with_focus(
            &property,
            society_name,
            entity_refs.clone(),
            &serving_bundle,
            surface,
            surface_focus,
        ) {
            Some(scene) => scenes.push(scene),
            None => missing.push(SurfaceSceneMissing {
                surface_id: surface_id.clone(),
                reason: "surface_scene_empty".to_string(),
            }),
        }
    }
    Ok(PropertySurfacesResponse {
        contract_version: crate::surfaces::SURFACE_SCENE_CONTRACT_VERSION,
        property_id: property.id,
        scenes,
        missing,
    })
}

fn parse_surface_ids(ids: Option<&str>) -> Result<Vec<String>, (StatusCode, Json<ErrorResponse>)> {
    let ids = ids.unwrap_or("around_this_home");
    validate_surface_ids(
        ids.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
    )
    .map_err(route_error)
}

fn parse_focus(
    focus: Option<&str>,
) -> Result<Option<ProofFocus>, (StatusCode, Json<ErrorResponse>)> {
    let Some(focus) = focus.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    serde_json::from_str::<ProofFocus>(focus)
        .map(Some)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_focus"))
}

fn validate_surface_ids(surface_ids: Vec<String>) -> Result<Vec<String>, SurfaceRouteError> {
    if surface_ids.is_empty() {
        return Err(SurfaceRouteError::bad_request("surface_ids_required"));
    }
    if surface_ids.len() > MAX_SURFACE_IDS {
        return Err(SurfaceRouteError::bad_request(
            "too_many_surface_ids_requested",
        ));
    }
    let mut deduped = Vec::new();
    for surface_id in surface_ids {
        if !deduped.iter().any(|existing| existing == &surface_id) {
            deduped.push(surface_id);
        }
    }
    Ok(deduped)
}

#[derive(Debug)]
struct SurfaceRouteError {
    status: StatusCode,
    message: String,
    reason_codes: Vec<String>,
}

impl SurfaceRouteError {
    fn status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            reason_codes: Vec::new(),
        }
    }

    fn not_ready(property: &crate::models::Property) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: "property_not_ready".to_string(),
            reason_codes: property
                .buyer_eligibility
                .decision(crate::buyer_eligibility::DETAIL_SURFACE)
                .map(|decision| decision.reason_codes.clone())
                .unwrap_or_default(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::status(StatusCode::BAD_REQUEST, message)
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::status(StatusCode::NOT_FOUND, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::status(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

fn route_error(err: SurfaceRouteError) -> (StatusCode, Json<ErrorResponse>) {
    (
        err.status,
        Json(ErrorResponse {
            error: err.message,
            reason_codes: err.reason_codes,
        }),
    )
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
            reason_codes: Vec::new(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_surface_ids_defaults_to_around_this_home() {
        let ids = parse_surface_ids(None).expect("default surface ids");
        assert_eq!(ids, vec!["around_this_home"]);
    }

    #[test]
    fn validate_surface_ids_dedupes_in_request_order() {
        let ids = validate_surface_ids(vec![
            "around_this_home".to_string(),
            "water_context".to_string(),
            "around_this_home".to_string(),
        ])
        .expect("valid surface ids");
        assert_eq!(ids, vec!["around_this_home", "water_context"]);
    }

    #[test]
    fn surface_response_envelopes_serialize_camel_case_contract() {
        let response = SurfaceBatchResponse {
            contract_version: crate::surfaces::SURFACE_SCENE_CONTRACT_VERSION,
            items: vec![PropertySurfacesResponse {
                contract_version: crate::surfaces::SURFACE_SCENE_CONTRACT_VERSION,
                property_id: "property:test".to_string(),
                scenes: Vec::new(),
                missing: vec![SurfaceSceneMissing {
                    surface_id: "water_context".to_string(),
                    reason: "surface_scene_empty".to_string(),
                }],
            }],
        };
        let json = serde_json::to_value(response).expect("surface batch response serializes");
        assert_eq!(json["contractVersion"], 1);
        assert_eq!(json["items"][0]["propertyId"], "property:test");
        assert_eq!(json["items"][0]["missing"][0]["surfaceId"], "water_context");
        assert_eq!(
            json["items"][0]["missing"][0]["reason"],
            "surface_scene_empty"
        );
    }
}
