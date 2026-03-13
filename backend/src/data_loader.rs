use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};

use crate::cache::{Cache, InMemoryCache};
use crate::discovery::{DiscoveryCache, GeminiClient};
use crate::knowledge;
use crate::knowledge::embed_client::EmbedClient;
use crate::models::{AreaProfile, Property, Seller, Society};
use crate::state::AppState;
use crate::storage::{LocalFsBackend, StorageBackend};

/// Load all seed data and construct the full AppState with storage and cache layers.
pub async fn load_app_state(project_root: &Path) -> AppState {
    let lake_root = project_root.join("data").join("lake");
    let seed_root = project_root.join("data").join("seed");

    let storage: Arc<dyn StorageBackend> =
        Arc::new(LocalFsBackend::new(lake_root, seed_root.clone()));
    let cache: Arc<dyn Cache> = Arc::new(InMemoryCache::new());

    // Load data through the storage abstraction, falling back to seed files.
    let properties = load_via_storage::<Vec<Property>>(&storage, "seed/properties.json")
        .await
        .unwrap_or_else(|| {
            println!("WARN: Could not load properties via storage, trying direct file read");
            load_json_direct::<Vec<Property>>(&seed_root.join("properties.json"))
        });

    let areas = load_via_storage::<Vec<AreaProfile>>(&storage, "seed/area_profiles.json")
        .await
        .unwrap_or_else(|| {
            println!("WARN: Could not load areas via storage, trying direct file read");
            load_json_direct::<Vec<AreaProfile>>(&seed_root.join("area_profiles.json"))
        });

    let societies = load_via_storage::<Vec<Society>>(&storage, "seed/societies.json")
        .await
        .unwrap_or_else(|| {
            println!("WARN: Could not load societies via storage, trying direct file read");
            load_json_direct::<Vec<Society>>(&seed_root.join("societies.json"))
        });

    // Load sellers from data/sellers/sellers.json
    let sellers_path = project_root.join("data").join("sellers").join("sellers.json");
    let sellers: Vec<Seller> = if sellers_path.exists() {
        load_json_direct::<Vec<Seller>>(&sellers_path)
    } else {
        println!("WARN: No sellers.json found at {}", sellers_path.display());
        Vec::new()
    };

    println!(
        "Loaded {} properties, {} areas, {} societies, {} sellers",
        properties.len(),
        areas.len(),
        societies.len(),
        sellers.len()
    );

    // --- Knowledge Graph ---
    // Try loading from disk first; if not found, bootstrap from seed data.
    let kg_dir = knowledge::store::knowledge_dir(project_root);
    let graph = match knowledge::store::load_graph(&kg_dir) {
        Some(graph) => {
            let stats = graph.stats();
            println!(
                "Knowledge graph loaded: {} nodes, {} edges, {} facts",
                stats.total_nodes, stats.total_edges, stats.total_facts
            );
            graph
        }
        None => {
            println!("No existing knowledge graph found, bootstrapping from seed data...");
            let graph =
                knowledge::bootstrap::bootstrap_from_seed(&properties, &societies, &areas);
            let stats = graph.stats();
            println!(
                "Knowledge graph bootstrapped: {} nodes, {} edges, {} facts",
                stats.total_nodes, stats.total_edges, stats.total_facts
            );

            // Persist the bootstrapped graph
            if let Err(e) = knowledge::store::save_graph(&kg_dir, &graph) {
                println!("WARN: Failed to save bootstrapped knowledge graph: {}", e);
            }

            graph
        }
    };

    // Pre-populate cache with frequently accessed data.
    let ttl = Duration::from_secs(300); // 5 minutes
    if let Ok(val) = serde_json::to_value(&properties) {
        let _ = cache.set("all_properties", val, ttl).await;
    }
    if let Ok(val) = serde_json::to_value(&areas) {
        let _ = cache.set("all_areas", val, ttl).await;
    }
    if let Ok(val) = serde_json::to_value(&societies) {
        let _ = cache.set("all_societies", val, ttl).await;
    }

    // Cache individual properties by ID for detail page lookups.
    for p in &properties {
        let key = format!("property:{}", p.id);
        if let Ok(val) = serde_json::to_value(p) {
            let _ = cache.set(&key, val, ttl).await;
        }
    }

    // --- Live Discovery + Semantic Search ---
    let api_key = std::env::var("GOOGLE_AI_API_KEY").ok();
    let gemini = api_key.as_ref().map(|key| {
        println!("Live discovery enabled (GOOGLE_AI_API_KEY found)");
        GeminiClient::new(key.clone())
    });
    let embed_client = api_key.map(|key| {
        println!("Semantic search enabled (embedding client initialized)");
        EmbedClient::new(key)
    });
    if gemini.is_none() {
        println!("Live discovery + semantic search disabled (set GOOGLE_AI_API_KEY to enable)");
    }

    let max_per_hour: u32 = std::env::var("MAX_DISCOVERIES_PER_HOUR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let discovery_cache = Mutex::new(DiscoveryCache::new(24, max_per_hour));

    AppState {
        storage,
        cache,
        properties: RwLock::new(properties),
        areas,
        societies,
        sellers: RwLock::new(sellers),
        knowledge: Arc::new(RwLock::new(graph)),
        project_root: project_root.to_path_buf(),
        gemini,
        discovery_cache,
        embed_client,
        interest_counter: AtomicU64::new(0),
        interest_rate_limiter: RwLock::new((Instant::now(), 0)),
        registration_counter: AtomicU64::new(0),
        registration_rate_limiter: RwLock::new((Instant::now(), 0)),
        publish_rate_limiter: RwLock::new((Instant::now(), 0)),
    }
}

/// Load and deserialize JSON through the storage backend.
async fn load_via_storage<T: serde::de::DeserializeOwned>(
    storage: &Arc<dyn StorageBackend>,
    key: &str,
) -> Option<T> {
    let data = storage.get(key).await.ok()?;
    serde_json::from_slice(&data).ok()
}

/// Direct file read fallback (sync) — same as the old data_loader behavior.
fn load_json_direct<T: serde::de::DeserializeOwned>(path: &PathBuf) -> T {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}

/// Legacy sync loader — kept for backward compatibility but new code should use load_app_state.
#[allow(dead_code)]
pub fn load_seed_data(data_dir: &Path) -> AppState {
    let properties: Vec<Property> = load_json_sync(data_dir.join("properties.json"));
    let areas: Vec<AreaProfile> = load_json_sync(data_dir.join("area_profiles.json"));
    let societies: Vec<Society> = load_json_sync(data_dir.join("societies.json"));

    println!(
        "Loaded {} properties, {} areas, {} societies (legacy loader)",
        properties.len(),
        areas.len(),
        societies.len()
    );

    let storage: Arc<dyn StorageBackend> = Arc::new(LocalFsBackend::new(
        data_dir.parent().unwrap_or(data_dir).join("lake"),
        data_dir.to_path_buf(),
    ));
    let cache: Arc<dyn Cache> = Arc::new(InMemoryCache::new());
    let graph = knowledge::bootstrap::bootstrap_from_seed(&properties, &societies, &areas);

    AppState {
        storage,
        cache,
        properties: RwLock::new(properties),
        areas,
        societies,
        sellers: RwLock::new(Vec::new()),
        knowledge: Arc::new(RwLock::new(graph)),
        project_root: data_dir.parent().unwrap_or(data_dir).to_path_buf(),
        gemini: None,
        discovery_cache: Mutex::new(DiscoveryCache::new(24, 10)),
        embed_client: None,
        interest_counter: AtomicU64::new(0),
        interest_rate_limiter: RwLock::new((Instant::now(), 0)),
        registration_counter: AtomicU64::new(0),
        registration_rate_limiter: RwLock::new((Instant::now(), 0)),
        publish_rate_limiter: RwLock::new((Instant::now(), 0)),
    }
}

fn load_json_sync<T: serde::de::DeserializeOwned>(path: std::path::PathBuf) -> Vec<T> {
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}
