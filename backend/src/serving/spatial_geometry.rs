use std::collections::HashMap;

use geo::{
    Area, BooleanOps, BoundingRect, Centroid, Distance, Euclidean, Geometry, Intersects,
    LineString, MapCoords, MultiPolygon, Point, Polygon,
};
use geojson::{GeoJson, Value as GeoJsonValue};
use rstar::{RTree, RTreeObject, AABB};

use crate::knowledge::FactValue;

use super::{ServingEdgeRecord, ServingEntityRecord, ServingFactIndex};

const GEOMETRY_FACT_KEY: &str = "geo.geometry_geojson";
const AREA_EPSILON: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialBounds {
    pub min_longitude: f64,
    pub min_latitude: f64,
    pub max_longitude: f64,
    pub max_latitude: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpatialGeometry {
    Point(Point<f64>),
    Polygon(Polygon<f64>),
    MultiPolygon(MultiPolygon<f64>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpatialFeature {
    pub entity_id: String,
    pub entity_type: String,
    pub name: String,
    pub geometry: SpatialGeometry,
    pub bounds: SpatialBounds,
    pub confidence: f32,
    pub source_type: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SpatialGeometryIndex {
    features: Vec<SpatialFeature>,
    tree: RTree<IndexedFeature>,
    outgoing: HashMap<(String, String), Vec<String>>,
    incoming: HashMap<(String, String), Vec<String>>,
}

#[derive(Debug, Clone)]
struct IndexedFeature {
    envelope: AABB<[f64; 2]>,
    index: usize,
}

impl RTreeObject for IndexedFeature {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

impl SpatialGeometryIndex {
    pub fn from_serving_bundle(
        entities: &[ServingEntityRecord],
        facts: &ServingFactIndex,
        edges: &[ServingEdgeRecord],
    ) -> Self {
        let mut features = entities
            .iter()
            .filter_map(|entity| spatial_feature(entity, facts))
            .collect::<Vec<_>>();
        features.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        let tree = RTree::bulk_load(
            features
                .iter()
                .enumerate()
                .map(|(index, feature)| IndexedFeature {
                    envelope: bounds_envelope(feature.bounds),
                    index,
                })
                .collect(),
        );
        let mut outgoing = HashMap::<(String, String), Vec<String>>::new();
        let mut incoming = HashMap::<(String, String), Vec<String>>::new();
        for edge in edges {
            let relation = edge.edge_type.to_ascii_lowercase();
            push_unique(
                outgoing
                    .entry((edge.from_entity_id.clone(), relation.clone()))
                    .or_default(),
                &edge.to_entity_id,
            );
            push_unique(
                incoming
                    .entry((edge.to_entity_id.clone(), relation))
                    .or_default(),
                &edge.from_entity_id,
            );
        }
        for values in outgoing.values_mut().chain(incoming.values_mut()) {
            values.sort();
        }
        Self {
            features,
            tree,
            outgoing,
            incoming,
        }
    }

    pub fn feature(&self, entity_id: &str) -> Option<&SpatialFeature> {
        self.features
            .binary_search_by(|feature| feature.entity_id.as_str().cmp(entity_id))
            .ok()
            .and_then(|index| self.features.get(index))
    }

    pub fn features(&self) -> &[SpatialFeature] {
        &self.features
    }

    /// Returns an index anchor for resolving a named spatial entity. Polygon
    /// topology remains authoritative for containment; this point is only a
    /// deterministic recall/radius anchor derived from the sourced geometry.
    pub fn representative_coordinate(&self, entity_id: &str) -> Option<(f64, f64, f32)> {
        let feature = self.feature(entity_id)?;
        let point = match &feature.geometry {
            SpatialGeometry::Point(point) => *point,
            SpatialGeometry::Polygon(polygon) => polygon.centroid()?,
            SpatialGeometry::MultiPolygon(polygons) => polygons.centroid()?,
        };
        Some((point.y(), point.x(), feature.confidence))
    }

    pub fn features_intersecting(&self, entity_id: &str) -> Vec<&SpatialFeature> {
        let Some(anchor) = self.feature(entity_id) else {
            return Vec::new();
        };
        let mut matches = self
            .tree
            .locate_in_envelope_intersecting(&bounds_envelope(anchor.bounds))
            .filter_map(|indexed| self.features.get(indexed.index))
            .filter(|candidate| candidate.entity_id != entity_id)
            .filter(|candidate| geometries_intersect(&anchor.geometry, &candidate.geometry))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        matches
    }

    pub fn features_containing_coordinate(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Vec<&SpatialFeature> {
        let point = AABB::from_point([longitude, latitude]);
        let mut matches = self
            .tree
            .locate_in_envelope_intersecting(&point)
            .filter_map(|indexed| self.features.get(indexed.index))
            .filter(|candidate| {
                geometry_contains_point(
                    &candidate.geometry,
                    &SpatialGeometry::Point(Point::new(longitude, latitude)),
                )
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        matches
    }

    /// Boundary points count as contained. This is the useful buyer/search
    /// interpretation and avoids dropping homes whose trusted coordinate lies
    /// exactly on an administrative boundary.
    pub fn contains_point(&self, container_id: &str, point_id: &str) -> bool {
        let (Some(container), Some(point)) = (self.feature(container_id), self.feature(point_id))
        else {
            return false;
        };
        geometry_contains_point(&container.geometry, &point.geometry)
    }

    pub fn contains_coordinate(&self, container_id: &str, latitude: f64, longitude: f64) -> bool {
        let Some(container) = self.feature(container_id) else {
            return false;
        };
        geometry_contains_point(
            &container.geometry,
            &SpatialGeometry::Point(Point::new(longitude, latitude)),
        )
    }

    pub fn overlap_ratio(&self, subject_id: &str, container_id: &str) -> Option<f64> {
        let subject = self.feature(subject_id)?;
        let container = self.feature(container_id)?;
        polygon_overlap_ratio(&subject.geometry, &container.geometry)
    }

    pub fn adjacent(&self, left_id: &str, right_id: &str) -> bool {
        let (Some(left), Some(right)) = (self.feature(left_id), self.feature(right_id)) else {
            return false;
        };
        polygons_adjacent(&left.geometry, &right.geometry)
    }

    /// Minimum footprint distance in kilometres. Coordinates are projected to
    /// a local equirectangular plane before the exact geometry distance is
    /// measured; this avoids centroid distance for areas and society polygons.
    pub fn distance_km(&self, left_id: &str, right_id: &str) -> Option<f64> {
        let left = self.feature(left_id)?;
        let right = self.feature(right_id)?;
        projected_geometry_distance_km(&left.geometry, left.bounds, &right.geometry, right.bounds)
    }

    pub fn distance_to_coordinate_km(
        &self,
        entity_id: &str,
        latitude: f64,
        longitude: f64,
    ) -> Option<f64> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return None;
        }
        let feature = self.feature(entity_id)?;
        let point = SpatialGeometry::Point(Point::new(longitude, latitude));
        let point_bounds = SpatialBounds {
            min_longitude: longitude,
            min_latitude: latitude,
            max_longitude: longitude,
            max_latitude: latitude,
        };
        projected_geometry_distance_km(&feature.geometry, feature.bounds, &point, point_bounds)
    }

    pub fn bounds(&self, entity_id: &str) -> Option<SpatialBounds> {
        self.feature(entity_id).map(|feature| feature.bounds)
    }

    pub fn has_footprint(&self, entity_id: &str) -> bool {
        self.feature(entity_id)
            .is_some_and(|feature| !matches!(feature.geometry, SpatialGeometry::Point(_)))
    }

    pub fn related(&self, entity_id: &str, relation: &str) -> Vec<&str> {
        self.outgoing
            .get(&(entity_id.to_string(), relation.to_ascii_lowercase()))
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect()
    }

    pub fn related_incoming(&self, entity_id: &str, relation: &str) -> Vec<&str> {
        self.incoming
            .get(&(entity_id.to_string(), relation.to_ascii_lowercase()))
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect()
    }
}

fn projected_geometry_distance_km(
    left: &SpatialGeometry,
    left_bounds: SpatialBounds,
    right: &SpatialGeometry,
    right_bounds: SpatialBounds,
) -> Option<f64> {
    let reference_latitude = (left_bounds.min_latitude
        + left_bounds.max_latitude
        + right_bounds.min_latitude
        + right_bounds.max_latitude)
        / 4.0;
    let longitude_scale = 111.32 * reference_latitude.to_radians().cos().abs().max(0.01);
    let project = |coordinate: geo::Coord<f64>| geo::Coord {
        x: coordinate.x * longitude_scale,
        y: coordinate.y * 110.574,
    };
    let left = geometry_value(left).map_coords(project);
    let right = geometry_value(right).map_coords(project);
    Some(Euclidean.distance(&left, &right))
}

fn geometry_value(geometry: &SpatialGeometry) -> Geometry<f64> {
    match geometry {
        SpatialGeometry::Point(value) => Geometry::Point(*value),
        SpatialGeometry::Polygon(value) => Geometry::Polygon(value.clone()),
        SpatialGeometry::MultiPolygon(value) => Geometry::MultiPolygon(value.clone()),
    }
}

fn spatial_feature(
    entity: &ServingEntityRecord,
    facts: &ServingFactIndex,
) -> Option<SpatialFeature> {
    let fact = facts
        .entity(&entity.entity_id)?
        .facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(GEOMETRY_FACT_KEY))
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => Some((fact, value.as_str())),
            _ => None,
        })
        .max_by(|(left, _), (right, _)| {
            left.confidence
                .total_cmp(&right.confidence)
                .then_with(|| left.learned_at.cmp(&right.learned_at))
        })?;
    let geometry = parse_geojson_geometry(fact.1)?;
    let bounds = geometry_bounds(&geometry)?;
    Some(SpatialFeature {
        entity_id: entity.entity_id.clone(),
        entity_type: entity.entity_type.clone(),
        name: entity.name.clone(),
        geometry,
        bounds,
        confidence: fact.0.confidence,
        source_type: fact.0.source_type.clone(),
        source_url: fact.0.source_url.clone(),
    })
}

fn parse_geojson_geometry(value: &str) -> Option<SpatialGeometry> {
    let geojson = value.parse::<GeoJson>().ok()?;
    let value = match geojson {
        GeoJson::Geometry(geometry) => geometry.value,
        GeoJson::Feature(feature) => feature.geometry?.value,
        GeoJson::FeatureCollection(_) => return None,
    };
    match value {
        GeoJsonValue::Point(coordinate) => {
            valid_coordinate(&coordinate).map(|(x, y)| SpatialGeometry::Point(Point::new(x, y)))
        }
        GeoJsonValue::Polygon(rings) => polygon(&rings).map(SpatialGeometry::Polygon),
        GeoJsonValue::MultiPolygon(polygons) => {
            let polygons = polygons
                .iter()
                .map(|rings| polygon(rings))
                .collect::<Option<Vec<_>>>()?;
            (!polygons.is_empty())
                .then(|| SpatialGeometry::MultiPolygon(MultiPolygon::new(polygons)))
        }
        _ => None,
    }
}

fn polygon(rings: &[Vec<Vec<f64>>]) -> Option<Polygon<f64>> {
    let (exterior, holes) = rings.split_first()?;
    let exterior = ring(exterior)?;
    let holes = holes
        .iter()
        .map(|hole| ring(hole))
        .collect::<Option<Vec<_>>>()?;
    let polygon = Polygon::new(exterior, holes);
    (polygon.unsigned_area() > AREA_EPSILON).then_some(polygon)
}

fn ring(coordinates: &[Vec<f64>]) -> Option<LineString<f64>> {
    if coordinates.len() < 4 {
        return None;
    }
    let points = coordinates
        .iter()
        .map(|coordinate| valid_coordinate(coordinate).map(|(x, y)| (x, y)))
        .collect::<Option<Vec<_>>>()?;
    if points.first() != points.last() {
        return None;
    }
    let unique =
        points[..points.len() - 1]
            .iter()
            .fold(Vec::<(f64, f64)>::new(), |mut values, point| {
                if !values.contains(point) {
                    values.push(*point);
                }
                values
            });
    (unique.len() >= 3).then(|| LineString::from(points))
}

fn valid_coordinate(coordinate: &[f64]) -> Option<(f64, f64)> {
    let [longitude, latitude, ..] = coordinate else {
        return None;
    };
    (longitude.is_finite()
        && latitude.is_finite()
        && (-180.0..=180.0).contains(longitude)
        && (-90.0..=90.0).contains(latitude))
    .then_some((*longitude, *latitude))
}

fn geometry_bounds(geometry: &SpatialGeometry) -> Option<SpatialBounds> {
    let rect = match geometry {
        SpatialGeometry::Point(point) => {
            return Some(SpatialBounds {
                min_longitude: point.x(),
                min_latitude: point.y(),
                max_longitude: point.x(),
                max_latitude: point.y(),
            })
        }
        SpatialGeometry::Polygon(polygon) => polygon.bounding_rect()?,
        SpatialGeometry::MultiPolygon(polygons) => polygons.bounding_rect()?,
    };
    Some(SpatialBounds {
        min_longitude: rect.min().x,
        min_latitude: rect.min().y,
        max_longitude: rect.max().x,
        max_latitude: rect.max().y,
    })
}

fn bounds_envelope(bounds: SpatialBounds) -> AABB<[f64; 2]> {
    AABB::from_corners(
        [bounds.min_longitude, bounds.min_latitude],
        [bounds.max_longitude, bounds.max_latitude],
    )
}

fn geometry_contains_point(container: &SpatialGeometry, point: &SpatialGeometry) -> bool {
    let SpatialGeometry::Point(point) = point else {
        return false;
    };
    match container {
        SpatialGeometry::Polygon(polygon) => polygon.intersects(point),
        SpatialGeometry::MultiPolygon(polygons) => polygons.intersects(point),
        SpatialGeometry::Point(container) => container == point,
    }
}

fn geometries_intersect(left: &SpatialGeometry, right: &SpatialGeometry) -> bool {
    match (left, right) {
        (SpatialGeometry::Point(left), SpatialGeometry::Point(right)) => left == right,
        (SpatialGeometry::Point(point), SpatialGeometry::Polygon(polygon))
        | (SpatialGeometry::Polygon(polygon), SpatialGeometry::Point(point)) => {
            polygon.intersects(point)
        }
        (SpatialGeometry::Point(point), SpatialGeometry::MultiPolygon(polygons))
        | (SpatialGeometry::MultiPolygon(polygons), SpatialGeometry::Point(point)) => {
            polygons.intersects(point)
        }
        (SpatialGeometry::Polygon(left), SpatialGeometry::Polygon(right)) => left.intersects(right),
        (SpatialGeometry::Polygon(left), SpatialGeometry::MultiPolygon(right)) => {
            right.intersects(left)
        }
        (SpatialGeometry::MultiPolygon(left), SpatialGeometry::Polygon(right)) => {
            left.intersects(right)
        }
        (SpatialGeometry::MultiPolygon(left), SpatialGeometry::MultiPolygon(right)) => {
            left.intersects(right)
        }
    }
}

fn as_multi_polygon(geometry: &SpatialGeometry) -> Option<MultiPolygon<f64>> {
    match geometry {
        SpatialGeometry::Polygon(polygon) => Some(MultiPolygon::new(vec![polygon.clone()])),
        SpatialGeometry::MultiPolygon(polygons) => Some(polygons.clone()),
        SpatialGeometry::Point(_) => None,
    }
}

fn polygon_overlap_ratio(subject: &SpatialGeometry, container: &SpatialGeometry) -> Option<f64> {
    let subject = as_multi_polygon(subject)?;
    let container = as_multi_polygon(container)?;
    let subject_area = subject.unsigned_area();
    if subject_area <= AREA_EPSILON {
        return None;
    }
    Some((subject.intersection(&container).unsigned_area() / subject_area).clamp(0.0, 1.0))
}

fn polygons_adjacent(left: &SpatialGeometry, right: &SpatialGeometry) -> bool {
    let (Some(left), Some(right)) = (as_multi_polygon(left), as_multi_polygon(right)) else {
        return false;
    };
    left.intersects(&right) && left.intersection(&right).unsigned_area() <= AREA_EPSILON
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::serving::ServingFactRecord;

    fn entity(id: &str, entity_type: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: entity_type.to_string(),
            name: id.to_string(),
            root_source: Some("OpenStreetMap".to_string()),
            searchable_text: id.to_string(),
        }
    }

    fn geometry(id: &str, value: &str) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: id.to_string(),
            fact_key: GEOMETRY_FACT_KEY.to_string(),
            value_type: "text".to_string(),
            value_text: Some(value.to_string()),
            value: FactValue::Text(value.to_string()),
            confidence: 0.9,
            source_type: "OpenStreetMap".to_string(),
            source_url: Some("https://www.openstreetmap.org/".to_string()),
            model: None,
            skill_id: Some("osm_locality_boundaries".to_string()),
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    fn index(rows: &[(&str, &str)]) -> SpatialGeometryIndex {
        let entities = rows
            .iter()
            .map(|(id, value)| {
                entity(
                    id,
                    if value.contains("Point") {
                        "place"
                    } else {
                        "area"
                    },
                )
            })
            .collect::<Vec<_>>();
        let facts = ServingFactIndex::from_records(
            rows.iter().map(|(id, value)| geometry(id, value)).collect(),
            Vec::new(),
        );
        SpatialGeometryIndex::from_serving_bundle(&entities, &facts, &[])
    }

    #[test]
    fn holes_exclude_points_but_boundary_points_are_retained() {
        let index = index(&[
            (
                "area:ring",
                r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]],[[4,4],[6,4],[6,6],[4,6],[4,4]]]}"#,
            ),
            ("place:inside", r#"{"type":"Point","coordinates":[2,2]}"#),
            ("place:hole", r#"{"type":"Point","coordinates":[5,5]}"#),
            ("place:boundary", r#"{"type":"Point","coordinates":[0,5]}"#),
        ]);
        assert!(index.contains_point("area:ring", "place:inside"));
        assert!(!index.contains_point("area:ring", "place:hole"));
        assert!(index.contains_point("area:ring", "place:boundary"));
    }

    #[test]
    fn multipolygon_contains_points_in_either_part() {
        let index = index(&[
            (
                "area:multi",
                r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[2,0],[2,2],[0,2],[0,0]]],[[[8,8],[10,8],[10,10],[8,10],[8,8]]]]}"#,
            ),
            ("place:second", r#"{"type":"Point","coordinates":[9,9]}"#),
        ]);
        assert!(index.contains_point("area:multi", "place:second"));
    }

    #[test]
    fn distance_uses_polygon_footprint_instead_of_centroid() {
        let index = index(&[
            (
                "area:hoodi",
                r#"{"type":"Polygon","coordinates":[[[77.70,12.97],[77.73,12.97],[77.73,13.00],[77.70,13.00],[77.70,12.97]]]}"#,
            ),
            (
                "place:inside",
                r#"{"type":"Point","coordinates":[77.701,12.971]}"#,
            ),
            (
                "place:east",
                r#"{"type":"Point","coordinates":[77.74,12.985]}"#,
            ),
        ]);

        assert_eq!(index.distance_km("area:hoodi", "place:inside"), Some(0.0));
        let east_distance = index
            .distance_km("area:hoodi", "place:east")
            .expect("both sourced geometries have a distance");
        assert!((1.0..1.2).contains(&east_distance), "{east_distance}");
    }

    #[test]
    fn overlap_and_adjacency_use_exact_geometry_after_envelope_recall() {
        let index = index(&[
            (
                "area:left",
                r#"{"type":"Polygon","coordinates":[[[0,0],[2,0],[2,2],[0,2],[0,0]]]}"#,
            ),
            (
                "area:half",
                r#"{"type":"Polygon","coordinates":[[[1,0],[3,0],[3,2],[1,2],[1,0]]]}"#,
            ),
            (
                "area:touch",
                r#"{"type":"Polygon","coordinates":[[[2,0],[4,0],[4,2],[2,2],[2,0]]]}"#,
            ),
        ]);
        assert_eq!(index.overlap_ratio("area:left", "area:half"), Some(0.5));
        assert!(!index.adjacent("area:left", "area:half"));
        assert!(index.adjacent("area:left", "area:touch"));
        assert_eq!(index.features_intersecting("area:left").len(), 2);
    }

    #[test]
    fn invalid_or_open_geometry_is_not_indexed() {
        let index = index(&[
            (
                "area:open",
                r#"{"type":"Polygon","coordinates":[[[0,0],[2,0],[2,2],[0,2]]]}"#,
            ),
            (
                "area:invalid",
                r#"{"type":"Polygon","coordinates":[[[0,0],[181,0],[2,2],[0,0]]]}"#,
            ),
        ]);
        assert!(index.feature("area:open").is_none());
        assert!(index.feature("area:invalid").is_none());
    }
}
