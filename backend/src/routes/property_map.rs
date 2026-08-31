//! Structured neighborhood plate projection for property detail.
//!
//! Joins society nearby facts to place-entity coordinates by Google place URL,
//! and surfaces groundwater class when available. Soft zone geometry is
//! illustrative in the UI — this payload carries the receipt, not polygons.

use std::collections::HashMap;

use serde::Serialize;

use crate::dag_config::{
    load_fact_registry_index, ui_surfaces_config, CoordinateEntityScope, FactRegistryIndex,
};
use crate::knowledge::FactValue;
use crate::models::Property;
use crate::related_societies::{
    names_compatible, normalized_project_name, related_society_entity_ids,
    related_society_match_names, serving_society_rows_match_names,
};
use crate::search::geo::{extract_first_distance_km, haversine_km};
use crate::serving::{
    resolve_serving_coordinates, ServingEntityFactRows, ServingFactIndex, ServingFactRecord,
    SocietyFactProjection,
};
use crate::surfaces::{SceneGeometry, SurfaceSceneResponse};

use super::enrichment::society_node_id;

const AROUND_THIS_HOME_SURFACE_ID: &str = "around_this_home";
const BUYER_SOURCE_FALLBACK: &str = "80feet";
const DEFAULT_MAP_LAYER_CAP: usize = 3;
const METRO_MAP_RADIUS_KM: f64 = 10.0;
const METRO_MAP_STATION_CAP: usize = 8;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PropertyMapContext {
    pub home: MapHomeAnchor,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<MapPlacePin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<MapWaterContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metro_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub red_flag_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub green_patches: Vec<crate::routes::map_overlays::MapOverlayPolygon>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lakes: Vec<crate::routes::map_overlays::MapOverlayPolygon>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapHomeAnchor {
    pub entity_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<crate::routes::map_overlays::MapOverlayPolygon>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapPlacePin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_entity_id: Option<String>,
    pub layer: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub source_type: String,
    #[serde(skip)]
    sort_priority: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapWaterContext {
    pub groundwater_class: String,
    pub summary: String,
    pub scope_radius_km: f64,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Soft zone fill is UI geometry; class/summary are source-backed.
    pub illustrative_zone: bool,
}

#[derive(Clone, Debug)]
struct PlaceLookup {
    entity_id: String,
    name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    rating: Option<f64>,
    review_count: Option<u32>,
}

#[derive(Clone, Debug)]
struct MapLayerConfig {
    id: String,
    fact_keys: Vec<String>,
    linked_entity_fact_keys: Vec<String>,
    sort_priority_fact_keys: Vec<String>,
    sort: Option<String>,
    max_items: usize,
    expanded_max_items: usize,
    spread_min_distance_km: Option<f64>,
    show_review_metrics: bool,
    include_name_markers: Vec<String>,
    include_related_society_facts: bool,
}

#[derive(Clone, Debug)]
struct DagMetroStation {
    entity_id: String,
    name: String,
    latitude: f64,
    longitude: f64,
    lines: Vec<String>,
}

const DAG_METRO_MATCH_KM: f64 = 0.45;

pub fn build_property_map_context(
    property: &Property,
    society_name: Option<&str>,
    serving_facts: Option<&ServingFactIndex>,
    map_overlays: Option<&crate::routes::map_overlays::CityMapOverlays>,
) -> Option<PropertyMapContext> {
    let facts = serving_facts?;
    let projection = SocietyFactProjection::from_index(facts, &property.society_id);
    let home_entity_id = society_node_id(&property.society_id);
    let home_coords = coordinates_for_candidates(facts, &property.society_id);
    let place_by_url = place_lookup_by_google_url(facts);
    let dag_metro_stations = dag_metro_stations(facts);
    let fact_registry = load_fact_registry_index().ok();

    let layer_configs = match around_this_home_layers() {
        Ok(layers) => layers,
        Err(err) => {
            eprintln!("ERROR: failed to load around_this_home map layer config: {err}");
            return None;
        }
    };

    let mut places = Vec::new();
    for layer_config in layer_configs {
        let mut layer_pins = map_layer_pins(
            property,
            &projection,
            facts,
            &layer_config,
            &place_by_url,
            home_coords.as_ref(),
            fact_registry.as_ref(),
        );
        sort_layer_pins(&mut layer_pins, &layer_config);
        layer_pins
            .dedup_by(|left, right| left.name == right.name && left.source_url == right.source_url);
        layer_pins = select_layer_pins(layer_pins, &layer_config);
        if layer_config.id == "metro" {
            enrich_metro_pins_from_dag(&mut layer_pins, &dag_metro_stations);
        }
        places.extend(layer_pins);
    }

    let water = map_water_context(property, facts, &projection);
    let overlay_home = home_coords;
    let (metro_lines, green_patches, lakes) = match (map_overlays, overlay_home) {
        (Some(overlays), Some(home)) => {
            let metro_stations = metro_corridor_anchors(&places, &dag_metro_stations, home);
            let metro_lines =
                crate::routes::map_overlays::metro_network_near(overlays, home, &metro_stations);
            let (green_patches, lakes) =
                crate::routes::map_overlays::clip_green_patches(overlays, home);
            (metro_lines, green_patches, lakes)
        }
        _ => (Vec::new(), Vec::new(), Vec::new()),
    };
    if let Some(home) = overlay_home {
        add_metro_line_stations(&mut places, &dag_metro_stations, &metro_lines, home);
    }

    if places.is_empty()
        && water.is_none()
        && home_coords.is_none()
        && metro_lines.is_empty()
        && green_patches.is_empty()
        && lakes.is_empty()
    {
        return None;
    }

    Some(PropertyMapContext {
        home: MapHomeAnchor {
            entity_id: home_entity_id,
            name: society_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(property.title.as_str())
                .to_string(),
            area: (!property.area.trim().is_empty()).then(|| property.area.clone()),
            latitude: overlay_home.map(|coords| coords.0),
            longitude: overlay_home.map(|coords| coords.1),
            boundary: None,
        },
        places,
        water,
        metro_lines,
        access_lines: Vec::new(),
        red_flag_lines: Vec::new(),
        green_patches,
        lakes,
    })
}

pub fn property_map_context_from_surface_scene(
    scene: &SurfaceSceneResponse,
) -> Option<PropertyMapContext> {
    let home_coords = point_coordinates(scene.anchor.geometry.as_ref());
    let receipts_by_id = scene
        .receipts
        .iter()
        .map(|receipt| (receipt.id.as_str(), receipt))
        .collect::<HashMap<_, _>>();
    let places = scene
        .features
        .iter()
        .filter_map(|feature| {
            let coordinates = point_coordinates(Some(&feature.geometry))?;
            let receipt = feature
                .receipt_ids
                .iter()
                .find_map(|receipt_id| receipts_by_id.get(receipt_id.as_str()));
            Some(MapPlacePin {
                place_entity_id: feature.entity_id.clone(),
                layer: feature.layer_id.clone(),
                name: feature.label.clone(),
                latitude: Some(coordinates.0),
                longitude: Some(coordinates.1),
                distance_km: feature
                    .metrics
                    .as_ref()
                    .and_then(|metrics| metrics.distance_m)
                    .map(|distance_m| f64::from(distance_m) / 1000.0),
                rating: feature.metrics.as_ref().and_then(|metrics| metrics.rating),
                review_count: feature
                    .metrics
                    .as_ref()
                    .and_then(|metrics| metrics.review_count),
                note: None,
                lines: Vec::new(),
                source_url: receipt.and_then(|receipt| receipt.source_url.clone()),
                source_type: receipt
                    .map(|receipt| receipt.source_type.clone())
                    .unwrap_or_else(|| BUYER_SOURCE_FALLBACK.to_string()),
                sort_priority: 0,
            })
        })
        .collect::<Vec<_>>();
    let line_from_feature = |feature: &crate::surfaces::SceneFeature| {
        let coordinates = line_coordinates(&feature.geometry)?;
        let receipt = feature
            .receipt_ids
            .iter()
            .find_map(|receipt_id| receipts_by_id.get(receipt_id.as_str()));
        Some(crate::routes::map_overlays::MapOverlayLine {
            id: feature.id.clone(),
            name: feature.label.clone(),
            label: feature.short_label.clone(),
            distance_km: feature
                .metrics
                .as_ref()
                .and_then(|metrics| metrics.distance_m)
                .map(|distance_m| f64::from(distance_m) / 1000.0),
            details: feature.details.clone(),
            kind: feature.kind.clone(),
            coordinates,
            source_type: receipt
                .map(|receipt| receipt.source_type.clone())
                .unwrap_or_else(|| BUYER_SOURCE_FALLBACK.to_string()),
            source_url: receipt.and_then(|receipt| receipt.source_url.clone()),
        })
    };
    let access_lines = scene
        .features
        .iter()
        .filter(|feature| feature.layer_id == "metro")
        .filter_map(&line_from_feature)
        .collect::<Vec<_>>();
    let red_flag_lines = scene
        .features
        .iter()
        .filter(|feature| feature.layer_id == "red_flags")
        .filter_map(line_from_feature)
        .collect::<Vec<_>>();

    if places.is_empty()
        && access_lines.is_empty()
        && red_flag_lines.is_empty()
        && home_coords.is_none()
    {
        return None;
    }

    Some(PropertyMapContext {
        home: MapHomeAnchor {
            entity_id: scene.anchor.entity_id.clone(),
            name: scene.anchor.label.clone(),
            area: scene.anchor.area.clone(),
            latitude: home_coords.map(|coords| coords.0),
            longitude: home_coords.map(|coords| coords.1),
            boundary: scene.anchor.boundary.as_ref().and_then(|boundary| {
                polygon_coordinates(&boundary.geometry).map(|coordinates| {
                    crate::routes::map_overlays::MapOverlayPolygon {
                        id: format!("{}:boundary", scene.anchor.entity_id),
                        name: scene.anchor.label.clone(),
                        kind: "society_boundary".to_string(),
                        coordinates,
                        distance_km: None,
                        source_type: boundary.source_type.clone(),
                    }
                })
            }),
        },
        places,
        water: None,
        metro_lines: Vec::new(),
        access_lines,
        red_flag_lines,
        green_patches: Vec::new(),
        lakes: Vec::new(),
    })
}

fn point_coordinates(geometry: Option<&SceneGeometry>) -> Option<(f64, f64)> {
    match geometry {
        Some(SceneGeometry::Point { coordinates }) => {
            let [longitude, latitude] = *coordinates;
            if latitude.is_finite() && longitude.is_finite() {
                Some((latitude, longitude))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn line_coordinates(geometry: &SceneGeometry) -> Option<Vec<[f64; 2]>> {
    match geometry {
        SceneGeometry::LineString { coordinates } => {
            let valid = coordinates
                .iter()
                .all(|[longitude, latitude]| latitude.is_finite() && longitude.is_finite());
            (coordinates.len() >= 2 && valid).then(|| coordinates.clone())
        }
        _ => None,
    }
}

fn polygon_coordinates(geometry: &SceneGeometry) -> Option<Vec<[f64; 2]>> {
    match geometry {
        SceneGeometry::Polygon { coordinates } => {
            coordinates.first().filter(|ring| ring.len() >= 4).cloned()
        }
        SceneGeometry::MultiPolygon { coordinates } => coordinates
            .iter()
            .filter_map(|polygon| polygon.first())
            .max_by(|left, right| polygon_ring_area(left).total_cmp(&polygon_ring_area(right)))
            .filter(|ring| ring.len() >= 4)
            .cloned(),
        _ => None,
    }
}

fn polygon_ring_area(ring: &[[f64; 2]]) -> f64 {
    ring.windows(2)
        .map(|pair| pair[0][0] * pair[1][1] - pair[1][0] * pair[0][1])
        .sum::<f64>()
        .abs()
}

fn add_metro_line_stations(
    places: &mut Vec<MapPlacePin>,
    stations: &[DagMetroStation],
    metro_lines: &[crate::routes::map_overlays::MapOverlayLine],
    home: (f64, f64),
) {
    let visible_lines = metro_lines
        .iter()
        .map(|line| line.name.as_str())
        .collect::<Vec<_>>();
    if visible_lines.is_empty() {
        return;
    }

    let mut candidates = stations
        .iter()
        .filter(|station| {
            station
                .lines
                .iter()
                .any(|line| visible_lines.iter().any(|visible| line == visible))
        })
        .map(|station| {
            (
                haversine_km(home.0, home.1, station.latitude, station.longitude),
                station,
            )
        })
        .filter(|(distance, _)| *distance <= METRO_MAP_RADIUS_KM)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.total_cmp(&right.0));

    for (distance, station) in candidates.into_iter().take(METRO_MAP_STATION_CAP) {
        let already_present = places.iter().any(|place| {
            place.place_entity_id.as_deref() == Some(station.entity_id.as_str())
                || (place.layer == "metro"
                    && place.latitude.is_some_and(|latitude| {
                        place.longitude.is_some_and(|longitude| {
                            haversine_km(latitude, longitude, station.latitude, station.longitude)
                                <= DAG_METRO_MATCH_KM
                        })
                    }))
        });
        if already_present {
            continue;
        }

        places.push(MapPlacePin {
            place_entity_id: Some(station.entity_id.clone()),
            layer: "metro".to_string(),
            name: station.name.clone(),
            latitude: Some(station.latitude),
            longitude: Some(station.longitude),
            distance_km: Some(distance),
            rating: None,
            review_count: None,
            note: None,
            lines: station.lines.clone(),
            source_url: None,
            source_type: "OpenStreetMap".to_string(),
            sort_priority: 0,
        });
    }
}

fn dag_metro_stations(facts: &ServingFactIndex) -> Vec<DagMetroStation> {
    let mut stations = Vec::new();
    for (entity_id, rows) in facts.rows() {
        if !entity_id.starts_with("place:metro:") {
            continue;
        }
        let Some(coordinates) = resolve_serving_coordinates(rows, CoordinateEntityScope::Place)
        else {
            continue;
        };
        let name = text_fact(rows, "place.name").unwrap_or_else(|| entity_id.to_string());
        let lines = tags_fact(rows, "transit.lines")
            .into_iter()
            .filter_map(|value| crate::routes::map_overlays::trunk_line_display_name(&value))
            .collect::<Vec<_>>();
        stations.push(DagMetroStation {
            entity_id: entity_id.to_string(),
            name,
            latitude: coordinates.latitude,
            longitude: coordinates.longitude,
            lines,
        });
    }
    stations
}

fn enrich_metro_pins_from_dag(pins: &mut [MapPlacePin], stations: &[DagMetroStation]) {
    for pin in pins.iter_mut() {
        let Some(matched) = match_dag_metro_station(pin, stations) else {
            continue;
        };
        if pin.place_entity_id.is_none() {
            pin.place_entity_id = Some(matched.entity_id.clone());
        }
        if pin.lines.is_empty() {
            pin.lines = matched.lines.clone();
        }
        if pin.note.is_none() && !matched.lines.is_empty() {
            pin.note = Some(matched.lines.join(" · "));
        }
        // Prefer DAG station coordinates when available — they align with seed
        // corridor geometry better than Google Places estimates.
        pin.latitude = Some(matched.latitude);
        pin.longitude = Some(matched.longitude);
    }
}

fn match_dag_metro_station<'a>(
    pin: &MapPlacePin,
    stations: &'a [DagMetroStation],
) -> Option<&'a DagMetroStation> {
    let (lat, lng) = match (pin.latitude, pin.longitude) {
        (Some(lat), Some(lng)) => (lat, lng),
        _ => return None,
    };
    let pin_name = compact_place_name(&pin.name);
    let mut ranked = stations
        .iter()
        .map(|station| {
            let distance = haversine_km(lat, lng, station.latitude, station.longitude);
            let station_name = compact_place_name(&station.name);
            let name_match = !pin_name.is_empty()
                && !station_name.is_empty()
                && (pin_name == station_name
                    || names_compatible(&pin_name, &station_name)
                    || pin_name.contains(&station_name)
                    || station_name.contains(&pin_name));
            let max_km = if name_match {
                DAG_METRO_MATCH_KM * 2.0
            } else {
                DAG_METRO_MATCH_KM
            };
            let name_bonus = if name_match { 0.0 } else { 0.15 };
            (distance + name_bonus, distance, max_km, station)
        })
        .filter(|(_, distance, max_km, _)| *distance <= *max_km)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| left.0.total_cmp(&right.0));
    ranked.first().map(|item| item.3)
}

fn compact_place_name(value: &str) -> String {
    normalized_project_name(value)
        .unwrap_or_default()
        .replace(' ', "")
}

fn metro_corridor_anchors(
    places: &[MapPlacePin],
    stations: &[DagMetroStation],
    home: (f64, f64),
) -> Vec<crate::routes::map_overlays::MetroCorridorAnchor> {
    let mut anchors = places
        .iter()
        .filter(|place| place.layer == "metro")
        .filter_map(|place| {
            let (latitude, longitude) = match (place.latitude, place.longitude) {
                (Some(latitude), Some(longitude)) => (latitude, longitude),
                _ => return None,
            };
            Some(crate::routes::map_overlays::MetroCorridorAnchor {
                latitude,
                longitude,
                preferred_lines: place.lines.clone(),
            })
        })
        .collect::<Vec<_>>();

    // Only fall back when we already have a nearby Google metro pin that lacked
    // usable line tags. Do not invent a corridor from home alone.
    if !anchors.is_empty()
        && anchors
            .iter()
            .all(|anchor| anchor.preferred_lines.is_empty())
    {
        if let Some(station) =
            stations.iter().min_by(|left, right| {
                haversine_km(home.0, home.1, left.latitude, left.longitude).total_cmp(
                    &haversine_km(home.0, home.1, right.latitude, right.longitude),
                )
            })
        {
            if haversine_km(home.0, home.1, station.latitude, station.longitude) <= 8.0 {
                anchors.push(crate::routes::map_overlays::MetroCorridorAnchor {
                    latitude: station.latitude,
                    longitude: station.longitude,
                    preferred_lines: station.lines.clone(),
                });
            }
        }
    }
    anchors
}

fn tags_fact(rows: &ServingEntityFactRows, key: &str) -> Vec<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key == key)
        .find_map(|fact| match &fact.value {
            FactValue::Tags(values) => Some(
                values
                    .iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>(),
            ),
            FactValue::Text(value) if !value.trim().is_empty() => Some(
                value
                    .split(|character: char| {
                        character == ';' || character == ',' || character == '|'
                    })
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

fn around_this_home_layers() -> Result<Vec<MapLayerConfig>, String> {
    let config = ui_surfaces_config().map_err(|err| err.to_string())?;
    let scene = config
        .surfaces
        .iter()
        .find(|surface| surface.id == AROUND_THIS_HOME_SURFACE_ID)
        .and_then(|surface| surface.scene.as_ref())
        .ok_or_else(|| format!("surface {AROUND_THIS_HOME_SURFACE_ID} has no scene config"))?;
    let mut layers = scene
        .layers
        .iter()
        .filter(|layer| layer.render_kind == "pin")
        .map(|layer| MapLayerConfig {
            id: layer.id.clone(),
            fact_keys: layer.fact_keys.clone(),
            linked_entity_fact_keys: layer.linked_entity_fact_keys.clone(),
            sort_priority_fact_keys: layer.sort_priority_fact_keys.clone(),
            sort: layer.sort.clone(),
            max_items: layer.max_items.unwrap_or(DEFAULT_MAP_LAYER_CAP),
            expanded_max_items: layer
                .expanded_max_items
                .unwrap_or_else(|| layer.max_items.unwrap_or(DEFAULT_MAP_LAYER_CAP)),
            spread_min_distance_km: layer.spread_min_distance_km,
            show_review_metrics: layer.show_review_metrics.unwrap_or(true),
            include_name_markers: layer.include_name_markers.clone(),
            include_related_society_facts: layer.include_related_society_facts,
        })
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| {
        scene
            .layers
            .iter()
            .find(|candidate| candidate.id == layer.id)
            .and_then(|candidate| candidate.rank)
            .unwrap_or(u32::MAX)
    });
    Ok(layers)
}

fn map_layer_pins(
    property: &Property,
    projection: &SocietyFactProjection<'_>,
    facts: &ServingFactIndex,
    layer: &MapLayerConfig,
    place_by_url: &HashMap<String, PlaceLookup>,
    home_coords: Option<&(f64, f64)>,
    fact_registry: Option<&FactRegistryIndex>,
) -> Vec<MapPlacePin> {
    let mut pins = Vec::new();
    let evidence_by_source =
        layer_evidence_by_source(property, facts, projection, layer, &layer.fact_keys);
    let linked_source_urls = layer
        .linked_entity_fact_keys
        .iter()
        .flat_map(|fact_key| layer_records(property, facts, projection, layer, fact_key))
        .filter_map(|fact| fact.source_url.clone())
        .collect::<std::collections::HashSet<_>>();

    for fact_key in &layer.fact_keys {
        pins.extend(
            layer_records(property, facts, projection, layer, fact_key)
                .into_iter()
                .filter(|fact| {
                    fact.source_url
                        .as_ref()
                        .is_none_or(|source_url| !linked_source_urls.contains(source_url))
                })
                .filter_map(|fact| {
                    map_place_pin(
                        fact,
                        &layer.id,
                        place_by_url,
                        home_coords,
                        sort_priority_key(layer, &fact.fact_key),
                    )
                }),
        );
    }

    for linked_fact_key in &layer.linked_entity_fact_keys {
        pins.extend(
            layer_records(property, facts, projection, layer, linked_fact_key)
                .into_iter()
                .filter_map(|fact| {
                    let primary_fact_key = fact
                        .source_url
                        .as_deref()
                        .and_then(|source_url| evidence_by_source.get(source_url).copied())
                        .map(|evidence| evidence.fact_key.as_str())
                        .unwrap_or(fact.fact_key.as_str());
                    map_linked_place_pin(
                        fact,
                        &layer.id,
                        facts,
                        home_coords,
                        &evidence_by_source,
                        fact_registry,
                        sort_priority_key(layer, primary_fact_key),
                    )
                }),
        );
    }

    pins.retain(|pin| pin_matches_name_policy(layer, pin));
    if !layer.show_review_metrics {
        for pin in &mut pins {
            pin.rating = None;
            pin.review_count = None;
            pin.note = None;
        }
    }
    pins
}

fn layer_evidence_by_source<'a>(
    property: &Property,
    facts: &'a ServingFactIndex,
    projection: &'a SocietyFactProjection<'_>,
    layer: &MapLayerConfig,
    fact_keys: &[String],
) -> HashMap<String, &'a ServingFactRecord> {
    let mut by_source = HashMap::new();
    for fact_key in fact_keys {
        for fact in layer_records(property, facts, projection, layer, fact_key) {
            if let Some(source_url) = fact.source_url.as_deref() {
                by_source.entry(source_url.to_string()).or_insert(fact);
            }
        }
    }
    by_source
}

fn layer_records<'a>(
    property: &Property,
    facts: &'a ServingFactIndex,
    projection: &'a SocietyFactProjection<'_>,
    layer: &MapLayerConfig,
    fact_key: &str,
) -> Vec<&'a ServingFactRecord> {
    let mut records = projection.records(fact_key);
    if layer.include_related_society_facts {
        for entity_id in related_society_entity_ids(property, facts) {
            if let Some(rows) = facts.entity(&entity_id) {
                records.extend(rows.facts.iter().filter(|fact| fact.fact_key == fact_key));
            }
        }
    }
    records.sort_by(|left, right| {
        right
            .learned_at
            .cmp(&left.learned_at)
            .then_with(|| left.source_url.cmp(&right.source_url))
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
    records.dedup_by(|left, right| {
        left.entity_id == right.entity_id
            && left.fact_key == right.fact_key
            && left.source_url == right.source_url
            && fact_text(&left.value) == fact_text(&right.value)
    });
    records
}

fn map_place_pin(
    fact: &ServingFactRecord,
    layer: &str,
    place_by_url: &HashMap<String, PlaceLookup>,
    home_coords: Option<&(f64, f64)>,
    sort_priority: usize,
) -> Option<MapPlacePin> {
    let display = match &fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => value.trim(),
        _ => return None,
    };
    let parsed = parse_nearby_display(display);
    let place = fact
        .source_url
        .as_deref()
        .and_then(|url| place_by_url.get(url));

    let latitude = place.and_then(|item| item.latitude);
    let longitude = place.and_then(|item| item.longitude);
    let distance_km = parsed
        .distance_km
        .or_else(|| match (home_coords, latitude, longitude) {
            (Some((home_lat, home_lng)), Some(lat), Some(lng)) => {
                Some((haversine_km(*home_lat, *home_lng, lat, lng) * 10.0).round() / 10.0)
            }
            _ => None,
        });
    let rating = place.and_then(|item| item.rating).or(parsed.rating);
    let review_count = place
        .and_then(|item| item.review_count)
        .or(parsed.review_count);
    let name = place
        .and_then(|item| item.name.clone())
        .unwrap_or(parsed.name);
    let note = pin_note(rating, review_count);

    Some(MapPlacePin {
        place_entity_id: place.map(|item| item.entity_id.clone()),
        layer: layer.to_string(),
        name,
        latitude,
        longitude,
        distance_km,
        rating,
        review_count,
        note,
        lines: Vec::new(),
        source_url: fact.source_url.clone(),
        source_type: if fact.source_type.trim().is_empty() {
            "Google".to_string()
        } else {
            fact.source_type.clone()
        },
        sort_priority,
    })
}

