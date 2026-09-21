//! Bounded spatial context over promoted facts. No layout, layers, camera or DOM identity.
use crate::knowledge::FactValue;
use crate::models::{KgEntityRefs, Property};
use crate::search::proof::ProofResolution;
use crate::serving::{EvidenceRef, LoadedServingBundle, ServingFactRecord};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct ContextConfig {
    maximum_features: usize,
    geometry_fact_key: String,
    place_url_fact_key: String,
    attribute_fact_keys: Vec<String>,
    bindings: BTreeMap<String, ContextBinding>,
}
#[derive(Debug, Deserialize)]
struct ContextBinding {
    linked_entity_fact_keys: Vec<String>,
    attribute_fact_keys: Vec<String>,
}
fn config() -> &'static ContextConfig {
    static CONFIG: OnceLock<ContextConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        #[derive(Deserialize)]
        struct Registry {
            property_context: ContextConfig,
        }
        let registry: Registry =
            crate::dag_config::load_json(&crate::dag_config::dag_root().join("fact_registry.json"))
                .expect("fact registry must bind property context");
        assert!(registry.property_context.maximum_features > 0);
        registry.property_context
    })
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFact {
    pub id: String,
    pub entity_id: String,
    pub fact_key: String,
    pub value: FactValue,
    pub evidence: EvidenceRef,
    pub source_type: String,
    pub source_url: Option<String>,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub confidence: f32,
}
#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntity {
    pub entity_id: String,
    pub name: String,
    pub point: Option<[f64; 2]>,
    pub geometry: Option<ContextGeometry>,
    pub geometry_evidence: Vec<EvidenceRef>,
    pub geometry_source: Option<ContextFact>,
}
#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFeature {
    pub fact: ContextFact,
    pub target: Option<ContextEntity>,
    pub attributes: Vec<ContextFact>,
    // Distance is a reproducible computation over durable spatial inputs.
    pub distance: Option<crate::serving::DerivedEvidence>,
}
#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyContext {
    pub contract_version: u32,
    pub snapshot_identity: String,
    pub property_id: String,
    pub entity_refs: KgEntityRefs,
    pub anchor: ContextEntity,
    pub features: Vec<ContextFeature>,
    pub truncated: bool,
    pub matched_proof: Option<ProofResolution>,
}

fn fact_view(bundle: &LoadedServingBundle, fact: &ServingFactRecord) -> Option<ContextFact> {
    fact.validate_observation().ok()?;
    let observation = fact.observation.as_ref()?;
    // Only the immutable evidence index can authorize a buyer receipt.
    bundle
        .evidence_index
        .fact(&observation.observation_id, &fact.fact_key)?;
    Some(ContextFact {
        id: format!("{}:{}", observation.observation_id.as_str(), fact.fact_key),
        entity_id: fact.entity_id.clone(),
        fact_key: fact.fact_key.clone(),
        value: fact.value.clone(),
        evidence: EvidenceRef::for_observation(
            bundle.manifest.proof_snapshot_identity(),
            observation,
        ),
        source_type: observation.provider.clone(),
        source_url: observation.source_url.clone(),
        observed_at: observation.observed_at,
        confidence: fact.confidence,
    })
}
fn entity_view(bundle: &LoadedServingBundle, entity_id: &str) -> Option<ContextEntity> {
    let entity = bundle
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)?;
    let rows = bundle.fact_index.entity(entity_id);
    let geometry_fact = rows
        .into_iter()
        .flat_map(|rows| &rows.facts)
        .filter(|fact| fact.fact_key == config().geometry_fact_key)
        .find_map(|fact| {
            let view = fact_view(bundle, fact)?;
            let FactValue::Text(value) = &fact.value else {
                return None;
            };
            Some((geometry_from_geojson(value)?, view))
        });
    let point = bundle.spatial_index.point_for_entity(entity_id);
    let mut geometry_evidence = geometry_fact
        .as_ref()
        .map(|(_, fact)| vec![fact.evidence.clone()])
        .unwrap_or_default();
    // Point coordinates are admitted by the spatial index using source observations.
    if let Some(point) = point {
        geometry_evidence.extend(point.observations.iter().map(|observation| {
            EvidenceRef::for_observation(bundle.manifest.proof_snapshot_identity(), observation)
        }));
    }
    geometry_evidence.sort_by_key(EvidenceRef::stable_key);
    geometry_evidence.dedup();
    Some(ContextEntity {
        entity_id: entity_id.to_string(),
        name: entity.name.clone(),
        point: point
            .filter(|_| !geometry_evidence.is_empty())
            .map(|p| [p.longitude, p.latitude]),
        geometry: geometry_fact.as_ref().map(|(geometry, _)| geometry.clone()),
        geometry_evidence,
        geometry_source: geometry_fact.map(|(_, fact)| fact),
    })
}

