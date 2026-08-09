use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, StringArray, UInt32Array};
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

use crate::dag_config::{
    better_source_type_for_fact, load_fact_registry, load_resolution_policies,
};
use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};
use crate::parquet_data::{
    float64_list_array, float64_list_field, optional_f64_list_column_value,
    optional_string_list_column_value, string_list_array, string_list_field, typed_value_arrays,
    typed_value_fields, typed_value_from_batch, OptionalListColumn, TypedFactValue,
    ANSWERS_PREFERENCES_COLUMN, SCORING_THRESHOLDS_COLUMN,
};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SkillFactAnnotationRecord, SkillFactRecord,
    SourceWatermark,
};

pub const KG_SOCIETY_VIEW_ASSET_ID: &str = "kg_society_view";
const KG_SOCIETY_VIEW_FORMAT_VERSION: u32 = 2;

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
        sort_facts(&mut facts);
        sort_fact_annotations(&mut fact_annotations);

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

    pub fn from_graph_with_skill_facts(
        graph: &KnowledgeGraph,
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<Self, KgSocietyViewMaterializeError> {
        Self::from_graph_with_asset_rows(graph, &[], &[], support_facts, support_annotations)
    }

    pub fn from_graph_with_asset_rows(
        graph: &KnowledgeGraph,
        canonical_entities: &[KgViewEntityRecord],
        canonical_edges: &[KgViewEdgeRecord],
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<Self, KgSocietyViewMaterializeError> {
        let mut records = Self::from_graph(graph)?;
        let canonical_aliases = canonical_society_alias_map(canonical_entities);
        records.rewrite_entity_references(&canonical_aliases);
        let shadow_alias_entity_ids = canonical_aliases.keys().cloned().collect();
        records.remove_entities(&shadow_alias_entity_ids)?;
        records.merge_canonical_rows(canonical_entities, canonical_edges)?;
        let support_facts = rewrite_skill_facts(support_facts, &canonical_aliases);
        let support_annotations =
            rewrite_skill_annotations(support_annotations, &canonical_aliases);
        records.merge_skill_facts(&support_facts, &support_annotations)?;
        Ok(records)
    }

    fn rewrite_entity_references(&mut self, aliases: &HashMap<String, String>) {
        for fact in &mut self.facts {
            rewrite_entity_id(&mut fact.entity_id, aliases);
        }
        for annotation in &mut self.fact_annotations {
            rewrite_entity_id(&mut annotation.entity_id, aliases);
        }
        for edge in &mut self.edges {
            rewrite_entity_id(&mut edge.from_entity_id, aliases);
            rewrite_entity_id(&mut edge.to_entity_id, aliases);
        }
    }

    fn remove_entities(
        &mut self,
        entity_ids: &HashSet<String>,
    ) -> Result<(), KgSocietyViewMaterializeError> {
        if entity_ids.is_empty() {
            return Ok(());
        }

        self.entities
            .retain(|entity| !entity_ids.contains(&entity.entity_id));
        self.facts
            .retain(|fact| !entity_ids.contains(&fact.entity_id));
        self.fact_annotations
            .retain(|annotation| !entity_ids.contains(&annotation.entity_id));
        self.edges.retain(|edge| {
            !entity_ids.contains(&edge.from_entity_id) && !entity_ids.contains(&edge.to_entity_id)
        });
        self.update_entity_fact_counts();
        self.content_hash = content_hash(
            &self.entities,
            &self.facts,
            &self.fact_annotations,
            &self.edges,
        )?;
        Ok(())
    }

    fn merge_canonical_rows(
        &mut self,
        canonical_entities: &[KgViewEntityRecord],
        canonical_edges: &[KgViewEdgeRecord],
    ) -> Result<(), KgSocietyViewMaterializeError> {
        let mut entity_positions: HashMap<_, _> = self
            .entities
            .iter()
            .enumerate()
            .map(|(index, entity)| (entity.entity_id.clone(), index))
            .collect();
        for entity in canonical_entities {
            if let Some(index) = entity_positions.get(&entity.entity_id).copied() {
                let existing = &mut self.entities[index];
                existing.entity_type.clone_from(&entity.entity_type);
                existing.name.clone_from(&entity.name);
                existing.root_source.clone_from(&entity.root_source);
                existing.created_at = existing.created_at.min(entity.created_at);
                existing.updated_at = existing.updated_at.max(entity.updated_at);
            } else {
                entity_positions.insert(entity.entity_id.clone(), self.entities.len());
                self.entities.push(entity.clone());
            }
        }
        self.entities
            .sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

        let mut edge_positions: HashMap<_, _> = self
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                (
                    (
                        edge.from_entity_id.clone(),
                        edge.to_entity_id.clone(),
                        edge.relation.clone(),
                    ),
                    index,
                )
            })
            .collect();
        for edge in canonical_edges {
            let key = (
                edge.from_entity_id.clone(),
                edge.to_entity_id.clone(),
                edge.relation.clone(),
            );
            if let Some(index) = edge_positions.get(&key).copied() {
                self.edges[index] = edge.clone();
            } else {
                edge_positions.insert(key, self.edges.len());
                self.edges.push(edge.clone());
            }
        }
        self.edges.sort_by(|left, right| {
            left.from_entity_id
                .cmp(&right.from_entity_id)
                .then(left.to_entity_id.cmp(&right.to_entity_id))
                .then(left.relation.cmp(&right.relation))
        });
        self.content_hash = content_hash(
            &self.entities,
            &self.facts,
            &self.fact_annotations,
            &self.edges,
        )?;
        Ok(())
    }

    fn merge_skill_facts(
        &mut self,
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<(), KgSocietyViewMaterializeError> {
        let support_place_entities = support_place_entities(support_facts)?;
        merge_synthesized_entities(&mut self.entities, support_place_entities);
        let mut accepted_entities: HashSet<String> = self
            .entities
            .iter()
            .map(|entity| entity.entity_id.clone())
            .collect();
        let mut society_name_counts = std::collections::HashMap::<String, usize>::new();
        for entity in self
            .entities
            .iter()
            .filter(|entity| entity.entity_type == "society")
        {
            *society_name_counts.entry(slug(&entity.name)).or_default() += 1;
        }
        accepted_entities.extend(
            society_name_counts
                .into_iter()
                .filter(|(_, count)| *count == 1)
                .map(|(name_slug, _)| format!("society:{name_slug}")),
        );

        let mut support_fact_records = Vec::new();
        let mut support_fact_keys = HashSet::<(String, String)>::new();

        for fact in support_facts
            .iter()
            .filter(|fact| accepted_entities.contains(&fact.entity_id))
        {
            let fact_value: FactValue = serde_json::from_str(&fact.value_json)?;
            let record = KgViewFactRecord {
                entity_id: fact.entity_id.clone(),
                fact_key: fact.fact_key.clone(),
                fact_version: 1,
                value_type: fact.value_type.clone(),
                value_text: fact_value_text(&fact_value),
                value_json: fact.value_json.clone(),
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
                source_url: fact.source_url.clone(),
                model: fact.model.clone(),
                skill_id: fact.skill_id.clone(),
                triggered_by: fact.triggered_by.clone(),
                learned_at: fact.learned_at,
            };
            support_fact_keys.insert((record.entity_id.clone(), record.fact_key.clone()));
            support_fact_records.push(record);
        }

        let mut support_annotation_records = Vec::new();
        for annotation in support_annotations
            .iter()
            .filter(|annotation| accepted_entities.contains(&annotation.entity_id))
            .filter(|annotation| {
                support_fact_keys
                    .contains(&(annotation.entity_id.clone(), annotation.fact_key.clone()))
            })
        {
            support_annotation_records.push(KgViewFactAnnotationRecord {
                entity_id: annotation.entity_id.clone(),
                fact_key: annotation.fact_key.clone(),
                display_template: annotation.display_template.clone(),
                answers_preferences_json: annotation.answers_preferences_json.clone(),
                scoring_direction: annotation.scoring_direction.clone(),
                scoring_weight: annotation.scoring_weight,
                scoring_thresholds_json: annotation.scoring_thresholds_json.clone(),
            });
        }

        self.facts.extend(support_fact_records);
        self.facts = dedupe_facts(std::mem::take(&mut self.facts));
        self.fact_annotations.extend(support_annotation_records);
        self.fact_annotations = dedupe_fact_annotations(std::mem::take(&mut self.fact_annotations));
        self.update_entity_fact_counts();
        self.content_hash = content_hash(
            &self.entities,
            &self.facts,
            &self.fact_annotations,
            &self.edges,
        )?;
        Ok(())
    }

    fn update_entity_fact_counts(&mut self) {
        let mut fact_counts = HashMap::<&str, u32>::new();
        for fact in &self.facts {
            *fact_counts.entry(fact.entity_id.as_str()).or_default() += 1;
        }
        for entity in &mut self.entities {
            entity.fact_count = fact_counts
                .get(entity.entity_id.as_str())
                .copied()
                .unwrap_or(0);
        }
    }
}

