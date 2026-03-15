use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::{Mutex, RwLock};

use crate::cache::Cache;
use crate::discovery::{DiscoveryCache, GeminiClient};
use crate::knowledge::KnowledgeGraph;
use crate::knowledge::embed_client::EmbedClient;
use crate::models::{AreaProfile, Property, Seller, Society};
use crate::storage::StorageBackend;

pub struct AppState {
    #[allow(dead_code)] // Infrastructure for future S3-backed storage — not yet read at request time
    pub storage: Arc<dyn StorageBackend>,
    #[allow(dead_code)] // Pre-populated at startup, request-time cache reads not yet wired
    pub cache: Arc<dyn Cache>,
    /// In-memory hot data loaded at startup. Routes can read directly from here
    /// for fast access without going through storage/cache on every request.
    pub properties: RwLock<Vec<Property>>,
    pub areas: Vec<AreaProfile>,
    pub societies: Vec<Society>,
    pub sellers: RwLock<Vec<Seller>>,
    /// The knowledge graph — the brain that learns from every search.
    pub knowledge: Arc<RwLock<KnowledgeGraph>>,
    /// Project root path (for persistence operations).
    pub project_root: PathBuf,
    /// Gemini client for live discovery (None if GOOGLE_AI_API_KEY not set).
    pub gemini: Option<GeminiClient>,
    /// Discovery cache — prevents duplicate Gemini calls.
    pub discovery_cache: Mutex<DiscoveryCache>,
    /// Embedding client for semantic search (None if GOOGLE_AI_API_KEY not set).
    pub embed_client: Option<EmbedClient>,
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
