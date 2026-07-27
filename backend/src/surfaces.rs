use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::Serialize;

use crate::dag_config::{UiSurfaceConfig, UiSurfaceLayerRule};
use crate::knowledge::FactValue;
use crate::models::{KgEntityRefs, Property};
use crate::proof_focus::ProofFocus;
use crate::related_societies::related_society_entity_ids;
use crate::search::geo::{extract_first_distance_km, haversine_km};
use crate::serving::{LoadedServingBundle, ServingEntityFactRows, ServingFactRecord};

pub const SURFACE_SCENE_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSceneResponse {
    pub contract_version: u32,
    pub surface_id: String,
    pub property_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_bundle_version: Option<String>,
    pub entity_refs: KgEntityRefs,
    pub anchor: SceneAnchor,
    pub viewport: SceneViewport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_focus: Option<ProofFocus>,
    pub layers: Vec<SceneLayer>,
    pub features: Vec<SceneFeature>,
    pub relations: Vec<SceneRelation>,
    pub callouts: Vec<SceneCallout>,
    pub receipts: Vec<SceneReceipt>,
    pub fill_rate: SceneFillRate,
    pub gaps: Vec<SceneGap>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneAnchor {
    pub entity_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<SceneGeometry>,
    pub coordinate_quality: CoordinateQuality,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneViewport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<SceneBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_m: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneLayer {
    pub id: String,
    pub label: String,
    pub family: String,
    pub render_kind: String,
    pub relation_class: String,
    pub enabled_by_default: bool,
    pub rank: u32,
    pub available_count: usize,
    pub shown_count: usize,
    pub fill_state: FillState,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFeature {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    pub layer_id: String,
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_label: Option<String>,
    pub geometry: SceneGeometry,
    pub coordinate_quality: CoordinateQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<SceneMetrics>,
    pub display: SceneFeatureDisplay,
    pub confidence: f32,
    pub receipt_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel_time_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFeatureDisplay {
    pub tone: DisplayTone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneRelation {
    pub from_id: String,
    pub to_id: String,
    pub edge_type: String,
    pub relation_class: String,
    pub direct: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<u32>,
    pub confidence: f32,
    pub receipt_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneCallout {
    pub id: String,
    pub tone: DisplayTone,
    pub label: String,
    pub feature_ids: Vec<String>,
    pub receipt_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneReceipt {
    pub id: String,
    pub entity_id: String,
    pub fact_key: String,
    pub claim: String,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub learned_at: DateTime<Utc>,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneFillRate {
    pub filled_layers: usize,
    pub partial_layers: usize,
    pub empty_layers: usize,
    pub shown_features: usize,
    pub available_features: usize,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneGap {
    pub layer_id: String,
    pub fill_state: FillState,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum SceneGeometry {
    Point { coordinates: [f64; 2] },
    LineString { coordinates: Vec<[f64; 2]> },
    Polygon { coordinates: Vec<Vec<[f64; 2]>> },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateQuality {
    Exact,
    Derived,
    Approximate,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTone {
    Positive,
    Neutral,
    Caution,
    Risk,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FillState {
    Filled,
    Partial,
    Empty,
}

pub fn build_surface_scene(
    property: &Property,
    society_name: Option<&str>,
    entity_refs: KgEntityRefs,
    bundle: &LoadedServingBundle,
    surface: &UiSurfaceConfig,
) -> Option<SurfaceSceneResponse> {
    build_surface_scene_with_focus(property, society_name, entity_refs, bundle, surface, None)
}

pub fn build_surface_scene_with_focus(
    property: &Property,
    society_name: Option<&str>,
    entity_refs: KgEntityRefs,
    bundle: &LoadedServingBundle,
    surface: &UiSurfaceConfig,
    proof_focus: Option<&ProofFocus>,
) -> Option<SurfaceSceneResponse> {
    let scene_config = surface.scene.as_ref()?;
    let requested_focus = proof_focus.filter(|focus| focus.surface_id == surface.id);
    let mut applied_focus = None;
    let anchor_entity_id = match scene_config.anchor.entity_ref.as_str() {
        "society" => entity_refs.society_entity_id.clone(),
        "property" => entity_refs.property_entity_id.clone(),
        "area" => entity_refs.area_entity_id.clone(),
        "builder" => entity_refs.builder_entity_id.clone()?,
        _ => entity_refs.society_entity_id.clone(),
    };
    let anchor_coords = coordinates_for_entity(bundle, &anchor_entity_id);
    let anchor = SceneAnchor {
        entity_id: anchor_entity_id.clone(),
        label: society_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(property.title.as_str())
            .to_string(),
        area: (!property.area.trim().is_empty()).then(|| property.area.clone()),
        geometry: anchor_coords.map(point_geometry),
        coordinate_quality: anchor_coords
            .map(|_| CoordinateQuality::Exact)
            .unwrap_or(CoordinateQuality::Missing),
    };
    let place_index = ScenePlaceIndex::from_bundle(bundle);
    let mut features = Vec::new();
    let mut relations = Vec::new();
    let mut receipts = Vec::new();
    let mut layers = Vec::new();

    for (layer_index, layer_rule) in scene_config.layers.iter().enumerate() {
        let layer_rank = layer_rule.rank.unwrap_or((layer_index as u32) + 1);
        let mut candidates = features_for_layer(
            layer_rule,
            property,
            &anchor_entity_id,
            anchor_coords,
            bundle,
            &place_index,
        );
        if layer_rule.sort.as_deref() == Some("reviews") {
            candidates.sort_by(|left, right| {
                sort_priority_key(layer_rule, &left.receipt.fact_key)
                    .cmp(&sort_priority_key(layer_rule, &right.receipt.fact_key))
                    .then_with(|| review_sort_key(right).cmp(&review_sort_key(left)))
                    .then_with(|| {
                        distance_sort_key(left.distance_m).cmp(&distance_sort_key(right.distance_m))
                    })
                    .then_with(|| left.label.cmp(&right.label))
            });
        } else if layer_rule.sort.as_deref() == Some("distance") {
            candidates.sort_by(|left, right| {
                sort_priority_key(layer_rule, &left.receipt.fact_key)
                    .cmp(&sort_priority_key(layer_rule, &right.receipt.fact_key))
                    .then_with(|| {
                        distance_sort_key(left.distance_m).cmp(&distance_sort_key(right.distance_m))
                    })
                    .then_with(|| left.label.cmp(&right.label))
            });
        }
        dedup_candidates(&mut candidates);
        let available_count = candidates.len();
        if let Some(max_items) = layer_rule.max_items {
            let all_candidates = candidates.clone();
            candidates = select_layer_candidates(layer_rule, candidates, max_items);
            if let Some(focus) = requested_focus.filter(|focus| focus.layer_id == layer_rule.id) {
                if let Some(focused) = all_candidates
                    .iter()
                    .find(|candidate| candidate_matches_focus(candidate, focus))
                    .cloned()
                {
                    let already_selected = candidates
                        .iter()
                        .any(|candidate| same_scene_candidate(candidate, &focused));
                    if !already_selected {
                        candidates.push(focused);
                    }
                }
            }
        }
        let shown_count = candidates.len();
        let fill_state = fill_state(available_count, shown_count);
        for candidate in candidates {
            let feature_id = format!(
                "{}:{}:{}",
                surface.id,
                layer_rule.id,
                candidate
                    .entity_id
                    .as_deref()
                    .unwrap_or(candidate.receipt.id.as_str())
                    .replace(':', "-")
            );
            let receipt_id = candidate.receipt.id.clone();
            if requested_focus.is_some_and(|focus| candidate_matches_focus(&candidate, focus)) {
                if let Some(focus) = requested_focus {
                    let mut focus = focus.clone();
                    focus.feature_id = Some(feature_id.clone());
                    focus.receipt_id = Some(receipt_id.clone());
                    if focus.entity_id.is_none() {
                        focus.entity_id = candidate.entity_id.clone();
                    }
                    if focus.matched_label.is_none() {
                        focus.matched_label = Some(candidate.label.clone());
                    }
                    if focus.distance_m.is_none() {
                        focus.distance_m = candidate.distance_m;
                    }
                    applied_focus = Some(focus);
                }
            }
            relations.push(SceneRelation {
                from_id: anchor_entity_id.clone(),
                to_id: feature_id.clone(),
                edge_type: relation_edge_type(layer_rule),
                relation_class: layer_rule.relation_class.clone(),
                direct: true,
                distance_m: candidate.distance_m,
                confidence: candidate.confidence,
                receipt_ids: vec![receipt_id.clone()],
            });
            receipts.push(candidate.receipt);
            features.push(SceneFeature {
                id: feature_id,
                entity_id: candidate.entity_id,
                layer_id: layer_rule.id.clone(),
                kind: candidate.kind,
                label: candidate.label,
                short_label: candidate.short_label,
                geometry: candidate.geometry,
                coordinate_quality: candidate.coordinate_quality,
                metrics: Some(SceneMetrics {
                    distance_m: candidate.distance_m,
                    travel_time_min: None,
                    rating: if layer_rule.show_review_metrics.unwrap_or(true) {
                        candidate.rating
                    } else {
                        None
                    },
                    review_count: if layer_rule.show_review_metrics.unwrap_or(true) {
                        candidate.review_count
                    } else {
                        None
                    },
                    severity: None,
                }),
                display: SceneFeatureDisplay {
                    tone: tone_for_layer(layer_rule),
                    icon: icon_for_layer(layer_rule),
                    priority: layer_rank,
                },
                confidence: candidate.confidence,
                receipt_ids: vec![receipt_id],
            });
        }
        layers.push(SceneLayer {
            id: layer_rule.id.clone(),
            label: layer_rule.label.clone(),
            family: layer_rule.family.clone(),
            render_kind: layer_rule.render_kind.clone(),
            relation_class: layer_rule.relation_class.clone(),
            enabled_by_default: layer_rule.enabled_by_default,
            rank: layer_rank,
            available_count,
            shown_count,
            fill_state,
        });
    }

    let fill_rate = scene_fill_rate(&layers);
    let gaps = layers
        .iter()
        .filter(|layer| layer.fill_state != FillState::Filled)
        .map(|layer| SceneGap {
            layer_id: layer.id.clone(),
            fill_state: layer.fill_state,
        })
        .collect();
    let viewport = scene_viewport(anchor_coords, &features);
    Some(SurfaceSceneResponse {
        contract_version: SURFACE_SCENE_CONTRACT_VERSION,
        surface_id: surface.id.clone(),
        property_id: property.id.clone(),
        serving_bundle_version: Some(bundle.manifest.bundle_version.clone()),
        entity_refs,
        anchor,
        viewport,
        proof_focus: applied_focus,
        layers,
        features,
        relations,
        callouts: Vec::new(),
        receipts,
        fill_rate,
        gaps,
    })
}

#[derive(Debug, Clone)]
struct SceneFeatureCandidate {
    entity_id: Option<String>,
    kind: String,
    label: String,
    short_label: Option<String>,
    geometry: SceneGeometry,
    coordinate_quality: CoordinateQuality,
    distance_m: Option<u32>,
    rating: Option<f64>,
    review_count: Option<u32>,
    confidence: f32,
    receipt: SceneReceipt,
}

fn candidate_matches_focus(candidate: &SceneFeatureCandidate, focus: &ProofFocus) -> bool {
    if !candidate
        .receipt
        .fact_key
        .eq_ignore_ascii_case(&focus.fact_key)
    {
        return false;
    }
    let entity_matches = focus
        .entity_id
        .as_deref()
        .is_some_and(|entity_id| candidate.entity_id.as_deref() == Some(entity_id));
    let receipt_matches = focus
        .receipt_id
        .as_deref()
        .is_some_and(|receipt_id| candidate.receipt.id == receipt_id);
    let label_matches = focus
        .matched_label
        .as_deref()
        .is_some_and(|label| candidate_text_matches(candidate, label));
    let value_matches = focus
        .matched_value
        .as_deref()
        .is_some_and(|value| candidate_text_matches(candidate, value));
    let distance_matches = focus.distance_m.is_some_and(|distance_m| {
        candidate
            .distance_m
            .is_some_and(|candidate_distance_m| candidate_distance_m.abs_diff(distance_m) <= 50)
    });

    if focus.entity_id.is_some() {
        if !entity_matches && !label_matches && !value_matches {
            return false;
        }
    }
    if focus.receipt_id.is_some() {
        if !receipt_matches {
            return false;
        }
    }
    if let Some(label) = focus.matched_label.as_deref() {
        if !candidate_text_matches(candidate, label) {
            return false;
        }
    }
    if focus.entity_id.is_none()
        && focus.receipt_id.is_none()
        && focus.matched_label.is_none()
        && (focus.matched_value.is_some() || focus.distance_m.is_some())
    {
        return value_matches || distance_matches;
    }
    true
}

fn same_scene_candidate(left: &SceneFeatureCandidate, right: &SceneFeatureCandidate) -> bool {
    left.receipt.id == right.receipt.id
        || (left.entity_id.is_some()
            && left.entity_id == right.entity_id
            && left
                .receipt
                .fact_key
                .eq_ignore_ascii_case(&right.receipt.fact_key))
}

fn candidate_text_matches(candidate: &SceneFeatureCandidate, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    text_contains_ci(&candidate.label, needle)
        || candidate
            .short_label
            .as_deref()
            .is_some_and(|label| text_contains_ci(label, needle))
        || text_contains_ci(&candidate.receipt.claim, needle)
        || candidate
            .receipt
            .scope
            .as_deref()
            .is_some_and(|scope| text_contains_ci(scope, needle))
}

fn text_contains_ci(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[derive(Debug, Clone)]
struct PlaceLookup {
    entity_id: String,
    label: String,
    coordinates: Option<(f64, f64)>,
    rating: Option<f64>,
    review_count: Option<u32>,
}

#[derive(Debug, Clone, Default)]
struct ScenePlaceIndex {
    by_entity_id: HashMap<String, PlaceLookup>,
    by_source_url: HashMap<String, PlaceLookup>,
}

impl ScenePlaceIndex {
    fn from_bundle(bundle: &LoadedServingBundle) -> Self {
        let mut index = Self::default();
        for entity in &bundle.entities {
            if !entity.entity_type.eq_ignore_ascii_case("place") {
                continue;
            }
            let Some(rows) = bundle.fact_index.entity(&entity.entity_id) else {
                continue;
            };
            let place = PlaceLookup {
                entity_id: entity.entity_id.clone(),
                label: entity.name.clone(),
                coordinates: bundle
                    .spatial_index
                    .point_for_entity(&entity.entity_id)
                    .map(|point| (point.latitude, point.longitude))
                    .or_else(|| coordinates_from_rows(rows)),
                rating: numeric_fact(rows, "google_rating"),
                review_count: numeric_fact(rows, "google_review_count").map(|value| value as u32),
            };
            index
                .by_entity_id
                .entry(entity.entity_id.clone())
                .or_insert_with(|| place.clone());
            for source_url in text_facts(rows, "google_place_url") {
                index
                    .by_source_url
                    .entry(source_url)
                    .or_insert_with(|| place.clone());
            }
        }
        index
    }
}

fn features_for_layer(
    layer_rule: &UiSurfaceLayerRule,
    property: &Property,
    anchor_entity_id: &str,
    anchor_coords: Option<(f64, f64)>,
    bundle: &LoadedServingBundle,
    place_index: &ScenePlaceIndex,
) -> Vec<SceneFeatureCandidate> {
    let edge_places = edge_target_places(layer_rule, anchor_entity_id, bundle, place_index);
    let edge_targets = edge_target_entity_ids(layer_rule, anchor_entity_id, bundle);
    let fact_keys = layer_rule
        .fact_keys
        .iter()
        .map(|key| key.as_str())
        .collect::<HashSet<_>>();
    let mut row_entity_ids = vec![anchor_entity_id.to_string()];
    if layer_rule.include_related_society_facts {
        row_entity_ids.extend(related_society_entity_ids(property, &bundle.fact_index));
    }
    row_entity_ids.sort();
    row_entity_ids.dedup();

    let mut fact_sources = Vec::new();
    for row_entity_id in row_entity_ids {
        let Some(rows) = bundle.fact_index.entity(&row_entity_id) else {
            continue;
        };
        fact_sources.extend(
            rows.facts
                .iter()
                .enumerate()
                .filter(|(_, fact)| fact_keys.contains(fact.fact_key.as_str()))
                .map(|(index, fact)| {
                    let linked_entity_id = linked_entity_id_for_fact(
                        rows,
                        fact,
                        index,
                        &layer_rule.linked_entity_fact_keys,
                        &fact_keys,
                    );
                    let linked_rows = linked_entity_id
                        .as_deref()
                        .and_then(|entity_id| bundle.fact_index.entity(entity_id));
                    let (coordinates, coordinate_quality) =
                        if let Some(entity_id) = linked_entity_id.as_deref() {
                            edge_target_feature_coordinates(
                                coordinates_for_entity(bundle, entity_id),
                                anchor_coords,
                            )
                        } else {
                            (anchor_coords, CoordinateQuality::Exact)
                        };
                    FactFeatureSource {
                        fact,
                        entity_id: linked_entity_id,
                        coordinates,
                        geometry: linked_rows
                            .and_then(scene_geometry_from_rows)
                            .or_else(|| scene_geometry_from_rows(rows)),
                        coordinate_quality,
                        index,
                    }
                }),
        );
    }
    for target_entity_id in edge_targets {
        let Some(target_rows) = bundle.fact_index.entity(&target_entity_id) else {
            continue;
        };
        let (feature_coords, coordinate_quality) = edge_target_feature_coordinates(
            coordinates_for_entity(bundle, &target_entity_id),
            anchor_coords,
        );
        for fact in target_rows
            .facts
            .iter()
            .filter(|fact| fact_keys.contains(fact.fact_key.as_str()))
        {
            let index = fact_sources.len();
            fact_sources.push(FactFeatureSource {
                fact,
                entity_id: Some(target_entity_id.clone()),
                coordinates: feature_coords,
                geometry: scene_geometry_from_rows(target_rows),
                coordinate_quality,
                index,
            });
        }
    }

    fact_sources
        .into_iter()
        .filter_map(|source| {
            feature_candidate_from_fact(
                layer_rule,
                source,
                anchor_coords,
                place_index,
                &edge_places,
            )
        })
        .filter(|candidate| candidate_matches_name_policy(layer_rule, candidate))
        .collect()
}

#[derive(Debug, Clone)]
struct FactFeatureSource<'a> {
    fact: &'a ServingFactRecord,
    entity_id: Option<String>,
    coordinates: Option<(f64, f64)>,
    geometry: Option<SceneGeometry>,
    coordinate_quality: CoordinateQuality,
    index: usize,
}

fn feature_candidate_from_fact(
    layer_rule: &UiSurfaceLayerRule,
    source: FactFeatureSource<'_>,
    anchor_coords: Option<(f64, f64)>,
    place_index: &ScenePlaceIndex,
    edge_places: &[&PlaceLookup],
) -> Option<SceneFeatureCandidate> {
    let fact = source.fact;
    let claim = fact_claim(fact)?;
    let parsed = parse_nearby_display(&claim);
    let place = fact
        .source_url
        .as_deref()
        .and_then(|url| place_index.by_source_url.get(url))
        .or_else(|| place_from_edges(&parsed.name, edge_places));
    let place_coordinates = place.and_then(|place| place.coordinates);
    let coordinates = place_coordinates.or(source.coordinates).or(anchor_coords)?;
    let label = place
        .map(|place| place.label.clone())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| {
            if source.entity_id.is_some() && !should_keep_linked_claim_label(&parsed.name) {
                readable_fact_label(&fact.fact_key)
            } else {
                parsed.name
            }
        });
    let geometry = source
        .geometry
        .clone()
        .unwrap_or_else(|| point_geometry(coordinates));
    let distance_m = parsed.distance_km.map(km_to_meters).or_else(|| {
        let (anchor_lat, anchor_lng) = anchor_coords?;
        Some(km_to_meters(haversine_km(
            anchor_lat,
            anchor_lng,
            coordinates.0,
            coordinates.1,
        )))
    });
    let rating = place.and_then(|place| place.rating).or(parsed.rating);
    let review_count = place
        .and_then(|place| place.review_count)
        .or(parsed.review_count);
    let receipt = SceneReceipt {
        id: receipt_id(fact, source.index),
        entity_id: fact.entity_id.clone(),
        fact_key: fact.fact_key.clone(),
        claim,
        source_type: if fact.source_type.trim().is_empty() {
            "Unknown".to_string()
        } else {
            fact.source_type.clone()
        },
        source_url: fact.source_url.clone(),
        learned_at: fact.learned_at,
        confidence: fact.confidence,
        scope: distance_m.map(|distance| format!("within {} m", rounded_scope_m(distance))),
    };
    Some(SceneFeatureCandidate {
        entity_id: place
            .map(|place| place.entity_id.clone())
            .or(source.entity_id),
        kind: kind_for_layer(layer_rule),
        label,
        short_label: None,
        geometry,
        coordinate_quality: if place_coordinates.is_some() {
            CoordinateQuality::Exact
        } else {
            source.coordinate_quality
        },
        distance_m,
        rating,
        review_count,
        confidence: fact.confidence,
        receipt,
    })
}

fn edge_target_places<'a>(
    layer_rule: &UiSurfaceLayerRule,
    anchor_entity_id: &str,
    bundle: &LoadedServingBundle,
    place_index: &'a ScenePlaceIndex,
) -> Vec<&'a PlaceLookup> {
    if layer_rule.edge_types.is_empty() {
        return Vec::new();
    }
    edge_target_entity_ids(layer_rule, anchor_entity_id, bundle)
        .into_iter()
        .filter_map(|entity_id| place_index.by_entity_id.get(&entity_id))
        .collect()
}

fn edge_target_entity_ids(
    layer_rule: &UiSurfaceLayerRule,
    anchor_entity_id: &str,
    bundle: &LoadedServingBundle,
) -> Vec<String> {
    if layer_rule.edge_types.is_empty() {
        return Vec::new();
    }
    let edge_types = layer_rule
        .edge_types
        .iter()
        .map(|edge_type| edge_type.as_str())
        .collect::<Vec<_>>();
    bundle
        .graph_index
        .targets_out(anchor_entity_id, &edge_types)
}

fn edge_target_feature_coordinates(
    target_coords: Option<(f64, f64)>,
    anchor_coords: Option<(f64, f64)>,
) -> (Option<(f64, f64)>, CoordinateQuality) {
    match target_coords {
        Some(coords) => (Some(coords), CoordinateQuality::Exact),
        None => (anchor_coords, CoordinateQuality::Approximate),
    }
}

fn place_from_edges<'a>(
    claim_name: &str,
    edge_places: &[&'a PlaceLookup],
) -> Option<&'a PlaceLookup> {
    edge_places
        .iter()
        .copied()
        .find(|place| labels_compatible(&place.label, claim_name))
}

fn labels_compatible(left: &str, right: &str) -> bool {
    let left = normalized_label(left);
    let right = normalized_label(right);
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(&right) || right.contains(&left))
}

fn normalized_label(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn coordinates_for_entity(bundle: &LoadedServingBundle, entity_id: &str) -> Option<(f64, f64)> {
    bundle
        .spatial_index
        .point_for_entity(entity_id)
        .map(|point| (point.latitude, point.longitude))
        .or_else(|| {
            bundle
                .fact_index
                .entity(entity_id)
                .and_then(coordinates_from_rows)
        })
}

fn coordinates_from_rows(rows: &ServingEntityFactRows) -> Option<(f64, f64)> {
    let latitude =
        numeric_fact(rows, "geo.latitude").or_else(|| numeric_fact(rows, "project_latitude"))?;
    let longitude =
        numeric_fact(rows, "geo.longitude").or_else(|| numeric_fact(rows, "project_longitude"))?;
    if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) {
        Some((latitude, longitude))
    } else {
        None
    }
}

fn text_facts(rows: &ServingEntityFactRows, fact_key: &str) -> Vec<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key == fact_key)
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            _ => None,
        })
        .collect()
}

fn linked_entity_id_for_fact(
    rows: &ServingEntityFactRows,
    fact: &ServingFactRecord,
    fact_index: usize,
    linked_fact_keys: &[String],
    primary_fact_keys: &HashSet<&str>,
) -> Option<String> {
    if linked_fact_keys.is_empty() {
        return None;
    }
    let linked_keys = linked_fact_keys
        .iter()
        .map(|key| key.as_str())
        .collect::<HashSet<_>>();
    let linked_facts = rows
        .facts
        .iter()
        .filter(|candidate| linked_keys.contains(candidate.fact_key.as_str()))
        .collect::<Vec<_>>();
    if linked_facts.is_empty() {
        return None;
    }
    if let Some(source_url) = fact
        .source_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if let Some(entity_id) = linked_facts
            .iter()
            .find(|candidate| candidate.source_url.as_deref() == Some(source_url))
            .and_then(|candidate| fact_text_value(candidate))
        {
            return Some(entity_id);
        }
    }
    if linked_facts.len() == 1 {
        return fact_text_value(linked_facts[0]);
    }
    let ordinal = rows
        .facts
        .iter()
        .take(fact_index)
        .filter(|candidate| primary_fact_keys.contains(candidate.fact_key.as_str()))
        .count();
    linked_facts
        .get(ordinal)
        .and_then(|candidate| fact_text_value(candidate))
}

fn fact_text_value(fact: &ServingFactRecord) -> Option<String> {
    match &fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

fn should_keep_linked_claim_label(label: &str) -> bool {
    let trimmed = label.trim();
    !trimmed.is_empty() && !trimmed.starts_with("way/") && !trimmed.eq_ignore_ascii_case("drain")
}

fn readable_fact_label(fact_key: &str) -> String {
    let label = fact_key
        .replace('.', " ")
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut characters = label.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn scene_geometry_from_rows(rows: &ServingEntityFactRows) -> Option<SceneGeometry> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key == "geo.geometry_geojson")
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) => scene_geometry_from_geojson(value),
            _ => None,
        })
        .next()
}

