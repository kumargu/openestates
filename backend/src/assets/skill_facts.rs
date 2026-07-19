use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, StringArray};
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
use crate::parquet_data::{
    float64_list_array, float64_list_field, optional_f64_list_column_value,
    optional_string_list_column_value, string_list_array, string_list_field, typed_value_arrays,
    typed_value_fields, typed_value_from_batch, OptionalListColumn, TypedFactValue,
    ANSWERS_PREFERENCES_COLUMN, SCORING_THRESHOLDS_COLUMN,
};

use super::types::AssetIdError;
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SourceWatermark,
};

pub const LEGACY_SEED_FACTS_ASSET_ID: &str = "legacy_seed_facts";
pub const REDDIT_RESIDENT_FACTS_ASSET_ID: &str = "reddit_resident_facts";
pub const GOOGLE_REVIEW_FACTS_ASSET_ID: &str = "google_review_facts";
pub const GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID: &str = "google_nearby_place_facts";
const SKILL_FACT_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFactRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub value_type: String,
    pub value_json: String,
    pub confidence: f32,
    pub source_type: String,
    pub source_url: Option<String>,
    pub model: Option<String>,
    pub skill_id: Option<String>,
    pub triggered_by: Option<String>,
    pub learned_at: DateTime<Utc>,
    pub run_id: String,
    pub input_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFactAnnotationRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub display_template: Option<String>,
    pub answers_preferences_json: String,
    pub scoring_direction: Option<String>,
    pub scoring_weight: Option<f32>,
    pub scoring_thresholds_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFactManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub source: String,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub fact_count: u64,
    pub fact_annotation_count: u64,
    pub fact_parquet_key: String,
    pub fact_annotation_parquet_key: String,
    pub manifest_key: String,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone)]
