use std::collections::{HashMap, HashSet};

use chrono::Utc;

use crate::dag_config::{load_fact_registry_index, scoring_direction_from_hint, FactRegistryEntry};
use crate::knowledge::FactValue;
use crate::search::geo::haversine_km;
use crate::search::{analyzer, schema};

use super::{
    ServingEdgeRecord, ServingEntityFactRows, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, ServingSearchMetadataRecord,
};

const NEAR_PLACE_EDGE: &str = "near_place";
const DERIVED_SOURCE_TYPE: &str = "Computed";
const DERIVED_MODEL: &str = "serving-proximity-v1";

#[derive(Debug, Clone, Default)]
pub struct DerivedProximityRecords {
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
    pub edges: Vec<ServingEdgeRecord>,
}

#[derive(Debug, Clone)]
struct ProximityFactSpec {
    fact_key: String,
    max_distance_km: f64,
    match_tokens: HashSet<String>,
    display_template: Option<String>,
    answers_preferences: Vec<String>,
    scoring_direction: Option<String>,
    scoring_weight: Option<f32>,
    scoring_thresholds: Vec<f64>,
}

#[derive(Debug, Clone)]
struct EntityPoint {
    entity_id: String,
    name: String,
    latitude: f64,
    longitude: f64,
    confidence: f32,
}

#[derive(Debug, Clone)]
struct PlacePoint {
    point: EntityPoint,
    match_tokens: HashSet<String>,
    source_url: Option<String>,
}

#[derive(Debug, Clone)]
struct NearbyPlaceCandidate<'a> {
    place: &'a PlacePoint,
    spec: &'a ProximityFactSpec,
    distance_km: f64,
    confidence: f32,
}

pub fn derive_proximity_records(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    existing_edges: &[ServingEdgeRecord],
) -> DerivedProximityRecords {
    let specs = proximity_fact_specs();
    if specs.is_empty() {
        return DerivedProximityRecords::default();
    }
    let target_entities = target_entity_points(entities, fact_index);
    let places = place_points(entities, fact_index);
    if target_entities.is_empty() || places.is_empty() {
        return DerivedProximityRecords::default();
    }

    let existing_mentions = existing_nearby_mentions(fact_index, &specs);
    let mut output = DerivedProximityRecords::default();
    let mut seen_edges = existing_near_place_edges(existing_edges);

    for target in &target_entities {
        let mut by_fact_key = HashMap::<&str, Vec<NearbyPlaceCandidate<'_>>>::new();
        for place in &places {
            let distance_km = haversine_km(
                target.latitude,
                target.longitude,
                place.point.latitude,
                place.point.longitude,
            );
            if !distance_km.is_finite() || distance_km < 0.0 {
                continue;
            }
            for spec in &specs {
                if distance_km > spec.max_distance_km || !place_matches_spec(place, spec) {
                    continue;
                }
                let confidence = target.confidence.min(place.point.confidence).min(0.9);
                by_fact_key
                    .entry(spec.fact_key.as_str())
                    .or_default()
                    .push(NearbyPlaceCandidate {
                        place,
                        spec,
                        distance_km,
                        confidence,
                    });
            }
        }

        for (fact_key, candidates) in by_fact_key.iter_mut() {
            candidates.sort_by(|left, right| {
                left.distance_km
                    .partial_cmp(&right.distance_km)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| right.confidence.total_cmp(&left.confidence))
                    .then_with(|| left.place.point.name.cmp(&right.place.point.name))
            });

            for candidate in candidates {
                let already_mentioned = existing_mentions
                    .get(&(target.entity_id.clone(), (*fact_key).to_string()))
                    .is_some_and(|names| {
                        names
                            .iter()
                            .any(|name| place_names_compatible(name, &candidate.place.point.name))
                    });

                if seen_edges.insert((
                    target.entity_id.clone(),
                    candidate.place.point.entity_id.clone(),
                )) {
                    output.edges.push(ServingEdgeRecord {
                        from_entity_id: target.entity_id.clone(),
                        edge_type: NEAR_PLACE_EDGE.to_string(),
                        to_entity_id: candidate.place.point.entity_id.clone(),
                        confidence: candidate.confidence,
                        source_type: DERIVED_SOURCE_TYPE.to_string(),
                    });
                }
                if already_mentioned {
                    continue;
                }
                output.facts.push(derived_nearby_fact(target, candidate));
                output
                    .search_metadata
                    .push(derived_search_metadata(target, candidate.spec));
            }
        }
    }

    output
}