fn scene_geometry_from_geojson(value: &str) -> Option<SceneGeometry> {
    let parsed = value.parse::<GeoJson>().ok()?;
    match parsed {
        GeoJson::Geometry(geometry) => scene_geometry_from_geojson_value(&geometry.value),
        GeoJson::Feature(feature) => feature
            .geometry
            .as_ref()
            .and_then(|geometry| scene_geometry_from_geojson_value(&geometry.value)),
        GeoJson::FeatureCollection(collection) => collection
            .features
            .iter()
            .filter_map(|feature| feature.geometry.as_ref())
            .find_map(|geometry| scene_geometry_from_geojson_value(&geometry.value)),
    }
}

fn scene_geometry_from_geojson_value(value: &GeoJsonValue) -> Option<SceneGeometry> {
    match value {
        GeoJsonValue::Point(point) => geojson_point(point).map(point_geometry),
        GeoJsonValue::LineString(points) => geojson_line_string(points),
        GeoJsonValue::MultiLineString(lines) => lines.iter().find_map(|line| {
            geojson_line_string(line).filter(|geometry| match geometry {
                SceneGeometry::LineString { coordinates } => coordinates.len() >= 2,
                _ => false,
            })
        }),
        GeoJsonValue::GeometryCollection(geometries) => geometries
            .iter()
            .find_map(|geometry| scene_geometry_from_geojson_value(&geometry.value)),
        _ => None,
    }
}

