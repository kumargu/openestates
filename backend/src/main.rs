mod data_loader;
mod models;
mod routes;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
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
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside project root")
        .join("data")
        .join("seed");

    let state = Arc::new(data_loader::load_seed_data(&data_dir));

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
        .route("/api/areas", get(routes::areas::list_areas))
        .route("/api/areas/{id}", get(routes::areas::get_area))
        .route("/api/shortlist", get(routes::shortlist::get_shortlist))
        .layer(cors)
        .with_state(state);

    println!("OpenEstates API listening on http://localhost:4000");
    println!("Routes:");
    println!("  GET /");
    println!("  GET /api/health");
    println!("  GET /api/properties");
    println!("  GET /api/properties/{{id}}");
    println!("  GET /api/areas");
    println!("  GET /api/areas/{{id}}");
    println!("  GET /api/shortlist");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000")
        .await
        .expect("Failed to bind port 4000");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
