//! Structured neighborhood plate projection for property detail.
//!
//! Joins society nearby facts to place-entity coordinates by Google place URL,
//! and surfaces groundwater class when available. Soft zone geometry is
//! illustrative in the UI — this payload carries the receipt, not polygons.

use std::collections::HashMap;

use serde::Serialize;

use crate::knowledge::FactValue;
use crate::models::Property;
use crate::search::geo::{extract_first_distance_km, haversine_km};
use crate::serving::{
    ServingEntityFactRows, ServingFactIndex, ServingFactRecord, SocietyFactProjection,
};

use super::enrichment::society_node_id;

const NEARBY_LAYERS: &[(&str, &str, usize)] = &[
    ("nearby_metro_stations", "metro", 2),
    ("nearby_schools", "schools", 3),
    ("nearby_hospitals", "hospitals", 2),
    ("nearby_tech_parks", "tech", 2),
];
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

    let mut places = Vec::new();
    for &(fact_key, layer, cap) in NEARBY_LAYERS {
        let mut layer_pins = projection
            .records(fact_key)
            .into_iter()
            .filter_map(|fact| map_place_pin(fact, layer, &place_by_url, home_coords.as_ref()))
            .collect::<Vec<_>>();
        layer_pins.sort_by(|left, right| {
            distance_sort_key(left.distance_km)
                .partial_cmp(&distance_sort_key(right.distance_km))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.name.cmp(&right.name))
        });
        layer_pins
            .dedup_by(|left, right| left.name == right.name && left.source_url == right.source_url);
        layer_pins.truncate(cap);
        if layer == "metro" {
            enrich_metro_pins_from_dag(&mut layer_pins, &dag_metro_stations);
        }
        places.extend(layer_pins);
    }

    let water = map_water_context(property, facts, &projection);
    let overlay_home = home_coords.or_else(|| approximate_home_from_places(&places));
    let (metro_lines, green_patches, lakes) = match (map_overlays, overlay_home) {
        (Some(overlays), Some(home)) => {
            let metro_stations = metro_corridor_anchors(&places, &dag_metro_stations, home);
            let metro_lines = crate::routes::map_overlays::nearest_metro_corridor(
                overlays,
                home,
                &metro_stations,
            );
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
        },
        places,
        water,
        metro_lines,
        green_patches,
        lakes,
    })
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
        });
    }
}

