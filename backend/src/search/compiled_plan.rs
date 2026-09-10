use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::graph::GraphIndex;
use crate::serving::{EvidenceRef, SpatialServingIndex};

use super::ast::{
    semantic_search_fingerprint, ConstraintExpr, ConstraintTerm, IntentAst, NumericBound,
};
use super::intent::{SearchIntent, SourceSpan};

pub type BranchId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledPredicateBinding {
    pub predicate_id: String,
    pub path: Vec<usize>,
    pub family: super::ast::PredicateFamily,
    pub polarity: super::ast::PredicatePolarity,
    pub semantic_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_span: Option<SourceSpan>,
}

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
    Unresolved {
        anchors: Vec<GeoAnchor>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        requested: Vec<String>,
        reason: GeoScopeResolution,
    },
    BundleWide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoScopeResolution {
    MissingTopology,
    MissingSpatialIndex,
    UnresolvedExplicitGeography,
}

impl GeoScope {
    pub fn market_locality_ids(&self) -> &[String] {
        match self {
            Self::Scoped {
                market_locality_ids,
                ..
            } => market_locality_ids,
            Self::Unresolved { .. } => &[],
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
            Self::Unresolved { .. } => None,
            Self::BundleWide => None,
        }
    }

    pub fn is_bundle_wide(&self) -> bool {
        matches!(self, Self::BundleWide)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoBranch {
    pub branch_id: BranchId,
    pub geo_cluster_id: String,
    pub source_spans: Vec<SourceSpan>,
    pub geo_scope: GeoScope,
    pub predicates: ConstraintExpr,
    pub eligibility_predicates: ConstraintExpr,
    pub spatial_predicates: Vec<ConstraintTerm>,
    #[serde(default)]
    pub predicate_bindings: Vec<CompiledPredicateBinding>,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
    pub ranking_intent: SearchIntent,
    pub buyer_summary: String,
    pub recall_query: String,
    pub scoring_query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_text: Option<String>,
}

impl GeoBranch {
    pub(crate) fn restore_portable_ranking_intent(&mut self, ranking_intent: SearchIntent) {
        self.scoring_query = canonical_scoring_query(
            &self.eligibility_predicates,
            &ranking_intent,
            &self.resolved_entities,
        );
        self.recall_query = canonical_recall_query(
            &self.predicates,
            &ranking_intent,
            &self.resolved_entities,
            &self.scoring_query,
        );
        self.ranking_intent = ranking_intent;
        self.fallback_text = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSearchPlan {
    pub root: BoolExpr<BranchId>,
    pub branches: Vec<GeoBranch>,
    pub resolution_gaps: Vec<String>,
    pub aggregate_intent: SearchIntent,
    pub semantic_fingerprint: String,
    pub snapshot_identity: String,
}

impl CompiledSearchPlan {
    pub fn assign_source_turn(&mut self, source_turn_id: &str) {
        self.aggregate_intent
            .bhk_spans
            .iter_mut()
            .for_each(|span| assign_span_turn(span, source_turn_id));
        for branch in &mut self.branches {
            branch
                .source_spans
                .iter_mut()
                .for_each(|span| assign_span_turn(span, source_turn_id));
            branch
                .resolved_entities
                .iter_mut()
                .filter_map(|entity| entity.source_span.as_mut())
                .for_each(|span| assign_span_turn(span, source_turn_id));
            assign_expression_turn(&mut branch.predicates, source_turn_id);
            assign_expression_turn(&mut branch.eligibility_predicates, source_turn_id);
            branch.spatial_predicates = compiled_spatial_predicates(&branch.predicates);
            branch
                .ranking_intent
                .bhk_spans
                .iter_mut()
                .for_each(|span| assign_span_turn(span, source_turn_id));
        }
        refresh_predicate_bindings(&mut self.branches, None);
    }

    pub fn shift_source_spans(&mut self, offset: usize) {
        self.aggregate_intent
            .bhk_spans
            .iter_mut()
            .for_each(|span| shift_span(span, offset));
        for branch in &mut self.branches {
            branch
                .source_spans
                .iter_mut()
                .for_each(|span| shift_span(span, offset));
            branch
                .resolved_entities
                .iter_mut()
                .filter_map(|entity| entity.source_span.as_mut())
                .for_each(|span| shift_span(span, offset));
            shift_expression_spans(&mut branch.predicates, offset);
            shift_expression_spans(&mut branch.eligibility_predicates, offset);
            branch.spatial_predicates = compiled_spatial_predicates(&branch.predicates);
            branch
                .ranking_intent
                .bhk_spans
                .iter_mut()
                .for_each(|span| shift_span(span, offset));
        }
        refresh_predicate_bindings(&mut self.branches, None);
    }
    pub fn compile_for_snapshot(
        compiled_query: IntentAst,
        snapshot_identity: impl Into<String>,
        resolved_entities: &[ResolvedEntityHandle],
        topology: &GraphIndex,
        spatial_index: Option<&SpatialServingIndex>,
        geo_cell_policy: GeoCellSearchPolicy,
    ) -> Self {
        let snapshot_identity = snapshot_identity.into();
        let mut resolution_gaps = Vec::new();
        let spatial_keys_by_branch = compiled_query
            .branches
            .iter()
            .map(spatial_preference_keys)
            .collect::<Vec<_>>();
        let all_spatial_keys = spatial_keys_by_branch
            .iter()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
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
                let mut eligibility_predicates = predicates.clone();
                eligibility_predicates.drop_society_includes();
                eligibility_predicates.drop_area_includes();
                eligibility_predicates.drop_optional_spatial_includes();
                let owned_spatial_keys = &spatial_keys_by_branch[index];
                let external_spatial_keys = all_spatial_keys
                    .difference(owned_spatial_keys)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let constraints = project_branch_intent(
                    &compiled_query.intent,
                    &eligibility_predicates,
                    owned_spatial_keys,
                    &external_spatial_keys,
                );
                let scoring_query = canonical_scoring_query(
                    &eligibility_predicates,
                    &constraints,
                    &branch_entities,
                );
                let recall_query = canonical_recall_query(
                    &predicates,
                    &constraints,
                    &branch_entities,
                    &scoring_query,
                );
                let fallback_text = (recall_query.is_empty()
                    && !compiled_query.raw.trim().is_empty())
                .then(|| normalize_fallback_text(&compiled_query.raw));
                let spatial_predicates = compiled_spatial_predicates(&predicates);
                GeoBranch {
                    branch_id: format!("branch-{}", index + 1),
                    geo_cluster_id: String::new(),
                    source_spans,
                    geo_scope,
                    buyer_summary: eligibility_predicates.buyer_label(),
                    predicates,
                    eligibility_predicates,
                    spatial_predicates,
                    predicate_bindings: Vec::new(),
                    resolved_entities: branch_entities,
                    ranking_intent: constraints,
                    recall_query: if recall_query.is_empty() {
                        fallback_text.clone().unwrap_or_default()
                    } else {
                        recall_query
                    },
                    scoring_query,
                    fallback_text,
                }
            })
            .collect::<Vec<_>>();
        assign_geo_clusters(&mut branches, topology);
        refresh_predicate_bindings(&mut branches, None);
        let root = root_for(&branches);
        let predicates = branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let constraints = branches
            .iter()
            .map(|branch| branch.ranking_intent.clone())
            .collect::<Vec<_>>();
        let semantic_fingerprint = semantic_plan_fingerprint(&predicates, &constraints, &branches);
        Self {
            root,
            branches,
            resolution_gaps,
            aggregate_intent: compiled_query.intent,
            semantic_fingerprint,
            snapshot_identity,
        }
    }

    pub fn recompile_for_snapshot(
        &self,
        branch_predicates: Vec<(BranchId, ConstraintExpr)>,
        aggregate_intent: SearchIntent,
        resolved_entities: Vec<ResolvedEntityHandle>,
        topology: &GraphIndex,
        spatial_index: Option<&SpatialServingIndex>,
        geo_cell_policy: GeoCellSearchPolicy,
    ) -> Self {
        let compiled_query = IntentAst {
            raw: String::new(),
            constraints: ConstraintExpr::any_of(
                branch_predicates
                    .iter()
                    .map(|(_, predicates)| predicates.clone())
                    .collect(),
            ),
            branches: branch_predicates
                .iter()
                .map(|(_, predicates)| predicates.clone())
                .collect(),
            intent: aggregate_intent,
        };
        let mut recompiled = Self::compile_for_snapshot(
            compiled_query,
            self.snapshot_identity.clone(),
            &resolved_entities,
            topology,
            spatial_index,
            geo_cell_policy,
        );
        for (branch, (branch_id, _)) in recompiled.branches.iter_mut().zip(&branch_predicates) {
            branch.branch_id.clone_from(branch_id);
        }
        recompiled.root = root_for(&recompiled.branches);
        refresh_predicate_bindings(&mut recompiled.branches, Some(&self.branches));
        for branch in &mut recompiled.branches {
            if let Some(previous) = self
                .branches
                .iter()
                .find(|previous| previous.branch_id == branch.branch_id)
            {
                branch.fallback_text.clone_from(&previous.fallback_text);
                if branch.recall_query.is_empty() {
                    branch.recall_query.clone_from(&previous.recall_query);
                }
            }
        }
        recompiled.refresh_semantic_fingerprint();
        recompiled
    }

    pub fn refresh_semantic_fingerprint(&mut self) {
        let predicates = self
            .branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let constraints = self
            .branches
            .iter()
            .map(|branch| branch.ranking_intent.clone())
            .collect::<Vec<_>>();
        self.semantic_fingerprint =
            semantic_plan_fingerprint(&predicates, &constraints, &self.branches);
    }
}

fn compiled_spatial_predicates(expression: &ConstraintExpr) -> Vec<ConstraintTerm> {
    let mut predicates = Vec::new();
    collect_spatial_predicates(expression, &mut predicates);
    predicates
}

fn collect_spatial_predicates(expression: &ConstraintExpr, predicates: &mut Vec<ConstraintTerm>) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_spatial_predicates(clause, predicates);
            }
        }
        ConstraintExpr::Not { clause } => collect_spatial_predicates(clause, predicates),
        ConstraintExpr::Term {
            term: term @ ConstraintTerm::Spatial { .. },
        } => predicates.push(term.clone()),
        ConstraintExpr::Term { .. } => {}
    }
}

