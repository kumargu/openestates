mod cache;
mod data_loader;
mod discovery;
mod enrichment_queue;
mod knowledge;
mod models;
mod routes;
mod scoring;
mod search;
mod state;
mod storage;
mod utils;

use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post, put};
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
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside project root")
        .to_path_buf();

    let state = Arc::new(data_loader::load_app_state(&project_root).await);

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
        .route("/api/search", get(routes::search::search_properties))
        .route(
            "/api/societies/search",
            get(routes::societies::search_societies),
        )
        .route(
            "/api/societies/{slug}",
            get(routes::societies::get_society),
        )
        // Knowledge graph endpoints
        .route("/api/knowledge/stats", get(routes::knowledge::graph_stats))
        .route("/api/knowledge/nodes", get(routes::knowledge::list_nodes))
        .route(
            "/api/knowledge/nodes/{id}",
            get(routes::knowledge::get_node),
        )
        .route(
            "/api/knowledge/nodes/{id}/neighbors",
            get(routes::knowledge::get_neighbors),
        )
        .route(
            "/api/knowledge/nodes/{id}/facts",
            post(routes::knowledge::add_facts),
        )
        .route(
            "/api/knowledge/enrichment/queue",
            get(routes::knowledge::enrichment_queue),
        )
        .route(
            "/api/knowledge/search-log",
            get(routes::knowledge::search_log),
        )
        // Graph query endpoints
        .route("/api/knowledge/path", get(routes::knowledge::find_path))
        .route(
            "/api/knowledge/nodes/{id}/subgraph",
            get(routes::knowledge::get_subgraph),
        )
        .route(
            "/api/knowledge/compare",
            get(routes::knowledge::compare_nodes),
        )
        .route(
            "/api/knowledge/coverage",
            get(routes::knowledge::fact_coverage),
        )
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
        .route(
            "/api/admin/reload-knowledge",
            post(routes::admin::reload_knowledge),
        )
        // Embedding / similarity endpoints
        .route(
            "/api/knowledge/nodes/{id}/similar",
            get(routes::knowledge::similar_nodes),
        )
        .route(
            "/api/knowledge/embeddings/stats",
            get(routes::knowledge::embedding_stats),
        )
        // Marketplace — sellers + bids
        .route("/api/sellers/register", post(routes::marketplace::register_seller))
        .route("/api/sellers/{id}", get(routes::marketplace::get_seller))
        .route("/api/sellers/{id}/listings", get(routes::marketplace::get_seller_listings))
        .route("/api/properties/{id}/bids", post(routes::marketplace::place_bid))
        .route("/api/properties/{id}/bids", get(routes::marketplace::list_bids))
        .route("/api/properties/{id}/bid-stats", get(routes::marketplace::get_bid_stats))
        .route(
            "/api/properties/{id}/bids/{bid_id}/status",
            put(routes::marketplace::update_bid_status),
        )
        // Debug endpoints — gated by ENABLE_DEBUG_API=true
        .route("/api/debug/score", get(routes::debug::score_debug))
        .layer(cors)
        .with_state(state);

    println!("OpenEstates API listening on http://localhost:4000");
    println!("Routes:");
    println!("  GET /api/health");
    println!("  GET /api/properties | /api/properties/{{id}}");
    println!("  GET /api/areas | /api/areas/{{id}}");
    println!("  GET /api/search?q=...");
    println!("  GET /api/societies/search?q=... | /api/societies/{{slug}}");
    println!("  GET /api/knowledge/stats");
    println!("  GET /api/knowledge/nodes?type=... | /api/knowledge/nodes/{{id}}");
    println!("  GET /api/knowledge/nodes/{{id}}/neighbors");
    println!("  GET /api/knowledge/enrichment/queue");
    println!("  GET /api/knowledge/search-log");
    println!("  GET /api/knowledge/path?from=...&to=...");
    println!("  GET /api/knowledge/nodes/{{id}}/subgraph?depth=2");
    println!("  GET /api/knowledge/compare?a=...&b=...");
    println!("  GET /api/knowledge/coverage?type=society");
    println!("  GET /api/knowledge/nodes/{{id}}/similar?top_n=5");
    println!("  GET /api/knowledge/embeddings/stats");
    println!("  GET /api/sellers | /api/sellers/{{id}} | /api/sellers/{{id}}/dashboard");
    println!("  POST /api/interests");
    println!("  GET /api/properties/{{id}}/interests/count");
    println!("  POST /api/registrations | GET /api/registrations/{{id}} | PUT /api/registrations/{{id}}/step/{{n}} | POST /api/registrations/{{id}}/publish");
    println!("  POST /api/claims");
    println!("  GET  /api/sitemap.xml");
    println!("  POST /api/admin/reload-knowledge");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4000")
        .await
        .expect("Failed to bind port 4000");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
