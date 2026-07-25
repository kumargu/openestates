use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeStore};

use super::kg_view::{KgViewEdgeRecord, KgViewEntityRecord};
use super::rera::{read_edges, read_entities, write_edges, write_entities, ReraAssetError};
use super::skill_facts::{
    read_skill_fact_artifact_rows, write_fact_annotations_parquet, write_facts_parquet,
    SkillFactAnnotationRecord, SkillFactArtifactRows, SkillFactMaterializeError, SkillFactRecord,
};
use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    CanonicalNodeRows, CanonicalSocietyRows, MaterializationId, MaterializationRecord,
    SourceWatermark, CANONICAL_SOCIETY_NODES_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID,
    RERA_LEGAL_FACTS_ASSET_ID,
};

pub const APPROACH_ROAD_GRAPH_FACTS_ASSET_ID: &str = "approach_road_graph_facts";

const APPROACH_ROAD_GRAPH_FORMAT_VERSION: u32 = 1;
const APPROACH_ROAD_GRAPH_SOURCE: &str = "approach_road";
const ACCESS_ROAD_QUALITY_FACT_KEY: &str = "access_road_quality";
const APPROACH_ROAD_FRAMES_FACT_KEY: &str = "media.approach_road_frames";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApproachRoadGraphManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub source: String,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,
    pub edge_count: u64,
    pub fact_count: u64,
    pub fact_annotation_count: u64,
    pub entity_parquet_key: String,
    pub edge_parquet_key: String,
    pub fact_parquet_key: String,
    pub fact_annotation_parquet_key: String,
    pub manifest_key: String,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone)]
