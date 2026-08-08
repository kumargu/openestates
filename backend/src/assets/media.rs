use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float64Array, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeStore};

use super::{
    read_canonical_society_rows, ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, AssetStage, MaterializationId, MaterializationRecord, ReraAssetError,
    SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const EXTERNAL_IMAGES_WEEKLY_ASSET_ID: &str = "external_images_weekly";
pub const IMAGE_MEDIA_FACTS_ASSET_ID: &str = "image_media_facts";

const EXTERNAL_IMAGE_FORMAT_VERSION: u32 = 1;
const STAGED_MEDIA_PREFIX: &str = "/_staged_media/";
const CONTENT_ADDRESSED_MEDIA_PREFIX: &str = "media/images/sha256";
const MAX_RETAINED_MEDIA_PER_ENTITY: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMediaInventoryEntry {
    pub source_path: String,
    pub media_url: String,
    pub content_sha256: String,
    pub content_type: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalMediaInventory {
    version: u32,
    entries: Vec<LocalMediaInventoryEntry>,
}

#[derive(Debug, Clone)]
struct IngestedMedia {
    media_url: String,
    content_sha256: String,
    content_type: &'static str,
    size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalImageObservationRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub source_name: String,
    pub source_page_url: String,
    pub image_url: String,
    pub original_image_url: Option<String>,
    pub image_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_bucket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_slots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification_method: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub rank: Option<u64>,
    pub score: Option<f64>,
    pub alt_text: Option<String>,
    pub storage_policy: Option<String>,
    pub content_sha256: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalImagesWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<ExternalImageObservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_health: Vec<ExternalImageSourceHealthRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_qa_report: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalImageSourceHealthRecord {
    pub entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crawl_budget: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_interval_hours: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_on_block_hours: Option<u64>,
    pub source_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_page_url: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub candidate_count: u64,
    pub observed_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalImageSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_health: Vec<ExternalImageSourceHealthRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_qa_report: Option<serde_json::Value>,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Clone)]
pub struct MediaAssetMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl MediaAssetMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_external_images(
        &self,
        input: &ExternalImagesWeeklyInput,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<MaterializationRecord, MediaAssetError> {
        let input = self
            .with_current_lake_media(input, &record_partition)
            .await?;
        validate_external_images_input(&input)?;
        let run_id = dag_run_id.to_string();
        let artifact_partition = AssetPartition::new([("dt", input.snapshot_date.as_str())]);
        let parquet_key = AssetPathBuilder::raw_snapshot_key(
            "external_images",
            &artifact_partition,
            &run_id,
            "images/part-00000.parquet",
        );
        let parquet_meta = self
            .lake
            .put_bytes(&parquet_key, write_external_images_parquet(&input.records)?)
            .await?;
        let mut artifacts = vec![ArtifactRef::parquet(parquet_meta)];
        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "external_images",
            &artifact_partition,
            &run_id,
            "manifest.json",
        );
        let manifest = ExternalImageSnapshotManifest {
            asset_id: EXTERNAL_IMAGES_WEEKLY_ASSET_ID.to_string(),
            format_version: EXTERNAL_IMAGE_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id: run_id.clone(),
            created_at: Utc::now(),
            row_count: input.records.len() as u64,
            source_health: input.source_health.clone(),
            media_qa_report: input.media_qa_report.clone(),
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        let record = MaterializationRecord::succeeded(
            asset_id_value(EXTERNAL_IMAGES_WEEKLY_ASSET_ID),
            AssetStage::Raw,
            record_partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(input.source_watermarks.clone())
        .with_row_count(input.records.len() as u64)
        .with_run_id(dag_run_id);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }

    async fn with_current_lake_media(
        &self,
        input: &ExternalImagesWeeklyInput,
        partition: &AssetPartition,
    ) -> Result<ExternalImagesWeeklyInput, MediaAssetError> {
        let asset_id = asset_id_value(EXTERNAL_IMAGES_WEEKLY_ASSET_ID);
        let current = match self
            .materializations
            .current_record(&asset_id, partition)
            .await
        {
            Ok(record) => record,
            Err(error) if error.is_not_found() => return Ok(input.clone()),
            Err(error) => return Err(MediaAssetError::Lake(error)),
        };
        let previous = read_external_image_rows(&self.lake, &current).await?;
        let mut by_entity =
            BTreeMap::<String, BTreeMap<String, ExternalImageObservationRecord>>::new();
        let mut refreshed_sources = HashSet::<(String, String)>::new();
        for record in &input.records {
            refreshed_sources.insert((record.entity_id.clone(), media_source_identity(record)));
            by_entity
                .entry(record.entity_id.clone())
                .or_default()
                .insert(media_identity(record), record.clone());
        }
        for record in previous.into_iter().filter(|record| {
            record.storage_policy.as_deref() == Some("lake_content_addressed")
                && record.image_url.starts_with("/media/images/sha256/")
                && !refreshed_sources
                    .contains(&(record.entity_id.clone(), media_source_identity(record)))
        }) {
            by_entity
                .entry(record.entity_id.clone())
                .or_default()
                .entry(media_identity(&record))
                .or_insert(record);
        }
        let mut records = Vec::new();
        for (_, unique) in by_entity {
            let mut entity_records = unique.into_values().collect::<Vec<_>>();
            sort_image_rows(&mut entity_records);
            entity_records.truncate(MAX_RETAINED_MEDIA_PER_ENTITY);
            records.extend(entity_records);
        }
        let mut merged = input.clone();
        merged.records = records;
        Ok(merged)
    }
}

fn media_identity(record: &ExternalImageObservationRecord) -> String {
    record
        .content_sha256
        .as_deref()
        .map(|hash| format!("sha256:{}", hash.to_ascii_lowercase()))
        .unwrap_or_else(|| format!("url:{}", record.image_url))
}

/// Identify the upstream photo independently from its current delivery bytes.
///
/// Re-encoding an image changes its content hash and immutable lake URL, but it
/// must replace the previous delivery copy rather than consume another gallery
/// slot. Collector observations retain the original URL for this purpose. The
/// source handle fallback keeps locally staged and older records deterministic.
fn media_source_identity(record: &ExternalImageObservationRecord) -> String {
    record
        .original_image_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .map(|url| format!("original:{url}"))
        .unwrap_or_else(|| {
            format!(
                "source:{}|{}|{}",
                record.source_name,
                record.source_page_url,
                record.rank.unwrap_or_default()
            )
        })
}

/// Move local collector output into the durable, backend-neutral media keyspace.
///
/// The source collector may stage files locally, but raw Parquet and all
/// downstream facts only receive immutable `/media/images/sha256/*` URLs. The
/// selected bytes are verified and copied to the configured local/S3 lake.
pub async fn ingest_local_media_assets(
    lake: &LakeStore,
    project_root: &Path,
    input: &ExternalImagesWeeklyInput,
) -> Result<ExternalImagesWeeklyInput, MediaAssetError> {
    let staged_root = project_root.join("data/cache/media_ingest");
    let staged = archive_local_media_tree(
        lake,
        project_root,
        &staged_root,
        STAGED_MEDIA_PREFIX,
        "staged-media",
    )
    .await?;

    let mut normalized = input.clone();
    for record in &mut normalized.records {
        let storage_policy = record.storage_policy.as_deref().unwrap_or_default();
        if storage_policy == "static_public_asset" || record.image_url.starts_with("/societies/") {
            return Err(MediaAssetError::InvalidInput(format!(
                "frontend-packaged media {} is retired; stage it under data/cache/media_ingest",
                record.image_url
            )));
        }
        let ingested = if storage_policy == "staged_local_asset"
            || record.image_url.starts_with(STAGED_MEDIA_PREFIX)
        {
            staged.get(&record.image_url).cloned().ok_or_else(|| {
                MediaAssetError::InvalidInput(format!(
                    "staged media record {} does not resolve under data/cache/media_ingest",
                    record.image_url
                ))
            })?
        } else {
            continue;
        };

        verify_declared_media_hash(record, &ingested.content_sha256)?;
        let previous_url = record.image_url.clone();
        record.image_url = ingested.media_url.clone();
        if record.source_page_url.starts_with(STAGED_MEDIA_PREFIX) {
            record.source_page_url = ingested.media_url.clone();
        }
        if record.original_image_url.as_deref() == Some(previous_url.as_str())
            || record
                .original_image_url
                .as_deref()
                .is_some_and(|url| url.starts_with(STAGED_MEDIA_PREFIX))
        {
            record.original_image_url = Some(ingested.media_url.clone());
        }
        record.storage_policy = Some("lake_content_addressed".to_string());
        record.content_sha256 = Some(ingested.content_sha256.clone());
        record.dedupe_key = Some(format!("sha256:{}", ingested.content_sha256));
    }
    Ok(normalized)
}

fn verify_declared_media_hash(
    record: &ExternalImageObservationRecord,
    actual_sha256: &str,
) -> Result<(), MediaAssetError> {
    let Some(expected) = record.content_sha256.as_deref() else {
        return Ok(());
    };
    if expected.eq_ignore_ascii_case(actual_sha256) {
        return Ok(());
    }
    Err(MediaAssetError::InvalidInput(format!(
        "media {} declared sha256 {}, got {}",
        record.image_url, expected, actual_sha256
    )))
}

async fn archive_local_media_tree(
    lake: &LakeStore,
    project_root: &Path,
    root: &Path,
    url_prefix: &str,
    inventory_name: &str,
) -> Result<HashMap<String, IngestedMedia>, MediaAssetError> {
    let files = local_media_files(root).await?;
    let mut by_url = HashMap::new();
    let mut inventory = Vec::with_capacity(files.len());
    for path in files {
        let relative = path.strip_prefix(root).map_err(|_| {
            MediaAssetError::InvalidInput(format!(
                "media path {} escaped {}",
                path.display(),
                root.display()
            ))
        })?;
        let relative_url = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let source_url = format!("{}{}", url_prefix, relative_url);
        let ingested = ingest_file(lake, &path).await?;
        inventory.push(LocalMediaInventoryEntry {
            source_path: path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string(),
            media_url: ingested.media_url.clone(),
            content_sha256: ingested.content_sha256.clone(),
            content_type: ingested.content_type.to_string(),
            size_bytes: ingested.size_bytes,
        });
        by_url.insert(source_url, ingested);
    }
    if !inventory.is_empty() {
        inventory.sort_by(|left, right| left.source_path.cmp(&right.source_path));
        let key =
            crate::lake::LakeKey::join(&["media", "inventory", &format!("{inventory_name}.json")])?;
        lake.put_json(
            &key,
            &LocalMediaInventory {
                version: 1,
                entries: inventory,
            },
        )
        .await?;
    }
    Ok(by_url)
}

async fn local_media_files(root: &Path) -> Result<Vec<PathBuf>, MediaAssetError> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(MediaAssetError::Io(error)),
        };
        while let Some(entry) = entries.next_entry().await.map_err(MediaAssetError::Io)? {
            let file_type = entry.file_type().await.map_err(MediaAssetError::Io)?;
            if file_type.is_symlink() {
                return Err(MediaAssetError::InvalidInput(format!(
                    "media staging tree cannot contain symlink {}",
                    entry.path().display()
                )));
            }
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

async fn ingest_file(lake: &LakeStore, path: &Path) -> Result<IngestedMedia, MediaAssetError> {
    let bytes = tokio::fs::read(path).await.map_err(MediaAssetError::Io)?;
    ingest_image_bytes(lake, bytes).await
}

async fn ingest_image_bytes(
    lake: &LakeStore,
    bytes: Vec<u8>,
) -> Result<IngestedMedia, MediaAssetError> {
    let (extension, content_type) = canonical_image_format(&bytes).ok_or_else(|| {
        MediaAssetError::InvalidInput("local media bytes are not a supported image".to_string())
    })?;
    let content_sha256 = sha256_hex(&Sha256::digest(&bytes));
    let size_bytes = bytes.len();
    let key = crate::lake::LakeKey::join(&[
        CONTENT_ADDRESSED_MEDIA_PREFIX,
        &content_sha256[..2],
        &format!("{content_sha256}.{extension}"),
    ])?;
    match lake
        .verify_artifact(&key, size_bytes, &content_sha256)
        .await
    {
        Ok(_) => {}
        Err(error) if error.is_not_found() => {
            let metadata = lake.put_bytes(&key, bytes).await?;
            if metadata.content_hash != content_sha256 {
                return Err(MediaAssetError::InvalidInput(format!(
                    "lake write changed media content for {key}"
                )));
            }
        }
        Err(error) => return Err(MediaAssetError::Lake(error)),
    }
    Ok(IngestedMedia {
        media_url: format!("/{key}"),
        content_sha256,
        content_type,
        size_bytes,
    })
}

fn canonical_image_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(("jpg", "image/jpeg"));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("png", "image/png"));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && (&bytes[8..12] == b"avif" || &bytes[8..12] == b"avis")
    {
        return Some(("avif", "image/avif"));
    }
    None
}

pub async fn image_media_facts_input_with_aliases(
    lake: &LakeStore,
    image_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, MediaAssetError> {
    let rows = read_external_image_rows(lake, image_record).await?;
    let canonical = read_canonical_society_rows(lake, canonical_record).await?;
    let aliases = canonical
        .mappings
        .into_iter()
        .filter_map(|mapping| {
            mapping
                .alias_entity_id
                .filter(|alias| alias != &mapping.canonical_entity_id)
                .map(|alias| (mapping.canonical_entity_id, alias))
        })
        .collect::<HashMap<_, _>>();
    let mut rows_by_entity = BTreeMap::<String, Vec<ExternalImageObservationRecord>>::new();
    for row in rows {
        rows_by_entity
            .entry(row.entity_id.clone())
            .or_default()
            .push(row);
    }
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for (entity_id, mut rows) in rows_by_entity {
        sort_image_rows(&mut rows);
        append_image_facts(&entity_id, &rows, run_id, &mut facts, &mut annotations)?;
        if let Some(alias) = aliases.get(&entity_id) {
            append_image_facts(alias, &rows, run_id, &mut facts, &mut annotations)?;
        }
    }
    Ok(SkillFactsInput {
        source: "external_image".to_string(),
        snapshot_date: image_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: image_record.source_watermarks.clone(),
    })
}

async fn read_external_image_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<ExternalImageObservationRecord>, MediaAssetError> {
    let bytes = read_artifact(lake, record, "images/part-00000.parquet").await?;
    let mut rows = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let source_name = string_column(&batch, "source_name")?;
        let source_page_url = string_column(&batch, "source_page_url")?;
        let image_url = string_column(&batch, "image_url")?;
        let original_image_url = string_column(&batch, "original_image_url")?;
        let image_kind = string_column(&batch, "image_kind")?;
        let source_bucket = optional_string_column(&batch, "source_bucket");
        let candidate_kind = optional_string_column(&batch, "candidate_kind");
        let quality_score = optional_f64_column(&batch, "quality_score");
        let relevance_score = optional_f64_column(&batch, "relevance_score");
        let reject_reason = optional_string_column(&batch, "reject_reason");
        let allowed_slots = optional_string_column(&batch, "allowed_slots");
        let dedupe_key = optional_string_column(&batch, "dedupe_key");
        let classification_method = optional_string_column(&batch, "classification_method");
        let width = u64_column(&batch, "width")?;
        let height = u64_column(&batch, "height")?;
        let rank = u64_column(&batch, "rank")?;
        let score = f64_column(&batch, "score")?;
        let alt_text = string_column(&batch, "alt_text")?;
        let storage_policy = string_column(&batch, "storage_policy")?;
        let content_sha256 = string_column(&batch, "content_sha256")?;
        let observed_at = string_column(&batch, "observed_at")?;
        for row in 0..batch.num_rows() {
            rows.push(ExternalImageObservationRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                source_name: required_string(source_name, row, "source_name")?,
                source_page_url: required_string(source_page_url, row, "source_page_url")?,
                image_url: required_string(image_url, row, "image_url")?,
                original_image_url: optional_string(original_image_url, row),
                image_kind: optional_string(image_kind, row),
                source_bucket: optional_string_opt(source_bucket, row),
                candidate_kind: optional_string_opt(candidate_kind, row),
                quality_score: optional_f64_opt(quality_score, row),
                relevance_score: optional_f64_opt(relevance_score, row),
                reject_reason: optional_string_opt(reject_reason, row),
                allowed_slots: parse_allowed_slots(optional_string_opt(allowed_slots, row))?,
                dedupe_key: optional_string_opt(dedupe_key, row),
                classification_method: optional_string_opt(classification_method, row),
                width: optional_u64(width, row),
                height: optional_u64(height, row),
                rank: optional_u64(rank, row),
                score: optional_f64(score, row),
                alt_text: optional_string(alt_text, row),
                storage_policy: optional_string(storage_policy, row),
                content_sha256: optional_string(content_sha256, row),
                observed_at: parse_timestamp(observed_at, row)?,
            });
        }
    }
    Ok(rows)
}

