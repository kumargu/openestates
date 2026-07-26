use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;

use super::{SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark};

pub const BENGALURU_METRO_STATION_FACTS_ASSET_ID: &str = "bengaluru_metro_station_facts";

const BENGALURU_METRO_SOURCE: &str = "openstreetmap_bengaluru_metro";
const METRO_STATION_CONFIDENCE: f32 = 0.82;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BengaluruMetroStationsInput {
    pub snapshot_date: String,
    pub source_url: String,
    #[serde(default)]
    pub stations: Vec<BengaluruMetroStationInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BengaluruMetroStationInput {
    pub station_id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operational_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_tags: BTreeMap<String, String>,
}

pub fn bengaluru_metro_station_facts_input(
    input: &BengaluruMetroStationsInput,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, TransitAssetError> {
    validate_input(input)?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut annotation_keys = BTreeSet::<(String, String)>::new();

    for station in &input.stations {
        let entity_id = format!("place:metro:{}", slug(&station.name));
        let source_url = station
            .source_url
            .clone()
            .or_else(|| Some(input.source_url.clone()));
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &entity_id,
            "place.name",
            FactValue::Text(station.name.trim().to_string()),
            source_url.clone(),
            station,
            run_id,
            learned_at,
        )?;
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &entity_id,
            "place.category",
            FactValue::Text("metro_station".to_string()),
            source_url.clone(),
            station,
            run_id,
            learned_at,
        )?;
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &entity_id,
            "place.types",
            FactValue::Tags(place_types(station)),
            source_url.clone(),
            station,
            run_id,
            learned_at,
        )?;
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &entity_id,
            "geo.latitude",
            FactValue::Numeric(station.latitude),
            source_url.clone(),
            station,
            run_id,
            learned_at,
        )?;
        push_fact(
            &mut facts,
            &mut annotations,
            &mut annotation_keys,
            &entity_id,
            "geo.longitude",
            FactValue::Numeric(station.longitude),
            source_url.clone(),
            station,
            run_id,
            learned_at,
        )?;
        if !station.lines.is_empty() {
            let normalized_lines = normalize_metro_lines(&station.lines);
            if !normalized_lines.is_empty() {
                push_fact(
                    &mut facts,
                    &mut annotations,
                    &mut annotation_keys,
                    &entity_id,
                    "transit.lines",
                    FactValue::Tags(normalized_lines),
                    source_url.clone(),
                    station,
                    run_id,
                    learned_at,
                )?;
            }
        }
        if let Some(network) = station
            .network
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                "transit.network",
                FactValue::Text(network.trim().to_string()),
                source_url.clone(),
                station,
                run_id,
                learned_at,
            )?;
        }
        if let Some(operator) = station
            .operator
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                "transit.operator",
                FactValue::Text(operator.trim().to_string()),
                source_url.clone(),
                station,
                run_id,
                learned_at,
            )?;
        }
        if let Some(status) = station
            .operational_status
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            push_fact(
                &mut facts,
                &mut annotations,
                &mut annotation_keys,
                &entity_id,
                "transit.operational_status",
                FactValue::Text(status.trim().to_string()),
                source_url.clone(),
                station,
                run_id,
                learned_at,
            )?;
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
        source: "bengaluru_metro_station_count".to_string(),
        high_watermark: input.stations.len().to_string(),
    });

    Ok(SkillFactsInput {
        source: BENGALURU_METRO_SOURCE.to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: watermarks,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    annotation_keys: &mut BTreeSet<(String, String)>,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    source_url: Option<String>,
    station: &BengaluruMetroStationInput,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<(), TransitAssetError> {
    let value_type = fact_value_type(&value).to_string();
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type,
        value_json: serde_json::to_string(&value)?,
        confidence: METRO_STATION_CONFIDENCE,
        source_type: "OpenStreetMap".to_string(),
        source_url,
        model: None,
        skill_id: Some(BENGALURU_METRO_STATION_FACTS_ASSET_ID.to_string()),
        triggered_by: Some("overpass_station_snapshot".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: station_fact_hash(station, fact_key),
    });
    let key = (entity_id.to_string(), fact_key.to_string());
    if annotation_keys.insert(key) {
        annotations.push(annotation(entity_id, fact_key)?);
    }
    Ok(())
}

fn annotation(
    entity_id: &str,
    fact_key: &str,
) -> Result<SkillFactAnnotationRecord, TransitAssetError> {
    let (display_template, answers_preferences, scoring_direction, scoring_weight) = match fact_key
    {
        "place.name" => (
            Some("Metro station: {value}".to_string()),
            vec!["metro", "metro station", "near metro"],
            Some("TextMatch".to_string()),
            Some(0.2),
        ),
        "place.category" => (
            Some("Place category: {value}".to_string()),
            vec!["metro", "nearby", "transit"],
            Some("TextMatch".to_string()),
            Some(0.0),
        ),
        "place.types" => (
            Some("Place types: {value}".to_string()),
            vec!["metro", "line", "transit"],
            Some("TextMatch".to_string()),
            Some(0.0),
        ),
        "geo.latitude" | "geo.longitude" => (
            None,
            vec!["coordinates", "location"],
            Some("LowerIsBetter".to_string()),
            Some(0.0),
        ),
        "transit.lines" => (
            Some("Metro line: {value}".to_string()),
            vec!["metro line", "purple line", "green line", "yellow line"],
            Some("TextMatch".to_string()),
            Some(0.3),
        ),
        "transit.network" => (
            Some("Transit network: {value}".to_string()),
            vec!["namma metro", "metro network"],
            Some("TextMatch".to_string()),
            Some(0.1),
        ),
        "transit.operator" => (
            Some("Transit operator: {value}".to_string()),
            vec!["bmrcl", "metro operator"],
            Some("TextMatch".to_string()),
            Some(0.0),
        ),
        "transit.operational_status" => (
            Some("Metro status: {value}".to_string()),
            vec!["operational metro", "future metro"],
            Some("TextMatch".to_string()),
            Some(0.2),
        ),
        _ => (None, Vec::new(), None, None),
    };
    Ok(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template,
        answers_preferences_json: serde_json::to_string(&answers_preferences)?,
        scoring_direction,
        scoring_weight,
        scoring_thresholds_json: serde_json::to_string(&Vec::<f64>::new())?,
    })
}

