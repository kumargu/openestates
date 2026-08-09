//! Immutable K-RERA receipt primitives for the evidence-graph rebuild.
//!
//! This layer intentionally has no fact keys, buyer labels, or society-name
//! resolution. It preserves source bytes and creates stable registration IDs;
//! later materializers own normalization, claims, and serving projections.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, StringArray};
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
use url::Url;

use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder,
    AssetStage, MaterializationId, MaterializationRecord, SourceWatermark,
};

pub const RERA_RECEIPTS_ASSET_ID: &str = "rera_receipts";
const RERA_RECEIPTS_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraReceiptKind {
    RegistryListing,
    ProjectDetail,
    QuarterlyProgress,
    Document,
}

impl ReraReceiptKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RegistryListing => "registry_listing",
            Self::ProjectDetail => "project_detail",
            Self::QuarterlyProgress => "quarterly_progress",
            Self::Document => "document",
        }
    }

    fn from_str(value: &str) -> Result<Self, ReraEvidenceError> {
        match value {
            "registry_listing" => Ok(Self::RegistryListing),
            "project_detail" => Ok(Self::ProjectDetail),
            "quarterly_progress" => Ok(Self::QuarterlyProgress),
            "document" => Ok(Self::Document),
            other => Err(ReraEvidenceError::InvalidReceiptKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraRegistrationIdentity {
    pub registration_id: String,
    pub normalized_registration_number: String,
}

/// K-RERA registration values are identifier-like and must not be replaced by
/// a project name or acknowledgement number. We reject non-ASCII input here
/// rather than silently constructing an unstable ID; K-RERA's official number
/// format is ASCII and a non-ASCII value needs an upstream normalization fix.
pub fn rera_registration_identity(value: &str) -> Result<ReraRegistrationIdentity, ReraEvidenceError> {
    let normalized_registration_number = normalize_registration_number(value)?;
    let digest = sha256_hex(normalized_registration_number.as_bytes());
    Ok(ReraRegistrationIdentity {
        registration_id: format!("rera_registration:in-ka:{digest}"),
        normalized_registration_number,
    })
}

fn normalize_registration_number(value: &str) -> Result<String, ReraEvidenceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReraEvidenceError::BlankRegistrationNumber);
    }
    if !trimmed.is_ascii() {
        return Err(ReraEvidenceError::NonAsciiRegistrationNumber(trimmed.to_string()));
    }
    let normalized = trimmed
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(ReraEvidenceError::BlankRegistrationNumber);
    }
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReraReceiptInput {
    pub kind: ReraReceiptKind,
    pub source_url: String,
    pub content_type: String,
    pub body: Vec<u8>,
    pub captured_at: DateTime<Utc>,
    pub registration_number: Option<String>,
    pub parent_receipt_id: Option<String>,
    pub crawl_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReceiptRecord {
    pub receipt_id: String,
    pub capture_id: String,
    pub kind: ReraReceiptKind,
    pub source_url: String,
    pub content_type: String,
    pub content_sha256: String,
    pub body_key: String,
    pub captured_at: DateTime<Utc>,
    pub registration_id: Option<String>,
    pub normalized_registration_number: Option<String>,
    pub parent_receipt_id: Option<String>,
    pub crawl_run_id: String,
}

impl ReraReceiptInput {
    pub fn to_record(&self) -> Result<ReraReceiptRecord, ReraEvidenceError> {
        if self.body.is_empty() {
            return Err(ReraEvidenceError::EmptyReceiptBody);
        }
        let source_url = canonical_source_url(&self.source_url)?;
        let content_type = self.content_type.trim().to_ascii_lowercase();
        if content_type.is_empty() {
            return Err(ReraEvidenceError::BlankContentType);
        }
        if self.crawl_run_id.trim().is_empty() {
            return Err(ReraEvidenceError::BlankCrawlRunId);
        }
        let content_sha256 = sha256_hex(&self.body);
        let receipt_id = format!("rera_receipt:sha256:{content_sha256}");
        let capture_material = format!(
            "rera_capture.v1\n{receipt_id}\n{source_url}\n{}",
            self.captured_at.to_rfc3339()
        );
        let capture_id = format!("rera_capture:sha256:{}", sha256_hex(capture_material.as_bytes()));
        let identity = self
            .registration_number
            .as_deref()
            .filter(|number| !number.trim().is_empty())
            .map(rera_registration_identity)
            .transpose()?;
        let body_key = AssetPathBuilder::raw_receipt_key("rera", &content_sha256, "body")
            .to_string();
        Ok(ReraReceiptRecord {
            receipt_id,
            capture_id,
            kind: self.kind,
            source_url,
            content_type,
            content_sha256,
            body_key,
            captured_at: self.captured_at,
            registration_id: identity.as_ref().map(|identity| identity.registration_id.clone()),
            normalized_registration_number: identity
                .as_ref()
                .map(|identity| identity.normalized_registration_number.clone()),
            parent_receipt_id: self.parent_receipt_id.clone(),
            crawl_run_id: self.crawl_run_id.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReraReceiptsInput {
    pub snapshot_date: String,
    pub receipts: Vec<ReraReceiptInput>,
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReceiptsQualityReport {
    pub receipt_count: u64,
    pub unique_body_count: u64,
    pub registration_scoped_capture_count: u64,
    pub unscoped_capture_count: u64,
    pub duplicate_capture_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReceiptsManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub quality: ReraReceiptsQualityReport,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Clone)]
pub struct ReraReceiptsMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ReraReceiptsMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        input: &ReraReceiptsInput,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<MaterializationRecord, ReraEvidenceError> {
        if input.snapshot_date.trim().is_empty() {
            return Err(ReraEvidenceError::BlankSnapshotDate);
        }
        let mut receipt_rows = input
            .receipts
            .iter()
            .map(|receipt| {
                receipt
                    .to_record()
                    .map(|record| (receipt, record))
            })
            .collect::<Result<Vec<_>, ReraEvidenceError>>()?;
        receipt_rows.sort_by(|(_, left), (_, right)| left.capture_id.cmp(&right.capture_id));
        let rows = receipt_rows
            .iter()
            .map(|(_, row)| row.clone())
            .collect::<Vec<_>>();
        let duplicate_capture_ids = duplicate_capture_ids(&rows);
        if !duplicate_capture_ids.is_empty() {
            return Err(ReraEvidenceError::DuplicateCaptureIds(duplicate_capture_ids));
        }

        let mut body_artifacts = BTreeMap::new();
        for (receipt, row) in &receipt_rows {
            let key = AssetPathBuilder::raw_receipt_key("rera", &row.content_sha256, "body");
            let metadata = self.lake.put_bytes(&key, receipt.body.clone()).await?;
            body_artifacts.insert(
                key.to_string(),
                ArtifactRef {
                    key: key.to_string(),
                    content_hash: metadata.content_hash,
                    hash_algorithm: metadata.hash_algorithm,
                    size_bytes: metadata.size_bytes,
                    content_type: row.content_type.clone(),
                },
            );
        }

        let run_id = dag_run_id.to_string();
        let records_key = AssetPathBuilder::raw_snapshot_key(
            "rera",
            &partition,
            &run_id,
            "receipts/part-00000.parquet",
        );
        let records_metadata = self
            .lake
            .put_bytes(&records_key, write_receipt_records(&rows)?)
            .await?;
        let mut artifacts = body_artifacts.into_values().collect::<Vec<_>>();
        artifacts.push(ArtifactRef::parquet(records_metadata));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));
        let quality = ReraReceiptsQualityReport {
            receipt_count: rows.len() as u64,
            unique_body_count: rows
                .iter()
                .map(|row| row.receipt_id.as_str())
                .collect::<BTreeSet<_>>()
                .len() as u64,
            registration_scoped_capture_count: rows
                .iter()
                .filter(|row| row.registration_id.is_some())
                .count() as u64,
            unscoped_capture_count: rows
                .iter()
                .filter(|row| row.registration_id.is_none())
                .count() as u64,
            duplicate_capture_ids: Vec::new(),
        };
        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "rera",
            &partition,
            &run_id,
            "receipts/manifest.json",
        );
        let manifest = ReraReceiptsManifest {
            asset_id: RERA_RECEIPTS_ASSET_ID.to_string(),
            format_version: RERA_RECEIPTS_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            quality,
            artifacts: artifacts.clone(),
        };
        artifacts.push(ArtifactRef::json(self.lake.put_json(&manifest_key, &manifest).await?));
        let record = MaterializationRecord::succeeded(
            AssetId::new(RERA_RECEIPTS_ASSET_ID).expect("static RERA receipt asset id is valid"),
            AssetStage::Raw,
            partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_source_watermarks(input.source_watermarks.clone())
        .with_row_count(rows.len() as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

pub async fn read_rera_receipt_records(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<ReraReceiptRecord>, ReraEvidenceError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("receipts/part-00000.parquet"))
        .ok_or(ReraEvidenceError::MissingReceiptRecordsArtifact)?;
    read_receipt_records(lake.get_bytes(&LakeKey::new(&artifact.key)?).await?)
}

fn canonical_source_url(value: &str) -> Result<String, ReraEvidenceError> {
    let mut url = Url::parse(value).map_err(|_| ReraEvidenceError::InvalidSourceUrl(value.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ReraEvidenceError::InvalidSourceUrl(value.to_string()));
    }
    url.set_fragment(None);
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    if !pairs.is_empty() {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
    }
    Ok(url.into())
}

fn duplicate_capture_ids(rows: &[ReraReceiptRecord]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    rows.iter()
        .filter_map(|row| (!seen.insert(row.capture_id.clone())).then_some(row.capture_id.clone()))
        .collect()
}

fn write_receipt_records(rows: &[ReraReceiptRecord]) -> Result<Vec<u8>, ReraEvidenceError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("receipt_id", DataType::Utf8, false),
        Field::new("capture_id", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, false),
        Field::new("content_type", DataType::Utf8, false),
        Field::new("content_sha256", DataType::Utf8, false),
        Field::new("body_key", DataType::Utf8, false),
        Field::new("captured_at", DataType::Utf8, false),
        Field::new("registration_id", DataType::Utf8, true),
        Field::new("normalized_registration_number", DataType::Utf8, true),
        Field::new("parent_receipt_id", DataType::Utf8, true),
        Field::new("crawl_run_id", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            strings(rows.iter().map(|row| row.receipt_id.clone())),
            strings(rows.iter().map(|row| row.capture_id.clone())),
            strings(rows.iter().map(|row| row.kind.as_str().to_string())),
            strings(rows.iter().map(|row| row.source_url.clone())),
            strings(rows.iter().map(|row| row.content_type.clone())),
            strings(rows.iter().map(|row| row.content_sha256.clone())),
            strings(rows.iter().map(|row| row.body_key.clone())),
            strings(rows.iter().map(|row| row.captured_at.to_rfc3339())),
            optional_strings(rows.iter().map(|row| row.registration_id.clone())),
            optional_strings(rows.iter().map(|row| row.normalized_registration_number.clone())),
            optional_strings(rows.iter().map(|row| row.parent_receipt_id.clone())),
            strings(rows.iter().map(|row| row.crawl_run_id.clone())),
        ],
    )?;
    let mut bytes = Vec::new();
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(bytes)
}

fn read_receipt_records(bytes: Vec<u8>) -> Result<Vec<ReraReceiptRecord>, ReraEvidenceError> {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            rows.push(ReraReceiptRecord {
                receipt_id: required_string(&batch, "receipt_id", row)?,
                capture_id: required_string(&batch, "capture_id", row)?,
                kind: ReraReceiptKind::from_str(&required_string(&batch, "kind", row)?)?,
                source_url: required_string(&batch, "source_url", row)?,
                content_type: required_string(&batch, "content_type", row)?,
                content_sha256: required_string(&batch, "content_sha256", row)?,
                body_key: required_string(&batch, "body_key", row)?,
                captured_at: DateTime::parse_from_rfc3339(&required_string(&batch, "captured_at", row)?)
                    .map_err(ReraEvidenceError::Timestamp)?
                    .with_timezone(&Utc),
                registration_id: optional_string(&batch, "registration_id", row)?,
                normalized_registration_number: optional_string(
                    &batch,
                    "normalized_registration_number",
                    row,
                )?,
                parent_receipt_id: optional_string(&batch, "parent_receipt_id", row)?,
                crawl_run_id: required_string(&batch, "crawl_run_id", row)?,
            });
        }
    }
    Ok(rows)
}

