use std::collections::HashSet;

use super::{
    ServingEdgeRecord, ServingEntityRecord, ServingFactIndex, SpatialGeometry, SpatialServingIndex,
};

const IN_AREA: &str = "in_area";
const ADJACENT_AREA: &str = "adjacent_area";
const MIN_SOCIETY_OVERLAP: f64 = 0.5;
const TIE_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialTopologyReport {
    pub edges: Vec<ServingEdgeRecord>,
    pub ambiguous_entity_ids: Vec<String>,
    pub missing_geometry_entity_ids: Vec<String>,
}

/// Derive generic topology before a serving bundle is promoted.
///
/// Polygon overlap owns society containment. A trusted point is only used when
/// that society has no polygon. Ambiguous geometry is reported and deliberately
/// produces no relationship.
pub fn derive_spatial_topology(
    entities: &[ServingEntityRecord],
    facts: &ServingFactIndex,
    existing_edges: &[ServingEdgeRecord],
) -> SpatialTopologyReport {
    let spatial =
        SpatialServingIndex::from_serving_bundle_with_edges(entities, facts, existing_edges);
    let areas = spatial
        .geometry()
        .features()
        .iter()
        .filter(|feature| feature.entity_type.eq_ignore_ascii_case("area"))
        .filter(|feature| !matches!(feature.geometry, SpatialGeometry::Point(_)))
        .collect::<Vec<_>>();
    let mut report = SpatialTopologyReport::default();
    let mut seen = existing_edges
        .iter()
        .map(|edge| {
            (
                edge.from_entity_id.clone(),
                edge.edge_type.to_ascii_lowercase(),
                edge.to_entity_id.clone(),
            )
        })
        .collect::<HashSet<_>>();

    for entity in entities.iter().filter(|entity| {
        entity.entity_type.eq_ignore_ascii_case("society")
            || entity.entity_type.eq_ignore_ascii_case("place")
    }) {
        let feature = spatial.geometry().feature(&entity.entity_id);
        let polygon_subject =
            feature.is_some_and(|feature| !matches!(feature.geometry, SpatialGeometry::Point(_)));
        let containing = if polygon_subject {
            greatest_overlap(&spatial, &entity.entity_id, &areas)
        } else {
            point_containment(&spatial, &entity.entity_id, &areas)
        };
        match containing {
            Containment::One(area_id, confidence) => push_edge(
                &mut report.edges,
                &mut seen,
                &entity.entity_id,
                IN_AREA,
                &area_id,
                confidence,
            ),
            Containment::Ambiguous => report.ambiguous_entity_ids.push(entity.entity_id.clone()),
            Containment::Missing => report
                .missing_geometry_entity_ids
                .push(entity.entity_id.clone()),
        }
    }

    for (index, left) in areas.iter().enumerate() {
        for right in areas.iter().skip(index + 1) {
            if !spatial
                .geometry()
                .adjacent(&left.entity_id, &right.entity_id)
            {
                continue;
            }
            let confidence = left.confidence.min(right.confidence);
            push_edge(
                &mut report.edges,
                &mut seen,
                &left.entity_id,
                ADJACENT_AREA,
                &right.entity_id,
                confidence,
            );
            push_edge(
                &mut report.edges,
                &mut seen,
                &right.entity_id,
                ADJACENT_AREA,
                &left.entity_id,
                confidence,
            );
        }
    }
    report.ambiguous_entity_ids.sort();
    report.ambiguous_entity_ids.dedup();
    report.missing_geometry_entity_ids.sort();
    report.missing_geometry_entity_ids.dedup();
    report
}

enum Containment {
    One(String, f32),
    Ambiguous,
    Missing,
}

