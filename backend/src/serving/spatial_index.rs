use rstar::{PointDistance, RTree, RTreeObject, AABB};

use crate::knowledge::FactValue;
use crate::search::geo::haversine_km;

use super::{ServingEntityFactRows, ServingEntityRecord, ServingFactIndex};

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

    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    pub fn point_for_entity(&self, entity_id: &str) -> Option<&SpatialPoint> {
        self.points
            .binary_search_by(|point| point.entity_id.as_str().cmp(entity_id))
            .ok()
            .and_then(|index| self.points.get(index))
    }

    pub fn points_in_bbox(
        &self,
        west: f64,
        south: f64,
        east: f64,
        north: f64,
    ) -> Vec<&SpatialPoint> {
        if !west.is_finite()
            || !south.is_finite()
            || !east.is_finite()
            || !north.is_finite()
            || west > east
            || south > north
        {
            return Vec::new();
        }
        let envelope = AABB::from_corners([west, south], [east, north]);
        self.tree
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|indexed| self.points.get(indexed.index))
            .collect()
    }

    pub fn nearest_points(
        &self,
        latitude: f64,
        longitude: f64,
        limit: usize,
    ) -> Vec<&SpatialPoint> {
        if limit == 0 || !valid_latitude(latitude) || !valid_longitude(longitude) {
            return Vec::new();
        }
        let mut nearest = self
            .points
            .iter()
            .filter_map(|point| {
                let distance_km =
                    haversine_km(latitude, longitude, point.latitude, point.longitude);
                distance_km.is_finite().then_some((point, distance_km))
            })
            .collect::<Vec<_>>();
        nearest.sort_by(|(left, left_distance), (right, right_distance)| {
            left_distance
                .partial_cmp(right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        nearest
            .into_iter()
            .take(limit)
            .map(|(point, _)| point)
            .collect()
    }
}

fn spatial_point_from_rows(
    entity: &ServingEntityRecord,
    rows: &ServingEntityFactRows,
) -> Option<SpatialPoint> {
    let latitude = coordinate_value(rows, &["geo.latitude", "project_latitude"])?;
    let longitude = coordinate_value(rows, &["geo.longitude", "project_longitude"])?;
    if !valid_latitude(latitude.value) || !valid_longitude(longitude.value) {
        return None;
    }
    Some(SpatialPoint {
        entity_id: entity.entity_id.clone(),
        entity_type: entity.entity_type.clone(),
        name: entity.name.clone(),
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

fn coordinate_value(rows: &ServingEntityFactRows, keys: &[&str]) -> Option<CoordinateValue> {
    keys.iter().find_map(|key| {
        rows.facts
            .iter()
            .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
            .filter_map(|fact| match &fact.value {
                FactValue::Numeric(value) if value.is_finite() => Some(CoordinateValue {
                    value: *value,
                    confidence: fact.confidence,
                }),
                FactValue::Score { value, .. } if value.is_finite() => Some(CoordinateValue {
                    value: *value,
                    confidence: fact.confidence,
                }),
                _ => None,
            })
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
    })
}

fn valid_latitude(value: f64) -> bool {
    value.is_finite() && (-90.0..=90.0).contains(&value)
}

fn valid_longitude(value: f64) -> bool {
    value.is_finite() && (-180.0..=180.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
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
            source_type: "test".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn spatial_index_loads_points_and_queries_bbox() {
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

        assert_eq!(index.point_count(), 3);
        assert_eq!(
            index
                .point_for_entity("society:one")
                .map(|point| point.latitude),
            Some(12.98)
        );
        let nearby = index.points_in_bbox(77.7, 12.9, 77.8, 13.0);
        assert_eq!(
            nearby
                .iter()
                .map(|point| point.entity_id.as_str())
                .collect::<Vec<_>>(),
            vec!["society:one", "place:metro"]
        );

        let nearest = index.nearest_points(12.98, 77.75, 2);
        assert_eq!(
            nearest
                .iter()
                .map(|point| point.entity_id.as_str())
                .collect::<Vec<_>>(),
            vec!["society:one", "place:metro"]
        );
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
        assert_eq!(index.point_count(), 0);
    }
}