fn refresh_predicate_bindings(branches: &mut [GeoBranch], prior: Option<&[GeoBranch]>) {
    for branch in branches {
        let mut bindings = Vec::new();
        collect_predicate_bindings(
            &branch.predicates,
            false,
            &mut Vec::new(),
            &branch.branch_id,
            &mut bindings,
        );
        if let Some(previous) = prior.and_then(|branches| {
            branches
                .iter()
                .find(|previous| previous.branch_id == branch.branch_id)
        }) {
            for binding in &mut bindings {
                if let Some(existing) = previous.predicate_bindings.iter().find(|existing| {
                    existing.family == binding.family
                        && existing.polarity == binding.polarity
                        && (existing.semantic_key == binding.semantic_key
                            || (existing.source_span.is_some()
                                && existing.source_span == binding.source_span)
                            || existing.path == binding.path)
                }) {
                    binding.predicate_id.clone_from(&existing.predicate_id);
                }
            }
        }
        branch.predicate_bindings = bindings;
    }
}

fn collect_predicate_bindings(
    expression: &ConstraintExpr,
    negated: bool,
    path: &mut Vec<usize>,
    branch_id: &str,
    bindings: &mut Vec<CompiledPredicateBinding>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for (index, clause) in clauses.iter().enumerate() {
                path.push(index);
                collect_predicate_bindings(clause, negated, path, branch_id, bindings);
                path.pop();
            }
        }
        ConstraintExpr::Not { clause } => {
            path.push(0);
            collect_predicate_bindings(clause, !negated, path, branch_id, bindings);
            path.pop();
        }
        ConstraintExpr::Term { term } => {
            let source_span = term.source_span().cloned();
            let turn_id = source_span
                .as_ref()
                .filter(|span| !span.source_turn_id.is_empty())
                .map(|span| span.source_turn_id.as_str())
                .unwrap_or("root");
            bindings.push(CompiledPredicateBinding {
                predicate_id: format!(
                    "predicate:{turn_id}:{branch_id}:{}",
                    path.iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(".")
                ),
                path: path.clone(),
                family: term.predicate_family(),
                polarity: if negated {
                    super::ast::PredicatePolarity::Negated
                } else {
                    super::ast::PredicatePolarity::Positive
                },
                semantic_key: predicate_semantic_key(term),
                source_span,
            });
        }
    }
}

