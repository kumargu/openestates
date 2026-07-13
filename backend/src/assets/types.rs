use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lake::ArtifactMetadata;

/// Stable identifier for a data product in the OpenEstates asset graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(String);

impl AssetId {
    pub fn new(value: impl Into<String>) -> Result<Self, AssetIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AssetIdError::Empty);
        }
        if !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        {
            return Err(AssetIdError::InvalidCharacters);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetIdError {
    Empty,
    InvalidCharacters,
}

impl std::fmt::Display for AssetIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("asset id cannot be empty"),
            Self::InvalidCharacters => {
                f.write_str("asset id must contain lowercase ASCII, digits, '_' or '-'")
            }
        }
    }
}

impl std::error::Error for AssetIdError {}

/// Stable identifier for one run/materialization record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaterializationId(Uuid);

impl MaterializationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[cfg(test)]
    pub fn fixed(value: impl AsRef<str>) -> Self {
        Self(Uuid::parse_str(value.as_ref()).expect("valid fixed materialization id"))
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MaterializationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MaterializationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Coarse lifecycle stage for an asset artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStage {
    Raw,
    Silver,
    Gold,
    Serving,
}

/// Partition coordinates for an asset materialization.
///
/// We keep the representation ordered so paths and manifests are stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPartition {
    parts: Vec<(String, String)>,
}

impl AssetPartition {
    pub fn new(parts: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        let mut parts: Vec<(String, String)> = parts
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        parts.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
        Self { parts }
    }

    pub fn global() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn path_segments(&self) -> Vec<String> {
        self.parts
            .iter()
            .map(|(key, value)| format!("{}={}", slug_segment(key), slug_segment(value)))
            .collect()
    }

    pub fn is_global(&self) -> bool {
        self.parts.is_empty()
    }
}

impl Default for AssetPartition {
    fn default() -> Self {
        Self::global()
    }
}

/// One artifact produced by a materialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub key: String,
    pub content_hash: String,
    pub hash_algorithm: String,
    pub size_bytes: usize,
    pub content_type: String,
}

impl ArtifactRef {
    pub fn json(meta: ArtifactMetadata) -> Self {
        Self {
            key: meta.key.to_string(),
            content_hash: meta.content_hash,
            hash_algorithm: meta.hash_algorithm,
            size_bytes: meta.size_bytes,
            content_type: "application/json".to_string(),
        }
    }

    pub fn parquet(meta: ArtifactMetadata) -> Self {
        Self {
            key: meta.key.to_string(),
            content_hash: meta.content_hash,
            hash_algorithm: meta.hash_algorithm,
            size_bytes: meta.size_bytes,
            content_type: "application/vnd.apache.parquet".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWatermark {
    pub source: String,
    pub high_watermark: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterializationStatus {
    Succeeded,
    Failed,
}

/// A durable record that says exactly what an asset produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializationRecord {
    pub materialization_id: MaterializationId,
    pub asset_id: AssetId,
    pub stage: AssetStage,
    pub partition: AssetPartition,
    pub version: String,
    pub run_id: MaterializationId,
    pub schema_version: u32,
    pub parent_materializations: Vec<MaterializationId>,
    pub source_watermarks: Vec<SourceWatermark>,
    pub artifacts: Vec<ArtifactRef>,
    pub row_count: u64,
    pub status: MaterializationStatus,
    pub created_at: DateTime<Utc>,
}

impl MaterializationRecord {
    pub fn succeeded(
        asset_id: AssetId,
        stage: AssetStage,
        partition: AssetPartition,
        version: impl Into<String>,
        artifacts: Vec<ArtifactRef>,
    ) -> Self {
        Self {
            materialization_id: MaterializationId::new(),
            asset_id,
            stage,
            partition,
            version: version.into(),
            run_id: MaterializationId::new(),
            schema_version: 1,
            parent_materializations: Vec::new(),
            source_watermarks: Vec::new(),
            artifacts,
            row_count: 0,
            status: MaterializationStatus::Succeeded,
            created_at: Utc::now(),
        }
    }

    pub fn with_parent_materializations(mut self, parents: Vec<MaterializationId>) -> Self {
        self.parent_materializations = parents;
        self
    }

    pub fn with_source_watermarks(mut self, watermarks: Vec<SourceWatermark>) -> Self {
        self.source_watermarks = watermarks;
        self
    }

    pub fn with_row_count(mut self, row_count: u64) -> Self {
        self.row_count = row_count;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentAssetPointer {
    pub asset_id: AssetId,
    pub partition: AssetPartition,
    pub materialization_id: MaterializationId,
    pub materialization_key: String,
    pub version: String,
    pub updated_at: DateTime<Utc>,
}

fn slug_segment(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_segments_are_stable_and_s3_safe() {
        let partition = AssetPartition::new([("dt", "2026-07-12"), ("Source", "RERA KA")]);
        assert_eq!(
            partition.path_segments(),
            vec!["dt=2026-07-12".to_string(), "source=rera-ka".to_string()]
        );
    }

    #[test]
    fn asset_ids_reject_path_like_values() {
        assert!(AssetId::new("rera_registry_monthly").is_ok());
        assert!(AssetId::new("Rera Registry").is_err());
        assert!(AssetId::new("../rera").is_err());
    }
}
