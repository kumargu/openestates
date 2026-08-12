//! Typed, lineage-complete RERA source records.
//!
//! This is L1 of the evidence rebuild. It deliberately preserves source
//! language and parser output without projecting buyer-facing facts or claims.

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

use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{
    read_rera_receipt_records, rera_registration_identity, ArtifactRef, AssetId,
    AssetMaterializationStore, AssetPartition, AssetPathBuilder, MaterializationId,
    MaterializationRecord, ReraEvidenceError, SourceWatermark,
};

pub const RERA_SOURCE_RECORDS_ASSET_ID: &str = "rera_source_records";
const RERA_SOURCE_RECORDS_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraSourceRecordKind {
    RegistrationSummary,
    RegistrationRelation,
    PromoterDeclaration,
    Completion,
    QuarterlyProgress,
    Inventory,
    TowerInventory,
    ComplaintOrder,
    DocumentApproval,
    FinanceDeclaration,
    WaterServiceDeclaration,
    SourceWarning,
    Unknown,
}

impl ReraSourceRecordKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RegistrationSummary => "registration_summary",
            Self::RegistrationRelation => "registration_relation",
            Self::PromoterDeclaration => "promoter_declaration",
            Self::Completion => "completion",
            Self::QuarterlyProgress => "quarterly_progress",
            Self::Inventory => "inventory",
            Self::TowerInventory => "tower_inventory",
            Self::ComplaintOrder => "complaint_order",
            Self::DocumentApproval => "document_approval",
            Self::FinanceDeclaration => "finance_declaration",
            Self::WaterServiceDeclaration => "water_service_declaration",
            Self::SourceWarning => "source_warning",
            Self::Unknown => "unknown",
        }
    }

    fn from_str(value: &str) -> Result<Self, ReraSourceRecordsError> {
        match value {
            "registration_summary" => Ok(Self::RegistrationSummary),
            "registration_relation" => Ok(Self::RegistrationRelation),
            "promoter_declaration" => Ok(Self::PromoterDeclaration),
            "completion" => Ok(Self::Completion),
            "quarterly_progress" => Ok(Self::QuarterlyProgress),
            "inventory" => Ok(Self::Inventory),
            "tower_inventory" => Ok(Self::TowerInventory),
            "complaint_order" => Ok(Self::ComplaintOrder),
            "document_approval" => Ok(Self::DocumentApproval),
            "finance_declaration" => Ok(Self::FinanceDeclaration),
            "water_service_declaration" => Ok(Self::WaterServiceDeclaration),
            "source_warning" => Ok(Self::SourceWarning),
            "unknown" => Ok(Self::Unknown),
            other => Err(ReraSourceRecordsError::InvalidRecordKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraSourceRecordInput {
    pub kind: ReraSourceRecordKind,
    pub registration_number: String,
    pub receipt_id: String,
    pub capture_id: String,
    pub source_locator: String,
    pub raw_label: String,
    pub raw_value: String,
    pub observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filing_at: Option<String>,
    pub parser_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraSourceRecord {
    pub record_id: String,
    pub kind: ReraSourceRecordKind,
    pub registration_id: String,
    pub normalized_registration_number: String,
    pub receipt_id: String,
    pub capture_id: String,
    pub source_locator: String,
    pub raw_label: String,
    pub raw_value: String,
    pub observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filing_at: Option<String>,
    pub parser_version: String,
}

impl ReraSourceRecordInput {
    fn normalize(self) -> Result<ReraSourceRecord, ReraSourceRecordsError> {
        let identity = rera_registration_identity(&self.registration_number)?;
        for (field, value) in [
            ("receipt_id", self.receipt_id.as_str()),
            ("capture_id", self.capture_id.as_str()),
            ("source_locator", self.source_locator.as_str()),
            ("raw_label", self.raw_label.as_str()),
            ("parser_version", self.parser_version.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ReraSourceRecordsError::BlankRequiredField(field));
            }
        }
        let material = format!(
            "rera_source_record.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.kind.as_str(),
            identity.registration_id,
            self.receipt_id,
            self.capture_id,
            self.source_locator,
            self.raw_label,
            self.raw_value,
            self.observed_at.to_rfc3339(),
            self.effective_at.as_deref().unwrap_or_default(),
            self.filing_at.as_deref().unwrap_or_default(),
            self.parser_version,
        );
        Ok(ReraSourceRecord {
            record_id: format!(
                "rera_source_record:sha256:{}",
                sha256_hex(material.as_bytes())
            ),
            kind: self.kind,
            registration_id: identity.registration_id,
            normalized_registration_number: identity.normalized_registration_number,
            receipt_id: self.receipt_id,
            capture_id: self.capture_id,
            source_locator: self.source_locator,
            raw_label: self.raw_label,
            raw_value: self.raw_value,
            observed_at: self.observed_at,
            effective_at: self.effective_at,
            filing_at: self.filing_at,
            parser_version: self.parser_version,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraSourceRecordsInput {
    pub snapshot_date: String,
    pub records: Vec<ReraSourceRecordInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraSourceRecordsQualityReport {
    pub record_count: u64,
    pub record_counts_by_kind: BTreeMap<String, u64>,
    pub unknown_labels: Vec<String>,
    pub duplicate_record_ids: Vec<String>,
    pub lineage_failures: Vec<String>,
    pub parser_versions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraSourceRecordsManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub quality: ReraSourceRecordsQualityReport,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Clone)]
pub struct ReraSourceRecordsMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ReraSourceRecordsMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        input: &ReraSourceRecordsInput,
        receipts_record: &MaterializationRecord,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<MaterializationRecord, ReraSourceRecordsError> {
        if input.snapshot_date.trim().is_empty() {
            return Err(ReraSourceRecordsError::BlankSnapshotDate);
        }
        if input.records.is_empty() {
            return Err(ReraSourceRecordsError::EmptyRecords);
        }
        let receipts = read_rera_receipt_records(&self.lake, receipts_record).await?;
        let receipt_lineage = receipts
            .into_iter()
            .map(|receipt| {
                (
                    (receipt.receipt_id, receipt.capture_id),
                    receipt.registration_id,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut rows = input
            .records
            .clone()
            .into_iter()
            .map(ReraSourceRecordInput::normalize)
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by(|left, right| left.record_id.cmp(&right.record_id));
        let duplicate_record_ids = duplicate_record_ids(&rows);
        if !duplicate_record_ids.is_empty() {
            return Err(ReraSourceRecordsError::DuplicateRecordIds(
                duplicate_record_ids,
            ));
        }

        let mut lineage_failures = Vec::new();
        for row in &rows {
            match receipt_lineage.get(&(row.receipt_id.clone(), row.capture_id.clone())) {
                None => {
                    lineage_failures.push(format!("{}: missing receipt/capture", row.record_id))
                }
                Some(Some(receipt_registration_id))
                    if receipt_registration_id != &row.registration_id =>
                {
                    lineage_failures
                        .push(format!("{}: registration scope mismatch", row.record_id));
                }
                Some(_) => {}
            }
        }
        if !lineage_failures.is_empty() {
            return Err(ReraSourceRecordsError::LineageFailures(lineage_failures));
        }

        let run_id = dag_run_id.to_string();
        let mut artifacts = Vec::new();
        for kind in all_record_kinds() {
            let kind_rows = rows
                .iter()
                .filter(|row| row.kind == kind)
                .cloned()
                .collect::<Vec<_>>();
            if kind_rows.is_empty() {
                continue;
            }
            let key = AssetPathBuilder::silver_asset_key(
                RERA_SOURCE_RECORDS_ASSET_ID,
                "rera",
                &input.snapshot_date,
                &run_id,
                &format!("records/{}/part-00000.parquet", kind.as_str()),
            );
            artifacts.push(ArtifactRef::parquet(
                self.lake
                    .put_bytes(&key, write_source_records(&kind_rows)?)
                    .await?,
            ));
        }
        let quality = quality_report(&rows);
        let manifest_key = AssetPathBuilder::silver_asset_key(
            RERA_SOURCE_RECORDS_ASSET_ID,
            "rera",
            &input.snapshot_date,
            &run_id,
            "manifest.json",
        );
        let manifest = ReraSourceRecordsManifest {
            asset_id: RERA_SOURCE_RECORDS_ASSET_ID.to_string(),
            format_version: RERA_SOURCE_RECORDS_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            quality,
            artifacts: artifacts.clone(),
        };
        artifacts.push(ArtifactRef::json(
            self.lake.put_json(&manifest_key, &manifest).await?,
        ));
        let record = MaterializationRecord::succeeded(
            AssetId::new(RERA_SOURCE_RECORDS_ASSET_ID)
                .expect("static RERA source asset id is valid"),
            super::AssetStage::Silver,
            partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(vec![receipts_record.materialization_id.clone()])
        .with_source_watermarks(input.source_watermarks.clone())
        .with_row_count(rows.len() as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

pub async fn read_rera_source_records(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<ReraSourceRecord>, ReraSourceRecordsError> {
    let mut artifacts = record
        .artifacts
        .iter()
        .filter(|artifact| artifact.key.contains("/records/") && artifact.key.ends_with(".parquet"))
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.key.cmp(&right.key));
    let mut rows = Vec::new();
    for artifact in artifacts {
        rows.extend(read_source_records(
            lake.get_bytes(&LakeKey::new(&artifact.key)?).await?,
        )?);
    }
    rows.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    Ok(rows)
}

fn all_record_kinds() -> [ReraSourceRecordKind; 13] {
    [
        ReraSourceRecordKind::RegistrationSummary,
        ReraSourceRecordKind::RegistrationRelation,
        ReraSourceRecordKind::PromoterDeclaration,
        ReraSourceRecordKind::Completion,
        ReraSourceRecordKind::QuarterlyProgress,
        ReraSourceRecordKind::Inventory,
        ReraSourceRecordKind::TowerInventory,
        ReraSourceRecordKind::ComplaintOrder,
        ReraSourceRecordKind::DocumentApproval,
        ReraSourceRecordKind::FinanceDeclaration,
        ReraSourceRecordKind::WaterServiceDeclaration,
        ReraSourceRecordKind::SourceWarning,
        ReraSourceRecordKind::Unknown,
    ]
}

fn quality_report(rows: &[ReraSourceRecord]) -> ReraSourceRecordsQualityReport {
    let mut record_counts_by_kind = BTreeMap::new();
    let mut unknown_labels = BTreeSet::new();
    let mut parser_versions = BTreeSet::new();
    for row in rows {
        *record_counts_by_kind
            .entry(row.kind.as_str().to_string())
            .or_insert(0) += 1;
        if row.kind == ReraSourceRecordKind::Unknown {
            unknown_labels.insert(row.raw_label.clone());
        }
        parser_versions.insert(row.parser_version.clone());
    }
    ReraSourceRecordsQualityReport {
        record_count: rows.len() as u64,
        record_counts_by_kind,
        unknown_labels: unknown_labels.into_iter().collect(),
        duplicate_record_ids: Vec::new(),
        lineage_failures: Vec::new(),
        parser_versions: parser_versions.into_iter().collect(),
    }
}

fn duplicate_record_ids(rows: &[ReraSourceRecord]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    rows.iter()
        .filter_map(|row| (!seen.insert(row.record_id.clone())).then_some(row.record_id.clone()))
        .collect()
}

fn write_source_records(rows: &[ReraSourceRecord]) -> Result<Vec<u8>, ReraSourceRecordsError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("record_id", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("registration_id", DataType::Utf8, false),
        Field::new("normalized_registration_number", DataType::Utf8, false),
        Field::new("receipt_id", DataType::Utf8, false),
        Field::new("capture_id", DataType::Utf8, false),
        Field::new("source_locator", DataType::Utf8, false),
        Field::new("raw_label", DataType::Utf8, false),
        Field::new("raw_value", DataType::Utf8, false),
        Field::new("observed_at", DataType::Utf8, false),
        Field::new("effective_at", DataType::Utf8, true),
        Field::new("filing_at", DataType::Utf8, true),
        Field::new("parser_version", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            strings(rows.iter().map(|row| row.record_id.clone())),
            strings(rows.iter().map(|row| row.kind.as_str().to_string())),
            strings(rows.iter().map(|row| row.registration_id.clone())),
            strings(
                rows.iter()
                    .map(|row| row.normalized_registration_number.clone()),
            ),
            strings(rows.iter().map(|row| row.receipt_id.clone())),
            strings(rows.iter().map(|row| row.capture_id.clone())),
            strings(rows.iter().map(|row| row.source_locator.clone())),
            strings(rows.iter().map(|row| row.raw_label.clone())),
            strings(rows.iter().map(|row| row.raw_value.clone())),
            strings(rows.iter().map(|row| row.observed_at.to_rfc3339())),
            optional_strings(rows.iter().map(|row| row.effective_at.clone())),
            optional_strings(rows.iter().map(|row| row.filing_at.clone())),
            strings(rows.iter().map(|row| row.parser_version.clone())),
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

fn read_source_records(bytes: Vec<u8>) -> Result<Vec<ReraSourceRecord>, ReraSourceRecordsError> {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            rows.push(ReraSourceRecord {
                record_id: required_string(&batch, "record_id", row)?,
                kind: ReraSourceRecordKind::from_str(&required_string(&batch, "kind", row)?)?,
                registration_id: required_string(&batch, "registration_id", row)?,
                normalized_registration_number: required_string(
                    &batch,
                    "normalized_registration_number",
                    row,
                )?,
                receipt_id: required_string(&batch, "receipt_id", row)?,
                capture_id: required_string(&batch, "capture_id", row)?,
                source_locator: required_string(&batch, "source_locator", row)?,
                raw_label: required_string(&batch, "raw_label", row)?,
                raw_value: required_string(&batch, "raw_value", row)?,
                observed_at: DateTime::parse_from_rfc3339(&required_string(
                    &batch,
                    "observed_at",
                    row,
                )?)?
                .with_timezone(&Utc),
                effective_at: optional_string(&batch, "effective_at", row)?,
                filing_at: optional_string(&batch, "filing_at", row)?,
                parser_version: required_string(&batch, "parser_version", row)?,
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

fn required_string(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<String, ReraSourceRecordsError> {
    let column = string_column(batch, name)?;
    if column.is_null(row) {
        return Err(ReraSourceRecordsError::NullColumn(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Option<String>, ReraSourceRecordsError> {
    let column = string_column(batch, name)?;
    Ok((!column.is_null(row)).then(|| column.value(row).to_string()))
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, ReraSourceRecordsError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| ReraSourceRecordsError::MissingColumn(name.to_string()))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ReraSourceRecordsError::InvalidColumn(name.to_string()))
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
pub enum ReraSourceRecordsError {
    Arrow(arrow::error::ArrowError),
    BlankRequiredField(&'static str),
    BlankSnapshotDate,
    DuplicateRecordIds(Vec<String>),
    EmptyRecords,
    Evidence(ReraEvidenceError),
    InvalidColumn(String),
    InvalidRecordKind(String),
    Key(crate::lake::keys::KeyError),
    Lake(LakeError),
    LineageFailures(Vec<String>),
    MissingColumn(String),
    NullColumn(String),
    Parquet(parquet::errors::ParquetError),
    Timestamp(chrono::ParseError),
}

impl fmt::Display for ReraSourceRecordsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(error) => write!(f, "RERA source record Arrow error: {error}"),
            Self::BlankRequiredField(field) => write!(f, "RERA source record {field} is blank"),
            Self::BlankSnapshotDate => write!(f, "RERA source record snapshot date is blank"),
            Self::DuplicateRecordIds(ids) => {
                write!(f, "duplicate RERA source records: {}", ids.join(", "))
            }
            Self::EmptyRecords => write!(f, "RERA source record input is empty"),
            Self::Evidence(error) => write!(f, "RERA source record evidence error: {error}"),
            Self::InvalidColumn(column) => {
                write!(f, "RERA source record column {column} has an invalid type")
            }
            Self::InvalidRecordKind(kind) => write!(f, "invalid RERA source record kind {kind}"),
            Self::Key(error) => write!(f, "RERA source record lake key error: {error}"),
            Self::Lake(error) => write!(f, "RERA source record lake error: {error}"),
            Self::LineageFailures(failures) => write!(
                f,
                "RERA source record lineage failures: {}",
                failures.join(", ")
            ),
            Self::MissingColumn(column) => {
                write!(f, "RERA source record column {column} is missing")
            }
            Self::NullColumn(column) => write!(f, "RERA source record column {column} is null"),
            Self::Parquet(error) => write!(f, "RERA source record Parquet error: {error}"),
            Self::Timestamp(error) => write!(f, "RERA source record timestamp error: {error}"),
        }
    }
}

impl std::error::Error for ReraSourceRecordsError {}

impl From<ReraEvidenceError> for ReraSourceRecordsError {
    fn from(value: ReraEvidenceError) -> Self {
        Self::Evidence(value)
    }
}

impl From<arrow::error::ArrowError> for ReraSourceRecordsError {
    fn from(value: arrow::error::ArrowError) -> Self {
        Self::Arrow(value)
    }
}

impl From<LakeError> for ReraSourceRecordsError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}

impl From<parquet::errors::ParquetError> for ReraSourceRecordsError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        Self::Parquet(value)
    }
}

impl From<chrono::ParseError> for ReraSourceRecordsError {
    fn from(value: chrono::ParseError) -> Self {
        Self::Timestamp(value)
    }
}

impl From<crate::lake::keys::KeyError> for ReraSourceRecordsError {
    fn from(value: crate::lake::keys::KeyError) -> Self {
        Self::Key(value)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::assets::{
        AssetPartition, ReraReceiptInput, ReraReceiptKind, ReraReceiptsInput,
        ReraReceiptsMaterializer,
    };

    fn source_input(
        receipt_id: String,
        capture_id: String,
        now: DateTime<Utc>,
    ) -> ReraSourceRecordsInput {
        ReraSourceRecordsInput {
            snapshot_date: "2026-08-09".to_string(),
            records: vec![
                ReraSourceRecordInput {
                    kind: ReraSourceRecordKind::RegistrationSummary,
                    registration_number: "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                    receipt_id: receipt_id.clone(),
                    capture_id: capture_id.clone(),
                    source_locator: "applicationNameList2[0]".to_string(),
                    raw_label: "Registration Number".to_string(),
                    raw_value: "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                    observed_at: now,
                    effective_at: None,
                    filing_at: None,
                    parser_version: "rera_listing.v1".to_string(),
                },
                ReraSourceRecordInput {
                    kind: ReraSourceRecordKind::Unknown,
                    registration_number: "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                    receipt_id: receipt_id.clone(),
                    capture_id: capture_id.clone(),
                    source_locator: "futureField[0]".to_string(),
                    raw_label: "Future field".to_string(),
                    raw_value: "preserved raw value".to_string(),
                    observed_at: now,
                    effective_at: None,
                    filing_at: None,
                    parser_version: "rera_listing.v1".to_string(),
                },
                ReraSourceRecordInput {
                    kind: ReraSourceRecordKind::Inventory,
                    registration_number: "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                    receipt_id,
                    capture_id,
                    source_locator: "#menu2/development-inventory/row-1".to_string(),
                    raw_label: "Declared inventory configuration".to_string(),
                    raw_value: r#"{"inventory_type":"2BHK+2T","unit_count":42}"#.to_string(),
                    observed_at: now,
                    effective_at: None,
                    filing_at: None,
                    parser_version: "rera_project_detail_source_records.v1".to_string(),
                },
            ],
            source_watermarks: Vec::new(),
        }
    }

    #[tokio::test]
    async fn materializes_separate_typed_tables_with_receipt_lineage() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
        let receipt_input = ReraReceiptsInput {
            snapshot_date: "2026-08-09".to_string(),
            receipts: vec![ReraReceiptInput {
                kind: ReraReceiptKind::ProjectDetail,
                source_url: "https://rera.karnataka.gov.in/projectDetails?id=1".to_string(),
                content_type: "text/html".to_string(),
                body: b"<html>fixture</html>".to_vec(),
                captured_at: now,
                registration_number: Some("PRM/KA/RERA/1251/446/PR/200811/003528".to_string()),
                parent_receipt_id: None,
                crawl_run_id: "fixture".to_string(),
            }],
            source_watermarks: Vec::new(),
        };
        let partition = AssetPartition::global();
        let receipt_record = ReraReceiptsMaterializer::new(lake.clone())
            .materialize_for_run(&receipt_input, MaterializationId::new(), partition.clone())
            .await
            .unwrap();
        let receipt = read_rera_receipt_records(&lake, &receipt_record)
            .await
            .unwrap()
            .pop()
            .unwrap();

        let record = ReraSourceRecordsMaterializer::new(lake.clone())
            .materialize_for_run(
                &source_input(receipt.receipt_id, receipt.capture_id, now),
                &receipt_record,
                MaterializationId::new(),
                partition,
            )
            .await
            .unwrap();
        let rows = read_rera_source_records(&lake, &record).await.unwrap();

        assert_eq!(rows.len(), 3);
        assert!(record
            .artifacts
            .iter()
            .any(|artifact| artifact.key.contains("records/registration_summary/")));
        assert!(record
            .artifacts
            .iter()
            .any(|artifact| artifact.key.contains("records/unknown/")));
        assert!(record
            .artifacts
            .iter()
            .any(|artifact| artifact.key.contains("records/inventory/")));
        assert_eq!(
            rows[0].registration_id,
            rera_registration_identity("PRM/KA/RERA/1251/446/PR/200811/003528")
                .unwrap()
                .registration_id
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.kind == ReraSourceRecordKind::Unknown)
                .unwrap()
                .raw_value,
            "preserved raw value"
        );
        assert!(rows
            .iter()
            .any(|row| row.kind == ReraSourceRecordKind::Inventory));
    }

    #[tokio::test]
    async fn rejects_source_rows_without_a_captured_receipt() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
        let partition = AssetPartition::global();
        let receipt_record = ReraReceiptsMaterializer::new(lake.clone())
            .materialize_for_run(
                &ReraReceiptsInput {
                    snapshot_date: "2026-08-09".to_string(),
                    receipts: vec![ReraReceiptInput {
                        kind: ReraReceiptKind::ProjectDetail,
                        source_url: "https://rera.karnataka.gov.in/projectDetails?id=1".to_string(),
                        content_type: "text/html".to_string(),
                        body: b"<html>fixture</html>".to_vec(),
                        captured_at: now,
                        registration_number: Some(
                            "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                        ),
                        parent_receipt_id: None,
                        crawl_run_id: "fixture".to_string(),
                    }],
                    source_watermarks: Vec::new(),
                },
                MaterializationId::new(),
                partition.clone(),
            )
            .await
            .unwrap();
        let input = source_input(
            "wrong-receipt".to_string(),
            "wrong-capture".to_string(),
            now,
        );

        let error = ReraSourceRecordsMaterializer::new(lake)
            .materialize_for_run(&input, &receipt_record, MaterializationId::new(), partition)
            .await
            .unwrap_err();
        assert!(matches!(error, ReraSourceRecordsError::LineageFailures(_)));
    }
}
