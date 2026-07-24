use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float32Array, Float64Array, StringArray, UInt32Array,
};
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
use crate::lake::keys::KeyError;
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::skill_facts::{
    read_fact_annotation_records, read_facts_parquet_records, write_fact_annotations_parquet,
    write_facts_parquet, SkillFactMaterializeError,
};
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    KgViewEdgeRecord, KgViewEntityRecord, MaterializationId, MaterializationRecord,
    SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const RERA_REGISTRY_MONTHLY_ASSET_ID: &str = "rera_registry_monthly";
pub const CANONICAL_SOCIETY_NODES_ASSET_ID: &str = "canonical_society_nodes";
pub const RERA_LEGAL_FACTS_ASSET_ID: &str = "rera_legal_facts";
const RERA_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraProjectSnapshotRecord {
    pub ack_number: Option<String>,
    pub registration_number: Option<String>,
    pub project_name: String,
    pub promoter_name: Option<String>,
    pub status: Option<String>,
    pub project_type: Option<String>,
    pub project_address: Option<String>,
    pub area_name: Option<String>,
    pub district: Option<String>,
    pub taluk: Option<String>,
    pub total_land_area_sqm: Option<f64>,
    pub land_litigation: Option<bool>,
    pub source_url: String,
    pub fetched_at: DateTime<Utc>,
}

