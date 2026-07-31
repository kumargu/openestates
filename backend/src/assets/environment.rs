use std::collections::{BTreeMap, HashMap};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::{
    load_resolution_policies, resolve_coordinate_pair, CoordinateEntityScope,
    CoordinatePairCandidate, DagConfigError,
};
use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    read_canonical_society_rows, read_skill_fact_artifact_rows, MaterializationId,
    MaterializationRecord, SkillFactAnnotationRecord, SkillFactMaterializeError, SkillFactRecord,
    SkillFactsInput, SourceEntitySeed, SourceWatermark, CANONICAL_SOCIETY_NODES_ASSET_ID,
    GOOGLE_REVIEW_FACTS_ASSET_ID,
};

pub const SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID: &str =
    "society_groundwater_potential_facts";

const GROUNDWATER_SOURCE: &str = "opencity_groundwater_potential";
const GROUNDWATER_FACT_KEY: &str = "environment.groundwater_potential_class";
const GROUNDWATER_CONFIDENCE: f32 = 0.85;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentGroundwaterPotentialInput {
    pub snapshot_date: String,
    pub source_url: String,
    #[serde(default)]
    pub zones: Vec<EnvironmentGroundwaterPotentialZone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentGroundwaterPotentialZone {
    pub zone_id: String,
    pub groundwater_potential_class: String,
    #[serde(default)]
    pub rings: Vec<Vec<EnvironmentRingPoint>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentRingPoint {
    pub latitude: f64,
    pub longitude: f64,
}

pub async fn society_groundwater_potential_facts_input(
    lake: &LakeStore,
    input: &EnvironmentGroundwaterPotentialInput,
    parent_records: &[MaterializationRecord],
    source_entities: &[SourceEntitySeed],
    run_id: &MaterializationId,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, EnvironmentalAssetError> {
    validate_input(input)?;
    let canonical_record = dependency_record(parent_records, CANONICAL_SOCIETY_NODES_ASSET_ID)?;
    let canonical_rows = read_canonical_society_rows(lake, canonical_record).await?;
    let coordinate_records = parent_records
        .iter()
        .filter(|record| record.asset_id.as_str() == GOOGLE_REVIEW_FACTS_ASSET_ID)
        .cloned()
        .collect::<Vec<_>>();
    let fact_rows = read_skill_fact_artifact_rows(lake, &coordinate_records).await?;
    let resolution_policies = load_resolution_policies()?;
    let mut coordinates = society_coordinates(&fact_rows.facts, &resolution_policies)?;
    add_source_entity_seed_coordinates(&mut coordinates, source_entities, &resolution_policies);
    let society_names = groundwater_subject_names(&canonical_rows, source_entities);

    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for (entity_id, _) in &society_names {
        let Some(coordinate) = coordinates.get(entity_id.as_str()) else {
            continue;
        };
        let Some(zone) = input.zones.iter().find(|zone| {
            zone.rings
                .iter()
                .any(|ring| point_in_ring(coordinate.longitude, coordinate.latitude, ring))
        }) else {
            continue;
        };
        facts.push(groundwater_fact(
            entity_id.clone(),
            coordinate,
            zone,
            input,
            run_id,
            learned_at,
        )?);
        annotations.push(groundwater_annotation(entity_id.clone())?);
    }

    facts.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    annotations.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

    let mut watermarks = input.source_watermarks.clone();
    watermarks.push(SourceWatermark {
        source: "society_groundwater_point_in_polygon_join".to_string(),
        high_watermark: format!(
            "matched={};coordinates={};societies={}",
            facts.len(),
            coordinates.len(),
            society_names.len()
        ),
    });
    if facts.is_empty() {
        watermarks.push(SourceWatermark {
            source: "society_groundwater_potential_empty".to_string(),
            high_watermark: format!(
                "matched=0;coordinates={};societies={}",
                coordinates.len(),
                society_names.len()
            ),
        });
    }

    Ok(SkillFactsInput {
        source: GROUNDWATER_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: watermarks,
    })
}

fn groundwater_fact(
    entity_id: String,
    coordinate: &SocietyCoordinate,
    zone: &EnvironmentGroundwaterPotentialZone,
    input: &EnvironmentGroundwaterPotentialInput,
    run_id: &MaterializationId,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactRecord, EnvironmentalAssetError> {
    let value = zone.groundwater_potential_class.trim().to_string();
    let input_hash = groundwater_input_hash(&entity_id, coordinate, zone);
    Ok(SkillFactRecord {
        entity_id,
        fact_key: GROUNDWATER_FACT_KEY.to_string(),
        value_type: "text".to_string(),
        value_json: serde_json::to_string(&FactValue::Text(value))?,
        confidence: GROUNDWATER_CONFIDENCE,
        source_type: "OpenCity".to_string(),
        source_url: Some(input.source_url.clone()),
        model: None,
        skill_id: Some(SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID.to_string()),
        triggered_by: Some("offline_point_in_polygon_join".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash,
    })
}

fn groundwater_annotation(
    entity_id: String,
) -> Result<SkillFactAnnotationRecord, EnvironmentalAssetError> {
    Ok(SkillFactAnnotationRecord {
        entity_id,
        fact_key: GROUNDWATER_FACT_KEY.to_string(),
        display_template: Some("groundwater potential: {value}".to_string()),
        answers_preferences_json: serde_json::to_string(&[
            "groundwater potential",
            "water context",
            "groundwater zone",
        ])?,
        scoring_direction: Some("TextMatch".to_string()),
        scoring_weight: Some(0.8),
        scoring_thresholds_json: serde_json::to_string(&Vec::<f64>::new())?,
    })
}

fn validate_input(
    input: &EnvironmentGroundwaterPotentialInput,
) -> Result<(), EnvironmentalAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(EnvironmentalAssetError::InvalidInput(
            "snapshot_date cannot be empty".to_string(),
        ));
    }
    if input.source_url.trim().is_empty() {
        return Err(EnvironmentalAssetError::InvalidInput(
            "source_url cannot be empty".to_string(),
        ));
    }
    if input.zones.is_empty() {
        return Err(EnvironmentalAssetError::InvalidInput(
            "groundwater zones cannot be empty".to_string(),
        ));
    }
    for zone in &input.zones {
        if zone.groundwater_potential_class.trim().is_empty() {
            return Err(EnvironmentalAssetError::InvalidInput(format!(
                "zone {} has empty groundwater_potential_class",
                zone.zone_id
            )));
        }
        if zone.rings.iter().all(|ring| ring.len() < 3) {
            return Err(EnvironmentalAssetError::InvalidInput(format!(
                "zone {} has no usable polygon rings",
                zone.zone_id
            )));
        }
    }
    Ok(())
}

fn dependency_record<'a>(
    records: &'a [MaterializationRecord],
    asset_id: &str,
) -> Result<&'a MaterializationRecord, EnvironmentalAssetError> {
    records
        .iter()
        .find(|record| record.asset_id.as_str() == asset_id)
        .ok_or_else(|| EnvironmentalAssetError::MissingDependency(asset_id.to_string()))
}

#[derive(Debug, Clone)]
struct SocietyCoordinate {
    latitude: f64,
    longitude: f64,
    confidence: f32,
    source_type: String,
}

fn groundwater_subject_names(
    canonical_rows: &super::CanonicalSocietyRows,
    source_entities: &[SourceEntitySeed],
) -> BTreeMap<String, String> {
    if !source_entities.is_empty() {
        return source_entities
            .iter()
            .map(|seed| (seed.entity_id.clone(), seed.name.clone()))
            .collect();
    }
    canonical_rows
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| (entity.entity_id.clone(), entity.name.clone()))
        .collect()
}

