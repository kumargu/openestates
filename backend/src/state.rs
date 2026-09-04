use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::discovery::DiscoveryConfig;
use crate::knowledge::KnowledgeGraph;
use crate::knowledge::SearchEvent;
use crate::models::{AreaProfile, Property, Society};
use crate::recommendations::RecommendationResponse;
use crate::scoring::scoring_policy;
use crate::search::{SearchIndex, SearchResponse};
use crate::security::security_tuning;
use crate::security::ExecutionLanes;
use crate::serving::LoadedServingBundle;

pub const SEARCH_ENGINE_VERSION: &str = "openestates-search-runtime-v2";

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
    inner: Arc<std::sync::Mutex<SearchCacheState>>,
}

struct SearchCacheState {
    entries: LruCache<SearchCacheKey, WeightedSearchOutput>,
    resident_bytes: usize,
    max_bytes: usize,
    max_in_flight: usize,
    in_flight: HashMap<SearchCacheKey, InFlightSearch>,
    next_reservation_id: u64,
}

struct InFlightSearch {
    sender: tokio::sync::watch::Sender<Option<CachedSearchOutput>>,
    reservation_id: u64,
}

struct WeightedSearchOutput {
    output: CachedSearchOutput,
    weight_bytes: usize,
}

pub enum SearchCacheLookup {
    Hit(CachedSearchOutput),
    Leader(SearchCacheReservation),
    Waiter(tokio::sync::watch::Receiver<Option<CachedSearchOutput>>),
    Overloaded,
}

pub struct SearchCacheReservation {
    inner: Arc<std::sync::Mutex<SearchCacheState>>,
    key: SearchCacheKey,
    reservation_id: u64,
    completed: bool,
}

impl SearchResponseCache {
    pub fn from_env() -> Self {
        let tuning = &security_tuning().search_cache;
        Self::new_with_budget(tuning.capacity, tuning.max_bytes)
    }

    pub fn new(capacity: usize) -> Self {
        Self::new_with_budget(capacity, security_tuning().search_cache.max_bytes)
    }

    pub fn new_with_budget(capacity: usize, max_bytes: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1)).expect("capacity is non-zero");
        Self {
            inner: Arc::new(std::sync::Mutex::new(SearchCacheState {
                entries: LruCache::new(capacity),
                resident_bytes: 0,
                max_bytes: max_bytes.max(1),
                max_in_flight: security_tuning().requests.search_concurrency,
                in_flight: HashMap::new(),
                next_reservation_id: 0,
            })),
        }
    }

    pub async fn get(&self, key: &SearchCacheKey) -> Option<CachedSearchOutput> {
        self.inner
            .lock()
            .expect("search cache lock poisoned")
            .entries
            .get(key)
            .map(|entry| entry.output.clone())
    }

    pub async fn put(&self, key: SearchCacheKey, output: CachedSearchOutput) {
        let weight_bytes = cached_search_weight_bytes(&output);
        let mut state = self.inner.lock().expect("search cache lock poisoned");
        insert_search_cache_entry(&mut state, key, output, weight_bytes);
    }

    pub async fn lookup_or_reserve(&self, key: &SearchCacheKey) -> SearchCacheLookup {
        let mut state = self.inner.lock().expect("search cache lock poisoned");
        if let Some(entry) = state.entries.get(key) {
            return SearchCacheLookup::Hit(entry.output.clone());
        }
        if let Some(in_flight) = state.in_flight.get(key) {
            return SearchCacheLookup::Waiter(in_flight.sender.subscribe());
        }
        if state.in_flight.len() >= state.max_in_flight {
            return SearchCacheLookup::Overloaded;
        }

        let (sender, _receiver) = tokio::sync::watch::channel(None);
        state.next_reservation_id = state.next_reservation_id.wrapping_add(1);
        let reservation_id = state.next_reservation_id;
        state.in_flight.insert(
            key.clone(),
            InFlightSearch {
                sender,
                reservation_id,
            },
        );
        SearchCacheLookup::Leader(SearchCacheReservation {
            inner: self.inner.clone(),
            key: key.clone(),
            reservation_id,
            completed: false,
        })
    }

    pub async fn clear(&self) {
        let mut state = self.inner.lock().expect("search cache lock poisoned");
        state.entries.clear();
        state.resident_bytes = 0;
        state.in_flight.clear();
    }

    #[cfg(test)]
    async fn resident_bytes(&self) -> usize {
        self.inner
            .lock()
            .expect("search cache lock poisoned")
            .resident_bytes
    }
}

