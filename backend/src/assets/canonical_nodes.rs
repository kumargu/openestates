use std::fmt;

use crate::lake::{LakeError, LakeStore};

use super::kg_view::{KgViewEdgeRecord, KgViewEntityRecord};
use super::rera::{read_edges, read_entities, ReraAssetError};
use super::MaterializationRecord;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalNodeRows {
    pub entities: Vec<KgViewEntityRecord>,
    pub edges: Vec<KgViewEdgeRecord>,
}

pub async fn read_canonical_node_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<CanonicalNodeRows, CanonicalNodesError> {
    let entity_bytes = read_parquet_artifact(lake, record, "entities/part-00000.parquet").await?;
    let edge_bytes = read_parquet_artifact(lake, record, "edges/part-00000.parquet").await?;
    Ok(CanonicalNodeRows {
        entities: read_entities(entity_bytes)?,
        edges: read_edges(edge_bytes)?,
    })
}

async fn read_parquet_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    relative_path: &str,
) -> Result<Vec<u8>, CanonicalNodesError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(relative_path))
        .map(|artifact| artifact.key.clone())
        .ok_or_else(|| CanonicalNodesError::MissingArtifact {
            asset_id: record.asset_id.to_string(),
            path: relative_path.to_string(),
        })?;
    lake.get_bytes(&crate::lake::LakeKey::new(key).map_err(CanonicalNodesError::Key)?)
        .await
        .map_err(CanonicalNodesError::Lake)
}

#[derive(Debug)]
pub enum CanonicalNodesError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Lake(LakeError),
    Key(crate::lake::keys::KeyError),
    Rera(ReraAssetError),
    MissingArtifact { asset_id: String, path: String },
}

impl fmt::Display for CanonicalNodesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "canonical nodes IO error: {err}"),
            Self::Json(err) => write!(f, "canonical nodes JSON error: {err}"),
            Self::Lake(err) => write!(f, "canonical nodes lake error: {err}"),
            Self::Key(err) => write!(f, "canonical nodes key error: {err}"),
            Self::Rera(err) => write!(f, "canonical nodes parquet error: {err}"),
            Self::MissingArtifact { asset_id, path } => {
                write!(f, "missing artifact {path} for {asset_id}")
            }
        }
    }
}

impl std::error::Error for CanonicalNodesError {}

impl From<std::io::Error> for CanonicalNodesError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for CanonicalNodesError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<LakeError> for CanonicalNodesError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<ReraAssetError> for CanonicalNodesError {
    fn from(err: ReraAssetError) -> Self {
        Self::Rera(err)
    }
}
