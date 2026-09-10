use std::collections::HashSet;

use crate::dag_config::SpatialTopologyPolicy;
use crate::knowledge::FactValue;

use super::{
    DerivedEvidence, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex, SpatialGeometry,
    SpatialServingIndex,
};

const IN_AREA: &str = "in_area";
const ADJACENT_AREA: &str = "adjacent_area";
const ADMIN_LEVEL_FACT_KEY: &str = "area.admin_level";

pub(crate) fn remove_derived_spatial_topology_edges(edges: &mut Vec<ServingEdgeRecord>) {
    edges.retain(|edge| {
        !edge.source_type.eq_ignore_ascii_case("SpatialTopology")
            || (!edge.edge_type.eq_ignore_ascii_case(IN_AREA)
                && !edge.edge_type.eq_ignore_ascii_case(ADJACENT_AREA))
    });
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialTopologyReport {
    pub edges: Vec<ServingEdgeRecord>,
    pub ambiguous_entity_ids: Vec<String>,
    pub missing_geometry_entity_ids: Vec<String>,
    pub missing_admin_level_entity_ids: Vec<String>,
}

/// Derive generic topology before a serving bundle is promoted.
///
/// Polygon overlap owns containment. Trusted points are reserved for labelled
/// distance fallback and never produce containment. Ambiguous geometry is
/// reported and deliberately produces no relationship.
pub fn derive_spatial_topology(
    entities: &[ServingEntityRecord],
    facts: &ServingFactIndex,
    existing_edges: &[ServingEdgeRecord],
    policy: &SpatialTopologyPolicy,
    snapshot_identity: &str,
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
    let area_levels = areas
        .iter()
        .filter_map(|area| {
            admin_level(facts, &area.entity_id).map(|level| (&area.entity_id, level))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut report = SpatialTopologyReport::default();
    let mut seen = existing_edges
        .iter()
        .filter(|edge| {
            edge.derivation.is_some() && edge.validate_derivation(snapshot_identity).is_ok()
        })
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
            || (entity.entity_type.eq_ignore_ascii_case("place")
                && !super::is_canonical_spatial_entity(entity))
    }) {
        let feature = spatial.geometry().feature(&entity.entity_id);
        let containing = if feature
            .is_some_and(|feature| !matches!(feature.geometry, SpatialGeometry::Point(_)))
        {
            polygon_containment(&spatial, facts, &entity.entity_id, policy)
        } else {
            Containment::Missing
        };
        match containing {
            Containment::Matches(matches) => {
                for (area_id, confidence) in matches {
                    let derivation = geometry_derivation(
                        &spatial,
                        snapshot_identity,
                        &entity.entity_id,
                        &area_id,
                        IN_AREA,
                        "footprint_containment",
                        confidence,
                    );
                    push_edge(
                        &mut report.edges,
                        &mut seen,
                        &entity.entity_id,
                        IN_AREA,
                        &area_id,
                        confidence,
                        derivation,
                    );
                }
            }
            Containment::Ambiguous => report.ambiguous_entity_ids.push(entity.entity_id.clone()),
            Containment::Missing => report
                .missing_geometry_entity_ids
                .push(entity.entity_id.clone()),
        }
    }

    for child in &areas {
        let Some(child_level) = area_levels.get(&child.entity_id) else {
            report
                .missing_admin_level_entity_ids
                .push(child.entity_id.clone());
            continue;
        };
        for parent in spatial.geometry().features_intersecting(&child.entity_id) {
            if !parent.entity_type.eq_ignore_ascii_case("area") {
                continue;
            }
            let Some(parent_level) = area_levels.get(&parent.entity_id) else {
                continue;
            };
            if child_level <= parent_level
                || spatial
                    .geometry()
                    .overlap_ratio(&child.entity_id, &parent.entity_id)
                    .is_none_or(|ratio| ratio < policy.minimum_area_containment_ratio)
            {
                continue;
            }
            push_edge(
                &mut report.edges,
                &mut seen,
                &child.entity_id,
                IN_AREA,
                &parent.entity_id,
                child.confidence.min(parent.confidence),
                geometry_derivation(
                    &spatial,
                    snapshot_identity,
                    &child.entity_id,
                    &parent.entity_id,
                    IN_AREA,
                    "footprint_containment",
                    child.confidence.min(parent.confidence),
                ),
            );
        }
    }

    for left in &areas {
        let Some(left_level) = area_levels.get(&left.entity_id) else {
            continue;
        };
        for right in spatial.geometry().features_intersecting(&left.entity_id) {
            if !right.entity_type.eq_ignore_ascii_case("area") || left.entity_id >= right.entity_id
            {
                continue;
            }
            let Some(right_level) = area_levels.get(&right.entity_id) else {
                continue;
            };
            if policy.require_matching_admin_level_for_adjacency && left_level != right_level {
                continue;
            }
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
                geometry_derivation(
                    &spatial,
                    snapshot_identity,
                    &left.entity_id,
                    &right.entity_id,
                    ADJACENT_AREA,
                    "same_level_adjacency",
                    confidence,
                ),
            );
            push_edge(
                &mut report.edges,
                &mut seen,
                &right.entity_id,
                ADJACENT_AREA,
                &left.entity_id,
                confidence,
                geometry_derivation(
                    &spatial,
                    snapshot_identity,
                    &right.entity_id,
                    &left.entity_id,
                    ADJACENT_AREA,
                    "same_level_adjacency",
                    confidence,
                ),
            );
        }
    }
    report.ambiguous_entity_ids.sort();
    report.ambiguous_entity_ids.dedup();
    report.missing_geometry_entity_ids.sort();
    report.missing_geometry_entity_ids.dedup();
    report.missing_admin_level_entity_ids.sort();
    report.missing_admin_level_entity_ids.dedup();
    report
}

