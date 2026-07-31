use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{DateTime, Utc};
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

pub const OSM_POWER_LINE_FACTS_ASSET_ID: &str = "osm_power_line_facts";
const OSM_POWER_SOURCE: &str = "openstreetmap_power";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmPowerInfrastructureInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<OsmPowerLineObservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmPowerLineObservationRecord {
    pub entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    pub query: String,
    pub osm_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub power: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_kv: Option<f64>,
    pub distance_meters: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_longitude: Option<f64>,
    pub latitude: f64,
    pub longitude: f64,
    pub geometry_geojson: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OsmPowerConfigFile {
    transmission_lines: TransmissionLineConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct TransmissionLineConfig {
    fact_key: String,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
    linked_place_fact_key: String,
    linked_place_display_label: String,
    #[serde(default)]
    accepted_power_values: Vec<String>,
    min_voltage_kv: f64,
    #[serde(default)]
    include_unknown_voltage: bool,
    max_distance_meters: f64,
    #[serde(default)]
    severity_bands: Vec<SeverityBand>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeverityBand {
    severity: String,
    max_distance_meters: f64,
}

pub fn osm_power_line_facts_input(
    input: &OsmPowerInfrastructureInput,
    run_id: &str,
) -> Result<SkillFactsInput, OsmPowerAssetError> {
    validate_input(input)?;
    let config = load_osm_power_config()?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::<(String, String)>::new();

    let mut rows = input
        .records
        .iter()
        .filter(|record| record_matches_transmission_policy(record, &config.transmission_lines))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then_with(|| {
                left.distance_meters
                    .partial_cmp(&right.distance_meters)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.osm_id.cmp(&right.osm_id))
    });

    for record in rows {
        let line_entity_id = format!("place:osm-power-line:{}", slug(&record.osm_id));
        push_society_transmission_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &config.transmission_lines,
            run_id,
        )?;
        push_linked_place_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &config.transmission_lines,
            &line_entity_id,
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "place.name",
            FactValue::Text(line_display_name(record)),
            "Infrastructure name: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "geo.latitude",
            FactValue::Numeric(record.latitude),
            "Latitude: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "geo.longitude",
            FactValue::Numeric(record.longitude),
            "Longitude: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "place.category",
            FactValue::Text("power_line".to_string()),
            "Infrastructure category: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "place.types",
            FactValue::Tags(vec![
                "openstreetmap".to_string(),
                "power".to_string(),
                record.power.clone(),
            ]),
            "Infrastructure types: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "geo.geometry_geojson",
            FactValue::Text(record.geometry_geojson.clone()),
            "Map geometry: {value}",
            run_id,
        )?;
        push_line_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &line_entity_id,
            "osm.power",
            FactValue::Text(record.power.clone()),
            "OSM power tag: {value}",
            run_id,
        )?;
        if let Some(voltage_kv) = record.voltage_kv {
            push_line_identity_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                record,
                &line_entity_id,
                "electrical.voltage_kv",
                FactValue::Numeric(voltage_kv),
                "Voltage: {value} kV",
                run_id,
            )?;
        }
    }

    Ok(SkillFactsInput {
        source: OSM_POWER_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: power_watermarks(input),
    })
}

pub async fn canonicalize_osm_power_infrastructure_input(
    lake: &LakeStore,
    input: &OsmPowerInfrastructureInput,
    canonical_record: &MaterializationRecord,
    source_entities: &[SourceEntitySeed],
    scope: SourceEntityResolutionScope,
) -> Result<OsmPowerInfrastructureInput, OsmPowerAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let resolver = SourceEntityResolver::new(&canonical, source_entities, scope);
    let mut resolved = input.clone();
    let mut records = Vec::with_capacity(resolved.records.len());
    for mut record in resolved.records {
        let entity_id =
            resolver.resolve(record.entity_id.as_str(), record.project_key.as_deref())?;
        record.entity_id = entity_id;
        records.push(record);
    }
    resolved.records = records;
    Ok(resolved)
}

