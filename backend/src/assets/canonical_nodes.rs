use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use chrono::Utc;
use serde::Deserialize;

use crate::lake::{LakeError, LakeStore};

use super::kg_view::{KgViewEdgeRecord, KgViewEntityRecord};
use super::rera::{read_edges, read_entities, write_edges, write_entities, ReraAssetError};
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord,
};

pub const CANONICAL_PROPERTY_NODES_ASSET_ID: &str = "canonical_property_nodes";
pub const CANONICAL_AREA_NODES_ASSET_ID: &str = "canonical_area_nodes";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalNodeRows {
    pub entities: Vec<KgViewEntityRecord>,
    pub edges: Vec<KgViewEdgeRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct SeedPropertyRecord {
    id: String,
    title: String,
    #[serde(default)]
    society_id: String,
    #[serde(default)]
    area_id: String,
    #[serde(default)]
    area: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedAreaRecord {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedSocietyRecord {
    id: String,
    name: String,
    #[serde(default)]
    area: String,
}

#[derive(Clone)]
pub struct CanonicalPropertyNodesMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
    project_root: PathBuf,
}

#[derive(Clone)]
pub struct CanonicalAreaNodesMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
    project_root: PathBuf,
}

impl CanonicalPropertyNodesMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            project_root: openestates_project_root(),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        version: &str,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
        parent_materializations: Vec<MaterializationId>,
    ) -> Result<MaterializationRecord, CanonicalNodesError> {
        let rows = property_rows_from_seed(&self.project_root)?;
        write_canonical_node_materialization(
            &self.lake,
            &self.materializations,
            CANONICAL_PROPERTY_NODES_ASSET_ID,
            version,
            dag_run_id,
            partition,
            parent_materializations,
            rows,
        )
        .await
    }
}

impl CanonicalAreaNodesMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            project_root: openestates_project_root(),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        version: &str,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
        parent_materializations: Vec<MaterializationId>,
    ) -> Result<MaterializationRecord, CanonicalNodesError> {
        let rows = area_rows_from_seed(&self.project_root)?;
        write_canonical_node_materialization(
            &self.lake,
            &self.materializations,
            CANONICAL_AREA_NODES_ASSET_ID,
            version,
            dag_run_id,
            partition,
            parent_materializations,
            rows,
        )
        .await
    }
}

async fn write_canonical_node_materialization(
    lake: &LakeStore,
    materializations: &AssetMaterializationStore,
    asset: &str,
    version: &str,
    dag_run_id: MaterializationId,
    partition: AssetPartition,
    parent_materializations: Vec<MaterializationId>,
    rows: CanonicalNodeRows,
) -> Result<MaterializationRecord, CanonicalNodesError> {
    let entity_key =
        AssetPathBuilder::gold_asset_key(asset, version, "entities/part-00000.parquet");
    let edge_key = AssetPathBuilder::gold_asset_key(asset, version, "edges/part-00000.parquet");
    let artifacts = vec![
        ArtifactRef::parquet(
            lake.put_bytes(&entity_key, write_entities(&rows.entities)?)
                .await?,
        ),
        ArtifactRef::parquet(
            lake.put_bytes(&edge_key, write_edges(&rows.edges)?)
                .await?,
        ),
    ];
    let record = MaterializationRecord::succeeded(
        asset_id(asset),
        AssetStage::Gold,
        partition,
        version.to_string(),
        artifacts,
    )
    .with_run_id(dag_run_id)
    .with_parent_materializations(parent_materializations)
    .with_row_count((rows.entities.len() + rows.edges.len()) as u64);
    materializations.write_materialization(&record).await?;
    Ok(record)
}

pub async fn read_canonical_node_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<CanonicalNodeRows, CanonicalNodesError> {
    let entity_bytes = read_parquet_artifact(lake, record, "entities/part-00000.parquet").await?;
    let edge_bytes = read_parquet_artifact(lake, record, "edges/part-00000.parquet").await?;
    Ok(CanonicalNodeRows {
        entities: read_entities(entity_bytes)?,
        edges: read_edges(edge_bytes)?,
    })
}

pub fn property_rows_from_seed(project_root: &PathBuf) -> Result<CanonicalNodeRows, CanonicalNodesError> {
    let path = project_root.join("data/seed/properties.json");
    let bytes = std::fs::read(&path).map_err(CanonicalNodesError::Io)?;
    let properties: Vec<SeedPropertyRecord> =
        serde_json::from_slice(&bytes).map_err(CanonicalNodesError::Json)?;
    let now = Utc::now();
    let mut entities = BTreeMap::<String, KgViewEntityRecord>::new();
    let mut edges = BTreeMap::<(String, String, String), KgViewEdgeRecord>::new();

    for property in properties {
        let property_id = format!("property:{}", property.id.trim());
        insert_entity(
            &mut entities,
            property_id.clone(),
            "property",
            &property.title,
            "legacy_seed",
            now,
        );
        if let Some(society_id) = society_entity_id(property.society_id.trim()) {
            insert_edge(
                &mut edges,
                &property_id,
                &society_id,
                "in_society",
                "LegacySeed",
                0.85,
            );
        }
    }

    Ok(CanonicalNodeRows {
        entities: entities.into_values().collect(),
        edges: edges.into_values().collect(),
    })
}

