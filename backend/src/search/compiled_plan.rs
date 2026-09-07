use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::serving::{EvidenceRef, ServingEdgeRecord, ServingEntityRecord, SpatialServingIndex};

use super::ast::{
    semantic_search_fingerprint, CompiledQuery, ConstraintExpr, ConstraintTerm, NumericBound,
};
use super::intent::{SearchIntent, SourceSpan};

pub type BranchId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoAnchor {
    pub entity_id: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoCellSeed {
    pub cell_id: String,
    pub supporting_evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoCellPath {
    pub cell_ids: Vec<String>,
    pub hops: u8,
    pub distance_km: f64,
    pub supporting_evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoCellSearchPolicy {
    pub max_hops: u8,
    pub max_distance_km: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeoTopologyLink {
    pub target_entity_id: String,
    pub evidence_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, Default)]
pub struct GeoTopologyIndex {
    entity_types: HashMap<String, String>,
    market_area_ids: HashSet<String>,
    market_memberships: HashMap<String, Vec<GeoTopologyLink>>,
    cell_occupancies: HashMap<String, Vec<GeoTopologyLink>>,
    market_cell_coverage: HashMap<String, Vec<GeoTopologyLink>>,
    cell_adjacency: HashMap<String, Vec<GeoTopologyLink>>,
    internal_cell_ids: HashSet<String>,
}

impl GeoTopologyIndex {
    pub fn build(
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
        snapshot_identity: &str,
    ) -> Self {
        let entity_types = entities
            .iter()
            .map(|entity| (entity.entity_id.clone(), entity.entity_type.clone()))
            .collect();
        let internal_cell_ids = entities
            .iter()
            .filter(|entity| !entity.visibility.is_searchable())
            .map(|entity| entity.entity_id.clone())
            .collect();
        let mut index = Self {
            entity_types,
            internal_cell_ids,
            ..Self::default()
        };
        for edge in edges {
            let Some(evidence_refs) = validated_edge_evidence(edge, snapshot_identity) else {
                continue;
            };
            let link = GeoTopologyLink {
                target_entity_id: edge.to_entity_id.clone(),
                evidence_refs: evidence_refs.clone(),
            };
            match edge.edge_type.as_str() {
                "in_market_locality" => {
                    index.market_area_ids.insert(edge.to_entity_id.clone());
                    push_topology_link(
                        index
                            .market_memberships
                            .entry(edge.from_entity_id.clone())
                            .or_default(),
                        link,
                    );
                }
                "occupies_geo_cell" => {
                    push_topology_link(
                        index
                            .cell_occupancies
                            .entry(edge.from_entity_id.clone())
                            .or_default(),
                        link,
                    );
                }
                "covers_geo_cell" => {
                    index.market_area_ids.insert(edge.from_entity_id.clone());
                    push_topology_link(
                        index
                            .market_cell_coverage
                            .entry(edge.from_entity_id.clone())
                            .or_default(),
                        link,
                    );
                }
                "adjacent_area" => {
                    push_topology_link(
                        index
                            .cell_adjacency
                            .entry(edge.from_entity_id.clone())
                            .or_default(),
                        link,
                    );
                    push_topology_link(
                        index
                            .cell_adjacency
                            .entry(edge.to_entity_id.clone())
                            .or_default(),
                        GeoTopologyLink {
                            target_entity_id: edge.from_entity_id.clone(),
                            evidence_refs,
                        },
                    );
                }
                _ => {}
            }
        }
        index
    }

    pub fn entity_type(&self, entity_id: &str) -> Option<&str> {
        self.entity_types.get(entity_id).map(String::as_str)
    }

    pub fn is_market_area(&self, entity_id: &str) -> bool {
        self.market_area_ids.contains(entity_id)
    }

    pub fn market_memberships(&self, entity_id: &str) -> &[GeoTopologyLink] {
        self.market_memberships
            .get(entity_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn occupied_cells(&self, entity_id: &str) -> &[GeoTopologyLink] {
        self.cell_occupancies
            .get(entity_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn covered_cells(&self, market_id: &str) -> &[GeoTopologyLink] {
        self.market_cell_coverage
            .get(market_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn adjacent_cells(&self, cell_id: &str) -> &[GeoTopologyLink] {
        self.cell_adjacency
            .get(cell_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn are_adjacent(&self, left: &str, right: &str) -> bool {
        self.adjacent_cells(left)
            .iter()
            .any(|link| link.target_entity_id == right)
    }

    pub fn is_internal_cell(&self, entity_id: &str) -> bool {
        self.internal_cell_ids.contains(entity_id)
    }
}

fn push_topology_link(links: &mut Vec<GeoTopologyLink>, link: GeoTopologyLink) {
    if links
        .iter()
        .any(|existing| existing.target_entity_id == link.target_entity_id)
    {
        return;
    }
    links.push(link);
    links.sort_by(|left, right| left.target_entity_id.cmp(&right.target_entity_id));
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "camelCase")]
pub enum BoolExpr<T> {
    All(Vec<BoolExpr<T>>),
    Any(Vec<BoolExpr<T>>),
    Not(Box<BoolExpr<T>>),
    Leaf(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEntityHandle {
    pub entity_id: String,
    pub entity_type: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GeoScope {
    Scoped {
        anchors: Vec<GeoAnchor>,
        market_locality_ids: Vec<String>,
        seed_cells: Vec<GeoCellSeed>,
        expanded_cell_paths: Vec<GeoCellPath>,
        supporting_evidence: Vec<EvidenceRef>,
        max_distance_km: f64,
    },
    BundleWide,
}

impl GeoScope {
    pub fn market_locality_ids(&self) -> &[String] {
        match self {
            Self::Scoped {
                market_locality_ids,
                ..
            } => market_locality_ids,
            Self::BundleWide => &[],
        }
    }

    pub fn cell_path(&self, cell_id: &str) -> Option<&GeoCellPath> {
        match self {
            Self::Scoped {
                expanded_cell_paths,
                ..
            } => expanded_cell_paths
                .iter()
                .find(|path| path.cell_ids.last().is_some_and(|id| id == cell_id)),
            Self::BundleWide => None,
        }
    }

    pub fn is_bundle_wide(&self) -> bool {
        matches!(self, Self::BundleWide)
    }
}

#[derive(Debug, Clone)]
pub struct GeoBranch {
    pub branch_id: BranchId,
    pub geo_cluster_id: String,
    pub source_spans: Vec<SourceSpan>,
    pub geo_scope: GeoScope,
    pub predicates: ConstraintExpr,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
    pub constraints: SearchIntent,
    pub buyer_summary: String,
    pub source_query: String,
    pub compiled_query: CompiledQuery,
}

#[derive(Debug, Clone)]
pub struct CompiledSearchPlan {
    pub root: BoolExpr<BranchId>,
    pub branches: Vec<GeoBranch>,
    pub resolution_gaps: Vec<String>,
    pub semantic_fingerprint: String,
    pub snapshot_identity: String,
}

impl CompiledSearchPlan {
    /// Compile without serving topology for inert cache/test values. Search
    /// execution always uses `compile_for_snapshot`.
    pub fn compile(compiled_query: CompiledQuery, snapshot_identity: impl Into<String>) -> Self {
        let snapshot_identity = snapshot_identity.into();
        let branches = compiled_query
            .branches
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, predicates)| GeoBranch {
                branch_id: format!("branch-{}", index + 1),
                geo_cluster_id: format!("geo-cluster-{}", index + 1),
                source_spans: all_source_spans(&predicates),
                geo_scope: GeoScope::BundleWide,
                buyer_summary: predicates.buyer_label(),
                source_query: compiled_query.raw.clone(),
                predicates: predicates.clone(),
                resolved_entities: Vec::new(),
                constraints: compiled_query.intent.clone(),
                compiled_query: compiled_query.for_branch(predicates),
            })
            .collect::<Vec<_>>();
        let predicates = branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let constraints = branches
            .iter()
            .map(|branch| branch.constraints.clone())
            .collect::<Vec<_>>();
        Self {
            root: root_for(&branches),
            branches,
            resolution_gaps: Vec::new(),
            semantic_fingerprint: semantic_search_fingerprint(&predicates, &constraints),
            snapshot_identity,
        }
    }

    pub fn compile_for_snapshot(
        compiled_query: CompiledQuery,
        snapshot_identity: impl Into<String>,
        resolved_entities: &[ResolvedEntityHandle],
        topology: &GeoTopologyIndex,
        spatial_index: Option<&SpatialServingIndex>,
        geo_cell_policy: GeoCellSearchPolicy,
    ) -> Self {
        let snapshot_identity = snapshot_identity.into();
        let mut resolution_gaps = Vec::new();
        let mut branches = compiled_query
            .branches
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, predicates)| {
                let source_spans = all_source_spans(&predicates);
                let branch_entities = resolved_entities
                    .iter()
                    .filter(|entity| {
                        entity.source_span.as_ref().is_none_or(|entity_span| {
                            source_spans
                                .iter()
                                .any(|branch_span| spans_overlap(branch_span, entity_span))
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let geo_scope = compile_geo_scope(
                    &predicates,
                    topology,
                    spatial_index,
                    geo_cell_policy,
                    &snapshot_identity,
                    &mut resolution_gaps,
                );

                // A named society is a geographic anchor. Its sourced area
                // owns recall and the society itself is not an eligibility
                // predicate. Ranking may still prioritize an eligible exact
                // society match through the config-owned search policy.
                let geography_spans = positive_geography_spans(&predicates);
                let mut eligibility_predicates = predicates.clone();
                eligibility_predicates.drop_society_includes();
                eligibility_predicates.drop_area_includes();
                let mut branch_query = compiled_query.for_branch(eligibility_predicates.clone());
                branch_query.raw =
                    scoring_query(&branch_query.raw, &source_spans, &geography_spans);
                let constraints =
                    project_branch_intent(&compiled_query.intent, &eligibility_predicates);
                branch_query.intent = constraints.clone();

                GeoBranch {
                    branch_id: format!("branch-{}", index + 1),
                    geo_cluster_id: String::new(),
                    source_spans,
                    geo_scope,
                    buyer_summary: eligibility_predicates.buyer_label(),
                    source_query: compiled_query.raw.clone(),
                    predicates,
                    resolved_entities: branch_entities,
                    constraints,
                    compiled_query: branch_query,
                }
            })
            .collect::<Vec<_>>();
        assign_geo_clusters(&mut branches, topology);
        let root = root_for(&branches);
        let predicates = branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let constraints = branches
            .iter()
            .map(|branch| branch.constraints.clone())
            .collect::<Vec<_>>();
        Self {
            root,
            branches,
            resolution_gaps,
            semantic_fingerprint: semantic_search_fingerprint(&predicates, &constraints),
            snapshot_identity,
        }
    }
}

fn root_for(branches: &[GeoBranch]) -> BoolExpr<BranchId> {
    if branches.len() == 1 {
        BoolExpr::Leaf(branches[0].branch_id.clone())
    } else {
        BoolExpr::Any(
            branches
                .iter()
                .map(|branch| BoolExpr::Leaf(branch.branch_id.clone()))
                .collect(),
        )
    }
}

fn compile_geo_scope<'a>(
    predicates: &'a ConstraintExpr,
    topology: &GeoTopologyIndex,
    spatial_index: Option<&SpatialServingIndex>,
    policy: GeoCellSearchPolicy,
    snapshot_identity: &str,
    resolution_gaps: &mut Vec<String>,
) -> GeoScope {
    let mut explicit_area_ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    collect_geo_anchors(
        predicates,
        false,
        topology,
        &mut explicit_area_ids,
        &mut anchors,
    );
    explicit_area_ids.retain(|area_id| topology.is_market_area(area_id));

    let mut market_locality_ids = explicit_area_ids
        .iter()
        .map(|id| (*id).to_string())
        .collect::<BTreeSet<_>>();
    let mut supporting_evidence = Vec::new();
    for anchor in &anchors {
        for membership in topology.market_memberships(anchor) {
            if !topology
                .entity_type(&membership.target_entity_id)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("area"))
            {
                continue;
            }
            market_locality_ids.insert(membership.target_entity_id.clone());
            extend_unique(&mut supporting_evidence, membership.evidence_refs.clone());
        }
    }

    let mut scope_anchors = explicit_area_ids
        .iter()
        .map(|id| GeoAnchor {
            entity_id: (*id).to_string(),
            entity_type: "area".to_string(),
        })
        .chain(anchors.iter().filter_map(|id| {
            Some(GeoAnchor {
                entity_id: (*id).to_string(),
                entity_type: topology.entity_type(id)?.to_string(),
            })
        }))
        .collect::<Vec<_>>();
    scope_anchors.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    scope_anchors.dedup_by(|left, right| left.entity_id == right.entity_id);

    let mut seed_cells_by_id = BTreeMap::<String, Vec<EvidenceRef>>::new();
    for anchor in &anchors {
        for occupancy in topology.occupied_cells(anchor) {
            extend_unique(
                seed_cells_by_id
                    .entry(occupancy.target_entity_id.clone())
                    .or_default(),
                occupancy.evidence_refs.clone(),
            );
        }
    }
    for market_id in &market_locality_ids {
        for coverage in topology.covered_cells(market_id) {
            extend_unique(
                seed_cells_by_id
                    .entry(coverage.target_entity_id.clone())
                    .or_default(),
                coverage.evidence_refs.clone(),
            );
        }
    }

    if scope_anchors.is_empty() {
        return GeoScope::BundleWide;
    }
    if seed_cells_by_id.is_empty() {
        for anchor in &scope_anchors {
            let gap = format!("missing sourced geo-cell seed for {}", anchor.entity_id);
            if !resolution_gaps.contains(&gap) {
                resolution_gaps.push(gap);
            }
        }
        return GeoScope::BundleWide;
    }
    let Some(spatial_index) = spatial_index else {
        resolution_gaps.push("missing spatial index for geo-cell traversal".to_string());
        return GeoScope::BundleWide;
    };

    let seed_cells = seed_cells_by_id
        .iter()
        .map(|(cell_id, evidence)| GeoCellSeed {
            cell_id: cell_id.clone(),
            supporting_evidence: evidence.clone(),
        })
        .collect::<Vec<_>>();
    for seed in &seed_cells {
        extend_unique(&mut supporting_evidence, seed.supporting_evidence.clone());
    }

    let expanded_cell_paths = expand_geo_cells(
        &scope_anchors,
        &seed_cells,
        topology,
        spatial_index,
        policy,
        snapshot_identity,
    );
    GeoScope::Scoped {
        anchors: scope_anchors,
        market_locality_ids: market_locality_ids.into_iter().collect(),
        seed_cells,
        expanded_cell_paths,
        supporting_evidence,
        max_distance_km: policy.max_distance_km,
    }
}

fn expand_geo_cells(
    anchors: &[GeoAnchor],
    seed_cells: &[GeoCellSeed],
    topology: &GeoTopologyIndex,
    spatial_index: &SpatialServingIndex,
    policy: GeoCellSearchPolicy,
    snapshot_identity: &str,
) -> Vec<GeoCellPath> {
    let non_area_anchors = anchors
        .iter()
        .filter(|anchor| !anchor.entity_type.eq_ignore_ascii_case("area"))
        .map(|anchor| anchor.entity_id.as_str())
        .collect::<Vec<_>>();
    let seed_ids = seed_cells
        .iter()
        .map(|seed| seed.cell_id.as_str())
        .collect::<Vec<_>>();
    let mut paths = BTreeMap::<String, GeoCellPath>::new();
    let mut queue = VecDeque::new();
    for seed in seed_cells {
        let path = GeoCellPath {
            cell_ids: vec![seed.cell_id.clone()],
            hops: 0,
            distance_km: 0.0,
            supporting_evidence: seed.supporting_evidence.clone(),
        };
        paths.insert(seed.cell_id.clone(), path.clone());
        queue.push_back(path);
    }

    while let Some(path) = queue.pop_front() {
        if path.hops >= policy.max_hops {
            continue;
        }
        let Some(current) = path.cell_ids.last() else {
            continue;
        };
        for adjacency in topology.adjacent_cells(current) {
            let next = &adjacency.target_entity_id;
            if path.cell_ids.contains(next) {
                continue;
            }
            let Some(distance) = geo_cell_distance(
                &non_area_anchors,
                &seed_ids,
                next,
                spatial_index,
                snapshot_identity,
            )
            .filter(|distance| distance.distance_km <= policy.max_distance_km) else {
                continue;
            };
            let mut candidate = GeoCellPath {
                cell_ids: path.cell_ids.clone(),
                hops: path.hops + 1,
                distance_km: distance.distance_km,
                supporting_evidence: path.supporting_evidence.clone(),
            };
            candidate.cell_ids.push(next.clone());
            extend_unique(
                &mut candidate.supporting_evidence,
                adjacency.evidence_refs.clone(),
            );
            extend_unique(&mut candidate.supporting_evidence, distance.evidence_refs);
            let replace = paths.get(next).is_none_or(|existing| {
                candidate.hops < existing.hops
                    || (candidate.hops == existing.hops
                        && (candidate.distance_km < existing.distance_km
                            || (candidate.distance_km == existing.distance_km
                                && candidate.cell_ids < existing.cell_ids)))
            });
            if replace {
                paths.insert(next.clone(), candidate.clone());
                queue.push_back(candidate);
            }
        }
    }
    paths.into_values().collect()
}

fn geo_cell_distance(
    non_area_anchors: &[&str],
    seed_cell_ids: &[&str],
    cell_id: &str,
    spatial_index: &SpatialServingIndex,
    snapshot_identity: &str,
) -> Option<crate::serving::SpatialDistance> {
    let sources = if non_area_anchors.is_empty() {
        seed_cell_ids
    } else {
        non_area_anchors
    };
    sources
        .iter()
        .filter_map(|source| {
            spatial_index.distance_from_entity_to_area(source, cell_id, snapshot_identity)
        })
        .min_by(|left, right| left.distance_km.total_cmp(&right.distance_km))
}

fn validated_edge_evidence(
    edge: &ServingEdgeRecord,
    snapshot_identity: &str,
) -> Option<Vec<EvidenceRef>> {
    let derivation = edge.derivation.as_ref()?;
    edge.validate_derivation(snapshot_identity).ok()?;
    Some(vec![EvidenceRef::for_derivation(derivation)])
}

fn collect_geo_anchors<'a>(
    expression: &'a ConstraintExpr,
    negated: bool,
    topology: &GeoTopologyIndex,
    explicit_area_ids: &mut BTreeSet<&'a str>,
    anchors: &mut BTreeSet<&'a str>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_geo_anchors(clause, negated, topology, explicit_area_ids, anchors);
            }
        }
        ConstraintExpr::Not { clause } => {
            collect_geo_anchors(clause, !negated, topology, explicit_area_ids, anchors)
        }
        ConstraintExpr::Term { term } if !negated => match term {
            ConstraintTerm::Area {
                entity_id: Some(entity_id),
                ..
            } => {
                explicit_area_ids.insert(entity_id);
            }
            ConstraintTerm::Society { entity_id, .. } => {
                anchors.insert(entity_id);
            }
            ConstraintTerm::Spatial { entity_id, .. } => {
                if entity_id.is_empty() {
                    return;
                }
                if topology
                    .entity_type(entity_id)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("area"))
                {
                    explicit_area_ids.insert(entity_id);
                } else {
                    anchors.insert(entity_id);
                }
            }
            _ => {}
        },
        ConstraintExpr::Term { .. } => {}
    }
}

fn assign_geo_clusters(branches: &mut [GeoBranch], topology: &GeoTopologyIndex) {
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for branch_index in 0..branches.len() {
        let cluster = clusters.iter_mut().find(|members| {
            members.iter().all(|member| {
                directly_connected_scopes(
                    &branches[*member].geo_scope,
                    &branches[branch_index].geo_scope,
                    topology,
                )
            })
        });
        if let Some(cluster) = cluster {
            cluster.push(branch_index);
        } else {
            clusters.push(vec![branch_index]);
        }
    }
    for (cluster_index, members) in clusters.into_iter().enumerate() {
        for member in members {
            branches[member].geo_cluster_id = format!("geo-cluster-{}", cluster_index + 1);
        }
    }
}

fn directly_connected_scopes(
    left: &GeoScope,
    right: &GeoScope,
    topology: &GeoTopologyIndex,
) -> bool {
    if left.is_bundle_wide() || right.is_bundle_wide() {
        return left.is_bundle_wide() && right.is_bundle_wide();
    }
    let (
        GeoScope::Scoped {
            seed_cells: left_seeds,
            ..
        },
        GeoScope::Scoped {
            seed_cells: right_seeds,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };
    left_seeds.iter().any(|left_seed| {
        right_seeds.iter().any(|right_seed| {
            left_seed.cell_id == right_seed.cell_id
                || topology.are_adjacent(&left_seed.cell_id, &right_seed.cell_id)
        })
    })
}

fn extend_unique<T: PartialEq>(target: &mut Vec<T>, values: Vec<T>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn all_source_spans(expression: &ConstraintExpr) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    collect_all_source_spans(expression, &mut spans);
    spans.sort_by_key(|span| (span.start, span.end));
    spans.dedup_by(|left, right| left.start == right.start && left.end == right.end);
    spans
}

fn collect_all_source_spans(expression: &ConstraintExpr, spans: &mut Vec<SourceSpan>) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_all_source_spans(clause, spans);
            }
        }
        ConstraintExpr::Not { clause } => collect_all_source_spans(clause, spans),
        ConstraintExpr::Term { term } => {
            if let Some(span) = term.source_span() {
                spans.push(span.clone());
            }
        }
    }
}

fn positive_geography_spans(expression: &ConstraintExpr) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    collect_positive_geography_spans(expression, false, &mut spans);
    spans
}

fn collect_positive_geography_spans(
    expression: &ConstraintExpr,
    negated: bool,
    spans: &mut Vec<SourceSpan>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_positive_geography_spans(clause, negated, spans);
            }
        }
        ConstraintExpr::Not { clause } => collect_positive_geography_spans(clause, !negated, spans),
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Society {
                    span: Some(span), ..
                }
                | ConstraintTerm::Area {
                    span: Some(span), ..
                }
                | ConstraintTerm::Spatial {
                    span: Some(span), ..
                },
        } if !negated => spans.push(span.clone()),
        ConstraintExpr::Term { .. } => {}
    }
}