fn geojson_line_string(points: &[Vec<f64>]) -> Option<SceneGeometry> {
    let coordinates = points
        .iter()
        .map(|point| geojson_point(point).map(|(latitude, longitude)| [longitude, latitude]))
        .collect::<Option<Vec<_>>>()?;
    (coordinates.len() >= 2).then_some(SceneGeometry::LineString { coordinates })
}

fn geojson_point(point: &[f64]) -> Option<(f64, f64)> {
    if point.len() < 2 {
        return None;
    }
    let longitude = point[0];
    let latitude = point[1];
    if latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
    {
        Some((latitude, longitude))
    } else {
        None
    }
}

fn numeric_fact(rows: &ServingEntityFactRows, fact_key: &str) -> Option<f64> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key == fact_key)
        .filter_map(|fact| match &fact.value {
            FactValue::Numeric(value) if value.is_finite() => Some(*value),
            FactValue::Score { value, .. } if value.is_finite() => Some(*value),
            _ => None,
        })
        .next()
}

#[derive(Debug, PartialEq)]
struct ParsedNearbyDisplay {
    name: String,
    distance_km: Option<f64>,
    rating: Option<f64>,
    review_count: Option<u32>,
}

fn parse_nearby_display(value: &str) -> ParsedNearbyDisplay {
    let (name, meta) = match value.rfind(" (") {
        Some(index) if value.ends_with(')') => {
            let name = value[..index].trim().to_string();
            let meta = &value[index + 2..value.len() - 1];
            (name, Some(meta))
        }
        _ => (value.trim().to_string(), None),
    };
    let mut distance_km = None;
    let mut rating = None;
    let mut review_count = None;
    if let Some(meta) = meta {
        distance_km = extract_first_distance_km(meta);
        for part in meta.split(',') {
            let part = part.trim();
            if let Some(raw) = part.strip_suffix(" rating") {
                rating = raw
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.0..=5.0).contains(value));
            } else if let Some(raw) = part.strip_suffix(" reviews") {
                review_count = raw.trim().parse::<u32>().ok();
            }
        }
    }
    ParsedNearbyDisplay {
        name,
        distance_km,
        rating,
        review_count,
    }
}