enum Containment {
    Matches(Vec<(String, f32)>),
    Ambiguous,
    Missing,
}

fn polygon_containment(
    spatial: &SpatialServingIndex,
    facts: &ServingFactIndex,
    subject_id: &str,
    policy: &SpatialTopologyPolicy,
) -> Containment {
    let mut overlaps = spatial
        .geometry()
        .features_intersecting(subject_id)
        .into_iter()
        .filter(|area| area.entity_type.eq_ignore_ascii_case("area"))
        .filter_map(|area| {
            spatial
                .geometry()
                .overlap_ratio(subject_id, &area.entity_id)
                .filter(|ratio| *ratio >= policy.minimum_society_overlap_ratio)
                .map(|ratio| (area, ratio, admin_level(facts, &area.entity_id)))
        })
        .collect::<Vec<_>>();
    overlaps.sort_by(
        |(left, left_ratio, left_level), (right, right_ratio, right_level)| {
            left_level
                .cmp(right_level)
                .then_with(|| right_ratio.total_cmp(left_ratio))
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        },
    );
    if overlaps.is_empty() {
        return Containment::Missing;
    }
    let subject_confidence = spatial
        .geometry()
        .feature(subject_id)
        .map_or(0.0, |feature| feature.confidence);
    let mut matches = Vec::new();
    for same_level in grouped_by_level(&overlaps) {
        let (best, best_ratio, _) = same_level[0];
        if same_level
            .get(1)
            .is_some_and(|(_, ratio, _)| (best_ratio - ratio).abs() <= policy.ambiguity_epsilon)
        {
            return Containment::Ambiguous;
        }
        matches.push((
            best.entity_id.clone(),
            best.confidence.min(subject_confidence),
        ));
    }
    Containment::Matches(matches)
}

fn grouped_by_level<'a>(
    values: &'a [(&'a super::SpatialFeature, f64, Option<u8>)],
) -> impl Iterator<Item = &'a [(&'a super::SpatialFeature, f64, Option<u8>)]> {
    values.chunk_by(|left, right| left.2 == right.2)
}

fn admin_level(facts: &ServingFactIndex, entity_id: &str) -> Option<u8> {
    facts
        .entity(entity_id)?
        .facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(ADMIN_LEVEL_FACT_KEY))
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) => value.trim().parse::<u8>().ok(),
            FactValue::Numeric(value) if value.fract() == 0.0 => Some(*value as u8),
            _ => None,
        })
        .next()
}

fn push_edge(
    edges: &mut Vec<ServingEdgeRecord>,
    seen: &mut HashSet<(String, String, String)>,
    from: &str,
    relation: &str,
    to: &str,
    confidence: f32,
    derivation: Option<DerivedEvidence>,
) {
    if seen.insert((from.to_string(), relation.to_string(), to.to_string())) {
        edges.push(ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence,
            source_type: "SpatialTopology".to_string(),
            derivation,
        });
    }
}

