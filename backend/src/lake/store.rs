use std::fmt;
use std::fmt::Write as _;
use std::path::Path as FsPath;
use std::sync::Arc;

use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use sha2::{Digest, Sha256};

use super::{LakeKey, LakePrefix};

/// Object-store facade using the same logical keys for local development and S3.
#[derive(Clone)]
pub struct LakeStore {
    store: Arc<dyn ObjectStore>,
}

/// Metadata captured after an artifact is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMetadata {
    pub key: LakeKey,
    pub content_hash: String,
    pub hash_algorithm: String,
    pub size_bytes: usize,
}

#[derive(Debug)]
pub enum LakeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ObjectStore(object_store::Error),
    Utf8(std::string::FromUtf8Error),
}

impl LakeStore {
    pub fn local(root: impl AsRef<FsPath>) -> Result<Self, LakeError> {
        std::fs::create_dir_all(root.as_ref()).map_err(LakeError::Io)?;
        let store = LocalFileSystem::new_with_prefix(root).map_err(LakeError::ObjectStore)?;
        Ok(Self::from_object_store(Arc::new(store)))
    }

    pub fn from_object_store(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    pub async fn put_json<T: serde::Serialize>(
        &self,
        key: &LakeKey,
        value: &T,
    ) -> Result<ArtifactMetadata, LakeError> {
        let bytes = serde_json::to_vec_pretty(value).map_err(LakeError::Json)?;
        self.put_bytes(key, bytes).await
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &LakeKey,
    ) -> Result<T, LakeError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice(&bytes).map_err(LakeError::Json)
    }

    pub async fn put_text(
        &self,
        key: &LakeKey,
        value: impl AsRef<str>,
    ) -> Result<ArtifactMetadata, LakeError> {
        self.put_bytes(key, value.as_ref().as_bytes().to_vec())
            .await
    }

    pub async fn get_text(&self, key: &LakeKey) -> Result<String, LakeError> {
        String::from_utf8(self.get_bytes(key).await?).map_err(LakeError::Utf8)
    }

    pub async fn put_bytes(
        &self,
        key: &LakeKey,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, LakeError> {
        let content_hash = sha256_hex(&bytes);
        let size_bytes = bytes.len();
        let location = object_path(key);
        self.store
            .put(&location, bytes.into())
            .await
            .map_err(LakeError::ObjectStore)?;
        Ok(ArtifactMetadata {
            key: key.clone(),
            content_hash,
            hash_algorithm: "sha256".to_string(),
            size_bytes,
        })
    }

    pub async fn get_bytes(&self, key: &LakeKey) -> Result<Vec<u8>, LakeError> {
        self.store
            .get(&object_path(key))
            .await
            .map_err(LakeError::ObjectStore)?
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(LakeError::ObjectStore)
    }

    pub async fn delete(&self, key: &LakeKey) -> Result<(), LakeError> {
        match self.store.delete(&object_path(key)).await {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(err) => Err(LakeError::ObjectStore(err)),
        }
    }

    pub fn prefix_key(
        &self,
        prefix: &LakePrefix,
        name: &str,
    ) -> Result<LakeKey, super::keys::KeyError> {
        if prefix.as_str().is_empty() {
            LakeKey::new(name)
        } else {
            LakeKey::join(&[prefix.as_str(), name])
        }
    }
}

impl fmt::Display for LakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "lake IO error: {err}"),
            Self::Json(err) => write!(f, "lake JSON error: {err}"),
            Self::ObjectStore(err) => write!(f, "lake object-store error: {err}"),
            Self::Utf8(err) => write!(f, "invalid UTF-8 artifact: {err}"),
        }
    }
}

impl std::error::Error for LakeError {}

impl LakeError {
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::ObjectStore(object_store::Error::NotFound { .. })
        )
    }
}

fn object_path(key: &LakeKey) -> ObjectPath {
    ObjectPath::from(key.as_str())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn local_store_writes_and_reads_by_lake_key() {
        let root = tempdir().unwrap();
        let store = LakeStore::local(root.path()).unwrap();
        let key = LakeKey::new("raw/source=rera/dt=2026-07/data.parquet").unwrap();

        let meta = store.put_text(&key, "{\"ok\":true}\n").await.unwrap();
        let body = store.get_text(&key).await.unwrap();

        assert_eq!(body, "{\"ok\":true}\n");
        assert_eq!(meta.key, key);
        assert_eq!(meta.size_bytes, body.len());
        assert_eq!(meta.hash_algorithm, "sha256");
        assert_eq!(meta.content_hash.len(), 64);
    }
}
