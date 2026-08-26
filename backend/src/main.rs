use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use backend::data_loader;
use backend::security::{security_tuning, ExecutionLanes};

fn main() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside project root")
        .to_path_buf();
    let loaded_env_file = backend::local_env::load_project_env(&project_root)
        .unwrap_or_else(|error| panic!("environment configuration failed: {error}"));
    backend::dag_config::set_project_dag_root(&project_root);
    let public_site_origin = backend::routes::sitemap::configured_site_origin()
        .unwrap_or_else(|error| panic!("public site configuration failed: {error}"));

    let internal_runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("openestates-internal")
        .worker_threads(security_tuning().runtime.internal_worker_threads)
        .enable_all()
        .build()
        .expect("internal runtime must start");
    let customer_compute_limit = security_tuning().runtime.customer_compute_limit;
    let customer_compute_runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("openestates-customer-compute")
        .worker_threads(security_tuning().runtime.customer_compute_worker_threads)
        .max_blocking_threads(customer_compute_limit)
        .enable_all()
        .build()
        .expect("customer compute runtime must start");
    let customer_http_runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("openestates-http")
        .worker_threads(security_tuning().runtime.http_worker_threads)
        .enable_all()
        .build()
        .expect("customer HTTP runtime must start");
    let execution = ExecutionLanes::new(
        internal_runtime.handle().clone(),
        customer_compute_runtime.handle().clone(),
        customer_compute_limit,
    );
    let state = Arc::new(
        internal_runtime.block_on(data_loader::load_app_state_with_execution(
            &project_root,
            execution,
        )),
    );
    let retention_root = project_root.clone();
    let active_cache_dir = state.search_runtime.load().bundle.cache_dir.clone();
    let retention_execution = state.execution.clone();
    let bind_address =
        std::env::var("OPENESTATES_API_ADDR").unwrap_or_else(|_| "127.0.0.1:4000".to_string());

    let app = backend::api::build_app_router(state, &project_root);

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
    println!("Public site origin: {public_site_origin}");
    if let Some(path) = loaded_env_file {
        println!("Environment file: {}", path.display());
    }
    println!("  POST /api/admin/serving-bundle/reload");
    println!("  GET  /api/admin/asset-runs/current");
    println!("  POST /api/admin/asset-runs");

    customer_http_runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&bind_address)
            .await
            .unwrap_or_else(|error| panic!("Failed to bind {bind_address}: {error}"));
        println!("OpenEstates API listening on http://{bind_address}");
        retention_execution.spawn_internal_blocking(move || {
            backend::security::prune_rebuildable_serving_cache(&retention_root, &active_cache_dir);
        });

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("Server error");
    });
}