fn push_society_transmission_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    record: &OsmPowerLineObservationRecord,
    config: &TransmissionLineConfig,
    run_id: &str,
) -> Result<(), OsmPowerAssetError> {
    let severity = severity_for_distance(record.distance_meters, config);
    let value = FactValue::Text(format!(
        "{} ({} m, {}, severity: {})",
        line_display_name(record),
        record.distance_meters.round() as i64,
        voltage_display(record),
        severity
    ));
    push_fact(
        facts,
        annotations,
        annotation_keys,
        &record.entity_id,
        &config.fact_key,
        value,
        Some(format!("{}: {{value}}", config.display_label)),
        Some(
            config
                .answers_preferences
                .iter()
                .map(String::as_str)
                .collect(),
        ),
        record,
        run_id,
    )
}

fn push_linked_place_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    record: &OsmPowerLineObservationRecord,
    config: &TransmissionLineConfig,
    line_entity_id: &str,
    run_id: &str,
) -> Result<(), OsmPowerAssetError> {
    push_fact(
        facts,
        annotations,
        annotation_keys,
        &record.entity_id,
        &config.linked_place_fact_key,
        FactValue::Text(line_entity_id.to_string()),
        Some(format!("{}: {{value}}", config.linked_place_display_label)),
        Some(vec!["map overlay", "geometry", "red flag"]),
        record,
        run_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_line_identity_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    record: &OsmPowerLineObservationRecord,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    run_id: &str,
) -> Result<(), OsmPowerAssetError> {
    push_fact(
        facts,
        annotations,
        annotation_keys,
        entity_id,
        fact_key,
        value,
        Some(display_template.to_string()),
        None,
        record,
        run_id,
    )
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
    answers_preferences: Option<Vec<&str>>,
    record: &OsmPowerLineObservationRecord,
    run_id: &str,
) -> Result<(), OsmPowerAssetError> {
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: fact_value_type(&value).to_string(),
        value_json: value_json.clone(),
        confidence: record.confidence,
        source_type: "OpenStreetMap".to_string(),
        source_url: record.source_url.clone(),
        model: None,
        skill_id: Some(OSM_POWER_LINE_FACTS_ASSET_ID.to_string()),
        triggered_by: Some(record.query.clone()),
        learned_at: record.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{entity_id}:{fact_key}:{}:{value_json}", record.osm_id).as_bytes())
        ),
    });
    if annotation_keys.insert((entity_id.to_string(), fact_key.to_string())) {
        annotations.push(SkillFactAnnotationRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template,
            answers_preferences_json: serde_json::to_string(
                &answers_preferences.unwrap_or_default(),
            )?,
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "[]".to_string(),
        });
    }
    Ok(())
}

fn validate_input(input: &OsmPowerInfrastructureInput) -> Result<(), OsmPowerAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(OsmPowerAssetError::InvalidInput(
            "OSM power snapshot date cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.osm_id.trim().is_empty()
            || record.power.trim().is_empty()
            || record.geometry_geojson.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(OsmPowerAssetError::InvalidInput(format!(
                "OSM power row for {} is missing required provenance",
                record.entity_id
            )));
        }
        if !record.distance_meters.is_finite() || record.distance_meters < 0.0 {
            return Err(OsmPowerAssetError::InvalidInput(format!(
                "OSM power row {} has invalid distance",
                record.osm_id
            )));
        }
        if record
            .voltage_kv
            .is_some_and(|voltage| !voltage.is_finite() || voltage < 0.0)
        {
            return Err(OsmPowerAssetError::InvalidInput(format!(
                "OSM power row {} has invalid voltage",
                record.osm_id
            )));
        }
        if !valid_coordinate(record.latitude, record.longitude) {
            return Err(OsmPowerAssetError::InvalidInput(format!(
                "OSM power row {} has invalid representative coordinates",
                record.osm_id
            )));
        }
        let subject = required_subject_point(
            record.subject_latitude,
            record.subject_longitude,
            &record.osm_id,
        )?;
        validate_geojson_geometry(
            &record.geometry_geojson,
            Some(subject),
            Some(record.distance_meters),
        )
        .map_err(|err| {
            OsmPowerAssetError::InvalidInput(format!(
                "OSM power row {} has invalid geometry: {err}",
                record.osm_id
            ))
        })?;
        if !record.confidence.is_finite() || !(0.0..=1.0).contains(&record.confidence) {
            return Err(OsmPowerAssetError::InvalidInput(format!(
                "OSM power row {} has invalid confidence",
                record.osm_id
            )));
        }
    }
    Ok(())
}

