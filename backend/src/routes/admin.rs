//! Admin inspection and atomic reload of the catalog-selected serving bundle.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::data_loader;
use crate::security::require_admin;
use crate::serving::LoadedServingBundle;
use crate::state::AppState;

pub async fn data_health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_admin(&headers) {
        return error.into_response();
    }

    let snapshot = state.search_runtime.load();
    Json(serde_json::json!({
        "status": "ok",
        "serving_bundle": serving_bundle_summary(&snapshot.bundle),
    }))
    .into_response()
}

pub async fn reload_serving_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_admin(&headers) {
        return error.into_response();
    }

    let project_root = state.project_root.clone();
    let loaded = state
        .execution
        .run_internal(async move { data_loader::load_serving_bundle(&project_root).await })
        .await;
    let bundle = match loaded {
        Ok(Ok(bundle)) => bundle,
        Ok(Err(error)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response()
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("internal reload task failed: {error}")
                })),
            )
                .into_response()
        }
    };

    let snapshot = data_loader::runtime_snapshot_from_serving_bundle(bundle);
    let summary = serving_bundle_summary(&snapshot.bundle);
    let mut properties = state.properties.write().await;
    let mut societies = state.societies.write().await;
    let mut areas = state.areas.write().await;
    let mut search_index = state.search_index.write().await;

    *properties = snapshot.properties.to_vec();
    *societies = snapshot.societies.to_vec();
    *areas = snapshot.areas.to_vec();
    *search_index = snapshot.search_index.clone();
    state.recommendation_cache.write().await.clear();
    *state.property_catalog_cache.lock().await = None;
    state.search_cache.clear().await;
    state.search_runtime.store(Arc::new(snapshot));

    Json(serde_json::json!({
        "status": "reloaded",
        "serving_bundle": summary,
    }))
    .into_response()
}

#[derive(Debug, Serialize)]
struct ServingBundleSummary {
    bundle_version: String,
    entity_count: u64,
    fact_count: u64,
    search_metadata_count: u64,
}

fn serving_bundle_summary(bundle: &LoadedServingBundle) -> ServingBundleSummary {
    ServingBundleSummary {
        bundle_version: bundle.manifest.bundle_version.clone(),
        entity_count: bundle.manifest.entity_count,
        fact_count: bundle.manifest.fact_count,
        search_metadata_count: bundle.manifest.search_metadata_count,
    }
}