fn write_external_images_parquet(
    records: &[ExternalImageObservationRecord],
) -> Result<Vec<u8>, MediaAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("source_name", DataType::Utf8, false),
        Field::new("source_page_url", DataType::Utf8, false),
        Field::new("image_url", DataType::Utf8, false),
        Field::new("original_image_url", DataType::Utf8, true),
        Field::new("image_kind", DataType::Utf8, true),
        Field::new("source_bucket", DataType::Utf8, true),
        Field::new("candidate_kind", DataType::Utf8, true),
        Field::new("quality_score", DataType::Float64, true),
        Field::new("relevance_score", DataType::Float64, true),
        Field::new("reject_reason", DataType::Utf8, true),
        Field::new("allowed_slots", DataType::Utf8, true),
        Field::new("dedupe_key", DataType::Utf8, true),
        Field::new("classification_method", DataType::Utf8, true),
        Field::new("width", DataType::UInt64, true),
        Field::new("height", DataType::UInt64, true),
        Field::new("rank", DataType::UInt64, true),
        Field::new("score", DataType::Float64, true),
        Field::new("alt_text", DataType::Utf8, true),
        Field::new("storage_policy", DataType::Utf8, true),
        Field::new("content_sha256", DataType::Utf8, true),
        Field::new("observed_at", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            strings(records.iter().map(|record| record.entity_id.clone())),
            optional_strings(records.iter().map(|record| record.project_key.clone())),
            strings(records.iter().map(|record| record.source_name.clone())),
            strings(records.iter().map(|record| record.source_page_url.clone())),
            strings(records.iter().map(|record| record.image_url.clone())),
            optional_strings(
                records
                    .iter()
                    .map(|record| record.original_image_url.clone()),
            ),
            optional_strings(records.iter().map(|record| record.image_kind.clone())),
            optional_strings(records.iter().map(|record| record.source_bucket.clone())),
            optional_strings(records.iter().map(|record| record.candidate_kind.clone())),
            optional_f64s(records.iter().map(|record| record.quality_score)),
            optional_f64s(records.iter().map(|record| record.relevance_score)),
            optional_strings(records.iter().map(|record| record.reject_reason.clone())),
            optional_strings(records.iter().map(|record| {
                if record.allowed_slots.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&record.allowed_slots).unwrap_or_default())
                }
            })),
            optional_strings(records.iter().map(|record| record.dedupe_key.clone())),
            optional_strings(
                records
                    .iter()
                    .map(|record| record.classification_method.clone()),
            ),
            optional_u64s(records.iter().map(|record| record.width)),
            optional_u64s(records.iter().map(|record| record.height)),
            optional_u64s(records.iter().map(|record| record.rank)),
            optional_f64s(records.iter().map(|record| record.score)),
            optional_strings(records.iter().map(|record| record.alt_text.clone())),
            optional_strings(records.iter().map(|record| record.storage_policy.clone())),
            optional_strings(records.iter().map(|record| record.content_sha256.clone())),
            strings(records.iter().map(|record| record.observed_at.to_rfc3339())),
        ],
    )?;
    write_batch(schema, batch)
}