pub struct SkillFactMaterialization {
    pub manifest: SkillFactManifest,
    pub record: MaterializationRecord,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillFactArtifactRows {
    pub facts: Vec<SkillFactRecord>,
    pub fact_annotations: Vec<SkillFactAnnotationRecord>,
}

#[derive(Clone)]
pub struct SkillFactMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl SkillFactMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_and_promote(
        &self,
        asset_id: impl Into<String>,
        source: impl Into<String>,
        snapshot_date: impl Into<String>,
        run_id: impl Into<String>,
        facts: &[SkillFactRecord],
        fact_annotations: &[SkillFactAnnotationRecord],
        parent_materializations: Vec<MaterializationId>,
        source_watermarks: Vec<SourceWatermark>,
    ) -> Result<SkillFactMaterialization, SkillFactMaterializeError> {
        let asset_id = asset_id.into();
        let source = source.into();
        let snapshot_date = snapshot_date.into();
        let run_id = run_id.into();
        let partition =
            AssetPartition::new([("dt", snapshot_date.as_str()), ("source", source.as_str())]);
        let materialization = self
            .materialize_for_run_inner(
                asset_id,
                source,
                snapshot_date,
                run_id,
                facts,
                fact_annotations,
                parent_materializations,
                source_watermarks,
                MaterializationId::new(),
                partition,
                false,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_for_run(
        &self,
        asset_id: impl Into<String>,
        source: impl Into<String>,
        snapshot_date: impl Into<String>,
        run_id: impl Into<String>,
        facts: &[SkillFactRecord],
        fact_annotations: &[SkillFactAnnotationRecord],
        parent_materializations: Vec<MaterializationId>,
        source_watermarks: Vec<SourceWatermark>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<SkillFactMaterialization, SkillFactMaterializeError> {
        self.materialize_for_run_inner(
            asset_id,
            source,
            snapshot_date,
            run_id,
            facts,
            fact_annotations,
            parent_materializations,
            source_watermarks,
            dag_run_id,
            record_partition,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_skipped_for_run(
        &self,
        asset_id: impl Into<String>,
        source: impl Into<String>,
        snapshot_date: impl Into<String>,
        run_id: impl Into<String>,
        parent_materializations: Vec<MaterializationId>,
        source_watermarks: Vec<SourceWatermark>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<SkillFactMaterialization, SkillFactMaterializeError> {
        if !source_watermarks
            .iter()
            .any(|watermark| watermark.source.ends_with("_skipped"))
        {
            return Err(SkillFactMaterializeError::EmptyFacts);
        }
        self.materialize_for_run_inner(
            asset_id,
            source,
            snapshot_date,
            run_id,
            &[],
            &[],
            parent_materializations,
            source_watermarks,
            dag_run_id,
            record_partition,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn materialize_for_run_inner(
        &self,
        asset_id: impl Into<String>,
        source: impl Into<String>,
        snapshot_date: impl Into<String>,
        run_id: impl Into<String>,
        facts: &[SkillFactRecord],
        fact_annotations: &[SkillFactAnnotationRecord],
        parent_materializations: Vec<MaterializationId>,
        source_watermarks: Vec<SourceWatermark>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
        allow_empty: bool,
    ) -> Result<SkillFactMaterialization, SkillFactMaterializeError> {
        if facts.is_empty() && !allow_empty {
            return Err(SkillFactMaterializeError::EmptyFacts);
        }
        let asset_id = asset_id.into();
        let source = source.into();
        let snapshot_date = snapshot_date.into();
        let run_id = run_id.into();
        let asset = AssetId::new(asset_id.clone()).map_err(SkillFactMaterializeError::AssetId)?;

        let fact_key = AssetPathBuilder::silver_asset_key(
            &asset_id,
            &source,
            &snapshot_date,
            &run_id,
            "facts/part-00000.parquet",
        );
        let fact_meta = self
            .lake
            .put_bytes(&fact_key, write_facts_parquet(facts)?)
            .await?;

        let fact_annotation_key = AssetPathBuilder::silver_asset_key(
            &asset_id,
            &source,
            &snapshot_date,
            &run_id,
            "fact_annotations/part-00000.parquet",
        );
        let fact_annotation_meta = self
            .lake
            .put_bytes(
                &fact_annotation_key,
                write_fact_annotations_parquet(fact_annotations)?,
            )
            .await?;

        let manifest_key = AssetPathBuilder::silver_asset_key(
            &asset_id,
            &source,
            &snapshot_date,
            &run_id,
            "manifest.json",
        );
        let mut artifacts = vec![
            ArtifactRef::parquet(fact_meta),
            ArtifactRef::parquet(fact_annotation_meta),
        ];
        let manifest = SkillFactManifest {
            asset_id: asset_id.clone(),
            format_version: SKILL_FACT_FORMAT_VERSION,
            source: source.clone(),
            snapshot_date: snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            fact_count: facts.len() as u64,
            fact_annotation_count: fact_annotations.len() as u64,
            fact_parquet_key: fact_key.to_string(),
            fact_annotation_parquet_key: fact_annotation_key.to_string(),
            manifest_key: manifest_key.to_string(),
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset,
            AssetStage::Silver,
            record_partition,
            snapshot_date,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(source_watermarks)
        .with_row_count(facts.len() as u64);

        self.materializations.write_materialization(&record).await?;

        Ok(SkillFactMaterialization { manifest, record })
    }
}

pub async fn read_skill_fact_artifact_rows(
    lake: &LakeStore,
    materializations: &[MaterializationRecord],
) -> Result<SkillFactArtifactRows, SkillFactMaterializeError> {
    let mut rows = SkillFactArtifactRows::default();

    for materialization in materializations {
        rows.facts.extend(read_facts_parquet_records(
            read_artifact_bytes(lake, materialization, "facts/part-00000.parquet").await?,
        )?);
        rows.fact_annotations.extend(read_fact_annotation_records(
            read_artifact_bytes(lake, materialization, "fact_annotations/part-00000.parquet")
                .await?,
        )?);
    }

    Ok(rows)
}

pub(crate) fn write_facts_parquet(
    facts: &[SkillFactRecord],
) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let typed_values = facts
        .iter()
        .map(|fact| {
            let value = serde_json::from_str(&fact.value_json)?;
            validate_fact_value_type(&fact.value_type, &value)?;
            Ok(TypedFactValue::from_fact_value(&value))
        })
        .collect::<Result<Vec<_>, SkillFactMaterializeError>>()?;

    let mut fields = vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("value_type", DataType::Utf8, false),
    ];
    fields.extend(typed_value_fields(true));
    fields.extend([
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("triggered_by", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
        Field::new("run_id", DataType::Utf8, false),
        Field::new("input_hash", DataType::Utf8, false),
    ]);
    let schema = Arc::new(Schema::new(fields));

    let mut columns = vec![
        string_array(facts.iter().map(|fact| fact.entity_id.clone())),
        string_array(facts.iter().map(|fact| fact.fact_key.clone())),
        string_array(facts.iter().map(|fact| fact.value_type.clone())),
    ];
    columns.extend(typed_value_arrays(&typed_values, true));
    columns.extend([
        Arc::new(Float32Array::from(
            facts.iter().map(|fact| fact.confidence).collect::<Vec<_>>(),
        )) as ArrayRef,
        string_array(facts.iter().map(|fact| fact.source_type.clone())),
        optional_string_array(facts.iter().map(|fact| fact.source_url.clone())),
        optional_string_array(facts.iter().map(|fact| fact.model.clone())),
        optional_string_array(facts.iter().map(|fact| fact.skill_id.clone())),
        optional_string_array(facts.iter().map(|fact| fact.triggered_by.clone())),
        string_array(facts.iter().map(|fact| fact.learned_at.to_rfc3339())),
        string_array(facts.iter().map(|fact| fact.run_id.clone())),
        string_array(facts.iter().map(|fact| fact.input_hash.clone())),
    ]);

    let batch =
        RecordBatch::try_new(schema.clone(), columns).map_err(SkillFactMaterializeError::Arrow)?;

    write_batch(batch)
}

pub(crate) fn read_facts_parquet_records(
    bytes: Vec<u8>,
) -> Result<Vec<SkillFactRecord>, SkillFactMaterializeError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let value_type = string_column(&batch, "value_type")?;
        let value_json = optional_string_column(&batch, "value_json")?;
        let confidence = float32_column(&batch, "confidence")?;
        let source_type = string_column(&batch, "source_type")?;
        let source_url = string_column(&batch, "source_url")?;
        let model = string_column(&batch, "model")?;
        let skill_id = string_column(&batch, "skill_id")?;
        let triggered_by = string_column(&batch, "triggered_by")?;
        let learned_at = string_column(&batch, "learned_at")?;
        let run_id = string_column(&batch, "run_id")?;
        let input_hash = string_column(&batch, "input_hash")?;

        for row in 0..batch.num_rows() {
            let value_type = required_string(value_type, row, "value_type")?;
            let value_json = match value_json {
                Some(value_json) => required_string(value_json, row, "value_json")?,
                None => typed_value_json_from_batch(&batch, row, &value_type)?,
            };
            records.push(SkillFactRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                value_type,
                value_json,
                confidence: required_f32(confidence, row, "confidence")?,
                source_type: required_string(source_type, row, "source_type")?,
                source_url: optional_string(source_url, row),
                model: optional_string(model, row),
                skill_id: optional_string(skill_id, row),
                triggered_by: optional_string(triggered_by, row),
                learned_at: DateTime::parse_from_rfc3339(&required_string(
                    learned_at,
                    row,
                    "learned_at",
                )?)?
                .with_timezone(&Utc),
                run_id: required_string(run_id, row, "run_id")?,
                input_hash: required_string(input_hash, row, "input_hash")?,
            });
        }
    }
    Ok(records)
}

pub(crate) fn write_fact_annotations_parquet(
    annotations: &[SkillFactAnnotationRecord],
) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let answers_preferences = annotations
        .iter()
        .map(|record| parse_string_vec(&record.answers_preferences_json).map(Some))
        .collect::<Result<Vec<_>, SkillFactMaterializeError>>()?;
    let scoring_thresholds = annotations
        .iter()
        .map(|record| parse_f64_vec(&record.scoring_thresholds_json).map(Some))
        .collect::<Result<Vec<_>, SkillFactMaterializeError>>()?;

    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("display_template", DataType::Utf8, true),
        string_list_field(ANSWERS_PREFERENCES_COLUMN, false),
        Field::new("scoring_direction", DataType::Utf8, true),
        Field::new("scoring_weight", DataType::Float32, true),
        float64_list_field(SCORING_THRESHOLDS_COLUMN, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(annotations.iter().map(|record| record.entity_id.clone())),
            string_array(annotations.iter().map(|record| record.fact_key.clone())),
            optional_string_array(
                annotations
                    .iter()
                    .map(|record| record.display_template.clone()),
            ),
            string_list_array(answers_preferences.into_iter()),
            optional_string_array(
                annotations
                    .iter()
                    .map(|record| record.scoring_direction.clone()),
            ),
            Arc::new(Float32Array::from(
                annotations
                    .iter()
                    .map(|record| record.scoring_weight)
                    .collect::<Vec<_>>(),
            )),
            float64_list_array(scoring_thresholds.into_iter()),
        ],
    )
    .map_err(SkillFactMaterializeError::Arrow)?;

    write_batch(batch)
}

pub(crate) fn read_fact_annotation_records(
    bytes: Vec<u8>,
) -> Result<Vec<SkillFactAnnotationRecord>, SkillFactMaterializeError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let display_template = string_column(&batch, "display_template")?;
        let answers_preferences_json = optional_string_column(&batch, "answers_preferences_json")?;
        let scoring_direction = string_column(&batch, "scoring_direction")?;
        let scoring_weight = float32_column(&batch, "scoring_weight")?;
        let scoring_thresholds_json = optional_string_column(&batch, "scoring_thresholds_json")?;

        for row in 0..batch.num_rows() {
            records.push(SkillFactAnnotationRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                display_template: optional_string(display_template, row),
                answers_preferences_json: list_json_from_batch(
                    &batch,
                    answers_preferences_json,
                    ANSWERS_PREFERENCES_COLUMN,
                    row,
                )?,
                scoring_direction: optional_string(scoring_direction, row),
                scoring_weight: optional_f32(scoring_weight, row),
                scoring_thresholds_json: thresholds_json_from_batch(
                    &batch,
                    scoring_thresholds_json,
                    row,
                )?,
            });
        }
    }
    Ok(records)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(props))
        .map_err(SkillFactMaterializeError::Parquet)?;
    writer
        .write(&batch)
        .map_err(SkillFactMaterializeError::Parquet)?;
    writer.close().map_err(SkillFactMaterializeError::Parquet)?;
    Ok(bytes)
}