fn map_linked_place_pin(
    linked_fact: &ServingFactRecord,
    layer: &str,
    facts: &ServingFactIndex,
    home_coords: Option<&(f64, f64)>,
    evidence_by_source: &HashMap<String, &ServingFactRecord>,
    fact_registry: Option<&FactRegistryIndex>,
    sort_priority: usize,
) -> Option<MapPlacePin> {
    let place_entity_id = match &linked_fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => value.trim(),
        _ => return None,
    };
    let rows = facts.entity(place_entity_id)?;
    let coordinates = resolve_serving_coordinates(rows, CoordinateEntityScope::Place)?;
    let latitude = coordinates.latitude;
    let longitude = coordinates.longitude;
    let evidence = linked_fact
        .source_url
        .as_deref()
        .and_then(|source_url| evidence_by_source.get(source_url).copied());
    let parsed =
        evidence.and_then(|fact| fact_text(&fact.value).as_deref().map(parse_nearby_display));
    let name = linked_pin_name(rows, evidence, fact_registry);
    let distance_km = parsed
        .as_ref()
        .and_then(|parsed| parsed.distance_km)
        .or_else(|| {
            home_coords.map(|(home_lat, home_lng)| {
                (haversine_km(*home_lat, *home_lng, latitude, longitude) * 10.0).round() / 10.0
            })
        });
    let note = parsed
        .as_ref()
        .map(linked_pin_note)
        .filter(|note| !note.is_empty());

    Some(MapPlacePin {
        place_entity_id: Some(place_entity_id.to_string()),
        layer: layer.to_string(),
        name,
        latitude: Some(latitude),
        longitude: Some(longitude),
        distance_km,
        rating: None,
        review_count: None,
        note,
        lines: Vec::new(),
        source_url: linked_fact.source_url.clone(),
        source_type: if linked_fact.source_type.trim().is_empty() {
            BUYER_SOURCE_FALLBACK.to_string()
        } else {
            linked_fact.source_type.clone()
        },
        sort_priority,
    })
}

