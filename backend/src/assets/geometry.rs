use std::fmt;

use geojson::{GeoJson, Geometry, Value};

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

pub fn validate_geojson_geometry(
    geometry_geojson: &str,
    subject: Option<(f64, f64)>,
    expected_distance_meters: Option<f64>,
) -> Result<(), GeometryValidationError> {
    let parsed = geometry_geojson
        .parse::<GeoJson>()
        .map_err(|err| GeometryValidationError::InvalidGeoJson(err.to_string()))?;
    let geometries = geojson_geometries(&parsed);
    if geometries.is_empty() {
        return Err(GeometryValidationError::EmptyGeometry);
    }
    let mut min_distance = None;
    let mut saw_coordinate = false;
    for geometry in geometries {
        let distance = validate_geometry_value(&geometry.value, subject, &mut saw_coordinate)?;
        min_distance = min_optional_distance(min_distance, distance);
    }
    if !saw_coordinate {
        return Err(GeometryValidationError::EmptyGeometry);
    }
    if let (Some(actual), Some(expected)) = (min_distance, expected_distance_meters) {
        let tolerance = expected.abs().mul_add(0.15, 25.0).max(25.0);
        if (actual - expected).abs() > tolerance {
            return Err(GeometryValidationError::DistanceMismatch {
                expected_meters: expected,
                computed_meters: actual,
                tolerance_meters: tolerance,
            });
        }
    }
    Ok(())
}

fn geojson_geometries(geojson: &GeoJson) -> Vec<&Geometry> {
    match geojson {
        GeoJson::Geometry(geometry) => vec![geometry],
        GeoJson::Feature(feature) => feature.geometry.iter().collect(),
        GeoJson::FeatureCollection(collection) => collection
            .features
            .iter()
            .filter_map(|feature| feature.geometry.as_ref())
            .collect(),
    }
}

fn validate_geometry_value(
    value: &Value,
    subject: Option<(f64, f64)>,
    saw_coordinate: &mut bool,
) -> Result<Option<f64>, GeometryValidationError> {
    match value {
        Value::Point(point) => validate_point(point, subject, saw_coordinate),
        Value::MultiPoint(points) | Value::LineString(points) => {
            validate_path(points, subject, saw_coordinate)
        }
        Value::MultiLineString(lines) => validate_paths(lines, subject, saw_coordinate),
        Value::Polygon(_) => Err(GeometryValidationError::UnsupportedGeometry("Polygon")),
        Value::MultiPolygon(_) => Err(GeometryValidationError::UnsupportedGeometry("MultiPolygon")),
        Value::GeometryCollection(geometries) => {
            let mut min_distance = None;
            for geometry in geometries {
                min_distance = min_optional_distance(
                    min_distance,
                    validate_geometry_value(&geometry.value, subject, saw_coordinate)?,
                );
            }
            Ok(min_distance)
        }
    }
}

fn validate_paths(
    paths: &[Vec<Vec<f64>>],
    subject: Option<(f64, f64)>,
    saw_coordinate: &mut bool,
) -> Result<Option<f64>, GeometryValidationError> {
    let mut min_distance = None;
    for path in paths {
        min_distance =
            min_optional_distance(min_distance, validate_path(path, subject, saw_coordinate)?);
    }
    Ok(min_distance)
}

fn validate_path(
    points: &[Vec<f64>],
    subject: Option<(f64, f64)>,
    saw_coordinate: &mut bool,
) -> Result<Option<f64>, GeometryValidationError> {
    let parsed = points
        .iter()
        .map(|point| coordinate(point))
        .collect::<Result<Vec<_>, _>>()?;
    if !parsed.is_empty() {
        *saw_coordinate = true;
    }
    let Some(subject) = subject else {
        return Ok(None);
    };
    if parsed.len() < 2 {
        return Ok(parsed
            .iter()
            .map(|point| haversine_meters(subject, *point))
            .reduce(f64::min));
    }
    Ok(parsed
        .windows(2)
        .map(|segment| point_segment_distance_meters(subject, segment[0], segment[1]))
        .reduce(f64::min))
}

fn validate_point(
    point: &[f64],
    subject: Option<(f64, f64)>,
    saw_coordinate: &mut bool,
) -> Result<Option<f64>, GeometryValidationError> {
    let point = coordinate(point)?;
    *saw_coordinate = true;
    Ok(subject.map(|subject| haversine_meters(subject, point)))
}

fn coordinate(point: &[f64]) -> Result<(f64, f64), GeometryValidationError> {
    if point.len() < 2 {
        return Err(GeometryValidationError::InvalidCoordinate);
    }
    let longitude = point[0];
    let latitude = point[1];
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return Err(GeometryValidationError::InvalidCoordinate);
    }
    Ok((latitude, longitude))
}

fn min_optional_distance(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn point_segment_distance_meters(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let origin_latitude = point.0.to_radians();
    let p = project(point, origin_latitude);
    let a = project(start, origin_latitude);
    let b = project(end, origin_latitude);
    let ab = (b.0 - a.0, b.1 - a.1);
    let ap = (p.0 - a.0, p.1 - a.1);
    let ab_len_sq = ab.0.mul_add(ab.0, ab.1 * ab.1);
    if ab_len_sq <= f64::EPSILON {
        return haversine_meters(point, start);
    }
    let t = ((ap.0 * ab.0 + ap.1 * ab.1) / ab_len_sq).clamp(0.0, 1.0);
    let closest = (a.0 + ab.0 * t, a.1 + ab.1 * t);
    ((p.0 - closest.0).powi(2) + (p.1 - closest.1).powi(2)).sqrt()
}

fn project(point: (f64, f64), origin_latitude_radians: f64) -> (f64, f64) {
    (
        point.1.to_radians() * EARTH_RADIUS_METERS * origin_latitude_radians.cos(),
        point.0.to_radians() * EARTH_RADIUS_METERS,
    )
}

fn haversine_meters(left: (f64, f64), right: (f64, f64)) -> f64 {
    let lat1 = left.0.to_radians();
    let lat2 = right.0.to_radians();
    let dlat = (right.0 - left.0).to_radians();
    let dlon = (right.1 - left.1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_METERS * a.sqrt().asin()
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeometryValidationError {
    InvalidGeoJson(String),
    EmptyGeometry,
    InvalidCoordinate,
    UnsupportedGeometry(&'static str),
    DistanceMismatch {
        expected_meters: f64,
        computed_meters: f64,
        tolerance_meters: f64,
    },
}

impl fmt::Display for GeometryValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGeoJson(err) => write!(f, "invalid GeoJSON: {err}"),
            Self::EmptyGeometry => write!(f, "GeoJSON geometry has no coordinates"),
            Self::InvalidCoordinate => write!(f, "GeoJSON geometry has invalid coordinates"),
            Self::UnsupportedGeometry(kind) => {
                write!(f, "GeoJSON geometry type {kind} is not supported for distance validation")
            }
            Self::DistanceMismatch {
                expected_meters,
                computed_meters,
                tolerance_meters,
            } => write!(
                f,
                "GeoJSON distance mismatch: expected {expected_meters:.1} m, computed {computed_meters:.1} m, tolerance {tolerance_meters:.1} m"
            ),
        }
    }
}

impl std::error::Error for GeometryValidationError {}
