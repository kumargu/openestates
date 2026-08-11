use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::assets::{AssetPathBuilder, MaterializationRecord, MaterializationStatus};
use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{
    read_edges_parquet, read_entities_parquet, read_facts_parquet, read_search_metadata_parquet,
    BundleArtifactKind, ParquetReadError, ServingBundleManifest, ServingFactRecord,
    SEARCH_SERVING_BUNDLE_ASSET_ID,
};

const PUBLIC_MEDIA_PREFIX: &str = "/societies/";
const LAKE_MEDIA_PREFIX: &str = "/media/";
const MEDIA_VALIDATION_CONCURRENCY: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServingBundleValidationIssue {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServingBundleValidationReport {
    pub materialization_id: String,
    pub bundle_version: String,
    pub artifacts_checked: usize,
    pub entity_count: usize,
    pub fact_count: usize,
    pub search_metadata_count: usize,
    pub edge_count: usize,
    pub media_references_checked: usize,
    pub passed: bool,
    pub issues: Vec<ServingBundleValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendMediaAsset {
    pub url: String,
    pub content_sha256: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendMediaManifest {
    pub version: u32,
    pub bundle_version: String,
    pub assets: Vec<FrontendMediaAsset>,
}

#[derive(Debug)]
pub enum ServingBundleValidationError {
    InvalidTarget(String),
    Lake(LakeError),
    Parquet(ParquetReadError),
    Key(crate::lake::keys::KeyError),
}

impl fmt::Display for ServingBundleValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget(message) => formatter.write_str(message),
            Self::Lake(error) => write!(formatter, "serving release lake error: {error}"),
            Self::Parquet(error) => write!(formatter, "serving release Parquet error: {error}"),
            Self::Key(error) => write!(formatter, "serving release key error: {error}"),
        }
    }
}

impl std::error::Error for ServingBundleValidationError {}

impl From<LakeError> for ServingBundleValidationError {
    fn from(error: LakeError) -> Self {
        Self::Lake(error)
    }
}

impl From<ParquetReadError> for ServingBundleValidationError {
    fn from(error: ParquetReadError) -> Self {
        Self::Parquet(error)
    }
}