fn string_array(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_string_array(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn parse_string_vec(value: &str) -> Result<Vec<String>, SkillFactMaterializeError> {
    Ok(serde_json::from_str(value)?)
}

fn parse_f64_vec(value: &str) -> Result<Vec<f64>, SkillFactMaterializeError> {
    Ok(serde_json::from_str(value)?)
}

fn typed_value_json_from_batch(
    batch: &RecordBatch,
    row: usize,
    value_type: &str,
) -> Result<String, SkillFactMaterializeError> {
    let typed = typed_value_from_batch(batch, row).ok_or_else(|| {
        SkillFactMaterializeError::InvalidParquet(format!(
            "missing typed fact value columns at row {row}"
        ))
    })?;
    let value = typed.to_fact_value(value_type).ok_or_else(|| {
        SkillFactMaterializeError::InvalidParquet(format!(
            "typed fact value columns do not match value_type {value_type} at row {row}"
        ))
    })?;
    Ok(serde_json::to_string(&value)?)
}

fn validate_fact_value_type(
    value_type: &str,
    value: &crate::knowledge::FactValue,
) -> Result<(), SkillFactMaterializeError> {
    if TypedFactValue::value_type_matches(value_type, value) {
        return Ok(());
    }
    Err(SkillFactMaterializeError::InvalidParquet(format!(
        "value_type {value_type} does not match fact value type {}",
        TypedFactValue::value_type_for(value)
    )))
}

fn list_json_from_batch(
    batch: &RecordBatch,
    legacy_json: Option<&StringArray>,
    typed_column: &str,
    row: usize,
) -> Result<String, SkillFactMaterializeError> {
    if let Some(legacy_json) = legacy_json {
        return required_string(legacy_json, row, typed_column);
    }
    let values = match optional_string_list_column_value(batch, typed_column, row) {
        Ok(OptionalListColumn::Values(values)) => values,
        Ok(OptionalListColumn::Null) => Vec::new(),
        Ok(OptionalListColumn::Missing) => {
            return Err(SkillFactMaterializeError::InvalidParquet(format!(
                "missing typed list column {typed_column}"
            )));
        }
        Err(message) => return Err(SkillFactMaterializeError::InvalidParquet(message)),
    };
    Ok(serde_json::to_string(&values)?)
}

fn thresholds_json_from_batch(
    batch: &RecordBatch,
    legacy_json: Option<&StringArray>,
    row: usize,
) -> Result<String, SkillFactMaterializeError> {
    if let Some(legacy_json) = legacy_json {
        return required_string(legacy_json, row, "scoring_thresholds_json");
    }
    let values = match optional_f64_list_column_value(batch, SCORING_THRESHOLDS_COLUMN, row) {
        Ok(OptionalListColumn::Values(values)) => values,
        Ok(OptionalListColumn::Null) => Vec::new(),
        Ok(OptionalListColumn::Missing) => {
            return Err(SkillFactMaterializeError::InvalidParquet(format!(
                "missing typed list column {SCORING_THRESHOLDS_COLUMN}"
            )));
        }
        Err(message) => return Err(SkillFactMaterializeError::InvalidParquet(message)),
    };
    Ok(serde_json::to_string(&values)?)
}

async fn read_artifact_bytes(
    lake: &LakeStore,
    materialization: &MaterializationRecord,
    suffix: &str,
) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let artifact = artifact_ref(materialization, suffix)?;
    validate_artifact_ref(materialization, artifact, suffix)?;
    let key = LakeKey::new(artifact.key.clone()).map_err(SkillFactMaterializeError::Key)?;
    let bytes = lake.get_bytes(&key).await?;
    validate_artifact_bytes(materialization, artifact, &bytes)?;
    Ok(bytes)
}

fn artifact_ref<'a>(
    materialization: &'a MaterializationRecord,
    suffix: &str,
) -> Result<&'a ArtifactRef, SkillFactMaterializeError> {
    materialization
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .ok_or_else(|| SkillFactMaterializeError::MissingArtifact {
            asset_id: materialization.asset_id.clone(),
            suffix: suffix.to_string(),
        })
}

