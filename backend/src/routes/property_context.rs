use super::properties::ErrorResponse;
use crate::{
    property_context::{build_property_context, PropertyContext},
    state::{AppState, SearchRuntimeSnapshot},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextQuery {
    pub snapshot_identity: Option<String>,
    pub proof_token: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextBatchRequest {
    pub property_ids: Vec<String>,
    pub snapshot_identity: Option<String>,
}
#[derive(schemars::JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBatchResponse {
    pub contract_version: u32,
    pub snapshot_identity: String,
    pub items: Vec<PropertyContext>,
}
type RouteError = (StatusCode, Json<ErrorResponse>);
fn error(status: StatusCode, code: &str) -> RouteError {
    (
        status,
        Json(ErrorResponse {
            error: code.to_string(),
        }),
    )
}
fn check_snapshot(
    runtime: &SearchRuntimeSnapshot,
    identity: Option<&str>,
) -> Result<(), RouteError> {
    if identity
        .is_some_and(|identity| identity != runtime.bundle.manifest.proof_snapshot_identity())
    {
        return Err(error(StatusCode::CONFLICT, "stale_snapshot"));
    }
    Ok(())
}
fn context(
    runtime: &SearchRuntimeSnapshot,
    id: &str,
    proof_token: Option<&str>,
) -> Result<PropertyContext, RouteError> {
    if id.trim().is_empty()
        || id.len()
            > crate::security::security_tuning()
                .context_requests
                .max_property_id_bytes
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_property_id"));
    }
    let property = runtime
        .property_by_id
        .get(id)
        .and_then(|index| runtime.properties.get(*index))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "property_not_found"))?;
    let proof = proof_token
        .map(|token| crate::search::proof::resolve_proof_token(runtime, token, Some(id)))
        .transpose()
        .map_err(|failure| {
            error(
                match failure {
                    crate::search::proof::ProofResolutionError::StaleSnapshot => {
                        StatusCode::CONFLICT
                    }
                    crate::search::proof::ProofResolutionError::MissingEvidence => {
                        StatusCode::NOT_FOUND
                    }
                    _ => StatusCode::BAD_REQUEST,
                },
                failure.code(),
            )
        })?;
    Ok(build_property_context(
        property,
        runtime.entity_refs_for_property(property),
        &runtime.bundle,
        &runtime.context_lookup,
        proof,
    ))
}
pub async fn get_property_context(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<PropertyContext>, RouteError> {
    let runtime = state.search_runtime.load_full();
    check_snapshot(&runtime, query.snapshot_identity.as_deref())?;
    context(&runtime, &id, query.proof_token.as_deref()).map(Json)
}
pub async fn get_property_context_batch(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ContextBatchRequest>,
) -> Result<Json<ContextBatchResponse>, RouteError> {
    let runtime = state.search_runtime.load_full();
    check_snapshot(&runtime, request.snapshot_identity.as_deref())?;
    if request.property_ids.is_empty()
        || request.property_ids.len()
            > crate::security::security_tuning()
                .context_requests
                .batch_property_limit
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_property_ids"));
    }
    let items = request
        .property_ids
        .iter()
        .map(|id| context(&runtime, id, None))
        .collect::<Result<_, _>>()?;
    Ok(Json(ContextBatchResponse {
        contract_version: 1,
        snapshot_identity: runtime
            .bundle
            .manifest
            .proof_snapshot_identity()
            .to_string(),
        items,
    }))
}
