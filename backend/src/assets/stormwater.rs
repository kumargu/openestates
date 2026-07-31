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

pub const STORMWATER_DRAIN_FACTS_ASSET_ID: &str = "stormwater_drain_facts";
const STORMWATER_SOURCE: &str = "stormwater_drain";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StormwaterDrainRiskInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<StormwaterDrainObservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StormwaterDrainObservationRecord {
    pub entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    pub query: String,
    pub drain_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub drain_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<String>,
    pub distance_meters: f64,
    #[serde(default)]
    pub intersects_property: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_latitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_longitude: Option<f64>,
    pub latitude: f64,
    pub longitude: f64,
    pub geometry_geojson: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encroachment_record: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StormwaterDrainRiskConfigFile {
    drains: StormwaterDrainPolicy,
}

#[derive(Debug, Clone, Deserialize)]
struct StormwaterDrainPolicy {
    fact_key: String,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
    linked_place_fact_key: String,
    linked_place_display_label: String,
    #[serde(default)]
    accepted_drain_types: Vec<String>,
    max_distance_meters: f64,
    #[serde(default)]
    severity_bands: Vec<SeverityBand>,
    #[serde(default)]
    type_specific_facts: Vec<TypeSpecificFact>,
    #[serde(default)]
    encroachment_fact: Option<EncroachmentFact>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeverityBand {
    severity: String,
    max_distance_meters: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct TypeSpecificFact {
    fact_key: String,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
    #[serde(default)]
    drain_type_aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EncroachmentFact {
    fact_key: String,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
}

pub fn stormwater_drain_facts_input(
    input: &StormwaterDrainRiskInput,
    run_id: &str,
) -> Result<SkillFactsInput, StormwaterAssetError> {
    validate_input(input)?;
    let config = load_stormwater_config()?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::<(String, String)>::new();

    let mut rows = input
        .records
        .iter()
        .filter(|record| record_matches_policy(record, &config.drains))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then_with(|| {
                effective_distance(*left)
                    .partial_cmp(&effective_distance(*right))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.drain_id.cmp(&right.drain_id))
    });

    for record in rows {
        let drain_entity_id = format!("place:stormwater-drain:{}", slug(&record.drain_id));
        push_society_drain_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &record.entity_id,
            &config.drains.fact_key,
            &config.drains.display_label,
            &config.drains.answers_preferences,
            drain_display(record, &config.drains),
            record,
            run_id,
        )?;
        push_linked_place_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &config.drains,
            &drain_entity_id,
            run_id,
        )?;
        for type_fact in &config.drains.type_specific_facts {
            if type_fact_matches(type_fact, record) {
                push_society_drain_fact(
                    &mut facts,
                    &mut annotations,
                    &mut annotation_keys,
                    &record.entity_id,
                    &type_fact.fact_key,
                    &type_fact.display_label,
                    &type_fact.answers_preferences,
                    drain_display(record, &config.drains),
                    record,
                    run_id,
                )?;
            }
        }
        if let (Some(encroachment_fact), Some(encroachment_record)) = (
            config.drains.encroachment_fact.as_ref(),
            record
                .encroachment_record
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        ) {
            push_society_drain_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &record.entity_id,
                &encroachment_fact.fact_key,
                &encroachment_fact.display_label,
                &encroachment_fact.answers_preferences,
                format!(
                    "{} ({})",
                    drain_display_name(record),
                    encroachment_record.trim()
                ),
                record,
                run_id,
            )?;
        }

        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "place.name",
            FactValue::Text(drain_display_name(record)),
            "Drain name: {value}",
            run_id,
        )?;
        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "geo.latitude",
            FactValue::Numeric(record.latitude),
            "Latitude: {value}",
            run_id,
        )?;
        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "geo.longitude",
            FactValue::Numeric(record.longitude),
            "Longitude: {value}",
            run_id,
        )?;
        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "place.category",
            FactValue::Text("stormwater_drain".to_string()),
            "Drain category: {value}",
            run_id,
        )?;
        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "place.types",
            FactValue::Tags(drain_place_types(record)),
            "Drain types: {value}",
            run_id,
        )?;
        push_drain_identity_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            record,
            &drain_entity_id,
            "geo.geometry_geojson",
            FactValue::Text(record.geometry_geojson.clone()),
            "Map geometry: {value}",
            run_id,
        )?;
    }

    Ok(SkillFactsInput {
        source: STORMWATER_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: stormwater_watermarks(input),
    })
}