fn validate_artifact_ref(
    materialization: &MaterializationRecord,
    artifact: &ArtifactRef,
    suffix: &str,
) -> Result<(), SkillFactMaterializeError> {
    let expected_prefix = format!("silver/{}/", materialization.asset_id);
    if !artifact.key.starts_with(&expected_prefix) {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            format!("expected key to start with {expected_prefix}"),
        ));
    }
    if !artifact.key.ends_with(suffix) {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            format!("expected key to end with {suffix}"),
        ));
    }
    if artifact.content_type != "application/vnd.apache.parquet" {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            format!(
                "expected Parquet content type, got {}",
                artifact.content_type
            ),
        ));
    }
    if artifact.hash_algorithm != "sha256" {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            format!("expected sha256 hash, got {}", artifact.hash_algorithm),
        ));
    }
    if artifact.size_bytes == 0 {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            "artifact size cannot be zero",
        ));
    }
    if artifact.content_hash.len() != 64
        || !artifact
            .content_hash
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            "artifact hash must be 64 hex characters",
        ));
    }
    Ok(())
}

fn validate_artifact_bytes(
    materialization: &MaterializationRecord,
    artifact: &ArtifactRef,
    bytes: &[u8],
) -> Result<(), SkillFactMaterializeError> {
    if artifact.size_bytes != bytes.len() {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            format!(
                "artifact size {} does not match bytes {}",
                artifact.size_bytes,
                bytes.len()
            ),
        ));
    }
    let actual_hash = sha256_hex(bytes);
    if artifact.content_hash != actual_hash {
        return Err(invalid_artifact_metadata(
            materialization,
            artifact,
            "artifact content hash does not match bytes",
        ));
    }
    Ok(())
}