fn strings(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_strings(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn required_string(batch: &RecordBatch, name: &str, row: usize) -> Result<String, ReraEvidenceError> {
    let column = batch
        .column_by_name(name)
        .ok_or_else(|| ReraEvidenceError::MissingColumn(name.to_string()))
        ?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ReraEvidenceError::InvalidColumn(name.to_string()))?;
    if column.is_null(row) {
        return Err(ReraEvidenceError::NullColumn(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Option<String>, ReraEvidenceError> {
    let column = batch
        .column_by_name(name)
        .ok_or_else(|| ReraEvidenceError::MissingColumn(name.to_string()))
        ?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ReraEvidenceError::InvalidColumn(name.to_string()))?;
    Ok((!column.is_null(row)).then(|| column.value(row).to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug)]
pub enum ReraEvidenceError {
    Arrow(arrow::error::ArrowError),
    BlankContentType,
    BlankCrawlRunId,
    BlankRegistrationNumber,
    BlankSnapshotDate,
    DuplicateCaptureIds(Vec<String>),
    EmptyReceiptBody,
    InvalidColumn(String),
    InvalidReceiptKind(String),
    InvalidSourceUrl(String),
    Lake(LakeError),
    MissingColumn(String),
    MissingReceiptRecordsArtifact,
    NonAsciiRegistrationNumber(String),
    NullColumn(String),
    Parquet(parquet::errors::ParquetError),
    Timestamp(chrono::ParseError),
    Key(crate::lake::keys::KeyError),
}

impl fmt::Display for ReraEvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(error) => write!(f, "RERA receipt Arrow error: {error}"),
            Self::BlankContentType => write!(f, "RERA receipt content type is blank"),
            Self::BlankCrawlRunId => write!(f, "RERA receipt crawl run ID is blank"),
            Self::BlankRegistrationNumber => write!(f, "RERA registration number is blank"),
            Self::BlankSnapshotDate => write!(f, "RERA receipt snapshot date is blank"),
            Self::DuplicateCaptureIds(ids) => write!(f, "duplicate RERA capture IDs: {}", ids.join(", ")),
            Self::EmptyReceiptBody => write!(f, "RERA receipt body is empty"),
            Self::InvalidColumn(column) => write!(f, "RERA receipt column {column} has an invalid type"),
            Self::InvalidReceiptKind(kind) => write!(f, "invalid RERA receipt kind {kind}"),
            Self::InvalidSourceUrl(url) => write!(f, "invalid RERA receipt URL {url}"),
            Self::Lake(error) => write!(f, "RERA receipt lake error: {error}"),
            Self::MissingColumn(column) => write!(f, "RERA receipt column {column} is missing"),
            Self::MissingReceiptRecordsArtifact => write!(f, "RERA receipt materialization has no records parquet"),
            Self::NonAsciiRegistrationNumber(value) => write!(f, "RERA registration number must be ASCII after upstream NFKC normalization: {value}"),
            Self::NullColumn(column) => write!(f, "RERA receipt column {column} is null"),
            Self::Parquet(error) => write!(f, "RERA receipt Parquet error: {error}"),
            Self::Timestamp(error) => write!(f, "RERA receipt timestamp error: {error}"),
            Self::Key(error) => write!(f, "RERA receipt lake key error: {error}"),
        }
    }
}

impl std::error::Error for ReraEvidenceError {}

impl From<arrow::error::ArrowError> for ReraEvidenceError {
    fn from(value: arrow::error::ArrowError) -> Self { Self::Arrow(value) }
}

impl From<LakeError> for ReraEvidenceError {
    fn from(value: LakeError) -> Self { Self::Lake(value) }
}

impl From<parquet::errors::ParquetError> for ReraEvidenceError {
    fn from(value: parquet::errors::ParquetError) -> Self { Self::Parquet(value) }
}

impl From<crate::lake::keys::KeyError> for ReraEvidenceError {
    fn from(value: crate::lake::keys::KeyError) -> Self { Self::Key(value) }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use tempfile::tempdir;

    use super::*;

    fn receipt(body: &[u8]) -> ReraReceiptInput {
        ReraReceiptInput {
            kind: ReraReceiptKind::ProjectDetail,
            source_url: "https://rera.karnataka.gov.in/projectDetails?b=2&a=1#discard".to_string(),
            content_type: "text/html".to_string(),
            body: body.to_vec(),
            captured_at: Utc.with_ymd_and_hms(2026, 8, 9, 10, 30, 0).unwrap(),
            registration_number: Some(" prm/ka/rera/1251/446/pr/200811/003528 ".to_string()),
            parent_receipt_id: None,
            crawl_run_id: "rera-backfill-2026-08-09".to_string(),
        }
    }

    #[test]
    fn registration_identity_uses_normalized_official_number_only() {
        let identity = rera_registration_identity(" prm/ka/rera/1251/446/pr/200811/003528 ").unwrap();
        assert_eq!(
            identity.normalized_registration_number,
            "PRM/KA/RERA/1251/446/PR/200811/003528"
        );
        assert!(identity.registration_id.starts_with("rera_registration:in-ka:"));
        assert_eq!(
            identity,
            rera_registration_identity("PRM/KA/RERA/1251/446/PR/200811/003528").unwrap()
        );
        assert!(rera_registration_identity("ＰＲＭ/123").is_err());
    }

    #[test]
    fn receipt_ids_are_content_addressed_and_captures_preserve_url_observation() {
        let first = receipt(b"<html>same evidence</html>").to_record().unwrap();
        let mut second_input = receipt(b"<html>same evidence</html>");
        second_input.source_url = "https://rera.karnataka.gov.in/projectDetails?a=1&b=2".to_string();
        second_input.captured_at = Utc.with_ymd_and_hms(2026, 8, 10, 10, 30, 0).unwrap();
        let second = second_input.to_record().unwrap();
        assert_eq!(first.receipt_id, second.receipt_id);
        assert_ne!(first.capture_id, second.capture_id);
        assert_eq!(
            first.source_url,
            "https://rera.karnataka.gov.in/projectDetails?a=1&b=2"
        );
    }

    #[tokio::test]
    async fn materializer_writes_content_addressed_bodies_and_round_trips_records() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let input = ReraReceiptsInput {
            snapshot_date: "2026-08-09".to_string(),
            receipts: vec![receipt(b"<html>evidence</html>")],
            source_watermarks: vec![SourceWatermark {
                source: "karnataka_rera".to_string(),
                high_watermark: "2026-08-09T10:30:00Z".to_string(),
            }],
        };
        let record = ReraReceiptsMaterializer::new(lake.clone())
            .materialize_for_run(&input, MaterializationId::new(), AssetPartition::global())
            .await
            .unwrap();
        let rows = read_rera_receipt_records(&lake, &record).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            lake.get_bytes(&LakeKey::new(&rows[0].body_key).unwrap()).await.unwrap(),
            input.receipts[0].body
        );
        assert!(record.artifacts.iter().any(|artifact| artifact.key.contains("raw/receipts/source=rera/sha256=")));
    }

    #[tokio::test]
    async fn materializer_rejects_duplicate_captures() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let input = ReraReceiptsInput {
            snapshot_date: "2026-08-09".to_string(),
            receipts: vec![receipt(b"same"), receipt(b"same")],
            source_watermarks: Vec::new(),
        };
        let error = ReraReceiptsMaterializer::new(lake)
            .materialize_for_run(&input, MaterializationId::new(), AssetPartition::global())
            .await
            .unwrap_err();
        assert!(matches!(error, ReraEvidenceError::DuplicateCaptureIds(_)));
    }
}
