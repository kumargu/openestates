use std::collections::HashMap;
use std::fmt;

use arrow::array::{Array, Float64Array, StringArray};
use bytes::Bytes;
use geojson::{GeoJson, Geometry, Value};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::assets::{
    AssetId, AssetMaterializationStore, AssetPartition, KgViewManifest, KG_SOCIETY_VIEW_ASSET_ID,
};
use crate::lake::{LakeError, LakeKey, LakeStore};
use crate::parquet_data::VALUE_NUMBER_COLUMN;

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
        Value::MultiLineString(lines) | Value::Polygon(lines) => {
            validate_paths(lines, subject, saw_coordinate)
        }
        Value::MultiPolygon(polygons) => {
            let mut min_distance = None;
            for polygon in polygons {
                min_distance = min_optional_distance(
                    min_distance,
                    validate_paths(polygon, subject, saw_coordinate)?,
                );
            }
            Ok(min_distance)
        }
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

pub async fn current_kg_subject_points(
    lake: &LakeStore,
) -> Result<HashMap<String, (f64, f64)>, LakeError> {
    let materializations = AssetMaterializationStore::new(lake.clone());
    let asset_id =
        AssetId::new(KG_SOCIETY_VIEW_ASSET_ID).expect("KG society view asset id is valid");
    let record = match materializations
        .current_record(&asset_id, &AssetPartition::global())
        .await
    {
        Ok(record) => record,
        Err(err) if err.is_not_found() => return Ok(HashMap::new()),
        Err(err) => return Err(err),
    };
    let Some(manifest_artifact) = record
        .artifacts
        .iter()
        .find(|artifact| artifact.content_type == "application/json")
    else {
        return Err(LakeError::InvalidMetadata(
            "KG society view current record has no manifest artifact".to_string(),
        ));
    };
    let manifest_key = LakeKey::new(manifest_artifact.key.clone()).map_err(LakeError::Key)?;
    let manifest: KgViewManifest = lake.get_json(&manifest_key).await?;
    let fact_key = LakeKey::new(manifest.fact_parquet_key).map_err(LakeError::Key)?;
    let fact_bytes = lake.get_bytes(&fact_key).await?;
    read_subject_points_from_kg_facts(fact_bytes)
}

fn read_subject_points_from_kg_facts(
    bytes: Vec<u8>,
) -> Result<HashMap<String, (f64, f64)>, LakeError> {
    let mut by_entity = HashMap::<String, (Option<f64>, Option<f64>)>::new();
    let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))
        .map_err(|err| LakeError::InvalidMetadata(format!("invalid KG fact parquet: {err}")))?
        .build()
        .map_err(|err| LakeError::InvalidMetadata(format!("invalid KG fact parquet: {err}")))?;
    for batch in reader {
        let batch = batch
            .map_err(|err| LakeError::InvalidMetadata(format!("invalid KG fact batch: {err}")))?;
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let value_number = float64_column(&batch, VALUE_NUMBER_COLUMN)?;
        for row in 0..batch.num_rows() {
            if entity_id.is_null(row) || fact_key.is_null(row) || value_number.is_null(row) {
                continue;
            }
            let key = fact_key.value(row);
            if key != "geo.latitude" && key != "geo.longitude" {
                continue;
            }
            let value = value_number.value(row);
            let entry = by_entity
                .entry(entity_id.value(row).to_string())
                .or_insert((None, None));
            match key {
                "geo.latitude" if valid_latitude(value) => entry.0 = Some(value),
                "geo.longitude" if valid_longitude(value) => entry.1 = Some(value),
                _ => {}
            }
        }
    }
    Ok(by_entity
        .into_iter()
        .filter_map(|(entity_id, (latitude, longitude))| Some((entity_id, (latitude?, longitude?))))
        .collect())
}

fn string_column<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<&'a StringArray, LakeError> {
    let index = batch.schema().index_of(name).map_err(|err| {
        LakeError::InvalidMetadata(format!("missing KG fact column {name}: {err}"))
    })?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| LakeError::InvalidMetadata(format!("KG fact column {name} is not Utf8")))
}

fn float64_column<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<&'a Float64Array, LakeError> {
    let index = batch.schema().index_of(name).map_err(|err| {
        LakeError::InvalidMetadata(format!("missing KG fact column {name}: {err}"))
    })?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| LakeError::InvalidMetadata(format!("KG fact column {name} is not Float64")))
}

fn valid_latitude(value: f64) -> bool {
    value.is_finite() && (-90.0..=90.0).contains(&value)
}

fn valid_longitude(value: f64) -> bool {
    value.is_finite() && (-180.0..=180.0).contains(&value)
}