/// Rebuildable lookup over promoted provider identities; created once per snapshot.
#[derive(Debug, Clone)]
pub struct ContextLookup {
    url_entities: BTreeMap<String, BTreeSet<String>>,
}
impl ContextLookup {
    pub fn from_bundle(bundle: &LoadedServingBundle) -> Self {
        let mut url_entities = BTreeMap::<String, BTreeSet<String>>::new();
        for (id, rows) in bundle.fact_index.rows() {
            for fact in &rows.facts {
                if fact.fact_key == config().place_url_fact_key {
                    if let FactValue::Text(url) = &fact.value {
                        url_entities
                            .entry(url.clone())
                            .or_default()
                            .insert(id.to_string());
                    }
                }
            }
        }
        Self { url_entities }
    }
}

/// Facts are subject-scoped; names and row positions never establish joins.
pub fn build_property_context(
    property: &Property,
    refs: KgEntityRefs,
    bundle: &LoadedServingBundle,
    lookup: &ContextLookup,
    matched_proof: Option<ProofResolution>,
) -> PropertyContext {
    let anchor = entity_view(bundle, &refs.society_entity_id).unwrap_or_else(|| ContextEntity {
        entity_id: refs.society_entity_id.clone(),
        name: property.title.clone(),
        point: None,
        geometry: None,
        geometry_evidence: Vec::new(),
        geometry_source: None,
    });
    let rows = bundle.fact_index.entity(&anchor.entity_id);
    let mut features = Vec::new();
    for fact in rows.into_iter().flat_map(|rows| &rows.facts) {
        let Some(binding) = config().bindings.get(&fact.fact_key) else {
            continue;
        };
        let Some(view) = fact_view(bundle, fact) else {
            continue;
        };
        let linked = rows
            .into_iter()
            .flat_map(|rows| &rows.facts)
            .filter(|candidate| {
                binding
                    .linked_entity_fact_keys
                    .contains(&candidate.fact_key)
            })
            .filter(|candidate| {
                candidate
                    .observation
                    .as_ref()
                    .zip(fact.observation.as_ref())
                    .is_some_and(|(left, right)| left.observation_id == right.observation_id)
                    || fact
                        .source_url
                        .as_ref()
                        .is_some_and(|url| candidate.source_url.as_ref() == Some(url))
            })
            .filter_map(|candidate| match &candidate.value {
                FactValue::Text(id) => Some(id.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let target_id = if linked.len() == 1 {
            linked.first().copied()
        } else {
            None
        }
        .or_else(|| {
            fact.source_url
                .as_deref()
                .and_then(|url| lookup.url_entities.get(url))
                .filter(|ids| ids.len() == 1)
                .and_then(|ids| ids.first().map(String::as_str))
        });
        let target = target_id.and_then(|id| entity_view(bundle, id));
        let attribute_keys = binding
            .attribute_fact_keys
            .iter()
            .chain(&config().attribute_fact_keys)
            .collect::<BTreeSet<_>>();
        let attributes = target_id
            .and_then(|id| bundle.fact_index.entity(id))
            .into_iter()
            .flat_map(|rows| &rows.facts)
            .filter(|fact| attribute_keys.contains(&fact.fact_key))
            .filter_map(|fact| fact_view(bundle, fact))
            .collect();
        let distance = target_id.and_then(|target| {
            let distance = bundle.spatial_index.distance_between(
                &anchor.entity_id,
                target,
                bundle.manifest.proof_snapshot_identity(),
            )?;
            crate::serving::DerivedEvidence::new(
                bundle.manifest.proof_snapshot_identity(),
                &anchor.entity_id,
                Some(target.to_string()),
                "distance",
                distance.metric,
                Some(distance.distance_km),
                Some("km".to_string()),
                "spatial-evaluator-v2",
                distance.confidence,
                distance.evidence_refs,
            )
            .ok()
        });
        features.push(ContextFeature {
            fact: view,
            target,
            attributes,
            distance,
        });
    }
    features.sort_by(|a, b| a.fact.id.cmp(&b.fact.id));
    features.dedup_by(|a, b| a.fact.id == b.fact.id);
    let truncated = features.len() > config().maximum_features;
    // Keep the matched observation even when the bounded resource is truncated.
    let focused = matched_proof
        .as_ref()
        .and_then(|proof| {
            features.iter().find(|feature| {
                feature.fact.fact_key == proof.fact_key
                    && proof.source_observations.iter().any(|source| {
                        feature.fact.evidence.evidence_id
                            == crate::serving::EvidenceId::Observation(
                                source.observation_id.clone(),
                            )
                    })
            })
        })
        .cloned();
    features.truncate(config().maximum_features);
    if let Some(focused) = focused {
        if !features
            .iter()
            .any(|feature| feature.fact.id == focused.fact.id)
        {
            features.push(focused);
        }
    }
    PropertyContext {
        contract_version: 1,
        snapshot_identity: bundle.manifest.proof_snapshot_identity().to_string(),
        property_id: property.id.clone(),
        entity_refs: refs,
        anchor,
        features,
        truncated,
        matched_proof,
    }
}
fn point_geometry((latitude, longitude): (f64, f64)) -> ContextGeometry {
    ContextGeometry::Point {
        coordinates: [longitude, latitude],
    }
}
#[derive(schemars::JsonSchema, Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum ContextGeometry {
    Point {
        coordinates: [f64; 2],
    },
    LineString {
        coordinates: Vec<[f64; 2]>,
    },
    Polygon {
        coordinates: Vec<Vec<[f64; 2]>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<[f64; 2]>>>,
    },
}

fn geometry_from_geojson(value: &str) -> Option<ContextGeometry> {
    let parsed = value.parse::<GeoJson>().ok()?;
    match parsed {
        GeoJson::Geometry(geometry) => geometry_from_geojson_value(&geometry.value),
        GeoJson::Feature(feature) => feature
            .geometry
            .as_ref()
            .and_then(|geometry| geometry_from_geojson_value(&geometry.value)),
        GeoJson::FeatureCollection(collection) => collection
            .features
            .iter()
            .filter_map(|feature| feature.geometry.as_ref())
            .find_map(|geometry| geometry_from_geojson_value(&geometry.value)),
    }
}

fn geometry_from_geojson_value(value: &GeoJsonValue) -> Option<ContextGeometry> {
    match value {
        GeoJsonValue::Point(point) => geojson_point(point).map(point_geometry),
        GeoJsonValue::LineString(points) => geojson_line_string(points),
        GeoJsonValue::MultiLineString(lines) => lines.iter().find_map(|line| {
            geojson_line_string(line).filter(|geometry| match geometry {
                ContextGeometry::LineString { coordinates } => coordinates.len() >= 2,
                _ => false,
            })
        }),
        GeoJsonValue::Polygon(rings) => geojson_polygon(rings),
        GeoJsonValue::MultiPolygon(polygons) => {
            let coordinates = polygons
                .iter()
                .map(|rings| normalized_polygon_coordinates(rings))
                .collect::<Option<Vec<_>>>()?;
            (!coordinates.is_empty()).then_some(ContextGeometry::MultiPolygon { coordinates })
        }
        GeoJsonValue::GeometryCollection(geometries) => geometries
            .iter()
            .find_map(|geometry| geometry_from_geojson_value(&geometry.value)),
        _ => None,
    }
}

fn geojson_polygon(rings: &[Vec<Vec<f64>>]) -> Option<ContextGeometry> {
    normalized_polygon_coordinates(rings)
        .map(|coordinates| ContextGeometry::Polygon { coordinates })
}

fn normalized_polygon_coordinates(rings: &[Vec<Vec<f64>>]) -> Option<Vec<Vec<[f64; 2]>>> {
    let mut coordinates = rings
        .iter()
        .map(|ring| {
            ring.iter()
                .map(|point| {
                    geojson_point(point).map(|(latitude, longitude)| [longitude, latitude])
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    if coordinates.is_empty() {
        return None;
    }
    for (index, ring) in coordinates.iter_mut().enumerate() {
        if ring.len() < 4
            || ring.first() != ring.last()
            || ring[..ring.len() - 1]
                .iter()
                .enumerate()
                .filter(|(index, point)| ring[..*index].iter().all(|seen| seen != *point))
                .count()
                < 3
        {
            return None;
        }
        let area = signed_ring_area(ring);
        if !area.is_finite() || area.abs() <= f64::EPSILON {
            return None;
        }
        let should_reverse = (index == 0 && area < 0.0) || (index > 0 && area > 0.0);
        if should_reverse {
            ring.reverse();
        }
    }
    Some(coordinates)
}

fn signed_ring_area(ring: &[[f64; 2]]) -> f64 {
    ring.windows(2)
        .map(|pair| pair[0][0] * pair[1][1] - pair[1][0] * pair[0][1])
        .sum::<f64>()
        / 2.0
}

fn geojson_line_string(points: &[Vec<f64>]) -> Option<ContextGeometry> {
    let coordinates = points
        .iter()
        .map(|point| geojson_point(point).map(|(latitude, longitude)| [longitude, latitude]))
        .collect::<Option<Vec<_>>>()?;
    (coordinates.len() >= 2).then_some(ContextGeometry::LineString { coordinates })
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_geometry_uses_geojson_order() {
        assert_eq!(
            point_geometry((12.98, 77.75)),
            ContextGeometry::Point {
                coordinates: [77.75, 12.98]
            }
        );
    }

    #[test]
    fn line_geometry_reads_valid_geojson_fact() {
        assert_eq!(
            geometry_from_geojson(
                r#"{"type":"LineString","coordinates":[[77.745,12.94],[77.747,12.942]]}"#
            ),
            Some(ContextGeometry::LineString {
                coordinates: vec![[77.745, 12.94], [77.747, 12.942]]
            })
        );
    }

    #[test]
    fn geometry_reader_reads_polygons_and_rejects_invalid_geojson() {
        assert_eq!(geometry_from_geojson("not geojson"), None);
        assert_eq!(
            geometry_from_geojson(
                r#"{"type":"Polygon","coordinates":[[[77.745,12.94],[77.747,12.94],[77.747,12.942],[77.745,12.94]]]}"#
            ),
            Some(ContextGeometry::Polygon {
                coordinates: vec![vec![
                    [77.745, 12.94],
                    [77.747, 12.94],
                    [77.747, 12.942],
                    [77.745, 12.94],
                ]]
            })
        );
        assert_eq!(
            geometry_from_geojson(r#"{"type":"LineString","coordinates":[[77.745,12.94]]}"#),
            None
        );
        assert_eq!(
            geometry_from_geojson(
                r#"{"type":"Polygon","coordinates":[[[77.745,12.94],[77.747,12.94],[77.745,12.94],[77.745,12.94]]]}"#
            ),
            None
        );
        assert_eq!(
            geometry_from_geojson(
                r#"{"type":"Polygon","coordinates":[[[77.745,12.94],[77.747,12.94],[77.747,12.942],[77.745,12.942]]]}"#
            ),
            None
        );
    }

    #[test]
    fn geometry_reader_preserves_valid_multipolygons() {
        assert_eq!(
            geometry_from_geojson(
                r#"{"type":"MultiPolygon","coordinates":[[[[77.745,12.94],[77.747,12.94],[77.747,12.942],[77.745,12.94]]],[[[77.75,12.95],[77.752,12.95],[77.752,12.952],[77.75,12.95]]]]}"#
            ),
            Some(ContextGeometry::MultiPolygon {
                coordinates: vec![
                    vec![vec![
                        [77.745, 12.94],
                        [77.747, 12.94],
                        [77.747, 12.942],
                        [77.745, 12.94],
                    ]],
                    vec![vec![
                        [77.75, 12.95],
                        [77.752, 12.95],
                        [77.752, 12.952],
                        [77.75, 12.95],
                    ]],
                ],
            })
        );
    }
}
