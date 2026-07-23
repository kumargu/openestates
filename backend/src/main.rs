use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post, put};
use axum::{Json, Router};
use backend::{data_loader, routes};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "openestates-api",
        status: "ok",
    })
}

#[tokio::main]
async fn main() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside project root")
        .to_path_buf();
    backend::local_env::load_project_env(&project_root);
    backend::dag_config::set_project_dag_root(&project_root);

    let state = Arc::new(data_loader::load_app_state(&project_root).await);
    let bind_address =
        std::env::var("OPENESTATES_API_ADDR").unwrap_or_else(|_| "0.0.0.0:4000".to_string());

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(health))
        .route("/api/health", get(health))
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
        // Seller endpoints
        .route("/api/sellers", get(routes::sellers::list_sellers))
        .route("/api/sellers/{id}", get(routes::sellers::get_seller))
        .route(
            "/api/sellers/{id}/dashboard",
            get(routes::sellers::get_seller_dashboard),
        )
        // Interest endpoints
        .route("/api/interests", post(routes::interests::express_interest))
        .route(
            "/api/properties/{id}/interests/count",
            get(routes::interests::get_interest_count),
        )
        // Registration endpoints
        .route(
            "/api/registrations",
            post(routes::registration::create_registration),
        )
        .route(
            "/api/registrations/{id}",
            get(routes::registration::get_registration),
        )
        .route(
            "/api/registrations/{id}/step/{step_num}",
            put(routes::registration::update_registration_step),
        )
        .route(
            "/api/registrations/{id}/publish",
            post(routes::registration::publish_registration),
        )
        // Claims endpoint
        .route("/api/claims", post(routes::claims::submit_claim))
        // Sitemap endpoint
        .route("/api/sitemap.xml", get(routes::sitemap::sitemap_xml))
        // Admin endpoints
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
        .with_state(state);

    println!("OpenEstates API listening on http://{bind_address}");
    println!("Routes:");
    println!("  GET /api/health");
    println!("  GET /api/properties | /api/properties/{{id}} | /api/properties/{{id}}/evidence");
    println!("  POST /api/properties/evidence/batch");
    println!("  GET /api/areas | /api/areas/tracker | /api/areas/{{id}}");
    println!("  GET /api/discovery");
    println!("  GET /api/search?q=...");
    println!("  GET /api/societies/search?q=... | /api/societies/{{slug}}");
    println!("  GET /api/sellers | /api/sellers/{{id}} | /api/sellers/{{id}}/dashboard");
    println!("  POST /api/interests");
    println!("  GET /api/properties/{{id}}/interests/count");
    println!("  POST /api/registrations | GET /api/registrations/{{id}} | PUT /api/registrations/{{id}}/step/{{n}} | POST /api/registrations/{{id}}/publish");
    println!("  POST /api/claims");
    println!("  GET  /api/sitemap.xml");
    println!("  POST /api/admin/serving-bundle/reload");
    println!("  GET  /api/admin/asset-runs/current");
    println!("  POST /api/admin/asset-runs");

    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .unwrap_or_else(|error| panic!("Failed to bind {bind_address}: {error}"));

    axum::serve(listener, app).await.expect("Server error");
}
