use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use object_store::path::Path as ObjectPath;
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    PutMultipartOptions, PutOptions, PutPayload, PutResult,
};

use super::store::ObjectIdentity;
use super::LakeKey;

#[derive(Debug)]
pub(crate) struct PinnedObjectStore {
    inner: Arc<dyn ObjectStore>,
    identities: RwLock<HashMap<String, ObjectIdentity>>,
}

impl PinnedObjectStore {
    pub(crate) fn new(inner: Arc<dyn ObjectStore>) -> Self {
        Self {
            inner,
            identities: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn pin(&self, key: &LakeKey, identity: ObjectIdentity) -> Result<(), String> {
        let mut identities = self
            .identities
            .write()
            .map_err(|_| "pinned object identity map is poisoned".to_string())?;
        if let Some(existing) = identities.get(key.as_str()) {
            if existing != &identity {
                return Err(format!(
                    "lake object {} is already pinned to a different version",
                    key.as_str()
                ));
            }
            return Ok(());
        }
        identities.insert(key.to_string(), identity);
        Ok(())
    }

    fn identity(&self, location: &ObjectPath) -> object_store::Result<ObjectIdentity> {
        self.identities
            .read()
            .map_err(|_| object_store_error("pinned object identity map is poisoned"))?
            .get(location.as_ref())
            .cloned()
            .ok_or_else(|| object_store::Error::NotFound {
                path: location.to_string(),
                source: "object is not part of the manifest-pinned catalog".into(),
            })
    }
}

impl fmt::Display for PinnedObjectStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpenEstates manifest-pinned object store")
    }
}

#[async_trait]
impl ObjectStore for PinnedObjectStore {
    async fn put_opts(
        &self,
        _location: &ObjectPath,
        _payload: PutPayload,
        _options: PutOptions,
    ) -> object_store::Result<PutResult> {
        Err(read_only_error("put_opts"))
    }

    async fn put_multipart_opts(
        &self,
        _location: &ObjectPath,
        _options: PutMultipartOptions,
    ) -> object_store::Result<Box<dyn MultipartUpload>> {
        Err(read_only_error("put_multipart_opts"))
    }

    async fn get_opts(
        &self,
        location: &ObjectPath,
        options: GetOptions,
    ) -> object_store::Result<GetResult> {
        let identity = self.identity(location)?;
        let mut pinned = identity.pinned_get_options();
        pinned.range = options.range;
        pinned.head = options.head;
        pinned.extensions = options.extensions;
        self.inner.get_opts(location, pinned).await
    }

    fn delete_stream(
        &self,
        locations: BoxStream<'static, object_store::Result<ObjectPath>>,
    ) -> BoxStream<'static, object_store::Result<ObjectPath>> {
        locations
            .map(|location| match location {
                Ok(_) => Err(read_only_error("delete_stream")),
                Err(error) => Err(error),
            })
            .boxed()
    }

    fn list(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        let identities = match self.identities.read() {
            Ok(identities) => identities.clone(),
            Err(_) => {
                return futures_util::stream::once(async {
                    Err(object_store_error("pinned object identity map is poisoned"))
                })
                .boxed();
            }
        };
        self.inner
            .list(prefix)
            .filter_map(move |result| {
                let identities = identities.clone();
                async move {
                    match result {
                        Ok(meta) => identities
                            .get(meta.location.as_ref())
                            .map(|identity| verify_listed_identity(meta, identity)),
                        Err(error) => Some(Err(error)),
                    }
                }
            })
            .boxed()
    }

    async fn list_with_delimiter(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> object_store::Result<ListResult> {
        let identities = self
            .identities
            .read()
            .map_err(|_| object_store_error("pinned object identity map is poisoned"))?
            .clone();
        let mut result = self.inner.list_with_delimiter(prefix).await?;
        result.common_prefixes.clear();
        result.objects = result
            .objects
            .into_iter()
            .filter_map(|meta| {
                identities
                    .get(meta.location.as_ref())
                    .map(|identity| verify_listed_identity(meta, identity))
            })
            .collect::<object_store::Result<Vec<_>>>()?;
        Ok(result)
    }

    async fn copy_opts(
        &self,
        _from: &ObjectPath,
        _to: &ObjectPath,
        _options: CopyOptions,
    ) -> object_store::Result<()> {
        Err(read_only_error("copy_opts"))
    }
}

fn verify_listed_identity(
    meta: ObjectMeta,
    expected: &ObjectIdentity,
) -> object_store::Result<ObjectMeta> {
    let actual = ObjectIdentity::from_object_meta(&meta)
        .map_err(|error| object_store_error(&error.to_string()))?;
    if expected.matches(&actual) {
        Ok(meta)
    } else {
        Err(object_store::Error::Precondition {
            path: meta.location.to_string(),
            source: "object changed after catalog registration".into(),
        })
    }
}

fn read_only_error(operation: &str) -> object_store::Error {
    object_store::Error::NotImplemented {
        operation: operation.to_string(),
        implementer: "OpenEstates manifest-pinned object store".to_string(),
    }
}

fn object_store_error(message: &str) -> object_store::Error {
    object_store::Error::Generic {
        store: "openestates-pinned",
        source: message.to_string().into(),
    }
}
