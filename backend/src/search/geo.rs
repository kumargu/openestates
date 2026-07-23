use std::collections::HashSet;

use crate::knowledge::FactValue;
use crate::models::Property;
use crate::serving::{
    ServingEntityRecord, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
};

use super::resolver::query_contains_lower_text;
use super::schema;

pub(crate) const GEO_DISTANCE_SCORING_METHOD: &str = "serving-geo-distance";
pub(crate) const HAVERSINE_SCORING_METHOD: &str = "serving-haversine";
pub(crate) const DISTANCE_TO_PLACE_FACT_KEY: &str = "geo.distance_to_place";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GeoDistanceScore {
    pub score_delta: f64,
}

pub(crate) fn is_geo_distance_fact_key(fact_key: &str) -> bool {
    schema::ranking_policy()
        .geo_distance_fact_keys
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(fact_key))
}

pub(crate) fn score_serving_geo_distance(
    fact: &ServingFactRecord,
    metadata: &ServingSearchMetadataRecord,
) -> Option<GeoDistanceScore> {
    if !is_geo_distance_fact_key(&fact.fact_key) {
        return None;
    }

    let distance_km = serving_fact_distance_km(fact)?;
    if !distance_km.is_finite() || distance_km < 0.0 {
        return None;
    }

    let weight = f64::from(metadata.scoring_weight.unwrap_or(1.0)).clamp(0.0, 2.0);
    if weight <= 0.0 {
        return None;
    }

    let policy = schema::ranking_policy();
    let proximity = normalized_distance_score(
        distance_km,
        policy.nearby_distance_full_score_km,
        policy.nearby_distance_zero_score_km,
    )?;
    let bonus = proximity * policy.nearby_distance_bonus_cap.max(0.0);
    let score_delta = (weight + bonus).clamp(0.0, 2.0);
    if score_delta <= 0.0 {
        return None;
    }

    Some(GeoDistanceScore { score_delta })
}

pub(crate) fn normalized_distance_score(
    distance_km: f64,
    full_score_km: f64,
    zero_score_km: f64,
) -> Option<f64> {
    if !full_score_km.is_finite()
        || !zero_score_km.is_finite()
        || full_score_km < 0.0
        || zero_score_km <= full_score_km
    {
        return None;
    }

    if distance_km <= full_score_km {
        return Some(1.0);
    }
    if distance_km >= zero_score_km {
        return Some(0.0);
    }

    Some((zero_score_km - distance_km) / (zero_score_km - full_score_km))
}

#[derive(Debug, Clone, Default)]
pub struct GeoSearchIndex {
    places: Vec<GeoPlace>,
    society_coordinates: Vec<EntityCoordinates>,
}

