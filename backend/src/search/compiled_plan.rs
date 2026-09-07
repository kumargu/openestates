use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::serving::{EvidenceRef, ServingEdgeRecord, ServingEntityRecord, SpatialServingIndex};

use super::ast::{
    semantic_search_fingerprint, CompiledQuery, ConstraintExpr, ConstraintTerm, NumericBound,
};
use super::intent::{SearchIntent, SourceSpan};

pub type BranchId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocietyNeighborhoodMember {
    pub society_entity_id: String,
    pub distance_km: f64,
    pub metric: String,
    pub evidence_refs: Vec<EvidenceRef>,
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
    Areas {
        area_ids: Vec<String>,
        supporting_market_locality_edges: Vec<ServingEdgeRecord>,
    },
    SocietyNeighborhood {
        anchor_society_ids: Vec<String>,
        members: Vec<SocietyNeighborhoodMember>,
        radius_km: f64,
    },
    BundleWide,
}

impl GeoScope {
    pub fn area_ids(&self) -> &[String] {
        match self {
            Self::Areas { area_ids, .. } => area_ids,
            Self::SocietyNeighborhood { .. } | Self::BundleWide => &[],
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
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
        spatial_index: Option<&SpatialServingIndex>,
        society_neighborhood_radius_km: Option<f64>,
    ) -> Self {
        let snapshot_identity = snapshot_identity.into();
        let entity_types = entities
            .iter()
            .map(|entity| (entity.entity_id.as_str(), entity.entity_type.as_str()))
            .collect::<HashMap<_, _>>();
        let mut resolution_gaps = Vec::new();
        let mut branches = compiled_query
            .branches
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, mut predicates)| {
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
                    &entity_types,
                    edges,
                    spatial_index,
                    society_neighborhood_radius_km,
                    &snapshot_identity,
                    &mut resolution_gaps,
                );

                // A named society is a geographic anchor. Its sourced area
                // owns recall and the society itself is not an eligibility
                // predicate. Ranking may still prioritize an eligible exact
                // society match through the config-owned search policy.
                let geography_spans = positive_geography_spans(&predicates);
                predicates.drop_society_includes();
                let mut branch_query = compiled_query.for_branch(predicates.clone());
                branch_query.raw =
                    scoring_query(&branch_query.raw, &source_spans, &geography_spans);
                let constraints = project_branch_intent(&compiled_query.intent, &predicates);
                branch_query.intent = constraints.clone();

                GeoBranch {
                    branch_id: format!("branch-{}", index + 1),
                    geo_cluster_id: String::new(),
                    source_spans,
                    geo_scope,
                    buyer_summary: predicates.buyer_label(),
                    source_query: compiled_query.raw.clone(),
                    predicates,
                    resolved_entities: branch_entities,
                    constraints,
                    compiled_query: branch_query,
                }
            })
            .collect::<Vec<_>>();
        assign_geo_clusters(&mut branches, edges);
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
    entity_types: &HashMap<&str, &str>,
    edges: &[ServingEdgeRecord],
    spatial_index: Option<&SpatialServingIndex>,
    society_neighborhood_radius_km: Option<f64>,
    snapshot_identity: &str,
    resolution_gaps: &mut Vec<String>,
) -> GeoScope {
    let mut explicit_area_ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    collect_geo_anchors(
        predicates,
        false,
        entity_types,
        &mut explicit_area_ids,
        &mut anchors,
    );
    if !explicit_area_ids.is_empty() {
        let market_area_ids = edges
            .iter()
            .filter(|edge| edge.edge_type.eq_ignore_ascii_case("in_market_locality"))
            .map(|edge| edge.to_entity_id.as_str())
            .collect::<BTreeSet<_>>();
        explicit_area_ids.retain(|area_id| market_area_ids.contains(area_id));
        if !explicit_area_ids.is_empty() {
            return GeoScope::Areas {
                area_ids: explicit_area_ids.into_iter().map(str::to_string).collect(),
                supporting_market_locality_edges: Vec::new(),
            };
        }
    }

    let supporting = edges
        .iter()
        .filter(|edge| {
            edge.edge_type.eq_ignore_ascii_case("in_market_locality")
                && anchors.contains(edge.from_entity_id.as_str())
                && entity_types
                    .get(edge.to_entity_id.as_str())
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("area"))
        })
        .cloned()
        .collect::<Vec<_>>();
    let area_ids = supporting
        .iter()
        .map(|edge| edge.to_entity_id.clone())
        .collect::<BTreeSet<_>>();
    if area_ids.is_empty() {
        let society_anchors = anchors
            .iter()
            .filter(|anchor| {
                entity_types
                    .get(*anchor)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("society"))
            })
            .copied()
            .collect::<Vec<_>>();
        if let (Some(spatial_index), Some(radius_km)) =
            (spatial_index, society_neighborhood_radius_km)
        {
            let mut members = HashMap::<String, SocietyNeighborhoodMember>::new();
            for anchor in &society_anchors {
                for candidate in spatial_index
                    .points()
                    .iter()
                    .filter(|point| point.entity_type.eq_ignore_ascii_case("society"))
                {
                    let Some(distance) = spatial_index
                        .distance_between(anchor, &candidate.entity_id, snapshot_identity)
                        .filter(|distance| distance.distance_km <= radius_km)
                    else {
                        continue;
                    };
                    let member = SocietyNeighborhoodMember {
                        society_entity_id: candidate.entity_id.clone(),
                        distance_km: distance.distance_km,
                        metric: distance.metric.to_string(),
                        evidence_refs: distance.evidence_refs,
                    };
                    let replace = members
                        .get(&member.society_entity_id)
                        .is_none_or(|current| {
                            member.distance_km < current.distance_km
                                || (member.distance_km == current.distance_km
                                    && member.society_entity_id < current.society_entity_id)
                        });
                    if replace {
                        members.insert(member.society_entity_id.clone(), member);
                    }
                }
            }
            if !members.is_empty() {
                let mut members = members.into_values().collect::<Vec<_>>();
                members.sort_by(|left, right| {
                    left.distance_km
                        .total_cmp(&right.distance_km)
                        .then_with(|| left.society_entity_id.cmp(&right.society_entity_id))
                });
                return GeoScope::SocietyNeighborhood {
                    anchor_society_ids: society_anchors.into_iter().map(str::to_string).collect(),
                    members,
                    radius_km,
                };
            }
        }
        for anchor in anchors {
            let gap = format!("missing sourced in_market_locality relation for {anchor}");
            if !resolution_gaps.contains(&gap) {
                resolution_gaps.push(gap);
            }
        }
        GeoScope::BundleWide
    } else {
        GeoScope::Areas {
            area_ids: area_ids.into_iter().collect(),
            supporting_market_locality_edges: supporting,
        }
    }
}