pub struct ApproachRoadGraphMaterialization {
    pub manifest: ApproachRoadGraphManifest,
    pub record: MaterializationRecord,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApproachRoadGraphRows {
    pub canonical: CanonicalNodeRows,
    pub skill_facts: SkillFactArtifactRows,
}

#[derive(Clone)]
pub struct ApproachRoadGraphMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ApproachRoadGraphMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
        }
    }

    pub async fn materialize_for_run(
        &self,
        planned_at: DateTime<Utc>,
        parent_records: &[MaterializationRecord],
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<ApproachRoadGraphMaterialization, ApproachRoadGraphError> {
        let canonical_record = parent_records
            .iter()
            .find(|record| record.asset_id.as_str() == CANONICAL_SOCIETY_NODES_ASSET_ID)
            .ok_or(ApproachRoadGraphError::MissingCanonicalSocieties)?;
        let canonical_rows =
            super::read_canonical_society_rows(&self.lake, canonical_record).await?;
        let upstream_records = parent_records
            .iter()
            .filter(|record| {
                matches!(
                    record.asset_id.as_str(),
                    RERA_LEGAL_FACTS_ASSET_ID | GOOGLE_REVIEW_FACTS_ASSET_ID
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let upstream_rows = read_skill_fact_artifact_rows(&self.lake, &upstream_records).await?;
        let rows = rows_from_upstream(&canonical_rows, &upstream_rows, planned_at, &dag_run_id)?;

        let snapshot_date = planned_at.format("%Y-%m-%d").to_string();
        let run_id = dag_run_id.to_string();
        let entity_key = AssetPathBuilder::silver_asset_key(
            APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
            APPROACH_ROAD_GRAPH_SOURCE,
            &snapshot_date,
            &run_id,
            "entities/part-00000.parquet",
        );
        let entity_meta = self
            .lake
            .put_bytes(&entity_key, write_entities(&rows.canonical.entities)?)
            .await?;

        let edge_key = AssetPathBuilder::silver_asset_key(
            APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
            APPROACH_ROAD_GRAPH_SOURCE,
            &snapshot_date,
            &run_id,
            "edges/part-00000.parquet",
        );
        let edge_meta = self
            .lake
            .put_bytes(&edge_key, write_edges(&rows.canonical.edges)?)
            .await?;

        let fact_key = AssetPathBuilder::silver_asset_key(
            APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
            APPROACH_ROAD_GRAPH_SOURCE,
            &snapshot_date,
            &run_id,
            "facts/part-00000.parquet",
        );
        let fact_meta = self
            .lake
            .put_bytes(&fact_key, write_facts_parquet(&rows.skill_facts.facts)?)
            .await?;

        let fact_annotation_key = AssetPathBuilder::silver_asset_key(
            APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
            APPROACH_ROAD_GRAPH_SOURCE,
            &snapshot_date,
            &run_id,
            "fact_annotations/part-00000.parquet",
        );
        let fact_annotation_meta = self
            .lake
            .put_bytes(
                &fact_annotation_key,
                write_fact_annotations_parquet(&rows.skill_facts.fact_annotations)?,
            )
            .await?;

        let manifest_key = AssetPathBuilder::silver_asset_key(
            APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
            APPROACH_ROAD_GRAPH_SOURCE,
            &snapshot_date,
            &run_id,
            "manifest.json",
        );
        let mut artifacts = vec![
            ArtifactRef::parquet(entity_meta),
            ArtifactRef::parquet(edge_meta),
            ArtifactRef::parquet(fact_meta),
            ArtifactRef::parquet(fact_annotation_meta),
        ];
        let manifest = ApproachRoadGraphManifest {
            asset_id: APPROACH_ROAD_GRAPH_FACTS_ASSET_ID.to_string(),
            format_version: APPROACH_ROAD_GRAPH_FORMAT_VERSION,
            source: APPROACH_ROAD_GRAPH_SOURCE.to_string(),
            snapshot_date: snapshot_date.clone(),
            run_id: run_id.clone(),
            created_at: Utc::now(),
            entity_count: rows.canonical.entities.len() as u64,
            edge_count: rows.canonical.edges.len() as u64,
            fact_count: rows.skill_facts.facts.len() as u64,
            fact_annotation_count: rows.skill_facts.fact_annotations.len() as u64,
            entity_parquet_key: entity_key.to_string(),
            edge_parquet_key: edge_key.to_string(),
            fact_parquet_key: fact_key.to_string(),
            fact_annotation_parquet_key: fact_annotation_key.to_string(),
            manifest_key: manifest_key.to_string(),
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let parent_materializations = parent_records
            .iter()
            .map(|record| record.materialization_id.clone())
            .collect::<Vec<_>>();
        let record = MaterializationRecord::succeeded(
            AssetId::new(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID)
                .map_err(ApproachRoadGraphError::AssetId)?,
            AssetStage::Silver,
            record_partition,
            snapshot_date,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(vec![SourceWatermark {
            source: APPROACH_ROAD_GRAPH_SOURCE.to_string(),
            high_watermark: upstream_watermark(parent_records),
        }])
        .with_row_count(rows.skill_facts.facts.len() as u64);

        self.materializations.write_materialization(&record).await?;

        Ok(ApproachRoadGraphMaterialization { manifest, record })
    }
}

pub async fn read_approach_road_graph_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<ApproachRoadGraphRows, ApproachRoadGraphError> {
    let entities =
        read_entities(read_artifact_bytes(lake, record, "entities/part-00000.parquet").await?)?;
    let edges = read_edges(read_artifact_bytes(lake, record, "edges/part-00000.parquet").await?)?;
    let skill_facts = read_skill_fact_artifact_rows(lake, std::slice::from_ref(record)).await?;
    Ok(ApproachRoadGraphRows {
        canonical: CanonicalNodeRows { entities, edges },
        skill_facts,
    })
}

async fn read_artifact_bytes(
    lake: &LakeStore,
    record: &MaterializationRecord,
    relative_path: &str,
) -> Result<Vec<u8>, ApproachRoadGraphError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(relative_path))
        .map(|artifact| artifact.key.clone())
        .ok_or_else(|| ApproachRoadGraphError::MissingArtifact {
            asset_id: record.asset_id.to_string(),
            path: relative_path.to_string(),
        })?;
    lake.get_bytes(&crate::lake::LakeKey::new(key).map_err(ApproachRoadGraphError::Key)?)
        .await
        .map_err(ApproachRoadGraphError::Lake)
}

fn rows_from_upstream(
    canonical: &CanonicalSocietyRows,
    upstream: &SkillFactArtifactRows,
    learned_at: DateTime<Utc>,
    run_id: &MaterializationId,
) -> Result<ApproachRoadGraphRows, ApproachRoadGraphError> {
    let mut evidence_by_society = canonical_society_evidence(canonical);
    for fact in &upstream.facts {
        let Some(evidence) = evidence_by_society.get_mut(&fact.entity_id) else {
            continue;
        };
        evidence.accept_fact(fact)?;
    }

    let mut entities = BTreeMap::<String, KgViewEntityRecord>::new();
    let mut edges = BTreeMap::<(String, String, String), KgViewEdgeRecord>::new();
    let mut facts = Vec::new();
    let mut annotations = Vec::new();

    for evidence in evidence_by_society
        .values()
        .filter(|evidence| evidence.has_location_signal())
    {
        let road_entity_id = road_entity_id(&evidence.society_entity_id);
        let road_name = format!("{} approach", evidence.society_name);
        let quality = evidence.access_road_quality();
        let media = evidence.approach_road_media();
        let road_fact_count = 1 + u32::from(media.is_some());
        entities.insert(
            road_entity_id.clone(),
            KgViewEntityRecord {
                entity_id: road_entity_id.clone(),
                entity_type: "road_segment".to_string(),
                name: road_name,
                root_source: Some(quality.source_type.to_ascii_lowercase()),
                fact_count: road_fact_count,
                created_at: learned_at,
                updated_at: learned_at,
            },
        );
        edges.insert(
            (
                evidence.society_entity_id.clone(),
                road_entity_id.clone(),
                "served_by_road".to_string(),
            ),
            KgViewEdgeRecord {
                from_entity_id: evidence.society_entity_id.clone(),
                to_entity_id: road_entity_id.clone(),
                relation: "served_by_road".to_string(),
                weight: quality.confidence,
                metadata_json: serde_json::json!({
                    "method": "derived_from_upstream_facts",
                    "evidence": quality.evidence_keys
                })
                .to_string(),
                source_type: quality.source_type.clone(),
                source_url: quality.source_url.clone(),
                model: None,
                skill_id: Some(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID.to_string()),
                triggered_by: None,
            },
        );
        facts.push(SkillFactRecord {
            entity_id: road_entity_id.clone(),
            fact_key: ACCESS_ROAD_QUALITY_FACT_KEY.to_string(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text(quality.value))?,
            confidence: quality.confidence,
            source_type: quality.source_type,
            source_url: quality.source_url,
            model: None,
            skill_id: Some(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID.to_string()),
            triggered_by: Some(evidence.society_entity_id.clone()),
            learned_at,
            run_id: run_id.to_string(),
            input_hash: quality.input_hash,
        });
        annotations.push(SkillFactAnnotationRecord {
            entity_id: road_entity_id.clone(),
            fact_key: ACCESS_ROAD_QUALITY_FACT_KEY.to_string(),
            display_template: Some("Road quality: {value}".to_string()),
            answers_preferences_json: serde_json::to_string(&[
                "approach road",
                "access road",
                "road quality",
            ])?,
            scoring_direction: Some("TextMatch".to_string()),
            scoring_weight: Some(0.7),
            scoring_thresholds_json: serde_json::to_string(&Vec::<f64>::new())?,
        });
        if let Some(media) = media {
            facts.push(SkillFactRecord {
                entity_id: road_entity_id.clone(),
                fact_key: APPROACH_ROAD_FRAMES_FACT_KEY.to_string(),
                value_type: "text".to_string(),
                value_json: serde_json::to_string(&FactValue::Text(serde_json::to_string(
                    &media.record,
                )?))?,
                confidence: media.confidence,
                source_type: media.source_type,
                source_url: media.source_url,
                model: None,
                skill_id: Some(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID.to_string()),
                triggered_by: Some(evidence.society_entity_id.clone()),
                learned_at,
                run_id: run_id.to_string(),
                input_hash: media.input_hash,
            });
            annotations.push(SkillFactAnnotationRecord {
                entity_id: road_entity_id,
                fact_key: APPROACH_ROAD_FRAMES_FACT_KEY.to_string(),
                display_template: Some("Approach road visuals available".to_string()),
                answers_preferences_json: serde_json::to_string(&[
                    "approach road image",
                    "approach road visuals",
                    "street view",
                    "road outside society",
                ])?,
                scoring_direction: Some("TextMatch".to_string()),
                scoring_weight: Some(0.2),
                scoring_thresholds_json: serde_json::to_string(&Vec::<f64>::new())?,
            });
        }
    }

    Ok(ApproachRoadGraphRows {
        canonical: CanonicalNodeRows {
            entities: entities.into_values().collect(),
            edges: edges.into_values().collect(),
        },
        skill_facts: SkillFactArtifactRows {
            facts,
            fact_annotations: annotations,
        },
    })
}

#[derive(Debug, Clone)]
struct SocietyRoadEvidence {
    society_entity_id: String,
    society_name: String,
    address: Option<FactSignal>,
    latitude: Option<FactSignal>,
    longitude: Option<FactSignal>,
    frontage_road: Option<FactSignal>,
    google_place_id: Option<FactSignal>,
}

impl SocietyRoadEvidence {
    fn accept_fact(&mut self, fact: &SkillFactRecord) -> Result<(), ApproachRoadGraphError> {
        match fact.fact_key.as_str() {
            "rera_project_address" | "google_place_address" => {
                if let Some(value) = text_fact_value(fact)? {
                    self.accept_frontage_road(fact, &value)?;
                    self.address = Some(FactSignal::from_text_fact(fact, value));
                }
            }
            "geo.latitude" => {
                if let Some(value) = numeric_fact_value(fact)? {
                    self.latitude = Some(FactSignal::from_numeric_fact(fact, value));
                }
            }
            "geo.longitude" => {
                if let Some(value) = numeric_fact_value(fact)? {
                    self.longitude = Some(FactSignal::from_numeric_fact(fact, value));
                }
            }
            "google_place_id" => {
                if let Some(value) = text_fact_value(fact)? {
                    self.google_place_id = Some(FactSignal::from_text_fact(fact, value));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn accept_frontage_road(
        &mut self,
        fact: &SkillFactRecord,
        text: &str,
    ) -> Result<(), ApproachRoadGraphError> {
        if self.frontage_road.is_none() {
            if let Some(road_name) = extract_frontage_road_name(text) {
                self.frontage_road = Some(FactSignal::from_text_fact(fact, road_name));
            }
        }
        Ok(())
    }

    fn has_location_signal(&self) -> bool {
        self.address.is_some()
            || (self.latitude.is_some() && self.longitude.is_some())
            || self.frontage_road.is_some()
            || self.google_place_id.is_some()
    }

    fn access_road_quality(&self) -> DerivedRoadQuality {
        let mut evidence_keys = BTreeSet::new();
        let primary = if let (Some(latitude), Some(longitude)) = (&self.latitude, &self.longitude) {
            evidence_keys.insert("geo.latitude".to_string());
            evidence_keys.insert("geo.longitude".to_string());
            if let Some(address) = &self.address {
                evidence_keys.insert(address.fact_key.clone());
            }
            DerivedRoadQuality {
                value: "Approach road inferred from RERA project coordinates; visual road-width verification is pending.".to_string(),
                confidence: if self.address.is_some() { 0.74 } else { 0.7 },
                source_type: latitude.source_type.clone(),
                source_url: latitude.source_url.clone().or_else(|| longitude.source_url.clone()),
                evidence_keys: Vec::new(),
                input_hash: String::new(),
            }
        } else if let Some(address) = &self.address {
            evidence_keys.insert(address.fact_key.clone());
            DerivedRoadQuality {
                value: "Approach road inferred from RERA project address; visual road-width verification is pending.".to_string(),
                confidence: 0.66,
                source_type: address.source_type.clone(),
                source_url: address.source_url.clone(),
                evidence_keys: Vec::new(),
                input_hash: String::new(),
            }
        } else if let Some(place) = &self.google_place_id {
            evidence_keys.insert("google_place_id".to_string());
            DerivedRoadQuality {
                value: "Approach road inferred from Google place context; visual road-width verification is pending.".to_string(),
                confidence: 0.68,
                source_type: place.source_type.clone(),
                source_url: place.source_url.clone(),
                evidence_keys: Vec::new(),
                input_hash: String::new(),
            }
        } else {
            unreachable!("has_location_signal guarantees one non-review location signal")
        };
        let evidence_keys = evidence_keys.into_iter().collect::<Vec<_>>();
        DerivedRoadQuality {
            input_hash: format!("{}:{}", self.society_entity_id, evidence_keys.join(",")),
            evidence_keys,
            ..primary
        }
    }

    fn approach_road_media(&self) -> Option<DerivedRoadMedia> {
        if let Some(location_query) = self.street_view_frontage_query() {
            let source_signal = self
                .frontage_road
                .as_ref()
                .or(self.address.as_ref())
                .or(self.google_place_id.as_ref())?;
            return Some(self.query_backed_media(&location_query, source_signal, 0.72));
        }

        if let (Some(latitude), Some(longitude)) = (&self.latitude, &self.longitude) {
            if let (Some(latitude_value), Some(longitude_value)) =
                (latitude.numeric, longitude.numeric)
            {
                let frames = approach_road_frame_specs()
                    .map(|(label, distance_from_gate_m, heading)| ApproachRoadVisualFrameFact {
                        label: label.to_string(),
                        distance_from_gate_m,
                        pano_id: None,
                        latitude: Some(latitude_value),
                        longitude: Some(longitude_value),
                        location_query: None,
                        radius_m: Some(250),
                        heading,
                        pitch: 0.0,
                        fov: 80.0,
                        capture_date: "latest available".to_string(),
                        image_url: None,
                    })
                    .collect::<Vec<_>>();
                let source_url = latitude
                    .source_url
                    .clone()
                    .or_else(|| longitude.source_url.clone());
                return Some(DerivedRoadMedia {
                    record: ApproachRoadVisualFact {
                        provider: "Google Street View".to_string(),
                        coverage_quality: "usable".to_string(),
                        frames,
                    },
                    confidence: 0.7,
                    source_type: latitude.source_type.clone(),
                    source_url,
                    input_hash: format!(
                        "{}:street-view:{latitude_value:.7}:{longitude_value:.7}",
                        self.society_entity_id
                    ),
                });
            }
        };

        let location_query = self.street_view_location_query()?;
        let source_signal = self.google_place_id.as_ref().or(self.address.as_ref())?;
        Some(self.query_backed_media(&location_query, source_signal, 0.64))
    }

    fn query_backed_media(
        &self,
        location_query: &str,
        source_signal: &FactSignal,
        confidence: f32,
    ) -> DerivedRoadMedia {
        let frames = approach_road_frame_specs()
            .map(|(label, distance_from_gate_m, heading)| ApproachRoadVisualFrameFact {
                label: label.to_string(),
                distance_from_gate_m,
                pano_id: None,
                latitude: None,
                longitude: None,
                location_query: Some(location_query.to_string()),
                radius_m: Some(250),
                heading,
                pitch: 0.0,
                fov: 80.0,
                capture_date: "latest available".to_string(),
                image_url: None,
            })
            .collect::<Vec<_>>();
        DerivedRoadMedia {
            record: ApproachRoadVisualFact {
                provider: "Google Street View".to_string(),
                coverage_quality: "usable".to_string(),
                frames,
            },
            confidence,
            source_type: source_signal.source_type.clone(),
            source_url: source_signal.source_url.clone(),
            input_hash: format!("{}:street-view:{location_query}", self.society_entity_id),
        }
    }

    fn street_view_frontage_query(&self) -> Option<String> {
        let road_name = self.frontage_road.as_ref()?.text.as_deref()?.trim();
        if road_name.is_empty() {
            return None;
        }
        Some(format!("{} {} Bengaluru", road_name, self.society_name))
    }

    fn street_view_location_query(&self) -> Option<String> {
        if let Some(address) = &self.address {
            if let Some(text) = address
                .text
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            {
                return Some(text.trim().to_string());
            }
        }
        if self.google_place_id.is_some() {
            return Some(format!("{} Bengaluru", self.society_name));
        }
        None
    }
}

#[derive(Debug, Clone)]
struct FactSignal {
    fact_key: String,
    source_type: String,
    source_url: Option<String>,
    numeric: Option<f64>,
    text: Option<String>,
}

impl FactSignal {
    fn from_numeric_fact(fact: &SkillFactRecord, numeric: f64) -> Self {
        Self {
            fact_key: fact.fact_key.clone(),
            source_type: fact.source_type.clone(),
            source_url: fact.source_url.clone(),
            numeric: Some(numeric),
            text: None,
        }
    }

    fn from_text_fact(fact: &SkillFactRecord, text: String) -> Self {
        Self {
            fact_key: fact.fact_key.clone(),
            source_type: fact.source_type.clone(),
            source_url: fact.source_url.clone(),
            numeric: None,
            text: Some(text),
        }
    }
}

#[derive(Debug, Clone)]
struct DerivedRoadQuality {
    value: String,
    confidence: f32,
    source_type: String,
    source_url: Option<String>,
    evidence_keys: Vec<String>,
    input_hash: String,
}

#[derive(Debug, Clone)]
struct DerivedRoadMedia {
    record: ApproachRoadVisualFact,
    confidence: f32,
    source_type: String,
    source_url: Option<String>,
    input_hash: String,
}

#[derive(Debug, Clone, Serialize)]
struct ApproachRoadVisualFact {
    provider: String,
    coverage_quality: String,
    frames: Vec<ApproachRoadVisualFrameFact>,
}

#[derive(Debug, Clone, Serialize)]
struct ApproachRoadVisualFrameFact {
    label: String,
    distance_from_gate_m: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pano_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    radius_m: Option<u32>,
    heading: f64,
    pitch: f64,
    fov: f64,
    capture_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
}

fn approach_road_frame_specs() -> impl Iterator<Item = (&'static str, u32, f64)> {
    [
        ("Gate approach", 0, 0.0),
        ("Road axis", 0, 90.0),
        ("Opposite approach", 0, 180.0),
        ("Cross approach", 0, 270.0),
        ("Approach road ahead", 80, 0.0),
        ("Next stretch", 160, 0.0),
    ]
    .into_iter()
}

fn canonical_society_evidence(
    canonical: &CanonicalSocietyRows,
) -> BTreeMap<String, SocietyRoadEvidence> {
    let mut names_by_id = canonical
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity.name.clone()))
        .collect::<BTreeMap<_, _>>();
    for mapping in &canonical.mappings {
        if let Some(alias) = &mapping.alias_entity_id {
            names_by_id.insert(alias.clone(), mapping.project_name.clone());
        }
    }

    names_by_id
        .into_iter()
        .filter(|(entity_id, _)| entity_id.starts_with("society:"))
        .map(|(entity_id, name)| {
            (
                entity_id.clone(),
                SocietyRoadEvidence {
                    society_entity_id: entity_id,
                    society_name: title_case_name(&name),
                    address: None,
                    latitude: None,
                    longitude: None,
                    frontage_road: None,
                    google_place_id: None,
                },
            )
        })
        .collect()
}

fn text_fact_value(fact: &SkillFactRecord) -> Result<Option<String>, ApproachRoadGraphError> {
    match serde_json::from_str::<FactValue>(&fact.value_json)? {
        FactValue::Text(value) if !value.trim().is_empty() => Ok(Some(value)),
        _ => Ok(None),
    }
}

fn numeric_fact_value(fact: &SkillFactRecord) -> Result<Option<f64>, ApproachRoadGraphError> {
    match serde_json::from_str::<FactValue>(&fact.value_json)? {
        FactValue::Numeric(value) if value.is_finite() => Ok(Some(value)),
        _ => Ok(None),
    }
}

fn extract_frontage_road_name(text: &str) -> Option<String> {
    for segment in text
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        if let Some(candidate) = extract_frontage_road_name_from_segment(segment) {
            return Some(candidate);
        }
    }
    extract_frontage_road_name_from_segment(text)
}

fn extract_frontage_road_name_from_segment(text: &str) -> Option<String> {
    let tokens = road_tokens(text);
    for index in 0..tokens.len() {
        let token = tokens[index].as_str();
        if token.eq_ignore_ascii_case("road")
            || token.eq_ignore_ascii_case("street")
            || token.eq_ignore_ascii_case("marg")
        {
            if let Some(candidate) = road_name_ending_at(&tokens, index) {
                return Some(candidate);
            }
        }
        if token.eq_ignore_ascii_case("highway") {
            if let Some(candidate) = highway_name_at(&tokens, index) {
                return Some(candidate);
            }
        }
        if matches!(token.to_ascii_lowercase().as_str(), "sh" | "nh") {
            if let Some(next) = tokens.get(index + 1).filter(|next| is_road_number(next)) {
                return Some(format!("{} {}", token.to_uppercase(), next));
            }
        }
    }
    None
}

fn road_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() {
            current.push(character);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn road_name_ending_at(tokens: &[String], suffix_index: usize) -> Option<String> {
    let mut start = suffix_index;
    while start > 0 && suffix_index - start < 3 {
        let previous = &tokens[start - 1];
        if !is_road_name_token(previous) {
            break;
        }
        start -= 1;
    }
    if start == suffix_index {
        return None;
    }
    Some(tokens[start..=suffix_index].join(" "))
}

fn highway_name_at(tokens: &[String], index: usize) -> Option<String> {
    let mut parts = Vec::new();
    if index > 0 && is_road_name_token(&tokens[index - 1]) {
        parts.push(tokens[index - 1].clone());
    }
    parts.push(tokens[index].clone());
    if let Some(next) = tokens.get(index + 1).filter(|next| is_road_number(next)) {
        parts.push(next.clone());
    }
    (parts.len() > 1).then(|| parts.join(" "))
}

fn is_road_name_token(token: &str) -> bool {
    if token.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    let normalized = token.to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "a" | "an"
            | "and"
            | "are"
            | "at"
            | "close"
            | "for"
            | "from"
            | "in"
            | "is"
            | "location"
            | "near"
            | "of"
            | "on"
            | "the"
            | "to"
            | "with"
    )
}

fn is_road_number(token: &str) -> bool {
    token.chars().all(|character| character.is_ascii_digit())
}

fn road_entity_id(society_entity_id: &str) -> String {
    let slug = society_entity_id
        .strip_prefix("society:")
        .unwrap_or(society_entity_id);
    format!("road_segment:{}-approach", slugify(slug))
}

fn slugify(value: &str) -> String {
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

fn title_case_name(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut output = first.to_uppercase().collect::<String>();
                    output.push_str(&chars.as_str().to_lowercase());
                    output
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn upstream_watermark(parent_records: &[MaterializationRecord]) -> String {
    parent_records
        .iter()
        .map(|record| format!("{}:{}", record.asset_id.as_str(), record.materialization_id))
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Debug)]
pub enum ApproachRoadGraphError {
    Json(serde_json::Error),
    Lake(LakeError),
    Key(crate::lake::keys::KeyError),
    Rera(ReraAssetError),
    SkillFact(SkillFactMaterializeError),
    AssetId(super::types::AssetIdError),
    MissingCanonicalSocieties,
    MissingArtifact { asset_id: String, path: String },
}

impl fmt::Display for ApproachRoadGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "approach road fact parse failed: {err}"),
            Self::Lake(err) => write!(f, "approach road lake error: {err}"),
            Self::Key(err) => write!(f, "approach road lake key error: {err}"),
            Self::Rera(err) => write!(f, "approach road graph parquet error: {err}"),
            Self::SkillFact(err) => write!(f, "approach road fact parquet error: {err}"),
            Self::AssetId(err) => write!(f, "approach road asset id error: {err}"),
            Self::MissingCanonicalSocieties => {
                write!(f, "approach road graph is missing canonical society parent")
            }
            Self::MissingArtifact { asset_id, path } => {
                write!(f, "missing artifact {path} for {asset_id}")
            }
        }
    }
}

impl std::error::Error for ApproachRoadGraphError {}

impl From<serde_json::Error> for ApproachRoadGraphError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<LakeError> for ApproachRoadGraphError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<ReraAssetError> for ApproachRoadGraphError {
    fn from(err: ReraAssetError) -> Self {
        Self::Rera(err)
    }
}

impl From<SkillFactMaterializeError> for ApproachRoadGraphError {
    fn from(err: SkillFactMaterializeError) -> Self {
        Self::SkillFact(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_rows_include_sumadhura_road_edge_and_fact() {
        let canonical = CanonicalSocietyRows {
            entities: vec![KgViewEntityRecord {
                entity_id: "society:sumadhura-capitol-residences".to_string(),
                entity_type: "society".to_string(),
                name: "SUMADHURA CAPITOL RESIDENCES".to_string(),
                root_source: Some("rera".to_string()),
                fact_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let upstream = SkillFactArtifactRows {
            facts: vec![
                test_fact(
                    "society:sumadhura-capitol-residences",
                    "rera_project_address",
                    FactValue::Text("Pattandur Agrahara".to_string()),
                ),
                test_fact(
                    "society:sumadhura-capitol-residences",
                    "geo.latitude",
                    FactValue::Numeric(12.9853),
                ),
                test_fact(
                    "society:sumadhura-capitol-residences",
                    "geo.longitude",
                    FactValue::Numeric(77.7507),
                ),
            ],
            fact_annotations: Vec::new(),
        };
        let rows = rows_from_upstream(&canonical, &upstream, Utc::now(), &MaterializationId::new())
            .expect("upstream rows should materialize");

        assert!(rows.canonical.edges.iter().any(|edge| {
            edge.from_entity_id == "society:sumadhura-capitol-residences"
                && edge.to_entity_id == "road_segment:sumadhura-capitol-residences-approach"
                && edge.relation == "served_by_road"
        }));
        assert!(rows.skill_facts.facts.iter().any(|fact| {
            fact.entity_id == "road_segment:sumadhura-capitol-residences-approach"
                && fact.fact_key == "access_road_quality"
        }));
        let media_fact = rows
            .skill_facts
            .facts
            .iter()
            .find(|fact| {
                fact.entity_id == "road_segment:sumadhura-capitol-residences-approach"
                    && fact.fact_key == "media.approach_road_frames"
            })
            .expect("coordinate-backed societies should emit Street View frame facts");
        let FactValue::Text(payload) =
            serde_json::from_str::<FactValue>(&media_fact.value_json).unwrap()
        else {
            panic!("media frames should be carried as JSON text");
        };
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["provider"], "Google Street View");
        assert_eq!(payload["coverage_quality"], "usable");
        assert_eq!(payload["frames"].as_array().unwrap().len(), 6);
        assert_eq!(payload["frames"][0]["latitude"], 12.9853);
        assert_eq!(payload["frames"][0]["longitude"], 77.7507);
        assert_eq!(payload["frames"][4]["distance_from_gate_m"], 80);
        assert_eq!(payload["frames"][5]["distance_from_gate_m"], 160);
    }

    #[test]
    fn upstream_rows_emit_query_backed_media_for_google_place_only_society() {
        let canonical = CanonicalSocietyRows {
            entities: vec![KgViewEntityRecord {
                entity_id: "society:candeur-signature".to_string(),
                entity_type: "society".to_string(),
                name: "CANDEUR SIGNATURE".to_string(),
                root_source: Some("google".to_string()),
                fact_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let upstream = SkillFactArtifactRows {
            facts: vec![test_fact(
                "society:candeur-signature",
                "google_place_id",
                FactValue::Text("ChIJxyFokqwTrjsRsnomw2M9UjQ".to_string()),
            )],
            fact_annotations: Vec::new(),
        };
        let rows = rows_from_upstream(&canonical, &upstream, Utc::now(), &MaterializationId::new())
            .expect("upstream rows should materialize");

        let media_fact = rows
            .skill_facts
            .facts
            .iter()
            .find(|fact| {
                fact.entity_id == "road_segment:candeur-signature-approach"
                    && fact.fact_key == "media.approach_road_frames"
            })
            .expect("Google place backed societies should emit query-backed Street View frames");
        let FactValue::Text(payload) =
            serde_json::from_str::<FactValue>(&media_fact.value_json).unwrap()
        else {
            panic!("media frames should be carried as JSON text");
        };
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["provider"], "Google Street View");
        assert_eq!(payload["frames"].as_array().unwrap().len(), 6);
        assert_eq!(
            payload["frames"][0]["location_query"],
            "Candeur Signature Bengaluru"
        );
        assert_eq!(payload["frames"][0]["radius_m"], 250);
        assert_eq!(payload["frames"][4]["distance_from_gate_m"], 80);
    }

    #[test]
    fn upstream_rows_emit_road_first_media_query_from_google_address() {
        let canonical = CanonicalSocietyRows {
            entities: vec![KgViewEntityRecord {
                entity_id: "society:prestige-waterford".to_string(),
                entity_type: "society".to_string(),
                name: "PRESTIGE WATERFORD".to_string(),
                root_source: Some("google".to_string()),
                fact_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let upstream = SkillFactArtifactRows {
            facts: vec![
                test_fact(
                    "society:prestige-waterford",
                    "google_place_id",
                    FactValue::Text("ChIJq0gUpUARrjsR9L-vSCm748E".to_string()),
                ),
                test_fact(
                    "society:prestige-waterford",
                    "google_place_address",
                    FactValue::Text(
                        "Prestige Waterford, ECC Road, Whitefield, Bengaluru".to_string(),
                    ),
                ),
            ],
            fact_annotations: Vec::new(),
        };
        let rows = rows_from_upstream(&canonical, &upstream, Utc::now(), &MaterializationId::new())
            .expect("upstream rows should materialize");

        let media_fact = rows
            .skill_facts
            .facts
            .iter()
            .find(|fact| {
                fact.entity_id == "road_segment:prestige-waterford-approach"
                    && fact.fact_key == "media.approach_road_frames"
            })
            .expect("address road context should emit frontage Street View frames");
        let FactValue::Text(payload) =
            serde_json::from_str::<FactValue>(&media_fact.value_json).unwrap()
        else {
            panic!("media frames should be carried as JSON text");
        };
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload["frames"][0]["location_query"],
            "ECC Road Prestige Waterford Bengaluru"
        );
        assert!(media_fact
            .input_hash
            .ends_with("street-view:ECC Road Prestige Waterford Bengaluru"));
    }

    #[test]
    fn upstream_rows_do_not_infer_frontage_road_from_review_text() {
        let canonical = CanonicalSocietyRows {
            entities: vec![KgViewEntityRecord {
                entity_id: "society:prestige-waterford".to_string(),
                entity_type: "society".to_string(),
                name: "PRESTIGE WATERFORD".to_string(),
                root_source: Some("google".to_string()),
                fact_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let upstream = SkillFactArtifactRows {
            facts: vec![
                test_fact(
                    "society:prestige-waterford",
                    "google_place_id",
                    FactValue::Text("ChIJq0gUpUARrjsR9L-vSCm748E".to_string()),
                ),
                test_fact(
                    "society:prestige-waterford",
                    "google_review_snippets",
                    FactValue::Tags(vec![
                        "The location on ECC Road is a major plus, close to Whitefield."
                            .to_string(),
                    ]),
                ),
            ],
            fact_annotations: Vec::new(),
        };
        let rows = rows_from_upstream(&canonical, &upstream, Utc::now(), &MaterializationId::new())
            .expect("upstream rows should materialize");

        let media_fact = rows
            .skill_facts
            .facts
            .iter()
            .find(|fact| {
                fact.entity_id == "road_segment:prestige-waterford-approach"
                    && fact.fact_key == "media.approach_road_frames"
            })
            .expect("Google place backed societies should still emit broad fallback frames");
        let FactValue::Text(payload) =
            serde_json::from_str::<FactValue>(&media_fact.value_json).unwrap()
        else {
            panic!("media frames should be carried as JSON text");
        };
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload["frames"][0]["location_query"],
            "Prestige Waterford Bengaluru"
        );
    }

    #[test]
    fn upstream_rows_prefer_frontage_road_over_coordinates() {
        let canonical = CanonicalSocietyRows {
            entities: vec![KgViewEntityRecord {
                entity_id: "society:frontage-test".to_string(),
                entity_type: "society".to_string(),
                name: "FRONTAGE TEST".to_string(),
                root_source: Some("rera".to_string()),
                fact_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let upstream = SkillFactArtifactRows {
            facts: vec![
                test_fact(
                    "society:frontage-test",
                    "geo.latitude",
                    FactValue::Numeric(12.9853),
                ),
                test_fact(
                    "society:frontage-test",
                    "geo.longitude",
                    FactValue::Numeric(77.7507),
                ),
                test_fact(
                    "society:frontage-test",
                    "rera_project_address",
                    FactValue::Text("Survey 1, Varthur Main Road, Bengaluru".to_string()),
                ),
            ],
            fact_annotations: Vec::new(),
        };
        let rows = rows_from_upstream(&canonical, &upstream, Utc::now(), &MaterializationId::new())
            .expect("upstream rows should materialize");

        let media_fact = rows
            .skill_facts
            .facts
            .iter()
            .find(|fact| {
                fact.entity_id == "road_segment:frontage-test-approach"
                    && fact.fact_key == "media.approach_road_frames"
            })
            .expect("frontage road context should emit Street View frames");
        let FactValue::Text(payload) =
            serde_json::from_str::<FactValue>(&media_fact.value_json).unwrap()
        else {
            panic!("media frames should be carried as JSON text");
        };
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload["frames"][0]["location_query"],
            "Varthur Main Road Frontage Test Bengaluru"
        );
        assert!(payload["frames"][0]["latitude"].is_null());
        assert!(payload["frames"][0]["longitude"].is_null());
    }

    #[test]
    fn extracts_frontage_road_names_from_address_text() {
        assert_eq!(
            extract_frontage_road_name(
                "The location on ECC Road is a major plus. It also has wide internal roads."
            )
            .as_deref(),
            Some("ECC Road")
        );
        assert_eq!(
            extract_frontage_road_name("Survey 10, Varthur Main Road, Bengaluru").as_deref(),
            Some("Varthur Main Road")
        );
        assert_eq!(
            extract_frontage_road_name("Project abuts State Highway 35 near the gate").as_deref(),
            Some("State Highway 35")
        );
    }

    fn test_fact(entity_id: &str, fact_key: &str, value: FactValue) -> SkillFactRecord {
        let value_type = match &value {
            FactValue::Numeric(_) => "numeric",
            FactValue::Tags(_) => "tags",
            _ => "text",
        };
        SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: value_type.to_string(),
            value_json: serde_json::to_string(&value).unwrap(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
            source_url: Some("https://rera.karnataka.gov.in/projectViewDetails".to_string()),
            model: None,
            skill_id: Some("fetch_rera".to_string()),
            triggered_by: None,
            learned_at: Utc::now(),
            run_id: "test".to_string(),
            input_hash: "test".to_string(),
        }
    }
}
