use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::discovery::DiscoveryConfig;
use crate::knowledge::search_event::EnrichmentGap;
use crate::knowledge::KnowledgeGraph;
use crate::knowledge::SearchEvent;
use crate::models::{AreaProfile, Property, Society};
use crate::recommendations::RecommendationResponse;
use crate::scoring::scoring_policy;
use crate::search::{FastTextIntentClassifier, SearchIndex, SearchResponse};
use crate::serving::LoadedServingBundle;

pub const SEARCH_ENGINE_VERSION: &str = "openestates-search-runtime-v2";
const DEFAULT_SEARCH_CACHE_CAPACITY: usize = 1024;
const DEFAULT_SEARCH_LOG_QUEUE_CAPACITY: usize = 1024;

pub struct SearchRuntimeSnapshot {
    pub bundle: Arc<LoadedServingBundle>,
    pub properties: Arc<[Property]>,
    pub property_by_id: HashMap<String, usize>,
    pub search_index: SearchIndex,
    pub societies: Arc<[Society]>,
    pub society_names: HashMap<String, String>,
    pub areas: Arc<[AreaProfile]>,
    pub version_key: RuntimeVersionKey,
}

impl SearchRuntimeSnapshot {
    pub fn new(
        bundle: Arc<LoadedServingBundle>,
        properties: Vec<Property>,
        societies: Vec<Society>,
        areas: Vec<AreaProfile>,
        search_index: SearchIndex,
    ) -> Self {
        let property_by_id = properties
            .iter()
            .enumerate()
            .map(|(index, property)| (property.id.clone(), index))
            .collect();
        let society_names = societies
            .iter()
            .map(|society| (society.id.clone(), society.name.clone()))
            .collect();
        let version_key = RuntimeVersionKey {
            serving_bundle_version: bundle.manifest.bundle_version.clone(),
            scoring_policy_version: scoring_policy().version,
            search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
        };

        Self {
            bundle,
            properties: Arc::from(properties),
            property_by_id,
            search_index,
            societies: Arc::from(societies),
            society_names,
            areas: Arc::from(areas),
            version_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RuntimeVersionKey {
    pub serving_bundle_version: String,
    pub scoring_policy_version: u32,
    pub search_engine_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchCacheKey {
    normalized_query: String,
    version_key: RuntimeVersionKey,
}

impl SearchCacheKey {
    pub fn new(query: &str, version_key: &RuntimeVersionKey) -> Self {
        Self {
            normalized_query: normalize_search_query(query),
            version_key: version_key.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CachedSearchOutput {
    pub response: Arc<SearchResponse>,
    pub log_messages: Vec<SearchLogMessage>,
}

pub struct SearchResponseCache {
    inner: tokio::sync::Mutex<LruCache<SearchCacheKey, CachedSearchOutput>>,
}

impl SearchResponseCache {
    pub fn from_env() -> Self {
        let capacity = std::env::var("OPENESTATES_SEARCH_CACHE_CAPACITY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SEARCH_CACHE_CAPACITY);
        Self::new(capacity)
    }

    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1)).expect("capacity is non-zero");
        Self {
            inner: tokio::sync::Mutex::new(LruCache::new(capacity)),
        }
    }

    pub async fn get(&self, key: &SearchCacheKey) -> Option<CachedSearchOutput> {
        self.inner.lock().await.get(key).cloned()
    }

    pub async fn put(&self, key: SearchCacheKey, output: CachedSearchOutput) {
        self.inner.lock().await.put(key, output);
    }

    pub async fn clear(&self) {
        self.inner.lock().await.clear();
    }
}

fn normalize_search_query(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[derive(Clone)]
pub enum SearchLogMessage {
    SearchEvent(SearchEvent),
    PersistEnrichmentGaps(EnrichmentGapPersistence),
}

#[derive(Clone)]
pub struct EnrichmentGapPersistence {
    pub gaps: Vec<EnrichmentGap>,
    pub query: String,
    pub intent: crate::search::intent::SearchIntent,
    pub results_returned: usize,
    pub top_candidate_society_ids: Vec<String>,
}

pub fn search_log_queue_capacity_from_env() -> usize {
    std::env::var("OPENESTATES_SEARCH_LOG_QUEUE_CAPACITY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SEARCH_LOG_QUEUE_CAPACITY)
}

pub fn spawn_search_log_worker(
    knowledge: Arc<RwLock<KnowledgeGraph>>,
    mut rx: mpsc::Receiver<SearchLogMessage>,
) {
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                SearchLogMessage::SearchEvent(event) => {
                    let mut graph = knowledge.write().await;
                    graph.log_search(event);
                }
                SearchLogMessage::PersistEnrichmentGaps(payload) => {
                    tokio::task::spawn_blocking(move || persist_enrichment_gaps(payload));
                }
            }
        }
    });
}

fn persist_enrichment_gaps(payload: EnrichmentGapPersistence) {
    if payload.gaps.is_empty() {
        return;
    }

    let path = enrichment_gaps_output_path();
    let mut entries: Vec<serde_json::Value> = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|payload| serde_json::from_str(&payload).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let recorded_at = Utc::now().to_rfc3339();
    let query_categories = query_gap_categories(&payload.intent);
    for gap in payload.gaps {
        entries.push(serde_json::json!({
            "entity_id": gap.entity_id,
            "missing_fact": gap.missing_fact,
            "reason": gap.reason,
            "query": payload.query,
            "query_categories": &query_categories,
            "top_candidate_society_ids": &payload.top_candidate_society_ids,
            "results_returned": payload.results_returned,
            "intent_area": payload.intent.area.as_deref(),
            "intent_bhk": payload.intent.bhk,
            "intent_budget_max": payload.intent.budget_max,
            "recorded_at": recorded_at,
        }));
    }

    if entries.len() > 500 {
        let start = entries.len() - 500;
        entries = entries.split_off(start);
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(payload) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(path, payload);
    }
}

fn query_gap_categories(intent: &crate::search::intent::SearchIntent) -> Vec<String> {
    let mut categories = Vec::new();

    for constraint in &intent.hard_constraints {
        push_unique_string(
            &mut categories,
            format!("hard_constraint:{}", constraint.field),
        );
    }
    for preference in &intent.positive_preferences {
        push_unique_string(&mut categories, format!("positive:{}", preference.raw_text));
    }
    for preference in &intent.negative_preferences {
        push_unique_string(&mut categories, format!("negative:{}", preference.raw_text));
    }
    for inventory_type in &intent.unsupported_inventory_types {
        push_unique_string(
            &mut categories,
            format!("unsupported_inventory:{inventory_type}"),
        );
    }
    if let Some(archetype) = &intent.buyer_archetype {
        push_unique_string(&mut categories, format!("buyer_archetype:{archetype:?}"));
    }

    if categories.is_empty() {
        categories.push("general".to_string());
    }
    categories
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn enrichment_gaps_output_path() -> PathBuf {
    if let Ok(path) = std::env::var("OPENESTATES_ENRICHMENT_GAPS_PATH") {
        return PathBuf::from(path);
    }
    PathBuf::from("data/validation/enrichment_gaps.json")
}

pub struct AppState {
    /// Immutable search-serving snapshot. /api/search loads one Arc from here at
    /// request start and never observes mixed bundle/index/property state.
    pub search_runtime: ArcSwap<SearchRuntimeSnapshot>,
    /// Bounded optimization cache for non-debug search responses.
    pub search_cache: SearchResponseCache,
    pub intent_classifier: Option<Arc<FastTextIntentClassifier>>,
    /// Bounded best-effort side-effect queue for search logging.
    pub search_event_tx: mpsc::Sender<SearchLogMessage>,
    /// Best-effort count of search log side effects dropped because the bounded queue was full.
    pub search_log_dropped_count: AtomicU64,
    /// In-memory hot data loaded at startup. Routes can read directly from here
    /// for fast access without going through storage/cache on every request.
    pub properties: RwLock<Vec<Property>>,
    /// Local recall index rebuilt from app-owned property data.
    pub search_index: RwLock<SearchIndex>,
    /// Optional compiled KG serving bundle loaded from the local/S3-shaped lake.
    pub serving_bundle: RwLock<Option<Arc<LoadedServingBundle>>>,
    /// In-process cache keyed by property + bundle + scoring policy + engine version.
    pub recommendation_cache: RwLock<std::collections::HashMap<String, RecommendationResponse>>,
    pub areas: RwLock<Vec<AreaProfile>>,
    pub societies: RwLock<Vec<Society>>,
    /// Product-facing discovery copy and shelf metadata from app/config/product/discovery_home.json.
    pub discovery_config: DiscoveryConfig,
    /// Offline city map overlays (metro / parks / lakes) clipped per property detail.
    pub map_overlays: Arc<crate::routes::map_overlays::CityMapOverlays>,
    /// The knowledge graph — the brain that learns from every search.
    pub knowledge: Arc<RwLock<KnowledgeGraph>>,
    /// Project root path (for persistence operations).
    pub project_root: PathBuf,
    /// Runtime start timestamp, exposed for stale-backend detection in development.
    pub process_started_at: DateTime<Utc>,
    /// Monotonic counter for generating collision-free interest IDs.
    pub interest_counter: AtomicU64,
    /// Global rate limiter for POST /api/interests: (window_start, count_in_window).
    /// Resets every 60 seconds. Max 60 requests per window.
    pub interest_rate_limiter: RwLock<(std::time::Instant, u32)>,
}

impl AppState {
    pub fn search_log_dropped_count(&self) -> u64 {
        self.search_log_dropped_count.load(Ordering::Relaxed)
    }

    pub fn increment_search_log_dropped_count(&self) {
        self.search_log_dropped_count
            .fetch_add(1, Ordering::Relaxed);
    }
}
