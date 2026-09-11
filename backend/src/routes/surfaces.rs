use std::sync::Arc;

use axum::extract::{Json as RequestJson, Query};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::dag_config::ui_surfaces_config;
use crate::routes::enrichment::kg_entity_refs_for_property;
use crate::search::proof::{
    resolve_proof_token, resolved_proof_focus, ProofResolutionError, ResolvedProofFocus,
};
use crate::security::security_tuning;
use crate::state::AppState;
use crate::surfaces::{build_surface_scene_with_focus, ProofFocusStatus, SurfaceSceneResponse};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceListQuery {
    pub ids: Option<String>,
    pub proof_token: Option<String>,
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
    let runtime = state.search_runtime.load_full();
    let focus = resolve_surface_focus(&runtime, &property_id, query.proof_token.as_deref())
        .map_err(route_error)?;
    let response = build_property_surfaces_response(
        &state,
        runtime,
        &property_id,
        std::slice::from_ref(&surface_id),
        &focus,
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
    let runtime = state.search_runtime.load_full();
    let focus = resolve_surface_focus(&runtime, &property_id, query.proof_token.as_deref())
        .map_err(route_error)?;
    build_property_surfaces_response(&state, runtime, &property_id, &surface_ids, &focus)
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
    if request.property_ids.len() > security_tuning().surface_requests.batch_property_limit {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "too_many_property_ids_requested",
        ));
    }
    let surface_ids = validate_surface_ids(request.surface_ids).map_err(route_error)?;
    let runtime = state.search_runtime.load_full();
    let mut items = Vec::new();
    for property_id in request.property_ids {
        items.push(
            build_property_surfaces_response(
                &state,
                runtime.clone(),
                &property_id,
                &surface_ids,
                &SurfaceFocusOutcome::default(),
            )
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
    runtime: Arc<crate::state::SearchRuntimeSnapshot>,
    property_id: &str,
    surface_ids: &[String],
    proof_focus: &SurfaceFocusOutcome,
) -> Result<PropertySurfacesResponse, SurfaceRouteError> {
    if property_id.trim().is_empty()
        || property_id.len() > security_tuning().surface_requests.max_property_id_bytes
    {
        return Err(SurfaceRouteError::bad_request("invalid_property_id"));
    }
    let property = runtime
        .property_by_id
        .get(property_id)
        .and_then(|index| runtime.properties.get(*index))
        .cloned()
        .ok_or_else(|| SurfaceRouteError::not_found("property_not_found"))?;

    let config = ui_surfaces_config()
        .map_err(|err| SurfaceRouteError::internal(format!("surface_config_invalid: {err}")))?;

    let graph = state.knowledge.read().await;
    let entity_refs = kg_entity_refs_for_property(&property, &graph);
    drop(graph);
    let society_name = runtime
        .societies
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
        let surface_focus = proof_focus
            .focus
            .as_ref()
            .filter(|focus| focus.surface_id == surface.id);
        match build_surface_scene_with_focus(
            &property,
            society_name,
            entity_refs.clone(),
            &runtime.bundle,
            surface,
            surface_focus,
        ) {
            Some(mut scene) => {
                if proof_focus.status != ProofFocusStatus::Applied {
                    scene.proof_focus_status = proof_focus.status;
                    scene.proof_focus_message.clone_from(&proof_focus.message);
                } else if surface_focus.is_some() && scene.proof_focus.is_some() {
                    scene.proof_focus_status = ProofFocusStatus::Applied;
                } else if surface_focus.is_some() || proof_focus.focus.is_none() {
                    scene.proof_focus_status = ProofFocusStatus::Unavailable;
                    scene.proof_focus_message =
                        Some(config.proof_focus_messages.unavailable.clone());
                }
                scenes.push(scene);
            }
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

fn resolve_surface_focus(
    runtime: &crate::state::SearchRuntimeSnapshot,
    property_id: &str,
    proof_token: Option<&str>,
) -> Result<SurfaceFocusOutcome, SurfaceRouteError> {
    let Some(proof_token) = proof_token.filter(|value| !value.trim().is_empty()) else {
        return Ok(SurfaceFocusOutcome::default());
    };
    match resolve_proof_token(runtime, proof_token, Some(property_id)) {
        Ok(resolution) => Ok(SurfaceFocusOutcome {
            focus: resolved_proof_focus(&resolution),
            status: ProofFocusStatus::Applied,
            message: None,
        }),
        Err(ProofResolutionError::InvalidToken) => Err(SurfaceRouteError::bad_request(
            ProofResolutionError::InvalidToken.code(),
        )),
        Err(error) => {
            let messages = &ui_surfaces_config()
                .map_err(|err| {
                    SurfaceRouteError::internal(format!("surface_config_invalid: {err}"))
                })?
                .proof_focus_messages;
            let (status, message) = match error {
                ProofResolutionError::StaleSnapshot => {
                    (ProofFocusStatus::Stale, messages.stale.clone())
                }
                ProofResolutionError::MissingEvidence => {
                    (ProofFocusStatus::Retired, messages.retired.clone())
                }
                ProofResolutionError::WrongProperty
                | ProofResolutionError::WrongSubject
                | ProofResolutionError::DestinationMismatch => {
                    (ProofFocusStatus::Mismatch, messages.mismatch.clone())
                }
                ProofResolutionError::InvalidToken => unreachable!(),
            };
            Ok(SurfaceFocusOutcome {
                focus: None,
                status,
                message: Some(message),
            })
        }
    }
}

#[derive(Debug)]
struct SurfaceFocusOutcome {
    focus: Option<ResolvedProofFocus>,
    status: ProofFocusStatus,
    message: Option<String>,
}

impl Default for SurfaceFocusOutcome {
    fn default() -> Self {
        Self {
            focus: None,
            status: ProofFocusStatus::NotRequested,
            message: None,
        }
    }
}

fn validate_surface_ids(surface_ids: Vec<String>) -> Result<Vec<String>, SurfaceRouteError> {
    if surface_ids.is_empty() {
        return Err(SurfaceRouteError::bad_request("surface_ids_required"));
    }
    let tuning = &security_tuning().surface_requests;
    if surface_ids.len() > tuning.surface_id_limit {
        return Err(SurfaceRouteError::bad_request(
            "too_many_surface_ids_requested",
        ));
    }
    let mut deduped = Vec::new();
    for surface_id in surface_ids {
        if surface_id.trim().is_empty() || surface_id.len() > tuning.max_surface_id_bytes {
            return Err(SurfaceRouteError::bad_request("invalid_surface_id"));
        }
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
}

impl SurfaceRouteError {
    fn status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
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
    error(err.status, &err.message)
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
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
