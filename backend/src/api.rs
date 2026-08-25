use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use crate::lake::{LakeStore, LakeStoreLocation};
use crate::recommendations::RECOMMENDATION_ENGINE_VERSION;
use crate::routes;
use crate::scoring::scoring_policy;
use crate::security::{MediaStreamAdmission, SecurityPolicy};
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
    let lake = LakeStoreLocation::from_env(project_root)
        .and_then(|location| location.open())
        .unwrap_or_else(|error| panic!("media lake startup contract failed: {error}"));
    build_app_router_with_lake(state, lake)
}

pub fn build_app_router_with_lake(state: Arc<AppState>, lake: LakeStore) -> Router {
    let security = SecurityPolicy::from_env(&state.execution);
    let read_routes = security.protect_public_reads(
        Router::new()
            .route("/", get(health))
            .route("/api/health", get(health))
            .route("/media/{*path}", get(routes::media::get_media))
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
            .route("/api/areas", get(routes::areas::list_areas))
            .route("/api/areas/tracker", get(routes::areas::area_tracker))
            .route("/api/areas/{id}", get(routes::areas::get_area))
            .route("/api/shortlist", get(routes::shortlist::get_shortlist))
            .route("/api/discovery", get(routes::discovery::discovery_home))
            .route(
                "/api/societies/search",
                get(routes::societies::search_societies),
            )
            .route("/api/societies/{slug}", get(routes::societies::get_society))
            .route(
                "/api/properties/{id}/interests/count",
                get(routes::interests::get_interest_count),
            )
            .route("/api/sitemap.xml", get(routes::sitemap::sitemap_xml)),
    );

    let catalog_routes = security.protect_catalog(
        Router::new().route("/api/properties", get(routes::properties::list_properties)),
    );

    let search_routes = Router::new().route(
        "/api/search",
        security.protect_search(get(routes::search::search_properties)),
    );

    let batch_routes = security.protect_batch_reads(
        Router::new()
            .route(
                "/api/properties/surfaces/batch",
                post(routes::surfaces::get_property_surfaces_batch),
            )
            .route(
                "/api/properties/evidence/batch",
                post(routes::properties::get_property_evidence_batch),
            ),
    );

    let interest_routes = security.protect_interest_writes(
        Router::new().route("/api/interests", post(routes::interests::express_interest)),
    );

    let admin_routes = security.protect_admin(
        Router::new()
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
            ),
    );

    security
        .protect_application(
            Router::new()
                .merge(read_routes)
                .merge(catalog_routes)
                .merge(search_routes)
                .merge(batch_routes)
                .merge(interest_routes)
                .merge(admin_routes)
                .fallback(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .layer(Extension(lake))
        .layer(Extension(MediaStreamAdmission::from_env()))
        .with_state(state)
}
