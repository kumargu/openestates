//! City map overlay seeds (metro lines, parks, lakes) clipped per property.
//!
//! Geometry is loaded offline from `data/seed/map/*.geojson` at startup — never
//! fetched on the request path. Clipping keeps payloads small for the plate UI.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::search::geo::haversine_km;

const METRO_CORRIDOR_RADIUS_KM: f64 = 6.0;
const METRO_STATION_MATCH_KM: f64 = 2.5;
const METRO_SEGMENT_JOIN_KM: f64 = 0.15;
const GREEN_RADIUS_KM: f64 = 4.0;
const MAX_METRO_SEGMENTS: usize = 24;
const MAX_PARKS: usize = 12;
const MAX_LAKES: usize = 8;

#[derive(Clone, Debug, Default)]
pub struct CityMapOverlays {
    pub metro_lines: Vec<SeedLine>,
    pub parks: Vec<SeedPolygon>,
    pub lakes: Vec<SeedPolygon>,
}

#[derive(Clone, Debug)]
pub struct SeedLine {
    pub id: String,
    pub name: String,
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Clone, Debug)]
pub struct SeedPolygon {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub ring: Vec<[f64; 2]>,
    pub centroid: (f64, f64),
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapOverlayLine {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub coordinates: Vec<[f64; 2]>,
    pub source_type: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MapOverlayPolygon {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub coordinates: Vec<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    pub source_type: String,
}

#[derive(Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Option<FeatureProps>,
    geometry: Option<Geometry>,
}

#[derive(Deserialize)]
struct FeatureProps {
    id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    geom_type: String,
    coordinates: Value,
}

pub fn load_city_map_overlays(project_root: &Path) -> Arc<CityMapOverlays> {
    let root = project_root.join("data").join("seed").join("map");
    let metro_lines = load_lines(&root.join("bengaluru_metro_lines.geojson"));
    let parks = load_polygons(&root.join("bengaluru_parks.geojson"), "park");
    let lakes = load_polygons(&root.join("bengaluru_lakes.geojson"), "lake");
    println!(
        "Loaded map overlays: {} metro segments, {} parks, {} lakes",
        metro_lines.len(),
        parks.len(),
        lakes.len()
    );
    Arc::new(CityMapOverlays {
        metro_lines,
        parks,
        lakes,
    })
}

pub fn nearest_metro_corridor(
    overlays: &CityMapOverlays,
    home: (f64, f64),
    metro_stations: &[(f64, f64)],
) -> Vec<MapOverlayLine> {
    let anchor = metro_stations
        .iter()
        .min_by(|left, right| {
            haversine_km(home.0, home.1, left.0, left.1)
                .total_cmp(&haversine_km(home.0, home.1, right.0, right.1))
        })
        .copied()
        .unwrap_or(home);

    let Some((seed_index, seed_distance)) = overlays
        .metro_lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, nearest_line_distance_km(line, anchor)))
        .min_by(|left, right| left.1.total_cmp(&right.1))
    else {
        return Vec::new();
    };
    if seed_distance > METRO_STATION_MATCH_KM {
        return Vec::new();
    }

    let mut selected = vec![false; overlays.metro_lines.len()];
    selected[seed_index] = true;
    let mut frontier = vec![seed_index];
    while let Some(current_index) = frontier.pop() {
        let current = &overlays.metro_lines[current_index];
        for (candidate_index, candidate) in overlays.metro_lines.iter().enumerate() {
            if selected[candidate_index] || !lines_connect(current, candidate) {
                continue;
            }
            selected[candidate_index] = true;
            frontier.push(candidate_index);
        }
    }

    let corridor_name = overlays.metro_lines[seed_index].name.clone();
    let mut corridor = overlays
        .metro_lines
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
        .filter_map(|(_, line)| {
            let coordinates = clip_line_near(
                &line.coordinates,
                anchor.0,
                anchor.1,
                METRO_CORRIDOR_RADIUS_KM,
            );
            (coordinates.len() >= 2).then(|| MapOverlayLine {
                id: line.id.clone(),
                name: corridor_name.clone(),
                kind: "metro_line".to_string(),
                coordinates,
                source_type: "OpenStreetMap".to_string(),
            })
        })
        .collect::<Vec<_>>();
    corridor.sort_by(|left, right| left.id.cmp(&right.id));
    corridor.truncate(MAX_METRO_SEGMENTS);
    corridor
}

fn nearest_line_distance_km(line: &SeedLine, anchor: (f64, f64)) -> f64 {
    line.coordinates
        .iter()
        .map(|[lng, lat]| haversine_km(anchor.0, anchor.1, *lat, *lng))
        .fold(f64::INFINITY, f64::min)
}

