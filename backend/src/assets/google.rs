use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, Float64Array, StringArray, UInt64Array};
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
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord,
    SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const GOOGLE_PLACES_WEEKLY_ASSET_ID: &str = "google_places_weekly";
const GOOGLE_PLACE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlaceSnapshotRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub query: String,
    pub place_name: Option<String>,
    pub place_id: Option<String>,
    pub reviews_url: String,
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    pub address: Option<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlacesWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<GooglePlaceSnapshotRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlaceSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone)]
pub struct GooglePlaceSnapshotMaterialization {
    pub manifest: GooglePlaceSnapshotManifest,
    pub record: MaterializationRecord,
}

#[derive(Clone)]
pub struct GooglePlaceSnapshotMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl GooglePlaceSnapshotMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_and_promote(
        &self,
        input: &GooglePlacesWeeklyInput,
        run_id: impl Into<String>,
    ) -> Result<GooglePlaceSnapshotMaterialization, GooglePlaceAssetError> {
        let partition = AssetPartition::new([("source", "google")]);
        let materialization = self
            .materialize_for_run(
                input,
                run_id,
                Vec::new(),
                MaterializationId::new(),
                partition,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_for_run(
        &self,
        input: &GooglePlacesWeeklyInput,
        run_id: impl Into<String>,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<GooglePlaceSnapshotMaterialization, GooglePlaceAssetError> {
        validate_input(input)?;
        let run_id = run_id.into();
        let artifact_partition = AssetPartition::new([("dt", input.snapshot_date.as_str())]);
        let place_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "places/part-00000.parquet",
        );
        let place_meta = self
            .lake
            .put_bytes(&place_key, write_places_parquet(&input.records)?)
            .await?;
        let mut artifacts = vec![ArtifactRef::parquet(place_meta)];

        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "manifest.json",
        );
        let manifest = GooglePlaceSnapshotManifest {
            asset_id: GOOGLE_PLACES_WEEKLY_ASSET_ID.to_string(),
            format_version: GOOGLE_PLACE_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            row_count: input.records.len() as u64,
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id(GOOGLE_PLACES_WEEKLY_ASSET_ID),
            AssetStage::Raw,
            record_partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(place_watermarks(input))
        .with_row_count(input.records.len() as u64);
        self.materializations.write_materialization(&record).await?;

        Ok(GooglePlaceSnapshotMaterialization { manifest, record })
    }
}

pub async fn read_google_place_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<GooglePlaceSnapshotRecord>, GooglePlaceAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("places/part-00000.parquet"))
        .ok_or_else(|| GooglePlaceAssetError::MissingArtifact(record.asset_id.clone()))?;
    let key = LakeKey::new(&artifact.key).map_err(LakeError::Key)?;
    read_places_parquet(lake.get_bytes(&key).await?)
}

pub async fn google_review_facts_input(
    lake: &LakeStore,
    google_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let rows = read_google_place_rows(lake, google_record).await?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for row in rows {
        push_fact(
            &row,
            run_id,
            "google_reviews_url",
            FactValue::Text(row.reviews_url.clone()),
            "Google reviews: {value}",
            &["google reviews", "resident reviews", "reviews"],
            Some(("TextMatch", 0.8)),
            &mut facts,
            &mut annotations,
        )?;
        if let Some(place_id) = &row.place_id {
            push_fact(
                &row,
                run_id,
                "google_place_id",
                FactValue::Text(place_id.clone()),
                "Google place id: {value}",
                &["google reviews", "maps"],
                None,
                &mut facts,
                &mut annotations,
            )?;
        }
        if let Some(rating) = row.rating {
            push_fact(
                &row,
                run_id,
                "google_rating",
                FactValue::Numeric(rating),
                "Google rating: {value}",
                &["high rating", "good reviews", "google rating"],
                Some(("HigherIsBetter", 1.0)),
                &mut facts,
                &mut annotations,
            )?;
        }
        if let Some(review_count) = row.review_count {
            push_fact(
                &row,
                run_id,
                "google_review_count",
                FactValue::Numeric(review_count as f64),
                "Google reviews: {value}",
                &["many reviews", "review count", "google reviews"],
                Some(("HigherIsBetter", 0.5)),
                &mut facts,
                &mut annotations,
            )?;
        }
    }
    Ok(SkillFactsInput {
        source: "google".to_string(),
        snapshot_date: google_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: google_record.source_watermarks.clone(),
    })
}

