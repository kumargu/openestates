use std::collections::HashMap;

use rstar::{PointDistance, RTree, RTreeObject, AABB};

use crate::dag_config::{valid_coordinate_pair, CoordinateEntityScope};
use crate::search::geo::haversine_km;

use super::{
    resolve_serving_coordinates, DerivedEvidence, EvidenceRef, ServingEdgeRecord,
    ServingEntityFactRows, ServingEntityRecord, ServingFactIndex, SourceObservation,
    SpatialGeometry, SpatialGeometryIndex,
};

#[derive(Debug, Clone, Default)]
pub struct SpatialServingIndex {
    points: Vec<SpatialPoint>,
    tree: RTree<IndexedPoint>,
    geometry: SpatialGeometryIndex,
    entity_types: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialPoint {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub confidence: f32,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub observations: Vec<SourceObservation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialDistance {
    pub distance_km: f64,
    pub metric: &'static str,
    pub confidence: f32,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub evidence_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Clone)]
struct IndexedPoint {
    point: [f64; 2],
    index: usize,
}

impl RTreeObject for IndexedPoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for IndexedPoint {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let longitude_delta = self.point[0] - point[0];
        let latitude_delta = self.point[1] - point[1];
        longitude_delta.mul_add(longitude_delta, latitude_delta * latitude_delta)
    }
}

impl SpatialServingIndex {
    pub fn from_serving_bundle(
        entities: &[ServingEntityRecord],
        fact_index: &ServingFactIndex,
    ) -> Self {
        Self::from_serving_bundle_with_edges(entities, fact_index, &[])
    }

    pub fn from_serving_bundle_with_edges(
        entities: &[ServingEntityRecord],
        fact_index: &ServingFactIndex,
        edges: &[ServingEdgeRecord],
    ) -> Self {
        let mut points = entities
            .iter()
            .filter_map(|entity| {
                let rows = fact_index.entity(&entity.entity_id)?;
                spatial_point_from_rows(entity, rows)
            })
            .collect::<Vec<_>>();
        points.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        let tree = RTree::bulk_load(
            points
                .iter()
                .enumerate()
                .map(|(index, point)| IndexedPoint {
                    point: [point.longitude, point.latitude],
                    index,
                })
                .collect(),
        );
        let geometry = SpatialGeometryIndex::from_serving_bundle(entities, fact_index, edges);
        let entity_types = entities
            .iter()
            .map(|entity| (entity.entity_id.clone(), entity.entity_type.clone()))
            .collect();
        Self {
            points,
            tree,
            geometry,
            entity_types,
        }
    }

    pub fn geometry(&self) -> &SpatialGeometryIndex {
        &self.geometry
    }