pub async fn canonicalize_stormwater_drain_input(
    lake: &LakeStore,
    input: &StormwaterDrainRiskInput,
    canonical_record: &MaterializationRecord,
    source_entities: &[SourceEntitySeed],
    scope: SourceEntityResolutionScope,
) -> Result<StormwaterDrainRiskInput, StormwaterAssetError> {
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

#[allow(clippy::too_many_arguments)]
fn push_society_drain_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    entity_id: &str,
    fact_key: &str,
    display_label: &str,
    answers_preferences: &[String],
    display: String,
    record: &StormwaterDrainObservationRecord,
    run_id: &str,
) -> Result<(), StormwaterAssetError> {
    push_fact(
        facts,
        annotations,
        annotation_keys,
        entity_id,
        fact_key,
        FactValue::Text(display),
        Some(format!("{display_label}: {{value}}")),
        Some(answers_preferences.iter().map(String::as_str).collect()),
        record,
        run_id,
    )
}

fn push_linked_place_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    record: &StormwaterDrainObservationRecord,
    policy: &StormwaterDrainPolicy,
    drain_entity_id: &str,
    run_id: &str,
) -> Result<(), StormwaterAssetError> {
    push_fact(
        facts,
        annotations,
        annotation_keys,
        &record.entity_id,
        &policy.linked_place_fact_key,
        FactValue::Text(drain_entity_id.to_string()),
        Some(format!("{}: {{value}}", policy.linked_place_display_label)),
        Some(vec!["map overlay", "geometry", "red flag"]),
        record,
        run_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_drain_identity_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    record: &StormwaterDrainObservationRecord,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    run_id: &str,
) -> Result<(), StormwaterAssetError> {
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
    record: &StormwaterDrainObservationRecord,
    run_id: &str,
) -> Result<(), StormwaterAssetError> {
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: fact_value_type(&value).to_string(),
        value_json: value_json.clone(),
        confidence: record.confidence,
        source_type: record
            .source_type
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(STORMWATER_SOURCE)
            .to_string(),
        source_url: record.source_url.clone(),
        model: None,
        skill_id: Some(STORMWATER_DRAIN_FACTS_ASSET_ID.to_string()),
        triggered_by: Some(record.query.clone()),
        learned_at: record.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(
                format!("{entity_id}:{fact_key}:{}:{value_json}", record.drain_id).as_bytes()
            )
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

fn validate_input(input: &StormwaterDrainRiskInput) -> Result<(), StormwaterAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(StormwaterAssetError::InvalidInput(
            "stormwater drain snapshot date cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.drain_id.trim().is_empty()
            || record.drain_type.trim().is_empty()
            || record.geometry_geojson.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(StormwaterAssetError::InvalidInput(format!(
                "stormwater drain row for {} is missing required provenance",
                record.entity_id
            )));
        }
        if !record.distance_meters.is_finite() || record.distance_meters < 0.0 {
            return Err(StormwaterAssetError::InvalidInput(format!(
                "stormwater drain row {} has invalid distance",
                record.drain_id
            )));
        }
        if !valid_coordinate(record.latitude, record.longitude) {
            return Err(StormwaterAssetError::InvalidInput(format!(
                "stormwater drain row {} has invalid representative coordinates",
                record.drain_id
            )));
        }
        let subject = required_subject_point(
            record.subject_latitude,
            record.subject_longitude,
            &record.drain_id,
        )?;
        validate_geojson_geometry(
            &record.geometry_geojson,
            Some(subject),
            Some(record.distance_meters),
        )
        .map_err(|err| {
            StormwaterAssetError::InvalidInput(format!(
                "stormwater drain row {} has invalid geometry: {err}",
                record.drain_id
            ))
        })?;
        if !record.confidence.is_finite() || !(0.0..=1.0).contains(&record.confidence) {
            return Err(StormwaterAssetError::InvalidInput(format!(
                "stormwater drain row {} has invalid confidence",
                record.drain_id
            )));
        }
    }
    Ok(())
}

fn load_stormwater_config() -> Result<StormwaterDrainRiskConfigFile, StormwaterAssetError> {
    let config: StormwaterDrainRiskConfigFile =
        load_json(&dag_root().join("stormwater_drain_risk.json"))?;
    validate_stormwater_config(&config)?;
    Ok(config)
}

fn validate_stormwater_config(
    config: &StormwaterDrainRiskConfigFile,
) -> Result<(), StormwaterAssetError> {
    let policy = &config.drains;
    if policy.fact_key.trim().is_empty()
        || policy.display_label.trim().is_empty()
        || policy.linked_place_fact_key.trim().is_empty()
        || policy.linked_place_display_label.trim().is_empty()
        || policy.accepted_drain_types.is_empty()
        || !policy.max_distance_meters.is_finite()
        || policy.max_distance_meters < 0.0
    {
        return Err(StormwaterAssetError::InvalidInput(
            "stormwater drain config is missing required policy".to_string(),
        ));
    }
    for type_fact in &policy.type_specific_facts {
        if type_fact.fact_key.trim().is_empty()
            || type_fact.display_label.trim().is_empty()
            || type_fact.drain_type_aliases.is_empty()
        {
            return Err(StormwaterAssetError::InvalidInput(
                "stormwater drain config has invalid type-specific fact".to_string(),
            ));
        }
    }
    if let Some(encroachment_fact) = &policy.encroachment_fact {
        if encroachment_fact.fact_key.trim().is_empty()
            || encroachment_fact.display_label.trim().is_empty()
        {
            return Err(StormwaterAssetError::InvalidInput(
                "stormwater drain config has invalid encroachment fact".to_string(),
            ));
        }
    }
    Ok(())
}

fn record_matches_policy(
    record: &StormwaterDrainObservationRecord,
    policy: &StormwaterDrainPolicy,
) -> bool {
    effective_distance(record) <= policy.max_distance_meters
        && policy
            .accepted_drain_types
            .iter()
            .any(|accepted| normalize_type(accepted) == normalize_type(&record.drain_type))
}

fn type_fact_matches(
    type_fact: &TypeSpecificFact,
    record: &StormwaterDrainObservationRecord,
) -> bool {
    let haystack = drain_match_tokens(record);
    type_fact.drain_type_aliases.iter().any(|alias| {
        let alias = normalize_type(alias);
        haystack.iter().any(|token| token == &alias)
    })
}

fn drain_display(
    record: &StormwaterDrainObservationRecord,
    policy: &StormwaterDrainPolicy,
) -> String {
    let severity = severity_for_distance(effective_distance(record), policy);
    let distance = if record.intersects_property {
        "intersects property".to_string()
    } else {
        format!("{} m", record.distance_meters.round() as i64)
    };
    let hierarchy = record
        .hierarchy
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!(", {}", value.trim()))
        .unwrap_or_default();
    format!(
        "{} ({distance}, {}{}, severity: {severity})",
        drain_display_name(record),
        record.drain_type,
        hierarchy
    )
}

