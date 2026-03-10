use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::{Cache, CacheError};

struct CacheEntry {
    data: serde_json::Value,
    expires_at: Instant,
    last_accessed: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Thread-safe in-memory LRU cache with TTL expiration.
pub struct InMemoryCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_entries: usize,
}

impl InMemoryCache {
    /// Create a cache with reasonable defaults: 1000 max entries.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries: 1000,
        }
    }

    /// Create a cache with a custom max entries limit.
    #[allow(dead_code)]
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    /// Evict expired entries and, if still over capacity, evict the least recently accessed.
    async fn evict_if_needed(&self) {
        let mut map = self.entries.write().await;

        // Remove expired entries first.
        map.retain(|_, entry| !entry.is_expired());

        // If still over capacity, evict least recently accessed.
        while map.len() >= self.max_entries {
            if let Some(lru_key) = map
                .iter()
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(k, _)| k.clone())
            {
                map.remove(&lru_key);
            } else {
                break;
            }
        }
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Cache for InMemoryCache {
    async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let mut map = self.entries.write().await;
        let entry = map.get_mut(key)?;

        if entry.is_expired() {
            map.remove(key);
            return None;
        }

        entry.last_accessed = Instant::now();
        Some(entry.data.clone())
    }

    async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        self.evict_if_needed().await;

        let entry = CacheEntry {
            data: value,
            expires_at: Instant::now() + ttl,
            last_accessed: Instant::now(),
        };

        let mut map = self.entries.write().await;
        map.insert(key.to_string(), entry);
        Ok(())
    }

    async fn invalidate(&self, key: &str) {
        let mut map = self.entries.write().await;
        map.remove(key);
    }

    async fn clear(&self) {
        let mut map = self.entries.write().await;
        map.clear();
    }
}