fn linked_pin_name(
    rows: &ServingEntityFactRows,
    evidence: Option<&ServingFactRecord>,
    fact_registry: Option<&FactRegistryIndex>,
) -> String {
    let label = evidence.map(|fact| {
        fact_registry
            .and_then(|registry| registry.lookup(&fact.fact_key))
            .and_then(|entry| entry.label.as_ref())
            .map(|label| sentence_case(label))
            .unwrap_or_else(|| readable_fact_label(&fact.fact_key))
    });
    let place_name = text_fact(rows, "place.name");
    match (label, place_name) {
        (Some(label), Some(place_name)) if should_keep_place_name(&place_name) => {
            format!("{label}: {place_name}")
        }
        (Some(label), _) => label,
        (None, Some(place_name)) => place_name,
        (None, None) => "Red flag".to_string(),
    }
}

fn linked_pin_note(parsed: &ParsedNearbyDisplay) -> String {
    let mut parts = Vec::new();
    if let Some(distance_km) = parsed.distance_km {
        parts.push(if distance_km < 1.0 {
            format!("{:.0} m", distance_km * 1000.0)
        } else {
            format!("{distance_km:.1} km")
        });
    }
    if let Some(meta) = parsed.meta.as_ref() {
        parts.extend(
            meta.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .filter(|part| extract_first_distance_km(part).is_none())
                .filter(|part| !part.ends_with(" rating") && !part.ends_with(" reviews"))
                .map(str::to_string),
        );
    }
    parts.join(" · ")
}

