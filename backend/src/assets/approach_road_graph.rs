use std::collections::BTreeMap;
use std::fmt;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::lake::LakeStore;

use super::kg_view::{KgViewEdgeRecord, KgViewEntityRecord};
use super::rera::{read_edges, read_entities, write_edges, write_entities, ReraAssetError};
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SkillFactAnnotationRecord, SkillFactRecord,
    SkillFactsInput, SourceWatermark,
};

pub const CANONICAL_ROAD_NODES_ASSET_ID: &str = "canonical_road_nodes";
pub const APPROACH_ROAD_GRAPH_FACTS_ASSET_ID: &str = "approach_road_graph_facts";

const APPROACH_ROAD_VISUALS_JSON: &str =
    include_str!("../../../data/validation/approach_road_visuals.json");
const SOCIETY_LOCAL_CONTEXT_JSON: &str =
    include_str!("../../../data/validation/society_local_context.json");

#[derive(Debug, Clone, Deserialize)]
struct SocietyLocalContextFile {
    #[serde(default)]
    graphs: Vec<SocietyLocalContextGraph>,
}

#[derive(Debug, Clone, Deserialize)]
struct SocietyLocalContextGraph {
    society_id: String,
    #[serde(default)]
    entities: Vec<SocietyLocalContextEntity>,
    #[serde(default)]
    edges: Vec<SocietyLocalContextEdge>,
    #[serde(default)]
    facts: Vec<SocietyLocalContextFact>,
}