pub async fn canonicalize_google_places_input(
    lake: &LakeStore,
    input: &GooglePlacesWeeklyInput,
    canonical_record: &MaterializationRecord,
) -> Result<GooglePlacesWeeklyInput, GooglePlaceAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let canonical_ids: HashSet<_> = canonical
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.as_str())
        .collect();
    let by_project_key: HashMap<_, _> = canonical
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.project_key.as_str(),
                mapping.canonical_entity_id.as_str(),
            )
        })
        .collect();
    let mut resolved = input.clone();
    for record in &mut resolved.records {
        if canonical_ids.contains(record.entity_id.as_str()) {
            continue;
        }
        let entity_id = record
            .project_key
            .as_deref()
            .and_then(|key| by_project_key.get(key).copied())
            .ok_or_else(|| {
                GooglePlaceAssetError::InvalidInput(format!(
                    "Google place row for {} does not resolve to canonical society evidence",
                    record.query
                ))
            })?;
        record.entity_id = entity_id.to_string();
    }
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    row: &GooglePlaceSnapshotRecord,
    run_id: &MaterializationId,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    answers_preferences: &[&str],
    scoring: Option<(&str, f32)>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), GooglePlaceAssetError> {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: row.entity_id.clone(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence: row.confidence,
        source_type: "Google".to_string(),
        source_url: Some(row.reviews_url.clone()),
        model: None,
        skill_id: Some("fetch_google_review_links".to_string()),
        triggered_by: Some(row.query.clone()),
        learned_at: row.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{}:{fact_key}:{value_json}", row.entity_id).as_bytes())
        ),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: row.entity_id.clone(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: scoring.map(|(direction, _)| direction.to_string()),
        scoring_weight: scoring.map(|(_, weight)| weight),
        scoring_thresholds_json: "[]".to_string(),
    });
    Ok(())
}

fn write_places_parquet(
    records: &[GooglePlaceSnapshotRecord],
) -> Result<Vec<u8>, GooglePlaceAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("query", DataType::Utf8, false),
        Field::new("place_name", DataType::Utf8, true),
        Field::new("place_id", DataType::Utf8, true),
        Field::new("reviews_url", DataType::Utf8, false),
        Field::new("rating", DataType::Float64, true),
        Field::new("review_count", DataType::UInt64, true),
        Field::new("address", DataType::Utf8, true),
        Field::new("confidence", DataType::Float32, false),
        Field::new("fetched_at", DataType::Utf8, false),
        Field::new("fetch_source", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(records.iter().map(|record| record.entity_id.clone())),
            optional_string_array(records.iter().map(|record| record.project_key.clone())),
            string_array(records.iter().map(|record| record.query.clone())),
            optional_string_array(records.iter().map(|record| record.place_name.clone())),
            optional_string_array(records.iter().map(|record| record.place_id.clone())),
            string_array(records.iter().map(|record| record.reviews_url.clone())),
            optional_f64_array(records.iter().map(|record| record.rating)),
            optional_u64_array(records.iter().map(|record| record.review_count)),
            optional_string_array(records.iter().map(|record| record.address.clone())),
            Arc::new(Float32Array::from(
                records
                    .iter()
                    .map(|record| record.confidence)
                    .collect::<Vec<_>>(),
            )),
            string_array(records.iter().map(|record| record.fetched_at.to_rfc3339())),
            string_array(records.iter().map(|record| record.fetch_source.clone())),
        ],
    )?;
    write_batch(batch)
}

fn read_places_parquet(
    bytes: Vec<u8>,
) -> Result<Vec<GooglePlaceSnapshotRecord>, GooglePlaceAssetError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let query = string_column(&batch, "query")?;
        let place_name = string_column(&batch, "place_name")?;
        let place_id = string_column(&batch, "place_id")?;
        let reviews_url = string_column(&batch, "reviews_url")?;
        let rating = f64_column(&batch, "rating")?;
        let review_count = u64_column(&batch, "review_count")?;
        let address = string_column(&batch, "address")?;
        let confidence = f32_column(&batch, "confidence")?;
        let fetched_at = string_column(&batch, "fetched_at")?;
        let fetch_source = string_column(&batch, "fetch_source")?;
        for row in 0..batch.num_rows() {
            records.push(GooglePlaceSnapshotRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                query: required_string(query, row, "query")?,
                place_name: optional_string(place_name, row),
                place_id: optional_string(place_id, row),
                reviews_url: required_string(reviews_url, row, "reviews_url")?,
                rating: optional_f64(rating, row),
                review_count: optional_u64(review_count, row),
                address: optional_string(address, row),
                confidence: required_f32(confidence, row, "confidence")?,
                fetched_at: DateTime::parse_from_rfc3339(&required_string(
                    fetched_at,
                    row,
                    "fetched_at",
                )?)?
                .with_timezone(&Utc),
                fetch_source: required_string(fetch_source, row, "fetch_source")?,
            });
        }
    }
    Ok(records)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, GooglePlaceAssetError> {
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut output = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut output, batch.schema(), Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(output)
}