fn predicate_semantic_key(term: &super::ast::ConstraintTerm) -> String {
    let mut value = serde_json::to_value(term).expect("constraint terms serialize");
    if let serde_json::Value::Object(fields) = &mut value {
        fields.remove("span");
    }
    serde_json::to_string(&value).expect("constraint term identity serializes")
}

fn assign_expression_turn(expression: &mut ConstraintExpr, source_turn_id: &str) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter_mut()
            .for_each(|clause| assign_expression_turn(clause, source_turn_id)),
        ConstraintExpr::Not { clause } => assign_expression_turn(clause, source_turn_id),
        ConstraintExpr::Term { term } => {
            if let Some(span) = match term {
                ConstraintTerm::Bhk { span, .. }
                | ConstraintTerm::Area { span, .. }
                | ConstraintTerm::Society { span, .. }
                | ConstraintTerm::Builder { span, .. }
                | ConstraintTerm::Budget { span, .. }
                | ConstraintTerm::Evidence { span, .. }
                | ConstraintTerm::Spatial { span, .. } => span.as_mut(),
            } {
                assign_span_turn(span, source_turn_id);
            }
        }
    }
}

fn assign_span_turn(span: &mut SourceSpan, source_turn_id: &str) {
    if span.source_turn_id.is_empty() {
        span.source_turn_id = source_turn_id.to_string();
    }
}

