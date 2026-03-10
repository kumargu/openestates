use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;

use super::{StorageBackend, StorageError};

/// Filesystem-backed storage that reads from a configurable root directory.
///
/// Supports two layouts:
/// - **Lake layout** (`data/lake/{key}`): the new organized structure matching the Python pipeline
/// - **Seed layout** (`data/seed/{file}`): the legacy flat JSON format, used as fallback
pub struct LocalFsBackend {
    /// Primary root: typically `data/lake/`
    lake_root: PathBuf,
    /// Fallback root: typically `data/seed/` for backward compat
    seed_root: PathBuf,
}

impl LocalFsBackend {
    pub fn new(lake_root: PathBuf, seed_root: PathBuf) -> Self {
        Self {
            lake_root,
            seed_root,
        }
    }

    /// Resolve a key to a filesystem path, checking lake first then seed fallback.
    fn resolve_path(&self, key: &str) -> PathBuf {
        // If the key starts with "seed/", map to the seed root directly.
        if let Some(rest) = key.strip_prefix("seed/") {
            return self.seed_root.join(rest);
        }
        self.lake_root.join(key)
    }

    /// Try lake path first; if it doesn't exist and we have a seed fallback, try that.
    async fn resolve_with_fallback(&self, key: &str) -> Result<PathBuf, StorageError> {
        let lake_path = self.resolve_path(key);
        if fs::try_exists(&lake_path).await.unwrap_or(false) {
            return Ok(lake_path);
        }

        // For keys like "seed/properties.json", the resolve_path already points to seed_root.
        // For lake keys, there's no automatic fallback — the key scheme is different.
        Err(StorageError::NotFound(key.to_string()))
    }
}

#[async_trait]
impl StorageBackend for LocalFsBackend {
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve_with_fallback(key).await?;
        let data = fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(key.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        Ok(data)
    }

    async fn put(&self, key: &str, data: &[u8]) -> Result<(), StorageError> {
        let path = self.resolve_path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let dir = self.resolve_path(prefix);
        let mut keys = Vec::new();

        if !fs::try_exists(&dir).await.unwrap_or(false) {
            return Ok(keys);
        }

        collect_keys_recursive(&dir, &dir, prefix, &mut keys).await?;
        keys.sort();
        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.resolve_path(key);
        Ok(fs::try_exists(&path).await.unwrap_or(false))
    }
}

/// Recursively collect all file keys under a directory.
#[allow(dead_code)]
async fn collect_keys_recursive(
    base: &Path,
    dir: &Path,
    prefix: &str,
    keys: &mut Vec<String>,
) -> Result<(), StorageError> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let ft = entry.file_type().await?;
        if ft.is_dir() {
            Box::pin(collect_keys_recursive(base, &entry.path(), prefix, keys)).await?;
        } else if ft.is_file() {
            if let Ok(rel) = entry.path().strip_prefix(base) {
                let key = if prefix.is_empty() {
                    rel.to_string_lossy().to_string()
                } else {
                    format!("{}/{}", prefix, rel.to_string_lossy())
                };
                keys.push(key);
            }
        }
    }
    Ok(())
}
