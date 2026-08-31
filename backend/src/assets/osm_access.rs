use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::{dag_root, load_json, DagConfigError};
use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    geometry::validate_geojson_geometry,
    source_resolution::{
        SourceEntityResolutionError, SourceEntityResolutionScope, SourceEntityResolver,
    },
    MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord, SkillFactRecord,
    SkillFactsInput, SourceEntitySeed, SourceWatermark,
};

pub const OSM_SOCIETY_ACCESS_FACTS_ASSET_ID: &str = "osm_society_access_facts";
const OSM_SOCIETY_ACCESS_SOURCE: &str = "openstreetmap_society_access";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmSocietyAccessInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<OsmSocietyAccessRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmSocietyAccessRecord {
    pub entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    pub query: String,
    pub access_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_road_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_way_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_distance_meters: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_geometry_geojson: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_source_geometry_geojson: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach_association_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_way_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_geometry_geojson: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_longitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_association_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_source_url: Option<String>,
    pub subject_latitude: f64,
    pub subject_longitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OsmAccessConfigFile {
    approach_road: ApproachRoadConfig,
    entrance: EntranceConfig,
    society_boundary: SocietyBoundaryConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct ApproachRoadConfig {
    summary_fact_key: String,
    linked_entity_fact_key: String,
    direction_fact_key: String,
    association_fact_key: String,
    source_geometry_fact_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EntranceConfig {
    linked_entity_fact_key: String,
    status_fact_key: String,
    association_fact_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SocietyBoundaryConfig {
    geometry_fact_key: String,
}

pub fn osm_society_access_facts_input(
    input: &OsmSocietyAccessInput,
    run_id: &str,
) -> Result<SkillFactsInput, OsmAccessAssetError> {
    validate_input(input)?;
    let config = load_config()?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::new();
    let mut records = input.records.iter().collect::<Vec<_>>();
    records.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

    for record in records {
        push_approach_road_facts(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &config.approach_road,
            record,
            run_id,
        )?;
        push_entrance_facts(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &config.entrance,
            record,
            run_id,
        )?;
        push_society_boundary_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &config.society_boundary,
            record,
            run_id,
        )?;
    }

    facts.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
    });
    annotations.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
    });
    Ok(SkillFactsInput {
        source: OSM_SOCIETY_ACCESS_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: input.source_watermarks.clone(),
    })
}

fn push_society_boundary_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    config: &SocietyBoundaryConfig,
    record: &OsmSocietyAccessRecord,
    run_id: &str,
) -> Result<(), OsmAccessAssetError> {
    let Some(geometry) = record
        .boundary_geometry_geojson
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let source_url = record
        .boundary_way_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|way_id| format!("https://www.openstreetmap.org/way/{way_id}"));
    push_fact_with_source(
        facts,
        annotations,
        annotation_keys,
        &record.entity_id,
        &config.geometry_fact_key,
        FactValue::Text(geometry.to_string()),
        None,
        &[],
        record,
        run_id,
        source_url,
    )
}