fn scoring_query(
    query: &str,
    branch_spans: &[SourceSpan],
    geography_spans: &[SourceSpan],
) -> String {
    branch_spans
        .iter()
        .filter(|span| {
            !geography_spans
                .iter()
                .any(|geography| spans_overlap(span, geography))
        })
        .filter_map(|span| query.get(span.start..span.end))
        .collect::<Vec<_>>()
        .join(" ")
}

fn spans_overlap(left: &SourceSpan, right: &SourceSpan) -> bool {
    left.start < right.end && right.start < left.end
}

fn project_branch_intent(aggregate: &SearchIntent, predicates: &ConstraintExpr) -> SearchIntent {
    let mut intent = aggregate.clone();
    intent.area = None;
    intent.areas.clear();
    intent.bhk = None;
    intent.bhks.clear();
    intent.budget_min = None;
    intent.budget_max = None;
    intent.hard_constraints.clear();
    intent.excluded_areas.clear();
    intent.excluded_societies.clear();
    intent.excluded_builders.clear();
    intent.exclude_bhks.clear();
    collect_branch_intent(predicates, false, &mut intent);
    intent.area = (intent.areas.len() == 1).then(|| intent.areas[0].clone());
    intent.bhk = (intent.bhks.len() == 1).then(|| intent.bhks[0]);
    intent
}