fn validate_external_images_input(
    input: &ExternalImagesWeeklyInput,
) -> Result<(), MediaAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(MediaAssetError::InvalidInput(
            "external image snapshot_date is required".to_string(),
        ));
    }
    if input.records.is_empty()
        && !input
            .source_watermarks
            .iter()
            .any(|watermark| watermark.source.ends_with("_skipped"))
    {
        return Err(MediaAssetError::InvalidInput(
            "external image snapshot cannot be empty unless the source is skipped".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.source_name.trim().is_empty()
            || record.source_page_url.trim().is_empty()
            || record.image_url.trim().is_empty()
        {
            return Err(MediaAssetError::InvalidInput(
                "external image records require entity_id, source_name, source_page_url, and image_url"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

fn sort_image_rows(rows: &mut [ExternalImageObservationRecord]) {
    rows.sort_by(|left, right| {
        left.rank
            .unwrap_or(u64::MAX)
            .cmp(&right.rank.unwrap_or(u64::MAX))
            .then_with(|| {
                right
                    .score
                    .unwrap_or(0.0)
                    .partial_cmp(&left.score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.image_url.cmp(&right.image_url))
    });
}

fn append_image_facts(
    entity_id: &str,
    rows: &[ExternalImageObservationRecord],
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), MediaAssetError> {
    if rows.is_empty() {
        return Ok(());
    }
    let learned_at = rows
        .iter()
        .map(|row| row.observed_at)
        .max()
        .unwrap_or_else(Utc::now);
    let source_url = rows.first().map(|row| row.source_page_url.clone());
    let hero = best_slot_row(rows, "hero");
    if let Some(hero) = hero {
        append_fact(
            entity_id,
            FactValue::Text(hero.image_url.clone()),
            Some(hero.source_page_url.clone()),
            learned_at,
            run_id,
            MediaFactContext {
                fact_key: "hero_image",
                display_template: "Hero image: {value}",
                answers_preferences: &["photos", "image", "project photo", "hero image"],
            },
            MediaFactSink { facts, annotations },
        )?;
    }
    let image_urls = slot_rows(rows, "gallery")
        .into_iter()
        .map(|row| row.image_url.clone())
        .collect::<Vec<_>>();
    if !image_urls.is_empty() {
        append_fact(
            entity_id,
            FactValue::Tags(image_urls),
            source_url.clone(),
            learned_at,
            run_id,
            MediaFactContext {
                fact_key: "images",
                display_template: "Project photos: {value}",
                answers_preferences: &["photos", "gallery", "project images"],
            },
            MediaFactSink { facts, annotations },
        )?;
    }
    append_slot_fact(
        entity_id,
        "floor_plan_images",
        "floor_plan",
        "Floor plan images: {value}",
        &["floor plan", "unit layout"],
        rows,
        source_url.clone(),
        learned_at,
        run_id,
        facts,
        annotations,
    )?;
    append_slot_fact(
        entity_id,
        "site_plan_images",
        "site_plan",
        "Site plan images: {value}",
        &["site plan", "master plan", "project layout"],
        rows,
        source_url.clone(),
        learned_at,
        run_id,
        facts,
        annotations,
    )?;
    append_slot_fact(
        entity_id,
        "location_images",
        "location",
        "Location images: {value}",
        &["location map", "nearby context"],
        rows,
        source_url.clone(),
        learned_at,
        run_id,
        facts,
        annotations,
    )?;
    let gallery = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "image_url": row.image_url,
                "original_image_url": row.original_image_url,
                "source_name": row.source_name,
                "source_page_url": row.source_page_url,
                "image_kind": row.image_kind,
                "source_bucket": row.source_bucket,
                "candidate_kind": row.candidate_kind,
                "quality_score": row.quality_score,
                "relevance_score": row.relevance_score,
                "reject_reason": row.reject_reason,
                "allowed_slots": row.allowed_slots,
                "dedupe_key": row.dedupe_key,
                "classification_method": row.classification_method,
                "width": row.width,
                "height": row.height,
                "rank": row.rank,
                "score": row.score,
                "alt_text": row.alt_text,
                "storage_policy": row.storage_policy,
                "content_sha256": row.content_sha256,
                "observed_at": row.observed_at.to_rfc3339(),
            })
        })
        .collect::<Vec<_>>();
    append_fact(
        entity_id,
        FactValue::Text(serde_json::to_string(&gallery)?),
        source_url.clone(),
        learned_at,
        run_id,
        MediaFactContext {
            fact_key: "image_gallery",
            display_template: "Image gallery: {value}",
            answers_preferences: &["photos", "gallery", "image provenance"],
        },
        MediaFactSink { facts, annotations },
    )?;
    let mut source_pages = rows
        .iter()
        .map(|row| row.source_page_url.clone())
        .collect::<Vec<_>>();
    source_pages.sort();
    source_pages.dedup();
    append_fact(
        entity_id,
        FactValue::Tags(source_pages),
        source_url,
        learned_at,
        run_id,
        MediaFactContext {
            fact_key: "image_source_pages",
            display_template: "Image source pages: {value}",
            answers_preferences: &["photo source", "image attribution", "source"],
        },
        MediaFactSink { facts, annotations },
    )
}

fn append_slot_fact(
    entity_id: &str,
    fact_key: &'static str,
    slot: &str,
    display_template: &'static str,
    answers_preferences: &'static [&'static str],
    rows: &[ExternalImageObservationRecord],
    source_url: Option<String>,
    learned_at: DateTime<Utc>,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), MediaAssetError> {
    let urls = slot_rows(rows, slot)
        .into_iter()
        .map(|row| row.image_url.clone())
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Ok(());
    }
    append_fact(
        entity_id,
        FactValue::Tags(urls),
        source_url,
        learned_at,
        run_id,
        MediaFactContext {
            fact_key,
            display_template,
            answers_preferences,
        },
        MediaFactSink { facts, annotations },
    )
}

fn best_slot_row<'a>(
    rows: &'a [ExternalImageObservationRecord],
    slot: &str,
) -> Option<&'a ExternalImageObservationRecord> {
    slot_rows(rows, slot).into_iter().next()
}