fn fact_claim(fact: &ServingFactRecord) -> Option<String> {
    match &fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        FactValue::Numeric(value) if value.is_finite() => Some(format!("{value}")),
        FactValue::Bool(value) => Some(value.to_string()),
        FactValue::Score { value, explanation } if value.is_finite() => {
            if explanation.trim().is_empty() {
                Some(format!("{value}"))
            } else {
                Some(explanation.trim().to_string())
            }
        }
        FactValue::Tags(values) => {
            let values = values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            (!values.is_empty()).then(|| values.join(", "))
        }
        _ => None,
    }
}

fn receipt_id(fact: &ServingFactRecord, index: usize) -> String {
    format!(
        "receipt:{}:{}:{}:{index}",
        fact.entity_id.replace(':', "-"),
        fact.fact_key.replace('.', "-"),
        fact.learned_at.timestamp()
    )
}

fn point_geometry((latitude, longitude): (f64, f64)) -> SceneGeometry {
    SceneGeometry::Point {
        coordinates: [longitude, latitude],
    }
}

fn scene_viewport(anchor_coords: Option<(f64, f64)>, features: &[SceneFeature]) -> SceneViewport {
    let mut points = Vec::new();
    if let Some((latitude, longitude)) = anchor_coords {
        points.push((latitude, longitude));
    }
    for feature in features {
        collect_geometry_points(&feature.geometry, &mut points);
    }
    if points.is_empty() {
        return SceneViewport {
            center: None,
            bounds: None,
            radius_m: None,
        };
    }
    let min_lat = points
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let max_lat = points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lng = points
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_lng = points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let lat_pad = ((max_lat - min_lat) * 0.12).max(0.003);
    let lng_pad = ((max_lng - min_lng) * 0.12).max(0.003);
    SceneViewport {
        center: Some([(min_lng + max_lng) / 2.0, (min_lat + max_lat) / 2.0]),
        bounds: Some(SceneBounds {
            west: min_lng - lng_pad,
            south: min_lat - lat_pad,
            east: max_lng + lng_pad,
            north: max_lat + lat_pad,
        }),
        radius_m: Some(3000),
    }
}

fn collect_geometry_points(geometry: &SceneGeometry, points: &mut Vec<(f64, f64)>) {
    match geometry {
        SceneGeometry::Point { coordinates } => points.push((coordinates[1], coordinates[0])),
        SceneGeometry::LineString { coordinates } => {
            points.extend(coordinates.iter().map(|point| (point[1], point[0])));
        }
        SceneGeometry::Polygon { coordinates } => {
            points.extend(
                coordinates
                    .iter()
                    .flat_map(|ring| ring.iter().map(|point| (point[1], point[0]))),
            );
        }
    }
}

