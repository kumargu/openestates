use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::dag_config::{
    nearby_place_categories_config, nearby_place_fact_key_matches_category,
    requested_nearby_place_categories, search_resolution_config, CoordinateEntityScope,
};
use crate::knowledge::FactValue;
use crate::models::Property;
use crate::serving::{
    resolve_serving_coordinates, ServingEntityRecord, ServingFactIndex, ServingFactRecord,
    ServingSearchMetadataRecord,
};

use super::analyzer;
use super::parser;
use super::resolver::query_contains_lower_text;
use super::schema;

pub(crate) const GEO_DISTANCE_SCORING_METHOD: &str = "serving-geo-distance";
pub(crate) const HAVERSINE_SCORING_METHOD: &str = "serving-haversine";
pub(crate) const NAMED_PLACE_FACT_SCORING_METHOD: &str = "serving-named-place";
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
    clauses: Vec<ResolvedGeoClause>,
    unresolved_targets: Vec<String>,
    max_distance_km: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedGeoClause {
    pub target_text: String,
    pub place_entity_ids: Vec<String>,
    pub category_fact_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedGeoPlace {
    pub entity_id: String,
    pub name: String,
    pub category: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub confidence: f32,
    pub match_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HaversineEvidence {
    pub place_entity_id: String,
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
    category: Option<String>,
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
            if entity.entity_type.eq_ignore_ascii_case("place")
                || entity.entity_type.eq_ignore_ascii_case("area")
            {
                places.push(GeoPlace {
                    entity_id: entity.entity_id.clone(),
                    name: entity.name.clone(),
                    category: place_category_for_entity(fact_index, &entity.entity_id),
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
        let parsed_slots = parser::parse_query_slots(query);
        if parsed_slots.relations.is_empty() {
            return None;
        }
        let tokens = parser::query_tokens(query);
        let mut places = Vec::new();
        let mut clauses = Vec::new();
        let mut unresolved_targets = Vec::new();
        for relation in &parsed_slots.relations {
            let Some(target_text) = parser::relation_target_text(&tokens, relation) else {
                continue;
            };
            let resolved = self.resolve_query_places(&target_text);
            let category_fact_keys = if resolved.is_empty()
                && !has_scoped_suffix(&target_text)
                && has_configured_place_family(&target_text)
            {
                requested_nearby_place_categories(&target_text.to_ascii_lowercase())
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            if resolved.is_empty() && category_fact_keys.is_empty() {
                unresolved_targets.push(target_text);
                continue;
            }
            let mut place_entity_ids = Vec::new();
            for place in resolved {
                if !place_entity_ids.iter().any(|id| id == &place.entity_id) {
                    place_entity_ids.push(place.entity_id.clone());
                }
                if !places
                    .iter()
                    .any(|existing: &ResolvedGeoPlace| existing.entity_id == place.entity_id)
                {
                    places.push(place);
                }
            }
            clauses.push(ResolvedGeoClause {
                target_text,
                place_entity_ids,
                category_fact_keys,
            });
        }
        let max_distance_km = (parsed_slots.relations.len() == 1)
            .then(|| {
                parsed_slots
                    .distance_limit
                    .as_ref()
                    .map(|distance| distance.value_km)
            })
            .flatten();
        (!clauses.is_empty()).then_some(GeoSearchQuery {
            index: self,
            places,
            clauses,
            unresolved_targets,
            max_distance_km,
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
        let mut exact_resolved = self
            .places
            .iter()
            .filter(|place| !place.match_tokens.is_empty())
            .filter(|place| !area_is_only_a_scoped_suffix(query, place))
            .filter(|place| query_contains_lower_text(&query_lower, &place.name))
            .map(|place| ResolvedGeoPlace {
                entity_id: place.entity_id.clone(),
                name: place.name.clone(),
                category: place.category.clone(),
                latitude: place.latitude,
                longitude: place.longitude,
                confidence: place.confidence,
                match_score: 1.0,
            })
            .collect::<Vec<_>>();
        if !exact_resolved.is_empty() {
            remove_exact_places_contained_in_longer_match(&mut exact_resolved);
            exact_resolved.sort_by(|left, right| {
                right
                    .name
                    .len()
                    .cmp(&left.name.len())
                    .then_with(|| right.confidence.total_cmp(&left.confidence))
                    .then_with(|| left.name.cmp(&right.name))
            });
            exact_resolved.truncate(3);
            return exact_resolved;
        }

        let token_document_counts = place_token_document_counts(&self.places);
        let mut resolved = self
            .places
            .iter()
            .filter(|place| !area_is_only_a_scoped_suffix(query, place))
            .filter_map(|place| {
                place_query_match_score(
                    place,
                    &query_lower,
                    &query_tokens,
                    &token_document_counts,
                    self.places.len(),
                )
                .map(|match_score| ResolvedGeoPlace {
                    entity_id: place.entity_id.clone(),
                    name: place.name.clone(),
                    category: place.category.clone(),
                    latitude: place.latitude,
                    longitude: place.longitude,
                    confidence: place.confidence,
                    match_score,
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

fn area_is_only_a_scoped_suffix(query: &str, place: &GeoPlace) -> bool {
    place.entity_id.starts_with("area:")
        && has_scoped_suffix(query)
        && !query.trim().eq_ignore_ascii_case(place.name.trim())
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
        self.clauses.is_empty()
    }

    pub(crate) fn candidate_property_ids(&self, properties: &[Property]) -> Vec<String> {
        let mut ids = Vec::new();
        let max_distance = self
            .max_distance_km
            .unwrap_or_else(|| schema::ranking_policy().named_place_zero_score_km);
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

    pub(crate) fn serving_fact_candidate_property_ids(
        &self,
        properties: &[Property],
        fact_index: &ServingFactIndex,
    ) -> Vec<String> {
        let mut ids = Vec::new();
        for property in properties {
            if !property.is_listable()
                || ids.iter().any(|existing| existing == &property.id)
                || !self.society_has_matching_nearby_fact(&property.society_id, fact_index)
            {
                continue;
            }
            ids.push(property.id.clone());
        }
        ids
    }

    fn society_has_matching_nearby_fact(
        &self,
        society_id: &str,
        fact_index: &ServingFactIndex,
    ) -> bool {
        let Some(rows) = society_entity_id_candidates(society_id)
            .into_iter()
            .find_map(|entity_id| fact_index.entity(&entity_id))
        else {
            return false;
        };

        self.clauses.iter().any(|clause| {
            rows.facts.iter().any(|fact| {
                fact.confidence >= schema::ranking_policy().min_support_evidence_confidence
                    && (clause.category_fact_keys.iter().any(|fact_key| {
                        fact.fact_key.eq_ignore_ascii_case(fact_key)
                            && serving_fact_distance_km(fact).is_some_and(|distance| {
                                distance <= category_max_distance_km(fact_key)
                            })
                    }) || serving_fact_text_snippets(fact).iter().any(|snippet| {
                        self.places_for_clause(clause).any(|place| {
                            self.fact_key_matches_resolved_place(&fact.fact_key, place)
                                && nearby_fact_mentions_place(snippet, &place.name)
                                && extract_first_distance_km(snippet).is_some_and(|distance_km| {
                                    self.max_distance_km.unwrap_or_else(|| {
                                        schema::ranking_policy().named_place_zero_score_km
                                    }) >= distance_km
                                })
                        })
                    }))
            })
        })
    }

    pub(crate) fn evidence_for_society(&self, society_id: &str) -> Vec<HaversineEvidence> {
        let Some(coordinates) = self.index.society_coordinates(society_id) else {
            return Vec::new();
        };
        self.clauses
            .iter()
            .filter_map(|clause| {
                self.places_for_clause(clause)
                    .filter_map(|place| {
                        let distance_km = haversine_km(
                            coordinates.latitude,
                            coordinates.longitude,
                            place.latitude,
                            place.longitude,
                        );
                        if self
                            .max_distance_km
                            .is_some_and(|max_distance| distance_km > max_distance)
                        {
                            return None;
                        }
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
            })
            .collect()
    }

    pub(crate) fn resolved_places(&self) -> &[ResolvedGeoPlace] {
        &self.places
    }

    pub(crate) fn resolved_clauses(&self) -> &[ResolvedGeoClause] {
        &self.clauses
    }

    pub(crate) fn unresolved_targets(&self) -> &[String] {
        &self.unresolved_targets
    }

    pub(crate) fn places_for_clause<'b>(
        &'b self,
        clause: &'b ResolvedGeoClause,
    ) -> impl Iterator<Item = &'b ResolvedGeoPlace> + 'b {
        self.places.iter().filter(|place| {
            clause
                .place_entity_ids
                .iter()
                .any(|entity_id| entity_id == &place.entity_id)
        })
    }

    pub(crate) fn fact_key_matches_resolved_place(
        &self,
        fact_key: &str,
        place: &ResolvedGeoPlace,
    ) -> bool {
        place.category.as_deref().map_or_else(
            || is_geo_distance_fact_key(fact_key),
            |category| nearby_place_fact_key_matches_category(fact_key, category),
        )
    }

    pub(crate) fn max_distance_km(&self) -> Option<f64> {
        self.max_distance_km
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

fn has_scoped_suffix(target: &str) -> bool {
    let target_lower = target.to_ascii_lowercase();
    search_resolution_config()
        .named_entity_scope_prefixes
        .iter()
        .any(|prefix| query_contains_lower_text(&target_lower, prefix))
}

fn has_configured_place_family(target: &str) -> bool {
    let target_lower = target.to_ascii_lowercase();
    search_resolution_config()
        .place_families
        .iter()
        .flat_map(|family| family.aliases.iter())
        .any(|alias| query_contains_lower_text(&target_lower, alias))
}

fn category_max_distance_km(fact_key: &str) -> f64 {
    nearby_place_categories_config()
        .categories
        .iter()
        .find(|category| category.fact_key.eq_ignore_ascii_case(fact_key))
        .and_then(|category| category.max_distance_km)
        .unwrap_or_else(|| schema::ranking_policy().named_place_zero_score_km)
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
        place_entity_id: place.entity_id.clone(),
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
    let scope = if entity_id.starts_with("place:") {
        CoordinateEntityScope::Place
    } else if entity_id.starts_with("area:") {
        CoordinateEntityScope::Area
    } else {
        CoordinateEntityScope::Society
    };
    let coordinates = resolve_serving_coordinates(rows, scope)?;
    Some(EntityCoordinates {
        entity_id: entity_id.to_string(),
        latitude: coordinates.latitude,
        longitude: coordinates.longitude,
        confidence: coordinates.confidence,
    })
}

fn place_category_for_entity(fact_index: &ServingFactIndex, entity_id: &str) -> Option<String> {
    fact_index
        .entity(entity_id)?
        .facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case("place.category"))
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => {
                Some((value.trim(), fact.confidence, fact.learned_at))
            }
            FactValue::Tags(values) => values
                .iter()
                .find(|value| !value.trim().is_empty())
                .map(|value| (value.trim(), fact.confidence, fact.learned_at)),
            _ => None,
        })
        .max_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.2.cmp(&right.2))
        })
        .map(|(category, _, _)| category.to_string())
}

fn remove_exact_places_contained_in_longer_match(places: &mut Vec<ResolvedGeoPlace>) {
    let names = places
        .iter()
        .map(|place| (place.entity_id.clone(), place.name.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    places.retain(|candidate| {
        !names.iter().any(|(entity_id, name)| {
            entity_id != &candidate.entity_id
                && name.len() > candidate.name.len()
                && query_contains_lower_text(name, &candidate.name)
        })
    });
}

fn place_query_match_score(
    place: &GeoPlace,
    query_lower: &str,
    query_tokens: &[String],
    token_document_counts: &HashMap<String, usize>,
    place_count: usize,
) -> Option<f64> {
    if place.match_tokens.is_empty() || query_tokens.is_empty() {
        return None;
    }
    if query_contains_lower_text(query_lower, &place.name) {
        return Some(1.0);
    }

    let matched_tokens = place
        .match_tokens
        .iter()
        .filter(|place_token| {
            query_tokens
                .iter()
                .any(|query_token| token_matches(query_token, place_token))
        })
        .collect::<Vec<_>>();
    let matched = matched_tokens.len();
    if matched == 0 {
        return None;
    }
    let distinctive_matched = matched_tokens
        .iter()
        .filter(|token| is_distinctive_place_token(token, token_document_counts, place_count))
        .count();
    if distinctive_matched == 0 {
        return None;
    }

    let required = if query_tokens.len() == 1 {
        1
    } else {
        place.match_tokens.len().min(2)
    };
    if matched < required {
        return None;
    }
    let coverage = matched as f64 / place.match_tokens.len() as f64;
    Some(coverage)
}

fn place_token_document_counts(places: &[GeoPlace]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for place in places {
        let mut seen = HashSet::new();
        for token in &place.match_tokens {
            if seen.insert(token) {
                *counts.entry(token.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn is_distinctive_place_token(
    token: &str,
    token_document_counts: &HashMap<String, usize>,
    place_count: usize,
) -> bool {
    let Some(count) = token_document_counts.get(token).copied() else {
        return false;
    };
    let policy = schema::ranking_policy();
    count == 1
        || (count <= policy.named_place_distinctive_token_max_place_count
            && place_count > 0
            && (count as f64 / place_count as f64)
                <= policy.named_place_distinctive_token_max_place_ratio)
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
        .filter(|token| {
            !is_query_match_stopword(token)
                && token
                    .chars()
                    .any(|character| character.is_ascii_alphabetic())
        })
        .collect()
}

fn tokenize(text: &str) -> Vec<String> {
    analyzer::stemmed_tokens(text)
}

fn is_place_match_stopword(token: &str) -> bool {
    configured_named_place_generic_tokens().contains(token)
}

fn is_query_match_stopword(token: &str) -> bool {
    is_place_match_stopword(token) || configured_named_place_query_stopwords().contains(token)
}

fn configured_named_place_generic_tokens() -> &'static HashSet<String> {
    static TOKENS: OnceLock<HashSet<String>> = OnceLock::new();
    TOKENS.get_or_init(|| {
        configured_stemmed_tokens(&schema::ranking_policy().named_place_generic_tokens)
    })
}

fn configured_named_place_query_stopwords() -> &'static HashSet<String> {
    static TOKENS: OnceLock<HashSet<String>> = OnceLock::new();
    TOKENS.get_or_init(|| {
        configured_stemmed_tokens(&schema::ranking_policy().named_place_query_stopwords)
    })
}

fn configured_stemmed_tokens(terms: &[String]) -> HashSet<String> {
    terms
        .iter()
        .flat_map(|term| tokenize(term))
        .collect::<HashSet<_>>()
}

fn token_matches(query_token: &str, place_token: &str) -> bool {
    query_token == place_token
        || (query_token.len() >= 4
            && place_token.len() >= 4
            && (query_token.starts_with(place_token) || place_token.starts_with(query_token)))
}

pub(crate) fn serving_fact_text_snippets(fact: &ServingFactRecord) -> Vec<String> {
    let mut snippets = Vec::new();
    match &fact.value {
        FactValue::Text(value) => snippets.push(value.clone()),
        FactValue::Tags(values) => snippets.extend(values.iter().cloned()),
        FactValue::Score { explanation, .. } => snippets.push(explanation.clone()),
        FactValue::Numeric(_) | FactValue::Bool(_) => {}
    }
    if let Some(value_text) = fact.value_text.as_deref() {
        if !snippets.iter().any(|snippet| snippet == value_text) {
            snippets.push(value_text.to_string());
        }
    }
    snippets
}

pub(crate) fn nearby_fact_mentions_place(snippet: &str, place_name: &str) -> bool {
    let snippet_lower = snippet.to_ascii_lowercase();
    if query_contains_lower_text(&snippet_lower, place_name) {
        return true;
    }

    let place_tokens = named_place_identity_tokens(place_name);
    if place_tokens.is_empty() {
        return false;
    }
    let snippet_tokens = analyzer::stemmed_tokens(snippet);
    place_tokens
        .iter()
        .all(|token| snippet_tokens.iter().any(|candidate| candidate == token))
}

fn named_place_identity_tokens(place_name: &str) -> Vec<String> {
    analyzer::stemmed_tokens(place_name)
        .into_iter()
        .filter(|token| !is_nearby_place_generic_token(token))
        .collect()
}

fn is_nearby_place_generic_token(token: &str) -> bool {
    configured_named_place_generic_tokens().contains(token)
}

pub(crate) fn haversine_km(
    latitude_a: f64,
    longitude_a: f64,
    latitude_b: f64,
    longitude_b: f64,
) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6_371.008_8;
    let lat1 = latitude_a.to_radians();
    let lat2 = latitude_b.to_radians();
    let delta_lat = (latitude_b - latitude_a).to_radians();
    let delta_lon = (longitude_b - longitude_a).to_radians();
    let a =
        (delta_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_KM * c
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
        if let Some(multiplier) = parser::distance_unit_multiplier(&unit) {
            return Some(value * multiplier);
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
    parser::distance_unit_multiplier(unit).map(|multiplier| value * multiplier)
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

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
    fn extracts_query_distance_limits() {
        assert_eq!(
            parser::parse_query_slots("homes within 500m of Deens Academy")
                .distance_limit
                .map(|distance| distance.value_km),
            Some(0.5)
        );
        assert_eq!(
            parser::parse_query_slots("3bhk under 1 km from Gopalan National School")
                .distance_limit
                .map(|distance| distance.value_km),
            Some(1.0)
        );
        assert_eq!(
            parser::parse_query_slots("3bhk near metro").distance_limit,
            None
        );
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

    #[test]
    fn serving_coordinate_lookup_ignores_rera_geo_facts() {
        let index = ServingFactIndex::from_records(
            vec![
                serving_numeric_fact("society:rera-green", "geo.latitude", 12.814964, "Rera"),
                serving_numeric_fact("society:rera-green", "geo.longitude", 77.509353, "Rera"),
            ],
            Vec::new(),
        );

        assert!(coordinates_for_entity(&index, "society:rera-green").is_none());
    }

    #[test]
    fn serving_coordinate_lookup_uses_google_over_rera_geo_facts() {
        let index = ServingFactIndex::from_records(
            vec![
                serving_numeric_fact("society:rera-green", "geo.latitude", 12.814964, "Rera"),
                serving_numeric_fact("society:rera-green", "geo.longitude", 77.509353, "Rera"),
                serving_numeric_fact("society:rera-green", "geo.latitude", 12.896276, "Google"),
                serving_numeric_fact("society:rera-green", "geo.longitude", 77.5308391, "Google"),
            ],
            Vec::new(),
        );

        let coordinates = coordinates_for_entity(&index, "society:rera-green").unwrap();

        assert_eq!(coordinates.latitude, 12.896276);
        assert_eq!(coordinates.longitude, 77.5308391);
    }

    fn serving_numeric_fact(
        entity_id: &str,
        fact_key: &str,
        value: f64,
        source_type: &str,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "numeric".to_string(),
            value_text: Some(value.to_string()),
            value: FactValue::Numeric(value),
            confidence: 1.0,
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: chrono::Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn exact_place_mentions_do_not_expand_to_generic_place_family_matches() {
        let index = GeoSearchIndex {
            places: vec![
                GeoPlace {
                    entity_id: "place:hm".to_string(),
                    name: "H M Tech Park".to_string(),
                    category: Some("tech_park".to_string()),
                    latitude: 12.0,
                    longitude: 77.0,
                    confidence: 0.99,
                    match_tokens: significant_place_tokens("H M Tech Park"),
                },
                GeoPlace {
                    entity_id: "place:example".to_string(),
                    name: "Example Tech Park".to_string(),
                    category: Some("tech_park".to_string()),
                    latitude: 12.1,
                    longitude: 77.1,
                    confidence: 0.82,
                    match_tokens: significant_place_tokens("Example Tech Park"),
                },
            ],
            society_coordinates: Vec::new(),
        };

        let query = index.query("3bhk near example tech park").unwrap();

        assert_eq!(query.resolved_places().len(), 1);
        assert_eq!(query.resolved_places()[0].name, "Example Tech Park");
    }

    #[test]
    fn distinctive_partial_place_token_resolves_named_place() {
        let index = GeoSearchIndex {
            places: vec![
                GeoPlace {
                    entity_id: "place:hospital".to_string(),
                    name: "Northstar Hospital Whitefield".to_string(),
                    category: Some("hospital".to_string()),
                    latitude: 12.0,
                    longitude: 77.0,
                    confidence: 0.93,
                    match_tokens: significant_place_tokens("Northstar Hospital Whitefield"),
                },
                GeoPlace {
                    entity_id: "place:tech".to_string(),
                    name: "Example Tech Park".to_string(),
                    category: Some("tech_park".to_string()),
                    latitude: 12.1,
                    longitude: 77.1,
                    confidence: 0.82,
                    match_tokens: significant_place_tokens("Example Tech Park"),
                },
            ],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("2 bhk near northstar within 3 km")
            .expect("distinctive short place token should resolve");

        assert_eq!(query.resolved_places().len(), 1);
        assert_eq!(
            query.resolved_places()[0].name,
            "Northstar Hospital Whitefield"
        );
    }

    #[test]
    fn distinctive_two_token_name_resolves_place_with_long_tagline() {
        let index = GeoSearchIndex {
            places: vec![GeoPlace {
                entity_id: "place:school".to_string(),
                name: "Northstar High - Learn. Lead. Succeed".to_string(),
                category: Some("school".to_string()),
                latitude: 12.0,
                longitude: 77.0,
                confidence: 0.93,
                match_tokens: significant_place_tokens("Northstar High - Learn. Lead. Succeed"),
            }],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("2 bhk near northstar high under budget")
            .expect("two-token place name should resolve despite an indexed tagline");

        assert_eq!(query.resolved_places().len(), 1);
        assert_eq!(
            query.resolved_places()[0].name,
            "Northstar High - Learn. Lead. Succeed"
        );
    }

    #[test]
    fn generic_place_family_tokens_do_not_resolve_as_named_places() {
        let index = GeoSearchIndex {
            places: vec![
                GeoPlace {
                    entity_id: "place:hm".to_string(),
                    name: "H M Tech Park".to_string(),
                    category: Some("tech_park".to_string()),
                    latitude: 12.0,
                    longitude: 77.0,
                    confidence: 0.99,
                    match_tokens: significant_place_tokens("H M Tech Park"),
                },
                GeoPlace {
                    entity_id: "place:example".to_string(),
                    name: "Example Tech Park".to_string(),
                    category: Some("tech_park".to_string()),
                    latitude: 12.1,
                    longitude: 77.1,
                    confidence: 0.82,
                    match_tokens: significant_place_tokens("Example Tech Park"),
                },
            ],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("3bhk near tech park")
            .expect("generic category should remain a fact-backed clause");
        assert!(query.resolved_places().is_empty());
        assert_eq!(
            query.resolved_clauses()[0].category_fact_keys,
            vec!["nearby_tech_parks"]
        );
    }

    #[test]
    fn generic_place_family_clauses_do_not_fuzzily_resolve_named_places() {
        let index = GeoSearchIndex {
            places: vec![GeoPlace {
                entity_id: "place:montessori".to_string(),
                name: "Mont Ivy Montessori Preschools Near Me".to_string(),
                category: Some("school".to_string()),
                latitude: 12.0,
                longitude: 77.0,
                confidence: 0.99,
                match_tokens: significant_place_tokens("Mont Ivy Montessori Preschools Near Me"),
            }],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("homes with both nearby metro-station evidence and nearby school evidence")
            .expect("generic categories should remain fact-backed clauses");
        assert!(query.resolved_places().is_empty());
        assert!(query
            .resolved_clauses()
            .iter()
            .all(|clause| !clause.category_fact_keys.is_empty()));
        assert!(index
            .query("projects with school evidence nearby")
            .is_none());
    }

    #[test]
    fn exact_generic_place_name_does_not_resolve_as_named_place() {
        assert!(
            significant_place_tokens("Tech Park").is_empty(),
            "generic place tokens should come from scoring policy config"
        );
        let index = GeoSearchIndex {
            places: vec![GeoPlace {
                entity_id: "place:generic-tech-park".to_string(),
                name: "Tech Park".to_string(),
                category: Some("tech_park".to_string()),
                latitude: 12.0,
                longitude: 77.0,
                confidence: 0.99,
                match_tokens: significant_place_tokens("Tech Park"),
            }],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("3bhk near tech park")
            .expect("generic category should remain a fact-backed clause");
        assert!(query.resolved_places().is_empty());
    }

    #[test]
    fn place_mentions_without_relation_do_not_trigger_geo_query() {
        let index = GeoSearchIndex {
            places: vec![GeoPlace {
                entity_id: "place:deens".to_string(),
                name: "Deens Academy".to_string(),
                category: Some("school".to_string()),
                latitude: 12.0,
                longitude: 77.0,
                confidence: 0.99,
                match_tokens: significant_place_tokens("Deens Academy"),
            }],
            society_coordinates: Vec::new(),
        };

        assert!(index.query("reviews for deens academy").is_none());
        assert!(index.query("homes close to deens academy").is_some());
        assert!(index.query("budget within 2cr for deens academy").is_none());
        assert!(index.query("homes within 500m of deens academy").is_some());
    }

    #[test]
    fn unsupported_or_named_targets_do_not_fall_back_to_partial_category_words() {
        let index = GeoSearchIndex::default();

        assert!(index.query("3bhk near a police station").is_none());
        assert!(index
            .query("2bhk near Basavanpura Lake under 2cr")
            .is_none());
    }

    #[test]
    fn mixed_anchor_query_keeps_named_category_and_unresolved_clauses_separate() {
        let entities = vec![ServingEntityRecord {
            entity_id: "area:whitefield".to_string(),
            entity_type: "area".to_string(),
            name: "Whitefield".to_string(),
            root_source: None,
            searchable_text: "Whitefield".to_string(),
        }];
        let facts = ServingFactIndex::from_records(
            vec![
                serving_numeric_fact("area:whitefield", "geo.latitude", 12.9698, "Google"),
                serving_numeric_fact("area:whitefield", "geo.longitude", 77.75, "Google"),
            ],
            Vec::new(),
        );
        let index = GeoSearchIndex::from_serving_bundle(&entities, &facts);

        let query = index
            .query(
                "3bhk near Whitefield close to kids school and near my wife office in Marathahalli",
            )
            .expect("the resolved anchors should remain usable");

        assert_eq!(query.resolved_clauses().len(), 2);
        assert_eq!(query.resolved_clauses()[0].target_text, "whitefield");
        assert_eq!(
            query.resolved_clauses()[1].category_fact_keys,
            vec!["nearby_schools"]
        );
        assert_eq!(
            query.unresolved_targets(),
            &["my wife office in marathahalli"]
        );
        assert!(query
            .resolved_places()
            .iter()
            .any(|place| { place.entity_id == "area:whitefield" && place.name == "Whitefield" }));
    }

    #[test]
    fn longest_exact_place_name_suppresses_contained_entity() {
        let index = GeoSearchIndex {
            places: vec![
                GeoPlace {
                    entity_id: "place:area:banashankari".to_string(),
                    name: "Banashankari".to_string(),
                    category: None,
                    latitude: 12.0,
                    longitude: 77.0,
                    confidence: 1.0,
                    match_tokens: significant_place_tokens("Banashankari"),
                },
                GeoPlace {
                    entity_id: "place:hospital:sri-banashankari".to_string(),
                    name: "Sri Banashankari Hospital".to_string(),
                    category: Some("hospital".to_string()),
                    latitude: 12.1,
                    longitude: 77.1,
                    confidence: 0.9,
                    match_tokens: significant_place_tokens("Sri Banashankari Hospital"),
                },
            ],
            society_coordinates: Vec::new(),
        };

        let query = index
            .query("homes near Sri Banashankari Hospital")
            .expect("full hospital name should resolve");

        assert_eq!(query.resolved_places().len(), 1);
        assert_eq!(
            query.resolved_places()[0].entity_id,
            "place:hospital:sri-banashankari"
        );
    }
}