fn lines_connect(left: &SeedLine, right: &SeedLine) -> bool {
    let Some(left_start) = left.coordinates.first() else {
        return false;
    };
    let Some(left_end) = left.coordinates.last() else {
        return false;
    };
    let Some(right_start) = right.coordinates.first() else {
        return false;
    };
    let Some(right_end) = right.coordinates.last() else {
        return false;
    };
    [left_start, left_end].iter().any(|left_point| {
        [right_start, right_end].iter().any(|right_point| {
            haversine_km(left_point[1], left_point[0], right_point[1], right_point[0])
                <= METRO_SEGMENT_JOIN_KM
        })
    })
}

pub fn clip_green_patches(
    overlays: &CityMapOverlays,
    home: (f64, f64),
) -> (Vec<MapOverlayPolygon>, Vec<MapOverlayPolygon>) {
    let parks = clip_polygons(&overlays.parks, home, GREEN_RADIUS_KM, MAX_PARKS);
    let lakes = clip_polygons(&overlays.lakes, home, GREEN_RADIUS_KM, MAX_LAKES);
    (parks, lakes)
}

fn clip_polygons(
    polygons: &[SeedPolygon],
    home: (f64, f64),
    radius_km: f64,
    limit: usize,
) -> Vec<MapOverlayPolygon> {
    let (lat, lng) = home;
    let mut scored = polygons
        .iter()
        .filter_map(|polygon| {
            let distance = haversine_km(lat, lng, polygon.centroid.0, polygon.centroid.1);
            if distance > radius_km {
                return None;
            }
            Some((
                distance,
                MapOverlayPolygon {
                    id: polygon.id.clone(),
                    name: polygon.name.clone(),
                    kind: polygon.kind.clone(),
                    coordinates: polygon.ring.clone(),
                    distance_km: Some((distance * 10.0).round() / 10.0),
                    source_type: "OpenStreetMap".to_string(),
                },
            ))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, polygon)| polygon)
        .collect()
}

fn clip_line_near(
    coordinates: &[[f64; 2]],
    home_lat: f64,
    home_lng: f64,
    radius_km: f64,
) -> Vec<[f64; 2]> {
    // Keep vertices inside the radius, plus one neighbor outside so the stretch
    // still reads as a continuous corridor on the plate.
    let mut keep = Vec::new();
    for (index, point) in coordinates.iter().enumerate() {
        let distance = haversine_km(home_lat, home_lng, point[1], point[0]);
        if distance <= radius_km {
            if index > 0 {
                let prev = coordinates[index - 1];
                if keep.last() != Some(&prev) {
                    keep.push(prev);
                }
            }
            keep.push(*point);
            if index + 1 < coordinates.len() {
                let next = coordinates[index + 1];
                if keep.last() != Some(&next) {
                    keep.push(next);
                }
            }
        }
    }
    dedupe_points(keep)
}

fn dedupe_points(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for point in points {
        if out.last() != Some(&point) {
            out.push(point);
        }
    }
    out
}

fn load_lines(path: &PathBuf) -> Vec<SeedLine> {
    let Some(collection) = read_collection(path) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for feature in collection.features {
        let props = feature.properties.unwrap_or(FeatureProps {
            id: None,
            name: None,
            kind: None,
        });
        let Some(geometry) = feature.geometry else {
            continue;
        };
        for (part_index, coords) in line_parts(&geometry).into_iter().enumerate() {
            if coords.len() < 2 {
                continue;
            }
            let id = props
                .id
                .clone()
                .unwrap_or_else(|| format!("metro-{part_index}"));
            lines.push(SeedLine {
                id: if part_index == 0 {
                    id
                } else {
                    format!("{id}-{part_index}")
                },
                name: props.name.clone().unwrap_or_else(|| "Metro".to_string()),
                coordinates: coords,
            });
        }
    }
    lines
}

fn load_polygons(path: &PathBuf, default_kind: &str) -> Vec<SeedPolygon> {
    let Some(collection) = read_collection(path) else {
        return Vec::new();
    };
    let mut polygons = Vec::new();
    for feature in collection.features {
        let props = feature.properties.unwrap_or(FeatureProps {
            id: None,
            name: None,
            kind: None,
        });
        let Some(geometry) = feature.geometry else {
            continue;
        };
        for (part_index, ring) in polygon_rings(&geometry).into_iter().enumerate() {
            if ring.len() < 4 {
                continue;
            }
            let centroid = ring_centroid(&ring);
            let id = props
                .id
                .clone()
                .unwrap_or_else(|| format!("{default_kind}-{part_index}"));
            polygons.push(SeedPolygon {
                id: if part_index == 0 {
                    id
                } else {
                    format!("{id}-{part_index}")
                },
                name: props
                    .name
                    .clone()
                    .unwrap_or_else(|| default_kind.to_string()),
                kind: props
                    .kind
                    .clone()
                    .unwrap_or_else(|| default_kind.to_string()),
                ring,
                centroid,
            });
        }
    }
    polygons
}