fn collect_geo_anchors<'a>(
    expression: &'a ConstraintExpr,
    negated: bool,
    entity_types: &HashMap<&str, &str>,
    explicit_area_ids: &mut BTreeSet<&'a str>,
    anchors: &mut BTreeSet<&'a str>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_geo_anchors(clause, negated, entity_types, explicit_area_ids, anchors);
            }
        }
        ConstraintExpr::Not { clause } => {
            collect_geo_anchors(clause, !negated, entity_types, explicit_area_ids, anchors)
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
                if entity_types
                    .get(entity_id.as_str())
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

fn assign_geo_clusters(branches: &mut [GeoBranch], edges: &[ServingEdgeRecord]) {
    let adjacency = edges
        .iter()
        .filter(|edge| {
            edge.edge_type
                .eq_ignore_ascii_case("adjacent_market_locality")
        })
        .map(|edge| (edge.from_entity_id.as_str(), edge.to_entity_id.as_str()))
        .collect::<BTreeSet<_>>();
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for branch_index in 0..branches.len() {
        let cluster = clusters.iter_mut().find(|members| {
            members.iter().all(|member| {
                directly_connected_scopes(
                    &branches[*member].geo_scope,
                    &branches[branch_index].geo_scope,
                    &adjacency,
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
    adjacency: &BTreeSet<(&str, &str)>,
) -> bool {
    if left.is_bundle_wide() || right.is_bundle_wide() {
        return left.is_bundle_wide() && right.is_bundle_wide();
    }
    match (left, right) {
        (
            GeoScope::Areas { area_ids: left, .. },
            GeoScope::Areas {
                area_ids: right, ..
            },
        ) => left.iter().any(|left_id| {
            right.iter().any(|right_id| {
                left_id == right_id
                    || adjacency.contains(&(left_id.as_str(), right_id.as_str()))
                    || adjacency.contains(&(right_id.as_str(), left_id.as_str()))
            })
        }),
        (
            GeoScope::SocietyNeighborhood { members: left, .. },
            GeoScope::SocietyNeighborhood { members: right, .. },
        ) => left.iter().any(|left| {
            right
                .iter()
                .any(|right| left.society_entity_id == right.society_entity_id)
        }),
        _ => false,
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

    #[test]
    fn geography_first_compiler_contract() {
        let entities = [
            entity("area:alpha", "area", "Alpha"),
            entity("area:beta", "area", "Beta"),
            entity("area:gamma", "area", "Gamma"),
            entity("area:parent", "area", "Parent"),
            entity("society:air", "society", "Air"),
            entity("society:waterford", "society", "Waterford"),
            entity("society:song", "society", "Song"),
            entity("society:ward-only", "society", "Ward Only"),
            entity("place:metro", "place", "Metro"),
        ];
        let edges = vec![
            edge("society:air", "in_market_locality", "area:alpha"),
            edge("society:waterford", "in_market_locality", "area:beta"),
            edge("society:song", "in_market_locality", "area:gamma"),
            edge("place:metro", "in_market_locality", "area:alpha"),
            edge("society:air", "in_area", "area:parent"),
            edge("society:ward-only", "in_area", "area:alpha"),
            edge("area:alpha", "in_area", "area:parent"),
            edge("area:beta", "in_area", "area:parent"),
            edge("area:alpha", "adjacent_market_locality", "area:beta"),
            edge("area:beta", "adjacent_market_locality", "area:gamma"),
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
        for (label, predicates, expected_areas, bundle_wide) in cases {
            let plan = compile(vec![predicates], &entities, &edges);
            assert_eq!(
                plan.branches[0].geo_scope.is_bundle_wide(),
                bundle_wide,
                "{label}"
            );
            assert_eq!(
                plan.branches[0].geo_scope.area_ids(),
                expected_areas,
                "{label}"
            );
        }

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
        let plan = compile(branches, &entities, &edges);
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
        assert!(plan.branches[0]
            .geo_scope
            .area_ids()
            .iter()
            .all(|area| area != "area:parent"));
        assert!(matches!(
            &plan.branches[0].predicates,
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
                &edges,
            ).branches[0].predicates,
            ConstraintExpr::Term { term: ConstraintTerm::Spatial { entity_id, .. } }
                if entity_id == "place:metro"
        ));
    }

    fn compile(
        branches: Vec<ConstraintExpr>,
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
    ) -> CompiledSearchPlan {
        let constraints = ConstraintExpr::any_of(branches.clone());
        CompiledSearchPlan::compile_for_snapshot(
            CompiledQuery {
                raw: "Air 2BHK Waterford 3BHK Song 4BHK".to_string(),
                constraints,
                branches,
                intent: SearchIntent::default(),
            },
            "test-snapshot",
            &[],
            entities,
            edges,
            None,
            None,
        )
    }

    fn entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: name.to_string(),
        }
    }

    fn edge(from: &str, relation: &str, to: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence: 0.9,
            source_type: "test".to_string(),
            derivation: None,
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
