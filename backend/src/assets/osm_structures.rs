use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{DateTime, Utc};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::{dag_root, load_json, DagConfigError};
use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    source_resolution::{
        SourceEntityResolutionError, SourceEntityResolutionScope, SourceEntityResolver,
    },
    MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord, SkillFactRecord,
    SkillFactsInput, SourceEntitySeed, SourceWatermark,
};

pub const OSM_SOCIETY_STRUCTURE_FACTS_ASSET_ID: &str = "osm_society_structure_facts";
const OSM_SOCIETY_STRUCTURE_SOURCE: &str = "openstreetmap_society_structures";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmSocietyStructuresInput {
    pub snapshot_date: String,
    pub collection_status: String,
    #[serde(default)]
    pub coverage: Vec<serde_json::Value>,
    #[serde(default)]
    pub records: Vec<OsmSocietyStructureRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmSocietyStructureRecord {
    pub society_entity_id: String,
    pub boundary_osm_id: String,
    pub osm_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_levels: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_meters: Option<f64>,
    pub latitude: f64,
    pub longitude: f64,
    pub geometry_geojson: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_tags: BTreeMap<String, String>,
    pub source_url: String,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
    #[serde(default)]
    pub coverage_tile_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OsmSocietyStructuresConfig {
    facts: StructureFactConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct StructureFactConfig {
    linked_entity_fact_key: String,
    observed_levels_fact_key: String,
    observed_height_fact_key: String,
}

pub fn osm_society_structure_facts_input(
    input: &OsmSocietyStructuresInput,
    run_id: &str,
) -> Result<SkillFactsInput, OsmSocietyStructuresAssetError> {
    validate_input(input)?;
    let config: OsmSocietyStructuresConfig =
        load_json(&dag_root().join("osm_society_structures.json"))?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::new();
    let mut records = input.records.iter().collect::<Vec<_>>();
    records.sort_by(|left, right| {
        left.society_entity_id
            .cmp(&right.society_entity_id)
            .then(left.osm_id.cmp(&right.osm_id))
    });

    for record in records {
        let entity_id = structure_entity_id(record);
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &record.society_entity_id,
            &config.facts.linked_entity_fact_key,
            FactValue::Text(entity_id.clone()),
            record,
            run_id,
        )?;
        let observed_name = record
            .name
            .as_deref()
            .or(record.r#ref.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(name) = observed_name {
            for fact_key in ["place.name", "structure.observed_name"] {
                push_fact(
                    &mut facts,
                    &mut annotations,
                    &mut annotation_keys,
                    &entity_id,
                    fact_key,
                    FactValue::Text(name.to_string()),
                    record,
                    run_id,
                )?;
            }
        } else {
            // Spatial support entities require a name. This neutral label stays
            // internal; the arrival tour only selects explicitly named groups.
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                "place.name",
                FactValue::Text("Mapped building".to_string()),
                record,
                run_id,
            )?;
        }
        for (fact_key, value) in [
            (
                "place.category",
                FactValue::Text("society_structure".to_string()),
            ),
            (
                "place.types",
                FactValue::Tags(vec![
                    "building".to_string(),
                    "society_structure".to_string(),
                ]),
            ),
            ("geo.latitude", FactValue::Numeric(record.latitude)),
            ("geo.longitude", FactValue::Numeric(record.longitude)),
            (
                "geo.geometry_geojson",
                FactValue::Text(record.geometry_geojson.clone()),
            ),
        ] {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                fact_key,
                value,
                record,
                run_id,
            )?;
        }
        if let Some(levels) = record.building_levels {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                &config.facts.observed_levels_fact_key,
                FactValue::Numeric(levels),
                record,
                run_id,
            )?;
        }
        if let Some(height) = record.height_meters {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                &config.facts.observed_height_fact_key,
                FactValue::Numeric(height),
                record,
                run_id,
            )?;
        }
    }
    facts.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
            .then(
                left.provider_observation_id
                    .cmp(&right.provider_observation_id),
            )
    });
    annotations.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
    });
    Ok(SkillFactsInput {
        source: OSM_SOCIETY_STRUCTURE_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: input.source_watermarks.clone(),
    })
}

