use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};

use crate::dag_config::{
    load_resolution_policies, normalize_source_type, resolve_coordinate_pair,
    CoordinateEntityScope, CoordinatePairCandidate, ResolutionPoliciesFile,
};
use crate::knowledge::FactValue;

use super::{ServingEntityFactRows, ServingFactRecord, SourceObservation};

#[derive(Debug, Clone, PartialEq)]
pub struct ServingCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub confidence: f32,
    pub source_type: String,
    pub learned_at: DateTime<Utc>,
    pub observations: Vec<SourceObservation>,
}

pub fn resolve_serving_coordinates(
    rows: &ServingEntityFactRows,
    scope: CoordinateEntityScope,
) -> Option<ServingCoordinates> {
    let policies = coordinate_policies()?;
    let mut observations = BTreeMap::<ObservationKey, PartialCoordinate>::new();
    for fact in &rows.facts {
        let axis = match fact.fact_key.as_str() {
            "geo.latitude" => Axis::Latitude,
            "geo.longitude" => Axis::Longitude,
            _ => continue,
        };
        let Some(value) = numeric_value(fact) else {
            continue;
        };
        let key = ObservationKey::from_fact(fact);
        let observation = observations
            .entry(key)
            .or_insert_with(|| PartialCoordinate {
                source_type: fact.source_type.clone(),
                learned_at: fact.learned_at,
                ..PartialCoordinate::default()
            });
        let candidate = AxisValue {
            value,
            confidence: fact.confidence,
            observation: fact.observation.clone(),
            identity: fact.stable_selection_key(),
        };
        match axis {
            Axis::Latitude => update_axis(&mut observation.latitude, candidate),
            Axis::Longitude => update_axis(&mut observation.longitude, candidate),
        }
    }

    let complete = observations
        .values()
        .filter_map(|observation| {
            let latitude = observation.latitude.as_ref()?;
            let longitude = observation.longitude.as_ref()?;
            Some((
                CoordinatePairCandidate {
                    source_type: &observation.source_type,
                    latitude: latitude.value,
                    longitude: longitude.value,
                    confidence: latitude.confidence.min(longitude.confidence),
                },
                observation,
            ))
        })
        .collect::<Vec<_>>();
    let resolved = resolve_coordinate_pair(
        scope,
        complete.iter().map(|(candidate, _)| *candidate),
        policies,
    )?;
    let selected = complete
        .iter()
        .find(|(candidate, _)| {
            normalize_source_type(candidate.source_type)
                == normalize_source_type(&resolved.source_type)
                && candidate.latitude == resolved.latitude
                && candidate.longitude == resolved.longitude
        })
        .map(|(_, observation)| *observation)?;
    let mut selected_observations = [
        selected
            .latitude
            .as_ref()
            .and_then(|axis| axis.observation.clone()),
        selected
            .longitude
            .as_ref()
            .and_then(|axis| axis.observation.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    selected_observations.sort_by(|left, right| {
        left.observation_id
            .as_str()
            .cmp(right.observation_id.as_str())
    });
    selected_observations.dedup_by(|left, right| left.observation_id == right.observation_id);
    Some(ServingCoordinates {
        latitude: resolved.latitude,
        longitude: resolved.longitude,
        confidence: resolved.confidence,
        source_type: resolved.source_type,
        learned_at: selected.learned_at,
        observations: selected_observations,
    })
}

fn coordinate_policies() -> Option<&'static ResolutionPoliciesFile> {
    static POLICIES: OnceLock<Option<ResolutionPoliciesFile>> = OnceLock::new();
    POLICIES
        .get_or_init(|| load_resolution_policies().ok())
        .as_ref()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ObservationKey {
    identity: String,
}

impl ObservationKey {
    fn from_fact(fact: &ServingFactRecord) -> Self {
        let identity = fact.observation.as_ref().map_or_else(
            || {
                format!(
                    "legacy|{}|{}|{}",
                    normalize_source_type(&fact.source_type),
                    fact.source_url.as_deref().unwrap_or_default(),
                    fact.skill_id.as_deref().unwrap_or_default()
                )
            },
            |observation| observation.observation_id.as_str().to_string(),
        );
        Self { identity }
    }
}

#[derive(Debug, Clone, Default)]
struct PartialCoordinate {
    source_type: String,
    latitude: Option<AxisValue>,
    longitude: Option<AxisValue>,
    learned_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct AxisValue {
    value: f64,
    confidence: f32,
    observation: Option<SourceObservation>,
    identity: String,
}

#[derive(Debug, Clone, Copy)]
enum Axis {
    Latitude,
    Longitude,
}

fn update_axis(slot: &mut Option<AxisValue>, candidate: AxisValue) {
    let replace = slot.as_ref().is_none_or(|current| {
        candidate.confidence > current.confidence
            || (candidate.confidence == current.confidence
                && (candidate.identity.as_str(), candidate.value.to_bits())
                    > (current.identity.as_str(), current.value.to_bits()))
    });
    if replace {
        *slot = Some(candidate);
    }
}

fn numeric_value(fact: &ServingFactRecord) -> Option<f64> {
    match fact.value {
        FactValue::Numeric(value) | FactValue::Score { value, .. } if value.is_finite() => {
            Some(value)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::serving::ServingFactIndex;

    #[test]
    fn refuses_to_mix_axes_from_different_observations() {
        let index = ServingFactIndex::from_records(
            vec![
                fact("geo.latitude", 12.9, "Google", "https://a", 0.9),
                fact("geo.longitude", 77.6, "Google", "https://b", 0.9),
            ],
            Vec::new(),
        );

        assert!(resolve_serving_coordinates(
            index.entity("society:test").unwrap(),
            CoordinateEntityScope::Society,
        )
        .is_none());
    }

    #[test]
    fn society_and_place_use_different_source_policies() {
        let facts = vec![
            fact("geo.latitude", 12.9, "OpenStreetMap", "https://osm", 0.9),
            fact("geo.longitude", 77.6, "OpenStreetMap", "https://osm", 0.9),
        ];
        let index = ServingFactIndex::from_records(facts, Vec::new());
        let rows = index.entity("society:test").unwrap();

        assert!(resolve_serving_coordinates(rows, CoordinateEntityScope::Society).is_none());
        assert!(resolve_serving_coordinates(rows, CoordinateEntityScope::Place).is_some());
    }

    #[test]
    fn timestamps_do_not_select_equal_confidence_observations() {
        let older_at = Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap();
        let newer_at = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let mut stable_latitude = fact("geo.latitude", 12.8, "Google", "https://a", 0.9);
        let mut stable_longitude = fact("geo.longitude", 77.5, "Google", "https://a", 0.9);
        let mut later_latitude = fact("geo.latitude", 12.9, "Google", "https://b", 0.9);
        let mut later_longitude = fact("geo.longitude", 77.6, "Google", "https://b", 0.9);
        stable_latitude.learned_at = older_at;
        stable_longitude.learned_at = older_at;
        later_latitude.learned_at = newer_at;
        later_longitude.learned_at = newer_at;
        let index = ServingFactIndex::from_records(
            vec![
                later_longitude,
                stable_latitude,
                later_latitude,
                stable_longitude,
            ],
            Vec::new(),
        );

        let resolved = resolve_serving_coordinates(
            index.entity("society:test").unwrap(),
            CoordinateEntityScope::Society,
        )
        .unwrap();

        assert_eq!((resolved.latitude, resolved.longitude), (12.8, 77.5));
        assert_eq!(resolved.learned_at, older_at);
    }

    fn fact(
        key: &str,
        value: f64,
        source_type: &str,
        source_url: &str,
        confidence: f32,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: "society:test".to_string(),
            fact_key: key.to_string(),
            value_type: "numeric".to_string(),
            value_text: Some(value.to_string()),
            value: FactValue::Numeric(value),
            confidence,
            source_type: source_type.to_string(),
            source_url: Some(source_url.to_string()),
            model: None,
            skill_id: Some("coordinate-test".to_string()),
            learned_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
            observation: None,
        }
    }
}
