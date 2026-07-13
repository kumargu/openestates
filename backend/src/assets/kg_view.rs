use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float32Array, StringArray, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::lake::{ArtifactMetadata, LakeError, LakeStore};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SourceWatermark,
};

pub const KG_SOCIETY_VIEW_ASSET_ID: &str = "kg_society_view";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KgViewEntityRecord {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub root_source: Option<String>,
    pub fact_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KgViewFactRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub fact_version: u32,
    pub value_type: String,
    pub value_text: Option<String>,
    pub value_json: String,
    pub confidence: f32,
    pub source_type: String,
    pub source_url: Option<String>,
    pub model: Option<String>,
    pub skill_id: Option<String>,
    pub triggered_by: Option<String>,
    pub learned_at: DateTime<Utc>,
}

/// Optional annotations layered over canonical facts.
///
/// Search can consume these fields, but the main fact table remains reusable by
/// calculators, benchmark jobs, valuation jobs, and future decision products.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KgViewFactAnnotationRecord {
    pub entity_id: String,
    pub fact_key: String,
    pub display_template: Option<String>,
    pub answers_preferences_json: String,
    pub scoring_direction: Option<String>,
    pub scoring_weight: Option<f32>,
    pub scoring_thresholds_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KgViewEdgeRecord {
    pub from_entity_id: String,
    pub to_entity_id: String,
    pub relation: String,
    pub weight: f32,
    pub metadata_json: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub model: Option<String>,
    pub skill_id: Option<String>,
    pub triggered_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KgViewRecords {
    pub entities: Vec<KgViewEntityRecord>,
    pub facts: Vec<KgViewFactRecord>,
    pub fact_annotations: Vec<KgViewFactAnnotationRecord>,
    pub edges: Vec<KgViewEdgeRecord>,
    pub content_hash: String,
}

impl KgViewRecords {
    pub fn from_graph(graph: &KnowledgeGraph) -> Result<Self, serde_json::Error> {
        let mut entities: Vec<_> = graph
            .nodes
            .values()
            .map(|node| KgViewEntityRecord {
                entity_id: node.id.clone(),
                entity_type: node.node_type.to_string(),
                name: node.name.clone(),
                root_source: node.root_source.map(|source| source.as_str().to_string()),
                fact_count: node.facts.len() as u32,
                created_at: node.created_at,
                updated_at: node.updated_at,
            })
            .collect();
        entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

        let mut facts = Vec::new();
        let mut fact_annotations = Vec::new();
        for node in graph.nodes.values() {
            for fact in &node.facts {
                facts.push(KgViewFactRecord {
                    entity_id: node.id.clone(),
                    fact_key: fact.key.clone(),
                    fact_version: fact.version,
                    value_type: fact_value_type(&fact.value).to_string(),
                    value_text: fact_value_text(&fact.value),
                    value_json: serde_json::to_string(&fact.value)?,
                    confidence: fact.confidence,
                    source_type: format!("{:?}", fact.source.source_type),
                    source_url: fact.source.url.clone(),
                    model: fact.source.model.clone(),
                    skill_id: fact.source.skill_id.clone(),
                    triggered_by: fact.source.triggered_by.clone(),
                    learned_at: fact.learned_at,
                });

                let scoring_direction = fact
                    .scoring_hint
                    .as_ref()
                    .map(|hint| format!("{:?}", hint.direction));
                let scoring_weight = fact.scoring_hint.as_ref().map(|hint| hint.weight);
                let scoring_thresholds = fact
                    .scoring_hint
                    .as_ref()
                    .map(|hint| hint.thresholds.as_slice())
                    .unwrap_or(&[]);
                fact_annotations.push(KgViewFactAnnotationRecord {
                    entity_id: node.id.clone(),
                    fact_key: fact.key.clone(),
                    display_template: fact.display_template.clone(),
                    answers_preferences_json: serde_json::to_string(&fact.answers_preferences)?,
                    scoring_direction,
                    scoring_weight,
                    scoring_thresholds_json: serde_json::to_string(scoring_thresholds)?,
                });
            }
        }
        facts.sort_by(|left, right| {
            left.entity_id
                .cmp(&right.entity_id)
                .then(left.fact_key.cmp(&right.fact_key))
                .then(left.fact_version.cmp(&right.fact_version))
                .then(left.source_type.cmp(&right.source_type))
                .then(left.learned_at.cmp(&right.learned_at))
        });
        fact_annotations.sort_by(|left, right| {
            left.entity_id
                .cmp(&right.entity_id)
                .then(left.fact_key.cmp(&right.fact_key))
        });

        let mut edges: Vec<_> = graph
            .edges
            .iter()
            .map(|edge| KgViewEdgeRecord {
                from_entity_id: edge.from.clone(),
                to_entity_id: edge.to.clone(),
                relation: format!("{:?}", edge.relation),
                weight: edge.weight,
                metadata_json: metadata_json(&edge.metadata),
                source_type: format!("{:?}", edge.source.source_type),
                source_url: edge.source.url.clone(),
                model: edge.source.model.clone(),
                skill_id: edge.source.skill_id.clone(),
                triggered_by: edge.source.triggered_by.clone(),
            })
            .collect();
        edges.sort_by(|left, right| {
            left.from_entity_id
                .cmp(&right.from_entity_id)
                .then(left.to_entity_id.cmp(&right.to_entity_id))
                .then(left.relation.cmp(&right.relation))
        });

        let content_hash = content_hash(&entities, &facts, &fact_annotations, &edges)?;
        Ok(Self {
            entities,
            facts,
            fact_annotations,
            edges,
            content_hash,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KgViewArtifactKind {
    EntitiesParquet,
    FactsParquet,
    FactAnnotationsParquet,
    EdgesParquet,
    ManifestJson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KgViewArtifact {
    pub kind: KgViewArtifactKind,
    pub key: String,
    pub format: String,
    pub content_hash: String,
    pub hash_algorithm: String,
    pub size_bytes: usize,
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KgViewManifest {
    pub view_version: String,
    pub format_version: u32,
    pub created_at: DateTime<Utc>,
    pub graph_content_hash: String,
    pub entity_count: u64,
    pub fact_count: u64,
    pub fact_annotation_count: u64,
    pub edge_count: u64,
    pub entity_parquet_key: String,
    pub fact_parquet_key: String,
    pub fact_annotation_parquet_key: String,
    pub edge_parquet_key: String,
    pub artifacts: Vec<KgViewArtifact>,
}

#[derive(Clone)]
pub struct KgSocietyViewMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

#[derive(Debug, Clone)]
pub struct KgSocietyViewMaterialization {
    pub manifest: KgViewManifest,
    pub record: MaterializationRecord,
    pub records: KgViewRecords,
}

impl KgSocietyViewMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
        }
    }

    pub async fn materialize_and_promote(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        let materialization = self
            .materialize_for_run(
                graph,
                view_version,
                source_watermarks,
                parent_materializations,
                MaterializationId::new(),
                AssetPartition::global(),
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_and_promote_for_run(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        let materialization = self
            .materialize_for_run(
                graph,
                view_version,
                source_watermarks,
                parent_materializations,
                run_id,
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
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        self.materialize_for_run_inner(
            graph,
            view_version,
            source_watermarks,
            parent_materializations,
            run_id,
            partition,
        )
        .await
    }

    async fn materialize_for_run_inner(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        let view_version = view_version.into();
        let records = KgViewRecords::from_graph(graph)?;

        let entity_key = AssetPathBuilder::gold_asset_key(
            KG_SOCIETY_VIEW_ASSET_ID,
            &view_version,
            "entities/part-00000.parquet",
        );
        let entity_meta = self
            .lake
            .put_bytes(&entity_key, write_entities_parquet(&records.entities)?)
            .await?;

        let fact_key = AssetPathBuilder::gold_asset_key(
            KG_SOCIETY_VIEW_ASSET_ID,
            &view_version,
            "facts/part-00000.parquet",
        );
        let fact_meta = self
            .lake
            .put_bytes(&fact_key, write_facts_parquet(&records.facts)?)
            .await?;

        let fact_annotation_key = AssetPathBuilder::gold_asset_key(
            KG_SOCIETY_VIEW_ASSET_ID,
            &view_version,
            "fact_annotations/part-00000.parquet",
        );
        let fact_annotation_meta = self
            .lake
            .put_bytes(
                &fact_annotation_key,
                write_fact_annotations_parquet(&records.fact_annotations)?,
            )
            .await?;

        let edge_key = AssetPathBuilder::gold_asset_key(
            KG_SOCIETY_VIEW_ASSET_ID,
            &view_version,
            "edges/part-00000.parquet",
        );
        let edge_meta = self
            .lake
            .put_bytes(&edge_key, write_edges_parquet(&records.edges)?)
            .await?;

        let artifacts = vec![
            artifact(
                KgViewArtifactKind::EntitiesParquet,
                &entity_meta,
                "application/vnd.apache.parquet",
                Some(records.entities.len() as u64),
            ),
            artifact(
                KgViewArtifactKind::FactsParquet,
                &fact_meta,
                "application/vnd.apache.parquet",
                Some(records.facts.len() as u64),
            ),
            artifact(
                KgViewArtifactKind::FactAnnotationsParquet,
                &fact_annotation_meta,
                "application/vnd.apache.parquet",
                Some(records.fact_annotations.len() as u64),
            ),
            artifact(
                KgViewArtifactKind::EdgesParquet,
                &edge_meta,
                "application/vnd.apache.parquet",
                Some(records.edges.len() as u64),
            ),
        ];

        let manifest = KgViewManifest {
            view_version: view_version.clone(),
            format_version: 1,
            created_at: Utc::now(),
            graph_content_hash: records.content_hash.clone(),
            entity_count: records.entities.len() as u64,
            fact_count: records.facts.len() as u64,
            fact_annotation_count: records.fact_annotations.len() as u64,
            edge_count: records.edges.len() as u64,
            entity_parquet_key: entity_key.to_string(),
            fact_parquet_key: fact_key.to_string(),
            fact_annotation_parquet_key: fact_annotation_key.to_string(),
            edge_parquet_key: edge_key.to_string(),
            artifacts,
        };

        let manifest_key = AssetPathBuilder::gold_asset_key(
            KG_SOCIETY_VIEW_ASSET_ID,
            &view_version,
            "manifest.json",
        );
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;

        let mut artifact_refs = vec![
            ArtifactRef::parquet(entity_meta),
            ArtifactRef::parquet(fact_meta),
            ArtifactRef::parquet(fact_annotation_meta),
            ArtifactRef::parquet(edge_meta),
            ArtifactRef::json(manifest_meta),
        ];
        artifact_refs.sort_by(|left, right| left.key.cmp(&right.key));

        let mut record_watermarks = vec![SourceWatermark {
            source: "knowledge_graph_content_hash".to_string(),
            high_watermark: records.content_hash.clone(),
        }];
        record_watermarks.extend(source_watermarks);

        let record = MaterializationRecord::succeeded(
            AssetId::new(KG_SOCIETY_VIEW_ASSET_ID)
                .expect("static KG society view asset id is valid"),
            AssetStage::Gold,
            partition,
            view_version,
            artifact_refs,
        )
        .with_run_id(run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(record_watermarks)
        .with_row_count(
            manifest.entity_count
                + manifest.fact_count
                + manifest.fact_annotation_count
                + manifest.edge_count,
        );

        self.materializations.write_materialization(&record).await?;
        self.materializations.promote_current(&record).await?;

        Ok(KgSocietyViewMaterialization {
            manifest,
            record,
            records,
        })
    }
}

fn write_entities_parquet(
    entities: &[KgViewEntityRecord],
) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("root_source", DataType::Utf8, true),
        Field::new("fact_count", DataType::UInt32, false),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("updated_at", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(entities.iter().map(|entity| entity.entity_id.clone())),
            string_array(entities.iter().map(|entity| entity.entity_type.clone())),
            string_array(entities.iter().map(|entity| entity.name.clone())),
            optional_string_array(entities.iter().map(|entity| entity.root_source.clone())),
            Arc::new(UInt32Array::from(
                entities
                    .iter()
                    .map(|entity| entity.fact_count)
                    .collect::<Vec<_>>(),
            )),
            string_array(entities.iter().map(|entity| entity.created_at.to_rfc3339())),
            string_array(entities.iter().map(|entity| entity.updated_at.to_rfc3339())),
        ],
    )
    .map_err(KgSocietyViewMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_facts_parquet(
    facts: &[KgViewFactRecord],
) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("fact_version", DataType::UInt32, false),
        Field::new("value_type", DataType::Utf8, false),
        Field::new("value_text", DataType::Utf8, true),
        Field::new("value_json", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("triggered_by", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(facts.iter().map(|fact| fact.entity_id.clone())),
            string_array(facts.iter().map(|fact| fact.fact_key.clone())),
            Arc::new(UInt32Array::from(
                facts
                    .iter()
                    .map(|fact| fact.fact_version)
                    .collect::<Vec<_>>(),
            )),
            string_array(facts.iter().map(|fact| fact.value_type.clone())),
            optional_string_array(facts.iter().map(|fact| fact.value_text.clone())),
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
        ],
    )
    .map_err(KgSocietyViewMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_fact_annotations_parquet(
    annotations: &[KgViewFactAnnotationRecord],
) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
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
    .map_err(KgSocietyViewMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_edges_parquet(
    edges: &[KgViewEdgeRecord],
) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("from_entity_id", DataType::Utf8, false),
        Field::new("to_entity_id", DataType::Utf8, false),
        Field::new("relation", DataType::Utf8, false),
        Field::new("weight", DataType::Float32, false),
        Field::new("metadata_json", DataType::Utf8, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("triggered_by", DataType::Utf8, true),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(edges.iter().map(|edge| edge.from_entity_id.clone())),
            string_array(edges.iter().map(|edge| edge.to_entity_id.clone())),
            string_array(edges.iter().map(|edge| edge.relation.clone())),
            Arc::new(Float32Array::from(
                edges.iter().map(|edge| edge.weight).collect::<Vec<_>>(),
            )),
            string_array(edges.iter().map(|edge| edge.metadata_json.clone())),
            string_array(edges.iter().map(|edge| edge.source_type.clone())),
            optional_string_array(edges.iter().map(|edge| edge.source_url.clone())),
            optional_string_array(edges.iter().map(|edge| edge.model.clone())),
            optional_string_array(edges.iter().map(|edge| edge.skill_id.clone())),
            optional_string_array(edges.iter().map(|edge| edge.triggered_by.clone())),
        ],
    )
    .map_err(KgSocietyViewMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(props))
        .map_err(KgSocietyViewMaterializeError::Parquet)?;
    writer
        .write(&batch)
        .map_err(KgSocietyViewMaterializeError::Parquet)?;
    writer
        .close()
        .map_err(KgSocietyViewMaterializeError::Parquet)?;
    Ok(bytes)
}

fn string_array(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_string_array(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn artifact(
    kind: KgViewArtifactKind,
    meta: &ArtifactMetadata,
    format: &str,
    row_count: Option<u64>,
) -> KgViewArtifact {
    KgViewArtifact {
        kind,
        key: meta.key.to_string(),
        format: format.to_string(),
        content_hash: meta.content_hash.clone(),
        hash_algorithm: meta.hash_algorithm.clone(),
        size_bytes: meta.size_bytes,
        row_count,
    }
}

fn fact_value_type(value: &FactValue) -> &'static str {
    match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    }
}

fn fact_value_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Numeric(value) => Some(format!("{value}")),
        FactValue::Text(value) => Some(value.clone()),
        FactValue::Bool(value) => Some(format!("{value}")),
        FactValue::Tags(values) => Some(values.join(" ")),
        FactValue::Score { value, explanation } => Some(format!("{value} {explanation}")),
    }
}

fn metadata_json(metadata: &HashMap<String, String>) -> String {
    serde_json::to_string(metadata).expect("edge metadata should serialize")
}

fn content_hash(
    entities: &[KgViewEntityRecord],
    facts: &[KgViewFactRecord],
    fact_annotations: &[KgViewFactAnnotationRecord],
    edges: &[KgViewEdgeRecord],
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(&(entities, facts, fact_annotations, edges))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

#[derive(Debug)]
pub enum KgSocietyViewMaterializeError {
    Arrow(arrow::error::ArrowError),
    Json(serde_json::Error),
    Lake(LakeError),
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for KgSocietyViewMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "KG view Arrow record batch error: {err}"),
            Self::Json(err) => write!(f, "KG view JSON error: {err}"),
            Self::Lake(err) => write!(f, "KG view lake error: {err}"),
            Self::Parquet(err) => write!(f, "KG view Parquet error: {err}"),
        }
    }
}

impl std::error::Error for KgSocietyViewMaterializeError {}

impl From<serde_json::Error> for KgSocietyViewMaterializeError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<LakeError> for KgSocietyViewMaterializeError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}