fn load_osm_power_config() -> Result<OsmPowerConfigFile, OsmPowerAssetError> {
    let config: OsmPowerConfigFile = load_json(&dag_root().join("osm_power_infrastructure.json"))?;
    if config.transmission_lines.fact_key.trim().is_empty()
        || config.transmission_lines.display_label.trim().is_empty()
        || config
            .transmission_lines
            .linked_place_fact_key
            .trim()
            .is_empty()
        || config
            .transmission_lines
            .linked_place_display_label
            .trim()
            .is_empty()
        || !config.transmission_lines.min_voltage_kv.is_finite()
        || !config.transmission_lines.max_distance_meters.is_finite()
    {
        return Err(OsmPowerAssetError::InvalidInput(
            "OSM power infrastructure config is missing required transmission-line policy"
                .to_string(),
        ));
    }
    Ok(config)
}

fn record_matches_transmission_policy(
    record: &OsmPowerLineObservationRecord,
    config: &TransmissionLineConfig,
) -> bool {
    if record.distance_meters > config.max_distance_meters {
        return false;
    }
    let power = record.power.trim().to_ascii_lowercase();
    if !config
        .accepted_power_values
        .iter()
        .any(|accepted| accepted.trim().eq_ignore_ascii_case(&power))
    {
        return false;
    }
    match record.voltage_kv {
        Some(voltage_kv) => voltage_kv >= config.min_voltage_kv,
        None => config.include_unknown_voltage,
    }
}

fn severity_for_distance(distance_meters: f64, config: &TransmissionLineConfig) -> &str {
    config
        .severity_bands
        .iter()
        .find(|band| distance_meters <= band.max_distance_meters)
        .map(|band| band.severity.as_str())
        .unwrap_or("info")
}

fn line_display_name(record: &OsmPowerLineObservationRecord) -> String {
    record
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::trim)
        .unwrap_or(record.osm_id.as_str())
        .to_string()
}

fn voltage_display(record: &OsmPowerLineObservationRecord) -> String {
    record
        .voltage_kv
        .map(|voltage| format!("{voltage:.0} kV"))
        .unwrap_or_else(|| "voltage unknown".to_string())
}

fn valid_coordinate(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn required_subject_point(
    latitude: Option<f64>,
    longitude: Option<f64>,
    record_id: &str,
) -> Result<(f64, f64), OsmPowerAssetError> {
    match (latitude, longitude) {
        (Some(latitude), Some(longitude)) if valid_coordinate(latitude, longitude) => {
            Ok((latitude, longitude))
        }
        _ => Err(OsmPowerAssetError::InvalidInput(format!(
            "OSM power row {record_id} requires valid subject coordinates"
        ))),
    }
}

fn power_watermarks(input: &OsmPowerInfrastructureInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: OSM_POWER_SOURCE.to_string(),
        high_watermark: input
            .records
            .iter()
            .map(|record| record.fetched_at)
            .max()
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| input.snapshot_date.clone()),
    }]
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
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
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
pub enum OsmPowerAssetError {
    Config(DagConfigError),
    InvalidInput(String),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
    Identity(SourceEntityResolutionError),
}

impl fmt::Display for OsmPowerAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "OSM power config error: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid OSM power input: {message}"),
            Self::Json(err) => write!(f, "OSM power JSON error: {err}"),
            Self::Canonical(err) => write!(f, "OSM power canonical society lookup failed: {err}"),
            Self::Identity(err) => write!(f, "OSM power source identity resolution failed: {err}"),
        }
    }
}

impl std::error::Error for OsmPowerAssetError {}

impl From<DagConfigError> for OsmPowerAssetError {
    fn from(value: DagConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<serde_json::Error> for OsmPowerAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ReraAssetError> for OsmPowerAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}

impl From<SourceEntityResolutionError> for OsmPowerAssetError {
    fn from(value: SourceEntityResolutionError) -> Self {
        Self::Identity(value)
    }
}