fn drain_display_name(record: &StormwaterDrainObservationRecord) -> String {
    record
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::trim)
        .unwrap_or(record.drain_id.as_str())
        .to_string()
}

fn drain_place_types(record: &StormwaterDrainObservationRecord) -> Vec<String> {
    let mut types = vec!["stormwater_drain".to_string(), record.drain_type.clone()];
    if let Some(hierarchy) = record
        .hierarchy
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        types.push(hierarchy.trim().to_string());
    }
    types
}

fn effective_distance(record: &StormwaterDrainObservationRecord) -> f64 {
    if record.intersects_property {
        0.0
    } else {
        record.distance_meters
    }
}

fn severity_for_distance(distance_meters: f64, policy: &StormwaterDrainPolicy) -> &str {
    policy
        .severity_bands
        .iter()
        .find(|band| distance_meters <= band.max_distance_meters)
        .map(|band| band.severity.as_str())
        .unwrap_or("info")
}

fn normalize_type(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn drain_match_tokens(record: &StormwaterDrainObservationRecord) -> Vec<String> {
    let mut tokens = vec![normalize_type(&record.drain_type)];
    if let Some(hierarchy) = record
        .hierarchy
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        tokens.push(normalize_type(hierarchy));
    }
    if let Some(name) = record
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        tokens.extend(
            normalize_type(name)
                .split('_')
                .filter(|part| !part.is_empty())
                .map(ToString::to_string),
        );
    }
    tokens.extend(record.source_tags.values().flat_map(|value| {
        normalize_type(value)
            .split('_')
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    }));
    tokens.sort();
    tokens.dedup();
    tokens
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
) -> Result<(f64, f64), StormwaterAssetError> {
    match (latitude, longitude) {
        (Some(latitude), Some(longitude)) if valid_coordinate(latitude, longitude) => {
            Ok((latitude, longitude))
        }
        _ => Err(StormwaterAssetError::InvalidInput(format!(
            "stormwater drain row {record_id} requires valid subject coordinates"
        ))),
    }
}

fn stormwater_watermarks(input: &StormwaterDrainRiskInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: STORMWATER_SOURCE.to_string(),
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
pub enum StormwaterAssetError {
    Config(DagConfigError),
    InvalidInput(String),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
    Identity(SourceEntityResolutionError),
}

impl fmt::Display for StormwaterAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "stormwater drain config error: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid stormwater drain input: {message}"),
            Self::Json(err) => write!(f, "stormwater drain JSON error: {err}"),
            Self::Canonical(err) => {
                write!(f, "stormwater drain canonical society lookup failed: {err}")
            }
            Self::Identity(err) => {
                write!(
                    f,
                    "stormwater drain source identity resolution failed: {err}"
                )
            }
        }
    }
}

impl std::error::Error for StormwaterAssetError {}

impl From<DagConfigError> for StormwaterAssetError {
    fn from(value: DagConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<serde_json::Error> for StormwaterAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ReraAssetError> for StormwaterAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}

impl From<SourceEntityResolutionError> for StormwaterAssetError {
    fn from(value: SourceEntityResolutionError) -> Self {
        Self::Identity(value)
    }
}