impl ReraProjectSnapshotRecord {
    fn project_key(&self) -> String {
        self.registration_number
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                self.ack_number
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::to_string)
            .unwrap_or_else(|| format!("project:{}", slug(&self.project_name)))
    }

    fn society_entity_id(&self) -> String {
        let digest = sha256_hex(self.project_key().as_bytes());
        format!("society:rera-{}", &digest[..16])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraRegistryMonthlyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub projects: Vec<ReraProjectSnapshotRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_facts: Vec<SkillFactRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_fact_annotations: Vec<SkillFactAnnotationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraAssetManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub detail_fact_count: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub detail_fact_annotation_count: u64,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalSocietyRows {
    pub entities: Vec<KgViewEntityRecord>,
    pub edges: Vec<KgViewEdgeRecord>,
    pub mappings: Vec<ReraCanonicalMappingRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReraCanonicalMappingRecord {
    pub project_key: String,
    pub canonical_entity_id: String,
    pub alias_entity_id: Option<String>,
    pub project_name: String,
    pub registration_number: Option<String>,
    pub ack_number: Option<String>,
}

#[derive(Clone)]
pub struct ReraRegistryMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ReraRegistryMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        input: &ReraRegistryMonthlyInput,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<MaterializationRecord, ReraAssetError> {
        validate_rera_input(input)?;
        let run_id = dag_run_id.to_string();
        let project_key = AssetPathBuilder::raw_snapshot_key(
            "rera",
            &partition,
            &run_id,
            "projects/part-00000.parquet",
        );
        let project_meta = self
            .lake
            .put_bytes(&project_key, write_rera_projects(&input.projects)?)
            .await?;
        let mut artifacts = vec![ArtifactRef::parquet(project_meta)];
        if !input.detail_facts.is_empty() {
            let fact_key = AssetPathBuilder::raw_snapshot_key(
                "rera",
                &partition,
                &run_id,
                "detail_facts/part-00000.parquet",
            );
            artifacts.push(ArtifactRef::parquet(
                self.lake
                    .put_bytes(&fact_key, write_facts_parquet(&input.detail_facts)?)
                    .await?,
            ));

            let annotation_key = AssetPathBuilder::raw_snapshot_key(
                "rera",
                &partition,
                &run_id,
                "detail_fact_annotations/part-00000.parquet",
            );
            artifacts.push(ArtifactRef::parquet(
                self.lake
                    .put_bytes(
                        &annotation_key,
                        write_fact_annotations_parquet(&input.detail_fact_annotations)?,
                    )
                    .await?,
            ));
        }

        let manifest_key =
            AssetPathBuilder::raw_snapshot_key("rera", &partition, &run_id, "manifest.json");
        let manifest = ReraAssetManifest {
            asset_id: RERA_REGISTRY_MONTHLY_ASSET_ID.to_string(),
            format_version: RERA_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            row_count: input.projects.len() as u64,
            detail_fact_count: input.detail_facts.len() as u64,
            detail_fact_annotation_count: input.detail_fact_annotations.len() as u64,
            artifacts: artifacts.clone(),
        };
        artifacts.push(ArtifactRef::json(
            self.lake.put_json(&manifest_key, &manifest).await?,
        ));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id(RERA_REGISTRY_MONTHLY_ASSET_ID),
            AssetStage::Raw,
            partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_source_watermarks(rera_watermarks(input))
        .with_row_count(input.projects.len() as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

#[derive(Clone)]
pub struct CanonicalSocietyMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl CanonicalSocietyMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_from_rera_for_run(
        &self,
        rera_record: &MaterializationRecord,
        version: &str,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<MaterializationRecord, ReraAssetError> {
        let projects = read_rera_project_rows(&self.lake, rera_record).await?;
        let rows = canonical_rows(&projects);
        let entity_key = AssetPathBuilder::gold_asset_key(
            CANONICAL_SOCIETY_NODES_ASSET_ID,
            version,
            "entities/part-00000.parquet",
        );
        let edge_key = AssetPathBuilder::gold_asset_key(
            CANONICAL_SOCIETY_NODES_ASSET_ID,
            version,
            "edges/part-00000.parquet",
        );
        let mapping_key = AssetPathBuilder::gold_asset_key(
            CANONICAL_SOCIETY_NODES_ASSET_ID,
            version,
            "mappings/part-00000.parquet",
        );
        let mut artifacts = vec![
            ArtifactRef::parquet(
                self.lake
                    .put_bytes(&entity_key, write_entities(&rows.entities)?)
                    .await?,
            ),
            ArtifactRef::parquet(
                self.lake
                    .put_bytes(&edge_key, write_edges(&rows.edges)?)
                    .await?,
            ),
            ArtifactRef::parquet(
                self.lake
                    .put_bytes(&mapping_key, write_mappings(&rows.mappings)?)
                    .await?,
            ),
        ];
        let manifest_key = AssetPathBuilder::gold_asset_key(
            CANONICAL_SOCIETY_NODES_ASSET_ID,
            version,
            "manifest.json",
        );
        let manifest = ReraAssetManifest {
            asset_id: CANONICAL_SOCIETY_NODES_ASSET_ID.to_string(),
            format_version: RERA_FORMAT_VERSION,
            snapshot_date: rera_record.version.clone(),
            run_id: dag_run_id.to_string(),
            created_at: Utc::now(),
            row_count: rows.entities.len() as u64,
            detail_fact_count: 0,
            detail_fact_annotation_count: 0,
            artifacts: artifacts.clone(),
        };
        artifacts.push(ArtifactRef::json(
            self.lake.put_json(&manifest_key, &manifest).await?,
        ));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id(CANONICAL_SOCIETY_NODES_ASSET_ID),
            AssetStage::Gold,
            partition,
            version,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(vec![rera_record.materialization_id.clone()])
        .with_source_watermarks(rera_record.source_watermarks.clone())
        .with_row_count(rows.entities.len() as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

pub async fn rera_legal_facts_input(
    lake: &LakeStore,
    rera_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, ReraAssetError> {
    let projects = read_rera_project_rows(lake, rera_record).await?;
    let canonical = read_canonical_society_rows(lake, canonical_record).await?;
    let canonical_by_project: BTreeMap<_, _> = canonical
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.project_key.as_str(),
                mapping.canonical_entity_id.as_str(),
            )
        })
        .collect();
    let mut facts = Vec::new();
    let mut annotations = Vec::new();

    for project in projects {
        let project_key = project.project_key();
        let entity_id = canonical_by_project
            .get(project_key.as_str())
            .ok_or_else(|| {
                ReraAssetError::InvalidArtifact(format!(
                    "canonical mapping is missing for RERA project {project_key}"
                ))
            })?;
        append_project_facts(&project, entity_id, run_id, &mut facts, &mut annotations)?;
        if let Some(alias_entity_id) = canonical
            .mappings
            .iter()
            .find(|mapping| mapping.project_key == project_key)
            .and_then(|mapping| mapping.alias_entity_id.as_deref())
            .filter(|alias| *alias != *entity_id)
        {
            append_project_facts(
                &project,
                alias_entity_id,
                run_id,
                &mut facts,
                &mut annotations,
            )?;
        }
    }

    let detail_rows = read_rera_detail_fact_rows(lake, rera_record).await?;
    let (facts, annotations) = merge_current_fact_rows(
        facts,
        annotations,
        detail_rows.facts,
        detail_rows.fact_annotations,
    );

    Ok(SkillFactsInput {
        source: "rera".to_string(),
        snapshot_date: rera_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: rera_record.source_watermarks.clone(),
    })
}

async fn read_rera_detail_fact_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<super::SkillFactArtifactRows, ReraAssetError> {
    if !record
        .artifacts
        .iter()
        .any(|artifact| artifact.key.ends_with("detail_facts/part-00000.parquet"))
    {
        return Ok(super::SkillFactArtifactRows::default());
    }

    let facts = read_facts_parquet_records(
        read_parquet_artifact(lake, record, "detail_facts/part-00000.parquet").await?,
    )?;
    let fact_annotations = read_fact_annotation_records(
        read_parquet_artifact(lake, record, "detail_fact_annotations/part-00000.parquet").await?,
    )?;
    Ok(super::SkillFactArtifactRows {
        facts,
        fact_annotations,
    })
}

fn merge_current_fact_rows(
    base_facts: Vec<SkillFactRecord>,
    base_annotations: Vec<SkillFactAnnotationRecord>,
    detail_facts: Vec<SkillFactRecord>,
    detail_annotations: Vec<SkillFactAnnotationRecord>,
) -> (Vec<SkillFactRecord>, Vec<SkillFactAnnotationRecord>) {
    type FactKey = (String, String);

    let mut annotations_by_key: BTreeMap<FactKey, SkillFactAnnotationRecord> = base_annotations
        .into_iter()
        .map(|annotation| {
            (
                (annotation.entity_id.clone(), annotation.fact_key.clone()),
                annotation,
            )
        })
        .collect();
    let detail_annotations_by_key: BTreeMap<FactKey, SkillFactAnnotationRecord> =
        detail_annotations
            .into_iter()
            .map(|annotation| {
                (
                    (annotation.entity_id.clone(), annotation.fact_key.clone()),
                    annotation,
                )
            })
            .collect();
    let mut current: BTreeMap<FactKey, (SkillFactRecord, bool)> = base_facts
        .into_iter()
        .map(|fact| {
            (
                (fact.entity_id.clone(), fact.fact_key.clone()),
                (fact, false),
            )
        })
        .collect();

    for fact in detail_facts {
        let key = (fact.entity_id.clone(), fact.fact_key.clone());
        let replace = current
            .get(&key)
            .is_none_or(|(existing, existing_is_detail)| {
                fact.learned_at > existing.learned_at
                    || (fact.learned_at == existing.learned_at
                        && (!*existing_is_detail || fact.confidence > existing.confidence))
            });
        if replace {
            current.insert(key.clone(), (fact, true));
            if let Some(annotation) = detail_annotations_by_key.get(&key) {
                annotations_by_key.insert(key, annotation.clone());
            } else {
                annotations_by_key.remove(&key);
            }
        }
    }

    let mut facts = Vec::with_capacity(current.len());
    let mut annotations = Vec::with_capacity(current.len());
    for (key, (fact, _)) in current {
        facts.push(fact);
        if let Some(annotation) = annotations_by_key.remove(&key) {
            annotations.push(annotation);
        }
    }
    (facts, annotations)
}

pub async fn read_rera_project_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<ReraProjectSnapshotRecord>, ReraAssetError> {
    let bytes = read_parquet_artifact(lake, record, "projects/part-00000.parquet").await?;
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?
        .build()
        .map_err(ReraAssetError::Parquet)?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch.map_err(ReraAssetError::Arrow)?;
        let project_names = required_strings(&batch, "project_name")?;
        let source_urls = required_strings(&batch, "source_url")?;
        let fetched_at = required_strings(&batch, "fetched_at")?;
        for row in 0..batch.num_rows() {
            rows.push(ReraProjectSnapshotRecord {
                ack_number: optional_string(&batch, "ack_number", row)?,
                registration_number: optional_string(&batch, "registration_number", row)?,
                project_name: project_names.value(row).to_string(),
                promoter_name: optional_string(&batch, "promoter_name", row)?,
                status: optional_string(&batch, "status", row)?,
                project_type: optional_string(&batch, "project_type", row)?,
                project_address: optional_string(&batch, "project_address", row)?,
                area_name: optional_string(&batch, "area_name", row)?,
                district: optional_string(&batch, "district", row)?,
                taluk: optional_string(&batch, "taluk", row)?,
                total_land_area_sqm: optional_f64(&batch, "total_land_area_sqm", row)?,
                land_litigation: optional_bool(&batch, "land_litigation", row)?,
                source_url: source_urls.value(row).to_string(),
                fetched_at: DateTime::parse_from_rfc3339(fetched_at.value(row))
                    .map_err(ReraAssetError::Timestamp)?
                    .with_timezone(&Utc),
            });
        }
    }
    Ok(rows)
}

pub async fn read_canonical_society_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<CanonicalSocietyRows, ReraAssetError> {
    let entity_bytes = read_parquet_artifact(lake, record, "entities/part-00000.parquet").await?;
    let edge_bytes = read_parquet_artifact(lake, record, "edges/part-00000.parquet").await?;
    let mapping_bytes = read_parquet_artifact(lake, record, "mappings/part-00000.parquet").await?;
    Ok(CanonicalSocietyRows {
        entities: read_entities(entity_bytes)?,
        edges: read_edges(edge_bytes)?,
        mappings: read_mappings(mapping_bytes)?,
    })
}

fn canonical_rows(projects: &[ReraProjectSnapshotRecord]) -> CanonicalSocietyRows {
    let mut entities = BTreeMap::<String, KgViewEntityRecord>::new();
    let mut edges = BTreeMap::<(String, String, String), KgViewEdgeRecord>::new();
    let mut mappings = BTreeMap::<String, ReraCanonicalMappingRecord>::new();
    let mut name_counts = BTreeMap::<String, usize>::new();
    for project in projects {
        *name_counts.entry(slug(&project.project_name)).or_default() += 1;
    }
    for project in projects {
        let society_id = project.society_entity_id();
        let project_key = project.project_key();
        let name_slug = slug(&project.project_name);
        mappings.insert(
            project_key.clone(),
            ReraCanonicalMappingRecord {
                project_key,
                canonical_entity_id: society_id.clone(),
                alias_entity_id: (name_counts.get(&name_slug) == Some(&1))
                    .then(|| format!("society:{name_slug}")),
                project_name: project.project_name.clone(),
                registration_number: project.registration_number.clone(),
                ack_number: project.ack_number.clone(),
            },
        );
        insert_entity(
            &mut entities,
            society_id.clone(),
            "society",
            &project.project_name,
            project.fetched_at,
        );
        if let Some(area_name) = nonempty(project.area_name.as_deref()) {
            let area_id = format!("area:{}", slug(area_name));
            insert_entity(
                &mut entities,
                area_id.clone(),
                "area",
                area_name,
                project.fetched_at,
            );
            insert_edge(
                &mut edges,
                &society_id,
                &area_id,
                "SocietyInArea",
                &project.source_url,
            );
        }
        if let Some(promoter) = nonempty(project.promoter_name.as_deref()) {
            let builder_id = format!("builder:{}", slug(promoter));
            insert_entity(
                &mut entities,
                builder_id.clone(),
                "builder",
                promoter,
                project.fetched_at,
            );
            insert_edge(
                &mut edges,
                &society_id,
                &builder_id,
                "BuiltBy",
                &project.source_url,
            );
        }
    }
    CanonicalSocietyRows {
        entities: entities.into_values().collect(),
        edges: edges.into_values().collect(),
        mappings: mappings.into_values().collect(),
    }
}

fn insert_entity(
    entities: &mut BTreeMap<String, KgViewEntityRecord>,
    entity_id: String,
    entity_type: &str,
    name: &str,
    timestamp: DateTime<Utc>,
) {
    entities
        .entry(entity_id.clone())
        .or_insert_with(|| KgViewEntityRecord {
            entity_id,
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: Some("rera".to_string()),
            fact_count: 0,
            created_at: timestamp,
            updated_at: timestamp,
        });
}

fn insert_edge(
    edges: &mut BTreeMap<(String, String, String), KgViewEdgeRecord>,
    from: &str,
    to: &str,
    relation: &str,
    source_url: &str,
) {
    let key = (from.to_string(), to.to_string(), relation.to_string());
    edges.entry(key).or_insert_with(|| KgViewEdgeRecord {
        from_entity_id: from.to_string(),
        to_entity_id: to.to_string(),
        relation: relation.to_string(),
        weight: 1.0,
        metadata_json: "{}".to_string(),
        source_type: "Rera".to_string(),
        source_url: Some(source_url.to_string()),
        model: None,
        skill_id: Some("fetch_rera".to_string()),
        triggered_by: None,
    });
}

fn append_project_facts(
    project: &ReraProjectSnapshotRecord,
    entity_id: &str,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ReraAssetError> {
    if nonempty(project.registration_number.as_deref()).is_some()
        || nonempty(project.ack_number.as_deref()).is_some()
    {
        push_fact(
            project,
            entity_id,
            run_id,
            "rera_registered",
            FactValue::Bool(true),
            "RERA Registered: {value}",
            &["rera verified", "legally verified", "verified project"],
            Some(("TextMatch", 3.0)),
            facts,
            annotations,
        )?;
    }
    for (key, value, template) in [
        (
            "rera_number",
            nonempty(project.registration_number.as_deref()),
            "RERA No: {value}",
        ),
        (
            "rera_ack_number",
            nonempty(project.ack_number.as_deref()),
            "RERA ACK: {value}",
        ),
        (
            "rera_status",
            nonempty(project.status.as_deref()),
            "RERA Status: {value}",
        ),
        (
            "rera_project_type",
            nonempty(project.project_type.as_deref()),
            "Project type: {value}",
        ),
        (
            "rera_project_address",
            nonempty(project.project_address.as_deref()),
            "Project address: {value}",
        ),
        (
            "rera_promoter_name",
            nonempty(project.promoter_name.as_deref()),
            "RERA Promoter: {value}",
        ),
    ] {
        if let Some(value) = value {
            push_fact(
                project,
                entity_id,
                run_id,
                key,
                FactValue::Text(value.to_string()),
                template,
                &[],
                None,
                facts,
                annotations,
            )?;
        }
    }
    if let Some(value) = project
        .total_land_area_sqm
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        push_fact(
            project,
            entity_id,
            run_id,
            "rera_total_land_area_sqm",
            FactValue::Numeric(value),
            "RERA land area: {value} sqm",
            &["large campus", "acres", "land area", "open space"],
            Some(("HigherBetter", 2.0)),
            facts,
            annotations,
        )?;
    }
    if let Some(value) = project.land_litigation {
        push_fact(
            project,
            entity_id,
            run_id,
            "rera_land_litigation",
            FactValue::Bool(value),
            "RERA land litigation: {value}",
            &["legal risk", "land litigation"],
            Some(("LowerBetter", 3.0)),
            facts,
            annotations,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    project: &ReraProjectSnapshotRecord,
    entity_id: &str,
    run_id: &MaterializationId,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    answers_preferences: &[&str],
    scoring: Option<(&str, f32)>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ReraAssetError> {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    let input_hash =
        sha256_hex(format!("{}:{fact_key}:{value_json}", project.project_key()).as_bytes());
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json,
        confidence: 1.0,
        source_type: "Rera".to_string(),
        source_url: Some(project.source_url.clone()),
        model: None,
        skill_id: Some("fetch_rera".to_string()),
        triggered_by: None,
        learned_at: project.fetched_at,
        run_id: run_id.to_string(),
        input_hash,
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: scoring.map(|(direction, _)| direction.to_string()),
        scoring_weight: scoring.map(|(_, weight)| weight),
        scoring_thresholds_json: "[]".to_string(),
    });
    Ok(())
}

fn write_rera_projects(records: &[ReraProjectSnapshotRecord]) -> Result<Vec<u8>, ReraAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("project_key", DataType::Utf8, false),
        Field::new("ack_number", DataType::Utf8, true),
        Field::new("registration_number", DataType::Utf8, true),
        Field::new("project_name", DataType::Utf8, false),
        Field::new("promoter_name", DataType::Utf8, true),
        Field::new("status", DataType::Utf8, true),
        Field::new("project_type", DataType::Utf8, true),
        Field::new("project_address", DataType::Utf8, true),
        Field::new("area_name", DataType::Utf8, true),
        Field::new("district", DataType::Utf8, true),
        Field::new("taluk", DataType::Utf8, true),
        Field::new("total_land_area_sqm", DataType::Float64, true),
        Field::new("land_litigation", DataType::Boolean, true),
        Field::new("source_url", DataType::Utf8, false),
        Field::new("fetched_at", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            strings(records.iter().map(ReraProjectSnapshotRecord::project_key)),
            optional_strings(records.iter().map(|record| record.ack_number.clone())),
            optional_strings(
                records
                    .iter()
                    .map(|record| record.registration_number.clone()),
            ),
            strings(records.iter().map(|record| record.project_name.clone())),
            optional_strings(records.iter().map(|record| record.promoter_name.clone())),
            optional_strings(records.iter().map(|record| record.status.clone())),
            optional_strings(records.iter().map(|record| record.project_type.clone())),
            optional_strings(records.iter().map(|record| record.project_address.clone())),
            optional_strings(records.iter().map(|record| record.area_name.clone())),
            optional_strings(records.iter().map(|record| record.district.clone())),
            optional_strings(records.iter().map(|record| record.taluk.clone())),
            Arc::new(Float64Array::from(
                records
                    .iter()
                    .map(|record| record.total_land_area_sqm)
                    .collect::<Vec<_>>(),
            )),
            Arc::new(BooleanArray::from(
                records
                    .iter()
                    .map(|record| record.land_litigation)
                    .collect::<Vec<_>>(),
            )),
            strings(records.iter().map(|record| record.source_url.clone())),
            strings(records.iter().map(|record| record.fetched_at.to_rfc3339())),
        ],
    )?;
    write_batch(batch)
}

pub(crate) fn write_entities(records: &[KgViewEntityRecord]) -> Result<Vec<u8>, ReraAssetError> {
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
        schema,
        vec![
            strings(records.iter().map(|record| record.entity_id.clone())),
            strings(records.iter().map(|record| record.entity_type.clone())),
            strings(records.iter().map(|record| record.name.clone())),
            optional_strings(records.iter().map(|record| record.root_source.clone())),
            Arc::new(UInt32Array::from(
                records
                    .iter()
                    .map(|record| record.fact_count)
                    .collect::<Vec<_>>(),
            )),
            strings(records.iter().map(|record| record.created_at.to_rfc3339())),
            strings(records.iter().map(|record| record.updated_at.to_rfc3339())),
        ],
    )?;
    write_batch(batch)
}

pub(crate) fn write_edges(records: &[KgViewEdgeRecord]) -> Result<Vec<u8>, ReraAssetError> {
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
        schema,
        vec![
            strings(records.iter().map(|record| record.from_entity_id.clone())),
            strings(records.iter().map(|record| record.to_entity_id.clone())),
            strings(records.iter().map(|record| record.relation.clone())),
            Arc::new(Float32Array::from(
                records
                    .iter()
                    .map(|record| record.weight)
                    .collect::<Vec<_>>(),
            )),
            strings(records.iter().map(|record| record.metadata_json.clone())),
            strings(records.iter().map(|record| record.source_type.clone())),
            optional_strings(records.iter().map(|record| record.source_url.clone())),
            optional_strings(records.iter().map(|record| record.model.clone())),
            optional_strings(records.iter().map(|record| record.skill_id.clone())),
            optional_strings(records.iter().map(|record| record.triggered_by.clone())),
        ],
    )?;
    write_batch(batch)
}

fn write_mappings(records: &[ReraCanonicalMappingRecord]) -> Result<Vec<u8>, ReraAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("project_key", DataType::Utf8, false),
        Field::new("canonical_entity_id", DataType::Utf8, false),
        Field::new("alias_entity_id", DataType::Utf8, true),
        Field::new("project_name", DataType::Utf8, false),
        Field::new("registration_number", DataType::Utf8, true),
        Field::new("ack_number", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            strings(records.iter().map(|record| record.project_key.clone())),
            strings(
                records
                    .iter()
                    .map(|record| record.canonical_entity_id.clone()),
            ),
            optional_strings(records.iter().map(|record| record.alias_entity_id.clone())),
            strings(records.iter().map(|record| record.project_name.clone())),
            optional_strings(
                records
                    .iter()
                    .map(|record| record.registration_number.clone()),
            ),
            optional_strings(records.iter().map(|record| record.ack_number.clone())),
        ],
    )?;
    write_batch(batch)
}

