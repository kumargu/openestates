use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use sha2::{Digest, Sha256};

use crate::dag_config::{
    nearby_place_categories_config, normalize_source_type, CoordinateEntityScope,
    NearbyPlaceCategory, SpatialRole,
};
use crate::knowledge::FactValue;
use crate::search::geo::haversine_km;

use super::{
    resolve_serving_coordinates, DerivedEvidence, EvidenceRef, ServingEdgeRecord,
    ServingEntityFactRows, ServingEntityRecord, ServingFactIndex, SpatialGeometryIndex,
};

pub const CANONICAL_SPATIAL_ROOT_SOURCE: &str = "canonical_spatial_identity";
pub const PROVIDER_BINDING_EDGE: &str = "has_provider_binding";
pub const CANONICAL_IDENTITY_ALGORITHM: &str = "canonical-spatial-identity-v1";

#[derive(Debug, Clone, Default)]
pub struct CanonicalSpatialIdentityReport {
    pub canonical_entities: Vec<ServingEntityRecord>,
    pub binding_edges: Vec<ServingEdgeRecord>,
    pub ambiguous_provider_entity_ids: Vec<String>,
    pub unclassified_provider_entity_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct PlaceCandidate<'a> {
    entity: &'a ServingEntityRecord,
    category: &'static NearbyPlaceCategory,
    provider: String,
    normalized_name: String,
    latitude: f64,
    longitude: f64,
    confidence: f32,
    area_ids: BTreeSet<String>,
    evidence_refs: Vec<EvidenceRef>,
    searchable_terms: Vec<String>,
}

pub fn remove_canonical_spatial_identities(
    entities: &mut Vec<ServingEntityRecord>,
    edges: &mut Vec<ServingEdgeRecord>,
) {
    let canonical_ids = entities
        .iter()
        .filter(|entity| is_canonical_spatial_entity(entity))
        .map(|entity| entity.entity_id.clone())
        .collect::<HashSet<_>>();
    entities.retain(|entity| !canonical_ids.contains(&entity.entity_id));
    edges.retain(|edge| {
        !edge.edge_type.eq_ignore_ascii_case(PROVIDER_BINDING_EDGE)
            && !canonical_ids.contains(&edge.from_entity_id)
            && !canonical_ids.contains(&edge.to_entity_id)
    });
}