/// Validate the complete serving release before changing any current pointer.
///
/// Lake artifacts are checked against the hashes in the bundle manifest. Local
/// media URLs are then resolved against the configured local/S3 lake. The
/// retired `/societies/*` frontend path fails promotion.
pub async fn validate_search_serving_candidate(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<ServingBundleValidationReport, ServingBundleValidationError> {
    if record.asset_id.as_str() != SEARCH_SERVING_BUNDLE_ASSET_ID {
        return Err(ServingBundleValidationError::InvalidTarget(format!(
            "serving validation requires {SEARCH_SERVING_BUNDLE_ASSET_ID}, got {}",
            record.asset_id
        )));
    }

    let mut issues = Vec::new();
    if record.status != MaterializationStatus::Succeeded {
        issue(
            &mut issues,
            "materialization_not_succeeded",
            format!("materialization status is {:?}", record.status),
            None,
        );
    }
    for artifact in &record.artifacts {
        let key = match LakeKey::new(artifact.key.clone()) {
            Ok(key) => key,
            Err(error) => {
                issue(
                    &mut issues,
                    "invalid_materialization_artifact_key",
                    error.to_string(),
                    Some(artifact.key.clone()),
                );
                continue;
            }
        };
        if artifact.hash_algorithm != "sha256" {
            issue(
                &mut issues,
                "unsupported_materialization_artifact_hash",
                format!("expected sha256, got {}", artifact.hash_algorithm),
                Some(artifact.key.clone()),
            );
            continue;
        }
        if let Err(error) = lake
            .verify_artifact(&key, artifact.size_bytes, &artifact.content_hash)
            .await
        {
            issue(
                &mut issues,
                "materialization_artifact_integrity_failure",
                error.to_string(),
                Some(artifact.key.clone()),
            );
        }
    }
    let manifest_key = manifest_key_for_record(record)?;
    let manifest: ServingBundleManifest = lake.get_json(&manifest_key).await?;
    if manifest.bundle_version != record.version {
        issue(
            &mut issues,
            "bundle_version_mismatch",
            format!(
                "materialization version {:?} does not match manifest version {:?}",
                record.version, manifest.bundle_version
            ),
            Some(manifest_key.to_string()),
        );
    }

    let expected_manifest_key =
        AssetPathBuilder::serving_bundle_key(&manifest.bundle_version, "manifest.json");
    let expected_prefix = expected_manifest_key
        .as_str()
        .strip_suffix("manifest.json")
        .expect("static manifest suffix should be present")
        .to_string();
    let mut artifact_keys = BTreeSet::new();
    let mut artifact_kinds = BTreeSet::new();
    for artifact in &manifest.artifacts {
        if !artifact_keys.insert(artifact.key.clone()) {
            issue(
                &mut issues,
                "duplicate_bundle_artifact",
                "bundle manifest contains the same artifact more than once",
                Some(artifact.key.clone()),
            );
            continue;
        }
        artifact_kinds.insert(format!("{:?}", artifact.kind));
        if !artifact.key.starts_with(&expected_prefix) {
            issue(
                &mut issues,
                "cross_version_artifact",
                format!("artifact is outside immutable bundle prefix {expected_prefix}"),
                Some(artifact.key.clone()),
            );
        }
        if artifact.hash_algorithm != "sha256" {
            issue(
                &mut issues,
                "unsupported_artifact_hash",
                format!("expected sha256, got {}", artifact.hash_algorithm),
                Some(artifact.key.clone()),
            );
            continue;
        }
        let key = match LakeKey::new(artifact.key.clone()) {
            Ok(key) => key,
            Err(error) => {
                issue(
                    &mut issues,
                    "invalid_artifact_key",
                    error.to_string(),
                    Some(artifact.key.clone()),
                );
                continue;
            }
        };
        if let Err(error) = lake
            .verify_artifact(&key, artifact.size_bytes, &artifact.content_hash)
            .await
        {
            issue(
                &mut issues,
                "artifact_integrity_failure",
                error.to_string(),
                Some(artifact.key.clone()),
            );
        }
    }
    for required in [
        BundleArtifactKind::EntitiesParquet,
        BundleArtifactKind::FactsParquet,
        BundleArtifactKind::EdgesParquet,
        BundleArtifactKind::SearchMetadataParquet,
        BundleArtifactKind::SchemaJson,
        BundleArtifactKind::TrustPolicyJson,
        BundleArtifactKind::TantivyIndexFile,
    ] {
        if !artifact_kinds.contains(&format!("{required:?}")) {
            issue(
                &mut issues,
                "missing_bundle_artifact_kind",
                format!("bundle has no {required:?} artifact"),
                None,
            );
        }
    }
    let mut manifest_table_keys = vec![
        &manifest.entity_parquet_key,
        &manifest.fact_parquet_key,
        &manifest.search_metadata_parquet_key,
        &manifest.schema_key,
        &manifest.trust_policy_key,
    ];
    if let Some(edge_key) = manifest.edge_parquet_key.as_ref() {
        manifest_table_keys.push(edge_key);
    }
    for key in manifest_table_keys {
        if !artifact_keys.contains(key) {
            issue(
                &mut issues,
                "unlisted_bundle_artifact",
                "manifest table key is not present in the hashed artifact inventory",
                Some(key.clone()),
            );
        }
    }
    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == BundleArtifactKind::TantivyIndexFile)
    {
        if !artifact.key.starts_with(&format!(
            "{}/",
            manifest.tantivy_index_prefix.trim_end_matches('/')
        )) {
            issue(
                &mut issues,
                "tantivy_prefix_mismatch",
                "Tantivy artifact is outside the declared index prefix",
                Some(artifact.key.clone()),
            );
        }
    }

    let entities = read_entities_parquet(
        &lake
            .get_bytes(&validated_key(&manifest.entity_parquet_key)?)
            .await?,
    )?;
    let facts = read_facts_parquet(
        &lake
            .get_bytes(&validated_key(&manifest.fact_parquet_key)?)
            .await?,
    )?;
    let metadata = read_search_metadata_parquet(
        &lake
            .get_bytes(&validated_key(&manifest.search_metadata_parquet_key)?)
            .await?,
    )?;
    let edges = match manifest.edge_parquet_key.as_deref() {
        Some(key) => read_edges_parquet(&lake.get_bytes(&validated_key(key)?).await?)?,
        None => Vec::new(),
    };
    check_count(
        &mut issues,
        "entity_count_mismatch",
        manifest.entity_count,
        entities.len(),
    );
    check_count(
        &mut issues,
        "fact_count_mismatch",
        manifest.fact_count,
        facts.len(),
    );
    check_count(
        &mut issues,
        "search_metadata_count_mismatch",
        manifest.search_metadata_count,
        metadata.len(),
    );
    check_count(
        &mut issues,
        "edge_count_mismatch",
        manifest.edge_count,
        edges.len(),
    );

    let media_references = collect_media_references(&facts);
    let media_references_checked = media_references.len();
    let mut media_results = stream::iter(media_references.into_values())
        .map(|reference| async move {
            let reference_url = reference.url.clone();
            let mut reference_issues = Vec::new();
            validate_media_reference(lake, &reference, &mut reference_issues).await;
            (reference_url, reference_issues)
        })
        .buffer_unordered(MEDIA_VALIDATION_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    media_results.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, reference_issues) in media_results {
        issues.extend(reference_issues);
    }

    Ok(ServingBundleValidationReport {
        materialization_id: record.materialization_id.to_string(),
        bundle_version: manifest.bundle_version,
        artifacts_checked: manifest.artifacts.len(),
        entity_count: entities.len(),
        fact_count: facts.len(),
        search_metadata_count: metadata.len(),
        edge_count: edges.len(),
        media_references_checked,
        passed: issues.is_empty(),
        issues,
    })
}

