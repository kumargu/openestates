mod memory;

pub use memory::InMemoryCache;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)] // Used via Arc<dyn Cache> — compiler can't see dynamic dispatch
pub enum CacheError {
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Dyn-compatible cache trait using serde_json::Value as the interchange format.
///
/// Callers serialize/deserialize at the boundary:
/// ```ignore
/// let val = serde_json::to_value(&my_struct).unwrap();
/// cache.set("key", val, ttl).await?;
/// let retrieved: MyStruct = serde_json::from_value(cache.get("key").await.unwrap()).unwrap();
/// ```
#[async_trait]
#[allow(dead_code)] // Used via Arc<dyn Cache> — compiler can't see dynamic dispatch
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Option<serde_json::Value>;
    async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
        ttl: std::time::Duration,
    ) -> Result<(), CacheError>;
    async fn invalidate(&self, key: &str);
    async fn clear(&self);
}
