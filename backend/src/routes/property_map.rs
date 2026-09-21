//! Typed map projection of the single promoted property-context scene.
use crate::surfaces::{SceneGeometry, SurfaceSceneResponse};
use serde::Serialize;
use std::collections::HashMap;
#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct PropertyMapContext {
    pub home: MapHomeAnchor,
    #[serde(default)]
    pub places: Vec<MapPlacePin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "MapWaterContext")]
    pub water: Option<MapWaterContext>,
    #[serde(default)]
    pub metro_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default)]
    pub access_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default)]
    pub red_flag_lines: Vec<crate::routes::map_overlays::MapOverlayLine>,
    #[serde(default)]
    pub green_patches: Vec<crate::routes::map_overlays::MapOverlayPolygon>,
    #[serde(default)]
    pub lakes: Vec<crate::routes::map_overlays::MapOverlayPolygon>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct MapHomeAnchor {
    pub entity_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub area: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "crate::routes::map_overlays::MapOverlayPolygon")]
    pub boundary: Option<crate::routes::map_overlays::MapOverlayPolygon>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct MapPlacePin {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub place_entity_id: Option<String>,
    pub layer: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub distance_km: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub note: Option<String>,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    pub source_type: String,
    #[serde(skip)]
    sort_priority: usize,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct MapWaterContext {
    pub groundwater_class: String,
    pub summary: String,
    pub scope_radius_km: f64,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    /// Soft zone fill is UI geometry; class/summary are source-backed.
    pub illustrative_zone: bool,
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
                .find_map(|receipt_id| receipts_by_id.get(receipt_id.as_str()))?;
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
                source_url: receipt.source_url.clone(),
                source_type: receipt.source_type.clone(),
                sort_priority: 0,
            })
        })
        .collect::<Vec<_>>();
    let line_from_feature = |feature: &crate::surfaces::SceneFeature| {
        let coordinates = line_coordinates(&feature.geometry)?;
        let receipt = feature
            .receipt_ids
            .iter()
            .find_map(|receipt_id| receipts_by_id.get(receipt_id.as_str()))?;
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
            source_type: receipt.source_type.clone(),
            source_url: receipt.source_url.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KgEntityRefs;
    use crate::surfaces::*;
    use chrono::{TimeZone, Utc};
    #[test]
    fn map_context_projects_from_surface_scene_without_parsing_claims() {
        let scene = SurfaceSceneResponse {
            snapshot_identity: "fixture".to_string(),
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
            proof_focus_status: crate::surfaces::ProofFocusStatus::NotRequested,
            proof_focus_message: None,
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
                evidence: crate::serving::EvidenceRef::for_observation(
                    "fixture",
                    &crate::serving::SourceObservation::new(
                        "Fixture",
                        "fixture-map",
                        "society:one",
                        chrono::Utc::now(),
                        None,
                        vec!["fixture/map".to_string()],
                    )
                    .unwrap(),
                ),
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
            snapshot_identity: "fixture".to_string(),
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
            proof_focus_status: crate::surfaces::ProofFocusStatus::NotRequested,
            proof_focus_message: None,
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
                    evidence: crate::serving::EvidenceRef::for_observation(
                        "fixture",
                        &crate::serving::SourceObservation::new(
                            "Fixture",
                            "fixture-map",
                            "society:one",
                            chrono::Utc::now(),
                            None,
                            vec!["fixture/map".to_string()],
                        )
                        .unwrap(),
                    ),
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
                    evidence: crate::serving::EvidenceRef::for_observation(
                        "fixture",
                        &crate::serving::SourceObservation::new(
                            "Fixture",
                            "fixture-map",
                            "society:one",
                            chrono::Utc::now(),
                            None,
                            vec!["fixture/map".to_string()],
                        )
                        .unwrap(),
                    ),
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
}
