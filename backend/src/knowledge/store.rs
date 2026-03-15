//! Persistence layer for the knowledge graph.
//!
//! Storage layout under `data/knowledge/`:
//! - `nodes/{type}/{slug}.json`       — one file per entity (atomic writes)
//! - `edges.json`                     — all edges in one file
//! - `search_log/{YYYY}/{MM}/{DD}.jsonl` — daily append-only log
//! - `enrichment/queue.json`          — pending tasks
//!
//! Atomic writes: we write to a `.tmp` file then rename, so a crash mid-write
//! never leaves a corrupt file on disk.

use std::path::{Path, PathBuf};

use crate::knowledge::edge::Edge;
use crate::knowledge::graph::KnowledgeGraph;
use crate::knowledge::node::Node;

/// Get the knowledge directory path from project root.
pub fn knowledge_dir(project_root: &Path) -> PathBuf {
    project_root.join("data").join("knowledge")
}

// ---------------------------------------------------------------------------
// Atomic write helper
// ---------------------------------------------------------------------------

/// Write content to a file atomically: write `.tmp`, then rename.
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-entity node storage
// ---------------------------------------------------------------------------

/// Directory for nodes of a given type: `{kg_dir}/nodes/{type}/`
fn node_dir(knowledge_dir: &Path, node_type: &str) -> PathBuf {
    knowledge_dir.join("nodes").join(node_type)
}

/// File path for a single node: `{kg_dir}/nodes/{type}/{slug}.json`
fn node_path(knowledge_dir: &Path, node_id: &str) -> Option<PathBuf> {
    // Node IDs are "{type}:{slug}", e.g. "society:prestige-lakeside"
    let (node_type, slug) = node_id.split_once(':')?;
    Some(node_dir(knowledge_dir, node_type).join(format!("{}.json", slug)))
}

/// Save a single node to its per-entity file.
pub fn save_node(knowledge_dir: &Path, node: &Node) -> std::io::Result<()> {
    let path = node_path(knowledge_dir, &node.id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid node ID format: {}", node.id),
        )
    })?;

    let content = serde_json::to_string_pretty(node)
        .map_err(std::io::Error::other)?;

    atomic_write(&path, content.as_bytes())
}

/// Load a single node from its per-entity file.
fn load_node(path: &Path) -> Option<Node> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Load all nodes from the per-entity file layout.
fn load_all_nodes(knowledge_dir: &Path) -> Vec<Node> {
    let nodes_dir = knowledge_dir.join("nodes");
    let mut nodes = Vec::new();

    if !nodes_dir.exists() {
        return nodes;
    }

    // Iterate type directories
    let type_dirs = match std::fs::read_dir(&nodes_dir) {
        Ok(dirs) => dirs,
        Err(_) => return nodes,
    };

    for type_entry in type_dirs.flatten() {
        if !type_entry.path().is_dir() {
            continue;
        }

        let files = match std::fs::read_dir(type_entry.path()) {
            Ok(f) => f,
            Err(_) => continue,
        };

        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            if let Some(node) = load_node(&path) {
                nodes.push(node);
            }
        }
    }

    nodes
}

// ---------------------------------------------------------------------------
// Edges storage
// ---------------------------------------------------------------------------

fn edges_path(knowledge_dir: &Path) -> PathBuf {
    knowledge_dir.join("edges.json")
}

/// Save all edges to a single file.
pub fn save_edges(knowledge_dir: &Path, edges: &[Edge]) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(edges)
        .map_err(std::io::Error::other)?;
    atomic_write(&edges_path(knowledge_dir), content.as_bytes())
}

/// Load edges from disk.
fn load_edges(knowledge_dir: &Path) -> Vec<Edge> {
    let path = edges_path(knowledge_dir);
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Full graph load / save
// ---------------------------------------------------------------------------

/// Load the knowledge graph from per-entity files.
/// Falls back to legacy `graph.json` if per-entity files don't exist yet.
pub fn load_graph(knowledge_dir: &Path) -> Option<KnowledgeGraph> {
    let nodes_dir = knowledge_dir.join("nodes");

    if nodes_dir.exists() {
        // New per-entity layout
        let nodes = load_all_nodes(knowledge_dir);
        if nodes.is_empty() {
            return load_legacy_graph(knowledge_dir);
        }
        let edges = load_edges(knowledge_dir);

        let mut graph = KnowledgeGraph::new();
        for node in nodes {
            graph.add_node(node);
        }
        for edge in edges {
            graph.add_edge(edge);
        }
        // Indexes are built by add_node/add_edge, but rebuild to be safe
        graph.rebuild_indexes();
        println!(
            "Loaded knowledge graph: {} nodes, {} edges (per-entity files)",
            graph.nodes.len(),
            graph.edges.len()
        );
        Some(graph)
    } else {
        // Try legacy monolithic graph.json
        load_legacy_graph(knowledge_dir)
    }
}

/// Load from legacy monolithic `graph.json` and migrate to per-entity files.
fn load_legacy_graph(knowledge_dir: &Path) -> Option<KnowledgeGraph> {
    let path = knowledge_dir.join("graph.json");
    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let mut graph: KnowledgeGraph = serde_json::from_str(&content).ok()?;
    graph.rebuild_indexes();

    println!(
        "Loaded legacy graph.json ({} nodes, {} edges) — migrating to per-entity files...",
        graph.nodes.len(),
        graph.edges.len()
    );

    // Migrate: write per-entity files
    if let Err(e) = save_graph(knowledge_dir, &graph) {
        eprintln!("WARN: Migration to per-entity files failed: {}", e);
    } else {
        // Rename legacy file so we don't re-migrate
        let backup = knowledge_dir.join("graph.json.migrated");
        let _ = std::fs::rename(&path, &backup);
        println!("Migration complete. Legacy file renamed to graph.json.migrated");
    }

    Some(graph)
}

/// Save the entire graph (all nodes + edges). Used for full persists after bulk operations.
pub fn save_graph(knowledge_dir: &Path, graph: &KnowledgeGraph) -> std::io::Result<()> {
    std::fs::create_dir_all(knowledge_dir)?;

    // Save each node individually
    for node in graph.nodes.values() {
        save_node(knowledge_dir, node)?;
    }

    // Save edges
    save_edges(knowledge_dir, &graph.edges)?;

    println!(
        "Knowledge graph saved: {} nodes, {} edges (per-entity files)",
        graph.nodes.len(),
        graph.edges.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Search log (unchanged — already good: daily append-only JSONL)
// ---------------------------------------------------------------------------

/// Append a search event to the daily JSONL log.
pub fn append_search_log(
    knowledge_dir: &Path,
    event: &super::search_event::SearchEvent,
) -> std::io::Result<()> {
    let log_dir = knowledge_dir.join("search_log");
    let date = event.timestamp.format("%Y/%m/%d").to_string();
    let log_path = log_dir.join(format!("{}.jsonl", date));

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let line = serde_json::to_string(event)
        .map_err(std::io::Error::other)?;

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    writeln!(file, "{}", line)?;

    Ok(())
}