pub(crate) fn read_entities(bytes: Vec<u8>) -> Result<Vec<KgViewEntityRecord>, ReraAssetError> {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            rows.push(KgViewEntityRecord {
                entity_id: required_string(&batch, "entity_id", row)?,
                entity_type: required_string(&batch, "entity_type", row)?,
                name: required_string(&batch, "name", row)?,
                root_source: optional_string(&batch, "root_source", row)?,
                fact_count: required_u32(&batch, "fact_count", row)?,
                created_at: parse_timestamp(&batch, "created_at", row)?,
                updated_at: parse_timestamp(&batch, "updated_at", row)?,
            });
        }
    }
    Ok(rows)
}

pub(crate) fn read_edges(bytes: Vec<u8>) -> Result<Vec<KgViewEdgeRecord>, ReraAssetError> {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            rows.push(KgViewEdgeRecord {
                from_entity_id: required_string(&batch, "from_entity_id", row)?,
                to_entity_id: required_string(&batch, "to_entity_id", row)?,
                relation: required_string(&batch, "relation", row)?,
                weight: required_f32(&batch, "weight", row)?,
                metadata_json: required_string(&batch, "metadata_json", row)?,
                source_type: required_string(&batch, "source_type", row)?,
                source_url: optional_string(&batch, "source_url", row)?,
                model: optional_string(&batch, "model", row)?,
                skill_id: optional_string(&batch, "skill_id", row)?,
                triggered_by: optional_string(&batch, "triggered_by", row)?,
            });
        }
    }
    Ok(rows)
}