fn geometry_derivation(
    spatial: &SpatialServingIndex,
    snapshot_identity: &str,
    subject_id: &str,
    target_id: &str,
    relation: &str,
    metric: &str,
    confidence: f32,
) -> Option<DerivedEvidence> {
    let inputs = spatial.footprint_evidence_refs(&[subject_id, target_id], snapshot_identity)?;
    DerivedEvidence::new(
        snapshot_identity,
        subject_id,
        Some(target_id.to_string()),
        relation,
        metric,
        Some(1.0),
        Some("boolean".to_string()),
        "spatial-topology-v2",
        confidence,
        inputs,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::knowledge::FactValue;
    use crate::serving::{ServingFactRecord, SourceObservation};

    fn entity(id: &str, kind: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: kind.to_string(),
            name: id.to_string(),
            root_source: Some("test".to_string()),
            visibility: Default::default(),
            searchable_text: id.to_string(),
        }
    }

    fn fact(id: &str, key: &str, value: FactValue, source: &str) -> ServingFactRecord {
        let observed_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let source_url = Some("https://example.test/source".to_string());
        ServingFactRecord {
            entity_id: id.to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: source.to_string(),
            source_url: source_url.clone(),
            model: None,
            skill_id: Some("spatial_topology_contract".to_string()),
            learned_at: observed_at,
            observation: Some(
                SourceObservation::new(
                    "TopologyFixture",
                    format!("source-record:{id}"),
                    id,
                    observed_at,
                    source_url,
                    vec!["asset:topology-fixture/v1".to_string()],
                )
                .unwrap(),
            ),
        }
    }

    #[test]
    fn society_polygon_uses_greatest_overlap_without_point_containment() {
        let entities = vec![
            entity("area:east", "area"),
            entity("area:hoodi", "area"),
            entity("area:whitefield", "area"),
            entity("society:godrej-air", "society"),
            entity("place:metro", "place"),
        ];
        let facts = ServingFactIndex::from_records(
            vec![
                fact(
                    "area:east",
                    "geo.geometry_geojson",
                    FactValue::Text(
                        r#"{"type":"Polygon","coordinates":[[[-1,-1],[11,-1],[11,6],[-1,6],[-1,-1]]]}"#
                            .to_string(),
                    ),
                    "OpenStreetMap",
                ),
                fact(
                    "area:east",
                    "area.admin_level",
                    FactValue::Text("9".to_string()),
                    "OpenStreetMap",
                ),
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
                    "area:hoodi",
                    "area.admin_level",
                    FactValue::Text("10".to_string()),
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
                    "area:whitefield",
                    "area.admin_level",
                    FactValue::Text("10".to_string()),
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
        let existing_edges = vec![ServingEdgeRecord {
            from_entity_id: "society:godrej-air".to_string(),
            edge_type: IN_AREA.to_string(),
            to_entity_id: "area:east".to_string(),
            confidence: 0.9,
            source_type: "LegacyTopology".to_string(),
            derivation: None,
        }];
        let report = derive_spatial_topology(
            &entities,
            &facts,
            &existing_edges,
            &SpatialTopologyPolicy::default(),
            "topology-fixture",
        );
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "society:godrej-air"
                && edge.edge_type == IN_AREA
                && edge.to_entity_id == "area:hoodi"));
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "society:godrej-air"
                && edge.edge_type == IN_AREA
                && edge.to_entity_id == "area:east"));
        assert!(!report
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
        assert!(report
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "area:hoodi"
                && edge.edge_type == IN_AREA
                && edge.to_entity_id == "area:east"));
        assert!(!report.edges.iter().any(|edge| {
            edge.edge_type == ADJACENT_AREA
                && (edge.from_entity_id == "area:east" || edge.to_entity_id == "area:east")
        }));
        assert!(report.edges.iter().all(|edge| edge
            .derivation
            .as_ref()
            .is_some_and(|derivation| derivation.validate().is_ok())));
        assert!(super::super::validate_serving_edge_evidence(
            &report.edges,
            facts.all_facts(),
            "topology-fixture",
        )
        .is_ok());

        let serving =
            SpatialServingIndex::from_serving_bundle_with_edges(&entities, &facts, &report.edges);
        assert_eq!(
            serving.society_ids_inside("area:hoodi"),
            ["society:godrej-air"]
        );
        assert!(serving.society_ids_inside("area:whitefield").is_empty());
    }
}
