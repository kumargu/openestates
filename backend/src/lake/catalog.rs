use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::datatypes::Schema;
use datafusion::error::DataFusionError;
use datafusion::prelude::{ParquetReadOptions, SessionConfig, SessionContext};
use url::Url;

use crate::assets::{AssetId, MaterializationId, MaterializationRecord, MaterializationStatus};

use super::pinned_store::PinnedObjectStore;
use super::{LakeError, LakeKey, LakeStore};

const LAKE_OBJECT_STORE_URL: &str = "openestates://lake";
const PARQUET_CONTENT_TYPE: &str = "application/vnd.apache.parquet";

/// Exact lineage for one DataFusion table registered from lake materializations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LakeCatalogTable {
    pub table_name: String,
    pub artifact_suffix: String,
    pub asset_ids: Vec<AssetId>,
    pub materialization_ids: Vec<MaterializationId>,
    pub artifact_keys: Vec<String>,
    pub schema: Schema,
}

/// Manifest-driven SQL catalog over Parquet artifacts in a [`LakeStore`].
pub struct LakeCatalog {
    lake: LakeStore,
    pinned_store: Arc<PinnedObjectStore>,
    context: SessionContext,
    tables: BTreeMap<String, LakeCatalogTable>,
}

#[derive(Debug)]
pub enum LakeCatalogError {
    InvalidTableName(String),
    InvalidArtifactSuffix(String),
    EmptyMaterializations,
    FailedMaterialization {
        asset_id: AssetId,
        materialization_id: MaterializationId,
    },
    MissingArtifact {
        materialization_id: MaterializationId,
        suffix: String,
    },
    AmbiguousArtifact {
        materialization_id: MaterializationId,
        suffix: String,
    },
    InvalidArtifact {
        materialization_id: MaterializationId,
        key: String,
        reason: String,
    },
    IncompatibleSchema {
        key: String,
        expected: String,
        actual: String,
    },
    ConflictingObjectIdentity(String),
    DuplicateArtifact(String),
    TableAlreadyRegistered(String),
    Lake(LakeError),
    DataFusion(DataFusionError),
}

impl LakeCatalog {
    pub fn new(lake: LakeStore) -> Self {
        let mut config = SessionConfig::new();
        config
            .options_mut()
            .execution
            .parquet
            .schema_force_view_types = false;
        let context = SessionContext::new_with_config(config);
        let url = Url::parse(LAKE_OBJECT_STORE_URL).expect("static lake object-store URL is valid");
        let pinned_store = Arc::new(PinnedObjectStore::new(lake.object_store()));
        context.register_object_store(&url, pinned_store.clone());
        Self {
            lake,
            pinned_store,
            context,
            tables: BTreeMap::new(),
        }
    }

    pub fn context(&self) -> &SessionContext {
        &self.context
    }

    pub fn tables(&self) -> &BTreeMap<String, LakeCatalogTable> {
        &self.tables
    }

    /// Register one logical table from the exact materializations supplied by a DAG manifest.
    pub async fn register_parquet_table(
        &mut self,
        table_name: &str,
        materializations: &[MaterializationRecord],
        artifact_suffix: &str,
        expected_schema: &Schema,
    ) -> Result<&LakeCatalogTable, LakeCatalogError> {
        validate_table_name(table_name)?;
        validate_artifact_suffix(artifact_suffix)?;
        if materializations.is_empty() {
            return Err(LakeCatalogError::EmptyMaterializations);
        }
        if self.tables.contains_key(table_name) {
            return Err(LakeCatalogError::TableAlreadyRegistered(
                table_name.to_string(),
            ));
        }

        let mut artifact_keys = Vec::with_capacity(materializations.len());
        let mut seen_keys = HashSet::with_capacity(materializations.len());
        for record in materializations {
            if record.status != MaterializationStatus::Succeeded {
                return Err(LakeCatalogError::FailedMaterialization {
                    asset_id: record.asset_id.clone(),
                    materialization_id: record.materialization_id.clone(),
                });
            }

            let matching: Vec<_> = record
                .artifacts
                .iter()
                .filter(|artifact| key_has_suffix(&artifact.key, artifact_suffix))
                .collect();
            let artifact = match matching.as_slice() {
                [] => {
                    return Err(LakeCatalogError::MissingArtifact {
                        materialization_id: record.materialization_id.clone(),
                        suffix: artifact_suffix.to_string(),
                    });
                }
                [artifact] => *artifact,
                _ => {
                    return Err(LakeCatalogError::AmbiguousArtifact {
                        materialization_id: record.materialization_id.clone(),
                        suffix: artifact_suffix.to_string(),
                    });
                }
            };
            validate_parquet_artifact(record, artifact)?;
            if !seen_keys.insert(artifact.key.clone()) {
                return Err(LakeCatalogError::DuplicateArtifact(artifact.key.clone()));
            }
            let key = LakeKey::new(artifact.key.clone()).map_err(|error| {
                LakeCatalogError::InvalidArtifact {
                    materialization_id: record.materialization_id.clone(),
                    key: artifact.key.clone(),
                    reason: format!("invalid lake key: {error}"),
                }
            })?;
            let identity = self
                .lake
                .verify_artifact(&key, artifact.size_bytes, &artifact.content_hash)
                .await
                .map_err(LakeCatalogError::Lake)?;
            self.pinned_store
                .pin(&key, identity)
                .map_err(LakeCatalogError::ConflictingObjectIdentity)?;
            artifact_keys.push(artifact.key.clone());
        }

        let paths: Vec<_> = artifact_keys
            .iter()
            .map(|key| format!("{LAKE_OBJECT_STORE_URL}/{key}"))
            .collect();
        for (key, path) in artifact_keys.iter().zip(&paths) {
            let frame = self
                .context
                .read_parquet(path, ParquetReadOptions::default())
                .await
                .map_err(LakeCatalogError::DataFusion)?;
            let schema = frame.schema().as_arrow().clone();
            if &schema != expected_schema {
                return Err(LakeCatalogError::IncompatibleSchema {
                    key: key.clone(),
                    expected: format!("{expected_schema:?}"),
                    actual: format!("{schema:?}"),
                });
            }
        }
        let frame = self
            .context
            .read_parquet(paths, ParquetReadOptions::default().schema(expected_schema))
            .await
            .map_err(LakeCatalogError::DataFusion)?;
        self.context
            .register_table(table_name, frame.into_view())
            .map_err(LakeCatalogError::DataFusion)?;

        let table = LakeCatalogTable {
            table_name: table_name.to_string(),
            artifact_suffix: artifact_suffix.to_string(),
            asset_ids: materializations
                .iter()
                .map(|record| record.asset_id.clone())
                .collect(),
            materialization_ids: materializations
                .iter()
                .map(|record| record.materialization_id.clone())
                .collect(),
            artifact_keys,
            schema: expected_schema.clone(),
        };
        Ok(self.tables.entry(table_name.to_string()).or_insert(table))
    }
}