pub async fn load_kg_view_records(
    lake: &LakeStore,
    manifest: &KgViewManifest,
) -> Result<KgViewRecords, KgSocietyViewMaterializeError> {
    let entities = read_entities_parquet(
        &lake
            .get_bytes(&LakeKey::new(manifest.entity_parquet_key.clone()).map_err(LakeError::Key)?)
            .await?,
    )?;
    let facts = read_facts_parquet(
        &lake
            .get_bytes(&LakeKey::new(manifest.fact_parquet_key.clone()).map_err(LakeError::Key)?)
            .await?,
    )?;
    let fact_annotations = read_fact_annotations_parquet(
        &lake
            .get_bytes(
                &LakeKey::new(manifest.fact_annotation_parquet_key.clone())
                    .map_err(LakeError::Key)?,
            )
            .await?,
    )?;
    let edges = read_edges_parquet(
        &lake
            .get_bytes(&LakeKey::new(manifest.edge_parquet_key.clone()).map_err(LakeError::Key)?)
            .await?,
    )?;
    let content_hash = content_hash(&entities, &facts, &fact_annotations, &edges)?;
    Ok(KgViewRecords {
        entities,
        facts,
        fact_annotations,
        edges,
        content_hash,
    })
}

fn merge_synthesized_entities(
    entities: &mut Vec<KgViewEntityRecord>,
    synthesized: Vec<KgViewEntityRecord>,
) {
    if synthesized.is_empty() {
        return;
    }
    let mut positions = entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (entity.entity_id.clone(), index))
        .collect::<HashMap<_, _>>();
    for entity in synthesized {
        if let Some(index) = positions.get(&entity.entity_id).copied() {
            let existing = &mut entities[index];
            existing.entity_type.clone_from(&entity.entity_type);
            existing.name.clone_from(&entity.name);
            existing.root_source.clone_from(&entity.root_source);
            existing.created_at = existing.created_at.min(entity.created_at);
            existing.updated_at = existing.updated_at.max(entity.updated_at);
        } else {
            positions.insert(entity.entity_id.clone(), entities.len());
            entities.push(entity);
        }
    }
    entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
}