fn shift_expression_spans(expression: &mut ConstraintExpr, offset: usize) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter_mut()
            .for_each(|clause| shift_expression_spans(clause, offset)),
        ConstraintExpr::Not { clause } => shift_expression_spans(clause, offset),
        ConstraintExpr::Term { term } => {
            if let Some(span) = match term {
                ConstraintTerm::Bhk { span, .. }
                | ConstraintTerm::Area { span, .. }
                | ConstraintTerm::Society { span, .. }
                | ConstraintTerm::Builder { span, .. }
                | ConstraintTerm::Budget { span, .. }
                | ConstraintTerm::Evidence { span, .. }
                | ConstraintTerm::Spatial { span, .. } => span.as_mut(),
            } {
                shift_span(span, offset);
            }
        }
    }
}

fn shift_span(span: &mut SourceSpan, offset: usize) {
    span.start += offset;
    span.end += offset;
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

fn compile_geo_scope(
    predicates: &ConstraintExpr,
    topology: &GraphIndex,
    spatial_index: Option<&SpatialServingIndex>,
    policy: GeoCellSearchPolicy,
    snapshot_identity: &str,
    resolution_gaps: &mut Vec<String>,
) -> GeoScope {
    let mut explicit_area_ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    collect_geo_anchors(predicates, false, &mut explicit_area_ids, &mut anchors);
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
        return if has_positive_explicit_geography(predicates, false) {
            GeoScope::Unresolved {
                anchors: Vec::new(),
                requested: Vec::new(),
                reason: GeoScopeResolution::UnresolvedExplicitGeography,
            }
        } else {
            GeoScope::BundleWide
        };
    }
    if seed_cells_by_id.is_empty() {
        if !has_positive_area_or_society_geography(predicates, false) {
            return GeoScope::BundleWide;
        }
        for anchor in &scope_anchors {
            let gap = format!("missing sourced geo-cell seed for {}", anchor.entity_id);
            if !resolution_gaps.contains(&gap) {
                resolution_gaps.push(gap);
            }
        }
        return GeoScope::Unresolved {
            requested: scope_anchors
                .iter()
                .map(|anchor| anchor.entity_id.clone())
                .collect(),
            anchors: scope_anchors,
            reason: GeoScopeResolution::MissingTopology,
        };
    }
    let Some(spatial_index) = spatial_index else {
        resolution_gaps.push("missing spatial index for geo-cell traversal".to_string());
        return GeoScope::Unresolved {
            requested: scope_anchors
                .iter()
                .map(|anchor| anchor.entity_id.clone())
                .collect(),
            anchors: scope_anchors,
            reason: GeoScopeResolution::MissingSpatialIndex,
        };
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

fn has_positive_explicit_geography(expression: &ConstraintExpr, negated: bool) -> bool {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| has_positive_explicit_geography(clause, negated)),
        ConstraintExpr::Not { clause } => has_positive_explicit_geography(clause, !negated),
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Area {
                    entity_id: Some(_), ..
                }
                | ConstraintTerm::Society { .. },
        } => !negated,
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Spatial {
                    entity_id,
                    required: true,
                    ..
                },
        } => !negated && !entity_id.is_empty(),
        ConstraintExpr::Term { .. } => false,
    }
}