fn slot_rows<'a>(
    rows: &'a [ExternalImageObservationRecord],
    slot: &str,
) -> Vec<&'a ExternalImageObservationRecord> {
    let mut approved = rows
        .iter()
        .filter(|row| row.reject_reason.is_none())
        .filter(|row| row.allowed_slots.iter().any(|allowed| allowed == slot))
        .collect::<Vec<_>>();
    approved.sort_by(|left, right| {
        right
            .quality_score
            .unwrap_or(0.0)
            .partial_cmp(&left.quality_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .relevance_score
                    .unwrap_or(0.0)
                    .partial_cmp(&left.relevance_score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.rank
                    .unwrap_or(u64::MAX)
                    .cmp(&right.rank.unwrap_or(u64::MAX))
            })
            .then_with(|| left.image_url.cmp(&right.image_url))
    });
    approved
}

struct MediaFactContext<'a> {
    fact_key: &'a str,
    display_template: &'a str,
    answers_preferences: &'a [&'a str],
}

struct MediaFactSink<'a> {
    facts: &'a mut Vec<SkillFactRecord>,
    annotations: &'a mut Vec<SkillFactAnnotationRecord>,
}

fn append_fact(
    entity_id: &str,
    value: FactValue,
    source_url: Option<String>,
    learned_at: DateTime<Utc>,
    run_id: &MaterializationId,
    context: MediaFactContext<'_>,
    sink: MediaFactSink<'_>,
) -> Result<(), MediaAssetError> {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    let input_hash = media_fact_hash(entity_id, context.fact_key, &value_json);
    sink.facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: context.fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json,
        confidence: 0.68,
        source_type: "ExternalImage".to_string(),
        source_url,
        model: None,
        skill_id: Some("image_media_facts".to_string()),
        triggered_by: Some("asset_dag".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash,
    });
    sink.annotations.push(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: context.fact_key.to_string(),
        display_template: Some(context.display_template.to_string()),
        answers_preferences_json: serde_json::to_string(context.answers_preferences)?,
        scoring_direction: Some("TextMatch".to_string()),
        scoring_weight: Some(0.2),
        scoring_thresholds_json: "[]".to_string(),
    });
    Ok(())
}