#[derive(Debug, Clone, Deserialize)]
struct SocietyLocalContextEntity {
    entity_id: String,
    entity_type: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SocietyLocalContextEdge {
    to_entity_id: String,
    relation: String,
    #[serde(default = "default_local_edge_weight")]
    weight: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct SocietyLocalContextFact {
    entity_id: String,
    fact_key: String,
    value: String,
    #[serde(default = "default_local_fact_confidence")]
    confidence: f32,
    #[serde(default = "default_local_fact_source")]
    source_type: String,
}

fn default_local_edge_weight() -> f32 {
    0.85
}

fn default_local_fact_confidence() -> f32 {
    0.75
}

fn default_local_fact_source() -> String {
    "LocalContextSeed".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct ApproachRoadVisualRecord {
    entity_id: String,
    slug: String,
    title: String,
    provider: String,
    coverage_quality: String,
    #[serde(default)]
    frames: Vec<ApproachRoadVisualFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApproachRoadVisualFrame {
    label: String,
    distance_from_gate_m: u32,
    pano_id: String,
    heading: f64,
    pitch: f64,
    fov: f64,
    capture_date: String,
    view_role: String,
    lat: f64,
    lng: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
}

#[derive(Clone)]
pub struct CanonicalRoadNodesMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl CanonicalRoadNodesMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        version: &str,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
        parent_materializations: Vec<MaterializationId>,
    ) -> Result<MaterializationRecord, ApproachRoadGraphError> {
        let rows = road_graph_rows_from_visuals()?;
        write_canonical_road_materialization(
            &self.lake,
            &self.materializations,
            version,
            dag_run_id,
            partition,
            parent_materializations,
            rows,
        )
        .await
    }
}

pub fn road_graph_rows_from_visuals(
) -> Result<super::canonical_nodes::CanonicalNodeRows, ApproachRoadGraphError> {
    let records: Vec<ApproachRoadVisualRecord> =
        serde_json::from_str(APPROACH_ROAD_VISUALS_JSON).map_err(ApproachRoadGraphError::Json)?;
    let now = Utc::now();
    let mut entities = BTreeMap::<String, KgViewEntityRecord>::new();
    let mut edges = BTreeMap::<(String, String, String), KgViewEdgeRecord>::new();

    for record in records {
        let society_id = record.entity_id.trim().to_string();
        if !society_id.starts_with("society:") {
            continue;
        }
        let road_id = road_segment_entity_id(&record.slug);
        let place_id = place_entity_id(&record.slug);
        insert_entity(
            &mut entities,
            road_id.clone(),
            "road_segment",
            &format!("{} approach road", record.title),
            "approach_road_bootstrap",
            now,
        );
        insert_entity(
            &mut entities,
            place_id.clone(),
            "place",
            &format!("Schools near {}", record.title),
            "approach_road_bootstrap",
            now,
        );
        insert_edge(
            &mut edges,
            &society_id,
            &road_id,
            "served_by_road",
            "ApproachRoadBootstrap",
            0.85,
        );
        insert_edge(
            &mut edges,
            &society_id,
            &place_id,
            "maps_to_place",
            "ApproachRoadBootstrap",
            0.7,
        );
    }

    merge_local_context_graph(&mut entities, &mut edges, now)?;

    Ok(super::canonical_nodes::CanonicalNodeRows {
        entities: entities.into_values().collect(),
        edges: edges.into_values().collect(),
    })
}

fn merge_local_context_graph(
    entities: &mut BTreeMap<String, KgViewEntityRecord>,
    edges: &mut BTreeMap<(String, String, String), KgViewEdgeRecord>,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApproachRoadGraphError> {
    let file: SocietyLocalContextFile =
        serde_json::from_str(SOCIETY_LOCAL_CONTEXT_JSON).map_err(ApproachRoadGraphError::Json)?;
    for graph in file.graphs {
        let society_id = graph.society_id.trim().to_string();
        if !society_id.starts_with("society:") {
            continue;
        }
        for entity in graph.entities {
            insert_entity(
                entities,
                entity.entity_id.clone(),
                &entity.entity_type,
                &entity.name,
                "local_context_seed",
                now,
            );
        }
        for edge in graph.edges {
            insert_edge(
                edges,
                &society_id,
                &edge.to_entity_id,
                &edge.relation,
                "LocalContextSeed",
                edge.weight,
            );
        }
    }
    Ok(())
}

pub fn approach_road_graph_facts_input(
    run_id: &MaterializationId,
    as_of: chrono::DateTime<Utc>,
) -> Result<SkillFactsInput, ApproachRoadGraphError> {
    let records: Vec<ApproachRoadVisualRecord> =
        serde_json::from_str(APPROACH_ROAD_VISUALS_JSON).map_err(ApproachRoadGraphError::Json)?;
    let learned_at = as_of;
    let snapshot_date = learned_at.format("%Y-%m-%d").to_string();
    let mut facts = Vec::new();
    let mut annotations = Vec::new();

    for record in records {
        let society_id = record.entity_id.trim().to_string();
        if !society_id.starts_with("society:") {
            continue;
        }
        let road_id = road_segment_entity_id(&record.slug);
        let place_id = place_entity_id(&record.slug);
        if record.coverage_quality != "missing" && !record.frames.is_empty() {
            let payload = serde_json::json!({
                "provider": record.provider,
                "coverage_quality": record.coverage_quality,
                "frames": frames_with_image_urls(&record.frames),
            });
            push_text_fact(
                &mut facts,
                &road_id,
                "media.approach_road_frames",
                &payload.to_string(),
                0.9,
                "ApproachRoadBootstrap",
                learned_at,
                run_id,
            );
            push_bool_fact(
                &mut facts,
                &road_id,
                "approach_road_visual_available",
                true,
                0.9,
                learned_at,
                run_id,
            );
            push_bool_fact(
                &mut facts,
                &society_id,
                "approach_road_visual_available",
                true,
                0.85,
                learned_at,
                run_id,
            );
        }
        push_text_fact(
            &mut facts,
            &road_id,
            "risk.approach_road_waterlogging",
            "mentioned",
            0.4,
            "RedditTheme",
            learned_at,
            run_id,
        );
        push_text_fact(
            &mut facts,
            &place_id,
            "positive.school_access",
            "mentioned",
            0.75,
            "Google",
            learned_at,
            run_id,
        );
        annotations.push(annotation_row(&road_id, "media.approach_road_frames"));
        annotations.push(annotation_row(&road_id, "risk.approach_road_waterlogging"));
        annotations.push(annotation_row(&place_id, "positive.school_access"));
    }

    let local_context: SocietyLocalContextFile =
        serde_json::from_str(SOCIETY_LOCAL_CONTEXT_JSON).map_err(ApproachRoadGraphError::Json)?;
    for graph in local_context.graphs {
        for fact in graph.facts {
            push_text_fact(
                &mut facts,
                &fact.entity_id,
                &fact.fact_key,
                &fact.value,
                fact.confidence,
                &fact.source_type,
                learned_at,
                run_id,
            );
            annotations.push(annotation_row(&fact.entity_id, &fact.fact_key));
        }
    }

    Ok(SkillFactsInput {
        source: "approach_road".to_string(),
        snapshot_date,
        facts,
        fact_annotations: annotations,
        source_watermarks: vec![SourceWatermark {
            source: "approach_road_graph_bootstrap".to_string(),
            high_watermark: learned_at.to_rfc3339(),
        }],
    })
}

fn annotation_row(entity_id: &str, fact_key: &str) -> SkillFactAnnotationRecord {
    SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: None,
        answers_preferences_json: "[]".to_string(),
        scoring_direction: None,
        scoring_weight: None,
        scoring_thresholds_json: "[]".to_string(),
    }
}

fn push_text_fact(
    facts: &mut Vec<SkillFactRecord>,
    entity_id: &str,
    fact_key: &str,
    value: &str,
    confidence: f32,
    source_type: &str,
    learned_at: chrono::DateTime<Utc>,
    run_id: &MaterializationId,
) {
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: "text".to_string(),
        value_json: serde_json::json!({"type":"Text","data": value}).to_string(),
        confidence,
        source_type: source_type.to_string(),
        source_url: None,
        model: None,
        skill_id: Some("approach_road_graph_bootstrap".to_string()),
        triggered_by: Some("bootstrap_import".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: "sha256:approach-road-graph".to_string(),
    });
}

fn push_bool_fact(
    facts: &mut Vec<SkillFactRecord>,
    entity_id: &str,
    fact_key: &str,
    value: bool,
    confidence: f32,
    learned_at: chrono::DateTime<Utc>,
    run_id: &MaterializationId,
) {
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: "bool".to_string(),
        value_json: serde_json::json!({"type":"Bool","data": value}).to_string(),
        confidence,
        source_type: "ApproachRoadBootstrap".to_string(),
        source_url: None,
        model: None,
        skill_id: Some("approach_road_graph_bootstrap".to_string()),
        triggered_by: Some("bootstrap_import".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: "sha256:approach-road-graph".to_string(),
    });
}

