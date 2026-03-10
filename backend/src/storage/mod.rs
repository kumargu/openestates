mod keys;
mod local_fs;

#[allow(unused_imports)]
pub use keys::StorageKey;
pub use local_fs::LocalFsBackend;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum StorageError {
    #[error("key not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
}

#[async_trait]
#[allow(dead_code)]
pub trait StorageBackend: Send + Sync {
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn put(&self, key: &str, data: &[u8]) -> Result<(), StorageError>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
}
