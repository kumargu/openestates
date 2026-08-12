use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use serde::Deserialize;

use crate::dag_config::{
    dag_root, load_fact_registry_index, load_json, scoring_direction_from_hint,
    CoordinateEntityScope, FactRegistryEntry,
};
use crate::knowledge::FactValue;
use crate::search::geo::haversine_km;
use crate::search::{analyzer, schema};

use super::{
    resolve_serving_coordinates, ServingEdgeRecord, ServingEntityFactRows, ServingEntityRecord,
    ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
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
    display_template: Option<String>,
    answers_preferences: Vec<String>,
    scoring_direction: Option<String>,
    scoring_weight: Option<f32>,
    scoring_thresholds: Vec<f64>,
    matcher: ProximityMatcher,
    chainable: bool,
}

#[derive(Debug, Clone)]
enum ProximityMatcher {
    Category(CategoryMatcher),
    Tokens(HashSet<String>),
}

#[derive(Debug, Clone)]
struct CategoryMatcher {
    category_aliases: Vec<String>,
    accepted_place_types: Vec<String>,
    name_markers: Vec<String>,
    name_block_markers: Vec<String>,
    require_name_marker: bool,
    allow_missing_place_types: bool,
}

#[derive(Debug, Clone)]
struct EntityPoint {
    entity_id: String,
    name: String,
    latitude: f64,
    longitude: f64,
    confidence: f32,
    learned_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PlacePoint {
    point: EntityPoint,
    place_types: Vec<String>,
    category: Option<String>,
    fallback_match_tokens: HashSet<String>,
    source_url: Option<String>,
}

#[derive(Debug, Clone)]
struct NearbyPlaceCandidate<'a> {
    place: &'a PlacePoint,
    spec: &'a ProximityFactSpec,
    distance_km: f64,
    confidence: f32,
}

#[derive(Debug, Clone)]
struct IndexedPlace {
    point: [f64; 2],
    index: usize,
}