fn support_place_entities(
    support_facts: &[SkillFactRecord],
) -> Result<Vec<KgViewEntityRecord>, KgSocietyViewMaterializeError> {
    let coordinate_entities = super::compaction::resolve_coordinate_fact_records(support_facts)?
        .into_iter()
        .map(|fact| fact.entity_id)
        .collect::<HashSet<_>>();
    let mut by_entity = BTreeMap::<String, SupportPlaceEntity>::new();
    for fact in support_facts
        .iter()
        .filter(|fact| fact.entity_id.starts_with("place:"))
    {
        let value = serde_json::from_str::<FactValue>(&fact.value_json)?;
        let entry = by_entity
            .entry(fact.entity_id.clone())
            .or_insert_with(|| SupportPlaceEntity {
                entity_id: fact.entity_id.clone(),
                name: None,
                has_coordinates: coordinate_entities.contains(&fact.entity_id),
                root_source: Some(fact.source_type.to_ascii_lowercase()),
                created_at: fact.learned_at,
                updated_at: fact.learned_at,
            });
        entry.created_at = entry.created_at.min(fact.learned_at);
        entry.updated_at = entry.updated_at.max(fact.learned_at);
        if entry.root_source.is_none() && !fact.source_type.trim().is_empty() {
            entry.root_source = Some(fact.source_type.to_ascii_lowercase());
        }
        match (fact.fact_key.as_str(), value) {
            ("place.name", FactValue::Text(name)) if !name.trim().is_empty() => {
                entry.name = Some(name.trim().to_string());
            }
            _ => {}
        }
    }

    Ok(by_entity
        .into_values()
        .filter_map(|entity| entity.into_record())
        .collect())
}

