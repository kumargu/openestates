use std::collections::{BTreeMap, HashMap};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    read_canonical_society_rows, read_skill_fact_artifact_rows, MaterializationId,
    MaterializationRecord, SkillFactAnnotationRecord, SkillFactMaterializeError, SkillFactRecord,
    SkillFactsInput, SourceWatermark, CANONICAL_SOCIETY_NODES_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID,
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
    run_id: &MaterializationId,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, EnvironmentalAssetError> {
    validate_input(input)?;
    let canonical_record = dependency_record(parent_records, CANONICAL_SOCIETY_NODES_ASSET_ID)?;
    let rera_facts_record = dependency_record(parent_records, RERA_LEGAL_FACTS_ASSET_ID)?;
    let canonical_rows = read_canonical_society_rows(lake, canonical_record).await?;
    let fact_rows =
        read_skill_fact_artifact_rows(lake, std::slice::from_ref(rera_facts_record)).await?;
    let coordinates = society_coordinates(&fact_rows.facts)?;
    let society_names = canonical_rows
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| (entity.entity_id.as_str(), entity.name.as_str()))
        .collect::<HashMap<_, _>>();

    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for entity in canonical_rows
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
    {
        let Some(coordinate) = coordinates.get(entity.entity_id.as_str()) else {
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
            entity.entity_id.clone(),
            coordinate,
            zone,
            input,
            run_id,
            learned_at,
        )?);
        annotations.push(groundwater_annotation(entity.entity_id.clone())?);
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

#[derive(Debug, Clone, Copy)]
struct SocietyCoordinate {
    latitude: f64,
    longitude: f64,
}

fn society_coordinates(
    facts: &[SkillFactRecord],
) -> Result<HashMap<&str, SocietyCoordinate>, EnvironmentalAssetError> {
    let mut partial = HashMap::<&str, (Option<f64>, Option<f64>)>::new();
    for fact in facts {
        match fact.fact_key.as_str() {
            "geo.latitude" => {
                let (latitude, _) = partial.entry(fact.entity_id.as_str()).or_default();
                *latitude = numeric_fact_value(fact)?;
            }
            "geo.longitude" => {
                let (_, longitude) = partial.entry(fact.entity_id.as_str()).or_default();
                *longitude = numeric_fact_value(fact)?;
            }
            _ => {}
        }
    }
    Ok(partial
        .into_iter()
        .filter_map(|(entity_id, (latitude, longitude))| {
            Some((
                entity_id,
                SocietyCoordinate {
                    latitude: latitude?,
                    longitude: longitude?,
                },
            ))
        })
        .collect())
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

#[cfg(test)]
mod tests {
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

        let coordinates = society_coordinates(&facts).unwrap();

        assert_eq!(coordinates.len(), 1);
        assert_eq!(coordinates["society:a"].latitude, 12.9);
        assert_eq!(coordinates["society:a"].longitude, 77.7);
    }

    fn test_numeric_fact(entity_id: &str, fact_key: &str, value: f64) -> SkillFactRecord {
        SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "numeric".to_string(),
            value_json: serde_json::to_string(&FactValue::Numeric(value)).unwrap(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            triggered_by: None,
            learned_at: Utc::now(),
            run_id: "test".to_string(),
            input_hash: "test".to_string(),
        }
    }
}