    pub fn society_ids_inside(&self, container_id: &str) -> Vec<String> {
        let mut ids = self
            .geometry
            .related_incoming(container_id, "in_area")
            .into_iter()
            .filter(|entity_id| {
                self.entity_types
                    .get(*entity_id)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("society"))
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn adjacent_area_ids(&self, area_id: &str) -> Vec<String> {
        let mut ids = self
            .geometry
            .related(area_id, "adjacent_area")
            .into_iter()
            .chain(self.geometry.related_incoming(area_id, "adjacent_area"))
            .map(str::to_string)
            .collect::<Vec<_>>();
        ids.extend(
            self.geometry
                .features_intersecting(area_id)
                .into_iter()
                .filter(|feature| feature.entity_type.eq_ignore_ascii_case("area"))
                .filter(|feature| self.geometry.adjacent(area_id, &feature.entity_id))
                .map(|feature| feature.entity_id.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn relation_derivation(
        &self,
        from_entity_id: &str,
        relation: &str,
        to_entity_id: &str,
        snapshot_identity: &str,
    ) -> Option<&DerivedEvidence> {
        let derivation =
            self.geometry
                .relation_derivation(from_entity_id, relation, to_entity_id)?;
        (derivation.snapshot_identity == snapshot_identity && derivation.validate().is_ok())
            .then_some(derivation)
    }

    /// Returns whether two resolved spatial scopes are connected by sourced
    /// containment/topology. Coordinate proximity alone is deliberately not
    /// enough to collapse buyer intent branches.
    pub fn scopes_connected(&self, left_id: &str, right_id: &str) -> bool {
        if left_id == right_id {
            return true;
        }
        let both_areas = self.entity_is_area(left_id) && self.entity_is_area(right_id);
        if both_areas {
            return self
                .geometry
                .related(left_id, "in_area")
                .contains(&right_id)
                || self
                    .geometry
                    .related(right_id, "in_area")
                    .contains(&left_id)
                || self
                    .adjacent_area_ids(left_id)
                    .iter()
                    .any(|id| id == right_id);
        }
        let left_areas = self.area_scope_ids(left_id);
        let right_areas = self.area_scope_ids(right_id);
        left_areas.iter().any(|left_area| {
            right_areas.contains(left_area)
                || self
                    .adjacent_area_ids(left_area)
                    .iter()
                    .any(|adjacent| right_areas.contains(adjacent))
        })
    }

    fn entity_is_area(&self, entity_id: &str) -> bool {
        self.entity_types
            .get(entity_id)
            .is_some_and(|kind| kind.eq_ignore_ascii_case("area"))
    }

    /// Returns the sourced area scope for an entity, including nested area
    /// ancestors. No coordinate or name inference is used here.
    pub fn area_scope_ids(&self, entity_id: &str) -> Vec<String> {
        let mut ids = Vec::new();
        if self.entity_is_area(entity_id) {
            ids.push(entity_id.to_string());
        }
        let mut frontier = vec![entity_id.to_string()];
        while let Some(subject_id) = frontier.pop() {
            for provider_id in self
                .geometry
                .related(&subject_id, super::PROVIDER_BINDING_EDGE)
            {
                if !frontier.iter().any(|existing| existing == provider_id) {
                    frontier.push(provider_id.to_string());
                }
            }
            for area_id in self.geometry.related(&subject_id, "in_area") {
                if !self
                    .entity_types
                    .get(area_id)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("area"))
                    || ids.iter().any(|existing| existing == area_id)
                {
                    continue;
                }
                ids.push(area_id.to_string());
                frontier.push(area_id.to_string());
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn point_for_entity(&self, entity_id: &str) -> Option<&SpatialPoint> {
        self.points
            .binary_search_by(|point| point.entity_id.as_str().cmp(entity_id))
            .ok()
            .and_then(|index| self.points.get(index))
    }

    /// Prefer sourced footprint geometry. A coordinate fallback is explicit in
    /// the returned metric and must never be used to prove containment or
    /// adjacency.
    pub fn distance_between(
        &self,
        left_id: &str,
        right_id: &str,
        snapshot_identity: &str,
    ) -> Option<SpatialDistance> {
        if let Some(derivation) =
            self.relation_derivation(left_id, "near_place", right_id, snapshot_identity)
        {
            let distance_km = derivation.value?;
            if derivation.unit.as_deref() == Some("km") {
                return Some(SpatialDistance {
                    distance_km,
                    metric: match derivation.metric.as_str() {
                        "footprint_to_footprint_distance" => "footprint_to_footprint_distance",
                        "footprint_to_destination_distance" => "footprint_to_destination_distance",
                        "footprint_to_trusted_point_distance" => {
                            "footprint_to_destination_distance"
                        }
                        _ => "trusted_point_distance_fallback",
                    },
                    confidence: derivation.confidence,
                    source_type: Some("Computed".to_string()),
                    source_url: None,
                    evidence_refs: vec![EvidenceRef::for_derivation(derivation)],
                });
            }
        }
        if let Some(distance_km) = self.geometry.distance_km(left_id, right_id) {
            let left = self.geometry.feature(left_id)?;
            let right = self.geometry.feature(right_id)?;
            let evidence_refs =
                self.footprint_evidence_refs(&[left_id, right_id], snapshot_identity)?;
            return Some(SpatialDistance {
                distance_km,
                metric: "footprint_distance",
                confidence: left.confidence.min(right.confidence),
                source_type: Some(format!("{}+{}", left.source_type, right.source_type)),
                source_url: left.source_url.clone().or_else(|| right.source_url.clone()),
                evidence_refs,
            });
        }
        let left = self.point_for_entity(left_id)?;
        let right = self.point_for_entity(right_id)?;
        let evidence_refs = point_evidence_refs([left, right], snapshot_identity)?;
        Some(SpatialDistance {
            distance_km: haversine_km(
                left.latitude,
                left.longitude,
                right.latitude,
                right.longitude,
            ),
            metric: "point_fallback_distance",
            confidence: left.confidence.min(right.confidence),
            source_type: left
                .source_type
                .clone()
                .or_else(|| right.source_type.clone()),
            source_url: left.source_url.clone().or_else(|| right.source_url.clone()),
            evidence_refs,
        })
    }

    /// Measure an entity against an area footprint while preserving the
    /// qualified geometry/coordinate observations used by the measurement.
    /// This is the search-time distance primitive for bounded geo-cell
    /// traversal; it never treats a centroid as a sourced point.
    pub fn distance_from_entity_to_area(
        &self,
        entity_id: &str,
        area_id: &str,
        snapshot_identity: &str,
    ) -> Option<SpatialDistance> {
        let area = self.geometry.feature(area_id)?;
        if matches!(area.geometry, SpatialGeometry::Point(_)) {
            return self.distance_between(entity_id, area_id, snapshot_identity);
        }
        if self
            .geometry
            .feature(entity_id)
            .is_some_and(|feature| !matches!(feature.geometry, SpatialGeometry::Point(_)))
        {
            return self.distance_between(entity_id, area_id, snapshot_identity);
        }
        let entity = self.point_for_entity(entity_id)?;
        let distance_km =
            self.geometry
                .distance_to_coordinate_km(area_id, entity.latitude, entity.longitude)?;
        let area_observation = area.observation.as_ref()?;
        let mut evidence_refs = point_evidence_refs([entity], snapshot_identity)?;
        evidence_refs.push(EvidenceRef::for_observation(
            snapshot_identity,
            area_observation,
        ));
        Some(SpatialDistance {
            distance_km,
            metric: "trusted_point_to_footprint_distance",
            confidence: entity.confidence.min(area.confidence),
            source_type: entity
                .source_type
                .clone()
                .or_else(|| Some(area.source_type.clone())),
            source_url: entity
                .source_url
                .clone()
                .or_else(|| area.source_url.clone()),
            evidence_refs,
        })
    }

    pub fn footprint_evidence_refs(
        &self,
        entity_ids: &[&str],
        snapshot_identity: &str,
    ) -> Option<Vec<EvidenceRef>> {
        let references = entity_ids
            .iter()
            .map(|entity_id| {
                let feature = self.geometry.feature(entity_id)?;
                if matches!(feature.geometry, SpatialGeometry::Point(_)) {
                    return None;
                }
                let observation = feature.observation.as_ref()?;
                Some(EvidenceRef::for_observation(snapshot_identity, observation))
            })
            .collect::<Option<Vec<_>>>()?;
        (!references.is_empty()).then_some(references)
    }

    pub fn points(&self) -> &[SpatialPoint] {
        &self.points
    }

    pub fn points_within_radius(
        &self,
        latitude: f64,
        longitude: f64,
        radius_km: f64,
    ) -> Vec<(&SpatialPoint, f64)> {
        if !valid_coordinate_pair(latitude, longitude) || !radius_km.is_finite() || radius_km <= 0.0
        {
            return Vec::new();
        }

        const KM_PER_LATITUDE_DEGREE: f64 = 111.32;
        let latitude_delta = radius_km / KM_PER_LATITUDE_DEGREE;
        let longitude_scale = latitude.to_radians().cos().abs().max(0.01);
        let longitude_delta = radius_km / (KM_PER_LATITUDE_DEGREE * longitude_scale);
        let envelope = AABB::from_corners(
            [longitude - longitude_delta, latitude - latitude_delta],
            [longitude + longitude_delta, latitude + latitude_delta],
        );
        let mut matches = self
            .tree
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|indexed| self.points.get(indexed.index))
            .filter_map(|point| {
                let distance_km =
                    haversine_km(latitude, longitude, point.latitude, point.longitude);
                (distance_km <= radius_km).then_some((point, distance_km))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|(left, left_distance), (right, right_distance)| {
            left_distance
                .partial_cmp(right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        matches
    }

    pub fn nearest_societies(
        &self,
        latitude: f64,
        longitude: f64,
        limit: usize,
    ) -> Vec<(&SpatialPoint, f64)> {
        self.nearest_societies_matching(latitude, longitude, limit, |_| true)
    }

    pub fn nearest_societies_matching(
        &self,
        latitude: f64,
        longitude: f64,
        limit: usize,
        mut is_eligible: impl FnMut(&SpatialPoint) -> bool,
    ) -> Vec<(&SpatialPoint, f64)> {
        if !valid_coordinate_pair(latitude, longitude) || limit == 0 {
            return Vec::new();
        }
        let target = [longitude, latitude];
        let mut matches = self
            .tree
            .nearest_neighbor_iter(&target)
            .filter_map(|indexed| self.points.get(indexed.index))
            .filter(|point| point.entity_type.eq_ignore_ascii_case("society"))
            .filter(|point| is_eligible(point))
            .take(limit)
            .map(|point| {
                (
                    point,
                    haversine_km(latitude, longitude, point.latitude, point.longitude),
                )
            })
            .collect::<Vec<_>>();
        matches.sort_by(|(left, left_distance), (right, right_distance)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        matches
    }
}

fn spatial_point_from_rows(
    entity: &ServingEntityRecord,
    rows: &ServingEntityFactRows,
) -> Option<SpatialPoint> {
    let scope = match entity.entity_type.to_ascii_lowercase().as_str() {
        "place" => CoordinateEntityScope::Place,
        "area" => CoordinateEntityScope::Area,
        _ => CoordinateEntityScope::Society,
    };
    let coordinates = resolve_serving_coordinates(rows, scope)?;
    Some(SpatialPoint {
        entity_id: entity.entity_id.clone(),
        entity_type: entity.entity_type.clone(),
        name: entity.name.clone(),
        latitude: coordinates.latitude,
        longitude: coordinates.longitude,
        confidence: coordinates.confidence,
        source_type: Some(coordinates.source_type.clone()),
        source_url: coordinates
            .observations
            .iter()
            .find_map(|observation| observation.source_url.clone()),
        observations: coordinates.observations,
    })
}

fn point_evidence_refs<const N: usize>(
    points: [&SpatialPoint; N],
    snapshot_identity: &str,
) -> Option<Vec<EvidenceRef>> {
    if points.iter().any(|point| point.observations.is_empty()) {
        return None;
    }
    let references = points
        .into_iter()
        .flat_map(|point| &point.observations)
        .map(|observation| EvidenceRef::for_observation(snapshot_identity, observation))
        .collect::<Vec<_>>();
    (!references.is_empty()).then_some(references)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::knowledge::FactValue;
    use crate::serving::ServingFactRecord;

    fn entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            visibility: Default::default(),
            searchable_text: name.to_string(),
        }
    }

    fn coord(entity_id: &str, fact_key: &str, value: f64, confidence: f32) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "number".to_string(),
            value_text: None,
            value: FactValue::Numeric(value),
            confidence,
            source_type: if entity_id.starts_with("place:") {
                "OpenStreetMap"
            } else {
                "Google"
            }
            .to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            observation: None,
        }
    }

    fn edge(from: &str, relation: &str, to: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence: 0.9,
            source_type: "OpenStreetMap".to_string(),
            derivation: None,
        }
    }

    #[test]
    fn spatial_index_loads_points_for_entity_lookup() {
        let entities = vec![
            entity("society:one", "society", "One"),
            entity("place:metro", "place", "Metro"),
            entity("place:far", "place", "Far"),
        ];
        let facts = vec![
            coord("society:one", "geo.latitude", 12.98, 0.8),
            coord("society:one", "geo.longitude", 77.75, 0.9),
            coord("place:metro", "geo.latitude", 12.981, 0.8),
            coord("place:metro", "geo.longitude", 77.751, 0.8),
            coord("place:far", "geo.latitude", 13.5, 0.8),
            coord("place:far", "geo.longitude", 78.1, 0.8),
        ];
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);

        assert_eq!(
            index
                .point_for_entity("society:one")
                .map(|point| point.latitude),
            Some(12.98)
        );
        assert!(index.point_for_entity("place:metro").is_some());
        assert!(index.point_for_entity("place:far").is_some());
    }

    #[test]
    fn spatial_index_queries_haversine_radius_and_loads_area_anchors() {
        let entities = vec![
            entity("area:anchor", "area", "Anchor"),
            entity("society:near", "society", "Near"),
            entity("society:far", "society", "Far"),
        ];
        let facts = vec![
            coord("area:anchor", "geo.latitude", 12.98, 0.9),
            coord("area:anchor", "geo.longitude", 77.75, 0.9),
            coord("society:near", "geo.latitude", 12.99, 0.9),
            coord("society:near", "geo.longitude", 77.75, 0.9),
            coord("society:far", "geo.latitude", 13.08, 0.9),
            coord("society:far", "geo.longitude", 77.75, 0.9),
        ];
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        let anchor = index
            .point_for_entity("area:anchor")
            .expect("area coordinate should be indexed");

        let matches = index.points_within_radius(anchor.latitude, anchor.longitude, 2.0);
        let ids = matches
            .iter()
            .map(|(point, _)| point.entity_id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"area:anchor"));
        assert!(ids.contains(&"society:near"));
        assert!(!ids.contains(&"society:far"));
    }

    #[test]
    fn spatial_index_rejects_invalid_coordinates() {
        let entities = vec![entity("society:bad", "society", "Bad")];
        let facts = vec![
            coord("society:bad", "geo.latitude", 190.0, 0.9),
            coord("society:bad", "geo.longitude", 77.75, 0.9),
        ];
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        assert!(index.point_for_entity("society:bad").is_none());
    }

    #[test]
    fn spatial_index_returns_only_nearest_societies() {
        let entities = vec![
            entity("place:anchor", "place", "Anchor"),
            entity("society:near", "society", "Near"),
            entity("society:next", "society", "Next"),
            entity("society:far", "society", "Far"),
        ];
        let facts = vec![
            coord("place:anchor", "geo.latitude", 12.98, 0.9),
            coord("place:anchor", "geo.longitude", 77.75, 0.9),
            coord("society:near", "geo.latitude", 12.981, 0.9),
            coord("society:near", "geo.longitude", 77.75, 0.9),
            coord("society:next", "geo.latitude", 12.99, 0.9),
            coord("society:next", "geo.longitude", 77.75, 0.9),
            coord("society:far", "geo.latitude", 13.08, 0.9),
            coord("society:far", "geo.longitude", 77.75, 0.9),
        ];
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);

        let matches = index.nearest_societies(12.98, 77.75, 2);
        assert_eq!(
            matches
                .iter()
                .map(|(point, _)| point.entity_id.as_str())
                .collect::<Vec<_>>(),
            ["society:near", "society:next"]
        );

        let eligible = index
            .nearest_societies_matching(12.98, 77.75, 1, |point| point.entity_id == "society:far");
        assert_eq!(eligible[0].0.entity_id, "society:far");
    }

    #[test]
    fn scope_connectivity_requires_sourced_containment_or_adjacency() {
        let entities = vec![
            entity("area:east", "area", "East"),
            entity("area:bengaluru", "area", "Bengaluru"),
            entity("area:next", "area", "Next"),
            entity("area:north", "area", "North"),
            entity("place:metro", "place", "Metro"),
            entity("society:home", "society", "Home"),
        ];
        let edges = vec![
            edge("place:metro", "in_area", "area:east"),
            edge("area:east", "in_area", "area:bengaluru"),
            edge("society:home", "in_area", "area:next"),
            edge("area:east", "adjacent_area", "area:next"),
        ];
        let index = SpatialServingIndex::from_serving_bundle_with_edges(
            &entities,
            &ServingFactIndex::default(),
            &edges,
        );

        assert!(index.scopes_connected("place:metro", "area:east"));
        assert_eq!(
            index.area_scope_ids("place:metro"),
            ["area:bengaluru", "area:east"]
        );
        assert!(index.scopes_connected("place:metro", "society:home"));
        assert!(!index.scopes_connected("place:metro", "area:north"));
        assert!(!index.scopes_connected("area:east", "area:north"));
    }
}