pub fn write_frontend_media_manifest(
    project_root: &Path,
    report: &ServingBundleValidationReport,
) -> Result<std::path::PathBuf, std::io::Error> {
    let path = project_root.join("frontend/media-manifest.json");
    let temporary = project_root.join("frontend/.media-manifest.json.tmp");
    let manifest = FrontendMediaManifest {
        version: 1,
        bundle_version: report.bundle_version.clone(),
        // This inventory is intentionally empty for a valid lake-backed release.
        // Keeping it in the generated certificate makes the frontend build fail
        // closed if packaged property media is ever reintroduced.
        assets: Vec::new(),
    };
    let payload = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(&temporary, payload)?;
    std::fs::rename(&temporary, &path)?;
    Ok(path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MediaReference {
    url: String,
    expected_sha256: Option<String>,
}

fn collect_media_references(facts: &[ServingFactRecord]) -> BTreeMap<String, MediaReference> {
    let mut references = BTreeMap::new();
    for fact in facts {
        match &fact.value {
            FactValue::Text(value) => collect_text_reference(value, None, &mut references),
            FactValue::Tags(values) => {
                for value in values {
                    collect_text_reference(value, None, &mut references);
                }
            }
            FactValue::Score { explanation, .. } => {
                collect_text_reference(explanation, None, &mut references)
            }
            FactValue::Numeric(_) | FactValue::Bool(_) => {}
        }
    }
    references
}

fn collect_text_reference(
    value: &str,
    expected_sha256: Option<&str>,
    references: &mut BTreeMap<String, MediaReference>,
) {
    let trimmed = value.trim();
    if is_media_url(trimmed) {
        insert_media_reference(trimmed, expected_sha256, references);
        return;
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        collect_json_references(&json, references);
    }
}

fn collect_json_references(
    value: &serde_json::Value,
    references: &mut BTreeMap<String, MediaReference>,
) {
    match value {
        serde_json::Value::Object(object) => {
            let expected_sha256 = object
                .get("content_sha256")
                .and_then(serde_json::Value::as_str);
            for child in object.values() {
                if let Some(value) = child.as_str() {
                    if is_media_url(value) {
                        insert_media_reference(value, expected_sha256, references);
                    }
                } else {
                    collect_json_references(child, references);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_json_references(value, references);
            }
        }
        serde_json::Value::String(value) if is_media_url(value) => {
            insert_media_reference(value, None, references)
        }
        _ => {}
    }
}

fn insert_media_reference(
    url: &str,
    expected_sha256: Option<&str>,
    references: &mut BTreeMap<String, MediaReference>,
) {
    let expected_sha256 = expected_sha256.and_then(normalized_sha256);
    references
        .entry(url.to_string())
        .and_modify(|reference| {
            if reference.expected_sha256.is_none() {
                reference.expected_sha256 = expected_sha256.clone();
            }
        })
        .or_insert_with(|| MediaReference {
            url: url.to_string(),
            expected_sha256,
        });
}

async fn validate_media_reference(
    lake: &LakeStore,
    reference: &MediaReference,
    issues: &mut Vec<ServingBundleValidationIssue>,
) {
    if reference.url.starts_with(PUBLIC_MEDIA_PREFIX) {
        issue(
            issues,
            "retired_public_media_path",
            "frontend-packaged society media is retired; use a content-addressed /media URL",
            Some(reference.url.clone()),
        );
        return;
    }
    if let Some(key) = reference.url.strip_prefix('/') {
        let key = match LakeKey::new(key) {
            Ok(key) => key,
            Err(error) => {
                issue(
                    issues,
                    "invalid_lake_media_path",
                    error.to_string(),
                    Some(reference.url.clone()),
                );
                return;
            }
        };
        match lake.get_bytes(&key).await {
            Ok(bytes) => {
                let actual_sha256 = media_sha256(&bytes);
                verify_media_hash(reference, &actual_sha256, issues);
            }
            Err(error) if error.is_not_found() => issue(
                issues,
                "missing_lake_media",
                "lake media object referenced by the bundle does not exist",
                Some(reference.url.clone()),
            ),
            Err(error) => issue(
                issues,
                "lake_media_read_failure",
                error.to_string(),
                Some(reference.url.clone()),
            ),
        }
    }
}

fn verify_media_hash(
    reference: &MediaReference,
    actual: &str,
    issues: &mut Vec<ServingBundleValidationIssue>,
) {
    let Some(expected) = reference.expected_sha256.as_deref() else {
        return;
    };
    if actual != expected {
        issue(
            issues,
            "media_hash_mismatch",
            format!("expected sha256 {expected}, got {actual}"),
            Some(reference.url.clone()),
        );
    }
}

fn media_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalized_sha256(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_prefix("sha256:").unwrap_or(&value);
    (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_string())
}

fn is_media_url(value: &str) -> bool {
    value.starts_with(PUBLIC_MEDIA_PREFIX) || value.starts_with(LAKE_MEDIA_PREFIX)
}

fn manifest_key_for_record(
    record: &MaterializationRecord,
) -> Result<LakeKey, ServingBundleValidationError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.content_type == "application/json" && artifact.key.ends_with("/manifest.json")
        })
        .map(|artifact| artifact.key.clone())
        .unwrap_or_else(|| {
            AssetPathBuilder::serving_bundle_key(&record.version, "manifest.json").to_string()
        });
    validated_key(&key)
}