fn existing_near_place_edges(existing_edges: &[ServingEdgeRecord]) -> HashSet<(String, String)> {
    existing_edges
        .iter()
        .filter(|edge| edge.edge_type.eq_ignore_ascii_case(NEAR_PLACE_EDGE))
        .map(|edge| (edge.from_entity_id.clone(), edge.to_entity_id.clone()))
        .collect()
}

fn derived_nearby_fact(
    society: &EntityPoint,
    candidate: &NearbyPlaceCandidate<'_>,
) -> ServingFactRecord {
    let display = nearby_display(candidate);
    ServingFactRecord {
        entity_id: society.entity_id.clone(),
        fact_key: candidate.spec.fact_key.clone(),
        value_type: "text".to_string(),
        value_text: Some(display.clone()),
        value: FactValue::Text(display),
        confidence: candidate.confidence,
        source_type: DERIVED_SOURCE_TYPE.to_string(),
        source_url: candidate.place.source_url.clone(),
        model: Some(DERIVED_MODEL.to_string()),
        skill_id: None,
        learned_at: Utc::now(),
    }
}

fn derived_search_metadata(
    society: &EntityPoint,
    spec: &ProximityFactSpec,
) -> ServingSearchMetadataRecord {
    ServingSearchMetadataRecord {
        entity_id: society.entity_id.clone(),
        fact_key: spec.fact_key.clone(),
        display_template: spec
            .display_template
            .clone()
            .or_else(|| Some("{value}".to_string())),
        answers_preferences: spec.answers_preferences.clone(),
        scoring_direction: spec.scoring_direction.clone(),
        scoring_weight: spec.scoring_weight,
        scoring_thresholds: spec.scoring_thresholds.clone(),
    }
}

fn nearby_display(candidate: &NearbyPlaceCandidate<'_>) -> String {
    format!(
        "{} ({:.1} km)",
        candidate.place.point.name, candidate.distance_km
    )
}

fn target_entity_points(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
) -> Vec<EntityPoint> {
    let mut points = Vec::new();
    for entity in entities {
        let Some(rows) = fact_index.entity(&entity.entity_id) else {
            continue;
        };
        if is_place_entity(entity, rows) {
            continue;
        }
        let Some(point) = entity_point_from_rows(&entity.entity_id, &entity.name, rows) else {
            continue;
        };
        points.push(point);
    }
    points
}

fn place_points(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
) -> Vec<PlacePoint> {
    entities
        .iter()
        .filter_map(|entity| {
            let rows = fact_index.entity(&entity.entity_id)?;
            if !is_place_entity(entity, rows) {
                return None;
            }
            let point = entity_point_from_rows(&entity.entity_id, &entity.name, rows)?;
            let place_types = text_tags(rows, "place.types");
            let category = text_fact(rows, "place.category");
            let match_tokens = place_match_tokens(&point.name, &place_types, category.as_deref());
            Some(PlacePoint {
                point,
                match_tokens,
                source_url: best_source_url(rows),
            })
        })
        .collect()
}

fn is_place_entity(entity: &ServingEntityRecord, rows: &ServingEntityFactRows) -> bool {
    entity.entity_type.eq_ignore_ascii_case("place")
        || has_fact_key(rows, "place.name")
        || has_fact_key(rows, "place.types")
        || has_fact_key(rows, "place.category")
}

fn proximity_fact_specs() -> Vec<ProximityFactSpec> {
    let policy = schema::ranking_policy();
    let registry = load_fact_registry_index().ok();
    let mut specs = policy
        .geo_distance_fact_keys
        .iter()
        .filter_map(|fact_key| {
            let registry_entry = registry
                .as_ref()
                .and_then(|index| index.lookup(fact_key))
                .cloned();
            proximity_fact_spec(fact_key, policy.named_place_zero_score_km, registry_entry)
        })
        .collect::<Vec<_>>();
    remove_cross_category_tokens(&mut specs);
    specs
        .into_iter()
        .filter(|spec| !spec.match_tokens.is_empty())
        .collect()
}