struct SupportPlaceEntity {
    entity_id: String,
    name: Option<String>,
    has_coordinates: bool,
    root_source: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SupportPlaceEntity {
    fn into_record(self) -> Option<KgViewEntityRecord> {
        let name = self.name?;
        if !self.has_coordinates {
            return None;
        }
        Some(KgViewEntityRecord {
            entity_id: self.entity_id,
            entity_type: "place".to_string(),
            name,
            root_source: self.root_source,
            fact_count: 0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut pending_dash = false;
    for character in value.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !output.is_empty() {
                output.push('-');
            }
            output.push(character);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    output
}

fn canonical_society_alias_map(
    canonical_entities: &[KgViewEntityRecord],
) -> HashMap<String, String> {
    canonical_entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .filter_map(|entity| {
            let alias_entity_id = format!("society:{}", slug(&entity.name));
            (alias_entity_id != entity.entity_id)
                .then_some((alias_entity_id, entity.entity_id.clone()))
        })
        .collect()
}

fn rewrite_entity_id(entity_id: &mut String, aliases: &HashMap<String, String>) {
    if let Some(canonical_id) = aliases.get(entity_id) {
        entity_id.clone_from(canonical_id);
    }
}

fn rewrite_skill_facts(
    facts: &[SkillFactRecord],
    aliases: &HashMap<String, String>,
) -> Vec<SkillFactRecord> {
    facts
        .iter()
        .cloned()
        .map(|mut fact| {
            rewrite_entity_id(&mut fact.entity_id, aliases);
            fact
        })
        .collect()
}

fn rewrite_skill_annotations(
    annotations: &[SkillFactAnnotationRecord],
    aliases: &HashMap<String, String>,
) -> Vec<SkillFactAnnotationRecord> {
    annotations
        .iter()
        .cloned()
        .map(|mut annotation| {
            rewrite_entity_id(&mut annotation.entity_id, aliases);
            annotation
        })
        .collect()
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
        self.materialize_and_promote_with_skill_facts(
            graph,
            view_version,
            source_watermarks,
            parent_materializations,
            &[],
            &[],
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_and_promote_with_skill_facts(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        let materialization = self
            .materialize_for_run_with_skill_facts(
                graph,
                view_version,
                source_watermarks,
                parent_materializations,
                MaterializationId::new(),
                AssetPartition::global(),
                support_facts,
                support_annotations,
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
            &[],
            &[],
            &[],
            &[],
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_for_run_with_skill_facts(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        self.materialize_for_run_with_asset_rows(
            graph,
            view_version,
            source_watermarks,
            parent_materializations,
            run_id,
            partition,
            &[],
            &[],
            support_facts,
            support_annotations,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_for_run_with_asset_rows(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
        canonical_entities: &[KgViewEntityRecord],
        canonical_edges: &[KgViewEdgeRecord],
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        self.materialize_for_run_inner(
            graph,
            view_version,
            source_watermarks,
            parent_materializations,
            run_id,
            partition,
            canonical_entities,
            canonical_edges,
            support_facts,
            support_annotations,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn materialize_for_run_inner(
        &self,
        graph: &KnowledgeGraph,
        view_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
        canonical_entities: &[KgViewEntityRecord],
        canonical_edges: &[KgViewEdgeRecord],
        support_facts: &[SkillFactRecord],
        support_annotations: &[SkillFactAnnotationRecord],
    ) -> Result<KgSocietyViewMaterialization, KgSocietyViewMaterializeError> {
        let view_version = view_version.into();
        let records = KgViewRecords::from_graph_with_asset_rows(
            graph,
            canonical_entities,
            canonical_edges,
            support_facts,
            support_annotations,
        )?;

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
            format_version: KG_SOCIETY_VIEW_FORMAT_VERSION,
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
    let typed_values = facts
        .iter()
        .map(|fact| {
            let value = serde_json::from_str(&fact.value_json)?;
            validate_fact_value_type(&fact.value_type, &value)?;
            Ok(TypedFactValue::from_fact_value(&value))
        })
        .collect::<Result<Vec<_>, KgSocietyViewMaterializeError>>()?;

    let mut fields = vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("fact_version", DataType::UInt32, false),
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
    ]);
    let schema = Arc::new(Schema::new(fields));

    let mut columns = vec![
        string_array(facts.iter().map(|fact| fact.entity_id.clone())),
        string_array(facts.iter().map(|fact| fact.fact_key.clone())),
        Arc::new(UInt32Array::from(
            facts
                .iter()
                .map(|fact| fact.fact_version)
                .collect::<Vec<_>>(),
        )) as ArrayRef,
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
    ]);

    let batch = RecordBatch::try_new(schema.clone(), columns)
        .map_err(KgSocietyViewMaterializeError::Arrow)?;

    write_batch(batch)
}

fn write_fact_annotations_parquet(
    annotations: &[KgViewFactAnnotationRecord],
) -> Result<Vec<u8>, KgSocietyViewMaterializeError> {
    let answers_preferences = annotations
        .iter()
        .map(|record| parse_string_vec(&record.answers_preferences_json).map(Some))
        .collect::<Result<Vec<_>, KgSocietyViewMaterializeError>>()?;
    let scoring_thresholds = annotations
        .iter()
        .map(|record| parse_f64_vec(&record.scoring_thresholds_json).map(Some))
        .collect::<Result<Vec<_>, KgSocietyViewMaterializeError>>()?;

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

fn read_entities_parquet(
    bytes: &[u8],
) -> Result<Vec<KgViewEntityRecord>, KgSocietyViewMaterializeError> {
    let mut records = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let entity_type = string_column(&batch, "entity_type")?;
        let name = string_column(&batch, "name")?;
        let root_source = string_column(&batch, "root_source")?;
        let fact_count = uint32_column(&batch, "fact_count")?;
        let created_at = string_column(&batch, "created_at")?;
        let updated_at = string_column(&batch, "updated_at")?;
        for row in 0..batch.num_rows() {
            records.push(KgViewEntityRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                entity_type: required_string(entity_type, row, "entity_type")?,
                name: required_string(name, row, "name")?,
                root_source: optional_string(root_source, row),
                fact_count: required_u32(fact_count, row, "fact_count")?,
                created_at: parse_timestamp(&required_string(created_at, row, "created_at")?)?,
                updated_at: parse_timestamp(&required_string(updated_at, row, "updated_at")?)?,
            });
        }
    }
    Ok(records)
}

fn read_facts_parquet(
    bytes: &[u8],
) -> Result<Vec<KgViewFactRecord>, KgSocietyViewMaterializeError> {
    let mut records = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let fact_version = uint32_column(&batch, "fact_version")?;
        let value_type = string_column(&batch, "value_type")?;
        let confidence = float32_column(&batch, "confidence")?;
        let source_type = string_column(&batch, "source_type")?;
        let source_url = string_column(&batch, "source_url")?;
        let model = string_column(&batch, "model")?;
        let skill_id = string_column(&batch, "skill_id")?;
        let triggered_by = string_column(&batch, "triggered_by")?;
        let learned_at = string_column(&batch, "learned_at")?;
        for row in 0..batch.num_rows() {
            let value_type = required_string(value_type, row, "value_type")?;
            let typed = typed_value_from_batch(&batch, row).ok_or_else(|| {
                KgSocietyViewMaterializeError::Read(format!(
                    "KG fact row {row} has no typed value columns"
                ))
            })?;
            let value = typed.to_fact_value(&value_type).ok_or_else(|| {
                KgSocietyViewMaterializeError::Read(format!(
                    "KG fact row {row} typed value does not match {value_type}"
                ))
            })?;
            records.push(KgViewFactRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                fact_version: required_u32(fact_version, row, "fact_version")?,
                value_type,
                value_text: typed.value_text,
                value_json: serde_json::to_string(&value)?,
                confidence: required_f32(confidence, row, "confidence")?,
                source_type: required_string(source_type, row, "source_type")?,
                source_url: optional_string(source_url, row),
                model: optional_string(model, row),
                skill_id: optional_string(skill_id, row),
                triggered_by: optional_string(triggered_by, row),
                learned_at: parse_timestamp(&required_string(learned_at, row, "learned_at")?)?,
            });
        }
    }
    Ok(records)
}

fn read_fact_annotations_parquet(
    bytes: &[u8],
) -> Result<Vec<KgViewFactAnnotationRecord>, KgSocietyViewMaterializeError> {
    let mut records = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let display_template = string_column(&batch, "display_template")?;
        let scoring_direction = string_column(&batch, "scoring_direction")?;
        let scoring_weight = float32_column(&batch, "scoring_weight")?;
        for row in 0..batch.num_rows() {
            let answers_preferences =
                required_string_list(&batch, ANSWERS_PREFERENCES_COLUMN, row)?;
            let scoring_thresholds = required_f64_list(&batch, SCORING_THRESHOLDS_COLUMN, row)?;
            records.push(KgViewFactAnnotationRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                display_template: optional_string(display_template, row),
                answers_preferences_json: serde_json::to_string(&answers_preferences)?,
                scoring_direction: optional_string(scoring_direction, row),
                scoring_weight: optional_f32(scoring_weight, row),
                scoring_thresholds_json: serde_json::to_string(&scoring_thresholds)?,
            });
        }
    }
    Ok(records)
}

fn read_edges_parquet(
    bytes: &[u8],
) -> Result<Vec<KgViewEdgeRecord>, KgSocietyViewMaterializeError> {
    let mut records = Vec::new();
    for batch in parquet_batches(bytes)? {
        let from_entity_id = string_column(&batch, "from_entity_id")?;
        let to_entity_id = string_column(&batch, "to_entity_id")?;
        let relation = string_column(&batch, "relation")?;
        let weight = float32_column(&batch, "weight")?;
        let metadata_json = string_column(&batch, "metadata_json")?;
        let source_type = string_column(&batch, "source_type")?;
        let source_url = string_column(&batch, "source_url")?;
        let model = string_column(&batch, "model")?;
        let skill_id = string_column(&batch, "skill_id")?;
        let triggered_by = string_column(&batch, "triggered_by")?;
        for row in 0..batch.num_rows() {
            records.push(KgViewEdgeRecord {
                from_entity_id: required_string(from_entity_id, row, "from_entity_id")?,
                to_entity_id: required_string(to_entity_id, row, "to_entity_id")?,
                relation: required_string(relation, row, "relation")?,
                weight: required_f32(weight, row, "weight")?,
                metadata_json: required_string(metadata_json, row, "metadata_json")?,
                source_type: required_string(source_type, row, "source_type")?,
                source_url: optional_string(source_url, row),
                model: optional_string(model, row),
                skill_id: optional_string(skill_id, row),
                triggered_by: optional_string(triggered_by, row),
            });
        }
    }
    Ok(records)
}

fn parquet_batches(bytes: &[u8]) -> Result<Vec<RecordBatch>, KgSocietyViewMaterializeError> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))
        .map_err(KgSocietyViewMaterializeError::Parquet)?
        .build()
        .map_err(KgSocietyViewMaterializeError::Parquet)?;
    reader
        .map(|batch| batch.map_err(KgSocietyViewMaterializeError::Arrow))
        .collect()
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, KgSocietyViewMaterializeError> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|error| KgSocietyViewMaterializeError::Read(error.to_string()))?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| KgSocietyViewMaterializeError::Read(format!("{name} is not Utf8")))
}