#[derive(Debug, Clone)]
pub struct GeoSearchQuery<'a> {
    index: &'a GeoSearchIndex,
    places: Vec<ResolvedGeoPlace>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedGeoPlace {
    pub entity_id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub confidence: f32,
    pub match_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HaversineEvidence {
    pub place_name: String,
    pub distance_km: f64,
    pub normalized_score: f64,
    pub score_delta: f64,
    pub confidence: f32,
    pub display: String,
}

#[derive(Debug, Clone)]
struct GeoPlace {
    entity_id: String,
    name: String,
    latitude: f64,
    longitude: f64,
    confidence: f32,
    match_tokens: Vec<String>,
}

#[derive(Debug, Clone)]
struct EntityCoordinates {
    entity_id: String,
    latitude: f64,
    longitude: f64,
    confidence: f32,
}

impl GeoSearchIndex {
    pub fn from_serving_bundle(
        entities: &[ServingEntityRecord],
        fact_index: &ServingFactIndex,
    ) -> Self {
        let mut places = Vec::new();
        let mut society_coordinates = Vec::new();
        let mut society_coordinate_ids = HashSet::<String>::new();
        for entity in entities {
            let Some(coordinates) = coordinates_for_entity(fact_index, &entity.entity_id) else {
                continue;
            };
            if entity.entity_type.eq_ignore_ascii_case("place") {
                places.push(GeoPlace {
                    entity_id: entity.entity_id.clone(),
                    name: entity.name.clone(),
                    latitude: coordinates.latitude,
                    longitude: coordinates.longitude,
                    confidence: coordinates.confidence,
                    match_tokens: significant_place_tokens(&entity.name),
                });
            } else if entity.entity_type.eq_ignore_ascii_case("society") {
                society_coordinate_ids.insert(coordinates.entity_id.clone());
                society_coordinates.push(coordinates);
            }
        }
        for (entity_id, _) in fact_index.rows() {
            if !entity_id.starts_with("society:") || society_coordinate_ids.contains(entity_id) {
                continue;
            }
            if let Some(coordinates) = coordinates_for_entity(fact_index, entity_id) {
                society_coordinate_ids.insert(coordinates.entity_id.clone());
                society_coordinates.push(coordinates);
            }
        }
        places.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.entity_id.cmp(&right.entity_id))
        });
        society_coordinates.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        Self {
            places,
            society_coordinates,
        }
    }

    pub(crate) fn query(&self, query: &str) -> Option<GeoSearchQuery<'_>> {
        let places = self.resolve_query_places(query);
        (!places.is_empty()).then_some(GeoSearchQuery {
            index: self,
            places,
        })
    }

    pub fn place_count(&self) -> usize {
        self.places.len()
    }

    pub fn society_coordinate_count(&self) -> usize {
        self.society_coordinates.len()
    }

    fn resolve_query_places(&self, query: &str) -> Vec<ResolvedGeoPlace> {
        let query_lower = query.to_ascii_lowercase();
        let query_tokens = significant_query_tokens(query);
        let mut resolved = self
            .places
            .iter()
            .filter_map(|place| {
                place_query_match_score(place, &query_lower, &query_tokens).map(|match_score| {
                    ResolvedGeoPlace {
                        entity_id: place.entity_id.clone(),
                        name: place.name.clone(),
                        latitude: place.latitude,
                        longitude: place.longitude,
                        confidence: place.confidence,
                        match_score,
                    }
                })
            })
            .collect::<Vec<_>>();
        resolved.sort_by(|left, right| {
            right
                .match_score
                .partial_cmp(&left.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.confidence.total_cmp(&left.confidence))
                .then_with(|| left.name.cmp(&right.name))
        });
        resolved.truncate(3);
        resolved
    }

    fn society_coordinates(&self, society_id: &str) -> Option<&EntityCoordinates> {
        society_entity_id_candidates(society_id)
            .into_iter()
            .find_map(|candidate| {
                self.society_coordinates
                    .binary_search_by(|coordinates| coordinates.entity_id.cmp(&candidate))
                    .ok()
                    .and_then(|index| self.society_coordinates.get(index))
            })
    }
}

fn society_entity_id_candidates(society_id: &str) -> Vec<String> {
    let normalized = society_id
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    let suffix = normalized.strip_prefix("society:").unwrap_or(&normalized);
    let canonical_suffix = suffix.strip_prefix("soc-").unwrap_or(suffix);
    let canonical = format!("society:{canonical_suffix}");
    let legacy = format!("society:{suffix}");

    if legacy == canonical {
        vec![canonical]
    } else {
        vec![canonical, legacy]
    }
}

impl<'a> GeoSearchQuery<'a> {
    pub(crate) fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    pub(crate) fn candidate_property_ids(&self, properties: &[Property]) -> Vec<String> {
        let mut ids = Vec::new();
        let max_distance = schema::ranking_policy().named_place_zero_score_km;
        for property in properties {
            if !property.is_listable() {
                continue;
            }
            let Some(coordinates) = self.index.society_coordinates(&property.society_id) else {
                continue;
            };
            if self.places.iter().any(|place| {
                haversine_km(
                    coordinates.latitude,
                    coordinates.longitude,
                    place.latitude,
                    place.longitude,
                ) <= max_distance
            }) {
                ids.push(property.id.clone());
            }
        }
        ids
    }

    pub(crate) fn evidence_for_society(&self, society_id: &str) -> Option<HaversineEvidence> {
        let coordinates = self.index.society_coordinates(society_id)?;
        self.places
            .iter()
            .filter_map(|place| {
                let distance_km = haversine_km(
                    coordinates.latitude,
                    coordinates.longitude,
                    place.latitude,
                    place.longitude,
                );
                named_place_distance_evidence(place, coordinates, distance_km)
            })
            .max_by(|left, right| {
                left.score_delta
                    .partial_cmp(&right.score_delta)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        right
                            .distance_km
                            .partial_cmp(&left.distance_km)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            })
    }

    pub(crate) fn resolved_places(&self) -> &[ResolvedGeoPlace] {
        &self.places
    }