fn validate_table_name(table_name: &str) -> Result<(), LakeCatalogError> {
    let mut characters = table_name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character == '_');
    if !valid_start
        || !characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(LakeCatalogError::InvalidTableName(table_name.to_string()));
    }
    Ok(())
}

fn validate_artifact_suffix(artifact_suffix: &str) -> Result<(), LakeCatalogError> {
    let valid = !artifact_suffix.is_empty()
        && artifact_suffix.trim() == artifact_suffix
        && !artifact_suffix.starts_with('/')
        && !artifact_suffix.contains("//")
        && !artifact_suffix
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        && artifact_suffix.ends_with(".parquet");
    if !valid {
        return Err(LakeCatalogError::InvalidArtifactSuffix(
            artifact_suffix.to_string(),
        ));
    }
    Ok(())
}

fn key_has_suffix(key: &str, suffix: &str) -> bool {
    key == suffix
        || key
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('/'))
}

fn validate_parquet_artifact(
    record: &MaterializationRecord,
    artifact: &crate::assets::ArtifactRef,
) -> Result<(), LakeCatalogError> {
    let invalid = |reason: String| LakeCatalogError::InvalidArtifact {
        materialization_id: record.materialization_id.clone(),
        key: artifact.key.clone(),
        reason,
    };
    let key = LakeKey::new(artifact.key.clone())
        .map_err(|error| invalid(format!("invalid lake key: {error}")))?;
    if key.as_str() != artifact.key {
        return Err(invalid("lake key is not normalized".to_string()));
    }
    if artifact.content_type != PARQUET_CONTENT_TYPE {
        return Err(invalid(format!(
            "expected content type {PARQUET_CONTENT_TYPE}, got {}",
            artifact.content_type
        )));
    }
    if artifact.hash_algorithm != "sha256"
        || artifact.content_hash.len() != 64
        || !artifact
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid("invalid sha256 artifact hash".to_string()));
    }
    if artifact.size_bytes == 0 {
        return Err(invalid("artifact size cannot be zero".to_string()));
    }
    Ok(())
}

impl fmt::Display for LakeCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTableName(name) => write!(f, "invalid lake catalog table name {name:?}"),
            Self::InvalidArtifactSuffix(suffix) => {
                write!(f, "invalid Parquet artifact suffix {suffix:?}")
            }
            Self::EmptyMaterializations => {
                f.write_str("lake catalog table requires at least one materialization")
            }
            Self::FailedMaterialization {
                asset_id,
                materialization_id,
            } => write!(
                f,
                "cannot register failed materialization {materialization_id} for asset {asset_id}"
            ),
            Self::MissingArtifact {
                materialization_id,
                suffix,
            } => write!(
                f,
                "materialization {materialization_id} is missing artifact suffix {suffix:?}"
            ),
            Self::AmbiguousArtifact {
                materialization_id,
                suffix,
            } => write!(
                f,
                "materialization {materialization_id} has multiple artifacts matching suffix {suffix:?}"
            ),
            Self::InvalidArtifact {
                materialization_id,
                key,
                reason,
            } => write!(
                f,
                "invalid artifact {key:?} in materialization {materialization_id}: {reason}"
            ),
            Self::IncompatibleSchema {
                key,
                expected,
                actual,
            } => write!(
                f,
                "Parquet schema for {key:?} does not match table contract: expected {expected}, got {actual}"
            ),
            Self::ConflictingObjectIdentity(message) => f.write_str(message),
            Self::DuplicateArtifact(key) => {
                write!(f, "artifact {key:?} was selected more than once")
            }
            Self::TableAlreadyRegistered(name) => {
                write!(f, "lake catalog table {name:?} is already registered")
            }
            Self::Lake(error) => write!(f, "lake catalog storage error: {error}"),
            Self::DataFusion(error) => write!(f, "DataFusion catalog error: {error}"),
        }
    }
}

impl std::error::Error for LakeCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lake(error) => Some(error),
            Self::DataFusion(error) => Some(error),
            _ => None,
        }
    }
}