fn float32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float32Array, KgSocietyViewMaterializeError> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|error| KgSocietyViewMaterializeError::Read(error.to_string()))?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| KgSocietyViewMaterializeError::Read(format!("{name} is not Float32")))
}

fn uint32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a UInt32Array, KgSocietyViewMaterializeError> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|error| KgSocietyViewMaterializeError::Read(error.to_string()))?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or_else(|| KgSocietyViewMaterializeError::Read(format!("{name} is not UInt32")))
}

fn required_string(
    array: &StringArray,
    row: usize,
    column: &str,
) -> Result<String, KgSocietyViewMaterializeError> {
    if array.is_null(row) {
        return Err(KgSocietyViewMaterializeError::Read(format!(
            "{column} is null at row {row}"
        )));
    }
    Ok(array.value(row).to_string())
}

fn optional_string(array: &StringArray, row: usize) -> Option<String> {
    (!array.is_null(row)).then(|| array.value(row).to_string())
}

fn required_f32(
    array: &Float32Array,
    row: usize,
    column: &str,
) -> Result<f32, KgSocietyViewMaterializeError> {
    if array.is_null(row) {
        return Err(KgSocietyViewMaterializeError::Read(format!(
            "{column} is null at row {row}"
        )));
    }
    Ok(array.value(row))
}