fn collect_branch_intent(expression: &ConstraintExpr, negated: bool, intent: &mut SearchIntent) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_branch_intent(clause, negated, intent);
            }
        }
        ConstraintExpr::Not { clause } => collect_branch_intent(clause, !negated, intent),
        ConstraintExpr::Term { term } => match term {
            ConstraintTerm::Bhk { value, .. } => {
                if negated {
                    push_unique(&mut intent.exclude_bhks, *value);
                } else {
                    push_unique(&mut intent.bhks, *value);
                }
            }
            ConstraintTerm::Area { value, .. } => {
                if negated {
                    push_unique(&mut intent.excluded_areas, value.clone());
                } else {
                    push_unique(&mut intent.areas, value.clone());
                }
            }
            ConstraintTerm::Society { display_name, .. } if negated => {
                push_unique(&mut intent.excluded_societies, display_name.clone());
            }
            ConstraintTerm::Builder { display_name, .. } if negated => {
                push_unique(&mut intent.excluded_builders, display_name.clone());
            }
            ConstraintTerm::Budget { min, max, .. } if !negated => {
                merge_min(&mut intent.budget_min, min.as_ref());
                merge_max(&mut intent.budget_max, max.as_ref());
            }
            ConstraintTerm::Evidence { constraint, .. } if !negated => {
                if !intent.hard_constraints.contains(constraint) {
                    intent.hard_constraints.push(constraint.clone());
                }
            }
            _ => {}
        },
    }
}