pub async fn canonicalize_osm_society_structures_input(
    lake: &LakeStore,
    input: &OsmSocietyStructuresInput,
    canonical_record: &MaterializationRecord,
    source_entities: &[SourceEntitySeed],
    scope: SourceEntityResolutionScope,
) -> Result<OsmSocietyStructuresInput, OsmSocietyStructuresAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let resolver = SourceEntityResolver::new(&canonical, source_entities, scope);
    let mut resolved = input.clone();
    for record in &mut resolved.records {
        record.society_entity_id = resolver.resolve(&record.society_entity_id, None)?;
    }
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    record: &OsmSocietyStructureRecord,
    run_id: &str,
) -> Result<(), OsmSocietyStructuresAssetError> {
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: fact_value_type(&value).to_string(),
        value_json: value_json.clone(),
        confidence: record.confidence,
        source_type: "OpenStreetMap".to_string(),
        source_url: Some(record.source_url.clone()),
        model: None,
        skill_id: Some(OSM_SOCIETY_STRUCTURE_FACTS_ASSET_ID.to_string()),
        triggered_by: Some(format!("inside {}", record.boundary_osm_id)),
        learned_at: record.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{entity_id}:{fact_key}:{}:{value_json}", record.osm_id).as_bytes())
        ),
        observation_provider: Some("OpenStreetMap".to_string()),
        provider_observation_id: Some(record.osm_id.clone()),
        asset_lineage: vec![format!(
            "asset:{OSM_SOCIETY_STRUCTURE_FACTS_ASSET_ID}/run:{run_id}"
        )],
    });
    if annotation_keys.insert((entity_id.to_string(), fact_key.to_string())) {
        annotations.push(SkillFactAnnotationRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: None,
            answers_preferences_json: "[]".to_string(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "[]".to_string(),
        });
    }
    Ok(())
}

fn validate_input(input: &OsmSocietyStructuresInput) -> Result<(), OsmSocietyStructuresAssetError> {
    if input.snapshot_date.trim().is_empty() || input.collection_status != "complete" {
        return Err(OsmSocietyStructuresAssetError::InvalidInput(
            "structure snapshot must have complete polygon coverage".to_string(),
        ));
    }
    for record in &input.records {
        if record.society_entity_id.trim().is_empty()
            || record.boundary_osm_id.trim().is_empty()
            || record.osm_id.trim().is_empty()
            || record.geometry_geojson.trim().is_empty()
            || record.source_url.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(OsmSocietyStructuresAssetError::InvalidInput(format!(
                "structure {} is missing identity, geometry, or provenance",
                record.osm_id
            )));
        }
        if !valid_coordinate(record.latitude, record.longitude)
            || !record.confidence.is_finite()
            || !(0.0..=1.0).contains(&record.confidence)
            || record
                .building_levels
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || record
                .height_meters
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(OsmSocietyStructuresAssetError::InvalidInput(format!(
                "structure {} has invalid numeric values",
                record.osm_id
            )));
        }
        let parsed = record
            .geometry_geojson
            .parse::<GeoJson>()
            .map_err(|error| {
                OsmSocietyStructuresAssetError::InvalidInput(format!(
                    "structure {} has invalid GeoJSON: {error}",
                    record.osm_id
                ))
            })?;
        let polygon = match parsed {
            GeoJson::Geometry(geometry) => {
                matches!(
                    geometry.value,
                    GeoJsonValue::Polygon(_) | GeoJsonValue::MultiPolygon(_)
                )
            }
            _ => false,
        };
        if !polygon {
            return Err(OsmSocietyStructuresAssetError::InvalidInput(format!(
                "structure {} geometry must be a polygon",
                record.osm_id
            )));
        }
    }
    Ok(())
}

fn structure_entity_id(record: &OsmSocietyStructureRecord) -> String {
    format!(
        "place:osm-structure:{}-{}",
        slug(&record.society_entity_id),
        slug(&record.osm_id)
    )
}

fn valid_coordinate(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
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

fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
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
pub enum OsmSocietyStructuresAssetError {
    Config(DagConfigError),
    InvalidInput(String),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
    Identity(SourceEntityResolutionError),
}

impl fmt::Display for OsmSocietyStructuresAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "OSM structure config error: {error}"),
            Self::InvalidInput(message) => {
                write!(formatter, "invalid OSM structure input: {message}")
            }
            Self::Json(error) => write!(formatter, "OSM structure JSON error: {error}"),
            Self::Canonical(error) => {
                write!(formatter, "OSM structure canonical lookup failed: {error}")
            }
            Self::Identity(error) => write!(
                formatter,
                "OSM structure identity resolution failed: {error}"
            ),
        }
    }
}

impl std::error::Error for OsmSocietyStructuresAssetError {}
impl From<DagConfigError> for OsmSocietyStructuresAssetError {
    fn from(value: DagConfigError) -> Self {
        Self::Config(value)
    }
}
impl From<serde_json::Error> for OsmSocietyStructuresAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
impl From<ReraAssetError> for OsmSocietyStructuresAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}
impl From<SourceEntityResolutionError> for OsmSocietyStructuresAssetError {
    fn from(value: SourceEntityResolutionError) -> Self {
        Self::Identity(value)
    }
}
