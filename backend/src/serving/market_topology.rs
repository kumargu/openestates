use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::dag_config::{normalize_source_type, MarketLocalityPolicy, SpatialTopologyPolicy};
use crate::knowledge::FactValue;

use super::{
    DerivedEvidence, EvidenceRef, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, SpatialServingIndex,
};

const IN_MARKET_LOCALITY: &str = "in_market_locality";
const OCCUPIES_GEO_CELL: &str = "occupies_geo_cell";
const COVERS_GEO_CELL: &str = "covers_geo_cell";
const ADMIN_LEVEL_FACT_KEY: &str = "area.admin_level";
const MARKET_LOCALITY_ROOT_SOURCE: &str = "market_locality";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketGeoTopologyReport {
    pub base_entity_count: usize,
    pub base_fact_count: usize,
    pub direct_market_locality_edge_count: usize,
    pub proximity_market_locality_edge_count: usize,
    pub geo_cell_area_count: usize,
    pub footprint_geo_cell_edge_count: usize,
    pub point_geo_cell_edge_count: usize,
    pub zero_cell_point_entity_ids: Vec<String>,
    pub ambiguous_point_cell_entity_ids: Vec<String>,
    pub multi_cell_footprint_entity_ids: Vec<String>,
    pub market_geo_cell_edge_count: usize,
    pub internal_geo_cell_entity_ids: Vec<String>,
}

#[derive(Debug)]
pub struct MarketGeoTopologyError(String);

impl fmt::Display for MarketGeoTopologyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MarketGeoTopologyError {}

/// Derive buyer market membership and internal geo-cell topology from the
/// normal serving inputs. This runs after canonical identity and physical area
/// topology, and before proximity facts are materialized.
pub fn derive_market_geo_topology(
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    market_policy: &MarketLocalityPolicy,
    spatial_policy: &SpatialTopologyPolicy,
    snapshot_identity: &str,
) -> Result<(Vec<ServingEdgeRecord>, MarketGeoTopologyReport), MarketGeoTopologyError> {
    let mut report = MarketGeoTopologyReport {
        base_entity_count: entities.len(),
        base_fact_count: facts.len(),
        ..MarketGeoTopologyReport::default()
    };
    let mut market_edges =
        derive_market_locality_edges(entities, facts, market_policy, snapshot_identity)?;
    report.direct_market_locality_edge_count = market_edges
        .iter()
        .filter(|edge| {
            edge.derivation
                .as_ref()
                .is_some_and(|derivation| derivation.metric == "address_component_match")
        })
        .count();
    report.proximity_market_locality_edge_count =
        market_edges.len() - report.direct_market_locality_edge_count;
    let mut geo_cell_edges = derive_geo_cell_edges(
        entities,
        facts,
        &market_edges,
        spatial_policy,
        snapshot_identity,
        &mut report,
    )?;
    market_edges.append(&mut geo_cell_edges);
    market_edges.sort_by(|left, right| {
        left.from_entity_id
            .cmp(&right.from_entity_id)
            .then_with(|| left.edge_type.cmp(&right.edge_type))
            .then_with(|| left.to_entity_id.cmp(&right.to_entity_id))
    });
    Ok((market_edges, report))
}

pub fn remove_derived_market_geo_topology_edges(edges: &mut Vec<ServingEdgeRecord>) {
    edges.retain(|edge| {
        !matches!(
            edge.edge_type.to_ascii_lowercase().as_str(),
            IN_MARKET_LOCALITY | OCCUPIES_GEO_CELL | COVERS_GEO_CELL
        )
    });
}