fn add_source_entity_seed_coordinates(
    coordinates: &mut HashMap<String, SocietyCoordinate>,
    source_entities: &[SourceEntitySeed],
    policies: &crate::dag_config::ResolutionPoliciesFile,
) {
    for seed in source_entities {
        let (Some(latitude), Some(longitude)) = (seed.latitude, seed.longitude) else {
            continue;
        };
        let existing = coordinates.get(seed.entity_id.as_str());
        let candidates = [
            CoordinatePairCandidate {
                source_type: "source_entity_seed",
                latitude,
                longitude,
                confidence: 0.95,
            },
            CoordinatePairCandidate {
                source_type: existing.map_or("", |value| value.source_type.as_str()),
                latitude: existing.map_or(f64::NAN, |value| value.latitude),
                longitude: existing.map_or(f64::NAN, |value| value.longitude),
                confidence: existing.map_or(0.0, |value| value.confidence),
            },
        ];
        if let Some(resolved) =
            resolve_coordinate_pair(CoordinateEntityScope::Society, candidates, policies)
        {
            coordinates.insert(
                seed.entity_id.clone(),
                SocietyCoordinate {
                    latitude: resolved.latitude,
                    longitude: resolved.longitude,
                    confidence: resolved.confidence,
                    source_type: resolved.source_type,
                },
            );
        }
    }
}