fn optional_f32(array: &Float32Array, row: usize) -> Option<f32> {
    (!array.is_null(row)).then(|| array.value(row))
}

fn required_u32(
    array: &UInt32Array,
    row: usize,
    column: &str,
) -> Result<u32, KgSocietyViewMaterializeError> {
    if array.is_null(row) {
        return Err(KgSocietyViewMaterializeError::Read(format!(
            "{column} is null at row {row}"
        )));
    }
    Ok(array.value(row))
}

fn required_string_list(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<Vec<String>, KgSocietyViewMaterializeError> {
    match optional_string_list_column_value(batch, column, row)
        .map_err(KgSocietyViewMaterializeError::Read)?
    {
        OptionalListColumn::Values(values) => Ok(values),
        OptionalListColumn::Missing | OptionalListColumn::Null => Err(
            KgSocietyViewMaterializeError::Read(format!("{column} is missing at row {row}")),
        ),
    }
}

fn required_f64_list(
    batch: &RecordBatch,
    column: &str,
    row: usize,
) -> Result<Vec<f64>, KgSocietyViewMaterializeError> {
    match optional_f64_list_column_value(batch, column, row)
        .map_err(KgSocietyViewMaterializeError::Read)?
    {
        OptionalListColumn::Values(values) => Ok(values),
        OptionalListColumn::Missing | OptionalListColumn::Null => Err(
            KgSocietyViewMaterializeError::Read(format!("{column} is missing at row {row}")),
        ),
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, KgSocietyViewMaterializeError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| KgSocietyViewMaterializeError::Read(error.to_string()))
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

fn parse_string_vec(value: &str) -> Result<Vec<String>, KgSocietyViewMaterializeError> {
    Ok(serde_json::from_str(value)?)
}

fn parse_f64_vec(value: &str) -> Result<Vec<f64>, KgSocietyViewMaterializeError> {
    Ok(serde_json::from_str(value)?)
}

fn validate_fact_value_type(
    value_type: &str,
    value: &FactValue,
) -> Result<(), KgSocietyViewMaterializeError> {
    if TypedFactValue::value_type_matches(value_type, value) {
        return Ok(());
    }
    Err(KgSocietyViewMaterializeError::InvalidFactValueType {
        value_type: value_type.to_string(),
        actual_type: TypedFactValue::value_type_for(value).to_string(),
    })
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

fn dedupe_facts(facts: Vec<KgViewFactRecord>) -> Vec<KgViewFactRecord> {
    let multi_value_fact_keys = multi_value_fact_keys();
    let mut by_key = BTreeMap::<FactDedupeKey, KgViewFactRecord>::new();
    for fact in facts {
        let key = FactDedupeKey {
            entity_id: fact.entity_id.clone(),
            fact_key: fact.fact_key.clone(),
            repeat_identity: multi_value_fact_keys
                .contains(fact.fact_key.as_str())
                .then(|| repeat_fact_identity(&fact)),
        };
        match by_key.get(&key) {
            Some(existing) if better_fact(existing, &fact) => {
                by_key.insert(key, fact);
            }
            None => {
                by_key.insert(key, fact);
            }
            Some(_) => {}
        }
    }

    let mut facts = by_key.into_values().collect::<Vec<_>>();
    sort_facts(&mut facts);
    facts
}

fn multi_value_fact_keys() -> HashSet<String> {
    load_fact_registry()
        .map(|registry| registry.runtime.multi_value_fact_keys.into_iter().collect())
        .unwrap_or_default()
}

fn repeat_fact_identity(fact: &KgViewFactRecord) -> RepeatFactIdentity {
    RepeatFactIdentity {
        source_type: fact.source_type.clone(),
        source_url_or_value: fact
            .source_url
            .clone()
            .unwrap_or_else(|| fact.value_json.clone()),
        skill_id: fact.skill_id.clone(),
    }
}

fn sort_facts(facts: &mut [KgViewFactRecord]) {
    facts.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
            .then(left.fact_version.cmp(&right.fact_version))
            .then(left.source_type.cmp(&right.source_type))
            .then(left.source_url.cmp(&right.source_url))
            .then(left.skill_id.cmp(&right.skill_id))
            .then(left.learned_at.cmp(&right.learned_at))
    });
}

fn better_fact(existing: &KgViewFactRecord, candidate: &KgViewFactRecord) -> bool {
    if existing.entity_id == candidate.entity_id && existing.fact_key == candidate.fact_key {
        if let Ok(policies) = load_resolution_policies() {
            if better_source_type_for_fact(
                Some(&candidate.fact_key),
                &candidate.source_type,
                &existing.source_type,
                candidate.confidence,
                existing.confidence,
                &policies,
            ) {
                return true;
            }
            if better_source_type_for_fact(
                Some(&existing.fact_key),
                &existing.source_type,
                &candidate.source_type,
                existing.confidence,
                candidate.confidence,
                &policies,
            ) {
                return false;
            }
        }
    }

    candidate.confidence > existing.confidence
        || ((candidate.confidence - existing.confidence).abs() < f32::EPSILON
            && candidate.learned_at > existing.learned_at)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FactDedupeKey {
    entity_id: String,
    fact_key: String,
    repeat_identity: Option<RepeatFactIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RepeatFactIdentity {
    source_type: String,
    source_url_or_value: String,
    skill_id: Option<String>,
}

fn dedupe_fact_annotations(
    annotations: Vec<KgViewFactAnnotationRecord>,
) -> Vec<KgViewFactAnnotationRecord> {
    let mut by_key = BTreeMap::<(String, String), KgViewFactAnnotationRecord>::new();
    for annotation in annotations {
        let key = (annotation.entity_id.clone(), annotation.fact_key.clone());
        match by_key.get(&key) {
            Some(existing) if annotation_quality(&annotation) > annotation_quality(existing) => {
                by_key.insert(key, annotation);
            }
            None => {
                by_key.insert(key, annotation);
            }
            Some(_) => {}
        }
    }

    let mut annotations = by_key.into_values().collect::<Vec<_>>();
    sort_fact_annotations(&mut annotations);
    annotations
}

fn sort_fact_annotations(annotations: &mut [KgViewFactAnnotationRecord]) {
    annotations.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
    });
}

fn annotation_quality(annotation: &KgViewFactAnnotationRecord) -> i32 {
    let mut quality = 0;
    if annotation.display_template.is_some() {
        quality += 1;
    }
    if annotation.scoring_direction.is_some() {
        quality += 1;
    }
    if annotation.scoring_weight.is_some() {
        quality += 1;
    }
    quality + annotation.answers_preferences_json.len() as i32
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
    InvalidFactValueType {
        value_type: String,
        actual_type: String,
    },
    Json(serde_json::Error),
    Lake(LakeError),
    Parquet(parquet::errors::ParquetError),
    Read(String),
    Coordinate(super::CurrentProjectFactsError),
}

impl fmt::Display for KgSocietyViewMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "KG view Arrow record batch error: {err}"),
            Self::InvalidFactValueType {
                value_type,
                actual_type,
            } => write!(
                f,
                "KG view fact value_type {value_type} does not match fact value type {actual_type}"
            ),
            Self::Json(err) => write!(f, "KG view JSON error: {err}"),
            Self::Lake(err) => write!(f, "KG view lake error: {err}"),
            Self::Parquet(err) => write!(f, "KG view Parquet error: {err}"),
            Self::Read(err) => write!(f, "KG view read error: {err}"),
            Self::Coordinate(err) => write!(f, "KG view coordinate resolution error: {err}"),
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