fn push_approach_road_facts(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    config: &ApproachRoadConfig,
    record: &OsmSocietyAccessRecord,
    run_id: &str,
) -> Result<(), OsmAccessAssetError> {
    let Some(name) = record
        .approach_road_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let Some(geometry) = record
        .approach_geometry_geojson
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let road_key = record
        .approach_way_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&record.access_id);
    let road_entity_id = format!(
        "place:approach-road:{}-{}",
        slug(&record.entity_id),
        slug(road_key),
    );

    for (entity_id, fact_key, value, template, preferences) in [
        (
            record.entity_id.as_str(),
            config.summary_fact_key.as_str(),
            FactValue::Text(name.to_string()),
            "Approach road: {value}",
            &["approach road", "road outside the gate"][..],
        ),
        (
            record.entity_id.as_str(),
            config.linked_entity_fact_key.as_str(),
            FactValue::Text(road_entity_id.clone()),
            "Approach road map entity: {value}",
            &["approach road map", "road geometry"][..],
        ),
        (
            road_entity_id.as_str(),
            "place.name",
            FactValue::Text(name.to_string()),
            "Approach road: {value}",
            &[][..],
        ),
        (
            road_entity_id.as_str(),
            "place.category",
            FactValue::Text("public_approach_corridor".to_string()),
            "Road category: {value}",
            &[][..],
        ),
        (
            road_entity_id.as_str(),
            "place.types",
            FactValue::Tags(vec![
                "road".to_string(),
                "public_approach_corridor".to_string(),
            ]),
            "Road types: {value}",
            &[][..],
        ),
        (
            road_entity_id.as_str(),
            "geo.latitude",
            FactValue::Numeric(record.subject_latitude),
            "Latitude: {value}",
            &[][..],
        ),
        (
            road_entity_id.as_str(),
            "geo.longitude",
            FactValue::Numeric(record.subject_longitude),
            "Longitude: {value}",
            &[][..],
        ),
        (
            road_entity_id.as_str(),
            "geo.geometry_geojson",
            FactValue::Text(geometry.to_string()),
            "Map geometry: {value}",
            &[][..],
        ),
    ] {
        push_fact(
            facts,
            annotations,
            annotation_keys,
            entity_id,
            fact_key,
            value,
            Some(template.to_string()),
            preferences,
            record,
            run_id,
        )?;
    }
    for (fact_key, value, template) in [
        (
            config.direction_fact_key.as_str(),
            record.approach_direction.as_deref(),
            "Road direction: {value}",
        ),
        (
            config.association_fact_key.as_str(),
            record.approach_association_method.as_deref(),
            "Road association: {value}",
        ),
        (
            config.source_geometry_fact_key.as_str(),
            record.approach_source_geometry_geojson.as_deref(),
            "Source geometry: {value}",
        ),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            push_fact(
                facts,
                annotations,
                annotation_keys,
                &road_entity_id,
                fact_key,
                FactValue::Text(value.to_string()),
                Some(template.to_string()),
                &[],
                record,
                run_id,
            )?;
        }
    }
    if let Some(distance) = record.approach_distance_meters {
        push_fact(
            facts,
            annotations,
            annotation_keys,
            &road_entity_id,
            "route.distance_m",
            FactValue::Numeric(distance),
            Some("Corridor distance: {value} m".to_string()),
            &[],
            record,
            run_id,
        )?;
    }
    Ok(())
}

fn push_entrance_facts(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    config: &EntranceConfig,
    record: &OsmSocietyAccessRecord,
    run_id: &str,
) -> Result<(), OsmAccessAssetError> {
    let (Some(entrance_id), Some(latitude), Some(longitude), Some(status), Some(method)) = (
        record.entrance_id.as_deref(),
        record.entrance_latitude,
        record.entrance_longitude,
        record.entrance_status.as_deref(),
        record.entrance_association_method.as_deref(),
    ) else {
        return Ok(());
    };
    let entity_id = format!(
        "place:society-entrance:{}-{}",
        slug(&record.entity_id),
        slug(entrance_id)
    );
    push_fact_with_source(
        facts,
        annotations,
        annotation_keys,
        &record.entity_id,
        &config.linked_entity_fact_key,
        FactValue::Text(entity_id.clone()),
        Some("Entrance: {value}".to_string()),
        &["entrance", "main gate"],
        record,
        run_id,
        record.entrance_source_url.clone(),
    )?;
    for (fact_key, value, template) in [
        (
            "place.name",
            FactValue::Text("Entrance".to_string()),
            "Entrance: {value}",
        ),
        (
            "place.category",
            FactValue::Text("society_entrance".to_string()),
            "Place category: {value}",
        ),
        (
            "place.types",
            FactValue::Tags(vec!["entrance".to_string(), "gate".to_string()]),
            "Place types: {value}",
        ),
        (
            "geo.latitude",
            FactValue::Numeric(latitude),
            "Latitude: {value}",
        ),
        (
            "geo.longitude",
            FactValue::Numeric(longitude),
            "Longitude: {value}",
        ),
        (
            config.status_fact_key.as_str(),
            FactValue::Text(status.to_string()),
            "Entrance status: {value}",
        ),
        (
            config.association_fact_key.as_str(),
            FactValue::Text(method.to_string()),
            "Entrance association: {value}",
        ),
    ] {
        push_fact_with_source(
            facts,
            annotations,
            annotation_keys,
            &entity_id,
            fact_key,
            value,
            Some(template.to_string()),
            &[],
            record,
            run_id,
            record.entrance_source_url.clone(),
        )?;
    }
    Ok(())
}