impl SearchCacheReservation {
    pub async fn complete(mut self, output: CachedSearchOutput) {
        let weight_bytes = cached_search_weight_bytes(&output);
        let mut state = self.inner.lock().expect("search cache lock poisoned");
        let is_current = state
            .in_flight
            .get(&self.key)
            .is_some_and(|in_flight| in_flight.reservation_id == self.reservation_id);
        if is_current {
            let in_flight = state
                .in_flight
                .remove(&self.key)
                .expect("current reservation exists");
            insert_search_cache_entry(&mut state, self.key.clone(), output.clone(), weight_bytes);
            let _ = in_flight.sender.send(Some(output));
        }
        self.completed = true;
    }
}

impl Drop for SearchCacheReservation {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if state
            .in_flight
            .get(&self.key)
            .is_some_and(|in_flight| in_flight.reservation_id == self.reservation_id)
        {
            state.in_flight.remove(&self.key);
        }
    }
}

fn insert_search_cache_entry(
    state: &mut SearchCacheState,
    key: SearchCacheKey,
    output: CachedSearchOutput,
    weight_bytes: usize,
) {
    if weight_bytes > state.max_bytes {
        return;
    }

    if let Some(replaced) = state.entries.pop(&key) {
        state.resident_bytes = state.resident_bytes.saturating_sub(replaced.weight_bytes);
    }
    while state.resident_bytes.saturating_add(weight_bytes) > state.max_bytes {
        let Some((_key, evicted)) = state.entries.pop_lru() else {
            break;
        };
        state.resident_bytes = state.resident_bytes.saturating_sub(evicted.weight_bytes);
    }

    let entry = WeightedSearchOutput {
        output,
        weight_bytes,
    };
    if let Some((_key, evicted)) = state.entries.push(key, entry) {
        state.resident_bytes = state.resident_bytes.saturating_sub(evicted.weight_bytes);
    }
    state.resident_bytes = state.resident_bytes.saturating_add(weight_bytes);
}

/// Serialized response bytes dominate these entries. Doubling that exact size
/// conservatively covers the live Rust object graph and small logging metadata.
fn cached_search_weight_bytes(output: &CachedSearchOutput) -> usize {
    serde_json::to_vec(output.response.as_ref())
        .map(|bytes| bytes.len().saturating_mul(2).saturating_add(1024))
        .unwrap_or(usize::MAX)
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
}

pub fn search_log_queue_capacity_from_env() -> usize {
    security_tuning().search_cache.log_queue_capacity
}

pub fn spawn_search_log_worker(
    execution: &ExecutionLanes,
    knowledge: Arc<RwLock<KnowledgeGraph>>,
    mut rx: mpsc::Receiver<SearchLogMessage>,
) {
    execution.spawn_internal(async move {
        while let Some(message) = rx.recv().await {
            match message {
                SearchLogMessage::SearchEvent(event) => {
                    let mut graph = knowledge.write().await;
                    graph.log_search(event, security_tuning().search_cache.event_history);
                }
            }
        }
    });
}

pub struct AppState {
    /// Explicit execution lanes keep customer request coordination isolated
    /// from CPU-heavy ranking and internal/background work.
    pub execution: ExecutionLanes,
    /// Immutable search-serving snapshot. /api/search loads one Arc from here at
    /// request start and never observes mixed bundle/index/property state.
    pub search_runtime: ArcSwap<SearchRuntimeSnapshot>,
    /// Bounded optimization cache for non-debug search responses.
    pub search_cache: SearchResponseCache,
    /// One serialized full-catalog response per active runtime version. This
    /// prevents repeated anonymous reads from rebuilding and serializing ~1 MiB.
    pub property_catalog_cache: tokio::sync::Mutex<Option<(String, bytes::Bytes)>>,
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
    /// Serializes bounded interest-file accounting and appends.
    pub interest_write_lock: tokio::sync::Mutex<()>,
    /// Prevents authenticated admin requests from spawning overlapping asset runs.
    pub asset_run_active: AtomicBool,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version_key(bundle: &str) -> RuntimeVersionKey {
        RuntimeVersionKey {
            serving_bundle_version: bundle.to_string(),
            scoring_policy_version: 1,
            search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
        }
    }

