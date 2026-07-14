use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::knowledge::FactValue;

pub const SEARCH_SERVING_BUNDLE_ASSET_ID: &str = "search_serving_bundle";

/// One entity row in the request-path bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServingEntityRecord {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub root_source: Option<String>,
    pub searchable_text: String,
}

/// One fact row in the request-path bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServingFactRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub value_type: String,
    pub value_text: Option<String>,
    pub value: FactValue,
    pub confidence: f32,
    pub source_type: String,
    pub source_url: Option<String>,
    pub model: Option<String>,
    pub skill_id: Option<String>,
    pub learned_at: DateTime<Utc>,
}

/// Search-specific metadata layered over canonical fact rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServingSearchMetadataRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub display_template: Option<String>,
    pub answers_preferences: Vec<String>,
    pub scoring_direction: Option<String>,
    pub scoring_weight: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingBundleSchema {
    pub format_version: u32,
    pub storage_format: String,
    pub fact_schema_registry_version: u32,
    pub tables: Vec<ServingTableSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingTableSchema {
    pub name: String,
    pub path: String,
    pub columns: Vec<ServingColumnSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingColumnSchema {
    pub name: String,
    pub logical_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServingFactIndex {
    by_entity: HashMap<String, ServingEntityFactRows>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServingEntityFactRows {
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
}

impl ServingFactIndex {
    pub fn from_records(
        facts: Vec<ServingFactRecord>,
        search_metadata: Vec<ServingSearchMetadataRecord>,
    ) -> Self {
        let mut by_entity = HashMap::<String, ServingEntityFactRows>::new();
        for fact in facts {
            by_entity
                .entry(fact.entity_id.clone())
                .or_default()
                .facts
                .push(fact);
        }
        for metadata in search_metadata {
            by_entity
                .entry(metadata.entity_id.clone())
                .or_default()
                .search_metadata
                .push(metadata);
        }
        Self { by_entity }
    }

    pub fn entity(&self, entity_id: &str) -> Option<&ServingEntityFactRows> {
        self.by_entity.get(entity_id)
    }

    pub fn entity_count(&self) -> usize {
        self.by_entity.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleArtifactKind {
    EntitiesParquet,
    FactsParquet,
    SearchMetadataParquet,
    SchemaJson,
    TrustPolicyJson,
    TantivyIndexFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleArtifact {
    pub kind: BundleArtifactKind,
    pub key: String,
    pub format: String,
    pub content_hash: String,
    pub hash_algorithm: String,
    pub size_bytes: usize,
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustPolicy {
    pub version: u32,
    pub proof_sources: Vec<String>,
    pub support_sources: Vec<String>,
    pub ai_source_max_confidence: f32,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            proof_sources: vec!["Rera".to_string(), "Bbmp".to_string(), "Manual".to_string()],
            support_sources: vec![
                "Reddit".to_string(),
                "Google".to_string(),
                "News".to_string(),
                "Computed".to_string(),
            ],
            ai_source_max_confidence: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServingBundleManifest {
    pub bundle_version: String,
    pub format_version: u32,
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,
    pub fact_count: u64,
    pub search_metadata_count: u64,
    pub entity_parquet_key: String,
    pub fact_parquet_key: String,
    pub search_metadata_parquet_key: String,
    pub schema_key: String,
    pub trust_policy_key: String,
    pub tantivy_index_prefix: String,
    pub artifacts: Vec<BundleArtifact>,
}