fn place_types(station: &BengaluruMetroStationInput) -> Vec<String> {
    let mut values = vec!["metro_station".to_string()];
    values.extend(
        normalize_metro_lines(&station.lines)
            .iter()
            .map(|line| slug(line)),
    );
    values.sort();
    values.dedup();
    values
}

fn normalize_metro_lines(raw: &[String]) -> Vec<String> {
    let mut values = Vec::new();
    for item in raw {
        if let Some(label) = normalize_metro_line_label(item) {
            if !values.iter().any(|existing| existing == &label) {
                values.push(label);
            }
        }
    }
    values
}

fn normalize_metro_line_label(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let hex = lower.trim_start_matches('#');
    const COLOR_HEX: &[(&str, &str)] = &[
        ("e542de", "Purple Line"),
        ("800080", "Purple Line"),
        ("9b59b6", "Purple Line"),
        ("6b2d5c", "Purple Line"),
        ("2ecc71", "Green Line"),
        ("008000", "Green Line"),
        ("00a651", "Green Line"),
        ("39b54a", "Green Line"),
        ("f1c40f", "Yellow Line"),
        ("ffd100", "Yellow Line"),
        ("ffcc00", "Yellow Line"),
        ("ffeb3b", "Yellow Line"),
        ("e91e63", "Pink Line"),
        ("ff69b4", "Pink Line"),
        ("ec407a", "Pink Line"),
        ("3498db", "Blue Line"),
        ("0077c8", "Blue Line"),
    ];
    for (code, label) in COLOR_HEX {
        if hex == *code {
            return Some((*label).to_string());
        }
    }
    for color in ["purple", "green", "yellow", "pink", "blue", "orange", "red"] {
        if lower == color || (lower.contains(color) && lower.contains("line")) {
            let mut chars = color.chars();
            let capitalized = chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default();
            return Some(format!("{capitalized} Line"));
        }
    }
    None
}