fn validate_input(input: &GooglePlacesWeeklyInput) -> Result<(), GooglePlaceAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google place snapshot date cannot be empty".to_string(),
        ));
    }
    if input.records.is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google place snapshot cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.reviews_url.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} is missing required provenance",
                record.entity_id
            )));
        }
        if !(record.reviews_url.starts_with("https://")
            || record.reviews_url.starts_with("http://"))
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has a non-navigable reviews URL",
                record.entity_id
            )));
        }
        if !record.confidence.is_finite() || !(0.0..=1.0).contains(&record.confidence) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has invalid confidence",
                record.entity_id
            )));
        }
        if record
            .rating
            .is_some_and(|rating| !rating.is_finite() || !(0.0..=5.0).contains(&rating))
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has invalid rating",
                record.entity_id
            )));
        }
    }
    Ok(())
}

fn place_watermarks(input: &GooglePlacesWeeklyInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: "google_places".to_string(),
        high_watermark: input
            .records
            .iter()
            .map(|record| record.fetched_at)
            .max()
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| input.snapshot_date.clone()),
    }]
}

fn string_array(values: impl IntoIterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(values))
}

fn optional_string_array(values: impl IntoIterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_f64_array(values: impl IntoIterator<Item = Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_u64_array(values: impl IntoIterator<Item = Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn f64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float64Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn u64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a UInt64Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn f32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float32Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float32Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn required_string(
    column: &StringArray,
    row: usize,
    name: &str,
) -> Result<String, GooglePlaceAssetError> {
    if column.is_null(row) {
        return Err(GooglePlaceAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(column: &StringArray, row: usize) -> Option<String> {
    (!column.is_null(row)).then(|| column.value(row).to_string())
}

fn optional_f64(column: &Float64Array, row: usize) -> Option<f64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn optional_u64(column: &UInt64Array, row: usize) -> Option<u64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn required_f32(
    column: &Float32Array,
    row: usize,
    name: &str,
) -> Result<f32, GooglePlaceAssetError> {
    if column.is_null(row) {
        return Err(GooglePlaceAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row))
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

fn asset_id(value: &str) -> AssetId {
    AssetId::new(value).expect("static Google asset id is valid")
}

#[derive(Debug)]
pub enum GooglePlaceAssetError {
    Arrow(arrow::error::ArrowError),
    Chrono(chrono::ParseError),
    InvalidInput(String),
    InvalidSchema(String),
    Lake(LakeError),
    MissingArtifact(AssetId),
    Parquet(parquet::errors::ParquetError),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
}

impl fmt::Display for GooglePlaceAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "Google place Arrow error: {err}"),
            Self::Chrono(err) => write!(f, "Google place timestamp error: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid Google place input: {message}"),
            Self::InvalidSchema(column) => {
                write!(f, "invalid Google place Parquet column: {column}")
            }
            Self::Lake(err) => write!(f, "Google place lake error: {err}"),
            Self::MissingArtifact(asset_id) => {
                write!(f, "Google place artifact missing for {asset_id}")
            }
            Self::Parquet(err) => write!(f, "Google place Parquet error: {err}"),
            Self::Json(err) => write!(f, "Google place JSON error: {err}"),
            Self::Canonical(err) => write!(f, "Google canonical society lookup failed: {err}"),
        }
    }
}

impl std::error::Error for GooglePlaceAssetError {}

impl From<arrow::error::ArrowError> for GooglePlaceAssetError {
    fn from(value: arrow::error::ArrowError) -> Self {
        Self::Arrow(value)
    }
}

impl From<chrono::ParseError> for GooglePlaceAssetError {
    fn from(value: chrono::ParseError) -> Self {
        Self::Chrono(value)
    }
}

impl From<LakeError> for GooglePlaceAssetError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}

impl From<parquet::errors::ParquetError> for GooglePlaceAssetError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        Self::Parquet(value)
    }
}

impl From<serde_json::Error> for GooglePlaceAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ReraAssetError> for GooglePlaceAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}
