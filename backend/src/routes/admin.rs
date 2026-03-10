//! Admin endpoints — operations like hot-reloading the knowledge graph.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use crate::knowledge::store;
use crate::state::AppState;

/// POST /api/admin/reload-knowledge
///
/// Re-reads the knowledge graph from disk (data/knowledge/nodes/) and replaces
/// the in-memory graph. This allows the Python pipeline to write facts to disk
/// and then trigger a reload without restarting the backend.
///
/// Protected by an admin token via the `X-Admin-Token` header. The expected
/// token is read from the `ADMIN_TOKEN` env var (defaults to "dev").
pub async fn reload_knowledge(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Auth check
    let expected_token = std::env::var("ADMIN_TOKEN").unwrap_or_else(|_| "dev".into());
    if let Some(provided) = headers.get("x-admin-token") {
        let provided_str = provided.to_str().unwrap_or("");
        if provided_str != expected_token {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid token"})),
            )
                .into_response();
        }
    }

    // Reload knowledge graph from disk
    let knowledge_dir = store::knowledge_dir(&state.project_root);
    match store::load_graph(&knowledge_dir) {
        Some(new_graph) => {
            let node_count = new_graph.nodes.len();
            let edge_count = new_graph.edges.len();
            let fact_count: usize = new_graph.nodes.values().map(|n| n.facts.len()).sum();

            let mut graph = state.knowledge.write().await;
            *graph = new_graph;

            println!(
                "Knowledge graph hot-reloaded: {} nodes, {} edges, {} facts",
                node_count, edge_count, fact_count
            );

            Json(serde_json::json!({
                "status": "reloaded",
                "nodes": node_count,
                "edges": edge_count,
                "facts": fact_count,
            }))
            .into_response()
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "reload failed: no knowledge graph found on disk"
            })),
        )
            .into_response(),
    }
}
