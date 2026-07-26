use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(feature = "s3")]
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
#[cfg(feature = "s3")]
use object_store::prefix::PrefixStore;
#[cfg(feature = "s3")]
use object_store::ObjectStore;
use url::Url;

use super::{LakeError, LakeStore};

pub const LAKE_URL_ENV: &str = "OPENESTATES_LAKE_URL";

/// Physical lake location. Logical [`super::LakeKey`] values are unchanged by
/// the selected backend or S3 prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LakeStoreLocation {
    Local(PathBuf),
    S3 {
        bucket: String,
        prefix: Option<String>,
    },
}

impl LakeStoreLocation {
    pub fn from_env(project_root: &Path) -> Result<Self, LakeError> {
        match env::var_os(LAKE_URL_ENV) {
            None => Self::parse(project_root, None),
            Some(value) => {
                let value = value.into_string().map_err(|_| {
                    LakeError::Configuration(format!("{LAKE_URL_ENV} must be valid UTF-8"))
                })?;
                Self::parse(project_root, Some(&value))
            }
        }
    }

    pub fn parse(project_root: &Path, configured_url: Option<&str>) -> Result<Self, LakeError> {
        let Some(configured_url) = configured_url else {
            return Ok(Self::Local(project_root.join("data").join("lake")));
        };
        if configured_url.is_empty() {
            return Err(LakeError::Configuration(format!(
                "{LAKE_URL_ENV} cannot be empty"
            )));
        }

        let url = Url::parse(configured_url).map_err(|err| {
            LakeError::Configuration(format!(
                "invalid {LAKE_URL_ENV} value {configured_url:?}: {err}"
            ))
        })?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err(LakeError::Configuration(format!(
                "{LAKE_URL_ENV} cannot contain a query string or fragment"
            )));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(LakeError::Configuration(format!(
                "{LAKE_URL_ENV} cannot contain credentials"
            )));
        }

        match url.scheme() {
            "file" => parse_file_url(url),
            "s3" => parse_s3_url(configured_url, url),
            scheme => Err(LakeError::Configuration(format!(
                "unsupported {LAKE_URL_ENV} scheme {scheme:?}; expected file:// or s3://"
            ))),
        }
    }

    pub fn open(&self) -> Result<LakeStore, LakeError> {
        match self {
            Self::Local(root) => LakeStore::local(root),
            Self::S3 { bucket, prefix } => open_s3(bucket, prefix.as_deref()),
        }
    }
}

#[cfg(feature = "s3")]
fn open_s3(bucket: &str, prefix: Option<&str>) -> Result<LakeStore, LakeError> {
    use std::sync::Arc;

    let store = AmazonS3Builder::from_env()
        .with_bucket_name(bucket)
        .build()
        .map_err(LakeError::ObjectStore)?;
    let store: Arc<dyn ObjectStore> = match prefix {
        Some(prefix) => Arc::new(PrefixStore::new(store, ObjectPath::from(prefix))),
        None => Arc::new(store),
    };
    Ok(LakeStore::from_object_store(store))
}

#[cfg(not(feature = "s3"))]
fn open_s3(_bucket: &str, _prefix: Option<&str>) -> Result<LakeStore, LakeError> {
    Err(LakeError::Configuration(
        "S3 lake support is not compiled in; rebuild the backend with --features s3".to_string(),
    ))
}

impl fmt::Display for LakeStoreLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(root) => write!(f, "file://{}", root.display()),
            Self::S3 { bucket, prefix } => match prefix {
                Some(prefix) => write!(f, "s3://{bucket}/{prefix}"),
                None => write!(f, "s3://{bucket}"),
            },
        }
    }
}

fn parse_file_url(url: Url) -> Result<LakeStoreLocation, LakeError> {
    if url.host_str().is_some() || url.port().is_some() {
        return Err(LakeError::Configuration(format!(
            "{LAKE_URL_ENV} file URLs must use an absolute local path without a host"
        )));
    }
    let root = url.to_file_path().map_err(|()| {
        LakeError::Configuration(format!(
            "{LAKE_URL_ENV} file URL must contain an absolute local path"
        ))
    })?;
    if !root.is_absolute() {
        return Err(LakeError::Configuration(format!(
            "{LAKE_URL_ENV} file URL must contain an absolute local path"
        )));
    }
    Ok(LakeStoreLocation::Local(root))
}

fn parse_s3_url(configured_url: &str, url: Url) -> Result<LakeStoreLocation, LakeError> {
    if url.port().is_some() {
        return Err(LakeError::Configuration(format!(
            "{LAKE_URL_ENV} S3 URLs cannot contain a port; use AWS_ENDPOINT_URL_S3 for custom endpoints"
        )));
    }
    let bucket = url
        .host_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LakeError::Configuration(format!("{LAKE_URL_ENV} S3 URL is missing a bucket"))
        })?;

    // Use the un-normalized URL path so encoded or literal parent traversal is
    // rejected instead of silently normalized by `url::Url`.
    let without_scheme = configured_url
        .strip_prefix("s3://")
        .ok_or_else(|| LakeError::Configuration(format!("invalid {LAKE_URL_ENV} S3 URL")))?;
    let raw_path = without_scheme
        .find('/')
        .map_or("", |index| &without_scheme[index..]);
    let prefix = ObjectPath::from_url_path(raw_path).map_err(|err| {
        LakeError::Configuration(format!("invalid {LAKE_URL_ENV} S3 prefix: {err}"))
    })?;
    let prefix = (!prefix.as_ref().is_empty()).then(|| prefix.to_string());

    Ok(LakeStoreLocation::S3 {
        bucket: bucket.to_string(),
        prefix,
    })
}
