use std::fmt;
use std::fmt::Write as _;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, UpdateVersion};
use sha2::{Digest, Sha256};

use super::keys::KeyError;
use super::{LakeKey, LakePrefix};

/// Object-store facade using the same logical keys for local development and S3.
#[derive(Clone)]
pub struct LakeStore {
    store: Arc<dyn ObjectStore>,
    local_root: Option<Arc<PathBuf>>,
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
    InvalidMetadata(String),
    ConcurrentModification(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Key(KeyError),
    ObjectStore(object_store::Error),
    Utf8(std::string::FromUtf8Error),
}

impl LakeStore {
    pub fn local(root: impl AsRef<FsPath>) -> Result<Self, LakeError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root).map_err(LakeError::Io)?;
        let store = LocalFileSystem::new_with_prefix(&root).map_err(LakeError::ObjectStore)?;
        Ok(Self {
            store: Arc::new(store),
            local_root: Some(Arc::new(root)),
        })
    }

    pub fn from_object_store(store: Arc<dyn ObjectStore>) -> Self {
        Self {
            store,
            local_root: None,
        }
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

    pub async fn put_json_if<T, F>(
        &self,
        key: &LakeKey,
        value: &T,
        replace: F,
    ) -> Result<bool, LakeError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: Fn(Option<&T>) -> bool,
    {
        if let Some(root) = &self.local_root {
            let _guard = LocalKeyLock::acquire(root, key).await?;
            let current = match self.get_json::<T>(key).await {
                Ok(current) => Some(current),
                Err(err) if err.is_not_found() => None,
                Err(err) => return Err(err),
            };
            if !replace(current.as_ref()) {
                return Ok(false);
            }
            self.put_json(key, value).await?;
            return Ok(true);
        }

        let bytes = serde_json::to_vec_pretty(value).map_err(LakeError::Json)?;
        for _ in 0..8 {
            let location = object_path(key);
            let (current, mode) = match self.store.get(&location).await {
                Ok(result) => {
                    let version = UpdateVersion {
                        e_tag: result.meta.e_tag.clone(),
                        version: result.meta.version.clone(),
                    };
                    let current = result
                        .bytes()
                        .await
                        .map_err(LakeError::ObjectStore)
                        .and_then(|bytes| {
                            serde_json::from_slice::<T>(&bytes).map_err(LakeError::Json)
                        })?;
                    (Some(current), PutMode::Update(version))
                }
                Err(object_store::Error::NotFound { .. }) => (None, PutMode::Create),
                Err(err) => return Err(LakeError::ObjectStore(err)),
            };
            if !replace(current.as_ref()) {
                return Ok(false);
            }
            match self
                .store
                .put_opts(&location, bytes.clone().into(), mode.into())
                .await
            {
                Ok(_) => return Ok(true),
                Err(object_store::Error::AlreadyExists { .. })
                | Err(object_store::Error::Precondition { .. }) => continue,
                Err(err) => return Err(LakeError::ObjectStore(err)),
            }
        }
        Err(LakeError::InvalidMetadata(format!(
            "conditional update contention for {key}"
        )))
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

    pub async fn artifact_metadata(&self, key: &LakeKey) -> Result<ArtifactMetadata, LakeError> {
        let bytes = self.get_bytes(key).await?;
        Ok(ArtifactMetadata {
            key: key.clone(),
            content_hash: sha256_hex(&bytes),
            hash_algorithm: "sha256".to_string(),
            size_bytes: bytes.len(),
        })
    }

    pub async fn list_keys(&self, prefix: &LakePrefix) -> Result<Vec<LakeKey>, LakeError> {
        let object_prefix = if prefix.as_str().is_empty() {
            None
        } else {
            Some(ObjectPath::from(prefix.as_str()))
        };
        let mut stream = self.store.list(object_prefix.as_ref());
        let mut keys = Vec::new();

        while let Some(meta) = stream.next().await {
            let meta = meta.map_err(LakeError::ObjectStore)?;
            keys.push(LakeKey::new(meta.location.to_string()).map_err(LakeError::Key)?);
        }

        keys.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(keys)
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

struct LocalKeyLock {
    path: PathBuf,
}

impl LocalKeyLock {
    async fn acquire(root: &FsPath, key: &LakeKey) -> Result<Self, LakeError> {
        let lock_dir = root.join(".openestates-locks");
        tokio::fs::create_dir_all(&lock_dir)
            .await
            .map_err(LakeError::Io)?;
        let path = lock_dir.join(format!("{}.lock", sha256_hex(key.as_str().as_bytes())));
        for _ in 0..500 {
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(_) => return Ok(Self { path }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = tokio::fs::metadata(&path)
                        .await
                        .ok()
                        .and_then(|metadata| metadata.modified().ok())
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > std::time::Duration::from_secs(30));
                    if stale {
                        match tokio::fs::remove_file(&path).await {
                            Ok(()) => continue,
                            Err(remove_err)
                                if remove_err.kind() == std::io::ErrorKind::NotFound =>
                            {
                                continue;
                            }
                            Err(_) => {}
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(err) => return Err(LakeError::Io(err)),
            }
        }
        Err(LakeError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out acquiring lake key lock for {key}"),
        )))
    }
}

impl Drop for LocalKeyLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl fmt::Display for LakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetadata(message) => write!(f, "lake metadata error: {message}"),
            Self::ConcurrentModification(message) => {
                write!(f, "lake concurrent modification: {message}")
            }
            Self::Io(err) => write!(f, "lake IO error: {err}"),
            Self::Json(err) => write!(f, "lake JSON error: {err}"),
            Self::Key(err) => write!(f, "lake key error: {err}"),
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

    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Io(err) => matches!(
                err.kind(),
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            ),
            Self::ObjectStore(err) => matches!(
                err,
                object_store::Error::Generic { .. } | object_store::Error::JoinError { .. }
            ),
            Self::InvalidMetadata(_)
            | Self::ConcurrentModification(_)
            | Self::Json(_)
            | Self::Key(_)
            | Self::Utf8(_) => false,
        }
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
    use std::io::ErrorKind;

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

    #[tokio::test]
    async fn local_store_lists_keys_by_prefix_in_stable_order() {
        let root = tempdir().unwrap();
        let store = LakeStore::local(root.path()).unwrap();

        store
            .put_text(
                &LakeKey::new("manifests/assets/a/dt=2/current.json").unwrap(),
                "{}",
            )
            .await
            .unwrap();
        store
            .put_text(
                &LakeKey::new("manifests/assets/a/dt=1/current.json").unwrap(),
                "{}",
            )
            .await
            .unwrap();
        store
            .put_text(
                &LakeKey::new("manifests/assets/b/current.json").unwrap(),
                "{}",
            )
            .await
            .unwrap();

        let keys = store
            .list_keys(&LakePrefix::new("manifests/assets/a").unwrap())
            .await
            .unwrap();

        assert_eq!(
            keys.iter().map(LakeKey::as_str).collect::<Vec<_>>(),
            vec![
                "manifests/assets/a/dt=1/current.json",
                "manifests/assets/a/dt=2/current.json"
            ]
        );
    }

    #[test]
    fn retryable_errors_are_limited_to_transient_storage_failures() {
        let source = || Box::new(std::io::Error::other("test")) as _;

        assert!(LakeError::Io(std::io::Error::from(ErrorKind::TimedOut)).is_retryable());
        assert!(LakeError::ObjectStore(object_store::Error::Generic {
            store: "test",
            source: source(),
        })
        .is_retryable());

        assert!(!LakeError::Io(std::io::Error::from(ErrorKind::PermissionDenied)).is_retryable());
        assert!(!LakeError::InvalidMetadata("invalid manifest".to_string()).is_retryable());
        for error in [
            object_store::Error::NotFound {
                path: "missing".to_string(),
                source: source(),
            },
            object_store::Error::Precondition {
                path: "current.json".to_string(),
                source: source(),
            },
            object_store::Error::PermissionDenied {
                path: "private".to_string(),
                source: source(),
            },
            object_store::Error::Unauthenticated {
                path: "private".to_string(),
                source: source(),
            },
        ] {
            assert!(!LakeError::ObjectStore(error).is_retryable());
        }
    }
}