    pub(crate) fn resolved_place_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for place in &self.places {
            for token in significant_place_tokens(&place.name) {
                if !terms.iter().any(|existing| existing == &token) {
                    terms.push(token);
                }
            }
        }
        terms
    }
}

fn named_place_distance_evidence(
    place: &ResolvedGeoPlace,
    coordinates: &EntityCoordinates,
    distance_km: f64,
) -> Option<HaversineEvidence> {
    if !distance_km.is_finite() || distance_km < 0.0 {
        return None;
    }
    let policy = schema::ranking_policy();
    let normalized_score = normalized_distance_score(
        distance_km,
        policy.named_place_full_score_km,
        policy.named_place_zero_score_km,
    )?;
    if normalized_score <= 0.0 {
        return None;
    }
    let score_delta = normalized_score * policy.named_place_score_weight.max(0.0);
    if score_delta <= 0.0 {
        return None;
    }
    Some(HaversineEvidence {
        place_name: place.name.clone(),
        distance_km,
        normalized_score,
        score_delta,
        confidence: coordinates.confidence.min(place.confidence),
        display: format!("{distance_km:.1} km from {}", place.name),
    })
}

fn coordinates_for_entity(
    fact_index: &ServingFactIndex,
    entity_id: &str,
) -> Option<EntityCoordinates> {
    let rows = fact_index.entity(entity_id)?;
    let latitude = coordinate_fact_value(rows, &["geo.latitude", "project_latitude"])?;
    let longitude = coordinate_fact_value(rows, &["geo.longitude", "project_longitude"])?;
    if !valid_latitude(latitude.value) || !valid_longitude(longitude.value) {
        return None;
    }
    Some(EntityCoordinates {
        entity_id: entity_id.to_string(),
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

fn coordinate_fact_value(
    rows: &crate::serving::ServingEntityFactRows,
    keys: &[&str],
) -> Option<CoordinateValue> {
    keys.iter().find_map(|key| {
        rows.facts
            .iter()
            .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
            .filter_map(|fact| {
                fact_value_numeric(&fact.value).map(|value| CoordinateValue {
                    value,
                    confidence: fact.confidence,
                })
            })
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
    })
}

fn fact_value_numeric(value: &FactValue) -> Option<f64> {
    match value {
        FactValue::Numeric(value) => Some(*value),
        FactValue::Score { value, .. } => Some(*value),
        _ => None,
    }
}

fn place_query_match_score(
    place: &GeoPlace,
    query_lower: &str,
    query_tokens: &[String],
) -> Option<f64> {
    if query_contains_lower_text(query_lower, &place.name) {
        return Some(1.0);
    }
    if place.match_tokens.is_empty() || query_tokens.is_empty() {
        return None;
    }

    let matched = place
        .match_tokens
        .iter()
        .filter(|place_token| {
            query_tokens
                .iter()
                .any(|query_token| token_matches(query_token, place_token))
        })
        .count();
    let required = if place.match_tokens.len() <= 2 {
        place.match_tokens.len()
    } else {
        3
    };
    if matched < required {
        return None;
    }
    let coverage = matched as f64 / place.match_tokens.len() as f64;
    (coverage >= 0.6).then_some(coverage)
}

fn significant_place_tokens(text: &str) -> Vec<String> {
    tokenize(text)
        .into_iter()
        .filter(|token| !is_place_match_stopword(token))
        .collect()
}

fn significant_query_tokens(text: &str) -> Vec<String> {
    tokenize(text)
        .into_iter()
        .filter(|token| !is_query_match_stopword(token))
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_place_match_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "near"
            | "bengaluru"
            | "bangalore"
            | "whitefield"
            | "station"
            | "stop"
            | "road"
    )
}

fn is_query_match_stopword(token: &str) -> bool {
    is_place_match_stopword(token)
        || matches!(
            token,
            "a" | "an"
                | "in"
                | "at"
                | "from"
                | "to"
                | "with"
                | "within"
                | "bhk"
                | "flat"
                | "apartment"
                | "home"
                | "homes"
                | "property"
                | "properties"
        )
}

fn token_matches(query_token: &str, place_token: &str) -> bool {
    query_token == place_token
        || (query_token.len() >= 4
            && place_token.len() >= 4
            && (query_token.starts_with(place_token) || place_token.starts_with(query_token)))
}

pub(crate) fn haversine_km(
    latitude_a: f64,
    longitude_a: f64,
    latitude_b: f64,
    longitude_b: f64,
) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6_371.0088;
    let lat1 = latitude_a.to_radians();
    let lat2 = latitude_b.to_radians();
    let delta_lat = (latitude_b - latitude_a).to_radians();
    let delta_lon = (longitude_b - longitude_a).to_radians();
    let a =
        (delta_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_KM * c
}

fn valid_latitude(value: f64) -> bool {
    value.is_finite() && (-90.0..=90.0).contains(&value)
}

fn valid_longitude(value: f64) -> bool {
    value.is_finite() && (-180.0..=180.0).contains(&value)
}

fn serving_fact_distance_km(fact: &ServingFactRecord) -> Option<f64> {
    match &fact.value {
        FactValue::Numeric(value) => Some(*value),
        FactValue::Score { value, .. } => Some(*value),
        FactValue::Text(value) => extract_first_distance_km(value),
        FactValue::Tags(values) => values
            .iter()
            .find_map(|value| extract_first_distance_km(value)),
        FactValue::Bool(_) => None,
    }
    .or_else(|| {
        fact.value_text
            .as_deref()
            .and_then(extract_first_distance_km)
    })
}

pub(crate) fn extract_first_distance_km(text: &str) -> Option<f64> {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if let Some(distance) = compact_distance_token_km(token) {
            return Some(distance);
        }

        let unit = clean_unit_token(token);
        if unit.is_empty() || index == 0 {
            continue;
        }
        let Some(value) = parse_number_token(tokens[index - 1]) else {
            continue;
        };
        if is_km_unit(&unit) {
            return Some(value);
        }
        if is_meter_unit(&unit) {
            return Some(value / 1000.0);
        }
    }
    None
}

fn compact_distance_token_km(token: &str) -> Option<f64> {
    let token = token
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_ascii_lowercase();
    let unit_start = token.find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))?;
    let (number, unit) = token.split_at(unit_start);
    let value = number.parse::<f64>().ok()?;
    if is_km_unit(unit) {
        Some(value)
    } else if is_meter_unit(unit) {
        Some(value / 1000.0)
    } else {
        None
    }
}