fn validated_key(value: &str) -> Result<LakeKey, ServingBundleValidationError> {
    LakeKey::new(value.to_string()).map_err(ServingBundleValidationError::Key)
}

fn check_count(
    issues: &mut Vec<ServingBundleValidationIssue>,
    code: &str,
    expected: u64,
    actual: usize,
) {
    if expected != actual as u64 {
        issue(
            issues,
            code,
            format!("manifest declares {expected}, artifact contains {actual}"),
            None,
        );
    }
}

fn issue(
    issues: &mut Vec<ServingBundleValidationIssue>,
    code: impl Into<String>,
    message: impl Into<String>,
    reference: Option<String>,
) {
    issues.push(ServingBundleValidationIssue {
        code: code.into(),
        message: message.into(),
        reference,
    });
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn fact(value: FactValue) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: "society:test".to_string(),
            fact_key: "test_media".to_string(),
            value_type: "text".to_string(),
            value_text: None,
            value,
            confidence: 1.0,
            source_type: "Manual".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc::now(),
        }
    }

    #[test]
    fn collects_direct_tag_and_gallery_media_without_fact_key_branches() {
        let hash = "a".repeat(64);
        let gallery = serde_json::json!([{
            "image_url": "/societies/test/1.jpg",
            "content_sha256": format!("sha256:{hash}"),
            "source_page_url": "https://example.com/project"
        }]);
        let references = collect_media_references(&[
            fact(FactValue::Text("/media/test/hero.webp".to_string())),
            fact(FactValue::Tags(vec![
                "/societies/test/2.jpg".to_string(),
                "https://example.com/remote.jpg".to_string(),
            ])),
            fact(FactValue::Text(gallery.to_string())),
        ]);

        assert_eq!(references.len(), 3);
        assert_eq!(
            references["/societies/test/1.jpg"].expected_sha256,
            Some(hash)
        );
        assert!(!references.contains_key("https://example.com/remote.jpg"));
    }
}
