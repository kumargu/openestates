use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::cache::Cache;
use crate::discovery::{DiscoveryCache, GeminiClient};
use crate::knowledge::KnowledgeGraph;
use crate::knowledge::embed_client::EmbedClient;
use crate::models::{AreaProfile, Property, Society};
use crate::storage::StorageBackend;

#[allow(dead_code)]
pub struct AppState {
    pub storage: Arc<dyn StorageBackend>,
    pub cache: Arc<dyn Cache>,
    /// In-memory hot data loaded at startup. Routes can read directly from here
    /// for fast access without going through storage/cache on every request.
    pub properties: RwLock<Vec<Property>>,
    pub areas: Vec<AreaProfile>,
    pub societies: Vec<Society>,
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
}