fn proximity_fact_spec(
    fact_key: &str,
    max_distance_km: f64,
    registry_entry: Option<FactRegistryEntry>,
) -> Option<ProximityFactSpec> {
    if !max_distance_km.is_finite() || max_distance_km <= 0.0 {
        return None;
    }
    let mut seed_text = vec![readable_key(fact_key)];
    if let Some(entry) = registry_entry.as_ref() {
        if let Some(label) = entry.label.as_ref() {
            seed_text.push(label.clone());
        }
        seed_text.extend(entry.answers_preferences.iter().cloned());
        if let Some(template) = entry.display_template.as_ref() {
            seed_text.push(template.clone());
        }
    }
    let match_tokens = token_set(seed_text.join(" "));
    let fallback_preference = readable_key(fact_key);
    let (
        display_template,
        answers_preferences,
        scoring_direction,
        scoring_weight,
        scoring_thresholds,
    ) = registry_entry
        .map(|entry| {
            let scoring_direction = entry.scoring_hint.as_ref().map(scoring_direction_from_hint);
            let scoring_weight = entry.scoring_hint.as_ref().and_then(|hint| hint.weight);
            let scoring_thresholds = entry
                .scoring_hint
                .as_ref()
                .map(|hint| hint.thresholds.clone())
                .unwrap_or_default();
            (
                entry.display_template,
                non_empty_or(entry.answers_preferences, fallback_preference.clone()),
                scoring_direction,
                scoring_weight,
                scoring_thresholds,
            )
        })
        .unwrap_or_else(|| {
            (
                None,
                vec![fallback_preference],
                Some("TextMatch".to_string()),
                None,
                Vec::new(),
            )
        });
    Some(ProximityFactSpec {
        fact_key: fact_key.to_string(),
        max_distance_km,
        match_tokens,
        display_template,
        answers_preferences,
        scoring_direction,
        scoring_weight,
        scoring_thresholds,
    })
}

fn remove_cross_category_tokens(specs: &mut [ProximityFactSpec]) {
    let mut token_counts = HashMap::<String, usize>::new();
    for spec in specs.iter() {
        for token in &spec.match_tokens {
            *token_counts.entry(token.clone()).or_default() += 1;
        }
    }
    for spec in specs {
        spec.match_tokens
            .retain(|token| token_counts.get(token).copied().unwrap_or_default() == 1);
    }
}

fn non_empty_or(values: Vec<String>, fallback: String) -> Vec<String> {
    if values.iter().any(|value| !value.trim().is_empty()) {
        values
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .collect()
    } else {
        vec![fallback]
    }
}

fn entity_point_from_rows(
    entity_id: &str,
    fallback_name: &str,
    rows: &ServingEntityFactRows,
) -> Option<EntityPoint> {
    let latitude = coordinate_value(rows, &["geo.latitude", "project_latitude"])?;
    let longitude = coordinate_value(rows, &["geo.longitude", "project_longitude"])?;
    if !valid_latitude(latitude.value) || !valid_longitude(longitude.value) {
        return None;
    }
    Some(EntityPoint {
        entity_id: entity_id.to_string(),
        name: text_fact(rows, "place.name")
            .or_else(|| text_fact(rows, "listing_society"))
            .or_else(|| text_fact(rows, "rera_project_name"))
            .unwrap_or_else(|| fallback_name.to_string()),
        latitude: latitude.value,
        longitude: longitude.value,
        confidence: latitude.confidence.min(longitude.confidence),
    })
}

#[derive(Debug, Clone, Copy)]
struct CoordinateValue {
    value: f64,
    confidence: f32,
}

fn coordinate_value(rows: &ServingEntityFactRows, keys: &[&str]) -> Option<CoordinateValue> {
    keys.iter().find_map(|key| {
        rows.facts
            .iter()
            .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
            .filter_map(|fact| {
                numeric_value(&fact.value).map(|value| CoordinateValue {
                    value,
                    confidence: fact.confidence,
                })
            })
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
    })
}