pub async fn canonicalize_osm_society_access_input(
    lake: &LakeStore,
    input: &OsmSocietyAccessInput,
    canonical_record: &MaterializationRecord,
    source_entities: &[SourceEntitySeed],
    scope: SourceEntityResolutionScope,
) -> Result<OsmSocietyAccessInput, OsmAccessAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let resolver = SourceEntityResolver::new(&canonical, source_entities, scope);
    let mut resolved = input.clone();
    for record in &mut resolved.records {
        record.entity_id = resolver.resolve(&record.entity_id, record.project_key.as_deref())?;
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
    display_template: Option<String>,
    answers_preferences: &[&str],
    record: &OsmSocietyAccessRecord,
    run_id: &str,
) -> Result<(), OsmAccessAssetError> {
    push_fact_with_source(
        facts,
        annotations,
        annotation_keys,
        entity_id,
        fact_key,
        value,
        display_template,
        answers_preferences,
        record,
        run_id,
        record.source_url.clone(),
    )
}

#[allow(clippy::too_many_arguments)]
fn push_fact_with_source(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    display_template: Option<String>,
    answers_preferences: &[&str],
    record: &OsmSocietyAccessRecord,
    run_id: &str,
    source_url: Option<String>,
) -> Result<(), OsmAccessAssetError> {
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: fact_value_type(&value).to_string(),
        value_json: value_json.clone(),
        confidence: record.confidence,
        source_type: "OpenStreetMap".to_string(),
        source_url,
        model: None,
        skill_id: Some(OSM_SOCIETY_ACCESS_FACTS_ASSET_ID.to_string()),
        triggered_by: Some(record.query.clone()),
        learned_at: record.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(
                format!("{entity_id}:{fact_key}:{}:{value_json}", record.access_id).as_bytes()
            )
        ),
    });
    if annotation_keys.insert((entity_id.to_string(), fact_key.to_string())) {
        annotations.push(SkillFactAnnotationRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template,
            answers_preferences_json: serde_json::to_string(answers_preferences)?,
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "[]".to_string(),
        });
    }
    Ok(())
}

fn validate_input(input: &OsmSocietyAccessInput) -> Result<(), OsmAccessAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(OsmAccessAssetError::InvalidInput(
            "OSM access corridor snapshot date cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.access_id.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(OsmAccessAssetError::InvalidInput(
                "OSM access corridor row is missing required provenance".to_string(),
            ));
        }
        for value in [record.subject_latitude, record.subject_longitude] {
            if !value.is_finite() {
                return Err(OsmAccessAssetError::InvalidInput(
                    "OSM access corridor row contains a non-finite value".to_string(),
                ));
            }
        }
        if !(0.0..=1.0).contains(&record.confidence) {
            return Err(OsmAccessAssetError::InvalidInput(
                "OSM access corridor row contains an invalid distance or confidence".to_string(),
            ));
        }
        if let Some(geometry) = record.approach_geometry_geojson.as_deref() {
            validate_geojson_geometry(geometry, None, None).map_err(|error| {
                OsmAccessAssetError::InvalidInput(format!(
                    "OSM approach road {} has invalid geometry: {error}",
                    record.access_id
                ))
            })?;
            if !is_route_line_geometry(geometry) {
                return Err(OsmAccessAssetError::InvalidInput(format!(
                    "OSM approach road {} must contain a LineString with at least two points",
                    record.access_id
                )));
            }
        }
        if let Some(geometry) = record.approach_source_geometry_geojson.as_deref() {
            if !is_route_line_geometry(geometry) {
                return Err(OsmAccessAssetError::InvalidInput(format!(
                    "OSM source road {} must contain a LineString",
                    record.access_id
                )));
            }
        }
        if let Some(boundary_geometry) = record.boundary_geometry_geojson.as_deref() {
            if !is_polygon_geometry(boundary_geometry) {
                return Err(OsmAccessAssetError::InvalidInput(format!(
                    "OSM society boundary {} must contain a valid closed Polygon",
                    record.access_id
                )));
            }
        }
        if record
            .approach_distance_meters
            .is_some_and(|distance| !distance.is_finite() || distance <= 0.0)
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM approach road {} has invalid length",
                record.access_id
            )));
        }
        let road_fields = [
            record.approach_road_name.is_some(),
            record.approach_way_id.is_some(),
            record.approach_distance_meters.is_some(),
            record.approach_geometry_geojson.is_some(),
            record.approach_direction.is_some(),
            record.approach_association_method.is_some(),
        ];
        if road_fields.iter().any(|present| *present) && !road_fields.iter().all(|present| *present)
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM approach road {} is incomplete",
                record.access_id
            )));
        }
        if record
            .approach_direction
            .as_deref()
            .is_some_and(|direction| {
                !matches!(direction, "two_way" | "oneway_forward" | "oneway_reverse")
            })
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM approach road {} has an invalid direction",
                record.access_id
            )));
        }
        let entrance_fields = [
            record.entrance_id.is_some(),
            record.entrance_latitude.is_some(),
            record.entrance_longitude.is_some(),
            record.entrance_status.is_some(),
            record.entrance_association_method.is_some(),
            record.entrance_source_url.is_some(),
        ];
        if entrance_fields.iter().any(|present| *present)
            && !entrance_fields.iter().all(|present| *present)
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM entrance {} is incomplete",
                record.access_id
            )));
        }
        if record
            .entrance_status
            .as_deref()
            .is_some_and(|status| !matches!(status, "verified" | "inferred"))
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM entrance {} has an invalid status",
                record.access_id
            )));
        }
        if [record.entrance_latitude, record.entrance_longitude]
            .into_iter()
            .flatten()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(OsmAccessAssetError::InvalidInput(format!(
                "OSM entrance {} has invalid coordinates",
                record.access_id
            )));
        }
    }
    Ok(())
}

