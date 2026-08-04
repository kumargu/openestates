use std::path::PathBuf;
use std::sync::Arc;

use backend::data_loader;

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

    let app = backend::api::build_app_router(state, &project_root);

    println!("OpenEstates API listening on http://{bind_address}");
    println!("Routes:");
    println!("  GET /api/health");
    println!("  GET /media/*path");
    println!("  GET /api/properties | /api/properties/{{id}} | /api/properties/{{id}}/evidence | /api/properties/{{id}}/rera | /api/properties/{{id}}/recommendations");
    println!(
        "  GET /api/properties/{{id}}/surfaces | /api/properties/{{id}}/surfaces/{{surface_id}}"
    );
    println!("  POST /api/properties/surfaces/batch");
    println!("  POST /api/properties/evidence/batch");
    println!("  GET /api/areas | /api/areas/tracker | /api/areas/{{id}}");
    println!("  GET /api/discovery");
    println!("  GET /api/search?q=...");
    println!("  GET /api/societies/search?q=... | /api/societies/{{slug}}");
    println!("  POST /api/interests");
    println!("  GET /api/properties/{{id}}/interests/count");
    println!("  GET  /api/sitemap.xml");
    println!("  POST /api/admin/serving-bundle/reload");
    println!("  GET  /api/admin/asset-runs/current");
    println!("  POST /api/admin/asset-runs");

    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .unwrap_or_else(|error| panic!("Failed to bind {bind_address}: {error}"));

    axum::serve(listener, app).await.expect("Server error");
}