fn parse_number_token(token: &str) -> Option<f64> {
    token
        .trim_matches(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .parse::<f64>()
        .ok()
}

fn clean_unit_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| !ch.is_ascii_alphabetic())
        .to_ascii_lowercase()
}

fn is_km_unit(unit: &str) -> bool {
    matches!(unit, "km" | "kms" | "kilometer" | "kilometers")
}

fn is_meter_unit(unit: &str) -> bool {
    matches!(unit, "m" | "meter" | "meters" | "metre" | "metres")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_distance_from_google_nearby_display() {
        let text = "Nearby metro: Kadugodi Tree Park (0.7 km, 4.5 rating, 1509 reviews)";

        assert_eq!(extract_first_distance_km(text), Some(0.7));
    }

    #[test]
    fn extracts_compact_meter_distance_as_km() {
        let text = "Nearby school: Example Public School (850m, 4.2 rating)";

        assert_eq!(extract_first_distance_km(text), Some(0.85));
    }

    #[test]
    fn ignores_ratings_and_reviews_without_distance_units() {
        let text = "Nearby gym: Example Fitness (4.5 rating, 1509 reviews)";

        assert_eq!(extract_first_distance_km(text), None);
    }

    #[test]
    fn haversine_distance_matches_nearby_bangalore_points() {
        let distance = haversine_km(12.985711, 77.746842, 12.9894945, 77.7337373);

        assert!(
            (1.45..=1.50).contains(&distance),
            "expected Kadugodi-to-Aster distance around 1.47 km, got {distance}"
        );
    }

    #[test]
    fn society_coordinate_lookup_normalizes_runtime_soc_prefix() {
        let index = GeoSearchIndex {
            places: Vec::new(),
            society_coordinates: vec![EntityCoordinates {
                entity_id: "society:sumadhura-capitol-residences".to_string(),
                latitude: 12.98535765887552,
                longitude: 77.75078040700681,
                confidence: 1.0,
            }],
        };

        let coordinates = index
            .society_coordinates("soc-sumadhura-capitol-residences")
            .expect("runtime society ids should resolve to canonical serving facts");

        assert_eq!(
            coordinates.entity_id,
            "society:sumadhura-capitol-residences"
        );
    }
}