impl From<super::CurrentProjectFactsError> for KgSocietyViewMaterializeError {
    fn from(err: super::CurrentProjectFactsError) -> Self {
        Self::Coordinate(err)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::knowledge::fact::{FactSource, SourceType};
    use crate::knowledge::node::{Node, NodeType, RootSource};
    use crate::knowledge::{FactValue, SourcedFact};

    #[test]
    fn canonical_rera_entity_suppresses_shadow_alias_graph_node() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 15, 7, 0, 0).unwrap();
        let mut graph = KnowledgeGraph::new();
        let mut alias = Node::new(
            "society:prestige-lavender-fields",
            NodeType::Society,
            "Prestige Lavender Fields",
        );
        alias.root_source = Some(RootSource::Legacy);
        alias.add_fact(SourcedFact {
            key: "google_rating".to_string(),
            value: FactValue::Numeric(3.2),
            confidence: 0.5,
            source: FactSource {
                source_type: SourceType::Google,
                url: None,
                model: None,
                skill_id: Some("google_places".to_string()),
                triggered_by: None,
            },
            learned_at,
            version: 1,
            display_template: Some("Google rating: {value}".to_string()),
            answers_preferences: vec!["google reviews".to_string()],
            scoring_hint: None,
        });
        graph.add_node(alias);