fn society_coordinates(
    facts: &[SkillFactRecord],
    policies: &crate::dag_config::ResolutionPoliciesFile,
) -> Result<HashMap<String, SocietyCoordinate>, EnvironmentalAssetError> {
    let mut partial =
        HashMap::<&str, HashMap<CoordinateObservationKey, PartialSocietyCoordinate>>::new();
    for fact in facts {
        match fact.fact_key.as_str() {
            "geo.latitude" => {
                let value = numeric_fact_value(fact)?;
                if let Some(value) = value {
                    let coordinate = partial
                        .entry(fact.entity_id.as_str())
                        .or_default()
                        .entry(CoordinateObservationKey::from_fact(fact))
                        .or_default();
                    update_coordinate_value(&mut coordinate.latitude, value, fact.confidence);
                }
            }
            "geo.longitude" => {
                let value = numeric_fact_value(fact)?;
                if let Some(value) = value {
                    let coordinate = partial
                        .entry(fact.entity_id.as_str())
                        .or_default()
                        .entry(CoordinateObservationKey::from_fact(fact))
                        .or_default();
                    update_coordinate_value(&mut coordinate.longitude, value, fact.confidence);
                }
            }
            _ => {}
        }
    }
    Ok(partial
        .into_iter()
        .filter_map(|(entity_id, candidates)| {
            let coordinate = select_coordinate_candidate(candidates, policies)?;
            Some((entity_id.to_string(), coordinate))
        })
        .collect())
}

#[derive(Debug, Clone, Copy, Default)]
struct PartialSocietyCoordinate {
    latitude: Option<CoordinateValue>,
    longitude: Option<CoordinateValue>,
}