fn derive_market_locality_edges(
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    policy: &MarketLocalityPolicy,
    snapshot_identity: &str,
) -> Result<Vec<ServingEdgeRecord>, MarketGeoTopologyError> {
    if snapshot_identity.trim().is_empty() {
        return Err(MarketGeoTopologyError(
            "market-locality derivation requires a snapshot identity".to_string(),
        ));
    }
    let fact_index = ServingFactIndex::from_records(facts.to_vec(), Vec::new());
    let spatial = SpatialServingIndex::from_serving_bundle(entities, &fact_index);
    let address_entity_ids = entities
        .iter()
        .filter(|entity| {
            entity.entity_type.eq_ignore_ascii_case("society")
                || entity.entity_type.eq_ignore_ascii_case("place")
        })
        .map(|entity| entity.entity_id.as_str())
        .collect::<HashSet<_>>();
    let address_keys = policy
        .direct_address_fact_keys
        .iter()
        .map(|key| key.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let address_sources = policy
        .direct_address_allowed_sources
        .iter()
        .map(|source| normalize_source_type(source))
        .collect::<HashSet<_>>();
    let mut edges = Vec::new();

    for locality in entities.iter().filter(|entity| {
        entity.entity_type.eq_ignore_ascii_case("area")
            && entity
                .root_source
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(MARKET_LOCALITY_ROOT_SOURCE))
    }) {
        let Some(locality_name_fact) = fact_index
            .entity(&locality.entity_id)
            .into_iter()
            .flat_map(|rows| &rows.facts)
            .find(|fact| {
                fact.fact_key
                    .eq_ignore_ascii_case(&policy.market_name_fact_key)
                    && fact.observation.is_some()
            })
        else {
            continue;
        };
        let normalized_locality = normalize_exact_area_name(&locality.name);
        let mut direct = BTreeMap::<String, &ServingFactRecord>::new();
        for fact in facts.iter().filter(|fact| {
            address_entity_ids.contains(fact.entity_id.as_str())
                && address_keys.contains(&fact.fact_key.to_ascii_lowercase())
                && address_sources.contains(&normalize_source_type(&fact.source_type))
                && fact.confidence >= policy.minimum_direct_address_confidence
                && fact.observation.is_some()
        }) {
            let FactValue::Text(address) = &fact.value else {
                continue;
            };
            if address_has_component(address, &normalized_locality) {
                let replace = direct.get(&fact.entity_id).is_none_or(|current| {
                    fact.confidence > current.confidence
                        || (fact.confidence == current.confidence
                            && fact.stable_selection_key() < current.stable_selection_key())
                });
                if replace {
                    direct.insert(fact.entity_id.clone(), fact);
                }
            }
        }

        for (society_id, address_fact) in &direct {
            let input_evidence = vec![
                evidence_for_fact(locality_name_fact, snapshot_identity)?,
                evidence_for_fact(address_fact, snapshot_identity)?,
            ];
            let confidence = locality_name_fact.confidence.min(address_fact.confidence);
            edges.push(market_locality_edge(
                society_id,
                &locality.entity_id,
                "address_component_match",
                confidence,
                input_evidence,
                snapshot_identity,
            )?);
        }

        for candidate in spatial
            .points()
            .iter()
            .filter(|point| point.entity_type.eq_ignore_ascii_case("society"))
            .filter(|point| !direct.contains_key(&point.entity_id))
        {
            let nearest = direct
                .iter()
                .filter_map(|(core_id, address_fact)| {
                    spatial
                        .distance_between(&candidate.entity_id, core_id, snapshot_identity)
                        .filter(|distance| distance.distance_km <= policy.neighborhood_radius_km)
                        .map(|distance| (core_id, *address_fact, distance))
                })
                .min_by(|left, right| {
                    left.2
                        .distance_km
                        .total_cmp(&right.2.distance_km)
                        .then_with(|| left.0.cmp(right.0))
                });
            let Some((_core_id, address_fact, distance)) = nearest else {
                continue;
            };
            let mut input_evidence = vec![
                evidence_for_fact(locality_name_fact, snapshot_identity)?,
                evidence_for_fact(address_fact, snapshot_identity)?,
            ];
            input_evidence.extend(distance.evidence_refs);
            let confidence = locality_name_fact
                .confidence
                .min(address_fact.confidence)
                .min(distance.confidence);
            edges.push(market_locality_edge(
                &candidate.entity_id,
                &locality.entity_id,
                "qualified_society_proximity",
                confidence,
                input_evidence,
                snapshot_identity,
            )?);
        }
    }
    edges.sort_by(|left, right| {
        left.from_entity_id
            .cmp(&right.from_entity_id)
            .then_with(|| left.to_entity_id.cmp(&right.to_entity_id))
    });
    edges.dedup_by(|left, right| {
        left.from_entity_id == right.from_entity_id
            && left.edge_type == right.edge_type
            && left.to_entity_id == right.to_entity_id
    });
    Ok(edges)
}

