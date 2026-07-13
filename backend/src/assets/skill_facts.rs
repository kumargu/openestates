use std::fmt;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};

use crate::lake::{LakeError, LakeStore};

use super::types::AssetIdError;
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SourceWatermark,
};

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
        if facts.is_empty() {
            return Err(SkillFactMaterializeError::EmptyFacts);
        }

        let asset_id = asset_id.into();
        let source = source.into();
        let snapshot_date = snapshot_date.into();
        let run_id = run_id.into();
        let asset = AssetId::new(asset_id.clone()).map_err(SkillFactMaterializeError::AssetId)?;
        let partition =
            AssetPartition::new([("dt", snapshot_date.as_str()), ("source", source.as_str())]);

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
            format_version: 1,
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
            partition,
            snapshot_date,
            artifacts,
        )
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(source_watermarks)
        .with_row_count(facts.len() as u64);

        self.materializations.write_materialization(&record).await?;
        self.materializations.promote_current(&record).await?;

        Ok(SkillFactMaterialization { manifest, record })
    }
}

fn write_facts_parquet(facts: &[SkillFactRecord]) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("value_type", DataType::Utf8, false),
        Field::new("value_json", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("triggered_by", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
        Field::new("run_id", DataType::Utf8, false),
        Field::new("input_hash", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(facts.iter().map(|fact| fact.entity_id.clone())),
            string_array(facts.iter().map(|fact| fact.fact_key.clone())),
            string_array(facts.iter().map(|fact| fact.value_type.clone())),
            string_array(facts.iter().map(|fact| fact.value_json.clone())),
            Arc::new(Float32Array::from(
                facts.iter().map(|fact| fact.confidence).collect::<Vec<_>>(),
            )),
            string_array(facts.iter().map(|fact| fact.source_type.clone())),
            optional_string_array(facts.iter().map(|fact| fact.source_url.clone())),
            optional_string_array(facts.iter().map(|fact| fact.model.clone())),
            optional_string_array(facts.iter().map(|fact| fact.skill_id.clone())),
            optional_string_array(facts.iter().map(|fact| fact.triggered_by.clone())),
            string_array(facts.iter().map(|fact| fact.learned_at.to_rfc3339())),
            string_array(facts.iter().map(|fact| fact.run_id.clone())),
            string_array(facts.iter().map(|fact| fact.input_hash.clone())),
        ],
    )
    .map_err(SkillFactMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_fact_annotations_parquet(
    annotations: &[SkillFactAnnotationRecord],
) -> Result<Vec<u8>, SkillFactMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("display_template", DataType::Utf8, true),
        Field::new("answers_preferences_json", DataType::Utf8, false),
        Field::new("scoring_direction", DataType::Utf8, true),
        Field::new("scoring_weight", DataType::Float32, true),
        Field::new("scoring_thresholds_json", DataType::Utf8, false),
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
            string_array(
                annotations
                    .iter()
                    .map(|record| record.answers_preferences_json.clone()),
            ),
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
            string_array(
                annotations
                    .iter()
                    .map(|record| record.scoring_thresholds_json.clone()),
            ),
        ],
    )
    .map_err(SkillFactMaterializeError::Arrow)?;

    write_batch(batch)
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

#[derive(Debug)]
pub enum SkillFactMaterializeError {
    Arrow(arrow::error::ArrowError),
    AssetId(AssetIdError),
    EmptyFacts,
    Lake(LakeError),
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for SkillFactMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "skill facts Arrow record batch error: {err}"),
            Self::AssetId(err) => write!(f, "invalid skill facts asset id: {err}"),
            Self::EmptyFacts => f.write_str("skill facts batch cannot be empty"),
            Self::Lake(err) => write!(f, "skill facts lake error: {err}"),
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