fn greatest_overlap(
    spatial: &SpatialServingIndex,
    subject_id: &str,
    areas: &[&super::SpatialFeature],
) -> Containment {
    let mut overlaps = areas
        .iter()
        .filter_map(|area| {
            spatial
                .geometry()
                .overlap_ratio(subject_id, &area.entity_id)
                .filter(|ratio| *ratio >= MIN_SOCIETY_OVERLAP)
                .map(|ratio| (*area, ratio))
        })
        .collect::<Vec<_>>();
    overlaps.sort_by(|(left, left_ratio), (right, right_ratio)| {
        right_ratio
            .total_cmp(left_ratio)
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
    let Some((best, best_ratio)) = overlaps.first() else {
        return Containment::Missing;
    };
    if overlaps
        .get(1)
        .is_some_and(|(_, ratio)| (best_ratio - ratio).abs() <= TIE_EPSILON)
    {
        return Containment::Ambiguous;
    }
    let subject_confidence = spatial
        .geometry()
        .feature(subject_id)
        .map_or(0.0, |feature| feature.confidence);
    Containment::One(
        best.entity_id.clone(),
        best.confidence.min(subject_confidence),
    )
}

fn point_containment(
    spatial: &SpatialServingIndex,
    subject_id: &str,
    areas: &[&super::SpatialFeature],
) -> Containment {
    let Some(point) = spatial.point_for_entity(subject_id) else {
        return Containment::Missing;
    };
    if point.confidence < 0.5 || point.source_type.as_deref().is_none_or(str::is_empty) {
        return Containment::Missing;
    }
    let containing = areas
        .iter()
        .filter(|area| {
            spatial
                .geometry()
                .contains_coordinate(&area.entity_id, point.latitude, point.longitude)
        })
        .collect::<Vec<_>>();
    match containing.as_slice() {
        [] => Containment::Missing,
        [area] => Containment::One(
            area.entity_id.clone(),
            area.confidence.min(point.confidence),
        ),
        _ => Containment::Ambiguous,
    }
}

fn push_edge(
    edges: &mut Vec<ServingEdgeRecord>,
    seen: &mut HashSet<(String, String, String)>,
    from: &str,
    relation: &str,
    to: &str,
    confidence: f32,
) {
    if seen.insert((from.to_string(), relation.to_string(), to.to_string())) {
        edges.push(ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence,
            source_type: "SpatialTopology".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::knowledge::FactValue;
    use crate::serving::ServingFactRecord;

    fn entity(id: &str, kind: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: kind.to_string(),
            name: id.to_string(),
            root_source: Some("test".to_string()),
            searchable_text: id.to_string(),
        }
    }

    fn fact(id: &str, key: &str, value: FactValue, source: &str) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: id.to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: source.to_string(),
            source_url: Some("https://example.test/source".to_string()),
            model: None,
            skill_id: Some("spatial_topology_contract".to_string()),
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn society_polygon_uses_greatest_overlap_and_points_join_the_real_area() {
        let entities = vec![
            entity("area:hoodi", "area"),
            entity("area:whitefield", "area"),
            entity("society:godrej-air", "society"),
            entity("place:metro", "place"),
        ];
        let facts = ServingFactIndex::from_records(
            vec![
                fact(
                    "area:hoodi",
                    "geo.geometry_geojson",
                    FactValue::Text(
                        r#"{"type":"Polygon","coordinates":[[[0,0],[5,0],[5,5],[0,5],[0,0]]]}"#
                            .to_string(),
                    ),
                    "OpenStreetMap",
                ),
                fact(
                    "area:whitefield",
                    "geo.geometry_geojson",
                    FactValue::Text(
                        r#"{"type":"Polygon","coordinates":[[[5,0],[10,0],[10,5],[5,5],[5,0]]]}"#
                            .to_string(),
                    ),
                    "OpenStreetMap",
                ),
                fact(
                    "society:godrej-air",
                    "geo.geometry_geojson",
                    FactValue::Text(
                        r#"{"type":"Polygon","coordinates":[[[1,1],[4,1],[4,4],[1,4],[1,1]]]}"#
                            .to_string(),
                    ),
                    "OpenStreetMap",
                ),
                fact(
                    "society:godrej-air",
                    "geo.latitude",
                    FactValue::Numeric(2.0),
                    "Google",
                ),
                fact(
                    "society:godrej-air",
                    "geo.longitude",
                    FactValue::Numeric(7.0),
                    "Google",
                ),
                fact(
                    "place:metro",
                    "geo.latitude",
                    FactValue::Numeric(2.0),
                    "Google",
                ),
                fact(
                    "place:metro",
                    "geo.longitude",
                    FactValue::Numeric(7.0),
                    "Google",
                ),
            ],
            Vec::new(),
        );
        let report = derive_spatial_topology(&entities, &facts, &[]);
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "society:godrej-air"
                && edge.edge_type == IN_AREA
                && edge.to_entity_id == "area:hoodi"));
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "place:metro"
                && edge.edge_type == IN_AREA
                && edge.to_entity_id == "area:whitefield"));
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "area:hoodi"
                && edge.edge_type == ADJACENT_AREA
                && edge.to_entity_id == "area:whitefield"));

        let serving =
            SpatialServingIndex::from_serving_bundle_with_edges(&entities, &facts, &report.edges);
        assert_eq!(
            serving.society_ids_inside("area:hoodi"),
            ["society:godrej-air"]
        );
        assert!(serving.society_ids_inside("area:whitefield").is_empty());
    }
}