fn read_collection(path: &PathBuf) -> Option<FeatureCollection> {
    if !path.exists() {
        println!("WARN: map overlay missing at {}", path.display());
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn line_parts(geometry: &Geometry) -> Vec<Vec<[f64; 2]>> {
    match geometry.geom_type.as_str() {
        "LineString" => parse_line_value(&geometry.coordinates)
            .into_iter()
            .collect(),
        "MultiLineString" => geometry
            .coordinates
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(parse_line_value)
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_line_value(value: &Value) -> Option<Vec<[f64; 2]>> {
    let points = value.as_array()?;
    let mut out = Vec::with_capacity(points.len());
    for point in points {
        let pair = point.as_array()?;
        if pair.len() < 2 {
            return None;
        }
        out.push([pair[0].as_f64()?, pair[1].as_f64()?]);
    }
    Some(out)
}

fn polygon_rings(geometry: &Geometry) -> Vec<Vec<[f64; 2]>> {
    match geometry.geom_type.as_str() {
        "Polygon" => geometry
            .coordinates
            .as_array()
            .and_then(|rings| rings.first())
            .and_then(parse_line_value)
            .into_iter()
            .collect(),
        "MultiPolygon" => geometry
            .coordinates
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|polygon| {
                polygon
                    .as_array()
                    .and_then(|rings| rings.first())
                    .and_then(parse_line_value)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn ring_centroid(ring: &[[f64; 2]]) -> (f64, f64) {
    let usable = if ring.len() > 1 && ring.first() == ring.last() {
        &ring[..ring.len() - 1]
    } else {
        ring
    };
    if usable.is_empty() {
        return (0.0, 0.0);
    }
    let lng = usable.iter().map(|point| point[0]).sum::<f64>() / usable.len() as f64;
    let lat = usable.iter().map(|point| point[1]).sum::<f64>() / usable.len() as f64;
    (lat, lng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_metro_line_within_radius() {
        let overlays = CityMapOverlays {
            metro_lines: vec![SeedLine {
                id: "line-1".to_string(),
                name: "Purple".to_string(),
                coordinates: vec![[77.75, 12.98], [77.76, 12.99], [77.90, 13.10]],
            }],
            parks: Vec::new(),
            lakes: Vec::new(),
        };
        let clipped = nearest_metro_corridor(&overlays, (12.985, 77.755), &[(12.985, 77.755)]);
        assert_eq!(clipped.len(), 1);
        assert!(clipped[0].coordinates.len() >= 2);
    }

    #[test]
    fn follows_connected_segments_for_one_station_corridor() {
        let overlays = CityMapOverlays {
            metro_lines: vec![
                SeedLine {
                    id: "line-a".to_string(),
                    name: "Purple".to_string(),
                    coordinates: vec![[77.75, 12.98], [77.76, 12.99]],
                },
                SeedLine {
                    id: "line-b".to_string(),
                    name: "Purple east".to_string(),
                    coordinates: vec![[77.76, 12.99], [77.77, 13.0]],
                },
                SeedLine {
                    id: "other".to_string(),
                    name: "Green".to_string(),
                    coordinates: vec![[77.60, 12.90], [77.61, 12.91]],
                },
            ],
            parks: Vec::new(),
            lakes: Vec::new(),
        };

        let corridor = nearest_metro_corridor(&overlays, (12.98, 77.75), &[(12.981, 77.751)]);

        assert_eq!(corridor.len(), 2);
        assert!(corridor.iter().all(|line| line.name == "Purple"));
        assert!(corridor.iter().all(|line| line.id != "other"));
    }

    #[test]
    fn clips_parks_within_green_radius() {
        let overlays = CityMapOverlays {
            metro_lines: Vec::new(),
            parks: vec![SeedPolygon {
                id: "park-1".to_string(),
                name: "Tree Park".to_string(),
                kind: "park".to_string(),
                ring: vec![
                    [77.76, 13.02],
                    [77.761, 13.02],
                    [77.761, 13.021],
                    [77.76, 13.021],
                    [77.76, 13.02],
                ],
                centroid: (13.0205, 77.7605),
            }],
            lakes: Vec::new(),
        };
        let (parks, lakes) = clip_green_patches(&overlays, (13.024, 77.761));
        assert_eq!(parks.len(), 1);
        assert!(lakes.is_empty());
        assert_eq!(parks[0].name, "Tree Park");
    }
}