#[derive(Debug, Clone, Copy)]
struct CoordinateValue {
    value: f64,
    confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CoordinateObservationKey {
    source_type: String,
    source_url: String,
    skill_id: String,
    run_id: String,
    learned_at: DateTime<Utc>,
}

impl CoordinateObservationKey {
    fn from_fact(fact: &SkillFactRecord) -> Self {
        Self {
            source_type: fact.source_type.clone(),
            source_url: fact.source_url.clone().unwrap_or_default(),
            skill_id: fact.skill_id.clone().unwrap_or_default(),
            run_id: fact.run_id.clone(),
            learned_at: fact.learned_at,
        }
    }
}

fn update_coordinate_value(slot: &mut Option<CoordinateValue>, value: f64, confidence: f32) {
    if slot
        .as_ref()
        .map(|current| confidence > current.confidence)
        .unwrap_or(true)
    {
        *slot = Some(CoordinateValue { value, confidence });
    }
}

fn select_coordinate_candidate(
    candidates: HashMap<CoordinateObservationKey, PartialSocietyCoordinate>,
    policies: &crate::dag_config::ResolutionPoliciesFile,
) -> Option<SocietyCoordinate> {
    let mut complete = candidates
        .iter()
        .filter_map(|(observation, candidate)| {
            let latitude = candidate.latitude?;
            let longitude = candidate.longitude?;
            Some((observation, latitude, longitude))
        })
        .collect::<Vec<_>>();
    complete.sort_by(|(left, _, _), (right, _, _)| {
        right
            .learned_at
            .cmp(&left.learned_at)
            .then_with(|| left.source_type.cmp(&right.source_type))
            .then_with(|| left.source_url.cmp(&right.source_url))
            .then_with(|| left.skill_id.cmp(&right.skill_id))
            .then_with(|| left.run_id.cmp(&right.run_id))
    });
    let resolved = resolve_coordinate_pair(
        CoordinateEntityScope::Society,
        complete.into_iter().map(
            |(observation, latitude, longitude)| CoordinatePairCandidate {
                source_type: &observation.source_type,
                latitude: latitude.value,
                longitude: longitude.value,
                confidence: latitude.confidence.min(longitude.confidence),
            },
        ),
        policies,
    )?;
    Some(SocietyCoordinate {
        latitude: resolved.latitude,
        longitude: resolved.longitude,
        confidence: resolved.confidence,
        source_type: resolved.source_type,
    })
}

fn numeric_fact_value(fact: &SkillFactRecord) -> Result<Option<f64>, EnvironmentalAssetError> {
    match serde_json::from_str::<FactValue>(&fact.value_json)? {
        FactValue::Numeric(value) => Ok(Some(value)),
        _ => Ok(None),
    }
}

fn point_in_ring(x: f64, y: f64, ring: &[EnvironmentRingPoint]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = ring.len() - 1;
    for current in 0..ring.len() {
        let current_point = ring[current];
        let previous_point = ring[previous];
        let crosses = (current_point.latitude > y) != (previous_point.latitude > y);
        if crosses {
            let denominator = previous_point.latitude - current_point.latitude;
            if denominator.abs() > f64::EPSILON {
                let intersection_x = (previous_point.longitude - current_point.longitude)
                    * (y - current_point.latitude)
                    / denominator
                    + current_point.longitude;
                if x < intersection_x {
                    inside = !inside;
                }
            }
        }
        previous = current;
    }
    inside
}

fn groundwater_input_hash(
    entity_id: &str,
    coordinate: &SocietyCoordinate,
    zone: &EnvironmentGroundwaterPotentialZone,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entity_id.as_bytes());
    hasher.update(coordinate.latitude.to_le_bytes());
    hasher.update(coordinate.longitude.to_le_bytes());
    hasher.update(zone.zone_id.as_bytes());
    hasher.update(zone.groundwater_potential_class.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug)]
pub enum EnvironmentalAssetError {
    InvalidInput(String),
    MissingDependency(String),
    Rera(super::ReraAssetError),
    SkillFact(SkillFactMaterializeError),
    Json(serde_json::Error),
    Config(DagConfigError),
}

impl fmt::Display for EnvironmentalAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid environmental input: {message}"),
            Self::MissingDependency(asset_id) => {
                write!(f, "environmental asset missing dependency {asset_id}")
            }
            Self::Rera(err) => write!(f, "environmental canonical read error: {err}"),
            Self::SkillFact(err) => write!(f, "environmental fact read error: {err}"),
            Self::Json(err) => write!(f, "environmental fact JSON error: {err}"),
            Self::Config(err) => write!(f, "environmental config error: {err}"),
        }
    }
}

impl std::error::Error for EnvironmentalAssetError {}

impl From<super::ReraAssetError> for EnvironmentalAssetError {
    fn from(err: super::ReraAssetError) -> Self {
        Self::Rera(err)
    }
}

impl From<SkillFactMaterializeError> for EnvironmentalAssetError {
    fn from(err: SkillFactMaterializeError) -> Self {
        Self::SkillFact(err)
    }
}