fn invalid_artifact_metadata(
    materialization: &MaterializationRecord,
    artifact: &ArtifactRef,
    message: impl Into<String>,
) -> SkillFactMaterializeError {
    SkillFactMaterializeError::InvalidArtifactMetadata {
        asset_id: materialization.asset_id.clone(),
        key: artifact.key.clone(),
        message: message.into(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, SkillFactMaterializeError> {
    let index = batch.schema().index_of(name)?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| {
            SkillFactMaterializeError::InvalidParquet(format!("column {name} is not Utf8"))
        })
}

fn optional_string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<Option<&'a StringArray>, SkillFactMaterializeError> {
    let Ok(index) = batch.schema().index_of(name) else {
        return Ok(None);
    };
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .map(Some)
        .ok_or_else(|| {
            SkillFactMaterializeError::InvalidParquet(format!("column {name} is not Utf8"))
        })
}

fn float32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float32Array, SkillFactMaterializeError> {
    let index = batch.schema().index_of(name)?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| {
            SkillFactMaterializeError::InvalidParquet(format!("column {name} is not Float32"))
        })
}

fn required_string(
    array: &StringArray,
    row: usize,
    column: &str,
) -> Result<String, SkillFactMaterializeError> {
    if array.is_null(row) {
        return Err(SkillFactMaterializeError::InvalidParquet(format!(
            "column {column} is unexpectedly null at row {row}"
        )));
    }
    Ok(array.value(row).to_string())
}