pub fn materialize_canonical_spatial_identities(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    existing_edges: &[ServingEdgeRecord],
    snapshot_identity: &str,
) -> Result<CanonicalSpatialIdentityReport, String> {
    let geometry = SpatialGeometryIndex::from_serving_bundle(entities, fact_index, existing_edges);
    let mut report = CanonicalSpatialIdentityReport::default();
    let mut candidates = Vec::new();
    for entity in entities.iter().filter(|entity| {
        entity.entity_type.eq_ignore_ascii_case("place") && !is_canonical_spatial_entity(entity)
    }) {
        let Some(rows) = fact_index.entity(&entity.entity_id) else {
            report
                .unclassified_provider_entity_ids
                .push(entity.entity_id.clone());
            continue;
        };
        let Some(candidate) = place_candidate(entity, rows, &geometry, snapshot_identity) else {
            report
                .unclassified_provider_entity_ids
                .push(entity.entity_id.clone());
            continue;
        };
        candidates.push(candidate);
    }
    candidates.sort_by(|left, right| {
        left.category
            .fact_key
            .cmp(&right.category.fact_key)
            .then_with(|| left.normalized_name.cmp(&right.normalized_name))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.entity.entity_id.cmp(&right.entity.entity_id))
    });

    let mut groups = BTreeMap::<(String, String), Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        groups
            .entry((
                candidate.category.fact_key.clone(),
                candidate.normalized_name.clone(),
            ))
            .or_default()
            .push(index);
    }
    let names_with_multiple_categories = candidates.iter().fold(
        BTreeMap::<&str, BTreeSet<&str>>::new(),
        |mut names, candidate| {
            names
                .entry(&candidate.normalized_name)
                .or_default()
                .insert(&candidate.category.fact_key);
            names
        },
    );
    let policy = &nearby_place_categories_config().canonical_identity;
    let mut used_ids = HashSet::new();

    for (_, group) in groups {
        let adjacency = compatible_candidate_graph(&candidates, &group, policy.merge_distance_m);
        let ambiguous = ambiguous_candidate_indices(&candidates, &group, &adjacency);
        report.ambiguous_provider_entity_ids.extend(
            ambiguous
                .iter()
                .map(|index| candidates[*index].entity.entity_id.clone()),
        );
        for component in connected_components(&group, &adjacency, &ambiguous) {
            let preferred =
                preferred_candidate(&candidates, &component, &policy.identity_source_priority);
            let needs_suffix = names_with_multiple_categories
                .get(preferred.normalized_name.as_str())
                .is_some_and(|categories| categories.len() > 1);
            let canonical_name = canonical_name(preferred, needs_suffix);
            let canonical_id = unique_canonical_id(preferred, &canonical_name, &mut used_ids);
            let searchable_text = component
                .iter()
                .flat_map(|index| candidates[*index].searchable_terms.iter())
                .chain(std::iter::once(&preferred.category.fact_key))
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(" ");
            report.canonical_entities.push(ServingEntityRecord {
                entity_id: canonical_id.clone(),
                entity_type: "place".to_string(),
                name: canonical_name,
                root_source: Some(CANONICAL_SPATIAL_ROOT_SOURCE.to_string()),
                searchable_text,
            });
            for index in component {
                let candidate = &candidates[index];
                let derivation = DerivedEvidence::new(
                    snapshot_identity,
                    canonical_id.clone(),
                    Some(candidate.entity.entity_id.clone()),
                    PROVIDER_BINDING_EDGE,
                    "canonical_identity_binding",
                    None,
                    None,
                    CANONICAL_IDENTITY_ALGORITHM,
                    candidate.confidence,
                    candidate.evidence_refs.clone(),
                )
                .map_err(|error| format!("invalid canonical place binding: {error}"))?;
                report.binding_edges.push(ServingEdgeRecord {
                    from_entity_id: canonical_id.clone(),
                    edge_type: PROVIDER_BINDING_EDGE.to_string(),
                    to_entity_id: candidate.entity.entity_id.clone(),
                    confidence: candidate.confidence,
                    source_type: "Computed".to_string(),
                    derivation: Some(derivation),
                });
            }
        }
        for index in ambiguous {
            let candidate = &candidates[index];
            let canonical_name = canonical_name(candidate, true);
            let canonical_id = unique_canonical_id(candidate, &canonical_name, &mut used_ids);
            report.canonical_entities.push(ServingEntityRecord {
                entity_id: canonical_id.clone(),
                entity_type: "place".to_string(),
                name: canonical_name,
                root_source: Some(CANONICAL_SPATIAL_ROOT_SOURCE.to_string()),
                searchable_text: candidate.searchable_terms.join(" "),
            });
            let derivation = DerivedEvidence::new(
                snapshot_identity,
                canonical_id.clone(),
                Some(candidate.entity.entity_id.clone()),
                PROVIDER_BINDING_EDGE,
                "canonical_identity_binding",
                None,
                None,
                CANONICAL_IDENTITY_ALGORITHM,
                candidate.confidence,
                candidate.evidence_refs.clone(),
            )
            .map_err(|error| format!("invalid ambiguous place binding: {error}"))?;
            report.binding_edges.push(ServingEdgeRecord {
                from_entity_id: canonical_id,
                edge_type: PROVIDER_BINDING_EDGE.to_string(),
                to_entity_id: candidate.entity.entity_id.clone(),
                confidence: candidate.confidence,
                source_type: "Computed".to_string(),
                derivation: Some(derivation),
            });
        }
    }

    report
        .canonical_entities
        .sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    report.binding_edges.sort_by(|left, right| {
        left.from_entity_id
            .cmp(&right.from_entity_id)
            .then_with(|| left.to_entity_id.cmp(&right.to_entity_id))
    });
    report.ambiguous_provider_entity_ids.sort();
    report.ambiguous_provider_entity_ids.dedup();
    report.unclassified_provider_entity_ids.sort();
    report.unclassified_provider_entity_ids.dedup();
    Ok(report)
}

pub fn is_canonical_spatial_entity(entity: &ServingEntityRecord) -> bool {
    entity.root_source.as_deref() == Some(CANONICAL_SPATIAL_ROOT_SOURCE)
}

pub fn bound_provider_entity_ids(edges: &[ServingEdgeRecord]) -> HashSet<&str> {
    edges
        .iter()
        .filter(|edge| edge.edge_type.eq_ignore_ascii_case(PROVIDER_BINDING_EDGE))
        .map(|edge| edge.to_entity_id.as_str())
        .collect()
}