impl From<serde_json::Error> for EnvironmentalAssetError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<DagConfigError> for EnvironmentalAssetError {
    fn from(err: DagConfigError) -> Self {
        Self::Config(err)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn point_in_ring_matches_inside_and_outside_coordinates() {
        let ring = vec![
            EnvironmentRingPoint {
                latitude: 12.0,
                longitude: 77.0,
            },
            EnvironmentRingPoint {
                latitude: 12.0,
                longitude: 78.0,
            },
            EnvironmentRingPoint {
                latitude: 13.0,
                longitude: 78.0,
            },
            EnvironmentRingPoint {
                latitude: 13.0,
                longitude: 77.0,
            },
        ];

        assert!(point_in_ring(77.5, 12.5, &ring));
        assert!(!point_in_ring(78.5, 12.5, &ring));
    }

    #[test]
    fn society_coordinates_require_both_latitude_and_longitude() {
        let facts = vec![
            test_numeric_fact("society:a", "geo.latitude", 12.9),
            test_numeric_fact("society:a", "geo.longitude", 77.7),
            test_numeric_fact("society:b", "geo.latitude", 12.8),
        ];

        let policies = load_resolution_policies().unwrap();
        let coordinates = society_coordinates(&facts, &policies).unwrap();

        assert_eq!(coordinates.len(), 1);
        assert_eq!(coordinates["society:a"].latitude, 12.9);
        assert_eq!(coordinates["society:a"].longitude, 77.7);
    }

    #[test]
    fn society_coordinates_pair_fact_specific_input_hashes() {
        let mut latitude = test_numeric_fact("society:a", "geo.latitude", 12.9);
        latitude.input_hash = "latitude-fact-hash".to_string();
        let mut longitude = test_numeric_fact("society:a", "geo.longitude", 77.7);
        longitude.input_hash = "longitude-fact-hash".to_string();

        let policies = load_resolution_policies().unwrap();
        let coordinates = society_coordinates(&[latitude, longitude], &policies).unwrap();

        assert_eq!(coordinates["society:a"].latitude, 12.9);
        assert_eq!(coordinates["society:a"].longitude, 77.7);
    }

    #[test]
    fn society_coordinates_prefer_google_pair_over_rera_pair() {
        let facts = vec![
            test_numeric_fact_with_source("society:a", "geo.latitude", 12.814964, "Rera", 1.0),
            test_numeric_fact_with_source("society:a", "geo.longitude", 77.509353, "Rera", 1.0),
            test_numeric_fact_with_source("society:a", "geo.latitude", 12.896276, "Google", 0.85),
            test_numeric_fact_with_source("society:a", "geo.longitude", 77.5308391, "Google", 0.85),
        ];
        let policies = load_resolution_policies().unwrap();

        let coordinates = society_coordinates(&facts, &policies).unwrap();

        assert_eq!(coordinates["society:a"].latitude, 12.896276);
        assert_eq!(coordinates["society:a"].longitude, 77.5308391);
    }

    #[test]
    fn society_coordinates_ignore_rera_only_coordinate_pair() {
        let facts = vec![
            test_numeric_fact_with_source("society:a", "geo.latitude", 12.814964, "Rera", 1.0),
            test_numeric_fact_with_source("society:a", "geo.longitude", 77.509353, "Rera", 1.0),
        ];
        let policies = load_resolution_policies().unwrap();

        let coordinates = society_coordinates(&facts, &policies).unwrap();

        assert!(!coordinates.contains_key("society:a"));
    }

    fn test_numeric_fact(entity_id: &str, fact_key: &str, value: f64) -> SkillFactRecord {
        test_numeric_fact_with_source(entity_id, fact_key, value, "Google", 1.0)
    }

    fn test_numeric_fact_with_source(
        entity_id: &str,
        fact_key: &str,
        value: f64,
        source_type: &str,
        confidence: f32,
    ) -> SkillFactRecord {
        SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "numeric".to_string(),
            value_json: serde_json::to_string(&FactValue::Numeric(value)).unwrap(),
            confidence,
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            triggered_by: None,
            learned_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
            run_id: "test".to_string(),
            input_hash: "test".to_string(),
        }
    }
}