fn merge_min(target: &mut Option<u64>, bound: Option<&NumericBound>) {
    if let Some(value) = bound.map(|bound| bound.value) {
        *target = Some(target.map_or(value, |current| current.max(value)));
    }
}

fn merge_max(target: &mut Option<u64>, bound: Option<&NumericBound>) {
    if let Some(value) = bound.map(|bound| bound.value) {
        *target = Some(target.map_or(value, |current| current.min(value)));
    }
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::FactValue;
    use crate::serving::{
        DerivedEvidence, EvidenceRef, ServingFactIndex, ServingFactRecord, SourceObservation,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn geography_first_compiler_contract() {
        let entities = vec![
            entity("area:alpha", "area", "Alpha"),
            entity("area:beta", "area", "Beta"),
            entity("area:gamma", "area", "Gamma"),
            entity("area:delta", "area", "Delta"),
            entity("area:cell:a", "area", "Internal A"),
            entity("area:cell:b", "area", "Internal B"),
            entity("area:cell:c", "area", "Internal C"),
            entity("area:cell:far", "area", "Internal Far"),
            entity("area:cell:disconnected", "area", "Internal Disconnected"),
            entity("society:air", "society", "Air"),
            entity("society:waterford", "society", "Waterford"),
            entity("society:song", "society", "Song"),
            entity("society:multi", "society", "Multi"),
            entity("society:ward-only", "society", "Ward Only"),
            entity("place:metro", "place", "Metro"),
        ];
        let facts = vec![
            point_fact("society:air", "geo.latitude", 12.905),
            point_fact("society:air", "geo.longitude", 77.005),
            point_fact("society:waterford", "geo.latitude", 12.905),
            point_fact("society:waterford", "geo.longitude", 77.015),
            point_fact("society:song", "geo.latitude", 12.905),
            point_fact("society:song", "geo.longitude", 77.025),
            point_fact("society:multi", "geo.latitude", 12.905),
            point_fact("society:multi", "geo.longitude", 77.010),
            point_fact("place:metro", "geo.latitude", 12.905),
            point_fact("place:metro", "geo.longitude", 77.006),
            geometry_fact("area:cell:a", 77.00, 77.01),
            geometry_fact("area:cell:b", 77.01, 77.02),
            geometry_fact("area:cell:c", 77.02, 77.03),
            geometry_fact("area:cell:far", 77.10, 77.11),
            geometry_fact("area:cell:disconnected", 78.00, 78.01),
        ];
        let edges = vec![
            edge("society:air", "in_market_locality", "area:alpha", &facts[0]),
            edge(
                "society:waterford",
                "in_market_locality",
                "area:beta",
                &facts[2],
            ),
            edge(
                "society:song",
                "in_market_locality",
                "area:gamma",
                &facts[4],
            ),
            edge("place:metro", "in_market_locality", "area:alpha", &facts[8]),
            edge("society:air", "occupies_geo_cell", "area:cell:a", &facts[0]),
            edge(
                "society:waterford",
                "occupies_geo_cell",
                "area:cell:b",
                &facts[2],
            ),
            edge(
                "society:song",
                "occupies_geo_cell",
                "area:cell:c",
                &facts[4],
            ),
            edge(
                "society:multi",
                "occupies_geo_cell",
                "area:cell:a",
                &facts[6],
            ),
            edge(
                "society:multi",
                "occupies_geo_cell",
                "area:cell:b",
                &facts[6],
            ),
            edge("place:metro", "occupies_geo_cell", "area:cell:a", &facts[8]),
            edge("area:alpha", "covers_geo_cell", "area:cell:a", &facts[10]),
            edge("area:beta", "covers_geo_cell", "area:cell:b", &facts[11]),
            edge("area:gamma", "covers_geo_cell", "area:cell:c", &facts[12]),
            edge(
                "area:delta",
                "covers_geo_cell",
                "area:cell:disconnected",
                &facts[14],
            ),
            edge("area:cell:a", "adjacent_area", "area:cell:b", &facts[10]),
            edge("area:cell:b", "adjacent_area", "area:cell:c", &facts[11]),
            edge("area:cell:b", "adjacent_area", "area:cell:far", &facts[11]),
        ];

        let cases = [
            (
                "direct area",
                term(ConstraintTerm::Area {
                    entity_id: Some("area:alpha".to_string()),
                    value: "Alpha".to_string(),
                    span: Some(span(0, 5, "Alpha")),
                }),
                vec!["area:alpha"],
                false,
            ),
            (
                "sourced society market locality",
                society_term("society:air", "Air", 0),
                vec!["area:alpha"],
                false,
            ),
            (
                "place keeps its exact predicate",
                term(ConstraintTerm::Spatial {
                    relation: "near".to_string(),
                    entity_id: "place:metro".to_string(),
                    display_name: "Metro".to_string(),
                    required: true,
                    span: Some(span(0, 5, "Metro")),
                }),
                vec!["area:alpha"],
                false,
            ),
            (
                "dangling society falls back bundle wide",
                society_term("society:missing", "Missing", 0),
                Vec::new(),
                true,
            ),
            (
                "administrative containment does not scope search",
                society_term("society:ward-only", "Ward Only", 0),
                Vec::new(),
                true,
            ),
            (
                "no geography falls back bundle wide",
                bhk_term(2, 0),
                Vec::new(),
                true,
            ),
        ];
        for (label, predicates, expected_markets, bundle_wide) in cases {
            let plan = compile(vec![predicates], &entities, &facts, &edges);
            assert_eq!(
                plan.branches[0].geo_scope.is_bundle_wide(),
                bundle_wide,
                "{label}"
            );
            assert_eq!(
                plan.branches[0].geo_scope.market_locality_ids(),
                expected_markets,
                "{label}"
            );
        }

        let air = compile(
            vec![society_term("society:air", "Air", 0)],
            &entities,
            &facts,
            &edges,
        );
        let scope = &air.branches[0].geo_scope;
        assert_eq!(scope.cell_path("area:cell:a").unwrap().hops, 0, "same cell");
        assert_eq!(
            scope
                .cell_path("area:cell:b")
                .unwrap_or_else(|| panic!("adjacent cell: {scope:?}"))
                .hops,
            1,
            "adjacent cell"
        );
        assert_eq!(
            scope.cell_path("area:cell:c").unwrap().hops,
            2,
            "bounded two-hop"
        );
        assert!(scope.cell_path("area:cell:far").is_none(), "over-distance");
        assert!(
            scope.cell_path("area:cell:disconnected").is_none(),
            "disconnected"
        );

        let multi = compile(
            vec![society_term("society:multi", "Multi", 0)],
            &entities,
            &facts,
            &edges,
        );
        let GeoScope::Scoped { seed_cells, .. } = &multi.branches[0].geo_scope else {
            panic!("multi-cell anchor must remain scoped");
        };
        assert_eq!(
            seed_cells.len(),
            2,
            "multi-cell footprints retain every seed"
        );
        assert_eq!(
            multi.branches[0]
                .geo_scope
                .cell_path("area:cell:c")
                .unwrap()
                .hops,
            1
        );

        let branches = vec![
            ConstraintExpr::and(vec![society_term("society:air", "Air", 0), bhk_term(2, 4)]),
            ConstraintExpr::and(vec![
                society_term("society:waterford", "Waterford", 10),
                bhk_term(3, 20),
            ]),
            ConstraintExpr::and(vec![
                society_term("society:song", "Song", 30),
                bhk_term(4, 35),
            ]),
        ];
        let plan = compile(branches, &entities, &facts, &edges);
        assert_eq!(plan.branches.len(), 3);
        assert_eq!(
            plan.branches[0].geo_cluster_id,
            plan.branches[1].geo_cluster_id
        );
        assert_ne!(
            plan.branches[0].geo_cluster_id,
            plan.branches[2].geo_cluster_id
        );
        assert_eq!(plan.branches[0].constraints.bhks, [2]);
        assert_eq!(plan.branches[1].constraints.bhks, [3]);
        assert_eq!(plan.branches[2].constraints.bhks, [4]);
        assert!(matches!(
            &plan.branches[0].predicates,
            ConstraintExpr::And { clauses }
                if clauses.iter().any(|clause| matches!(
                    clause,
                    ConstraintExpr::Term {
                        term: ConstraintTerm::Society { entity_id, .. }
                    } if entity_id == "society:air"
                )) && clauses.iter().any(|clause| matches!(
                    clause,
                    ConstraintExpr::Term {
                        term: ConstraintTerm::Bhk { value: 2, .. }
                    }
                ))
        ));
        assert!(matches!(
            &plan.branches[0].compiled_query.constraints,
            ConstraintExpr::Term {
                term: ConstraintTerm::Bhk { value: 2, .. }
            }
        ));
        assert!(matches!(
            &compile(
                vec![term(ConstraintTerm::Spatial {
                    relation: "near".to_string(),
                    entity_id: "place:metro".to_string(),
                    display_name: "Metro".to_string(),
                    required: true,
                    span: Some(span(0, 5, "Metro")),
                })],
                &entities,
                &facts,
                &edges,
            ).branches[0].predicates,
            ConstraintExpr::Term { term: ConstraintTerm::Spatial { entity_id, .. } }
                if entity_id == "place:metro"
        ));
    }

    fn compile(
        branches: Vec<ConstraintExpr>,
        entities: &[ServingEntityRecord],
        facts: &[ServingFactRecord],
        edges: &[ServingEdgeRecord],
    ) -> CompiledSearchPlan {
        let constraints = ConstraintExpr::any_of(branches.clone());
        let topology = GeoTopologyIndex::build(entities, edges, "test-snapshot");
        CompiledSearchPlan::compile_for_snapshot(
            CompiledQuery {
                raw: "Air 2BHK Waterford 3BHK Song 4BHK".to_string(),
                constraints,
                branches,
                intent: SearchIntent::default(),
            },
            "test-snapshot",
            &[],
            &topology,
            Some(&SpatialServingIndex::from_serving_bundle_with_edges(
                entities,
                &ServingFactIndex::from_records(facts.to_vec(), Vec::new()),
                edges,
            )),
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
    }

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

    fn edge(
        from: &str,
        relation: &str,
        to: &str,
        evidence_fact: &ServingFactRecord,
    ) -> ServingEdgeRecord {
        let evidence = EvidenceRef::for_observation(
            "test-snapshot",
            evidence_fact.observation.as_ref().unwrap(),
        );
        let derivation = DerivedEvidence::new(
            "test-snapshot",
            from,
            Some(to.to_string()),
            relation,
            "controlled_topology",
            Some(1.0),
            Some("boolean".to_string()),
            "compiler-contract-v1",
            0.9,
            vec![evidence],
        )
        .unwrap();
        ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence: 0.9,
            source_type: "test".to_string(),
            derivation: Some(derivation),
        }
    }

    fn point_fact(entity_id: &str, fact_key: &str, value: f64) -> ServingFactRecord {
        let mut fact = fact(entity_id, fact_key, FactValue::Numeric(value));
        fact.source_type = "Google".to_string();
        fact.observation = Some(
            SourceObservation::new(
                "Google",
                format!("{entity_id}:coordinates"),
                entity_id,
                fact.learned_at,
                fact.source_url.clone(),
                vec!["asset:compiler-contract/v1".to_string()],
            )
            .unwrap(),
        );
        fact
    }

    fn geometry_fact(entity_id: &str, min_lon: f64, max_lon: f64) -> ServingFactRecord {
        fact(
            entity_id,
            "geo.geometry_geojson",
            FactValue::Text(format!(
                "{{\"type\":\"Polygon\",\"coordinates\":[[[{min_lon},12.9],[{max_lon},12.9],[{max_lon},12.91],[{min_lon},12.91],[{min_lon},12.9]]]}}"
            )),
        )
    }

    fn fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
        let learned_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: "OpenStreetMap".to_string(),
            source_url: Some("https://example.test/topology".to_string()),
            model: None,
            skill_id: Some("compiler-contract".to_string()),
            learned_at,
            observation: Some(
                SourceObservation::new(
                    "OpenStreetMap",
                    if fact_key.starts_with("geo.lat") || fact_key.starts_with("geo.lon") {
                        format!("{entity_id}:coordinates")
                    } else {
                        format!("{entity_id}:{fact_key}")
                    },
                    entity_id,
                    learned_at,
                    Some("https://example.test/topology".to_string()),
                    vec!["asset:compiler-contract/v1".to_string()],
                )
                .unwrap(),
            ),
        }
    }

    fn term(term: ConstraintTerm) -> ConstraintExpr {
        ConstraintExpr::term(term)
    }

    fn society_term(id: &str, name: &str, start: usize) -> ConstraintExpr {
        term(ConstraintTerm::Society {
            entity_id: id.to_string(),
            display_name: name.to_string(),
            span: Some(span(start, start + name.len(), name)),
        })
    }

    fn bhk_term(value: u32, start: usize) -> ConstraintExpr {
        term(ConstraintTerm::Bhk {
            value,
            span: Some(span(start, start + 4, &format!("{value}BHK"))),
        })
    }

    fn span(start: usize, end: usize, raw_text: &str) -> SourceSpan {
        SourceSpan {
            start,
            end,
            raw_text: raw_text.to_string(),
        }
    }
}