pub fn validate_canonical_spatial_identities(
    entities: &[ServingEntityRecord],
    edges: &[ServingEdgeRecord],
) -> Result<(), String> {
    let entity_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<HashMap<_, _>>();
    let canonical_ids = entities
        .iter()
        .filter(|entity| is_canonical_spatial_entity(entity))
        .map(|entity| entity.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut canonical_by_provider = HashMap::<&str, &str>::new();
    let mut binding_count = HashMap::<&str, usize>::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.edge_type.eq_ignore_ascii_case(PROVIDER_BINDING_EDGE))
    {
        if !canonical_ids.contains(edge.from_entity_id.as_str()) {
            return Err(format!(
                "provider binding {} -> {} does not start at a canonical spatial entity",
                edge.from_entity_id, edge.to_entity_id
            ));
        }
        let Some(provider) = entity_by_id.get(edge.to_entity_id.as_str()) else {
            return Err(format!(
                "provider binding {} -> {} has a missing provider entity",
                edge.from_entity_id, edge.to_entity_id
            ));
        };
        if !provider.entity_type.eq_ignore_ascii_case("place")
            || is_canonical_spatial_entity(provider)
        {
            return Err(format!(
                "provider binding {} -> {} must target a source place",
                edge.from_entity_id, edge.to_entity_id
            ));
        }
        if let Some(existing) =
            canonical_by_provider.insert(&edge.to_entity_id, &edge.from_entity_id)
        {
            if existing != edge.from_entity_id {
                return Err(format!(
                    "provider place {} is bound to multiple canonical entities",
                    edge.to_entity_id
                ));
            }
        }
        *binding_count.entry(&edge.from_entity_id).or_default() += 1;
    }
    if let Some(canonical_id) = canonical_ids
        .iter()
        .find(|canonical_id| !binding_count.contains_key(**canonical_id))
    {
        return Err(format!(
            "canonical spatial entity {canonical_id} has no provider binding"
        ));
    }
    Ok(())
}

pub fn provider_entity_ids<'a>(edges: &'a [ServingEdgeRecord], canonical_id: &str) -> Vec<&'a str> {
    let mut ids = edges
        .iter()
        .filter(|edge| {
            edge.from_entity_id == canonical_id
                && edge.edge_type.eq_ignore_ascii_case(PROVIDER_BINDING_EDGE)
        })
        .map(|edge| edge.to_entity_id.as_str())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

pub fn classify_place<'a>(
    name: &str,
    rows: &ServingEntityFactRows,
) -> Option<&'a NearbyPlaceCategory> {
    let place_types = text_tags(rows, "place.types");
    let place_category = text_fact(rows, "place.category");
    let matches = nearby_place_categories_config()
        .categories
        .iter()
        .filter(|category| category.matches_place(name, &place_types, place_category.as_deref()))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0])
}

fn place_candidate<'a>(
    entity: &'a ServingEntityRecord,
    rows: &ServingEntityFactRows,
    geometry: &SpatialGeometryIndex,
    snapshot_identity: &str,
) -> Option<PlaceCandidate<'a>> {
    let coordinates = resolve_serving_coordinates(rows, CoordinateEntityScope::Place)?;
    let category = classify_place(&entity.name, rows)?;
    let evidence_refs = rows
        .facts
        .iter()
        .filter(|fact| {
            [
                "place.name",
                "place.category",
                "place.types",
                "geo.latitude",
                "geo.longitude",
            ]
            .iter()
            .any(|key| fact.fact_key.eq_ignore_ascii_case(key))
        })
        .filter_map(|fact| fact.observation.as_ref())
        .map(|observation| EvidenceRef::for_observation(snapshot_identity, observation))
        .collect::<Vec<_>>();
    if evidence_refs.is_empty() {
        return None;
    }
    let area_ids = geometry
        .features_containing_coordinate(coordinates.latitude, coordinates.longitude)
        .into_iter()
        .filter(|feature| feature.entity_type.eq_ignore_ascii_case("area"))
        .map(|feature| feature.entity_id.clone())
        .collect();
    let mut searchable_terms = vec![entity.name.clone(), category.fact_key.clone()];
    searchable_terms.extend(text_tags(rows, "place.types"));
    Some(PlaceCandidate {
        entity,
        category,
        provider: normalize_source_type(&coordinates.source_type),
        normalized_name: normalize_name(&entity.name),
        latitude: coordinates.latitude,
        longitude: coordinates.longitude,
        confidence: coordinates.confidence,
        area_ids,
        evidence_refs,
        searchable_terms,
    })
}