fn has_positive_area_or_society_geography(expression: &ConstraintExpr, negated: bool) -> bool {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| has_positive_area_or_society_geography(clause, negated)),
        ConstraintExpr::Not { clause } => has_positive_area_or_society_geography(clause, !negated),
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Area {
                    entity_id: Some(_), ..
                }
                | ConstraintTerm::Society { .. },
        } => !negated,
        ConstraintExpr::Term { .. } => false,
    }
}

fn expand_geo_cells(
    anchors: &[GeoAnchor],
    seed_cells: &[GeoCellSeed],
    topology: &GraphIndex,
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

fn collect_geo_anchors<'a>(
    expression: &'a ConstraintExpr,
    negated: bool,
    explicit_area_ids: &mut BTreeSet<&'a str>,
    anchors: &mut BTreeSet<&'a str>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_geo_anchors(clause, negated, explicit_area_ids, anchors);
            }
        }
        ConstraintExpr::Not { clause } => {
            collect_geo_anchors(clause, !negated, explicit_area_ids, anchors)
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
            ConstraintTerm::Spatial {
                entity_id,
                required: true,
                ..
            } if !entity_id.is_empty() => {
                anchors.insert(entity_id);
            }
            _ => {}
        },
        ConstraintExpr::Term { .. } => {}
    }
}