fn place_matches_spec(place: &PlacePoint, spec: &ProximityFactSpec) -> bool {
    if spec.match_tokens.is_empty() || place.match_tokens.is_empty() {
        return false;
    }
    spec.match_tokens.is_subset(&place.match_tokens)
}

fn existing_nearby_mentions(
    fact_index: &ServingFactIndex,
    specs: &[ProximityFactSpec],
) -> HashMap<(String, String), Vec<String>> {
    let fact_keys = specs
        .iter()
        .map(|spec| spec.fact_key.as_str())
        .collect::<HashSet<_>>();
    let mut mentions = HashMap::<(String, String), Vec<String>>::new();
    for (entity_id, rows) in fact_index.rows() {
        for fact in &rows.facts {
            if !fact_keys.contains(fact.fact_key.as_str()) {
                continue;
            }
            let Some(text) = fact_text(&fact.value).or_else(|| fact.value_text.clone()) else {
                continue;
            };
            mentions
                .entry((entity_id.to_string(), fact.fact_key.clone()))
                .or_default()
                .push(text);
        }
    }
    mentions
}

fn place_names_compatible(existing: &str, place_name: &str) -> bool {
    let existing = token_set(existing);
    let place = token_set(place_name);
    if existing.is_empty() || place.is_empty() {
        return false;
    }
    place.is_subset(&existing) || existing.is_subset(&place)
}

fn place_match_tokens(
    name: &str,
    place_types: &[String],
    category: Option<&str>,
) -> HashSet<String> {
    let mut text = vec![name.to_string()];
    text.extend(place_types.iter().cloned());
    if let Some(category) = category {
        text.push(category.to_string());
    }
    token_set(text.join(" "))
}

fn token_set(text: impl AsRef<str>) -> HashSet<String> {
    analyzer::stemmed_tokens(&readable_key(text.as_ref()))
        .into_iter()
        .collect()
}

fn readable_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn text_fact(rows: &ServingEntityFactRows, key: &str) -> Option<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .filter_map(|fact| fact_text(&fact.value).or_else(|| fact.value_text.clone()))
        .max_by_key(|value| value.len())
}

fn text_tags(rows: &ServingEntityFactRows, key: &str) -> Vec<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .flat_map(|fact| match &fact.value {
            FactValue::Tags(values) => values.clone(),
            FactValue::Text(value) => vec![value.clone()],
            _ => Vec::new(),
        })
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn has_fact_key(rows: &ServingEntityFactRows, key: &str) -> bool {
    rows.facts
        .iter()
        .any(|fact| fact.fact_key.eq_ignore_ascii_case(key))
}

fn best_source_url(rows: &ServingEntityFactRows) -> Option<String> {
    rows.facts
        .iter()
        .filter_map(|fact| fact.source_url.as_ref())
        .find(|url| !url.trim().is_empty())
        .cloned()
}

fn fact_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        FactValue::Tags(values) => {
            let joined = values
                .iter()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            (!joined.is_empty()).then_some(joined)
        }
        _ => None,
    }
}

fn numeric_value(value: &FactValue) -> Option<f64> {
    match value {
        FactValue::Numeric(value) => Some(*value),
        FactValue::Score { value, .. } => Some(*value),
        _ => None,
    }
}

fn valid_latitude(value: f64) -> bool {
    value.is_finite() && (-90.0..=90.0).contains(&value)
}

