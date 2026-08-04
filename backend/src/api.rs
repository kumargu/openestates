use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::recommendations::RECOMMENDATION_ENGINE_VERSION;
use crate::routes;
use crate::scoring::scoring_policy;
use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
    process_started_at: String,
    scoring_policy_version: u32,
    recommendation_engine_version: &'static str,
    serving_bundle_version: Option<String>,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let serving_bundle_version = state
        .serving_bundle
        .read()
        .await
        .as_ref()
        .map(|bundle| bundle.manifest.bundle_version.clone());
    Json(HealthResponse {
        service: "openestates-api",
        status: "ok",
        process_started_at: state.process_started_at.to_rfc3339(),
        scoring_policy_version: scoring_policy().version,
        recommendation_engine_version: RECOMMENDATION_ENGINE_VERSION,
        serving_bundle_version,
    })
}

/// Construct the production Axum application. Integration tests use this same
/// builder so route extraction, query parsing, state, and response shaping are
/// exercised without binding a TCP port.
pub fn build_app_router(state: Arc<AppState>, project_root: &Path) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(health))
        .route("/api/health", get(health))
        .nest_service(
            "/media",
            ServeDir::new(project_root.join("data/lake/media")),
        )
        .route("/api/properties", get(routes::properties::list_properties))
        .route(
            "/api/properties/{id}",
            get(routes::properties::get_property),
        )
        .route(
            "/api/properties/{id}/evidence",
            get(routes::properties::get_property_evidence),
        )
        .route(
            "/api/properties/{id}/rera",
            get(routes::properties::get_property_rera),
        )
        .route(
            "/api/properties/{id}/recommendations",
            get(routes::properties::get_property_recommendations),
        )
        .route(
            "/api/properties/{id}/surfaces/{surface_id}",
            get(routes::surfaces::get_property_surface),
        )
        .route(
            "/api/properties/{id}/surfaces",
            get(routes::surfaces::list_property_surfaces),
        )
        .route(
            "/api/properties/surfaces/batch",
            post(routes::surfaces::get_property_surfaces_batch),
        )
        .route(
            "/api/properties/evidence/batch",
            post(routes::properties::get_property_evidence_batch),
        )
        .route("/api/areas", get(routes::areas::list_areas))
        .route("/api/areas/tracker", get(routes::areas::area_tracker))
        .route("/api/areas/{id}", get(routes::areas::get_area))
        .route("/api/shortlist", get(routes::shortlist::get_shortlist))
        .route("/api/discovery", get(routes::discovery::discovery_home))
        .route("/api/search", get(routes::search::search_properties))
        .route(
            "/api/societies/search",
            get(routes::societies::search_societies),
        )
        .route("/api/societies/{slug}", get(routes::societies::get_society))
        .route("/api/interests", post(routes::interests::express_interest))
        .route(
            "/api/properties/{id}/interests/count",
            get(routes::interests::get_interest_count),
        )
        .route("/api/sitemap.xml", get(routes::sitemap::sitemap_xml))
        .route("/api/admin/data-health", get(routes::admin::data_health))
        .route(
            "/api/admin/serving-bundle/reload",
            post(routes::admin::reload_serving_bundle),
        )
        .route(
            "/api/admin/asset-runs/current",
            get(routes::admin::current_asset_run),
        )
        .route(
            "/api/admin/asset-runs",
            post(routes::admin::trigger_asset_run),
        )
        .layer(cors)
        .with_state(state)
}