        let canonical_entities = vec![KgViewEntityRecord {
            entity_id: "society:rera-a19f2cf2456fc549".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Lavender Fields".to_string(),
            root_source: Some("rera".to_string()),
            fact_count: 0,
            created_at: learned_at,
            updated_at: learned_at,
        }];
        let support_facts = vec![
            skill_fact(
                "society:rera-a19f2cf2456fc549",
                "google_rating",
                FactValue::Numeric(3.9),
                learned_at,
            ),
            skill_fact(
                "society:prestige-lavender-fields",
                "google_rating",
                FactValue::Numeric(3.9),
                learned_at,
            ),
        ];
        let support_annotations = vec![
            skill_annotation("society:rera-a19f2cf2456fc549", "google_rating"),
            skill_annotation("society:prestige-lavender-fields", "google_rating"),
        ];

        let records = KgViewRecords::from_graph_with_asset_rows(
            &graph,
            &canonical_entities,
            &[],
            &support_facts,
            &support_annotations,
        )
        .unwrap();

        assert!(records
            .entities
            .iter()
            .any(|entity| entity.entity_id == "society:rera-a19f2cf2456fc549"));
        assert!(!records
            .entities
            .iter()
            .any(|entity| entity.entity_id == "society:prestige-lavender-fields"));
        assert!(records.facts.iter().any(|fact| {
            fact.entity_id == "society:rera-a19f2cf2456fc549"
                && fact.fact_key == "google_rating"
                && fact.value_text.as_deref() == Some("3.9")
        }));
        let alias_facts = records
            .facts
            .iter()
            .filter(|fact| fact.entity_id == "society:prestige-lavender-fields")
            .collect::<Vec<_>>();
        assert!(alias_facts.is_empty());
        assert_eq!(
            records
                .facts
                .iter()
                .filter(|fact| {
                    fact.entity_id == "society:rera-a19f2cf2456fc549"
                        && fact.fact_key == "google_rating"
                })
                .count(),
            1
        );
        assert!(records
            .fact_annotations
            .iter()
            .all(|annotation| annotation.entity_id != "society:prestige-lavender-fields"));
    }

    #[test]
    fn merge_preserves_repeatable_nearby_facts_by_source() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 22, 7, 0, 0).unwrap();
        let mut graph = KnowledgeGraph::new();
        graph.add_node(Node::new(
            "society:sumadhura-eden-garden",
            NodeType::Society,
            "Sumadhura Eden Garden",
        ));

        let mut aster = skill_fact(
            "society:sumadhura-eden-garden",
            "nearby_hospitals",
            FactValue::Text("Aster Hospital Whitefield Bangalore (4.7 km, 4.7 rating)".to_string()),
            learned_at,
        );
        aster.source_url = Some("https://www.google.com/maps/place/aster".to_string());
        aster.skill_id = Some("fetch_google_nearby_places".to_string());

        let mut manipal = skill_fact(
            "society:sumadhura-eden-garden",
            "nearby_hospitals",
            FactValue::Text("Manipal Hospital Whitefield (5.1 km, 4.7 rating)".to_string()),
            learned_at,
        );
        manipal.source_url = Some("https://www.google.com/maps/place/manipal".to_string());
        manipal.skill_id = Some("fetch_google_nearby_places".to_string());

        let records = KgViewRecords::from_graph_with_skill_facts(&graph, &[aster, manipal], &[])
            .expect("repeatable nearby facts should merge");
        let hospital_values = records
            .facts
            .iter()
            .filter(|fact| {
                fact.entity_id == "society:sumadhura-eden-garden"
                    && fact.fact_key == "nearby_hospitals"
            })
            .filter_map(|fact| fact.value_text.as_deref())
            .collect::<Vec<_>>();

        assert_eq!(hospital_values.len(), 2);
        assert!(hospital_values
            .iter()
            .any(|value| value.contains("Aster Hospital")));
        assert!(hospital_values
            .iter()
            .any(|value| value.contains("Manipal Hospital")));
    }

    fn skill_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        learned_at: DateTime<Utc>,
    ) -> SkillFactRecord {
        let value_type = match &value {
            FactValue::Numeric(_) => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
            FactValue::Score { .. } => "score",
        }
        .to_string();
        SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type,
            value_json: serde_json::to_string(&value).unwrap(),
            confidence: 0.85,
            source_type: "Google".to_string(),
            source_url: None,
            model: None,
            skill_id: Some("fetch_google_review_links".to_string()),
            triggered_by: None,
            learned_at,
            run_id: "test-run".to_string(),
            input_hash: format!("{entity_id}:{fact_key}"),
        }
    }

    fn skill_annotation(entity_id: &str, fact_key: &str) -> SkillFactAnnotationRecord {
        SkillFactAnnotationRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: Some("Google rating: {value}".to_string()),
            answers_preferences_json: serde_json::to_string(&["google reviews"]).unwrap(),
            scoring_direction: Some("HigherIsBetter".to_string()),
            scoring_weight: Some(1.0),
            scoring_thresholds_json: serde_json::to_string(&[4.2, 3.8]).unwrap(),
        }
    }
}