fn media_fact_hash(entity_id: &str, fact_key: &str, value_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entity_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(fact_key.as_bytes());
    hasher.update(b"\0");
    hasher.update(value_json.as_bytes());
    format!("sha256:{}", sha256_hex(&hasher.finalize()))
}

fn write_batch(schema: Arc<Schema>, batch: RecordBatch) -> Result<Vec<u8>, MediaAssetError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(bytes)
}

fn parquet_batches(bytes: Vec<u8>) -> Result<Vec<RecordBatch>, MediaAssetError> {
    ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?
        .build()?
        .map(|batch| batch.map_err(MediaAssetError::Arrow))
        .collect()
}

async fn read_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
) -> Result<Vec<u8>, MediaAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .ok_or_else(|| MediaAssetError::MissingArtifact(record.asset_id.clone()))?;
    let key = crate::lake::LakeKey::new(&artifact.key)?;
    Ok(lake.get_bytes(&key).await?.to_vec())
}

fn strings(values: impl IntoIterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_strings(values: impl IntoIterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_u64s(values: impl IntoIterator<Item = Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_f64s(values: impl IntoIterator<Item = Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, MediaAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| MediaAssetError::InvalidSchema(name.to_string()))
}

fn optional_string_column<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
}

fn u64_column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a UInt64Array, MediaAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| MediaAssetError::InvalidSchema(name.to_string()))
}