fn derive_geo_cell_edges(
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    market_edges: &[ServingEdgeRecord],
    policy: &SpatialTopologyPolicy,
    snapshot_identity: &str,
    report: &mut MarketGeoTopologyReport,
) -> Result<Vec<ServingEdgeRecord>, MarketGeoTopologyError> {
    let fact_index = ServingFactIndex::from_records(facts.to_vec(), Vec::new());
    let spatial = SpatialServingIndex::from_serving_bundle(entities, &fact_index);
    let allowed_sources = policy
        .geo_cell_area_sources
        .iter()
        .map(|source| normalize_source_type(source))
        .collect::<HashSet<_>>();
    let cell_ids = entities
        .iter()
        .filter(|entity| entity.entity_type.eq_ignore_ascii_case("area"))
        .filter(|entity| {
            entity
                .root_source
                .as_deref()
                .is_some_and(|source| allowed_sources.contains(&normalize_source_type(source)))
        })
        .filter(|entity| {
            serving_admin_level(&fact_index, &entity.entity_id) == Some(policy.geo_cell_admin_level)
        })
        .filter(|entity| spatial.geometry().has_footprint(&entity.entity_id))
        .map(|entity| entity.entity_id.clone())
        .collect::<HashSet<_>>();
    report.geo_cell_area_count = cell_ids.len();
    report.internal_geo_cell_entity_ids = cell_ids.iter().cloned().collect();
    report.internal_geo_cell_entity_ids.sort();
    if cell_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut occupies = Vec::new();
    for entity in entities.iter().filter(|entity| {
        entity.entity_type.eq_ignore_ascii_case("society")
            || entity.entity_type.eq_ignore_ascii_case("place")
    }) {
        if spatial.geometry().has_footprint(&entity.entity_id) {
            let mut matches = spatial
                .geometry()
                .features_intersecting(&entity.entity_id)
                .into_iter()
                .filter(|cell| cell_ids.contains(&cell.entity_id))
                .filter_map(|cell| {
                    spatial
                        .geometry()
                        .overlap_ratio(&entity.entity_id, &cell.entity_id)
                        .filter(|ratio| *ratio >= policy.minimum_geo_cell_overlap_ratio)
                        .map(|ratio| (cell, ratio))
                })
                .collect::<Vec<_>>();
            matches.sort_by(|(left, _), (right, _)| left.entity_id.cmp(&right.entity_id));
            if matches.len() > 1 {
                report
                    .multi_cell_footprint_entity_ids
                    .push(entity.entity_id.clone());
            }
            for (cell, overlap_ratio) in matches {
                let Some(input_evidence) = spatial.footprint_evidence_refs(
                    &[&entity.entity_id, &cell.entity_id],
                    snapshot_identity,
                ) else {
                    continue;
                };
                let confidence = spatial
                    .geometry()
                    .feature(&entity.entity_id)
                    .map_or(0.0, |feature| feature.confidence)
                    .min(cell.confidence);
                occupies.push(geo_cell_edge(
                    &entity.entity_id,
                    &cell.entity_id,
                    OCCUPIES_GEO_CELL,
                    "footprint_overlap_ratio",
                    Some(overlap_ratio),
                    Some("ratio"),
                    confidence,
                    input_evidence,
                    snapshot_identity,
                )?);
                report.footprint_geo_cell_edge_count += 1;
            }
            continue;
        }

        let Some(point) = spatial
            .point_for_entity(&entity.entity_id)
            .filter(|point| point.confidence >= policy.minimum_point_confidence)
            .filter(|point| !point.observations.is_empty())
        else {
            continue;
        };
        let mut containing = spatial
            .geometry()
            .features_containing_coordinate(point.latitude, point.longitude)
            .into_iter()
            .filter(|cell| cell_ids.contains(&cell.entity_id))
            .collect::<Vec<_>>();
        containing.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        containing.dedup_by(|left, right| left.entity_id == right.entity_id);
        match containing.as_slice() {
            [] => report
                .zero_cell_point_entity_ids
                .push(entity.entity_id.clone()),
            [cell] => {
                let Some(cell_observation) = cell.observation.as_ref() else {
                    continue;
                };
                let mut input_evidence = point
                    .observations
                    .iter()
                    .map(|observation| EvidenceRef::for_observation(snapshot_identity, observation))
                    .collect::<Vec<_>>();
                input_evidence.push(EvidenceRef::for_observation(
                    snapshot_identity,
                    cell_observation,
                ));
                occupies.push(geo_cell_edge(
                    &entity.entity_id,
                    &cell.entity_id,
                    OCCUPIES_GEO_CELL,
                    "qualified_point_containment",
                    Some(1.0),
                    Some("boolean"),
                    point.confidence.min(cell.confidence),
                    input_evidence,
                    snapshot_identity,
                )?);
                report.point_geo_cell_edge_count += 1;
            }
            _ => report
                .ambiguous_point_cell_entity_ids
                .push(entity.entity_id.clone()),
        }
    }
    report.multi_cell_footprint_entity_ids.sort();
    report.multi_cell_footprint_entity_ids.dedup();
    report.zero_cell_point_entity_ids.sort();
    report.zero_cell_point_entity_ids.dedup();
    report.ambiguous_point_cell_entity_ids.sort();
    report.ambiguous_point_cell_entity_ids.dedup();

    let mut occupancy_by_entity = HashMap::<&str, Vec<&ServingEdgeRecord>>::new();
    for edge in &occupies {
        occupancy_by_entity
            .entry(edge.from_entity_id.as_str())
            .or_default()
            .push(edge);
    }
    let mut coverage_inputs = BTreeMap::<(String, String), (f32, Vec<EvidenceRef>)>::new();
    for membership in market_edges {
        let Some(membership_derivation) = membership.derivation.as_ref() else {
            continue;
        };
        for occupancy in occupancy_by_entity
            .get(membership.from_entity_id.as_str())
            .into_iter()
            .flatten()
        {
            let Some(occupancy_derivation) = occupancy.derivation.as_ref() else {
                continue;
            };
            let entry = coverage_inputs
                .entry((
                    membership.to_entity_id.clone(),
                    occupancy.to_entity_id.clone(),
                ))
                .or_insert((1.0, Vec::new()));
            entry.0 = entry.0.min(membership.confidence).min(occupancy.confidence);
            entry
                .1
                .push(EvidenceRef::for_derivation(membership_derivation));
            entry
                .1
                .push(EvidenceRef::for_derivation(occupancy_derivation));
        }
    }
    let mut coverage = coverage_inputs
        .into_iter()
        .map(|((market_id, cell_id), (confidence, input_evidence))| {
            geo_cell_edge(
                &market_id,
                &cell_id,
                COVERS_GEO_CELL,
                "member_geo_cell_coverage",
                Some(1.0),
                Some("boolean"),
                confidence,
                input_evidence,
                snapshot_identity,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    report.market_geo_cell_edge_count = coverage.len();
    occupies.append(&mut coverage);
    occupies.sort_by(|left, right| {
        left.from_entity_id
            .cmp(&right.from_entity_id)
            .then_with(|| left.edge_type.cmp(&right.edge_type))
            .then_with(|| left.to_entity_id.cmp(&right.to_entity_id))
    });
    Ok(occupies)
}

fn serving_admin_level(facts: &ServingFactIndex, entity_id: &str) -> Option<u8> {
    facts
        .entity(entity_id)?
        .facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(ADMIN_LEVEL_FACT_KEY))
        .find_map(|fact| match &fact.value {
            FactValue::Text(value) => value.trim().parse().ok(),
            FactValue::Numeric(value) if value.fract() == 0.0 => Some(*value as u8),
            _ => None,
        })
}

#[allow(clippy::too_many_arguments)]
fn geo_cell_edge(
    subject_id: &str,
    cell_id: &str,
    relation: &str,
    metric: &str,
    value: Option<f64>,
    unit: Option<&str>,
    confidence: f32,
    input_evidence: Vec<EvidenceRef>,
    snapshot_identity: &str,
) -> Result<ServingEdgeRecord, MarketGeoTopologyError> {
    let derivation = DerivedEvidence::new(
        snapshot_identity,
        subject_id,
        Some(cell_id.to_string()),
        relation,
        metric,
        value,
        unit.map(str::to_string),
        "geo-cell-materialization-v1",
        confidence,
        input_evidence,
    )
    .map_err(topology_error)?;
    Ok(ServingEdgeRecord {
        from_entity_id: subject_id.to_string(),
        edge_type: relation.to_string(),
        to_entity_id: cell_id.to_string(),
        confidence,
        source_type: "GeoCellMaterialization".to_string(),
        derivation: Some(derivation),
    })
}

fn address_has_component(address: &str, normalized_locality: &str) -> bool {
    address
        .split(',')
        .map(normalize_exact_area_name)
        .any(|component| component == normalized_locality)
}

fn evidence_for_fact(
    fact: &ServingFactRecord,
    snapshot_identity: &str,
) -> Result<EvidenceRef, MarketGeoTopologyError> {
    let observation = fact.observation.as_ref().ok_or_else(|| {
        MarketGeoTopologyError(format!(
            "market-locality input {}/{} lacks an observation",
            fact.entity_id, fact.fact_key
        ))
    })?;
    Ok(EvidenceRef::for_observation(snapshot_identity, observation))
}

fn market_locality_edge(
    society_id: &str,
    locality_id: &str,
    metric: &str,
    confidence: f32,
    input_evidence: Vec<EvidenceRef>,
    snapshot_identity: &str,
) -> Result<ServingEdgeRecord, MarketGeoTopologyError> {
    let derivation = DerivedEvidence::new(
        snapshot_identity,
        society_id,
        Some(locality_id.to_string()),
        IN_MARKET_LOCALITY,
        metric,
        Some(1.0),
        Some("boolean".to_string()),
        "market-locality-v1",
        confidence,
        input_evidence,
    )
    .map_err(topology_error)?;
    Ok(ServingEdgeRecord {
        from_entity_id: society_id.to_string(),
        edge_type: IN_MARKET_LOCALITY.to_string(),
        to_entity_id: locality_id.to_string(),
        confidence,
        source_type: "MarketLocality".to_string(),
        derivation: Some(derivation),
    })
}

fn normalize_exact_area_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn topology_error(error: impl fmt::Display) -> MarketGeoTopologyError {
    MarketGeoTopologyError(error.to_string())
}