fn compatible_candidate_graph(
    candidates: &[PlaceCandidate<'_>],
    group: &[usize],
    merge_distance_m: f64,
) -> HashMap<usize, Vec<usize>> {
    let mut graph = HashMap::<usize, Vec<usize>>::new();
    for (offset, left_index) in group.iter().enumerate() {
        for right_index in group.iter().skip(offset + 1) {
            let left = &candidates[*left_index];
            let right = &candidates[*right_index];
            if left.provider == right.provider
                || (!left.area_ids.is_empty()
                    && !right.area_ids.is_empty()
                    && left.area_ids.is_disjoint(&right.area_ids))
                || haversine_km(
                    left.latitude,
                    left.longitude,
                    right.latitude,
                    right.longitude,
                ) * 1000.0
                    > merge_distance_m
            {
                continue;
            }
            graph.entry(*left_index).or_default().push(*right_index);
            graph.entry(*right_index).or_default().push(*left_index);
        }
    }
    graph
}

fn ambiguous_candidate_indices(
    candidates: &[PlaceCandidate<'_>],
    group: &[usize],
    adjacency: &HashMap<usize, Vec<usize>>,
) -> HashSet<usize> {
    let mut ambiguous = HashSet::new();
    for index in group {
        let mut counts = HashMap::<&str, usize>::new();
        for neighbor in adjacency.get(index).into_iter().flatten() {
            *counts.entry(&candidates[*neighbor].provider).or_default() += 1;
        }
        if counts.values().any(|count| *count > 1) {
            ambiguous.insert(*index);
            ambiguous.extend(adjacency.get(index).into_iter().flatten().copied());
        }
    }
    ambiguous
}

fn connected_components(
    group: &[usize],
    adjacency: &HashMap<usize, Vec<usize>>,
    excluded: &HashSet<usize>,
) -> Vec<Vec<usize>> {
    let mut remaining = group
        .iter()
        .filter(|index| !excluded.contains(index))
        .copied()
        .collect::<BTreeSet<_>>();
    let mut components = Vec::new();
    while let Some(start) = remaining.pop_first() {
        let mut component = Vec::new();
        let mut queue = VecDeque::from([start]);
        while let Some(index) = queue.pop_front() {
            component.push(index);
            for neighbor in adjacency.get(&index).into_iter().flatten() {
                if !excluded.contains(neighbor) && remaining.remove(neighbor) {
                    queue.push_back(*neighbor);
                }
            }
        }
        component.sort();
        components.push(component);
    }
    components
}

fn preferred_candidate<'a>(
    candidates: &'a [PlaceCandidate<'a>],
    component: &[usize],
    source_priority: &[String],
) -> &'a PlaceCandidate<'a> {
    component
        .iter()
        .map(|index| &candidates[*index])
        .min_by(|left, right| {
            source_rank(&left.provider, source_priority)
                .cmp(&source_rank(&right.provider, source_priority))
                .then_with(|| right.confidence.total_cmp(&left.confidence))
                .then_with(|| left.entity.entity_id.cmp(&right.entity.entity_id))
        })
        .expect("canonical component is not empty")
}

fn source_rank(source: &str, source_priority: &[String]) -> usize {
    source_priority
        .iter()
        .position(|candidate| normalize_source_type(candidate) == source)
        .unwrap_or(usize::MAX)
}

fn canonical_name(candidate: &PlaceCandidate<'_>, needs_suffix: bool) -> String {
    let Some(suffix) = candidate
        .category
        .canonical_name_suffix
        .as_deref()
        .filter(|suffix| needs_suffix && !suffix.trim().is_empty())
    else {
        return candidate.entity.name.clone();
    };
    if normalize_name(&candidate.entity.name).contains(&normalize_name(suffix)) {
        candidate.entity.name.clone()
    } else {
        format!("{} {suffix}", candidate.entity.name)
    }
}

fn unique_canonical_id(
    candidate: &PlaceCandidate<'_>,
    canonical_name: &str,
    used_ids: &mut HashSet<String>,
) -> String {
    let base_payload = format!(
        "{}|{}|{:.6}|{:.6}",
        candidate.category.fact_key,
        normalize_name(canonical_name),
        candidate.latitude,
        candidate.longitude
    );
    let mut id = format!(
        "place:canonical:{}:{}",
        slug(canonical_name),
        short_hash(&base_payload)
    );
    if !used_ids.insert(id.clone()) {
        id = format!(
            "place:canonical:{}:{}",
            slug(canonical_name),
            short_hash(&format!("{base_payload}|{}", candidate.entity.entity_id))
        );
        used_ids.insert(id.clone());
    }
    id
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn slug(value: &str) -> String {
    normalize_name(value).replace(' ', "-")
}

fn text_fact(rows: &ServingEntityFactRows, key: &str) -> Option<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .filter_map(|fact| match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            _ => fact
                .value_text
                .clone()
                .filter(|value| !value.trim().is_empty()),
        })
        .max_by_key(String::len)
}

fn text_tags(rows: &ServingEntityFactRows, key: &str) -> Vec<String> {
    rows.facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(key))
        .flat_map(|fact| match &fact.value {
            FactValue::Tags(values) => values.clone(),
            FactValue::Text(value) => vec![value.clone()],
            _ => Vec::new(),
        })
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub fn canonical_spatial_role(
    canonical_id: &str,
    edges: &[ServingEdgeRecord],
    facts: &ServingFactIndex,
) -> Option<SpatialRole> {
    provider_entity_ids(edges, canonical_id)
        .into_iter()
        .filter_map(|provider_id| facts.entity(provider_id))
        .filter_map(|rows| {
            let name = text_fact(rows, "place.name")?;
            classify_place(&name, rows).map(|category| category.spatial_role)
        })
        .next()
}