fn f64_column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Float64Array, MediaAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| MediaAssetError::InvalidSchema(name.to_string()))
}

fn optional_f64_column<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Float64Array> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
}

fn required_string(
    column: &StringArray,
    row: usize,
    name: &str,
) -> Result<String, MediaAssetError> {
    if column.is_null(row) {
        return Err(MediaAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(column: &StringArray, row: usize) -> Option<String> {
    (!column.is_null(row)).then(|| column.value(row).to_string())
}

fn optional_string_opt(column: Option<&StringArray>, row: usize) -> Option<String> {
    column.and_then(|column| optional_string(column, row))
}

fn optional_u64(column: &UInt64Array, row: usize) -> Option<u64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn optional_f64(column: &Float64Array, row: usize) -> Option<f64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn optional_f64_opt(column: Option<&Float64Array>, row: usize) -> Option<f64> {
    column.and_then(|column| optional_f64(column, row))
}

fn parse_allowed_slots(value: Option<String>) -> Result<Vec<String>, MediaAssetError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return Ok(serde_json::from_str::<Vec<String>>(trimmed)?);
    }
    Ok(trimmed
        .split(',')
        .map(str::trim)
        .filter(|slot| !slot.is_empty())
        .map(str::to_string)
        .collect())
}

fn parse_timestamp(column: &StringArray, row: usize) -> Result<DateTime<Utc>, MediaAssetError> {
    Ok(
        DateTime::parse_from_rfc3339(&required_string(column, row, "observed_at")?)?
            .with_timezone(&Utc),
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn asset_id_value(value: &str) -> AssetId {
    AssetId::new(value).expect("static media asset id is valid")
}

#[derive(Debug)]
pub enum MediaAssetError {
    Arrow(arrow::error::ArrowError),
    Chrono(chrono::ParseError),
    Io(std::io::Error),
    InvalidInput(String),
    InvalidSchema(String),
    MissingArtifact(AssetId),
    Lake(LakeError),
    Parquet(parquet::errors::ParquetError),
    Json(serde_json::Error),
    Rera(ReraAssetError),
    Key(crate::lake::keys::KeyError),
}

impl fmt::Display for MediaAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(error) => write!(f, "media asset Arrow error: {error}"),
            Self::Chrono(error) => write!(f, "media asset timestamp error: {error}"),
            Self::Io(error) => write!(f, "media asset IO error: {error}"),
            Self::InvalidInput(message) => write!(f, "invalid media asset input: {message}"),
            Self::InvalidSchema(column) => write!(f, "invalid media Parquet column: {column}"),
            Self::MissingArtifact(asset_id) => {
                write!(f, "media artifact missing for {asset_id}")
            }
            Self::Lake(error) => write!(f, "media lake error: {error}"),
            Self::Parquet(error) => write!(f, "media Parquet error: {error}"),
            Self::Json(error) => write!(f, "media JSON error: {error}"),
            Self::Rera(error) => write!(f, "media RERA error: {error}"),
            Self::Key(error) => write!(f, "media lake key error: {error}"),
        }
    }
}