fn scene_fill_rate(layers: &[SceneLayer]) -> SceneFillRate {
    let filled_layers = layers
        .iter()
        .filter(|layer| layer.fill_state == FillState::Filled)
        .count();
    let partial_layers = layers
        .iter()
        .filter(|layer| layer.fill_state == FillState::Partial)
        .count();
    let empty_layers = layers
        .iter()
        .filter(|layer| layer.fill_state == FillState::Empty)
        .count();
    let shown_features = layers.iter().map(|layer| layer.shown_count).sum();
    let available_features = layers.iter().map(|layer| layer.available_count).sum();
    let value = if layers.is_empty() {
        0.0
    } else {
        (filled_layers as f32 + partial_layers as f32 * 0.5) / layers.len() as f32
    };
    SceneFillRate {
        filled_layers,
        partial_layers,
        empty_layers,
        shown_features,
        available_features,
        value,
    }
}

fn fill_state(available_count: usize, shown_count: usize) -> FillState {
    match (available_count, shown_count) {
        (0, _) => FillState::Empty,
        (available, shown) if shown >= available => FillState::Filled,
        _ => FillState::Partial,
    }
}

fn dedup_candidates(candidates: &mut Vec<SceneFeatureCandidate>) {
    let mut seen = HashSet::<String>::new();
    candidates.retain(|candidate| {
        let key = candidate
            .entity_id
            .clone()
            .unwrap_or_else(|| candidate.label.to_ascii_lowercase());
        seen.insert(key)
    });
}

fn select_layer_candidates(
    layer_rule: &UiSurfaceLayerRule,
    candidates: Vec<SceneFeatureCandidate>,
    max_items: usize,
) -> Vec<SceneFeatureCandidate> {
    if candidates.len() <= max_items {
        return candidates;
    }
    let expanded_max = layer_rule
        .expanded_max_items
        .unwrap_or(max_items)
        .max(max_items);
    let spread_min_distance_km = layer_rule.spread_min_distance_km.unwrap_or(0.0);
    if expanded_max == max_items || spread_min_distance_km <= 0.0 {
        return candidates.into_iter().take(max_items).collect();
    }

    let mut selected = Vec::new();
    let mut remaining = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        if index < max_items {
            selected.push(candidate);
        } else {
            remaining.push(candidate);
        }
    }

    for candidate in remaining {
        if selected.len() >= expanded_max {
            break;
        }
        if selected
            .iter()
            .all(|selected| candidate_spread_km(&candidate, selected) >= spread_min_distance_km)
        {
            selected.push(candidate);
        }
    }
    selected
}

fn candidate_matches_name_policy(
    layer_rule: &UiSurfaceLayerRule,
    candidate: &SceneFeatureCandidate,
) -> bool {
    if layer_rule.include_name_markers.is_empty() {
        return true;
    }
    let label = candidate.label.to_ascii_lowercase();
    layer_rule
        .include_name_markers
        .iter()
        .map(|marker| marker.trim().to_ascii_lowercase())
        .filter(|marker| !marker.is_empty())
        .any(|marker| label.contains(&marker))
}

fn candidate_spread_km(left: &SceneFeatureCandidate, right: &SceneFeatureCandidate) -> f64 {
    match (
        point_coordinates_from_geometry(&left.geometry),
        point_coordinates_from_geometry(&right.geometry),
    ) {
        (Some(left), Some(right)) => haversine_km(left.0, left.1, right.0, right.1),
        _ => {
            let left_distance = left.distance_m.unwrap_or(u32::MAX);
            let right_distance = right.distance_m.unwrap_or(u32::MAX);
            left_distance.abs_diff(right_distance) as f64 / 1000.0
        }
    }
}

fn point_coordinates_from_geometry(geometry: &SceneGeometry) -> Option<(f64, f64)> {
    match geometry {
        SceneGeometry::Point { coordinates } => Some((coordinates[1], coordinates[0])),
        _ => None,
    }
}

fn distance_sort_key(distance_m: Option<u32>) -> u32 {
    distance_m.unwrap_or(u32::MAX)
}

fn review_sort_key(candidate: &SceneFeatureCandidate) -> (u32, u32) {
    (
        candidate.review_count.unwrap_or(0),
        candidate
            .rating
            .map(|rating| (rating * 100.0).round() as u32)
            .unwrap_or(0),
    )
}

fn sort_priority_key(layer_rule: &UiSurfaceLayerRule, fact_key: &str) -> usize {
    layer_rule
        .sort_priority_fact_keys
        .iter()
        .position(|candidate| candidate == fact_key)
        .unwrap_or(layer_rule.sort_priority_fact_keys.len())
}

fn km_to_meters(distance_km: f64) -> u32 {
    (distance_km.max(0.0) * 1000.0).round() as u32
}

fn rounded_scope_m(distance_m: u32) -> u32 {
    if distance_m < 1000 {
        ((distance_m + 49) / 50) * 50
    } else {
        ((distance_m + 249) / 250) * 250
    }
}

fn relation_edge_type(layer_rule: &UiSurfaceLayerRule) -> String {
    layer_rule.edge_types.first().cloned().unwrap_or_else(|| {
        if layer_rule.render_kind == "evidence_list" {
            "has_fact".to_string()
        } else {
            "within_radius".to_string()
        }
    })
}

fn tone_for_layer(layer_rule: &UiSurfaceLayerRule) -> DisplayTone {
    match layer_rule.relation_class.as_str() {
        "risk_externality" => DisplayTone::Risk,
        "access" => DisplayTone::Positive,
        _ => DisplayTone::Neutral,
    }
}

fn icon_for_layer(layer_rule: &UiSurfaceLayerRule) -> Option<String> {
    if let Some(icon) = layer_rule
        .icon
        .as_ref()
        .map(|icon| icon.trim())
        .filter(|icon| !icon.is_empty())
    {
        return Some(icon.to_string());
    }
    match layer_rule.id.as_str() {
        "metro" => Some("train".to_string()),
        "schools" => Some("graduation-cap".to_string()),
        "hospitals" => Some("hospital".to_string()),
        "tech" => Some("briefcase-business".to_string()),
        "fitness" => Some("dumbbell".to_string()),
        "parks" => Some("trees".to_string()),
        "lakes" => Some("waves".to_string()),
        _ => None,
    }
}