fn assign_geo_clusters(branches: &mut [GeoBranch], topology: &GraphIndex) {
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

fn directly_connected_scopes(left: &GeoScope, right: &GeoScope, topology: &GraphIndex) -> bool {
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

fn canonical_scoring_query(
    predicates: &ConstraintExpr,
    intent: &SearchIntent,
    resolved_entities: &[ResolvedEntityHandle],
) -> String {
    let geography_spans = positive_geography_spans(predicates);
    let mut terms = all_source_spans(predicates)
        .into_iter()
        .filter(|span| {
            !geography_spans
                .iter()
                .any(|geography| spans_overlap(span, geography))
        })
        .map(|span| span.raw_text)
        .collect::<Vec<_>>();
    for preference in intent
        .positive_preferences
        .iter()
        .chain(intent.negative_preferences.iter())
    {
        push_unique_text(&mut terms, &preference.raw_text);
    }
    for priority in &intent.ranking_priorities {
        push_unique_text(&mut terms, priority);
    }
    for entity in resolved_entities
        .iter()
        .filter(|entity| entity.entity_type.eq_ignore_ascii_case("builder"))
    {
        push_unique_text(&mut terms, &entity.display_name);
    }
    normalize_terms(terms)
}

fn canonical_recall_query(
    predicates: &ConstraintExpr,
    intent: &SearchIntent,
    resolved_entities: &[ResolvedEntityHandle],
    scoring_query: &str,
) -> String {
    let mut terms = Vec::new();
    if !scoring_query.is_empty() {
        terms.push(scoring_query.to_string());
    }
    for entity in resolved_entities {
        push_unique_text(&mut terms, &entity.display_name);
    }
    for span in all_source_spans(predicates) {
        push_unique_text(&mut terms, &span.raw_text);
    }
    for preference in intent
        .positive_preferences
        .iter()
        .chain(intent.negative_preferences.iter())
    {
        push_unique_text(&mut terms, &preference.raw_text);
    }
    normalize_terms(terms)
}

fn normalize_terms(terms: Vec<String>) -> String {
    terms
        .into_iter()
        .flat_map(|term| {
            term.split_whitespace()
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_fallback_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_unique_text(values: &mut Vec<String>, value: &str) {
    if !value.trim().is_empty()
        && !values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

fn semantic_plan_fingerprint(
    predicates: &[ConstraintExpr],
    intents: &[SearchIntent],
    branches: &[GeoBranch],
) -> String {
    let base = semantic_search_fingerprint(predicates, intents);
    let fallback = branches
        .iter()
        .map(|branch| branch.fallback_text.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    let digest = Sha256::digest(
        serde_json::to_vec(&(base, fallback))
            .expect("compiled search semantics are JSON serializable"),
    );
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn spans_overlap(left: &SourceSpan, right: &SourceSpan) -> bool {
    if !left.source_turn_id.is_empty()
        && !right.source_turn_id.is_empty()
        && left.source_turn_id != right.source_turn_id
    {
        return false;
    }
    left.start < right.end && right.start < left.end
}

fn project_branch_intent(
    aggregate: &SearchIntent,
    predicates: &ConstraintExpr,
    owned_spatial_keys: &BTreeSet<String>,
    external_spatial_keys: &BTreeSet<String>,
) -> SearchIntent {
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
    intent.positive_preferences.retain(|preference| {
        preference_is_owned_by_branch(preference, owned_spatial_keys, external_spatial_keys)
    });
    intent.negative_preferences.retain(|preference| {
        preference_is_owned_by_branch(preference, owned_spatial_keys, external_spatial_keys)
    });
    intent.ranking_priorities.retain(|priority| {
        intent
            .positive_preferences
            .iter()
            .chain(intent.negative_preferences.iter())
            .any(|preference| preference.raw_text.eq_ignore_ascii_case(priority))
    });
    intent.preferences.retain(|preference| {
        intent
            .positive_preferences
            .iter()
            .chain(intent.negative_preferences.iter())
            .any(|signal| signal.raw_text.eq_ignore_ascii_case(preference))
            || intent
                .accepted_tradeoffs
                .iter()
                .any(|tradeoff| tradeoff.eq_ignore_ascii_case(preference))
    });
    collect_branch_intent(predicates, false, &mut intent);
    intent.area = (intent.areas.len() == 1).then(|| intent.areas[0].clone());
    intent.bhk = (intent.bhks.len() == 1).then(|| intent.bhks[0]);
    intent
}

fn preference_is_owned_by_branch(
    preference: &super::intent::PreferenceSignal,
    owned_spatial_keys: &BTreeSet<String>,
    external_spatial_keys: &BTreeSet<String>,
) -> bool {
    let overlaps_owned = preference
        .expanded_keys
        .iter()
        .any(|key| owned_spatial_keys.contains(key));
    let overlaps_external = preference
        .expanded_keys
        .iter()
        .any(|key| external_spatial_keys.contains(key));
    overlaps_owned || !overlaps_external
}

fn spatial_preference_keys(expression: &ConstraintExpr) -> BTreeSet<String> {
    fn collect(expression: &ConstraintExpr, negated: bool, keys: &mut BTreeSet<String>) {
        match expression {
            ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
                for clause in clauses {
                    collect(clause, negated, keys);
                }
            }
            ConstraintExpr::Not { clause } => collect(clause, !negated, keys),
            ConstraintExpr::Term {
                term:
                    ConstraintTerm::Spatial {
                        category_fact_keys, ..
                    },
            } if !negated => keys.extend(category_fact_keys.iter().cloned()),
            ConstraintExpr::Term { .. } => {}
        }
    }

    let mut keys = BTreeSet::new();
    collect(expression, false, &mut keys);
    keys
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
            ConstraintTerm::Evidence { constraint, .. }
                if !negated && !intent.hard_constraints.contains(constraint) =>
            {
                intent.hard_constraints.push(constraint.clone());
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
        DerivedEvidence, EvidenceRef, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex,
        ServingFactRecord, SourceObservation,
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
                    category_fact_keys: Vec::new(),
                    distance_limit_km: None,
                    span: Some(span(0, 5, "Metro")),
                }),
                vec!["area:alpha"],
                false,
            ),
            (
                "dangling society fails closed around the direct anchor",
                society_term("society:missing", "Missing", 0),
                Vec::new(),
                false,
            ),
            (
                "administrative containment cannot substitute for market topology",
                society_term("society:ward-only", "Ward Only", 0),
                Vec::new(),
                false,
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
        assert_eq!(plan.branches[0].ranking_intent.bhks, [2]);
        assert_eq!(plan.branches[1].ranking_intent.bhks, [3]);
        assert_eq!(plan.branches[2].ranking_intent.bhks, [4]);
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
            &plan.branches[0].eligibility_predicates,
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
                    category_fact_keys: Vec::new(),
                    distance_limit_km: None,
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
        let topology = GraphIndex::from_serving_bundle(entities, edges, "test-snapshot");
        CompiledSearchPlan::compile_for_snapshot(
            IntentAst {
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
            source_turn_id: String::new(),
            start,
            end,
            raw_text: raw_text.to_string(),
        }
    }
}