impl std::error::Error for MediaAssetError {}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for MediaAssetError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_error!(arrow::error::ArrowError, Arrow);
from_error!(chrono::ParseError, Chrono);
from_error!(LakeError, Lake);
from_error!(parquet::errors::ParquetError, Parquet);
from_error!(serde_json::Error, Json);
from_error!(ReraAssetError, Rera);
from_error!(crate::lake::keys::KeyError, Key);

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn raw_image_materialization_does_not_bypass_executor_promotion() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();
        let partition = AssetPartition::new([("source", "external_image")]);
        let input = ExternalImagesWeeklyInput {
            snapshot_date: "2026-08-01".to_string(),
            records: vec![test_row(
                "https://img.example.com/tower.webp",
                "exterior",
                vec!["hero", "gallery"],
                None,
                Some(0.9),
                Some(0.9),
                Some(1200),
                Some(800),
            )],
            source_health: Vec::new(),
            media_qa_report: None,
            source_watermarks: Vec::new(),
        };

        MediaAssetMaterializer::new(lake.clone())
            .materialize_external_images(
                &input,
                Vec::new(),
                MaterializationId::new(),
                partition.clone(),
            )
            .await
            .unwrap();

        assert!(AssetMaterializationStore::new(lake)
            .current_record(
                &AssetId::new(EXTERNAL_IMAGES_WEEKLY_ASSET_ID).unwrap(),
                &partition,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn local_media_is_content_addressed_deduplicated_and_rewritten() {
        let temp = tempdir().unwrap();
        let project_root = temp.path().join("project");
        let lake = LakeStore::local(temp.path().join("lake")).unwrap();
        let photo_dir = project_root.join("data/cache/media_ingest/societies/example");
        fs::create_dir_all(&photo_dir).unwrap();
        let bytes = b"\xff\xd8\xffshared-jpeg";
        fs::write(photo_dir.join("1.jpg"), bytes).unwrap();
        fs::write(photo_dir.join("2.jpeg"), bytes).unwrap();
        let expected_hash = sha256_hex(&Sha256::digest(bytes));
        let mut first = test_row(
            "/_staged_media/societies/example/1.jpg",
            "exterior",
            vec!["hero", "gallery"],
            None,
            Some(0.9),
            Some(0.9),
            Some(1200),
            Some(800),
        );
        first.storage_policy = Some("staged_local_asset".to_string());
        first.content_sha256 = Some(expected_hash.clone());
        let mut second = first.clone();
        second.image_url = "/_staged_media/societies/example/2.jpeg".to_string();
        let input = ExternalImagesWeeklyInput {
            snapshot_date: "2026-08-07".to_string(),
            records: vec![first, second],
            source_health: Vec::new(),
            media_qa_report: None,
            source_watermarks: Vec::new(),
        };

        let normalized = ingest_local_media_assets(&lake, &project_root, &input)
            .await
            .unwrap();
        let expected_url = format!(
            "/media/images/sha256/{}/{}.jpg",
            &expected_hash[..2],
            expected_hash
        );
        assert!(normalized
            .records
            .iter()
            .all(|record| record.image_url == expected_url));
        assert!(normalized
            .records
            .iter()
            .all(|record| { record.storage_policy.as_deref() == Some("lake_content_addressed") }));
        let keys = lake
            .list_keys(&crate::lake::LakePrefix::new("media/images/sha256").unwrap())
            .await
            .unwrap();
        assert_eq!(keys.len(), 1, "identical bytes must share one lake object");
        let inventory: LocalMediaInventory = lake
            .get_json(&crate::lake::LakeKey::new("media/inventory/staged-media.json").unwrap())
            .await
            .unwrap();
        assert_eq!(inventory.entries.len(), 2);
    }

    #[tokio::test]
    async fn local_media_rejects_a_declared_hash_mismatch() {
        let temp = tempdir().unwrap();
        let project_root = temp.path().join("project");
        let lake = LakeStore::local(temp.path().join("lake")).unwrap();
        let photo_dir = project_root.join("data/cache/media_ingest/societies/example");
        fs::create_dir_all(&photo_dir).unwrap();
        fs::write(photo_dir.join("1.jpg"), b"\xff\xd8\xffjpeg").unwrap();
        let mut row = test_row(
            "/_staged_media/societies/example/1.jpg",
            "exterior",
            vec!["hero"],
            None,
            Some(0.9),
            Some(0.9),
            Some(1200),
            Some(800),
        );
        row.storage_policy = Some("staged_local_asset".to_string());
        row.content_sha256 = Some("0".repeat(64));
        let input = ExternalImagesWeeklyInput {
            snapshot_date: "2026-08-07".to_string(),
            records: vec![row],
            source_health: Vec::new(),
            media_qa_report: None,
            source_watermarks: Vec::new(),
        };

        let error = ingest_local_media_assets(&lake, &project_root, &input)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("declared sha256"));
    }

    #[tokio::test]
    async fn weekly_snapshot_carries_forward_promoted_lake_media() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();
        let partition = AssetPartition::new([("source", "external_image")]);
        let mut retained = test_row(
            "/media/images/sha256/aa/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.webp",
            "exterior",
            vec!["hero", "gallery"],
            None,
            Some(0.9),
            Some(0.9),
            Some(1200),
            Some(800),
        );
        retained.storage_policy = Some("lake_content_addressed".to_string());
        retained.content_sha256 = Some("a".repeat(64));
        retained.original_image_url = Some("https://img.example.com/exterior.jpg".to_string());
        let materializer = MediaAssetMaterializer::new(lake.clone());
        let first = materializer
            .materialize_external_images(
                &ExternalImagesWeeklyInput {
                    snapshot_date: "2026-08-01".to_string(),
                    records: vec![retained],
                    source_health: Vec::new(),
                    media_qa_report: None,
                    source_watermarks: Vec::new(),
                },
                Vec::new(),
                MaterializationId::new(),
                partition.clone(),
            )
            .await
            .unwrap();
        AssetMaterializationStore::new(lake.clone())
            .force_promote_current(&first)
            .await
            .unwrap();

        let mut new_record = test_row(
            "https://img.example.com/new.webp",
            "exterior",
            vec!["hero"],
            None,
            Some(0.8),
            Some(0.8),
            Some(1000),
            Some(700),
        );
        new_record.entity_id = "society:new".to_string();
        let mut replacement = test_row(
            "/media/images/sha256/bb/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.jpg",
            "exterior",
            vec!["hero", "gallery"],
            None,
            Some(0.9),
            Some(0.9),
            Some(1200),
            Some(800),
        );
        replacement.storage_policy = Some("lake_content_addressed".to_string());
        replacement.content_sha256 = Some("b".repeat(64));
        replacement.original_image_url = Some("https://img.example.com/exterior.jpg".to_string());
        let second = materializer
            .materialize_external_images(
                &ExternalImagesWeeklyInput {
                    snapshot_date: "2026-08-08".to_string(),
                    records: vec![new_record, replacement],
                    source_health: Vec::new(),
                    media_qa_report: None,
                    source_watermarks: Vec::new(),
                },
                Vec::new(),
                MaterializationId::new(),
                partition,
            )
            .await
            .unwrap();
        let rows = read_external_image_rows(&lake, &second).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.entity_id == "society:new"));
        assert!(rows.iter().any(|row| {
            row.entity_id == "society:example-green"
                && row.content_sha256.as_deref()
                    == Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        }));
        assert!(!rows.iter().any(|row| row.content_sha256.as_deref()
            == Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")));
    }

    #[test]
    fn media_promotion_excludes_floor_plan_from_hero_and_gallery() {
        let rows = vec![
            test_row(
                "https://img.example.com/floor-plan.webp",
                "floor_plan",
                vec!["floor_plan"],
                None,
                Some(0.95),
                Some(0.93),
                Some(841),
                Some(566),
            ),
            test_row(
                "https://img.example.com/tower.webp",
                "exterior",
                vec!["hero", "gallery"],
                None,
                Some(0.9),
                Some(0.88),
                Some(1200),
                Some(800),
            ),
        ];
        let facts = facts_for(&rows);

        assert_eq!(
            text_fact(&facts, "hero_image").as_deref(),
            Some("https://img.example.com/tower.webp")
        );
        assert_eq!(
            tags_fact(&facts, "images"),
            vec!["https://img.example.com/tower.webp"]
        );
        assert_eq!(
            tags_fact(&facts, "floor_plan_images"),
            vec!["https://img.example.com/floor-plan.webp"]
        );
    }

    #[test]
    fn media_promotion_rejects_unknown_and_thumbnail_hero_candidates() {
        let rows = vec![
            test_row(
                "https://img.example.com/unknown.webp",
                "unknown",
                vec![],
                Some("kind:unknown"),
                Some(0.0),
                Some(0.0),
                Some(1600),
                Some(900),
            ),
            test_row(
                "https://img.example.com/Photo_h300_w450.webp",
                "exterior",
                vec![],
                Some("reject_pattern:Photo_h300_w450"),
                Some(0.0),
                Some(0.0),
                Some(450),
                Some(300),
            ),
            test_row(
                "https://img.example.com/master-plan.webp",
                "site_plan",
                vec!["site_plan"],
                None,
                Some(0.82),
                Some(0.9),
                Some(900),
                Some(500),
            ),
        ];
        let facts = facts_for(&rows);

        assert!(text_fact(&facts, "hero_image").is_none());
        assert!(tags_fact(&facts, "images").is_empty());
        assert_eq!(
            tags_fact(&facts, "site_plan_images"),
            vec!["https://img.example.com/master-plan.webp"]
        );
    }

    fn facts_for(rows: &[ExternalImageObservationRecord]) -> Vec<SkillFactRecord> {
        let mut facts = Vec::new();
        let mut annotations = Vec::new();
        append_image_facts(
            "society:example-green",
            rows,
            &MaterializationId::new(),
            &mut facts,
            &mut annotations,
        )
        .expect("media facts should append");
        facts
    }

    fn text_fact(facts: &[SkillFactRecord], key: &str) -> Option<String> {
        facts
            .iter()
            .find(|fact| fact.fact_key == key)
            .and_then(|fact| serde_json::from_str::<FactValue>(&fact.value_json).ok())
            .and_then(|value| match value {
                FactValue::Text(text) => Some(text),
                _ => None,
            })
    }

    fn tags_fact(facts: &[SkillFactRecord], key: &str) -> Vec<String> {
        facts
            .iter()
            .find(|fact| fact.fact_key == key)
            .and_then(|fact| serde_json::from_str::<FactValue>(&fact.value_json).ok())
            .and_then(|value| match value {
                FactValue::Tags(tags) => Some(tags),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn test_row(
        image_url: &str,
        kind: &str,
        slots: Vec<&str>,
        reject_reason: Option<&str>,
        quality_score: Option<f64>,
        relevance_score: Option<f64>,
        width: Option<u64>,
        height: Option<u64>,
    ) -> ExternalImageObservationRecord {
        ExternalImageObservationRecord {
            entity_id: "society:example-green".to_string(),
            project_key: Some("PRM/KA/RERA/TEST".to_string()),
            source_name: "Fixture".to_string(),
            source_page_url: "https://source.example/project".to_string(),
            image_url: image_url.to_string(),
            original_image_url: Some(image_url.to_string()),
            image_kind: Some(kind.to_string()),
            source_bucket: None,
            candidate_kind: Some(kind.to_string()),
            quality_score,
            relevance_score,
            reject_reason: reject_reason.map(str::to_string),
            allowed_slots: slots.into_iter().map(str::to_string).collect(),
            dedupe_key: Some(format!("url:{image_url}")),
            classification_method: Some("heuristic".to_string()),
            width,
            height,
            rank: Some(1),
            score: Some(80.0),
            alt_text: Some(kind.to_string()),
            storage_policy: Some("link_only".to_string()),
            content_sha256: None,
            observed_at: Utc::now(),
        }
    }
}
