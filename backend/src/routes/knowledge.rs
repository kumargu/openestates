//! Knowledge graph API endpoints.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::knowledge::{GraphStats, SourcedFact, store as kg_store};
use crate::knowledge::node::NodeType;
use crate::state::AppState;

/// GET /api/knowledge/stats — graph overview
pub async fn graph_stats(State(state): State<Arc<AppState>>) -> Json<GraphStats> {
    let graph = state.knowledge.read().await;
    Json(graph.stats())
}

#[derive(Deserialize)]
pub struct NodeQuery {
    #[serde(rename = "type")]
    pub node_type: Option<String>,
}

/// Lightweight node summary for list endpoints.
#[derive(Serialize)]
pub struct NodeSummary {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub fact_count: usize,
    pub fact_keys: Vec<String>,
}

/// GET /api/knowledge/nodes?type=society — list nodes, optionally filtered by type
pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
    Query(params): Query<NodeQuery>,
) -> Json<Vec<NodeSummary>> {
    let graph = state.knowledge.read().await;

    let nodes: Vec<NodeSummary> = graph
        .nodes
        .values()
        .filter(|n| {
            if let Some(ref t) = params.node_type {
                format!("{}", n.node_type) == *t
            } else {
                true
            }
        })
        .map(|n| NodeSummary {
            id: n.id.clone(),
            name: n.name.clone(),
            node_type: format!("{}", n.node_type),
            fact_count: n.facts.len(),
            fact_keys: n.fact_keys().into_iter().map(String::from).collect(),
        })
        .collect();

    Json(nodes)
}