fn road_segment_entity_id(slug: &str) -> String {
    format!("road_segment:{slug}-approach")
}

fn place_entity_id(slug: &str) -> String {
    format!("place:{slug}-nearby-schools")
}

fn frames_with_image_urls(frames: &[ApproachRoadVisualFrame]) -> Vec<ApproachRoadVisualFrame> {
    let api_key = crate::street_view::google_maps_api_key();
    frames
        .iter()
        .take(5)
        .map(|frame| {
            let mut enriched = frame.clone();
            if enriched.image_url.is_none() {
                if let Some(key) = api_key.as_deref() {
                    let input = crate::street_view::StreetViewFrameInput {
                        pano_id: frame.pano_id.clone(),
                        heading: frame.heading,
                        pitch: frame.pitch,
                        fov: frame.fov,
                    };
                    enriched.image_url = crate::street_view::street_view_static_url(&input, key);
                }
            }
            enriched
        })
        .collect()
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
        skill_id: Some("approach_road_graph_bootstrap".to_string()),
        triggered_by: None,
    });
}

async fn write_canonical_road_materialization(
    lake: &LakeStore,
    materializations: &AssetMaterializationStore,
    version: &str,
    dag_run_id: MaterializationId,
    partition: AssetPartition,
    parent_materializations: Vec<MaterializationId>,
    rows: super::canonical_nodes::CanonicalNodeRows,
) -> Result<MaterializationRecord, ApproachRoadGraphError> {
    let entity_key = AssetPathBuilder::gold_asset_key(
        CANONICAL_ROAD_NODES_ASSET_ID,
        version,
        "entities/part-00000.parquet",
    );
    let edge_key = AssetPathBuilder::gold_asset_key(
        CANONICAL_ROAD_NODES_ASSET_ID,
        version,
        "edges/part-00000.parquet",
    );
    let artifacts = vec![
        ArtifactRef::parquet(
            lake.put_bytes(&entity_key, write_entities(&rows.entities)?)
                .await?,
        ),
        ArtifactRef::parquet(lake.put_bytes(&edge_key, write_edges(&rows.edges)?).await?),
    ];
    let record = MaterializationRecord::succeeded(
        asset_id(CANONICAL_ROAD_NODES_ASSET_ID),
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

pub async fn read_canonical_road_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<super::canonical_nodes::CanonicalNodeRows, ApproachRoadGraphError> {
    let entity_bytes = read_parquet_artifact(lake, record, "entities/part-00000.parquet").await?;
    let edge_bytes = read_parquet_artifact(lake, record, "edges/part-00000.parquet").await?;
    Ok(super::canonical_nodes::CanonicalNodeRows {
        entities: read_entities(entity_bytes)?,
        edges: read_edges(edge_bytes)?,
    })
}

async fn read_parquet_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
) -> Result<Vec<u8>, ApproachRoadGraphError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .map(|artifact| artifact.key.clone())
        .ok_or_else(|| ApproachRoadGraphError::MissingArtifact {
            asset: record.asset_id.to_string(),
            suffix: suffix.to_string(),
        })?;
    lake.get_bytes(
        &crate::lake::LakeKey::new(key)
            .map_err(|err| ApproachRoadGraphError::Lake(crate::lake::LakeError::Key(err)))?,
    )
    .await
    .map_err(ApproachRoadGraphError::Lake)
}