fn valid_longitude(value: f64) -> bool {
    value.is_finite() && (-180.0..=180.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn derives_missing_nearby_place_for_society_from_coordinates() {
        let entities = vec![
            entity("society:test", "society", "Test Society"),
            entity("place:google:existing", "place", "Existing Medical Centre"),
            entity("place:google:derived", "place", "Derived Medical Centre"),
        ];
        let facts = vec![
            coord("society:test", "geo.latitude", 12.985711),
            coord("society:test", "geo.longitude", 77.746842),
            coord("place:google:existing", "geo.latitude", 12.9894945),
            coord("place:google:existing", "geo.longitude", 77.7337373),
            text(
                "place:google:existing",
                "place.name",
                "Existing Medical Centre",
            ),
            tags("place:google:existing", "place.types", &["hospital"]),
            coord("place:google:derived", "geo.latitude", 12.982),
            coord("place:google:derived", "geo.longitude", 77.742),
            text(
                "place:google:derived",
                "place.name",
                "Derived Medical Centre",
            ),
            tags("place:google:derived", "place.types", &["hospital"]),
            text(
                "society:test",
                "nearby_hospitals",
                "Existing Medical Centre (1.5 km, 4.5 rating)",
            ),
        ];
        let index = ServingFactIndex::from_records(facts, Vec::new());

        let derived = derive_proximity_records(&entities, &index, &[]);

        assert!(derived.facts.iter().any(|fact| {
            fact.entity_id == "society:test"
                && fact.fact_key == "nearby_hospitals"
                && fact
                    .value_text
                    .as_deref()
                    .is_some_and(|text| text.contains("Derived Medical Centre"))
        }));
        assert!(!derived.facts.iter().any(|fact| {
            fact.entity_id == "society:test"
                && fact.fact_key == "nearby_hospitals"
                && fact
                    .value_text
                    .as_deref()
                    .is_some_and(|text| text.contains("Existing Medical Centre"))
        }));
        assert!(derived.edges.iter().any(|edge| {
            edge.from_entity_id == "society:test"
                && edge.edge_type == NEAR_PLACE_EDGE
                && edge.to_entity_id == "place:google:derived"
        }));
        assert!(derived.edges.iter().any(|edge| {
            edge.from_entity_id == "society:test"
                && edge.edge_type == NEAR_PLACE_EDGE
                && edge.to_entity_id == "place:google:existing"
        }));
    }

    #[test]
    fn does_not_duplicate_existing_near_place_edges() {
        let entities = vec![
            entity("society:test", "society", "Test Society"),
            entity("place:generic:medical", "place", "Medical Access Point"),
        ];
        let facts = vec![
            coord("society:test", "geo.latitude", 12.985711),
            coord("society:test", "geo.longitude", 77.746842),
            coord("place:generic:medical", "geo.latitude", 12.982),
            coord("place:generic:medical", "geo.longitude", 77.742),
            text(
                "place:generic:medical",
                "place.name",
                "Medical Access Point",
            ),
            tags("place:generic:medical", "place.types", &["hospital"]),
        ];
        let existing_edges = vec![ServingEdgeRecord {
            from_entity_id: "society:test".to_string(),
            edge_type: NEAR_PLACE_EDGE.to_string(),
            to_entity_id: "place:generic:medical".to_string(),
            confidence: 0.9,
            source_type: "Google".to_string(),
        }];
        let index = ServingFactIndex::from_records(facts, Vec::new());

        let derived = derive_proximity_records(&entities, &index, &existing_edges);

        assert!(!derived.edges.iter().any(|edge| {
            edge.from_entity_id == "society:test"
                && edge.edge_type == NEAR_PLACE_EDGE
                && edge.to_entity_id == "place:generic:medical"
        }));
    }

    fn entity(id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: name.to_string(),
        }
    }

    fn coord(entity_id: &str, key: &str, value: f64) -> ServingFactRecord {
        fact(entity_id, key, FactValue::Numeric(value), None)
    }

    fn text(entity_id: &str, key: &str, value: &str) -> ServingFactRecord {
        fact(
            entity_id,
            key,
            FactValue::Text(value.to_string()),
            Some(value.to_string()),
        )
    }

    fn tags(entity_id: &str, key: &str, values: &[&str]) -> ServingFactRecord {
        fact(
            entity_id,
            key,
            FactValue::Tags(values.iter().map(|value| (*value).to_string()).collect()),
            Some(values.join("; ")),
        )
    }

    fn fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        value_text: Option<String>,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text,
            value,
            confidence: 0.9,
            source_type: "test".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc.with_ymd_and_hms(2026, 7, 27, 0, 0, 0).unwrap(),
        }
    }
}