fn sort_layer_pins(pins: &mut [MapPlacePin], layer: &MapLayerConfig) {
    if layer.sort.as_deref() == Some("reviews") {
        pins.sort_by(|left, right| {
            left.sort_priority
                .cmp(&right.sort_priority)
                .then_with(|| review_sort_key(right).cmp(&review_sort_key(left)))
                .then_with(|| {
                    distance_sort_key(left.distance_km)
                        .partial_cmp(&distance_sort_key(right.distance_km))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        return;
    }
    pins.sort_by(|left, right| {
        left.sort_priority
            .cmp(&right.sort_priority)
            .then_with(|| {
                distance_sort_key(left.distance_km)
                    .partial_cmp(&distance_sort_key(right.distance_km))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn select_layer_pins(mut pins: Vec<MapPlacePin>, layer: &MapLayerConfig) -> Vec<MapPlacePin> {
    if pins.len() <= layer.max_items {
        return pins;
    }
    let mut selected = pins.drain(..layer.max_items).collect::<Vec<_>>();
    let spread_min_distance_km = layer.spread_min_distance_km.unwrap_or(0.0);
    if layer.expanded_max_items <= layer.max_items || spread_min_distance_km <= 0.0 {
        return selected;
    }
    for pin in pins {
        if selected.len() >= layer.expanded_max_items {
            break;
        }
        if selected
            .iter()
            .all(|selected| pin_spread_km(&pin, selected) >= spread_min_distance_km)
        {
            selected.push(pin);
        }
    }
    selected
}

fn pin_matches_name_policy(layer: &MapLayerConfig, pin: &MapPlacePin) -> bool {
    if layer.include_name_markers.is_empty() {
        return true;
    }
    let name = pin.name.to_ascii_lowercase();
    layer
        .include_name_markers
        .iter()
        .map(|marker| marker.trim().to_ascii_lowercase())
        .filter(|marker| !marker.is_empty())
        .any(|marker| name.contains(&marker))
}

fn pin_spread_km(left: &MapPlacePin, right: &MapPlacePin) -> f64 {
    match (
        left.latitude,
        left.longitude,
        right.latitude,
        right.longitude,
    ) {
        (Some(left_lat), Some(left_lng), Some(right_lat), Some(right_lng)) => {
            haversine_km(left_lat, left_lng, right_lat, right_lng)
        }
        _ => {
            let left_distance = left.distance_km.unwrap_or(f64::INFINITY);
            let right_distance = right.distance_km.unwrap_or(f64::INFINITY);
            (left_distance - right_distance).abs()
        }
    }
}

fn review_sort_key(pin: &MapPlacePin) -> (u32, u32) {
    (
        pin.review_count.unwrap_or(0),
        pin.rating
            .map(|rating| (rating * 100.0).round() as u32)
            .unwrap_or(0),
    )
}

fn should_keep_place_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && !trimmed.starts_with("way/") && !trimmed.eq_ignore_ascii_case("drain")
}

fn sentence_case(value: &str) -> String {
    let mut characters = value.trim().chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn readable_fact_label(fact_key: &str) -> String {
    sentence_case(
        &fact_key
            .replace(['.', '_'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn fact_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

fn map_water_context(
    property: &Property,
    facts: &ServingFactIndex,
    projection: &SocietyFactProjection<'_>,
) -> Option<MapWaterContext> {
    const KEY: &str = "environment.groundwater_potential_class";
    let fact = projection
        .latest_record(KEY)
        .or_else(|| related_society_groundwater_fact(property, facts, KEY))?;
    let class = match &fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => strip_groundwater_label(value.trim()),
        _ => return None,
    };
    if class.is_empty() {
        return None;
    }
    Some(MapWaterContext {
        groundwater_class: class.clone(),
        summary: format!("{class} groundwater potential zone near the society."),
        scope_radius_km: 3.0,
        source_type: if fact.source_type.trim().is_empty() {
            "OpenCity".to_string()
        } else {
            fact.source_type.clone()
        },
        source_url: fact.source_url.clone(),
        illustrative_zone: true,
    })
}

fn related_society_groundwater_fact<'a>(
    property: &Property,
    facts: &'a ServingFactIndex,
    fact_key: &str,
) -> Option<&'a ServingFactRecord> {
    let target_names = related_society_match_names(property);
    if target_names.is_empty() {
        return None;
    }
    facts.rows().find_map(|(entity_id, rows)| {
        if !entity_id.starts_with("society:") {
            return None;
        }
        if !serving_society_rows_match_names(rows, &target_names) {
            return None;
        }
        rows.facts
            .iter()
            .filter(|fact| fact.fact_key == fact_key)
            .max_by_key(|fact| fact.learned_at)
    })
}

fn place_lookup_by_google_url(facts: &ServingFactIndex) -> HashMap<String, PlaceLookup> {
    let mut by_url = HashMap::new();
    for (entity_id, rows) in facts.rows() {
        if !entity_id.starts_with("place:google:") {
            continue;
        }
        let url = rows
            .facts
            .iter()
            .filter(|fact| fact.fact_key == "google_place_url")
            .filter_map(|fact| match &fact.value {
                FactValue::Text(value) if !value.trim().is_empty() => {
                    Some(value.trim().to_string())
                }
                _ => None,
            })
            .max_by_key(|value| value.len());
        let Some(url) = url else {
            continue;
        };
        let coordinates = resolve_serving_coordinates(rows, CoordinateEntityScope::Place);
        let latitude = coordinates.as_ref().map(|value| value.latitude);
        let longitude = coordinates.as_ref().map(|value| value.longitude);
        let name = text_fact(rows, "place.name");
        let rating = numeric_fact(rows, "google_rating");
        let review_count = numeric_fact(rows, "google_review_count").and_then(|value| {
            if value.is_finite() && (0.0..=u32::MAX as f64).contains(&value) {
                Some(value.round() as u32)
            } else {
                None
            }
        });
        by_url.insert(
            url,
            PlaceLookup {
                entity_id: entity_id.to_string(),
                name,
                latitude,
                longitude,
                rating,
                review_count,
            },
        );
    }
    by_url
}

fn coordinates_for_candidates(facts: &ServingFactIndex, society_id: &str) -> Option<(f64, f64)> {
    for candidate in society_entity_id_candidates(society_id) {
        if let Some(rows) = facts.entity(&candidate) {
            if let Some(coordinates) =
                resolve_serving_coordinates(rows, CoordinateEntityScope::Society)
            {
                return Some((coordinates.latitude, coordinates.longitude));
            }
        }
    }
    None
}

fn society_entity_id_candidates(society_id: &str) -> Vec<String> {
    let raw = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    let slug = raw
        .strip_prefix("society:")
        .or_else(|| raw.strip_prefix("soc-"))
        .unwrap_or(&raw);
    let canonical = format!("society:{slug}");
    if raw == canonical {
        vec![canonical]
    } else {
        vec![canonical, raw]
    }
}

fn numeric_fact(rows: &ServingEntityFactRows, key: &str) -> Option<f64> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .filter_map(finite_numeric_fact)
        .max_by(|left, right| left.total_cmp(right))
}

fn finite_numeric_fact(fact: &crate::serving::ServingFactRecord) -> Option<f64> {
    match &fact.value {
        FactValue::Numeric(value) if value.is_finite() => Some(*value),
        FactValue::Score { value, .. } if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn text_fact(rows: &ServingEntityFactRows, key: &str) -> Option<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key == key)
        .find_map(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            _ => None,
        })
}

#[derive(Debug, PartialEq)]
struct ParsedNearbyDisplay {
    name: String,
    meta: Option<String>,
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
        meta: meta.map(str::to_string),
        distance_km,
        rating,
        review_count,
    }
}

fn pin_note(rating: Option<f64>, review_count: Option<u32>) -> Option<String> {
    match (rating, review_count) {
        (Some(rating), Some(count)) => Some(format!("{rating:.1} · {count} reviews")),
        (Some(rating), None) => Some(format!("{rating:.1} rating")),
        (None, Some(count)) => Some(format!("{count} reviews")),
        (None, None) => None,
    }
}

fn strip_groundwater_label(value: &str) -> String {
    value
        .trim()
        .strip_prefix("groundwater potential:")
        .or_else(|| value.trim().strip_prefix("Groundwater potential:"))
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn distance_sort_key(distance: Option<f64>) -> f64 {
    distance.unwrap_or(f64::INFINITY)
}

fn sort_priority_key(layer: &MapLayerConfig, fact_key: &str) -> usize {
    layer
        .sort_priority_fact_keys
        .iter()
        .position(|candidate| candidate == fact_key)
        .unwrap_or(layer.sort_priority_fact_keys.len())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::models::KgEntityRefs;
    use crate::serving::ServingFactRecord;
    use crate::surfaces::{
        CoordinateQuality, DisplayTone, FillState, SceneAnchor, SceneBoundary, SceneFeature,
        SceneFeatureDisplay, SceneFillRate, SceneGeometry, SceneLayer, SceneMetrics, SceneReceipt,
        SceneViewport, SurfaceSceneResponse,
    };

    fn fact(
        entity_id: &str,
        key: &str,
        value: FactValue,
        source_url: Option<&str>,
        learned_at: i64,
    ) -> ServingFactRecord {
        fact_with_source(entity_id, key, value, "Google", source_url, learned_at)
    }

    fn fact_with_source(
        entity_id: &str,
        key: &str,
        value: FactValue,
        source_type: &str,
        source_url: Option<&str>,
        learned_at: i64,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: source_type.to_string(),
            source_url: source_url.map(str::to_string),
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(learned_at, 0).unwrap(),
        }
    }

    fn sample_property() -> Property {
        Property {
            id: "sample-3bhk".to_string(),
            title: "3 BHK in Assetz Marq".to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "soc-assetz-marq".to_string(),
            builder_name: "Assetz".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 20_000_000,
            price_min: None,
            price_max: None,
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

    #[test]
    fn map_context_projects_from_surface_scene_without_parsing_claims() {
        let scene = SurfaceSceneResponse {
            contract_version: 1,
            surface_id: "around_this_home".to_string(),
            property_id: "sample-3bhk".to_string(),
            serving_bundle_version: Some("test-bundle".to_string()),
            entity_refs: KgEntityRefs {
                property_entity_id: "property:sample".to_string(),
                society_entity_id: "society:sample".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            anchor: SceneAnchor {
                entity_id: "society:sample".to_string(),
                label: "Sample Society".to_string(),
                area: Some("Whitefield".to_string()),
                geometry: Some(SceneGeometry::Point {
                    coordinates: [77.75, 12.98],
                }),
                boundary: Some(SceneBoundary {
                    geometry: SceneGeometry::Polygon {
                        coordinates: vec![vec![
                            [77.74, 12.97],
                            [77.76, 12.97],
                            [77.76, 12.99],
                            [77.74, 12.97],
                        ]],
                    },
                    source_type: "OpenStreetMap".to_string(),
                    source_url: Some("https://www.openstreetmap.org/way/1".to_string()),
                    confidence: 0.78,
                }),
                coordinate_quality: CoordinateQuality::Exact,
            },
            experience: None,
            viewport: SceneViewport {
                center: None,
                bounds: None,
                radius_m: None,
            },
            proof_focus: None,
            layers: vec![SceneLayer {
                id: "schools".to_string(),
                label: "Schools".to_string(),
                family: "access".to_string(),
                render_kind: "pin".to_string(),
                map_presentation: None,
                experience: None,
                empty_state: None,
                feature_value_labels: HashMap::new(),
                relation_class: "access".to_string(),
                enabled_by_default: true,
                rank: 1,
                available_count: 1,
                shown_count: 1,
                fill_state: FillState::Filled,
            }],
            features: vec![SceneFeature {
                id: "around_this_home:schools:place-school".to_string(),
                entity_id: Some("place:school".to_string()),
                layer_id: "schools".to_string(),
                kind: "place".to_string(),
                label: "Green School".to_string(),
                short_label: None,
                details: Vec::new(),
                geometry: SceneGeometry::Point {
                    coordinates: [77.751, 12.981],
                },
                coordinate_quality: CoordinateQuality::Exact,
                metrics: Some(SceneMetrics {
                    distance_m: Some(650),
                    travel_time_min: None,
                    rating: Some(4.2),
                    review_count: Some(120),
                    severity: None,
                }),
                display: SceneFeatureDisplay {
                    tone: DisplayTone::Positive,
                    icon: None,
                    priority: 1,
                },
                properties: HashMap::new(),
                confidence: 0.8,
                receipt_ids: vec!["receipt:school".to_string()],
            }],
            relations: Vec::new(),
            callouts: Vec::new(),
            receipts: vec![SceneReceipt {
                id: "receipt:school".to_string(),
                entity_id: "society:sample".to_string(),
                fact_key: "nearby_schools".to_string(),
                claim: "not parsed for distance".to_string(),
                source_type: "Google".to_string(),
                source_url: Some("https://maps.example/school".to_string()),
                learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                confidence: 0.8,
                scope: None,
            }],
            fill_rate: SceneFillRate {
                filled_layers: 1,
                partial_layers: 0,
                empty_layers: 0,
                shown_features: 1,
                available_features: 1,
                value: 1.0,
            },
            gaps: Vec::new(),
        };

        let context = property_map_context_from_surface_scene(&scene)
            .expect("point scene should project to map context");
        assert_eq!(context.home.name, "Sample Society");
        assert_eq!(context.home.latitude, Some(12.98));
        assert_eq!(
            context
                .home
                .boundary
                .as_ref()
                .map(|boundary| boundary.coordinates.len()),
            Some(4)
        );
        assert_eq!(context.places[0].distance_km, Some(0.65));
        assert_eq!(context.places[0].source_type, "Google");
    }

    #[test]
    fn map_context_projects_evidence_lines_from_surface_scene() {
        let scene = SurfaceSceneResponse {
            contract_version: 1,
            surface_id: "around_this_home".to_string(),
            property_id: "sample-3bhk".to_string(),
            serving_bundle_version: Some("test-bundle".to_string()),
            entity_refs: KgEntityRefs {
                property_entity_id: "property:sample".to_string(),
                society_entity_id: "society:sample".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            anchor: SceneAnchor {
                entity_id: "society:sample".to_string(),
                label: "Sample Society".to_string(),
                area: Some("Whitefield".to_string()),
                geometry: Some(SceneGeometry::Point {
                    coordinates: [77.75, 12.98],
                }),
                boundary: None,
                coordinate_quality: CoordinateQuality::Exact,
            },
            experience: None,
            viewport: SceneViewport {
                center: None,
                bounds: None,
                radius_m: None,
            },
            proof_focus: None,
            layers: vec![SceneLayer {
                id: "red_flags".to_string(),
                label: "Red flags".to_string(),
                family: "risk".to_string(),
                render_kind: "pin".to_string(),
                map_presentation: None,
                experience: None,
                empty_state: None,
                feature_value_labels: HashMap::new(),
                relation_class: "risk_externality".to_string(),
                enabled_by_default: true,
                rank: 1,
                available_count: 1,
                shown_count: 1,
                fill_state: FillState::Filled,
            }],
            features: vec![
                SceneFeature {
                    id: "around_this_home:red_flags:line-one".to_string(),
                    entity_id: Some("place:osm-power-line:one".to_string()),
                    layer_id: "red_flags".to_string(),
                    kind: "place".to_string(),
                    label: "High voltage transmission line".to_string(),
                    short_label: Some("Transmission line".to_string()),
                    details: vec!["220 kV".to_string()],
                    geometry: SceneGeometry::LineString {
                        coordinates: vec![[77.75, 12.98], [77.752, 12.982]],
                    },
                    coordinate_quality: CoordinateQuality::Exact,
                    metrics: Some(SceneMetrics {
                        distance_m: Some(94),
                        travel_time_min: None,
                        rating: None,
                        review_count: None,
                        severity: Some("high".to_string()),
                    }),
                    display: SceneFeatureDisplay {
                        tone: DisplayTone::Risk,
                        icon: Some("flag".to_string()),
                        priority: 1,
                    },
                    properties: HashMap::new(),
                    confidence: 0.8,
                    receipt_ids: vec!["receipt:line".to_string()],
                },
                SceneFeature {
                    id: "around_this_home:metro:access-one".to_string(),
                    entity_id: Some("place:transit-access:one".to_string()),
                    layer_id: "metro".to_string(),
                    kind: "place".to_string(),
                    label: "ECC Road → Kadugodi Tree Park".to_string(),
                    short_label: None,
                    details: Vec::new(),
                    geometry: SceneGeometry::LineString {
                        coordinates: vec![[77.7409, 12.9814], [77.7475, 12.9855]],
                    },
                    coordinate_quality: CoordinateQuality::Exact,
                    metrics: Some(SceneMetrics {
                        distance_m: Some(1_120),
                        travel_time_min: None,
                        rating: None,
                        review_count: None,
                        severity: None,
                    }),
                    display: SceneFeatureDisplay {
                        tone: DisplayTone::Positive,
                        icon: Some("train".to_string()),
                        priority: 1,
                    },
                    properties: HashMap::new(),
                    confidence: 0.78,
                    receipt_ids: vec!["receipt:access".to_string()],
                },
            ],
            relations: Vec::new(),
            callouts: Vec::new(),
            receipts: vec![
                SceneReceipt {
                    id: "receipt:line".to_string(),
                    entity_id: "society:sample".to_string(),
                    fact_key: "high_voltage_transmission_line_nearby".to_string(),
                    claim: "way/1 (94 m, 220 kV)".to_string(),
                    source_type: "OpenStreetMap".to_string(),
                    source_url: Some("https://www.openstreetmap.org/way/1".to_string()),
                    learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    confidence: 0.8,
                    scope: Some("within 100 m".to_string()),
                },
                SceneReceipt {
                    id: "receipt:access".to_string(),
                    entity_id: "society:sample".to_string(),
                    fact_key: "approach_road".to_string(),
                    claim: "ECC Road → Kadugodi Tree Park (1.1 km)".to_string(),
                    source_type: "OpenStreetMap".to_string(),
                    source_url: Some("https://www.openstreetmap.org/way/23213668".to_string()),
                    learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    confidence: 0.78,
                    scope: Some("within 1250 m".to_string()),
                },
            ],
            fill_rate: SceneFillRate {
                filled_layers: 1,
                partial_layers: 0,
                empty_layers: 0,
                shown_features: 1,
                available_features: 1,
                value: 1.0,
            },
            gaps: Vec::new(),
        };

        let context = property_map_context_from_surface_scene(&scene)
            .expect("line scene should still project to map context");
        assert!(context.places.is_empty());
        assert_eq!(context.access_lines.len(), 1);
        assert_eq!(
            context.access_lines[0].name,
            "ECC Road → Kadugodi Tree Park"
        );
        assert_eq!(context.red_flag_lines.len(), 1);
        assert_eq!(
            context.red_flag_lines[0].coordinates,
            vec![[77.75, 12.98], [77.752, 12.982]]
        );
        assert_eq!(context.red_flag_lines[0].source_type, "OpenStreetMap");
        assert_eq!(
            context.red_flag_lines[0].source_url.as_deref(),
            Some("https://www.openstreetmap.org/way/1")
        );
        assert_eq!(
            context.red_flag_lines[0].label.as_deref(),
            Some("Transmission line")
        );
        assert_eq!(context.red_flag_lines[0].distance_km, Some(0.094));
        assert_eq!(context.red_flag_lines[0].details, vec!["220 kV"]);
    }

    #[test]
    fn map_context_joins_nearby_place_coordinates_by_url() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact(
                    "society:assetz-marq",
                    "geo.latitude",
                    FactValue::Numeric(12.98),
                    None,
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "geo.longitude",
                    FactValue::Numeric(77.75),
                    None,
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "nearby_schools",
                    FactValue::Text("Greenwood High (1.2 km, 4.3 rating)".to_string()),
                    Some("https://maps.google.com/greenwood"),
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "nearby_metro_stations",
                    FactValue::Text("Whitefield Metro (2.1 km)".to_string()),
                    Some("https://maps.google.com/metro"),
                    10,
                ),
                fact(
                    "place:google:greenwood",
                    "google_place_url",
                    FactValue::Text("https://maps.google.com/greenwood".to_string()),
                    None,
                    10,
                ),
                fact(
                    "place:google:greenwood",
                    "place.name",
                    FactValue::Text("Greenwood High".to_string()),
                    None,
                    10,
                ),
                fact(
                    "place:google:greenwood",
                    "geo.latitude",
                    FactValue::Numeric(12.985),
                    None,
                    10,
                ),
                fact(
                    "place:google:greenwood",
                    "geo.longitude",
                    FactValue::Numeric(77.752),
                    None,
                    10,
                ),
                fact(
                    "place:google:metro",
                    "google_place_url",
                    FactValue::Text("https://maps.google.com/metro".to_string()),
                    None,
                    10,
                ),
                fact(
                    "place:google:metro",
                    "place.name",
                    FactValue::Text("Whitefield Metro".to_string()),
                    None,
                    10,
                ),
                fact(
                    "place:google:metro",
                    "geo.latitude",
                    FactValue::Numeric(12.99),
                    None,
                    10,
                ),
                fact(
                    "place:google:metro",
                    "geo.longitude",
                    FactValue::Numeric(77.74),
                    None,
                    10,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("map context should build");
        assert_eq!(context.home.latitude, Some(12.98));
        assert_eq!(context.places.len(), 2);
        let school = context
            .places
            .iter()
            .find(|place| place.layer == "schools")
            .expect("school pin");
        assert_eq!(
            school.place_entity_id.as_deref(),
            Some("place:google:greenwood")
        );
        assert_eq!(school.latitude, Some(12.985));
        assert_eq!(school.distance_km, Some(1.2));
    }

    #[test]
    fn map_context_projects_linked_red_flag_place_entities() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact(
                    "society:assetz-marq",
                    "geo.latitude",
                    FactValue::Numeric(12.98),
                    None,
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "geo.longitude",
                    FactValue::Numeric(77.75),
                    None,
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "high_voltage_transmission_line_nearby",
                    FactValue::Text("way/126688602 (94 m, 220 kV, severity: high)".to_string()),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "high_voltage_transmission_line_place_entity",
                    FactValue::Text("place:osm-power-line:way-126688602".to_string()),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "place:osm-power-line:way-126688602",
                    "place.name",
                    FactValue::Text("way/126688602".to_string()),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "place:osm-power-line:way-126688602",
                    "geo.latitude",
                    FactValue::Numeric(12.9807),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "place:osm-power-line:way-126688602",
                    "geo.longitude",
                    FactValue::Numeric(77.7504),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("red flag context should build");
        let red_flag = context
            .places
            .iter()
            .find(|place| place.layer == "red_flags")
            .expect("red flag pin");
        assert_eq!(
            red_flag.place_entity_id.as_deref(),
            Some("place:osm-power-line:way-126688602")
        );
        assert_eq!(red_flag.latitude, Some(12.9807));
        assert_eq!(red_flag.longitude, Some(77.7504));
        assert_eq!(red_flag.distance_km, Some(0.094));
        assert_eq!(red_flag.source_type, "OpenStreetMap");
        assert!(red_flag.name.to_lowercase().contains("transmission line"));
    }

    #[test]
    fn map_context_prioritizes_linked_red_flag_by_primary_evidence_key() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact(
                    "society:assetz-marq",
                    "geo.latitude",
                    FactValue::Numeric(12.98),
                    None,
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "geo.longitude",
                    FactValue::Numeric(77.75),
                    None,
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "nearby_graveyards",
                    FactValue::Text("Burial ground (20 m)".to_string()),
                    "Google",
                    Some("https://maps.google.com/graveyard"),
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "high_voltage_transmission_line_nearby",
                    FactValue::Text("way/126688602 (500 m, 220 kV, severity: high)".to_string()),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "high_voltage_transmission_line_place_entity",
                    FactValue::Text("place:osm-power-line:way-126688602".to_string()),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "place:osm-power-line:way-126688602",
                    "geo.latitude",
                    FactValue::Numeric(12.984),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
                fact_with_source(
                    "place:osm-power-line:way-126688602",
                    "geo.longitude",
                    FactValue::Numeric(77.753),
                    "OpenStreetMap",
                    Some("https://www.openstreetmap.org/way/126688602"),
                    10,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("red flag context should build");
        let first_red_flag = context
            .places
            .iter()
            .find(|place| place.layer == "red_flags")
            .expect("red flag pin");
        assert_eq!(
            first_red_flag.place_entity_id.as_deref(),
            Some("place:osm-power-line:way-126688602")
        );
    }

    #[test]
    fn map_context_prefers_google_coordinates_over_rera_coordinates() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact_with_source(
                    "society:assetz-marq",
                    "geo.latitude",
                    FactValue::Numeric(13.640739),
                    "Rera",
                    None,
                    10,
                ),
                fact_with_source(
                    "society:assetz-marq",
                    "geo.longitude",
                    FactValue::Numeric(78.244397),
                    "Rera",
                    None,
                    10,
                ),
                fact(
                    "society:assetz-marq",
                    "geo.latitude",
                    FactValue::Numeric(12.9819914),
                    None,
                    11,
                ),
                fact(
                    "society:assetz-marq",
                    "geo.longitude",
                    FactValue::Numeric(77.7421819),
                    None,
                    11,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("map context should build");

        assert_eq!(context.home.latitude, Some(12.9819914));
        assert_eq!(context.home.longitude, Some(77.7421819));
    }

    #[test]
    fn map_context_finds_groundwater_on_related_rera_society() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact(
                    "society:rera-assetz",
                    "listing_society",
                    FactValue::Text("Assetz Marq Phase 3A".to_string()),
                    None,
                    10,
                ),
                fact(
                    "society:rera-assetz",
                    "environment.groundwater_potential_class",
                    FactValue::Text("Moderate".to_string()),
                    Some("https://data.opencity.in/example.kml"),
                    10,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("water-backed context should build");
        let water = context.water.expect("groundwater should resolve by name");
        assert_eq!(water.groundwater_class, "Moderate");
        assert_eq!(water.scope_radius_km, 3.0);
        assert!(water.illustrative_zone);
        assert!(water.summary.contains("Moderate"));
    }

    #[test]
    fn map_context_builds_without_home_coordinates() {
        let property = sample_property();
        let serving = ServingFactIndex::from_records(
            vec![
                fact(
                    "society:assetz-marq",
                    "nearby_schools",
                    FactValue::Text("Chrysalis High (0.2 km)".to_string()),
                    Some("https://maps.google.com/school"),
                    10,
                ),
                fact(
                    "place:google:school",
                    "google_place_url",
                    FactValue::Text("https://maps.google.com/school".to_string()),
                    None,
                    10,
                ),
                fact(
                    "place:google:school",
                    "geo.latitude",
                    FactValue::Numeric(12.981),
                    None,
                    10,
                ),
                fact(
                    "place:google:school",
                    "geo.longitude",
                    FactValue::Numeric(77.751),
                    None,
                    10,
                ),
            ],
            Vec::new(),
        );

        let context =
            build_property_map_context(&property, Some("Assetz Marq"), Some(&serving), None)
                .expect("places without home coords should still build");
        assert_eq!(context.home.latitude, None);
        assert_eq!(context.home.longitude, None);
        assert_eq!(context.places.len(), 1);
        assert_eq!(context.places[0].distance_km, Some(0.2));
    }

    #[test]
    fn parse_nearby_display_extracts_name_distance_and_rating() {
        let parsed =
            parse_nearby_display("Hopefarm Channasandra (0.6 km, 4.4 rating, 521 reviews)");
        assert_eq!(parsed.name, "Hopefarm Channasandra");
        assert_eq!(parsed.distance_km, Some(0.6));
        assert_eq!(parsed.rating, Some(4.4));
        assert_eq!(parsed.review_count, Some(521));
    }
}