fn approximate_home_from_places(places: &[MapPlacePin]) -> Option<(f64, f64)> {
    let mut ranked = places
        .iter()
        .filter_map(|place| match (place.latitude, place.longitude) {
            (Some(lat), Some(lng)) => Some((place.distance_km.unwrap_or(f64::INFINITY), lat, lng)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        return None;
    }
    ranked.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let near = &ranked[..ranked.len().min(3)];
    let lat = near.iter().map(|item| item.1).sum::<f64>() / near.len() as f64;
    let lng = near.iter().map(|item| item.2).sum::<f64>() / near.len() as f64;
    Some((lat, lng))
}

fn dag_metro_stations(facts: &ServingFactIndex) -> Vec<DagMetroStation> {
    let mut stations = Vec::new();
    for (entity_id, rows) in facts.rows() {
        if !entity_id.starts_with("place:metro:") {
            continue;
        }
        let Some(latitude) = numeric_fact(rows, "geo.latitude") else {
            continue;
        };
        let Some(longitude) = numeric_fact(rows, "geo.longitude") else {
            continue;
        };
        if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
            continue;
        }
        let name = text_fact(rows, "place.name").unwrap_or_else(|| entity_id.to_string());
        let lines = tags_fact(rows, "transit.lines")
            .into_iter()
            .filter_map(|value| crate::routes::map_overlays::trunk_line_display_name(&value))
            .collect::<Vec<_>>();
        stations.push(DagMetroStation {
            entity_id: entity_id.to_string(),
            name,
            latitude,
            longitude,
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

fn map_place_pin(
    fact: &ServingFactRecord,
    layer: &str,
    place_by_url: &HashMap<String, PlaceLookup>,
    home_coords: Option<&(f64, f64)>,
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
    })
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

fn related_society_match_names(property: &Property) -> Vec<String> {
    let mut names = vec![project_name_for(property), property.society_id.clone()];
    if let Some(society_slug) = property
        .society_id
        .strip_prefix("soc-")
        .or_else(|| property.society_id.strip_prefix("society:"))
    {
        names.push(society_slug.replace('-', " "));
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| normalized_project_name(&name))
        .collect()
}

fn serving_society_rows_match_names(rows: &ServingEntityFactRows, target_names: &[String]) -> bool {
    const NAME_FACT_KEYS: &[&str] = &["listing_society", "title", "rera_project_name"];
    rows.facts.iter().any(|fact| {
        NAME_FACT_KEYS.contains(&fact.fact_key.as_str())
            && match &fact.value {
                FactValue::Text(value) => normalized_project_name(value).is_some_and(|name| {
                    target_names
                        .iter()
                        .any(|target| names_compatible(target, &name))
                }),
                _ => false,
            }
    })
}

fn names_compatible(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_tokens = left.split_whitespace().collect::<Vec<_>>();
    let right_tokens = right.split_whitespace().collect::<Vec<_>>();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return false;
    }
    let (smaller, larger) = if left_tokens.len() <= right_tokens.len() {
        (&left_tokens, &right_tokens)
    } else {
        (&right_tokens, &left_tokens)
    };
    // Require the shorter name's tokens to appear in order in the longer name.
    // "assetz marq" matches "assetz marq phase 3a".
    let mut start = 0usize;
    for token in smaller {
        match larger[start..]
            .iter()
            .position(|candidate| candidate == token)
        {
            Some(index) => start += index + 1,
            None => return false,
        }
    }
    true
}

fn normalized_project_name(value: &str) -> Option<String> {
    let normalized = value
        .to_lowercase()
        .replace('&', " and ")
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| {
            !token.is_empty()
                && !matches!(
                    *token,
                    "soc" | "society" | "rera" | "project" | "phase" | "the"
                )
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn project_name_for(property: &Property) -> String {
    let prefix = format!("{} BHK in ", property.bhk);
    property
        .title
        .strip_prefix(&prefix)
        .unwrap_or(&property.title)
        .to_string()
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
        let latitude = numeric_fact(rows, "geo.latitude");
        let longitude = numeric_fact(rows, "geo.longitude");
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
            let latitude = preferred_coordinate_fact(rows, "geo.latitude")
                .or_else(|| preferred_coordinate_fact(rows, "project_latitude"))?;
            let longitude = preferred_coordinate_fact(rows, "geo.longitude")
                .or_else(|| preferred_coordinate_fact(rows, "project_longitude"))?;
            if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) {
                return Some((latitude, longitude));
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

fn preferred_coordinate_fact(rows: &ServingEntityFactRows, key: &str) -> Option<f64> {
    coordinate_fact_from_sources(rows, key, &["Google", "Manual"])
        .or_else(|| coordinate_fact_excluding_sources(rows, key, &["Rera"]))
}

fn coordinate_fact_from_sources(
    rows: &ServingEntityFactRows,
    key: &str,
    source_types: &[&str],
) -> Option<f64> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .filter(|fact| {
            source_types
                .iter()
                .any(|source_type| fact.source_type.eq_ignore_ascii_case(source_type))
        })
        .filter_map(finite_numeric_fact)
        .max_by(|left, right| left.total_cmp(right))
}

fn coordinate_fact_excluding_sources(
    rows: &ServingEntityFactRows,
    key: &str,
    excluded_source_types: &[&str],
) -> Option<f64> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .filter(|fact| {
            !excluded_source_types
                .iter()
                .any(|source_type| fact.source_type.eq_ignore_ascii_case(source_type))
        })
        .filter_map(finite_numeric_fact)
        .max_by(|left, right| left.total_cmp(right))
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

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::serving::ServingFactRecord;

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
        assert_eq!(context.home.latitude, Some(12.981));
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