impl RTreeObject for IndexedPlace {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for IndexedPlace {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let longitude_delta = self.point[0] - point[0];
        let latitude_delta = self.point[1] - point[1];
        longitude_delta.mul_add(longitude_delta, latitude_delta * latitude_delta)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct NearbyPlaceCategoryFile {
    categories: Vec<NearbyPlaceCategory>,
}

#[derive(Debug, Clone, Deserialize)]
struct NearbyPlaceCategory {
    fact_key: String,
    #[serde(default)]
    relation_class: Option<String>,
    #[serde(default)]
    category_aliases: Vec<String>,
    max_distance_km: f64,
    #[serde(default)]
    allow_missing_place_types: bool,
    #[serde(default)]
    accepted_place_types: Vec<String>,
    #[serde(default)]
    name_markers: Vec<String>,
    #[serde(default)]
    name_block_markers: Vec<String>,
    #[serde(default)]
    require_name_marker: bool,
    #[serde(default)]
    chainable: Option<bool>,
}

impl NearbyPlaceCategoryFile {
    fn category_for_fact_key(&self, fact_key: &str) -> Option<&NearbyPlaceCategory> {
        self.categories
            .iter()
            .find(|category| category.fact_key == fact_key)
    }
}

pub fn derive_proximity_records(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    existing_edges: &[ServingEdgeRecord],
) -> Result<DerivedProximityRecords, crate::dag_config::DagConfigError> {
    let specs = proximity_fact_specs()?;
    if specs.is_empty() {
        return Ok(DerivedProximityRecords::default());
    }
    let target_entities = target_entity_points(entities, fact_index);
    let places = place_points(entities, fact_index);
    if target_entities.is_empty() || places.is_empty() {
        return Ok(DerivedProximityRecords::default());
    }

    let existing_mentions = existing_nearby_mentions(fact_index, &specs);
    let mut output = DerivedProximityRecords::default();
    let mut seen_edges = existing_near_place_edges(existing_edges);
    let place_index = indexed_places(&places);
    let max_distance_km = specs
        .iter()
        .map(|spec| spec.max_distance_km)
        .filter(|value| value.is_finite() && *value > 0.0)
        .fold(0.0_f64, f64::max);

    for target in &target_entities {
        let mut by_fact_key = HashMap::<&str, Vec<NearbyPlaceCandidate<'_>>>::new();
        for place in nearest_candidate_places(target, &places, &place_index, max_distance_km) {
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

                if candidate.spec.chainable
                    && seen_edges.insert((
                        target.entity_id.clone(),
                        candidate.place.point.entity_id.clone(),
                    ))
                {
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

    Ok(output)
}

fn existing_near_place_edges(existing_edges: &[ServingEdgeRecord]) -> HashSet<(String, String)> {
    existing_edges
        .iter()
        .filter(|edge| edge.edge_type.eq_ignore_ascii_case(NEAR_PLACE_EDGE))
        .map(|edge| (edge.from_entity_id.clone(), edge.to_entity_id.clone()))
        .collect()
}

fn indexed_places(places: &[PlacePoint]) -> RTree<IndexedPlace> {
    RTree::bulk_load(
        places
            .iter()
            .enumerate()
            .map(|(index, place)| IndexedPlace {
                point: [place.point.longitude, place.point.latitude],
                index,
            })
            .collect(),
    )
}

fn nearest_candidate_places<'a>(
    target: &EntityPoint,
    places: &'a [PlacePoint],
    place_index: &RTree<IndexedPlace>,
    max_distance_km: f64,
) -> Vec<&'a PlacePoint> {
    if max_distance_km <= 0.0 {
        return Vec::new();
    }
    let lat_delta = km_to_lat_degrees(max_distance_km);
    let lng_delta = km_to_lng_degrees(max_distance_km, target.latitude);
    let max_planar_distance_2 = lat_delta.mul_add(lat_delta, lng_delta * lng_delta);
    let target_point = [target.longitude, target.latitude];
    place_index
        .nearest_neighbor_iter(&target_point)
        .take_while(|indexed| indexed.distance_2(&target_point) <= max_planar_distance_2)
        .filter_map(|indexed| places.get(indexed.index))
        .collect()
}

fn km_to_lat_degrees(distance_km: f64) -> f64 {
    distance_km / 110.574
}

fn km_to_lng_degrees(distance_km: f64, latitude: f64) -> f64 {
    let latitude_scale = latitude.to_radians().cos().abs().max(0.01);
    distance_km / (111.320 * latitude_scale)
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
        learned_at: latest_datetime(society.learned_at, candidate.place.point.learned_at),
    }
}

fn latest_datetime(left: DateTime<Utc>, right: DateTime<Utc>) -> DateTime<Utc> {
    left.max(right)
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
            let fallback_match_tokens =
                place_match_tokens(&point.name, &place_types, category.as_deref());
            Some(PlacePoint {
                point,
                place_types,
                category,
                fallback_match_tokens,
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

fn proximity_fact_specs() -> Result<Vec<ProximityFactSpec>, crate::dag_config::DagConfigError> {
    let policy = schema::ranking_policy();
    let registry = load_fact_registry_index().ok();
    let category_config = load_nearby_place_category_config()?;
    let mut specs = Vec::new();
    for fact_key in &policy.geo_distance_fact_keys {
        let registry_entry = registry
            .as_ref()
            .and_then(|index| index.lookup(fact_key))
            .cloned();
        let Some(category) = category_config.category_for_fact_key(fact_key) else {
            return Err(crate::dag_config::DagConfigError::InvalidConfig(format!(
                "geo distance fact key {fact_key} is missing from nearby_place_categories.json"
            )));
        };
        if let Some(spec) = proximity_fact_spec(
            fact_key,
            policy.named_place_zero_score_km,
            registry_entry,
            Some(category),
        ) {
            specs.push(spec);
        }
    }
    remove_cross_category_tokens(&mut specs);
    Ok(specs
        .into_iter()
        .filter(|spec| match &spec.matcher {
            ProximityMatcher::Category(_) => true,
            ProximityMatcher::Tokens(tokens) => !tokens.is_empty(),
        })
        .collect())
}

fn proximity_fact_spec(
    fact_key: &str,
    fallback_max_distance_km: f64,
    registry_entry: Option<FactRegistryEntry>,
    category: Option<&NearbyPlaceCategory>,
) -> Option<ProximityFactSpec> {
    let max_distance_km = category
        .map(|category| category.max_distance_km)
        .unwrap_or(fallback_max_distance_km);
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
    let fallback_match_tokens = token_set(seed_text.join(" "));
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
    let matcher = category
        .map(|category| {
            ProximityMatcher::Category(CategoryMatcher {
                category_aliases: normalized_values(&category.category_aliases),
                accepted_place_types: normalized_values(&category.accepted_place_types),
                name_markers: normalized_values(&category.name_markers),
                name_block_markers: normalized_values(&category.name_block_markers),
                require_name_marker: category.require_name_marker,
                allow_missing_place_types: category.allow_missing_place_types,
            })
        })
        .unwrap_or(ProximityMatcher::Tokens(fallback_match_tokens));
    Some(ProximityFactSpec {
        fact_key: fact_key.to_string(),
        max_distance_km,
        display_template,
        answers_preferences,
        scoring_direction,
        scoring_weight,
        scoring_thresholds,
        matcher,
        chainable: category.map(category_chainable).unwrap_or(true),
    })
}

fn category_chainable(category: &NearbyPlaceCategory) -> bool {
    if category
        .relation_class
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("risk_externality"))
    {
        return false;
    }
    category.chainable.unwrap_or(true)
}

fn remove_cross_category_tokens(specs: &mut [ProximityFactSpec]) {
    let mut token_counts = HashMap::<String, usize>::new();
    for spec in specs.iter() {
        if let ProximityMatcher::Tokens(tokens) = &spec.matcher {
            for token in tokens {
                *token_counts.entry(token.clone()).or_default() += 1;
            }
        }
    }
    for spec in specs {
        if let ProximityMatcher::Tokens(tokens) = &mut spec.matcher {
            tokens.retain(|token| token_counts.get(token).copied().unwrap_or_default() == 1);
        }
    }
}

fn load_nearby_place_category_config(
) -> Result<NearbyPlaceCategoryFile, crate::dag_config::DagConfigError> {
    load_json(&dag_root().join("nearby_place_categories.json"))
}

fn normalized_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_category_key(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_category_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn normalize_category_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn contains_category_text(haystack: &str, needle: &str) -> bool {
    if needle.trim().is_empty() {
        return false;
    }
    haystack.contains(&needle.replace('_', " ")) || haystack.replace(' ', "_").contains(needle)
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
    let scope = if entity_id.starts_with("place:") {
        CoordinateEntityScope::Place
    } else {
        CoordinateEntityScope::Society
    };
    let coordinates = resolve_serving_coordinates(rows, scope)?;
    Some(EntityPoint {
        entity_id: entity_id.to_string(),
        name: text_fact(rows, "place.name")
            .or_else(|| text_fact(rows, "listing_society"))
            .or_else(|| text_fact(rows, "rera_project_name"))
            .unwrap_or_else(|| fallback_name.to_string()),
        latitude: coordinates.latitude,
        longitude: coordinates.longitude,
        confidence: coordinates.confidence,
        learned_at: coordinates.learned_at,
    })
}

fn place_matches_spec(place: &PlacePoint, spec: &ProximityFactSpec) -> bool {
    match &spec.matcher {
        ProximityMatcher::Category(category) => place_matches_category(place, category),
        ProximityMatcher::Tokens(tokens) => {
            if tokens.is_empty() || place.fallback_match_tokens.is_empty() {
                return false;
            }
            tokens.is_subset(&place.fallback_match_tokens)
        }
    }
}

fn place_matches_category(place: &PlacePoint, category: &CategoryMatcher) -> bool {
    let place_name = normalize_category_text(&place.point.name);
    if category
        .name_block_markers
        .iter()
        .any(|blocked| contains_category_text(&place_name, blocked))
    {
        return false;
    }
    let name_marker_match = category
        .name_markers
        .iter()
        .any(|marker| contains_category_text(&place_name, marker));
    if category.require_name_marker {
        return name_marker_match;
    }

    let place_types = place
        .place_types
        .iter()
        .map(|value| normalize_category_key(value))
        .collect::<HashSet<_>>();
    let place_category = place.category.as_deref().map(normalize_category_key);

    if let Some(place_category) = place_category.as_ref() {
        if category
            .category_aliases
            .iter()
            .any(|alias| alias == place_category)
        {
            return true;
        }
    }
    if category
        .accepted_place_types
        .iter()
        .any(|accepted| place_types.contains(accepted))
    {
        return true;
    }
    if name_marker_match {
        return true;
    }
    if category.allow_missing_place_types && place_types.is_empty() {
        return category
            .category_aliases
            .iter()
            .any(|alias| contains_category_text(&place_name, alias));
    }
    false
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

        let derived = derive_proximity_records(&entities, &index, &[]).unwrap();

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
    fn nearest_candidates_still_emit_facts_by_haversine_distance() {
        let entities = vec![
            entity("society:test", "society", "Test Society"),
            entity("place:hospital:far", "place", "Far Hospital"),
            entity("place:hospital:near", "place", "Near Hospital"),
        ];
        let facts = vec![
            coord("society:test", "geo.latitude", 12.985711),
            coord("society:test", "geo.longitude", 77.746842),
            coord("place:hospital:far", "geo.latitude", 12.995),
            coord("place:hospital:far", "geo.longitude", 77.756),
            text("place:hospital:far", "place.name", "Far Hospital"),
            tags("place:hospital:far", "place.types", &["hospital"]),
            coord("place:hospital:near", "geo.latitude", 12.986),
            coord("place:hospital:near", "geo.longitude", 77.747),
            text("place:hospital:near", "place.name", "Near Hospital"),
            tags("place:hospital:near", "place.types", &["hospital"]),
        ];
        let index = ServingFactIndex::from_records(facts, Vec::new());

        let derived = derive_proximity_records(&entities, &index, &[]).unwrap();
        let hospital_facts = derived
            .facts
            .iter()
            .filter(|fact| fact.fact_key == "nearby_hospitals")
            .filter_map(|fact| fact.value_text.as_deref())
            .collect::<Vec<_>>();

        assert!(hospital_facts.len() >= 2);
        assert!(hospital_facts[0].contains("Near Hospital"));
        assert!(hospital_facts[1].contains("Far Hospital"));
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

        let derived = derive_proximity_records(&entities, &index, &existing_edges).unwrap();

        assert!(!derived.edges.iter().any(|edge| {
            edge.from_entity_id == "society:test"
                && edge.edge_type == NEAR_PLACE_EDGE
                && edge.to_entity_id == "place:generic:medical"
        }));
    }

    #[test]
    fn uses_category_radius_policy_instead_of_global_named_place_radius() {
        let entities = vec![
            entity("society:test", "society", "Test Society"),
            entity("place:tech:far-but-valid", "place", "Valid Tech Park"),
            entity("place:park:too-far", "place", "Too Far Public Park"),
        ];
        let facts = vec![
            coord("society:test", "geo.latitude", 12.985711),
            coord("society:test", "geo.longitude", 77.746842),
            coord("place:tech:far-but-valid", "geo.latitude", 12.985711),
            coord("place:tech:far-but-valid", "geo.longitude", 77.84),
            text("place:tech:far-but-valid", "place.name", "Valid Tech Park"),
            tags(
                "place:tech:far-but-valid",
                "place.types",
                &["corporate_office"],
            ),
            coord("place:park:too-far", "geo.latitude", 12.985711),
            coord("place:park:too-far", "geo.longitude", 77.786),
            text("place:park:too-far", "place.name", "Too Far Public Park"),
            tags("place:park:too-far", "place.types", &["park"]),
        ];
        let index = ServingFactIndex::from_records(facts, Vec::new());

        let derived = derive_proximity_records(&entities, &index, &[]).unwrap();

        assert!(derived.facts.iter().any(|fact| {
            fact.entity_id == "society:test"
                && fact.fact_key == "nearby_tech_parks"
                && fact
                    .value_text
                    .as_deref()
                    .is_some_and(|text| text.contains("Valid Tech Park"))
        }));
        assert!(!derived.facts.iter().any(|fact| {
            fact.entity_id == "society:test"
                && fact.fact_key == "nearby_public_parks"
                && fact
                    .value_text
                    .as_deref()
                    .is_some_and(|text| text.contains("Too Far Public Park"))
        }));
    }

    #[test]
    fn risk_externality_categories_are_direct_only() {
        let config = load_nearby_place_category_config().unwrap();
        let category = config.category_for_fact_key("nearby_lakes").unwrap();
        let spec = proximity_fact_spec("nearby_lakes", 5.0, None, Some(category)).unwrap();

        assert!(!spec.chainable);
        assert_eq!(spec.max_distance_km, 1.0);
    }

    #[test]
    fn category_can_require_name_marker_over_google_type() {
        let matcher = CategoryMatcher {
            category_aliases: vec!["fitness".to_string()],
            accepted_place_types: vec!["gym".to_string()],
            name_markers: vec!["cult".to_string(), "cult fit".to_string()],
            name_block_markers: Vec::new(),
            require_name_marker: true,
            allow_missing_place_types: true,
        };

        assert!(!place_matches_category(
            &place_point("Generic Premium Gym", &["gym"]),
            &matcher
        ));
        assert!(place_matches_category(
            &place_point("Cult Whitefield", &["gym"]),
            &matcher
        ));
    }

    fn entity(id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            aliases: Vec::new(),
            searchable_text: name.to_string(),
        }
    }

    fn place_point(name: &str, place_types: &[&str]) -> PlacePoint {
        PlacePoint {
            point: EntityPoint {
                entity_id: format!("place:test:{}", name.replace(' ', "-").to_lowercase()),
                name: name.to_string(),
                latitude: 12.98,
                longitude: 77.75,
                confidence: 0.9,
                learned_at: Utc.with_ymd_and_hms(2026, 7, 27, 0, 0, 0).unwrap(),
            },
            place_types: place_types
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            category: None,
            fallback_match_tokens: HashSet::new(),
            source_url: None,
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
            source_type: if entity_id.starts_with("place:") {
                "OpenStreetMap"
            } else {
                "Google"
            }
            .to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc.with_ymd_and_hms(2026, 7, 27, 0, 0, 0).unwrap(),
        }
    }
}
