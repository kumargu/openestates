use rstar::{PointDistance, RTree, RTreeObject, AABB};

use crate::dag_config::{valid_coordinate_pair, CoordinateEntityScope};
use crate::search::geo::haversine_km;

use super::{
    resolve_serving_coordinates, ServingEntityFactRows, ServingEntityRecord, ServingFactIndex,
};

#[derive(Debug, Clone, Default)]
pub struct SpatialServingIndex {
    points: Vec<SpatialPoint>,
    tree: RTree<IndexedPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialPoint {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub confidence: f32,
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
        Self { points, tree }
    }

    pub fn point_for_entity(&self, entity_id: &str) -> Option<&SpatialPoint> {
        self.points
            .binary_search_by(|point| point.entity_id.as_str().cmp(entity_id))
            .ok()
            .and_then(|index| self.points.get(index))
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
    })
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
}