fn read_mappings(bytes: Vec<u8>) -> Result<Vec<ReraCanonicalMappingRecord>, ReraAssetError> {
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()?;
    let mut rows = Vec::new();
    for batch in &mut reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            rows.push(ReraCanonicalMappingRecord {
                project_key: required_string(&batch, "project_key", row)?,
                canonical_entity_id: required_string(&batch, "canonical_entity_id", row)?,
                alias_entity_id: optional_string(&batch, "alias_entity_id", row)?,
                project_name: required_string(&batch, "project_name", row)?,
                registration_number: optional_string(&batch, "registration_number", row)?,
                ack_number: optional_string(&batch, "ack_number", row)?,
            });
        }
    }
    Ok(rows)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, ReraAssetError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(bytes)
}

async fn read_parquet_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
) -> Result<Vec<u8>, ReraAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .ok_or_else(|| ReraAssetError::MissingArtifact {
            asset_id: record.asset_id.clone(),
            suffix: suffix.to_string(),
        })?;
    if artifact.content_type != "application/vnd.apache.parquet" {
        return Err(ReraAssetError::InvalidArtifact(format!(
            "asset {} artifact {} is not Parquet",
            record.asset_id, artifact.key
        )));
    }
    let key = LakeKey::new(artifact.key.clone()).map_err(ReraAssetError::Key)?;
    let bytes = lake.get_bytes(&key).await?;
    if bytes.len() != artifact.size_bytes || sha256_hex(&bytes) != artifact.content_hash {
        return Err(ReraAssetError::InvalidArtifact(format!(
            "asset {} artifact {} failed size or checksum validation",
            record.asset_id, artifact.key
        )));
    }
    Ok(bytes)
}