fn validate_input(input: &BengaluruMetroStationsInput) -> Result<(), TransitAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(TransitAssetError::InvalidInput(
            "snapshot_date cannot be empty".to_string(),
        ));
    }
    if input.source_url.trim().is_empty() {
        return Err(TransitAssetError::InvalidInput(
            "source_url cannot be empty".to_string(),
        ));
    }
    if input.stations.is_empty() {
        return Err(TransitAssetError::InvalidInput(
            "metro station list cannot be empty".to_string(),
        ));
    }
    for station in &input.stations {
        if station.station_id.trim().is_empty() {
            return Err(TransitAssetError::InvalidInput(
                "station_id cannot be empty".to_string(),
            ));
        }
        if station.name.trim().is_empty() {
            return Err(TransitAssetError::InvalidInput(format!(
                "station {} has empty name",
                station.station_id
            )));
        }
        if !valid_latitude(station.latitude) || !valid_longitude(station.longitude) {
            return Err(TransitAssetError::InvalidInput(format!(
                "station {} has invalid coordinates",
                station.name
            )));
        }
    }
    Ok(())
}

fn station_fact_hash(station: &BengaluruMetroStationInput, fact_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(station.station_id.as_bytes());
    hasher.update(station.name.as_bytes());
    hasher.update(fact_key.as_bytes());
    hasher.update(station.latitude.to_le_bytes());
    hasher.update(station.longitude.to_le_bytes());
    for line in &station.lines {
        hasher.update(line.as_bytes());
    }
    hex_digest(&hasher.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
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

fn valid_latitude(value: f64) -> bool {
    value.is_finite() && (-90.0..=90.0).contains(&value)
}

fn valid_longitude(value: f64) -> bool {
    value.is_finite() && (-180.0..=180.0).contains(&value)
}

fn slug(value: &str) -> String {
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

#[derive(Debug)]
pub enum TransitAssetError {
    InvalidInput(String),
    Json(serde_json::Error),
}

impl fmt::Display for TransitAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "{message}"),
            Self::Json(err) => write!(f, "failed to encode transit fact JSON: {err}"),
        }
    }
}

impl std::error::Error for TransitAssetError {}

impl From<serde_json::Error> for TransitAssetError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn metro_station_input_emits_place_facts_for_serving_bundle() {
        let input = BengaluruMetroStationsInput {
            snapshot_date: "2026-07-24".to_string(),
            source_url: "https://overpass-api.de/api/interpreter".to_string(),
            stations: vec![BengaluruMetroStationInput {
                station_id: "node/1".to_string(),
                name: "Kadugodi Tree Park".to_string(),
                latitude: 12.995,
                longitude: 77.759,
                lines: vec!["#e542de".to_string(), "BYPH".to_string()],
                network: Some("Namma Metro".to_string()),
                operator: Some("BMRCL".to_string()),
                operational_status: Some("operational".to_string()),
                source_url: Some("https://www.openstreetmap.org/node/1".to_string()),
                source_tags: BTreeMap::new(),
            }],
            source_watermarks: Vec::new(),
        };

        let output = bengaluru_metro_station_facts_input(
            &input,
            "run-1",
            Utc.with_ymd_and_hms(2026, 7, 24, 9, 30, 0).unwrap(),
        )
        .expect("metro input should materialize");
        let facts = output
            .facts
            .iter()
            .map(|fact| (fact.entity_id.as_str(), fact.fact_key.as_str()))
            .collect::<Vec<_>>();

        assert!(facts.contains(&("place:metro:kadugodi-tree-park", "place.name")));
        assert!(facts.contains(&("place:metro:kadugodi-tree-park", "geo.latitude")));
        assert!(facts.contains(&("place:metro:kadugodi-tree-park", "geo.longitude")));
        assert!(facts.contains(&("place:metro:kadugodi-tree-park", "transit.lines")));
        let lines = output
            .facts
            .iter()
            .find(|fact| fact.fact_key == "transit.lines")
            .expect("transit.lines fact");
        assert!(lines.value_json.contains("Purple Line"));
        assert!(!lines.value_json.contains("BYPH"));
        assert!(output
            .fact_annotations
            .iter()
            .any(|annotation| annotation.fact_key == "transit.lines"));
    }
}