    fn zero_result_output(query: &str) -> CachedSearchOutput {
        let version = version_key("test-bundle");
        CachedSearchOutput {
            response: Arc::new(SearchResponse {
                query: query.to_string(),
                result_sets: Vec::new(),
                ordered_result_ids: Vec::new(),
                total_matches: 0,
                runtime_version: crate::search::SearchRuntimeVersion {
                    serving_bundle_version: version.serving_bundle_version,
                    scoring_policy_version: version.scoring_policy_version,
                    search_engine_version: version.search_engine_version,
                },
                area_context: None,
                state: "no_matches".to_string(),
                search_guidance: None,
            }),
            log_messages: Vec::new(),
        }
    }

    #[tokio::test]
    async fn search_cache_isolated_by_serving_version_including_zero_results() {
        let cache = SearchResponseCache::new(4);
        let query = "Ajmera Nucleus 2BHK under 1.5cr";
        let south_key = SearchCacheKey::new(query, &version_key("south-43"));
        let mixed_key = SearchCacheKey::new(query, &version_key("mixed-south-45"));

        cache
            .put(south_key.clone(), zero_result_output(query))
            .await;

        assert!(cache.get(&south_key).await.is_some());
        assert!(cache.get(&mixed_key).await.is_none());

        cache.clear().await;
        assert!(cache.get(&south_key).await.is_none());
    }

    #[tokio::test]
    async fn search_cache_evicts_to_its_byte_budget_and_rejects_oversized_entries() {
        let cache = SearchResponseCache::new_with_budget(8, 3_000);
        let first_key = SearchCacheKey::new("first", &version_key("bundle"));
        let second_key = SearchCacheKey::new("second", &version_key("bundle"));
        cache
            .put(first_key.clone(), zero_result_output(&"a".repeat(500)))
            .await;
        cache
            .put(second_key.clone(), zero_result_output(&"b".repeat(500)))
            .await;

        assert!(cache.resident_bytes().await <= 3_000);
        assert!(cache.get(&first_key).await.is_none());
        assert!(cache.get(&second_key).await.is_some());

        let oversized_key = SearchCacheKey::new("oversized", &version_key("bundle"));
        cache
            .put(
                oversized_key.clone(),
                zero_result_output(&"x".repeat(2_000)),
            )
            .await;
        assert!(cache.get(&oversized_key).await.is_none());
        assert!(cache.resident_bytes().await <= 3_000);
    }

    #[tokio::test]
    async fn search_cache_coalesces_duplicate_cold_queries() {
        let cache = SearchResponseCache::new(8);
        let key = SearchCacheKey::new("same query", &version_key("bundle"));
        let reservation = match cache.lookup_or_reserve(&key).await {
            SearchCacheLookup::Leader(reservation) => reservation,
            _ => panic!("first cold query should lead"),
        };
        let mut waiter = match cache.lookup_or_reserve(&key).await {
            SearchCacheLookup::Waiter(waiter) => waiter,
            _ => panic!("duplicate cold query should wait for its leader"),
        };

        reservation.complete(zero_result_output("same query")).await;
        let shared = waiter
            .wait_for(Option::is_some)
            .await
            .expect("leader publishes its result");
        assert_eq!(
            shared.as_ref().unwrap().response.query,
            "same query".to_string()
        );
    }

    #[tokio::test]
    async fn dropping_a_search_reservation_cancels_waiters_and_allows_retry() {
        let cache = SearchResponseCache::new(8);
        let key = SearchCacheKey::new("cancelled query", &version_key("bundle"));
        let reservation = match cache.lookup_or_reserve(&key).await {
            SearchCacheLookup::Leader(reservation) => reservation,
            _ => panic!("first query should lead"),
        };
        let mut waiter = match cache.lookup_or_reserve(&key).await {
            SearchCacheLookup::Waiter(waiter) => waiter,
            _ => panic!("duplicate query should wait"),
        };

        drop(reservation);
        assert!(waiter.changed().await.is_err());
        assert!(matches!(
            cache.lookup_or_reserve(&key).await,
            SearchCacheLookup::Leader(_)
        ));
    }
}
