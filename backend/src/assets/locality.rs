use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;

use super::{SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark};

pub const OSM_LOCALITY_BOUNDARY_FACTS_ASSET_ID: &str = "osm_locality_boundary_facts";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmLocalityBoundariesInput {
    pub snapshot_date: String,
    pub source_url: String,
    #[serde(default)]
    pub boundaries: Vec<OsmLocalityBoundaryInput>,
    #[serde(default)]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmLocalityBoundaryInput {
    pub osm_id: String,
    pub name: String,
    pub geometry_geojson: String,
    pub source_url: String,
    #[serde(default)]
    pub admin_level: Option<String>,
    #[serde(default)]
    pub members: Vec<OsmBoundaryMemberInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsmBoundaryMemberInput {
    pub member_type: String,
    pub member_ref: String,
    pub role: String,
}

pub fn osm_locality_boundary_facts_input(
    input: &OsmLocalityBoundariesInput,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, LocalityAssetError> {
    if input.boundaries.is_empty() {
        return Err(LocalityAssetError::Invalid(
            "locality boundary list is empty".to_string(),
        ));
    }
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::new();
    for boundary in &input.boundaries {
        validate_boundary(boundary)?;
        let entity_id = format!("area:osm:{}", boundary.osm_id.trim().replace('/', "-"));
        let mut boundary_facts = vec![
            (
                "place.name",
                FactValue::Text(boundary.name.trim().to_string()),
                Some("Area: {value}"),
            ),
            (
                "place.category",
                FactValue::Text("locality".to_string()),
                Some("Area type: {value}"),
            ),
            (
                "geo.geometry_geojson",
                FactValue::Text(boundary.geometry_geojson.clone()),
                None,
            ),
        ];
        if let Some(admin_level) = boundary
            .admin_level
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            boundary_facts.push((
                "area.admin_level",
                FactValue::Text(admin_level.to_string()),
                Some("Administrative level: {value}"),
            ));
        }
        if !boundary.members.is_empty() {
            boundary_facts.push((
                "area.osm_boundary_members",
                FactValue::Text(serde_json::to_string(&boundary.members)?),
                None,
            ));
        }
        for (fact_key, value, template) in boundary_facts {
            let value_json = serde_json::to_string(&value)?;
            let mut hasher = Sha256::new();
            hasher.update(entity_id.as_bytes());
            hasher.update(fact_key.as_bytes());
            hasher.update(value_json.as_bytes());
            facts.push(SkillFactRecord {
                entity_id: entity_id.clone(),
                fact_key: fact_key.to_string(),
                value_type: match value {
                    FactValue::Text(_) => "text",
                    _ => "unknown",
                }
                .to_string(),
                value_json,
                confidence: 0.9,
                source_type: "OpenStreetMap".to_string(),
                source_url: Some(boundary.source_url.clone()),
                model: None,
                skill_id: Some(OSM_LOCALITY_BOUNDARY_FACTS_ASSET_ID.to_string()),
                triggered_by: Some("overpass_administrative_boundary_snapshot".to_string()),
                learned_at,
                run_id: run_id.to_string(),
                input_hash: hex_digest(&hasher.finalize()),
                observation_provider: None,
                provider_observation_id: None,
                asset_lineage: Vec::new(),
            });
            if annotation_keys.insert((entity_id.clone(), fact_key.to_string())) {
                annotations.push(SkillFactAnnotationRecord {
                    entity_id: entity_id.clone(),
                    fact_key: fact_key.to_string(),
                    display_template: template.map(str::to_string),
                    answers_preferences_json: "[]".to_string(),
                    scoring_direction: None,
                    scoring_weight: Some(0.0),
                    scoring_thresholds_json: "[]".to_string(),
                });
            }
        }
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
    let mut watermarks = input.source_watermarks.clone();
    watermarks.push(SourceWatermark {
        source: "openstreetmap_locality_boundary_count".to_string(),
        high_watermark: input.boundaries.len().to_string(),
    });
    Ok(SkillFactsInput {
        source: "openstreetmap_locality_boundaries".to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: watermarks,
    })
}

fn validate_boundary(boundary: &OsmLocalityBoundaryInput) -> Result<(), LocalityAssetError> {
    if boundary.osm_id.trim().is_empty()
        || boundary.name.trim().is_empty()
        || boundary.source_url.trim().is_empty()
    {
        return Err(LocalityAssetError::Invalid(
            "locality boundary identity and source are required".to_string(),
        ));
    }
    let parsed = boundary
        .geometry_geojson
        .parse::<GeoJson>()
        .map_err(|error| LocalityAssetError::Invalid(error.to_string()))?;
    let value = match parsed {
        GeoJson::Geometry(geometry) => geometry.value,
        GeoJson::Feature(feature) => {
            feature
                .geometry
                .ok_or_else(|| {
                    LocalityAssetError::Invalid("locality feature has no geometry".to_string())
                })?
                .value
        }
        GeoJson::FeatureCollection(_) => {
            return Err(LocalityAssetError::Invalid(
                "locality geometry must be one polygon or multipolygon".to_string(),
            ))
        }
    };
    if !matches!(
        value,
        GeoJsonValue::Polygon(_) | GeoJsonValue::MultiPolygon(_)
    ) {
        return Err(LocalityAssetError::Invalid(
            "locality geometry must be Polygon or MultiPolygon".to_string(),
        ));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
pub enum LocalityAssetError {
    Invalid(String),
    Json(serde_json::Error),
}

impl fmt::Display for LocalityAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid OSM locality input: {message}"),
            Self::Json(error) => write!(formatter, "OSM locality JSON error: {error}"),
        }
    }
}

impl std::error::Error for LocalityAssetError {}

impl From<serde_json::Error> for LocalityAssetError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locality_boundary_emits_canonical_area_geometry_with_lineage() {
        let output = osm_locality_boundary_facts_input(&OsmLocalityBoundariesInput {
            snapshot_date: "2026-09-05".to_string(),
            source_url: "https://overpass-api.de/api/interpreter".to_string(),
            boundaries: vec![OsmLocalityBoundaryInput {
                osm_id: "relation/123".to_string(),
                name: "Fixture locality".to_string(),
                geometry_geojson: r#"{"type":"Polygon","coordinates":[[[77.0,12.0],[77.1,12.0],[77.1,12.1],[77.0,12.0]]]}"#.to_string(),
                source_url: "https://www.openstreetmap.org/relation/123".to_string(),
                admin_level: Some("10".to_string()),
                members: vec![OsmBoundaryMemberInput {
                    member_type: "way".to_string(),
                    member_ref: "456".to_string(),
                    role: "outer".to_string(),
                }],
            }],
            source_watermarks: Vec::new(),
        }, "run-1", Utc::now()).unwrap();
        assert_eq!(output.facts.len(), 5);
        assert!(output
            .facts
            .iter()
            .all(|fact| fact.entity_id == "area:osm:relation-123"));
        assert!(output
            .facts
            .iter()
            .any(|fact| fact.fact_key == "geo.geometry_geojson"
                && fact.source_type == "OpenStreetMap"));
        assert!(output.facts.iter().any(|fact| {
            fact.fact_key == "area.admin_level"
                && fact.value_json
                    == serde_json::to_string(&FactValue::Text("10".to_string())).unwrap()
        }));
        assert!(output
            .facts
            .iter()
            .any(|fact| fact.fact_key == "area.osm_boundary_members"));
    }
}
