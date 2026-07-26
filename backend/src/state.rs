use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::discovery::DiscoveryConfig;
use crate::knowledge::KnowledgeGraph;
use crate::models::{AreaProfile, Property, Seller, Society};
use crate::recommendations::RecommendationResponse;
use crate::search::{SearchIndex, SemanticEmbedder, SemanticSearchIndex};
use crate::serving::LoadedServingBundle;

pub struct AppState {
    /// In-memory hot data loaded at startup. Routes can read directly from here
    /// for fast access without going through storage/cache on every request.
    pub properties: RwLock<Vec<Property>>,
    /// Local recall index rebuilt from app-owned property data.
    pub search_index: RwLock<SearchIndex>,
    /// Local semantic recall index over serving search documents.
    pub semantic_index: RwLock<SemanticSearchIndex>,
    /// Query/document embedder used by the semantic recall index.
    pub semantic_embedder: Arc<dyn SemanticEmbedder>,
    /// Optional compiled KG serving bundle loaded from the local/S3-shaped lake.
    pub serving_bundle: RwLock<Option<Arc<LoadedServingBundle>>>,
    /// In-process cache keyed by property + bundle + scoring policy + engine version.
    pub recommendation_cache: RwLock<std::collections::HashMap<String, RecommendationResponse>>,
    pub areas: RwLock<Vec<AreaProfile>>,
    pub societies: RwLock<Vec<Society>>,
    pub sellers: RwLock<Vec<Seller>>,
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
    /// Monotonic counter for generating collision-free registration draft IDs.
    pub registration_counter: AtomicU64,
    /// Global rate limiter for POST /api/registrations: (window_start, count_in_window).
    /// Resets every 60 seconds. Max 30 requests per window.
    pub registration_rate_limiter: RwLock<(std::time::Instant, u32)>,
    /// Global rate limiter for POST /api/registrations/{id}/publish: (window_start, count_in_window).
    /// Resets every 60 seconds. Max 10 requests per window (tighter than registration creation).
    pub publish_rate_limiter: RwLock<(std::time::Instant, u32)>,
}