fn kind_for_layer(layer_rule: &UiSurfaceLayerRule) -> String {
    match layer_rule.render_kind.as_str() {
        "pin" => "place".to_string(),
        "polygon" => layer_rule.id.trim_end_matches('s').to_string(),
        "line" | "corridor" => "line".to_string(),
        _ => layer_rule.id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::graph::GraphIndex;
    use crate::search::geo::GeoSearchIndex;
    use crate::serving::{
        ServingBundleManifest, ServingEdgeRecord, ServingEntityRecord, SpatialServingIndex,
        TantivyRecallIndex,
    };

    #[test]
    fn parses_nearby_display_for_scene_metrics() {
        let parsed = parse_nearby_display("Aster Hospital (1.2 km, 4.4 rating, 521 reviews)");
        assert_eq!(parsed.name, "Aster Hospital");
        assert_eq!(parsed.distance_km, Some(1.2));
        assert_eq!(parsed.rating, Some(4.4));
        assert_eq!(parsed.review_count, Some(521));
    }

    #[test]
    fn point_geometry_uses_geojson_order() {
        assert_eq!(
            point_geometry((12.98, 77.75)),
            SceneGeometry::Point {
                coordinates: [77.75, 12.98]
            }
        );
    }

    #[test]
    fn line_geometry_reads_valid_geojson_fact() {
        assert_eq!(
            scene_geometry_from_geojson(
                r#"{"type":"LineString","coordinates":[[77.745,12.94],[77.747,12.942]]}"#
            ),
            Some(SceneGeometry::LineString {
                coordinates: vec![[77.745, 12.94], [77.747, 12.942]]
            })
        );
    }

    #[test]
    fn geometry_reader_rejects_invalid_or_unsupported_geojson() {
        assert_eq!(scene_geometry_from_geojson("not geojson"), None);
        assert_eq!(
            scene_geometry_from_geojson(
                r#"{"type":"Polygon","coordinates":[[[77.745,12.94],[77.747,12.94],[77.747,12.942],[77.745,12.94]]]}"#
            ),
            None
        );
        assert_eq!(
            scene_geometry_from_geojson(r#"{"type":"LineString","coordinates":[[77.745,12.94]]}"#),
            None
        );
    }

    #[test]
    fn linked_entity_fact_pairs_receipt_fact_with_map_entity() {
        let fact_index = crate::serving::ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:one",
                    "stormwater_drain_nearby",
                    FactValue::Text("Varthur Rajakaluve (42 m, severity: high)".to_string()),
                    None,
                ),
                serving_fact(
                    "society:one",
                    "stormwater_drain_place_entity",
                    FactValue::Text("place:stormwater-drain:swd-1".to_string()),
                    None,
                ),
            ],
            Vec::new(),
        );
        let rows = fact_index.entity("society:one").unwrap();
        let primary_keys = HashSet::from(["stormwater_drain_nearby"]);

        assert_eq!(
            linked_entity_id_for_fact(
                &rows,
                &rows.facts[0],
                0,
                &["stormwater_drain_place_entity".to_string()],
                &primary_keys,
            ),
            Some("place:stormwater-drain:swd-1".to_string())
        );
    }

    #[test]
    fn linked_entity_fact_uses_ordinal_when_source_url_is_missing() {
        let fact_index = crate::serving::ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:one",
                    "stormwater_drain_nearby",
                    FactValue::Text("Drain one".to_string()),
                    None,
                ),
                serving_fact(
                    "society:one",
                    "stormwater_drain_place_entity",
                    FactValue::Text("place:stormwater-drain:one".to_string()),
                    None,
                ),
                serving_fact(
                    "society:one",
                    "stormwater_drain_nearby",
                    FactValue::Text("Drain two".to_string()),
                    None,
                ),
                serving_fact(
                    "society:one",
                    "stormwater_drain_place_entity",
                    FactValue::Text("place:stormwater-drain:two".to_string()),
                    None,
                ),
            ],
            Vec::new(),
        );
        let rows = fact_index.entity("society:one").unwrap();
        let primary_keys = HashSet::from(["stormwater_drain_nearby"]);

        assert_eq!(
            linked_entity_id_for_fact(
                &rows,
                &rows.facts[2],
                2,
                &["stormwater_drain_place_entity".to_string()],
                &primary_keys,
            ),
            Some("place:stormwater-drain:two".to_string())
        );
    }

    #[test]
    fn surface_scene_projects_direct_risk_line_from_linked_entity_fact() {
        let entities = vec![
            serving_entity("society:one", "society", "One Society"),
            serving_entity("place:stormwater-drain:one", "place", "Varthur Rajakaluve"),
        ];
        let facts = vec![
            serving_fact(
                "society:one",
                "geo.latitude",
                FactValue::Numeric(12.94),
                None,
            ),
            serving_fact(
                "society:one",
                "geo.longitude",
                FactValue::Numeric(77.745),
                None,
            ),
            serving_fact(
                "society:one",
                "stormwater_drain_nearby",
                FactValue::Text("Varthur Rajakaluve (42 m, severity: high)".to_string()),
                None,
            ),
            serving_fact(
                "society:one",
                "stormwater_drain_place_entity",
                FactValue::Text("place:stormwater-drain:one".to_string()),
                None,
            ),
            serving_fact(
                "place:stormwater-drain:one",
                "geo.latitude",
                FactValue::Numeric(12.941),
                None,
            ),
            serving_fact(
                "place:stormwater-drain:one",
                "geo.longitude",
                FactValue::Numeric(77.746),
                None,
            ),
            serving_fact(
                "place:stormwater-drain:one",
                "geo.geometry_geojson",
                FactValue::Text(
                    r#"{"type":"LineString","coordinates":[[77.745,12.94],[77.747,12.942]]}"#
                        .to_string(),
                ),
                None,
            ),
        ];
        let bundle = loaded_bundle(entities, facts);
        let surface = UiSurfaceConfig {
            id: "flooding".to_string(),
            title: "Flooding & drainage".to_string(),
            kicker: None,
            leaf_keys: Vec::new(),
            traversal: Vec::new(),
            components: Vec::new(),
            primary_entity: Some("society".to_string()),
            scene: Some(crate::dag_config::UiSurfaceSceneConfig {
                anchor: crate::dag_config::UiSurfaceAnchorConfig {
                    entity_ref: "society".to_string(),
                },
                layers: vec![UiSurfaceLayerRule {
                    id: "drains".to_string(),
                    label: "Drains".to_string(),
                    fact_keys: vec!["stormwater_drain_nearby".to_string()],
                    edge_types: Vec::new(),
                    linked_entity_fact_keys: vec!["stormwater_drain_place_entity".to_string()],
                    sort_priority_fact_keys: Vec::new(),
                    family: "risk".to_string(),
                    relation_class: "risk_externality".to_string(),
                    render_kind: "line".to_string(),
                    icon: None,
                    sort: None,
                    max_items: Some(2),
                    expanded_max_items: None,
                    spread_min_distance_km: None,
                    show_review_metrics: None,
                    include_name_markers: Vec::new(),
                    include_related_society_facts: false,
                    enabled_by_default: true,
                    rank: Some(1),
                }],
            }),
        };

        let scene = build_surface_scene(
            &sample_property(),
            Some("One Society"),
            KgEntityRefs {
                property_entity_id: "property:one".to_string(),
                society_entity_id: "society:one".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            &bundle,
            &surface,
        )
        .expect("scene should build");

        assert_eq!(scene.layers[0].fill_state, FillState::Filled);
        assert_eq!(scene.features.len(), 1);
        assert_eq!(
            scene.features[0].entity_id.as_deref(),
            Some("place:stormwater-drain:one")
        );
        assert_eq!(
            scene.features[0].geometry,
            SceneGeometry::LineString {
                coordinates: vec![[77.745, 12.94], [77.747, 12.942]]
            }
        );
        assert_eq!(scene.relations[0].relation_class, "risk_externality");
        assert!(scene.relations[0].direct);
        assert_eq!(scene.receipts[0].fact_key, "stormwater_drain_nearby");
        assert!(scene.receipts[0].claim.contains("Varthur Rajakaluve"));
        assert!(!scene.receipts[0]
            .claim
            .contains("place:stormwater-drain:one"));
    }

    #[test]
    fn surface_scene_uses_configured_fact_priority_before_distance_cap() {
        let entities = vec![serving_entity("society:one", "society", "One Society")];
        let facts = vec![
            serving_fact(
                "society:one",
                "geo.latitude",
                FactValue::Numeric(12.94),
                None,
            ),
            serving_fact(
                "society:one",
                "geo.longitude",
                FactValue::Numeric(77.745),
                None,
            ),
            serving_fact(
                "society:one",
                "nearby_graveyards",
                FactValue::Text("Burial ground (50 m)".to_string()),
                None,
            ),
            serving_fact(
                "society:one",
                "high_voltage_transmission_line_nearby",
                FactValue::Text("Transmission line (400 m, 220 kV)".to_string()),
                None,
            ),
        ];
        let bundle = loaded_bundle(entities, facts);
        let surface = UiSurfaceConfig {
            id: "around_this_home".to_string(),
            title: "Around this home".to_string(),
            kicker: None,
            leaf_keys: Vec::new(),
            traversal: Vec::new(),
            components: Vec::new(),
            primary_entity: Some("society".to_string()),
            scene: Some(crate::dag_config::UiSurfaceSceneConfig {
                anchor: crate::dag_config::UiSurfaceAnchorConfig {
                    entity_ref: "society".to_string(),
                },
                layers: vec![UiSurfaceLayerRule {
                    id: "red_flags".to_string(),
                    label: "Red flags".to_string(),
                    fact_keys: vec![
                        "nearby_graveyards".to_string(),
                        "high_voltage_transmission_line_nearby".to_string(),
                    ],
                    edge_types: Vec::new(),
                    linked_entity_fact_keys: Vec::new(),
                    sort_priority_fact_keys: vec![
                        "high_voltage_transmission_line_nearby".to_string()
                    ],
                    family: "risk".to_string(),
                    relation_class: "risk_externality".to_string(),
                    render_kind: "pin".to_string(),
                    icon: Some("flag".to_string()),
                    sort: Some("distance".to_string()),
                    max_items: Some(1),
                    expanded_max_items: None,
                    spread_min_distance_km: None,
                    show_review_metrics: None,
                    include_name_markers: Vec::new(),
                    include_related_society_facts: false,
                    enabled_by_default: true,
                    rank: Some(1),
                }],
            }),
        };

        let scene = build_surface_scene(
            &sample_property(),
            Some("One Society"),
            KgEntityRefs {
                property_entity_id: "property:one".to_string(),
                society_entity_id: "society:one".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            &bundle,
            &surface,
        )
        .expect("scene should build");

        assert_eq!(scene.features.len(), 1);
        assert_eq!(
            scene.receipts[0].fact_key,
            "high_voltage_transmission_line_nearby"
        );
        assert_eq!(scene.layers[0].available_count, 2);
        assert_eq!(scene.layers[0].shown_count, 1);
        assert_eq!(scene.layers[0].fill_state, FillState::Partial);
    }

    #[test]
    fn surface_scene_adds_focused_candidate_without_hiding_default_candidate() {
        let entities = vec![serving_entity("society:one", "society", "One Society")];
        let facts = vec![
            serving_fact(
                "society:one",
                "geo.latitude",
                FactValue::Numeric(12.94),
                None,
            ),
            serving_fact(
                "society:one",
                "geo.longitude",
                FactValue::Numeric(77.745),
                None,
            ),
            serving_fact(
                "society:one",
                "nearby_graveyards",
                FactValue::Text("Burial ground (50 m)".to_string()),
                None,
            ),
            serving_fact(
                "society:one",
                "high_voltage_transmission_line_nearby",
                FactValue::Text("Transmission line (400 m, 220 kV)".to_string()),
                None,
            ),
        ];
        let bundle = loaded_bundle(entities, facts);
        let surface = UiSurfaceConfig {
            id: "around_this_home".to_string(),
            title: "Around this home".to_string(),
            kicker: None,
            leaf_keys: Vec::new(),
            traversal: Vec::new(),
            components: Vec::new(),
            primary_entity: Some("society".to_string()),
            scene: Some(crate::dag_config::UiSurfaceSceneConfig {
                anchor: crate::dag_config::UiSurfaceAnchorConfig {
                    entity_ref: "society".to_string(),
                },
                layers: vec![UiSurfaceLayerRule {
                    id: "red_flags".to_string(),
                    label: "Red flags".to_string(),
                    fact_keys: vec![
                        "nearby_graveyards".to_string(),
                        "high_voltage_transmission_line_nearby".to_string(),
                    ],
                    edge_types: Vec::new(),
                    linked_entity_fact_keys: Vec::new(),
                    sort_priority_fact_keys: vec![
                        "high_voltage_transmission_line_nearby".to_string()
                    ],
                    family: "risk".to_string(),
                    relation_class: "risk_externality".to_string(),
                    render_kind: "pin".to_string(),
                    icon: Some("flag".to_string()),
                    sort: Some("distance".to_string()),
                    max_items: Some(1),
                    expanded_max_items: None,
                    spread_min_distance_km: None,
                    show_review_metrics: None,
                    include_name_markers: Vec::new(),
                    include_related_society_facts: false,
                    enabled_by_default: true,
                    rank: Some(1),
                }],
            }),
        };
        let focus = crate::proof_focus::ProofFocus {
            surface_id: "around_this_home".to_string(),
            layer_id: "red_flags".to_string(),
            fact_key: "nearby_graveyards".to_string(),
            entity_id: None,
            feature_id: None,
            receipt_id: None,
            matched_label: Some("Burial ground".to_string()),
            matched_value: Some("Burial ground (50 m)".to_string()),
            requested_constraint: Some("near Burial ground".to_string()),
            distance_m: Some(50),
            reason: "matched near Burial ground".to_string(),
        };

        let scene = build_surface_scene_with_focus(
            &sample_property(),
            Some("One Society"),
            KgEntityRefs {
                property_entity_id: "property:one".to_string(),
                society_entity_id: "society:one".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            &bundle,
            &surface,
            Some(&focus),
        )
        .expect("scene should build");

        let fact_keys = scene
            .receipts
            .iter()
            .map(|receipt| receipt.fact_key.as_str())
            .collect::<Vec<_>>();
        assert!(fact_keys.contains(&"high_voltage_transmission_line_nearby"));
        assert!(fact_keys.contains(&"nearby_graveyards"));
        assert_eq!(scene.features.len(), 2);
        assert_eq!(scene.layers[0].available_count, 2);
        assert_eq!(scene.layers[0].shown_count, 2);
        let applied = scene.proof_focus.expect("focus should be applied");
        assert_eq!(applied.fact_key, "nearby_graveyards");
        assert!(applied.feature_id.is_some());
        assert!(applied.receipt_id.is_some());
    }

    #[test]
    fn surface_scene_includes_related_rera_society_proximity_facts() {
        let entities = vec![
            serving_entity("society:society-one", "society", "One Society"),
            serving_entity("society:rera-one", "society", "One Society Phase 1"),
        ];
        let facts = vec![
            serving_fact(
                "society:society-one",
                "geo.latitude",
                FactValue::Numeric(12.94),
                None,
            ),
            serving_fact(
                "society:society-one",
                "geo.longitude",
                FactValue::Numeric(77.745),
                None,
            ),
            serving_fact(
                "society:rera-one",
                "rera_project_name",
                FactValue::Text("One Society Phase 1".to_string()),
                None,
            ),
            serving_fact(
                "society:rera-one",
                "nearby_tech_parks",
                FactValue::Text("Bagmane Tech Park (5.3 km)".to_string()),
                Some("https://maps.example/bagmane"),
            ),
        ];
        let bundle = loaded_bundle(entities, facts);
        let surface = UiSurfaceConfig {
            id: "around_this_home".to_string(),
            title: "Around this home".to_string(),
            kicker: None,
            leaf_keys: Vec::new(),
            traversal: Vec::new(),
            components: Vec::new(),
            primary_entity: Some("society".to_string()),
            scene: Some(crate::dag_config::UiSurfaceSceneConfig {
                anchor: crate::dag_config::UiSurfaceAnchorConfig {
                    entity_ref: "society".to_string(),
                },
                layers: vec![UiSurfaceLayerRule {
                    id: "tech".to_string(),
                    label: "Tech parks".to_string(),
                    fact_keys: vec!["nearby_tech_parks".to_string()],
                    edge_types: Vec::new(),
                    linked_entity_fact_keys: Vec::new(),
                    sort_priority_fact_keys: Vec::new(),
                    family: "access".to_string(),
                    relation_class: "access".to_string(),
                    render_kind: "pin".to_string(),
                    icon: Some("briefcase-business".to_string()),
                    sort: Some("distance".to_string()),
                    max_items: Some(8),
                    expanded_max_items: None,
                    spread_min_distance_km: None,
                    show_review_metrics: None,
                    include_name_markers: Vec::new(),
                    include_related_society_facts: true,
                    enabled_by_default: true,
                    rank: Some(1),
                }],
            }),
        };

        let scene = build_surface_scene(
            &sample_property(),
            Some("One Society"),
            KgEntityRefs {
                property_entity_id: "property:one".to_string(),
                society_entity_id: "society:society-one".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            &bundle,
            &surface,
        )
        .expect("scene should build");

        assert_eq!(scene.features.len(), 1);
        assert_eq!(scene.features[0].label, "Bagmane Tech Park");
        assert_eq!(scene.receipts[0].entity_id, "society:rera-one");
    }

    #[test]
    fn surface_scene_hides_review_metrics_when_layer_policy_disables_them() {
        let entities = vec![serving_entity("society:one", "society", "One Society")];
        let facts = vec![
            serving_fact(
                "society:one",
                "geo.latitude",
                FactValue::Numeric(12.94),
                None,
            ),
            serving_fact(
                "society:one",
                "geo.longitude",
                FactValue::Numeric(77.745),
                None,
            ),
            serving_fact(
                "society:one",
                "nearby_graveyards",
                FactValue::Text("Burial ground (0.5 km, 4.8 rating, 900 reviews)".to_string()),
                Some("https://maps.example/graveyard"),
            ),
        ];
        let bundle = loaded_bundle(entities, facts);
        let surface = UiSurfaceConfig {
            id: "around_this_home".to_string(),
            title: "Around this home".to_string(),
            kicker: None,
            leaf_keys: Vec::new(),
            traversal: Vec::new(),
            components: Vec::new(),
            primary_entity: Some("society".to_string()),
            scene: Some(crate::dag_config::UiSurfaceSceneConfig {
                anchor: crate::dag_config::UiSurfaceAnchorConfig {
                    entity_ref: "society".to_string(),
                },
                layers: vec![UiSurfaceLayerRule {
                    id: "red_flags".to_string(),
                    label: "Red flags".to_string(),
                    fact_keys: vec!["nearby_graveyards".to_string()],
                    edge_types: Vec::new(),
                    linked_entity_fact_keys: Vec::new(),
                    sort_priority_fact_keys: Vec::new(),
                    family: "risk".to_string(),
                    relation_class: "risk_externality".to_string(),
                    render_kind: "pin".to_string(),
                    icon: Some("flag".to_string()),
                    sort: Some("distance".to_string()),
                    max_items: Some(5),
                    expanded_max_items: None,
                    spread_min_distance_km: None,
                    show_review_metrics: Some(false),
                    include_name_markers: Vec::new(),
                    include_related_society_facts: false,
                    enabled_by_default: true,
                    rank: Some(1),
                }],
            }),
        };

        let scene = build_surface_scene(
            &sample_property(),
            Some("One Society"),
            KgEntityRefs {
                property_entity_id: "property:one".to_string(),
                society_entity_id: "society:one".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            &bundle,
            &surface,
        )
        .expect("scene should build");

        let metrics = scene.features[0].metrics.as_ref().expect("metrics");
        assert_eq!(metrics.distance_m, Some(500));
        assert_eq!(metrics.rating, None);
        assert_eq!(metrics.review_count, None);
    }

    #[test]
    fn viewport_bounds_include_line_geometry() {
        let viewport = scene_viewport(
            Some((12.93, 77.73)),
            &[SceneFeature {
                id: "feature:drain".to_string(),
                entity_id: Some("place:stormwater-drain:one".to_string()),
                layer_id: "drains".to_string(),
                kind: "line".to_string(),
                label: "Drain".to_string(),
                short_label: None,
                geometry: SceneGeometry::LineString {
                    coordinates: vec![[77.745, 12.94], [77.747, 12.942]],
                },
                coordinate_quality: CoordinateQuality::Exact,
                metrics: None,
                display: SceneFeatureDisplay {
                    tone: DisplayTone::Risk,
                    icon: None,
                    priority: 1,
                },
                confidence: 0.8,
                receipt_ids: Vec::new(),
            }],
        );

        let bounds = viewport.bounds.unwrap();
        assert!(bounds.west < 77.73);
        assert!(bounds.east > 77.747);
        assert!(bounds.south < 12.93);
        assert!(bounds.north > 12.942);
    }

    #[test]
    fn edge_place_matching_uses_normalized_labels() {
        assert!(labels_compatible(
            "Kadugodi Tree Park Metro Station",
            "Kadugodi Tree Park"
        ));
        assert!(!labels_compatible("Aster Hospital", "Green School"));
    }

    #[test]
    fn evidence_list_relation_falls_back_to_has_fact() {
        let layer = UiSurfaceLayerRule {
            id: "groundwater".to_string(),
            label: "Groundwater".to_string(),
            fact_keys: vec!["environment.groundwater_potential_class".to_string()],
            edge_types: Vec::new(),
            linked_entity_fact_keys: Vec::new(),
            sort_priority_fact_keys: Vec::new(),
            family: "environment".to_string(),
            relation_class: "context".to_string(),
            render_kind: "evidence_list".to_string(),
            icon: None,
            sort: None,
            max_items: Some(1),
            expanded_max_items: None,
            spread_min_distance_km: None,
            show_review_metrics: None,
            include_name_markers: Vec::new(),
            include_related_society_facts: false,
            enabled_by_default: true,
            rank: Some(1),
        };
        assert_eq!(relation_edge_type(&layer), "has_fact");
    }

    #[test]
    fn direct_edge_target_selection_is_scoped_and_deduped() {
        let layer = UiSurfaceLayerRule {
            id: "approach_waterlogging".to_string(),
            label: "Waterlogging".to_string(),
            fact_keys: vec!["risk.approach_road_waterlogging".to_string()],
            edge_types: vec!["served_by_road".to_string()],
            linked_entity_fact_keys: Vec::new(),
            sort_priority_fact_keys: Vec::new(),
            family: "risk".to_string(),
            relation_class: "risk_externality".to_string(),
            render_kind: "evidence_list".to_string(),
            icon: None,
            sort: None,
            max_items: Some(2),
            expanded_max_items: None,
            spread_min_distance_km: None,
            show_review_metrics: None,
            include_name_markers: Vec::new(),
            include_related_society_facts: false,
            enabled_by_default: true,
            rank: Some(1),
        };
        let edges = vec![
            ServingEdgeRecord {
                from_entity_id: "society:one".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road:one".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:one".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road:one".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:one".to_string(),
                edge_type: "near_place".to_string(),
                to_entity_id: "place:metro".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:two".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road:two".to_string(),
                confidence: 0.9,
                source_type: "test".to_string(),
            },
        ];

        let graph_index = GraphIndex::from_serving_edges(&edges);
        let edge_types = layer
            .edge_types
            .iter()
            .map(|edge_type| edge_type.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            graph_index.targets_out("society:one", &edge_types),
            vec!["road:one"]
        );
    }

    #[test]
    fn edge_target_coordinate_fallback_marks_anchor_as_approximate() {
        assert_eq!(
            edge_target_feature_coordinates(Some((12.9, 77.7)), Some((12.98, 77.75))),
            (Some((12.9, 77.7)), CoordinateQuality::Exact)
        );
        assert_eq!(
            edge_target_feature_coordinates(None, Some((12.98, 77.75))),
            (Some((12.98, 77.75)), CoordinateQuality::Approximate)
        );
    }

    fn serving_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        source_url: Option<&str>,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "text".to_string(),
            value_text: None,
            value,
            confidence: 0.8,
            source_type: "test".to_string(),
            source_url: source_url.map(str::to_string),
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    fn serving_entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: name.to_string(),
        }
    }

    fn sample_property() -> Property {
        Property {
            id: "property-one".to_string(),
            title: "3 BHK in One Society".to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "society-one".to_string(),
            builder_name: "Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 20_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_500,
            super_builtup_sqft: 2_000,
            floor: 5,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready to move".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 8_000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 10,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        }
    }

    fn loaded_bundle(
        entities: Vec<ServingEntityRecord>,
        facts: Vec<ServingFactRecord>,
    ) -> LoadedServingBundle {
        let fact_index = crate::serving::ServingFactIndex::from_records(facts.clone(), Vec::new());
        let temp_dir = tempdir().unwrap();
        let recall_index =
            TantivyRecallIndex::build_in_dir(temp_dir.path(), &entities, &facts, &[]).unwrap();
        let geo_index = GeoSearchIndex::from_serving_bundle(&entities, &fact_index);
        let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "test-bundle".to_string(),
                format_version: 1,
                created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                entity_count: entities.len() as u64,
                fact_count: facts.len() as u64,
                search_metadata_count: 0,
                edge_count: 0,
                entity_parquet_key: "entities.parquet".to_string(),
                fact_parquet_key: "facts.parquet".to_string(),
                search_metadata_parquet_key: "search.parquet".to_string(),
                edge_parquet_key: None,
                semantic_embedding_parquet_key: None,
                schema_key: "schema.json".to_string(),
                trust_policy_key: "trust.json".to_string(),
                tantivy_index_prefix: "tantivy".to_string(),
                artifacts: Vec::new(),
            },
            entities,
            edges: Vec::new(),
            graph_index: GraphIndex::default(),
            recall_index,
            fact_index,
            geo_index,
            spatial_index,
            semantic_embeddings: Vec::new(),
            cache_dir: temp_dir.keep(),
        }
    }
}