fn required_strings<'a>(
    batch: &'a RecordBatch,
    column: &str,
) -> Result<&'a StringArray, ReraAssetError> {
    batch
        .column_by_name(column)
        .and_then(|array| array.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| ReraAssetError::InvalidArtifact(format!("missing string column {column}")))
}

fn required_string(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<String, ReraAssetError> {
    let values = required_strings(batch, column)?;
    if values.is_null(row) {
        return Err(ReraAssetError::InvalidArtifact(format!(
            "null value in required column {column} at row {row}"
        )));
    }
    Ok(values.value(row).to_string())
}

fn optional_string(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<Option<String>, ReraAssetError> {
    let values = required_strings(batch, column)?;
    Ok((!values.is_null(row)).then(|| values.value(row).to_string()))
}

fn optional_f64(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<Option<f64>, ReraAssetError> {
    let values = batch
        .column_by_name(column)
        .and_then(|array| array.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| ReraAssetError::InvalidArtifact(format!("missing f64 column {column}")))?;
    Ok((!values.is_null(row)).then(|| values.value(row)))
}

fn optional_bool(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<Option<bool>, ReraAssetError> {
    let values = batch
        .column_by_name(column)
        .and_then(|array| array.as_any().downcast_ref::<BooleanArray>())
        .ok_or_else(|| ReraAssetError::InvalidArtifact(format!("missing bool column {column}")))?;
    Ok((!values.is_null(row)).then(|| values.value(row)))
}

fn required_u32(batch: &RecordBatch, column: &str, row: usize) -> Result<u32, ReraAssetError> {
    batch
        .column_by_name(column)
        .and_then(|array| array.as_any().downcast_ref::<UInt32Array>())
        .filter(|array| !array.is_null(row))
        .map(|array| array.value(row))
        .ok_or_else(|| ReraAssetError::InvalidArtifact(format!("missing u32 column {column}")))
}

fn required_f32(batch: &RecordBatch, column: &str, row: usize) -> Result<f32, ReraAssetError> {
    batch
        .column_by_name(column)
        .and_then(|array| array.as_any().downcast_ref::<Float32Array>())
        .filter(|array| !array.is_null(row))
        .map(|array| array.value(row))
        .ok_or_else(|| ReraAssetError::InvalidArtifact(format!("missing f32 column {column}")))
}

fn parse_timestamp(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<DateTime<Utc>, ReraAssetError> {
    Ok(
        DateTime::parse_from_rfc3339(&required_string(batch, column, row)?)
            .map_err(ReraAssetError::Timestamp)?
            .with_timezone(&Utc),
    )
}

fn strings(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_strings(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn rera_watermarks(input: &ReraRegistryMonthlyInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: "karnataka_rera".to_string(),
        high_watermark: input
            .projects
            .iter()
            .map(|project| project.fetched_at)
            .max()
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| input.snapshot_date.clone()),
    }]
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn validate_rera_input(input: &ReraRegistryMonthlyInput) -> Result<(), ReraAssetError> {
    if input.projects.is_empty() {
        return Err(ReraAssetError::InvalidInput(
            "monthly RERA snapshot cannot be empty".to_string(),
        ));
    }
    for (index, project) in input.projects.iter().enumerate() {
        if project.project_name.trim().is_empty() {
            return Err(ReraAssetError::InvalidInput(format!(
                "RERA project at index {index} has no project name"
            )));
        }
        if project.source_url.trim().is_empty() {
            return Err(ReraAssetError::InvalidInput(format!(
                "RERA project {} has no source URL",
                project.project_name
            )));
        }
    }
    let fact_keys = input
        .detail_facts
        .iter()
        .map(|fact| (fact.entity_id.as_str(), fact.fact_key.as_str()))
        .collect::<BTreeSet<_>>();
    if fact_keys.len() != input.detail_facts.len() {
        return Err(ReraAssetError::InvalidInput(
            "detailed RERA facts must be unique by entity and fact key".to_string(),
        ));
    }
    for fact in &input.detail_facts {
        if fact.entity_id.trim().is_empty() || fact.fact_key.trim().is_empty() {
            return Err(ReraAssetError::InvalidInput(
                "detailed RERA facts require entity_id and fact_key".to_string(),
            ));
        }
        if !fact.confidence.is_finite() || !(0.0..=1.0).contains(&fact.confidence) {
            return Err(ReraAssetError::InvalidInput(format!(
                "detailed RERA fact {} has invalid confidence {}",
                fact.fact_key, fact.confidence
            )));
        }
    }
    let annotation_keys = input
        .detail_fact_annotations
        .iter()
        .map(|annotation| (annotation.entity_id.as_str(), annotation.fact_key.as_str()))
        .collect::<BTreeSet<_>>();
    if annotation_keys.len() != input.detail_fact_annotations.len() {
        return Err(ReraAssetError::InvalidInput(
            "detailed RERA annotations must be unique by entity and fact key".to_string(),
        ));
    }
    if fact_keys != annotation_keys {
        return Err(ReraAssetError::InvalidInput(
            "every detailed RERA fact must have exactly one matching annotation".to_string(),
        ));
    }
    Ok(())
}

fn asset_id(value: &str) -> AssetId {
    AssetId::new(value).expect("static RERA asset id is valid")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
pub enum ReraAssetError {
    Arrow(arrow::error::ArrowError),
    InvalidArtifact(String),
    InvalidInput(String),
    Json(serde_json::Error),
    Key(KeyError),
    Lake(LakeError),
    MissingArtifact { asset_id: AssetId, suffix: String },
    Parquet(parquet::errors::ParquetError),
    SkillFact(SkillFactMaterializeError),
    Timestamp(chrono::ParseError),
}

impl fmt::Display for ReraAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "RERA Arrow error: {err}"),
            Self::InvalidArtifact(message) => write!(f, "invalid RERA artifact: {message}"),
            Self::InvalidInput(message) => write!(f, "invalid RERA input: {message}"),
            Self::Json(err) => write!(f, "RERA JSON error: {err}"),
            Self::Key(err) => write!(f, "RERA lake key error: {err}"),
            Self::Lake(err) => write!(f, "RERA lake error: {err}"),
            Self::MissingArtifact { asset_id, suffix } => {
                write!(f, "RERA asset {asset_id} is missing artifact {suffix}")
            }
            Self::Parquet(err) => write!(f, "RERA Parquet error: {err}"),
            Self::SkillFact(err) => write!(f, "RERA fact artifact error: {err}"),
            Self::Timestamp(err) => write!(f, "RERA timestamp error: {err}"),
        }
    }
}

impl std::error::Error for ReraAssetError {}

impl From<arrow::error::ArrowError> for ReraAssetError {
    fn from(value: arrow::error::ArrowError) -> Self {
        Self::Arrow(value)
    }
}

impl From<LakeError> for ReraAssetError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}

impl From<parquet::errors::ParquetError> for ReraAssetError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        Self::Parquet(value)
    }
}

impl From<serde_json::Error> for ReraAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<SkillFactMaterializeError> for ReraAssetError {
    fn from(value: SkillFactMaterializeError) -> Self {
        Self::SkillFact(value)
    }
}