/// Full node detail with all facts and edges.
#[derive(Serialize)]
pub struct NodeDetail {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub facts: Vec<FactSummary>,
    pub edges_from: Vec<EdgeSummary>,
    pub edges_to: Vec<EdgeSummary>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct FactSummary {
    pub key: String,
    pub value: serde_json::Value,
    pub confidence: f32,
    pub source_type: String,
    pub learned_at: String,
}

#[derive(Serialize)]
pub struct EdgeSummary {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub weight: f32,
}

/// GET /api/knowledge/nodes/{id} — full node detail with facts and edges
pub async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<NodeDetail>, StatusCode> {
    let graph = state.knowledge.read().await;

    let node = graph.get_node(&id).ok_or(StatusCode::NOT_FOUND)?;

    let facts: Vec<FactSummary> = node
        .facts
        .iter()
        .map(|f| FactSummary {
            key: f.key.clone(),
            value: serde_json::to_value(&f.value).unwrap_or_default(),
            confidence: f.confidence,
            source_type: format!("{:?}", f.source.source_type),
            learned_at: f.learned_at.to_rfc3339(),
        })
        .collect();

    let edges_from: Vec<EdgeSummary> = graph
        .edges_from(&id)
        .iter()
        .map(|e| EdgeSummary {
            from: e.from.clone(),
            to: e.to.clone(),
            relation: format!("{:?}", e.relation),
            weight: e.weight,
        })
        .collect();

    let edges_to: Vec<EdgeSummary> = graph
        .edges_to(&id)
        .iter()
        .map(|e| EdgeSummary {
            from: e.from.clone(),
            to: e.to.clone(),
            relation: format!("{:?}", e.relation),
            weight: e.weight,
        })
        .collect();

    Ok(Json(NodeDetail {
        id: node.id.clone(),
        name: node.name.clone(),
        node_type: format!("{}", node.node_type),
        facts,
        edges_from,
        edges_to,
        created_at: node.created_at.to_rfc3339(),
        updated_at: node.updated_at.to_rfc3339(),
    }))
}

/// GET /api/knowledge/enrichment/queue — pending enrichment tasks
pub async fn enrichment_queue(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let graph = state.knowledge.read().await;
    let tasks = graph.pending_enrichments();
    Json(serde_json::to_value(tasks).unwrap_or_default())
}

/// GET /api/knowledge/search-log — recent search events
pub async fn search_log(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let graph = state.knowledge.read().await;
    // Return the most recent 50 search events
    let recent: Vec<_> = graph
        .search_log
        .iter()
        .rev()
        .take(50)
        .collect();
    Json(serde_json::to_value(recent).unwrap_or_default())
}

/// GET /api/knowledge/nodes/{id}/neighbors — get neighbors of a node
pub async fn get_neighbors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<NodeSummary>>, StatusCode> {
    let graph = state.knowledge.read().await;

    if graph.get_node(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let neighbors: Vec<NodeSummary> = graph
        .neighbors(&id, None)
        .iter()
        .map(|n| NodeSummary {
            id: n.id.clone(),
            name: n.name.clone(),
            node_type: format!("{}", n.node_type),
            fact_count: n.facts.len(),
            fact_keys: n.fact_keys().into_iter().map(String::from).collect(),
        })
        .collect();

    Ok(Json(neighbors))
}

/// POST /api/knowledge/nodes/{id}/facts — add facts to a node (from Python skills)
///
/// Accepts a JSON array of SourcedFact objects. This is the bridge between
/// Python skill execution and the Rust knowledge graph.
pub async fn add_facts(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(facts): Json<Vec<SourcedFact>>,
) -> Result<Json<FactsAddedResponse>, StatusCode> {
    let mut graph = state.knowledge.write().await;

    if graph.get_node(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let count = facts.len();
    for fact in facts {
        graph.add_fact_to_node(&id, fact);
    }

    // Persist only the modified node (not the entire graph)
    let kg_dir = kg_store::knowledge_dir(&state.project_root);
    if let Some(node) = graph.get_node(&id) {
        if let Err(e) = kg_store::save_node(&kg_dir, node) {
            eprintln!("WARN: Failed to save node {}: {}", id, e);
        }
    }

    Ok(Json(FactsAddedResponse {
        node_id: id,
        facts_added: count,
    }))
}

#[derive(Serialize)]
pub struct FactsAddedResponse {
    pub node_id: String,
    pub facts_added: usize,
}

// ---------------------------------------------------------------------------
// Graph query endpoints (powered by knowledge::query)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PathQuery {
    pub from: String,
    pub to: String,
    pub max_hops: Option<usize>,
}

/// GET /api/knowledge/path?from=...&to=...&max_hops=4 — find shortest path between nodes
pub async fn find_path(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PathQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let graph = state.knowledge.read().await;
    let max_hops = params.max_hops.unwrap_or(4);

    match graph.find_path(&params.from, &params.to, max_hops) {
        Some(path) => Ok(Json(serde_json::to_value(path).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Deserialize)]
pub struct SubgraphQuery {
    pub depth: Option<usize>,
}

/// GET /api/knowledge/nodes/{id}/subgraph?depth=2 — extract subgraph around a node
pub async fn get_subgraph(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<SubgraphQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let graph = state.knowledge.read().await;
    let depth = params.depth.unwrap_or(2);

    match graph.subgraph(&id, depth) {
        Some(sg) => Ok(Json(serde_json::to_value(sg).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Deserialize)]
pub struct CompareQuery {
    pub a: String,
    pub b: String,
}

/// GET /api/knowledge/compare?a=...&b=... — compare two nodes side-by-side
pub async fn compare_nodes(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CompareQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let graph = state.knowledge.read().await;

    match graph.compare_nodes(&params.a, &params.b) {
        Some(comparison) => Ok(Json(serde_json::to_value(comparison).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Deserialize)]
pub struct CoverageQuery {
    #[serde(rename = "type")]
    pub node_type: String,
}

/// GET /api/knowledge/coverage?type=society — fact coverage report by node type
pub async fn fact_coverage(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CoverageQuery>,
) -> Json<serde_json::Value> {
    let graph = state.knowledge.read().await;

    let node_type = match params.node_type.as_str() {
        "property" => NodeType::Property,
        "society" => NodeType::Society,
        "area" => NodeType::Area,
        "builder" => NodeType::Builder,
        _ => return Json(serde_json::json!({"error": "unknown node type"})),
    };

    let coverage = graph.fact_coverage(node_type);

    // Convert to serializable format: { "fact_key": { "has": N, "total": M, "pct": P } }
    let result: serde_json::Map<String, serde_json::Value> = coverage
        .into_iter()
        .map(|(key, (has, total))| {
            let pct = if total > 0 {
                (has as f64 / total as f64 * 100.0).round()
            } else {
                0.0
            };
            (
                key,
                serde_json::json!({ "has": has, "total": total, "pct": pct }),
            )
        })
        .collect();

    Json(serde_json::Value::Object(result))
}

#[derive(Deserialize)]
pub struct SimilarQuery {
    pub top_n: Option<usize>,
    #[serde(rename = "type")]
    pub node_type: Option<String>,
}

/// GET /api/knowledge/nodes/{id}/similar?top_n=5&type=society — find similar entities by embedding
pub async fn similar_nodes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<SimilarQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let graph = state.knowledge.read().await;

    if graph.get_node(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let top_n = params.top_n.unwrap_or(5);
    let type_filter = params.node_type.as_deref().and_then(|t| match t {
        "property" => Some(NodeType::Property),
        "society" => Some(NodeType::Society),
        "area" => Some(NodeType::Area),
        "builder" => Some(NodeType::Builder),
        _ => None,
    });

    let similar = graph.similar_to(&id, top_n, type_filter);
    Ok(Json(serde_json::to_value(similar).unwrap_or_default()))
}

/// GET /api/knowledge/embeddings/stats — embedding coverage stats
pub async fn embedding_stats(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let graph = state.knowledge.read().await;
    let stats = graph.embedding_stats();

    let result: Vec<serde_json::Value> = stats
        .into_iter()
        .map(|(node_type, has, total)| {
            serde_json::json!({
                "node_type": node_type,
                "with_embedding": has,
                "total": total,
                "pct": if total > 0 { (has as f64 / total as f64 * 100.0).round() } else { 0.0 },
            })
        })
        .collect();

    Json(serde_json::to_value(result).unwrap_or_default())
}