fn is_route_line_geometry(value: &str) -> bool {
    let Ok(parsed) = value.parse::<GeoJson>() else {
        return false;
    };
    let geometry = match &parsed {
        GeoJson::Geometry(geometry) => Some(&geometry.value),
        GeoJson::Feature(feature) => feature.geometry.as_ref().map(|geometry| &geometry.value),
        GeoJson::FeatureCollection(_) => None,
    };
    matches!(geometry, Some(GeoJsonValue::LineString(points)) if points.len() >= 2)
}

fn is_polygon_geometry(value: &str) -> bool {
    let Ok(parsed) = value.parse::<GeoJson>() else {
        return false;
    };
    let geometry = match &parsed {
        GeoJson::Geometry(geometry) => Some(&geometry.value),
        GeoJson::Feature(feature) => feature.geometry.as_ref().map(|geometry| &geometry.value),
        GeoJson::FeatureCollection(_) => None,
    };
    let Some(GeoJsonValue::Polygon(rings)) = geometry else {
        return false;
    };
    !rings.is_empty()
        && rings.iter().all(|ring| {
            ring.len() >= 4
                && ring.first() == ring.last()
                && ring.iter().all(|point| {
                    point.len() >= 2
                        && point[0].is_finite()
                        && point[1].is_finite()
                        && (-180.0..=180.0).contains(&point[0])
                        && (-90.0..=90.0).contains(&point[1])
                })
        })
}

fn load_config() -> Result<OsmAccessConfigFile, OsmAccessAssetError> {
    let config: OsmAccessConfigFile = load_json(&dag_root().join("osm_access_corridors.json"))?;
    if config.approach_road.summary_fact_key.trim().is_empty()
        || config
            .approach_road
            .linked_entity_fact_key
            .trim()
            .is_empty()
        || config.approach_road.direction_fact_key.trim().is_empty()
        || config.approach_road.association_fact_key.trim().is_empty()
        || config
            .approach_road
            .source_geometry_fact_key
            .trim()
            .is_empty()
        || config.entrance.linked_entity_fact_key.trim().is_empty()
        || config.entrance.status_fact_key.trim().is_empty()
        || config.entrance.association_fact_key.trim().is_empty()
        || config.society_boundary.geometry_fact_key.trim().is_empty()
    {
        return Err(OsmAccessAssetError::InvalidInput(
            "OSM access corridor config is incomplete".to_string(),
        ));
    }
    Ok(config)
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
    let slug = value
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
        .join("-");
    if slug.is_empty() {
        sha256_hex(value.as_bytes()).chars().take(12).collect()
    } else {
        slug
    }
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
pub enum OsmAccessAssetError {
    Config(DagConfigError),
    InvalidInput(String),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
    Identity(SourceEntityResolutionError),
}

impl fmt::Display for OsmAccessAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(formatter, "OSM access config error: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid OSM access input: {message}"),
            Self::Json(error) => write!(formatter, "OSM access JSON error: {error}"),
            Self::Canonical(error) => {
                write!(formatter, "OSM access canonical lookup failed: {error}")
            }
            Self::Identity(error) => {
                write!(formatter, "OSM access identity resolution failed: {error}")
            }
        }
    }
}

impl std::error::Error for OsmAccessAssetError {}

impl From<DagConfigError> for OsmAccessAssetError {
    fn from(value: DagConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<serde_json::Error> for OsmAccessAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ReraAssetError> for OsmAccessAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}

impl From<SourceEntityResolutionError> for OsmAccessAssetError {
    fn from(value: SourceEntityResolutionError) -> Self {
        Self::Identity(value)
    }
}