pub fn area_rows_from_seed(project_root: &PathBuf) -> Result<CanonicalNodeRows, CanonicalNodesError> {
    let area_path = project_root.join("data/seed/area_profiles.json");
    let society_path = project_root.join("data/seed/societies.json");
    let areas: Vec<SeedAreaRecord> = serde_json::from_slice(
        &std::fs::read(&area_path).map_err(CanonicalNodesError::Io)?,
    )
    .map_err(CanonicalNodesError::Json)?;
    let societies: Vec<SeedSocietyRecord> = serde_json::from_slice(
        &std::fs::read(&society_path).map_err(CanonicalNodesError::Io)?,
    )
    .map_err(CanonicalNodesError::Json)?;
    let now = Utc::now();
    let mut entities = BTreeMap::<String, KgViewEntityRecord>::new();
    let mut edges = BTreeMap::<(String, String, String), KgViewEdgeRecord>::new();

    for area in areas {
        let area_id = area_entity_id(&area.id);
        insert_entity(
            &mut entities,
            area_id.clone(),
            "area",
            &area.name,
            "legacy_seed",
            now,
        );
    }

    for society in societies {
        let society_id = society_entity_id(&society.id).unwrap_or_else(|| {
            format!("society:{}", slug(&society.name))
        });
        let area_id = area_entity_id_from_name(&society.area);
        insert_edge(
            &mut edges,
            &society_id,
            &area_id,
            "in_area",
            "LegacySeed",
            0.8,
        );
    }

    Ok(CanonicalNodeRows {
        entities: entities.into_values().collect(),
        edges: edges.into_values().collect(),
    })
}

fn insert_entity(
    entities: &mut BTreeMap<String, KgViewEntityRecord>,
    entity_id: String,
    entity_type: &str,
    name: &str,
    root_source: &str,
    timestamp: chrono::DateTime<Utc>,
) {
    entities
        .entry(entity_id.clone())
        .or_insert_with(|| KgViewEntityRecord {
            entity_id,
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: Some(root_source.to_string()),
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
    source_type: &str,
    confidence: f32,
) {
    let key = (from.to_string(), to.to_string(), relation.to_string());
    edges.entry(key).or_insert_with(|| KgViewEdgeRecord {
        from_entity_id: from.to_string(),
        to_entity_id: to.to_string(),
        relation: relation.to_string(),
        weight: confidence,
        metadata_json: "{}".to_string(),
        source_type: source_type.to_string(),
        source_url: None,
        model: None,
        skill_id: Some("legacy_seed_import".to_string()),
        triggered_by: None,
    });
}

fn society_entity_id(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    let slug = value.strip_prefix("soc-").unwrap_or(value);
    Some(format!("society:{slug}"))
}

fn area_entity_id(raw: &str) -> String {
    let value = raw.trim();
    let slug = value.strip_prefix("area-").unwrap_or(value);
    format!("area:{slug}")
}

fn area_entity_id_from_name(area_name: &str) -> String {
    format!("area:{}", slug(area_name))
}

fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn openestates_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate has parent directory")
        .to_path_buf()
}

fn asset_id(id: &str) -> AssetId {
    AssetId::new(id).expect("static canonical asset id is valid")
}

async fn read_parquet_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    relative_path: &str,
) -> Result<Vec<u8>, CanonicalNodesError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(relative_path))
        .map(|artifact| artifact.key.clone())
        .ok_or_else(|| CanonicalNodesError::MissingArtifact {
            asset_id: record.asset_id.to_string(),
            path: relative_path.to_string(),
        })?;
    lake.get_bytes(&crate::lake::LakeKey::new(key).map_err(CanonicalNodesError::Key)?)
        .await
        .map_err(CanonicalNodesError::Lake)
}

#[derive(Debug)]
pub enum CanonicalNodesError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Lake(LakeError),
    Key(crate::lake::keys::KeyError),
    Rera(ReraAssetError),
    MissingArtifact { asset_id: String, path: String },
}

impl fmt::Display for CanonicalNodesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "canonical nodes IO error: {err}"),
            Self::Json(err) => write!(f, "canonical nodes JSON error: {err}"),
            Self::Lake(err) => write!(f, "canonical nodes lake error: {err}"),
            Self::Key(err) => write!(f, "canonical nodes key error: {err}"),
            Self::Rera(err) => write!(f, "canonical nodes parquet error: {err}"),
            Self::MissingArtifact { asset_id, path } => {
                write!(f, "missing artifact {path} for {asset_id}")
            }
        }
    }
}

impl std::error::Error for CanonicalNodesError {}

impl From<std::io::Error> for CanonicalNodesError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for CanonicalNodesError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<LakeError> for CanonicalNodesError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<ReraAssetError> for CanonicalNodesError {
    fn from(err: ReraAssetError) -> Self {
        Self::Rera(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_rows_link_properties_to_societies() {
        let root = openestates_project_root();
        let rows = property_rows_from_seed(&root).expect("seed properties should load");
        assert!(!rows.entities.is_empty());
        assert!(!rows.edges.is_empty());
        assert!(rows
            .entities
            .iter()
            .any(|entity| entity.entity_type == "property"));
        assert!(rows
            .edges
            .iter()
            .any(|edge| edge.relation == "in_society"));
    }

    #[test]
    fn area_rows_link_societies_to_areas() {
        let root = openestates_project_root();
        let rows = area_rows_from_seed(&root).expect("seed areas should load");
        assert!(!rows.entities.is_empty());
        assert!(rows.edges.iter().any(|edge| edge.relation == "in_area"));
    }
}