fn optional_string(array: &StringArray, row: usize) -> Option<String> {
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row).to_string())
    }
}

fn required_f32(
    array: &Float32Array,
    row: usize,
    column: &str,
) -> Result<f32, SkillFactMaterializeError> {
    if array.is_null(row) {
        return Err(SkillFactMaterializeError::InvalidParquet(format!(
            "column {column} is unexpectedly null at row {row}"
        )));
    }
    Ok(array.value(row))
}

fn optional_f32(array: &Float32Array, row: usize) -> Option<f32> {
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}

#[derive(Debug)]
pub enum SkillFactMaterializeError {
    Arrow(arrow::error::ArrowError),
    AssetId(AssetIdError),
    Chrono(chrono::ParseError),
    EmptyFacts,
    InvalidArtifactMetadata {
        asset_id: AssetId,
        key: String,
        message: String,
    },
    InvalidParquet(String),
    Json(serde_json::Error),
    Key(crate::lake::keys::KeyError),
    Lake(LakeError),
    MissingArtifact {
        asset_id: AssetId,
        suffix: String,
    },
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for SkillFactMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "skill facts Arrow record batch error: {err}"),
            Self::AssetId(err) => write!(f, "invalid skill facts asset id: {err}"),
            Self::Chrono(err) => write!(f, "skill facts timestamp parse error: {err}"),
            Self::EmptyFacts => f.write_str("skill facts batch cannot be empty"),
            Self::InvalidArtifactMetadata {
                asset_id,
                key,
                message,
            } => write!(
                f,
                "invalid skill facts artifact metadata for {asset_id} at {key}: {message}"
            ),
            Self::InvalidParquet(message) => write!(f, "invalid skill facts Parquet: {message}"),
            Self::Json(err) => write!(f, "skill facts JSON compatibility error: {err}"),
            Self::Key(err) => write!(f, "skill facts artifact key error: {err}"),
            Self::Lake(err) => write!(f, "skill facts lake error: {err}"),
            Self::MissingArtifact { asset_id, suffix } => write!(
                f,
                "skill facts materialization {asset_id} is missing artifact ending in {suffix}"
            ),
            Self::Parquet(err) => write!(f, "skill facts Parquet error: {err}"),
        }
    }
}

impl std::error::Error for SkillFactMaterializeError {}

impl From<LakeError> for SkillFactMaterializeError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<arrow::error::ArrowError> for SkillFactMaterializeError {
    fn from(err: arrow::error::ArrowError) -> Self {
        Self::Arrow(err)
    }
}

impl From<chrono::ParseError> for SkillFactMaterializeError {
    fn from(err: chrono::ParseError) -> Self {
        Self::Chrono(err)
    }
}

impl From<serde_json::Error> for SkillFactMaterializeError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<parquet::errors::ParquetError> for SkillFactMaterializeError {
    fn from(err: parquet::errors::ParquetError) -> Self {
        Self::Parquet(err)
    }
}