fn asset_id(id: &str) -> AssetId {
    AssetId::new(id).expect("static approach road asset id is valid")
}

#[derive(Debug)]
pub enum ApproachRoadGraphError {
    Json(serde_json::Error),
    Lake(crate::lake::LakeError),
    Rera(ReraAssetError),
    MissingArtifact { asset: String, suffix: String },
}

impl fmt::Display for ApproachRoadGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "approach road graph JSON error: {err}"),
            Self::Lake(err) => write!(f, "approach road graph lake error: {err}"),
            Self::Rera(err) => write!(f, "approach road graph parquet error: {err}"),
            Self::MissingArtifact { asset, suffix } => {
                write!(f, "missing artifact {suffix} for asset {asset}")
            }
        }
    }
}

impl std::error::Error for ApproachRoadGraphError {}

impl From<serde_json::Error> for ApproachRoadGraphError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<crate::lake::LakeError> for ApproachRoadGraphError {
    fn from(value: crate::lake::LakeError) -> Self {
        Self::Lake(value)
    }
}

impl From<ReraAssetError> for ApproachRoadGraphError {
    fn from(value: ReraAssetError) -> Self {
        Self::Rera(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_graph_rows_include_local_context_places() {
        let rows = road_graph_rows_from_visuals().expect("rows");
        assert!(rows.entities.iter().any(|entity| {
            entity.entity_id == "place:deens-public-school" && entity.name == "Deens Public School"
        }));
        assert!(rows.edges.iter().any(|edge| {
            edge.from_entity_id == "society:prestige-waterford"
                && edge.to_entity_id == "road:ecc-road"
                && edge.relation == "served_by_road"
        }));
    }

    #[test]
    fn road_graph_rows_include_served_by_road_edges() {
        let rows = road_graph_rows_from_visuals().expect("rows");
        assert!(!rows.entities.is_empty());
        assert!(rows
            .edges
            .iter()
            .any(|edge| edge.relation == "served_by_road"));
        assert!(rows
            .entities
            .iter()
            .any(|entity| entity.entity_type == "road_segment"));
        assert!(rows
            .entities
            .iter()
            .any(|entity| entity.entity_type == "place"));
    }

    #[test]
    fn approach_road_facts_attach_to_road_segment() {
        let run_id = MaterializationId::new();
        let input = approach_road_graph_facts_input(&run_id, Utc::now()).expect("facts");
        assert!(input.facts.iter().any(|fact| {
            fact.entity_id.starts_with("road_segment:")
                && fact.fact_key == "media.approach_road_frames"
        }));
    }
}
