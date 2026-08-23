//! Typed buyer-query AST.
//!
//! Parsing finishes before this tree is compiled into index predicates. The
//! model is a small boolean algebra (`And` / `AnyOf` / `Not` / `Term`), not a
//! language CST.
//!
//! We do not use `syn` (Rust source), `rowan` (programming-language CST), or
//! Tantivy `BooleanQuery` as the buyer model. Tokenization stays in `winnow`.
//! Tantivy bool queries can compile from this tree later.

use serde::{Deserialize, Serialize};

use super::intent::{HardConstraint, SearchIntent, SourceSpan};
use super::parser::{BhkConstraint, ParsedBudgetConstraint, SlotPolarity};
use super::query_plan::{MentionPolarity, QueryPlan};

/// Inclusive/exclusive numeric bound with the buyer's original words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericBound {
    pub value: u64,
    pub inclusive: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum ConstraintTerm {
    Bhk {
        value: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
    Area {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
    Society {
        entity_id: String,
        display_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
    Builder {
        entity_id: String,
        display_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
    Budget {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<NumericBound>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<NumericBound>,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
    Evidence {
        constraint: HardConstraint,
        #[serde(skip_serializing_if = "Option::is_none")]
        span: Option<SourceSpan>,
    },
}

/// Boolean query tree compiled after parsing is complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ConstraintExpr {
    And { clauses: Vec<ConstraintExpr> },
    AnyOf { clauses: Vec<ConstraintExpr> },
    Not { clause: Box<ConstraintExpr> },
    Term { term: ConstraintTerm },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedEntityConstraint {
    pub entity_id: String,
    pub entity_type: String,
    pub display_name: String,
    pub span: SourceSpan,
    pub exclusion: bool,
}

/// Authoritative internal buyer query.
///
/// `intent` is a derived API/ranking summary. Hard eligibility always evaluates
/// `constraints`.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub raw: String,
    pub constraints: ConstraintExpr,
    pub intent: SearchIntent,
}

impl CompiledQuery {
    pub(crate) fn compile(
        query: &str,
        plan: &QueryPlan,
        intent: SearchIntent,
        entities: &[ResolvedEntityConstraint],
    ) -> Self {
        let constraints = compile_constraint_expr(query, plan, entities);
        Self {
            raw: query.to_string(),
            constraints,
            intent,
        }
    }

    pub fn from_text(query: &str) -> Self {
        let plan = super::query_plan::compile_query_plan(query);
        let intent = super::query_plan::project_search_intent(query, &plan);
        Self::compile(query, &plan, intent, &[])
    }

    #[cfg(test)]
    pub(crate) fn from_text_with_intent(query: &str, intent: SearchIntent) -> Self {
        let plan = super::query_plan::compile_query_plan(query);
        Self::compile(query, &plan, intent, &[])
    }
}

impl ConstraintExpr {
    pub fn and(clauses: Vec<Self>) -> Self {
        let mut simplified = Vec::new();
        for clause in clauses {
            if clause.is_match_none() {
                return Self::match_none();
            }
            if !clause.is_match_all() {
                simplified.push(clause);
            }
        }
        match simplified.as_slice() {
            [] => Self::match_all(),
            [single] => single.clone(),
            _ => Self::And {
                clauses: simplified,
            },
        }
    }

    pub fn any_of(clauses: Vec<Self>) -> Self {
        let mut simplified = Vec::new();
        for clause in clauses {
            if clause.is_match_all() {
                return Self::match_all();
            }
            if !clause.is_match_none() {
                simplified.push(clause);
            }
        }
        match simplified.as_slice() {
            [] => Self::match_none(),
            [single] => single.clone(),
            _ => Self::AnyOf {
                clauses: simplified,
            },
        }
    }

    pub fn negated(clause: Self) -> Self {
        if clause.is_match_all() {
            Self::match_none()
        } else if clause.is_match_none() {
            Self::match_all()
        } else if let Self::Not { clause } = clause {
            *clause
        } else {
            Self::Not {
                clause: Box::new(clause),
            }
        }
    }

    pub fn term(term: ConstraintTerm) -> Self {
        Self::Term { term }
    }

    /// Evaluate the Boolean tree without flattening grouped alternatives.
    pub fn evaluate(&self, term_matches: &mut impl FnMut(&ConstraintTerm) -> bool) -> bool {
        match self {
            Self::And { clauses } => clauses.iter().all(|clause| clause.evaluate(term_matches)),
            Self::AnyOf { clauses } => clauses.iter().any(|clause| clause.evaluate(term_matches)),
            Self::Not { clause } => !clause.evaluate(term_matches),
            Self::Term { term } => term_matches(term),
        }
    }

    /// Lower the supported Boolean subset into flat top-level alternatives.
    /// Nested language is not exposed as a query contract; `And` distributes
    /// only over the configured `AnyOf` groups produced by our compiler.
    pub fn flat_branches(&self) -> Vec<Self> {
        match self {
            Self::AnyOf { clauses } => clauses.iter().flat_map(Self::flat_branches).collect(),
            Self::And { clauses } => {
                clauses
                    .iter()
                    .fold(vec![Self::and(Vec::new())], |branches, clause| {
                        let alternatives = clause.flat_branches();
                        branches
                            .into_iter()
                            .flat_map(|branch| {
                                alternatives.iter().cloned().map(move |alternative| {
                                    Self::and(vec![branch.clone(), alternative])
                                })
                            })
                            .collect()
                    })
            }
            Self::Not { .. } | Self::Term { .. } => vec![self.clone()],
        }
    }

    pub fn buyer_label(&self) -> String {
        let mut labels = Vec::new();
        collect_buyer_labels(self, false, &mut labels);
        labels.into_iter().take(3).collect::<Vec<_>>().join(" · ")
    }

    pub fn has_terms(&self) -> bool {
        match self {
            Self::And { clauses } | Self::AnyOf { clauses } => clauses.iter().any(Self::has_terms),
            Self::Not { clause } => clause.has_terms(),
            Self::Term { .. } => true,
        }
    }

    pub fn drop_bhk_includes(&mut self) {
        let expr = std::mem::replace(self, Self::match_all());
        *self = remove_positive_terms(expr, false, &|term| {
            matches!(term, ConstraintTerm::Bhk { .. })
        });
    }

    pub fn drop_society_includes(&mut self) {
        let expr = std::mem::replace(self, Self::match_all());
        *self = remove_positive_terms(expr, false, &|term| {
            matches!(term, ConstraintTerm::Society { .. })
        });
    }

    pub fn has_budget_max(&self) -> bool {
        has_positive_budget_max(self, false)
    }

    pub fn bhk_include_label(&self) -> Option<String> {
        format_bhk_include_label(&collect_bhk_values(self, false))
    }

    pub fn matched_bhk_include_label(
        &self,
        term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
    ) -> Option<String> {
        let mut values = Vec::new();
        collect_matching_bhk_values(self, false, term_matches, &mut values);
        format_bhk_include_label(&values)
    }

    pub fn matched_budget_bounds(
        &self,
        term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
    ) -> Option<(Option<u64>, Option<u64>)> {
        matched_budget_bounds(self, term_matches)
    }

    pub fn matched_evidence_constraints(
        &self,
        term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
    ) -> Vec<HardConstraint> {
        let mut constraints = Vec::new();
        collect_matching_evidence_constraints(self, term_matches, &mut constraints);
        constraints
    }

    fn match_all() -> Self {
        Self::And {
            clauses: Vec::new(),
        }
    }

    fn match_none() -> Self {
        Self::AnyOf {
            clauses: Vec::new(),
        }
    }

    fn is_match_all(&self) -> bool {
        matches!(self, Self::And { clauses } if clauses.is_empty())
    }

    fn is_match_none(&self) -> bool {
        matches!(self, Self::AnyOf { clauses } if clauses.is_empty())
    }
}

fn collect_buyer_labels(expr: &ConstraintExpr, negated: bool, labels: &mut Vec<String>) {
    match expr {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_buyer_labels(clause, negated, labels);
            }
        }
        ConstraintExpr::Not { clause } => collect_buyer_labels(clause, !negated, labels),
        ConstraintExpr::Term { term } => {
            let label = match term {
                ConstraintTerm::Bhk { value, .. } => format!("{value} BHK"),
                ConstraintTerm::Area { value, .. } => value.clone(),
                ConstraintTerm::Society { display_name, .. }
                | ConstraintTerm::Builder { display_name, .. } => display_name.clone(),
                ConstraintTerm::Budget { min, max, .. } => match (min, max) {
                    (None, Some(max)) => format!("Under {}", format_money(max.value)),
                    (Some(min), None) => format!("Above {}", format_money(min.value)),
                    (Some(min), Some(max)) => {
                        format!("{}–{}", format_money(min.value), format_money(max.value))
                    }
                    (None, None) => return,
                },
                ConstraintTerm::Evidence { constraint, .. } => constraint.raw_text.clone(),
            };
            let label = if negated {
                format!("Not {label}")
            } else {
                label
            };
            if !labels
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&label))
            {
                labels.push(label);
            }
        }
    }
}

fn format_money(value: u64) -> String {
    if value >= 10_000_000 {
        format!("₹{:.2}Cr", value as f64 / 10_000_000.0)
    } else if value >= 100_000 {
        format!("₹{:.0}L", value as f64 / 100_000.0)
    } else {
        format!("₹{value}")
    }
}

/// Whole-clause BHK copy, never the first requested configuration alone.
pub fn format_bhk_include_label(values: &[u32]) -> Option<String> {
    match values {
        [] => None,
        [one] => Some(format!("{one} BHK")),
        many => Some(format!(
            "{} BHK",
            many.iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(" or ")
        )),
    }
}

fn compile_constraint_expr(
    query: &str,
    plan: &QueryPlan,
    entities: &[ResolvedEntityConstraint],
) -> ConstraintExpr {
    let mut clauses = Vec::new();

    let include_bhks = bhk_terms(plan, SlotPolarity::Include);
    let mut include_areas = area_terms(plan, MentionPolarity::Positive);
    include_areas.extend(resolved_area_terms(
        entities
            .iter()
            .filter(|entity| !entity.exclusion && is_entity_type(entity, "area")),
    ));
    let include_societies = society_terms(
        entities
            .iter()
            .filter(|entity| !entity.exclusion && is_entity_type(entity, "society")),
    );
    let include_builders = builder_terms(
        entities
            .iter()
            .filter(|entity| !entity.exclusion && is_entity_type(entity, "builder")),
    );
    let include_budgets = budget_terms(plan);
    let include_evidence = evidence_terms(query, &plan.evidence);
    let exclude_bhks = bhk_terms(plan, SlotPolarity::Exclude);
    let mut exclude_areas = area_terms(plan, MentionPolarity::Exclusion);
    exclude_areas.extend(resolved_area_terms(
        entities
            .iter()
            .filter(|entity| entity.exclusion && is_entity_type(entity, "area")),
    ));
    let excluded_societies = society_terms(
        entities
            .iter()
            .filter(|entity| entity.exclusion && is_entity_type(entity, "society")),
    );
    let excluded_builders = builder_terms(
        entities
            .iter()
            .filter(|entity| entity.exclusion && is_entity_type(entity, "builder")),
    );
    let mut positive_groups = vec![
        include_bhks.as_slice(),
        include_areas.as_slice(),
        include_societies.as_slice(),
        include_builders.as_slice(),
        include_budgets.as_slice(),
    ];
    positive_groups.extend(include_evidence.iter().map(std::slice::from_ref));
    let negative_groups = [
        exclude_bhks.as_slice(),
        exclude_areas.as_slice(),
        excluded_societies.as_slice(),
        excluded_builders.as_slice(),
    ];
    if let Some(plan) = compile_constraint_plan(plan, &positive_groups, &negative_groups) {
        clauses.push(plan.lower());
    } else {
        for group in positive_groups {
            if group.is_empty() {
                continue;
            }
            clauses.push(ConstraintExpr::any_of(
                group.iter().map(|term| term.expr.clone()).collect(),
            ));
        }
        for group in negative_groups {
            if group.is_empty() {
                continue;
            }
            clauses.push(ConstraintExpr::negated(ConstraintExpr::any_of(
                group.iter().map(|term| term.expr.clone()).collect(),
            )));
        }
    }

    ConstraintExpr::and(clauses)
}

#[derive(Clone)]
struct SpannedTerm {
    family: ConstraintFamily,
    expr: ConstraintExpr,
    span: Option<SourceSpan>,
}

#[derive(Clone, PartialEq, Eq)]
enum ConstraintFamily {
    Bhk,
    Area,
    Society,
    Builder,
    Budget,
    Evidence(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum AlternativeFamily {
    Bhk,
    Entity,
    Budget,
    Evidence,
}

fn alternative_family(family: &ConstraintFamily) -> AlternativeFamily {
    match family {
        ConstraintFamily::Bhk => AlternativeFamily::Bhk,
        ConstraintFamily::Area | ConstraintFamily::Society | ConstraintFamily::Builder => {
            AlternativeFamily::Entity
        }
        ConstraintFamily::Budget => AlternativeFamily::Budget,
        ConstraintFamily::Evidence(_) => AlternativeFamily::Evidence,
    }
}

fn bhk_terms(plan: &QueryPlan, polarity: SlotPolarity) -> Vec<SpannedTerm> {
    plan.slots
        .bhks
        .iter()
        .filter(|slot| slot.polarity == polarity)
        .map(spanned_bhk_term)
        .collect()
}

fn spanned_bhk_term(slot: &BhkConstraint) -> SpannedTerm {
    let span = SourceSpan {
        start: slot.start,
        end: slot.end,
        raw_text: slot.raw_text.clone(),
    };
    SpannedTerm {
        family: ConstraintFamily::Bhk,
        expr: ConstraintExpr::term(ConstraintTerm::Bhk {
            value: slot.value,
            span: Some(span.clone()),
        }),
        span: Some(span),
    }
}

fn area_terms(plan: &QueryPlan, polarity: MentionPolarity) -> Vec<SpannedTerm> {
    plan.areas
        .iter()
        .filter(|area| area.polarity == polarity)
        .map(|area| {
            let span = SourceSpan {
                start: area.span.start,
                end: area.span.end,
                raw_text: area.matched_text.clone(),
            };
            SpannedTerm {
                family: ConstraintFamily::Area,
                expr: ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: None,
                    value: area.canonical.clone(),
                    span: Some(span.clone()),
                }),
                span: Some(span),
            }
        })
        .collect()
}

fn resolved_area_terms<'a>(
    areas: impl Iterator<Item = &'a ResolvedEntityConstraint>,
) -> Vec<SpannedTerm> {
    areas
        .map(|area| SpannedTerm {
            family: ConstraintFamily::Area,
            expr: ConstraintExpr::term(ConstraintTerm::Area {
                entity_id: Some(area.entity_id.clone()),
                value: area.display_name.clone(),
                span: Some(area.span.clone()),
            }),
            span: Some(area.span.clone()),
        })
        .collect()
}

fn is_entity_type(entity: &ResolvedEntityConstraint, entity_type: &str) -> bool {
    entity.entity_type.eq_ignore_ascii_case(entity_type)
}

fn society_terms<'a>(
    societies: impl Iterator<Item = &'a ResolvedEntityConstraint>,
) -> Vec<SpannedTerm> {
    societies
        .map(|society| SpannedTerm {
            family: ConstraintFamily::Society,
            expr: ConstraintExpr::term(ConstraintTerm::Society {
                entity_id: society.entity_id.clone(),
                display_name: society.display_name.clone(),
                span: Some(society.span.clone()),
            }),
            span: Some(society.span.clone()),
        })
        .collect()
}

fn builder_terms<'a>(
    builders: impl Iterator<Item = &'a ResolvedEntityConstraint>,
) -> Vec<SpannedTerm> {
    builders
        .map(|builder| SpannedTerm {
            family: ConstraintFamily::Builder,
            expr: ConstraintExpr::term(ConstraintTerm::Builder {
                entity_id: builder.entity_id.clone(),
                display_name: builder.display_name.clone(),
                span: Some(builder.span.clone()),
            }),
            span: Some(builder.span.clone()),
        })
        .collect()
}

fn budget_terms(plan: &QueryPlan) -> Vec<SpannedTerm> {
    plan.slots.budgets.iter().map(spanned_budget_term).collect()
}

fn spanned_budget_term(budget: &ParsedBudgetConstraint) -> SpannedTerm {
    let raw_text = budget
        .min
        .as_ref()
        .into_iter()
        .chain(budget.max.as_ref())
        .map(|bound| bound.raw_text.as_str())
        .collect::<Vec<_>>()
        .join("–");
    let span = SourceSpan {
        start: budget.start,
        end: budget.end,
        raw_text,
    };
    let expr = ConstraintExpr::term(ConstraintTerm::Budget {
        min: budget.min.as_ref().map(|bound| NumericBound {
            value: bound.value,
            inclusive: true,
            raw_text: bound.raw_text.clone(),
        }),
        max: budget.max.as_ref().map(|bound| NumericBound {
            value: bound.value,
            inclusive: true,
            raw_text: bound.raw_text.clone(),
        }),
        span: Some(span.clone()),
    });
    SpannedTerm {
        family: ConstraintFamily::Budget,
        expr,
        span: Some(span),
    }
}

fn evidence_terms(
    query: &str,
    evidence: &[super::schema::HardConstraintSpanMatch],
) -> Vec<SpannedTerm> {
    evidence
        .iter()
        .cloned()
        .filter_map(|matched| {
            let raw_text = query.get(matched.start..matched.end)?.to_string();
            let span = SourceSpan {
                start: matched.start,
                end: matched.end,
                raw_text,
            };
            let family = ConstraintFamily::Evidence(matched.constraint.field.to_ascii_lowercase());
            Some(SpannedTerm {
                family,
                expr: ConstraintExpr::term(ConstraintTerm::Evidence {
                    constraint: matched.constraint,
                    span: Some(span.clone()),
                }),
                span: Some(span),
            })
        })
        .collect()
}

fn hard_constraints_match(left: &HardConstraint, right: &HardConstraint) -> bool {
    left.field.eq_ignore_ascii_case(&right.field)
        && left.operator == right.operator
        && (left.value - right.value).abs() <= f64::EPSILON
        && left.unit.eq_ignore_ascii_case(&right.unit)
}

struct ConstraintPlan {
    shared: Vec<ConstraintExpr>,
    branches: Vec<ConstraintExpr>,
}

impl ConstraintPlan {
    fn lower(self) -> ConstraintExpr {
        let mut clauses = self.shared;
        clauses.push(ConstraintExpr::any_of(self.branches));
        ConstraintExpr::and(clauses)
    }
}

#[derive(Clone, Copy)]
struct PlannedTermGroup<'a> {
    terms: &'a [SpannedTerm],
    negated: bool,
}

fn compile_constraint_plan(
    plan: &QueryPlan,
    positive_groups: &[&[SpannedTerm]],
    negative_groups: &[&[SpannedTerm]],
) -> Option<ConstraintPlan> {
    let active_groups = positive_groups
        .iter()
        .copied()
        .filter(|terms| !terms.is_empty())
        .map(|terms| PlannedTermGroup {
            terms,
            negated: false,
        })
        .chain(
            negative_groups
                .iter()
                .copied()
                .filter(|terms| !terms.is_empty())
                .map(|terms| PlannedTermGroup {
                    terms,
                    negated: true,
                }),
        )
        .collect::<Vec<_>>();
    if active_groups.len() < 2 {
        return None;
    }
    let mut family_spans = Vec::<(ConstraintFamily, bool, Vec<SourceSpan>)>::new();
    for group in &active_groups {
        for term in group.terms {
            let Some(span) = term.span.clone() else {
                continue;
            };
            if let Some((_, _, spans)) = family_spans
                .iter_mut()
                .find(|(family, negated, _)| family == &term.family && *negated == group.negated)
            {
                spans.push(span);
            } else {
                family_spans.push((term.family.clone(), group.negated, vec![span]));
            }
        }
    }
    let branch_span_groups = family_spans
        .into_iter()
        .map(|(_, _, spans)| spans)
        .collect::<Vec<_>>();
    let owner_spans = active_groups
        .iter()
        .flat_map(|group| group.terms.iter())
        .filter_map(|term| term.span.clone())
        .collect::<Vec<_>>();
    let layout = plan.alternative_clause_layout(&owner_spans, &branch_span_groups)?;
    let segments = layout.segments;
    let group_segments = active_groups
        .iter()
        .map(|group| {
            segments
                .iter()
                .map(|segment| terms_within_segment_refs(group.terms, segment.start, segment.end))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let entity_scope_segments = group_segments
        .iter()
        .enumerate()
        .filter(|(group_index, _)| {
            active_groups[*group_index]
                .terms
                .first()
                .is_some_and(|term| is_entity_family(&term.family))
        })
        .flat_map(|(_, segment_terms)| {
            segment_terms
                .iter()
                .enumerate()
                .filter_map(|(segment_index, terms)| (!terms.is_empty()).then_some(segment_index))
        })
        .collect::<std::collections::HashSet<_>>();
    let entity_scopes_are_branch_local = entity_scope_segments.len() > 1;
    let last_segment_index = segments.len() - 1;
    let mut family_segments =
        std::collections::HashMap::<AlternativeFamily, std::collections::HashSet<usize>>::new();
    for segment_terms in &group_segments {
        for (segment_index, terms) in segment_terms.iter().enumerate() {
            for term in terms {
                family_segments
                    .entry(alternative_family(&term.family))
                    .or_default()
                    .insert(segment_index);
            }
        }
    }
    let branch_families = family_segments
        .into_iter()
        .filter_map(|(family, segments)| (segments.len() > 1).then_some(family))
        .collect::<std::collections::HashSet<_>>();
    let first_branch_start = group_segments
        .iter()
        .flat_map(|segment_terms| segment_terms[0].iter())
        .filter(|term| branch_families.contains(&alternative_family(&term.family)))
        .filter_map(|term| term.span.as_ref().map(|span| span.start))
        .min();
    let last_branch_end = group_segments
        .iter()
        .flat_map(|segment_terms| segment_terms[last_segment_index].iter())
        .filter(|term| branch_families.contains(&alternative_family(&term.family)))
        .filter_map(|term| term.span.as_ref().map(|span| span.end))
        .max();
    let shared_group_indexes = group_segments
        .iter()
        .enumerate()
        .filter_map(|(group_index, segment_terms)| {
            if entity_scopes_are_branch_local
                && active_groups[group_index]
                    .terms
                    .first()
                    .is_some_and(|term| is_entity_family(&term.family))
            {
                return None;
            }
            if active_groups[group_index]
                .terms
                .iter()
                .all(|term| term.span.is_none())
            {
                return Some(group_index);
            }
            let populated = segment_terms
                .iter()
                .enumerate()
                .filter(|(_, terms)| !terms.is_empty())
                .collect::<Vec<_>>();
            let [(segment_index, terms)] = populated.as_slice() else {
                return None;
            };
            let shared_prefix = *segment_index == 0
                && first_branch_start.is_some_and(|anchor| {
                    terms
                        .iter()
                        .all(|term| term.span.as_ref().is_some_and(|span| span.end <= anchor))
                });
            let shared_suffix = *segment_index == last_segment_index
                && last_branch_end.is_some_and(|anchor| {
                    terms
                        .iter()
                        .all(|term| term.span.as_ref().is_some_and(|span| span.start >= anchor))
                });
            (shared_prefix || shared_suffix).then_some(group_index)
        })
        .collect::<Vec<_>>();

    let mut branches = Vec::new();
    for segment_index in 0..segments.len() {
        let mut branch_groups = Vec::new();
        for (group_index, segment_terms) in group_segments.iter().enumerate() {
            if shared_group_indexes.contains(&group_index) {
                continue;
            }
            let branch_terms = &segment_terms[segment_index];
            if !branch_terms.is_empty() {
                let expr = ConstraintExpr::any_of(
                    branch_terms.iter().map(|term| term.expr.clone()).collect(),
                );
                branch_groups.push(if active_groups[group_index].negated {
                    ConstraintExpr::negated(expr)
                } else {
                    expr
                });
            }
        }
        if branch_groups.is_empty() {
            return None;
        }
        branches.push(ConstraintExpr::and(branch_groups));
    }
    if branches.len() < 2 {
        return None;
    }
    let shared = shared_group_indexes
        .into_iter()
        .map(|index| {
            let expr = ConstraintExpr::any_of(
                active_groups[index]
                    .terms
                    .iter()
                    .map(|term| term.expr.clone())
                    .collect(),
            );
            if active_groups[index].negated {
                ConstraintExpr::negated(expr)
            } else {
                expr
            }
        })
        .collect::<Vec<_>>();
    Some(ConstraintPlan { shared, branches })
}

fn is_entity_family(family: &ConstraintFamily) -> bool {
    matches!(
        family,
        ConstraintFamily::Area | ConstraintFamily::Society | ConstraintFamily::Builder
    )
}

fn terms_within_segment_refs(terms: &[SpannedTerm], start: usize, end: usize) -> Vec<&SpannedTerm> {
    terms
        .iter()
        .filter(|term| {
            term.span
                .as_ref()
                .is_some_and(|span| span.start >= start && span.start < end)
        })
        .collect()
}

fn remove_positive_terms(
    expr: ConstraintExpr,
    negated: bool,
    should_remove: &impl Fn(&ConstraintTerm) -> bool,
) -> ConstraintExpr {
    match expr {
        ConstraintExpr::And { clauses } => ConstraintExpr::and(
            clauses
                .into_iter()
                .map(|clause| remove_positive_terms(clause, negated, should_remove))
                .collect(),
        ),
        ConstraintExpr::AnyOf { clauses } => ConstraintExpr::any_of(
            clauses
                .into_iter()
                .map(|clause| remove_positive_terms(clause, negated, should_remove))
                .collect(),
        ),
        ConstraintExpr::Not { clause } => {
            ConstraintExpr::negated(remove_positive_terms(*clause, !negated, should_remove))
        }
        ConstraintExpr::Term { term } if !negated && should_remove(&term) => {
            ConstraintExpr::match_all()
        }
        term => term,
    }
}

fn has_positive_budget_max(expr: &ConstraintExpr, negated: bool) -> bool {
    match expr {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| has_positive_budget_max(clause, negated)),
        ConstraintExpr::Not { clause } => has_positive_budget_max(clause, !negated),
        ConstraintExpr::Term {
            term: ConstraintTerm::Budget { max, .. },
        } => !negated && max.is_some(),
        ConstraintExpr::Term { .. } => false,
    }
}

fn collect_bhk_values(expr: &ConstraintExpr, negated: bool) -> Vec<u32> {
    let mut values = Vec::new();
    collect_bhk_values_into(expr, negated, &mut values);
    values
}

fn collect_bhk_values_into(expr: &ConstraintExpr, negated: bool, values: &mut Vec<u32>) {
    match expr {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                collect_bhk_values_into(clause, negated, values);
            }
        }
        ConstraintExpr::Not { clause } => collect_bhk_values_into(clause, !negated, values),
        ConstraintExpr::Term {
            term: ConstraintTerm::Bhk { value, .. },
        } if !negated => push_unique_u32(values, *value),
        ConstraintExpr::Term { .. } => {}
    }
}

fn collect_matching_bhk_values(
    expr: &ConstraintExpr,
    negated: bool,
    term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
    values: &mut Vec<u32>,
) {
    match expr {
        ConstraintExpr::And { clauses } => {
            if expr.evaluate(term_matches) {
                for clause in clauses {
                    collect_matching_bhk_values(clause, negated, term_matches, values);
                }
            }
        }
        ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                if clause.evaluate(term_matches) {
                    collect_matching_bhk_values(clause, negated, term_matches, values);
                }
            }
        }
        ConstraintExpr::Not { .. } => {}
        ConstraintExpr::Term {
            term: term @ ConstraintTerm::Bhk { value, .. },
        } if !negated && term_matches(term) => push_unique_u32(values, *value),
        ConstraintExpr::Term { .. } => {}
    }
}

fn collect_matching_evidence_constraints(
    expr: &ConstraintExpr,
    term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
    constraints: &mut Vec<HardConstraint>,
) {
    match expr {
        ConstraintExpr::And { clauses } => {
            if expr.evaluate(term_matches) {
                for clause in clauses {
                    collect_matching_evidence_constraints(clause, term_matches, constraints);
                }
            }
        }
        ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                if clause.evaluate(term_matches) {
                    collect_matching_evidence_constraints(clause, term_matches, constraints);
                }
            }
        }
        ConstraintExpr::Not { .. } => {}
        ConstraintExpr::Term {
            term: term @ ConstraintTerm::Evidence { constraint, .. },
        } if term_matches(term)
            && !constraints
                .iter()
                .any(|existing| hard_constraints_match(existing, constraint)) =>
        {
            constraints.push(constraint.clone());
        }
        ConstraintExpr::Term { .. } => {}
    }
}

fn matched_budget_bounds(
    expr: &ConstraintExpr,
    term_matches: &mut impl FnMut(&ConstraintTerm) -> bool,
) -> Option<(Option<u64>, Option<u64>)> {
    match expr {
        ConstraintExpr::And { clauses } => clauses
            .iter()
            .find_map(|clause| matched_budget_bounds(clause, term_matches)),
        ConstraintExpr::AnyOf { clauses } => {
            for clause in clauses {
                if clause.evaluate(term_matches) {
                    if let Some(bounds) = matched_budget_bounds(clause, term_matches) {
                        return Some(bounds);
                    }
                }
            }
            None
        }
        ConstraintExpr::Not { .. } => None,
        ConstraintExpr::Term {
            term: term @ ConstraintTerm::Budget { min, max, .. },
        } if term_matches(term) => Some((
            min.as_ref().map(|bound| bound.value),
            max.as_ref().map(|bound| bound.value),
        )),
        ConstraintExpr::Term { .. } => None,
    }
}

fn push_unique_u32(values: &mut Vec<u32>, value: u32) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compile_constraint_expr, CompiledQuery, ConstraintExpr, ConstraintTerm,
        ResolvedEntityConstraint, SourceSpan,
    };
    use crate::search::intent::parse_intent;

    #[test]
    fn two_or_three_bhk_not_four_evaluates_as_any_of_and_not() {
        let intent = parse_intent("2 or 3 BHK, not 4 BHK");
        assert_eq!(intent.bhks, vec![2, 3]);
        assert_eq!(intent.exclude_bhks, vec![4]);
        let query = CompiledQuery::from_text_with_intent("2 or 3 BHK, not 4 BHK", intent);
        assert!(matches_bhk(&query.constraints, 2));
        assert!(matches_bhk(&query.constraints, 3));
        assert!(!matches_bhk(&query.constraints, 4));
        assert_eq!(
            query.constraints.bhk_include_label().as_deref(),
            Some("2 or 3 BHK")
        );
    }

    #[test]
    fn budget_not_over_evaluates_as_inclusive_max() {
        let intent = parse_intent("3BHK budget not over 2Cr");
        assert_eq!(intent.budget_min, None);
        assert_eq!(intent.budget_max, Some(20_000_000));
        let query = CompiledQuery::from_text_with_intent("3BHK budget not over 2Cr", intent);
        assert!(matches_price(&query.constraints, 20_000_000));
        assert!(!matches_price(&query.constraints, 20_000_001));
    }

    #[test]
    fn dash_range_budget_stays_a_between_clause() {
        let intent = parse_intent("2 BHK 1.5–2Cr");
        assert_eq!(intent.budget_min, Some(15_000_000));
        assert_eq!(intent.budget_max, Some(20_000_000));
    }

    #[test]
    fn dropping_bhk_includes_keeps_exclusions() {
        let mut query = CompiledQuery::from_text("2 or 3 BHK, not 4 BHK");
        query.constraints.drop_bhk_includes();
        assert!(matches_bhk(&query.constraints, 1));
        assert!(!matches_bhk(&query.constraints, 4));
    }

    #[test]
    fn grouped_cross_field_alternatives_do_not_become_a_cross_product() {
        let branch = |area: &str, bhk: u32| {
            ConstraintExpr::and(vec![
                ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: None,
                    value: area.to_string(),
                    span: None,
                }),
                ConstraintExpr::term(ConstraintTerm::Bhk {
                    value: bhk,
                    span: None,
                }),
            ])
        };
        let expr = ConstraintExpr::any_of(vec![branch("Whitefield", 3), branch("Bellandur", 2)]);

        assert!(matches_home(&expr, "Whitefield", 3));
        assert!(matches_home(&expr, "Bellandur", 2));
        assert!(!matches_home(&expr, "Whitefield", 2));
        assert!(!matches_home(&expr, "Bellandur", 3));
    }

    #[test]
    fn buyer_query_compiles_cross_field_alternatives_as_branches() {
        let query = CompiledQuery::from_text("3BHK in East Bengaluru or 2BHK in South Bengaluru");

        assert!(matches_home(&query.constraints, "East Bengaluru", 3));
        assert!(matches_home(&query.constraints, "South Bengaluru", 2));
        assert!(!matches_home(&query.constraints, "East Bengaluru", 2));
        assert!(!matches_home(&query.constraints, "South Bengaluru", 3));
    }

    #[test]
    fn repeated_bhk_occurrences_remain_in_each_grouped_branch() {
        let raw = "East Bengaluru 3BHK under 2Cr or South Bengaluru 3BHK under 3Cr";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            3,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            2,
            29_000_000
        ));
    }

    #[test]
    fn exclusion_internal_or_does_not_split_positive_constraints() {
        let raw = "3BHK under 2Cr, not 4 or 5 BHK in East Bengaluru";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            2,
            19_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            4,
            19_000_000
        ));
    }

    #[test]
    fn exclusion_scoped_to_one_alternative_stays_in_that_branch() {
        let raw = "3BHK in East Bengaluru or 4BHK not in East Bengaluru";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home(&query.constraints, "East Bengaluru", 3));
        assert!(matches_home(&query.constraints, "South Bengaluru", 4));
        assert!(!matches_home(&query.constraints, "East Bengaluru", 4));
        assert!(!matches_home(&query.constraints, "South Bengaluru", 3));
    }

    #[test]
    fn dangling_or_is_ignored_without_flattening_valid_branches() {
        let raw = "East Bengaluru 3BHK under 2Cr or South Bengaluru 4BHK under 3Cr or";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            4,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            3,
            19_000_000
        ));
    }

    #[test]
    fn adjacent_or_is_normalized_without_flattening_valid_branches() {
        let raw = "East Bengaluru 3BHK under 2Cr or or South Bengaluru 4BHK under 3Cr";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            4,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            4,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "West Bengaluru",
            3,
            50_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            2,
            19_000_000
        ));
    }

    #[test]
    fn soft_preference_or_does_not_split_hard_constraint_branches() {
        let raw = "3BHK ready or recently completed in East Bengaluru under 2Cr or 4BHK in South Bengaluru under 3Cr";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &query.constraints,
            "South Bengaluru",
            4,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            4,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "West Bengaluru",
            3,
            50_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            2,
            19_000_000
        ));
    }

    #[test]
    fn branch_specific_soft_words_do_not_hide_a_complete_hard_branch_boundary() {
        let raw = "East Bengaluru 3BHK ready or under construction South Bengaluru 4BHK";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home(&query.constraints, "East Bengaluru", 3));
        assert!(matches_home(&query.constraints, "South Bengaluru", 4));
        assert!(!matches_home(&query.constraints, "East Bengaluru", 4));
        assert!(!matches_home(&query.constraints, "South Bengaluru", 3));
    }

    #[test]
    fn leading_soft_state_keeps_trailing_area_bhk_budget_conjoined() {
        let raw = "under construction 3BHK in Test Area under 3.1Cr";
        let plan = crate::search::query_plan::compile_query_plan(raw);
        let start = raw.find("Test Area").expect("area should be in query");
        let areas = [ResolvedEntityConstraint {
            entity_id: "area:test-area".to_string(),
            entity_type: "area".to_string(),
            display_name: "Test Area".to_string(),
            span: SourceSpan {
                start,
                end: start + "Test Area".len(),
                raw_text: "Test Area".to_string(),
            },
            exclusion: false,
        }];
        let constraints = compile_constraint_expr(raw, &plan, &areas);

        assert!(matches_home_budget(
            &constraints,
            "Test Area",
            3,
            30_000_000
        ));
        assert!(!matches_home_budget(
            &constraints,
            "Test Area",
            3,
            32_000_000
        ));
    }

    #[test]
    fn serving_areas_with_repeated_bhk_keep_complete_grouped_branches() {
        let raw = "Whitefield 3BHK under 2Cr or Bellandur 3BHK under 3Cr";
        let plan = crate::search::query_plan::compile_query_plan(raw);
        let areas = ["Whitefield", "Bellandur"]
            .into_iter()
            .map(|name| {
                let start = raw.find(name).expect("area should be in query");
                ResolvedEntityConstraint {
                    entity_id: format!("area:{}", name.to_ascii_lowercase()),
                    entity_type: "area".to_string(),
                    display_name: name.to_string(),
                    span: SourceSpan {
                        start,
                        end: start + name.len(),
                        raw_text: name.to_string(),
                    },
                    exclusion: false,
                }
            })
            .collect::<Vec<_>>();
        let constraints = compile_constraint_expr(raw, &plan, &areas);

        assert!(matches_home_budget(
            &constraints,
            "Whitefield",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &constraints,
            "Bellandur",
            3,
            29_000_000
        ));
        assert!(!matches_home_budget(
            &constraints,
            "Bellandur",
            2,
            29_000_000
        ));
    }

    #[test]
    fn shared_suffix_area_stays_conjoined_with_grouped_bhk_budget_branches() {
        let raw = "3BHK under 2Cr or 4BHK under 4Cr in East Bengaluru";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            19_000_000
        ));
        assert!(matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            4,
            39_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "East Bengaluru",
            3,
            39_000_000
        ));
        assert!(!matches_home_budget(
            &query.constraints,
            "Bellandur",
            4,
            39_000_000
        ));
    }

    #[test]
    fn evidence_constraint_stays_inside_its_grouped_branch() {
        let raw = "3BHK above 10 acres under 2Cr or 4BHK under 4Cr";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_evidence_branch(
            &query.constraints,
            3,
            19_000_000,
            true
        ));
        assert!(!matches_evidence_branch(
            &query.constraints,
            3,
            19_000_000,
            false
        ));
        assert!(matches_evidence_branch(
            &query.constraints,
            4,
            39_000_000,
            false
        ));
        assert!(!matches_evidence_branch(
            &query.constraints,
            4,
            41_000_000,
            false
        ));
    }

    #[test]
    fn repeated_evidence_dimension_keeps_each_branch_threshold() {
        let raw = "3BHK above 10 acres under 2Cr or 4BHK above 5 acres under 4Cr";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_evidence_threshold(
            &query.constraints,
            3,
            19_000_000,
            12.0
        ));
        assert!(!matches_evidence_threshold(
            &query.constraints,
            3,
            19_000_000,
            7.0
        ));
        assert!(matches_evidence_threshold(
            &query.constraints,
            4,
            39_000_000,
            6.0
        ));
        assert!(!matches_evidence_threshold(
            &query.constraints,
            4,
            39_000_000,
            4.0
        ));
    }

    #[test]
    fn evidence_family_preserves_soft_word_bordered_alternatives() {
        let raw = "above 10 acres ready or under construction above 5 acres";
        let query = CompiledQuery::from_text(raw);

        assert!(matches_evidence_threshold(
            &query.constraints,
            3,
            19_000_000,
            12.0
        ));
        assert!(matches_evidence_threshold(
            &query.constraints,
            3,
            19_000_000,
            6.0
        ));
        assert!(!matches_evidence_threshold(
            &query.constraints,
            3,
            19_000_000,
            4.0
        ));
    }

    #[test]
    fn cross_dimension_evidence_alternatives_keep_shared_bhk() {
        let query = CompiledQuery::from_text("3BHK with 10+ acres or at least 80% open space");
        let evidence = query
            .constraints
            .matched_evidence_constraints(&mut |_| true);
        assert!(evidence
            .iter()
            .any(|constraint| constraint.field == "land_area"));
        assert!(evidence
            .iter()
            .any(|constraint| constraint.field == "open_area_pct"));

        assert!(matches_cross_evidence_alternative(
            &query.constraints,
            3,
            12.0,
            4.5,
            40.0
        ));
        assert!(matches_cross_evidence_alternative(
            &query.constraints,
            3,
            5.0,
            4.5,
            85.0
        ));
        assert!(!matches_cross_evidence_alternative(
            &query.constraints,
            2,
            5.0,
            4.5,
            85.0
        ));

        let suffix = CompiledQuery::from_text("10+ acres or at least 80% open space for 3BHK");
        assert!(matches_cross_evidence_alternative(
            &suffix.constraints,
            3,
            12.0,
            4.5,
            40.0
        ));
        assert!(matches_cross_evidence_alternative(
            &suffix.constraints,
            3,
            5.0,
            4.5,
            85.0
        ));
        assert!(!matches_cross_evidence_alternative(
            &suffix.constraints,
            2,
            5.0,
            4.5,
            85.0
        ));

        let multi_prefix = CompiledQuery::from_text(
            "3BHK with 10+ acres and Google rating >= 4.2 or at least 80% open space",
        );
        assert!(matches_cross_evidence_alternative(
            &multi_prefix.constraints,
            3,
            5.0,
            3.0,
            85.0
        ));

        let multi_suffix = CompiledQuery::from_text(
            "10+ acres or at least 80% open space and Google rating >= 4.2 for 3BHK",
        );
        assert!(matches_cross_evidence_alternative(
            &multi_suffix.constraints,
            3,
            12.0,
            3.0,
            40.0
        ));
    }

    #[test]
    fn symbolic_evidence_operators_reach_the_ast() {
        let query = CompiledQuery::from_text(
            "3BHK with 10+ acres, at least 80% open space, and Google rating >= 4.2",
        );
        let constraints = query
            .constraints
            .matched_evidence_constraints(&mut |_| true);

        assert!(constraints.iter().any(|constraint| {
            constraint.field == "land_area"
                && constraint.operator == crate::search::intent::ConstraintOperator::Min
                && constraint.value == 10.0
        }));
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "open_area_pct"
                && constraint.operator == crate::search::intent::ConstraintOperator::Min
                && constraint.value == 80.0
        }));
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "google_rating"
                && constraint.operator == crate::search::intent::ConstraintOperator::Min
                && constraint.value == 4.2
        }));
    }

    #[test]
    fn resolved_societies_compile_with_their_bhk_branches() {
        let query = "Godrej Air 3BHK or Prestige Waterford 4BHK";
        let plan = crate::search::query_plan::compile_query_plan(query);
        let societies = ["Godrej Air", "Prestige Waterford"]
            .into_iter()
            .map(|name| {
                let start = query.find(name).expect("society should be in query");
                ResolvedEntityConstraint {
                    entity_id: format!("society:{}", name.to_ascii_lowercase().replace(' ', "-")),
                    entity_type: "society".to_string(),
                    display_name: name.to_string(),
                    span: SourceSpan {
                        start,
                        end: start + name.len(),
                        raw_text: name.to_string(),
                    },
                    exclusion: false,
                }
            })
            .collect::<Vec<_>>();
        let ast = compile_constraint_expr(query, &plan, &societies);

        assert!(matches_project(&ast, "Godrej Air", 3));
        assert!(matches_project(&ast, "Prestige Waterford", 4));
        assert!(
            !matches_project(&ast, "Godrej Air", 4),
            "tokens={:#?}\nast={:#?}",
            plan.tokens,
            ast,
        );
        assert!(!matches_project(&ast, "Prestige Waterford", 3));
    }

    #[test]
    fn resolved_society_prefix_applies_to_each_bhk_alternative() {
        let query = "Godrej Air 2BHK or 3BHK";
        let plan = crate::search::query_plan::compile_query_plan(query);
        let name = "Godrej Air";
        let start = query.find(name).expect("society should be in query");
        let societies = vec![ResolvedEntityConstraint {
            entity_id: "society:godrej-air".to_string(),
            entity_type: "society".to_string(),
            display_name: name.to_string(),
            span: SourceSpan {
                start,
                end: start + name.len(),
                raw_text: name.to_string(),
            },
            exclusion: false,
        }];
        let ast = compile_constraint_expr(query, &plan, &societies);

        assert!(matches_project(&ast, "Godrej Air", 2));
        assert!(matches_project(&ast, "Godrej Air", 3));
        assert!(!matches_project(&ast, "Other Society", 2));
        assert!(!matches_project(&ast, "Other Society", 3));
    }

    #[test]
    fn resolved_societies_keep_branch_specific_budgets() {
        let query = "Godrej Air 3BHK under ₹2Cr or Prestige Waterford 4BHK under ₹4Cr";
        let plan = crate::search::query_plan::compile_query_plan(query);
        let societies = ["Godrej Air", "Prestige Waterford"]
            .into_iter()
            .map(|name| {
                let start = query.find(name).expect("society should be in query");
                ResolvedEntityConstraint {
                    entity_id: format!("society:{}", name.to_ascii_lowercase().replace(' ', "-")),
                    entity_type: "society".to_string(),
                    display_name: name.to_string(),
                    span: SourceSpan {
                        start,
                        end: start + name.len(),
                        raw_text: name.to_string(),
                    },
                    exclusion: false,
                }
            })
            .collect::<Vec<_>>();
        let ast = compile_constraint_expr(query, &plan, &societies);

        assert!(matches_project_budget(&ast, "Godrej Air", 3, 19_000_000));
        assert!(!matches_project_budget(&ast, "Godrej Air", 3, 21_000_000));
        assert!(matches_project_budget(
            &ast,
            "Prestige Waterford",
            4,
            39_000_000
        ));
        assert!(!matches_project_budget(
            &ast,
            "Prestige Waterford",
            4,
            41_000_000
        ));
        assert!(!matches_project_budget(&ast, "Godrej Air", 4, 19_000_000));
    }

    #[test]
    fn resolved_positive_builder_is_an_authoritative_constraint() {
        let query = "Prestige under 2Cr";
        let plan = crate::search::query_plan::compile_query_plan(query);
        let builders = vec![ResolvedEntityConstraint {
            entity_id: "builder:prestige".to_string(),
            entity_type: "builder".to_string(),
            display_name: "Prestige".to_string(),
            span: SourceSpan {
                start: 0,
                end: "Prestige".len(),
                raw_text: "Prestige".to_string(),
            },
            exclusion: false,
        }];
        let ast = compile_constraint_expr(query, &plan, &builders);

        assert!(matches_builder_budget(&ast, "Prestige", 19_000_000));
        assert!(!matches_builder_budget(&ast, "Other Builder", 19_000_000));
        assert!(!matches_builder_budget(&ast, "Prestige", 21_000_000));
    }

    #[test]
    fn mixed_entity_families_remain_in_their_respective_branches() {
        let query = "Prestige 3BHK or Godrej Air 4BHK";
        let plan = crate::search::query_plan::compile_query_plan(query);
        let entities = [
            ("builder:prestige", "builder", "Prestige"),
            ("society:godrej-air", "society", "Godrej Air"),
        ]
        .into_iter()
        .map(|(entity_id, entity_type, name)| {
            let start = query.find(name).expect("entity should be in query");
            ResolvedEntityConstraint {
                entity_id: entity_id.to_string(),
                entity_type: entity_type.to_string(),
                display_name: name.to_string(),
                span: SourceSpan {
                    start,
                    end: start + name.len(),
                    raw_text: name.to_string(),
                },
                exclusion: false,
            }
        })
        .collect::<Vec<_>>();
        let ast = compile_constraint_expr(query, &plan, &entities);

        assert!(matches_mixed_entity(&ast, "Prestige", "Other", 3));
        assert!(matches_mixed_entity(&ast, "Other", "Godrej Air", 4));
        assert!(!matches_mixed_entity(&ast, "Prestige", "Other", 4));
        assert!(!matches_mixed_entity(&ast, "Other", "Godrej Air", 3));
    }

    #[test]
    fn compiled_terms_keep_real_query_byte_spans() {
        let query = "2/3 BHK under 2Cr";
        let compiled = CompiledQuery::from_text(query);
        let mut spans = Vec::new();
        collect_spans(&compiled.constraints, &mut spans);

        assert_eq!(&query[spans[0].0..spans[0].1], "2/3 BHK");
        assert_eq!(&query[spans[1].0..spans[1].1], "2/3 BHK");
        assert_eq!(&query[spans[2].0..spans[2].1], "2Cr");
    }

    fn collect_spans(expr: &ConstraintExpr, spans: &mut Vec<(usize, usize)>) {
        match expr {
            ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
                for clause in clauses {
                    collect_spans(clause, spans);
                }
            }
            ConstraintExpr::Not { clause } => collect_spans(clause, spans),
            ConstraintExpr::Term {
                term:
                    ConstraintTerm::Bhk {
                        span: Some(span), ..
                    }
                    | ConstraintTerm::Budget {
                        span: Some(span), ..
                    },
            } => spans.push((span.start, span.end)),
            ConstraintExpr::Term { .. } => {}
        }
    }

    fn matches_bhk(expr: &ConstraintExpr, bhk: u32) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Budget { .. }
            | ConstraintTerm::Area { .. }
            | ConstraintTerm::Society { .. }
            | ConstraintTerm::Builder { .. }
            | ConstraintTerm::Evidence { .. } => true,
        })
    }

    fn matches_price(expr: &ConstraintExpr, price: u64) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            _ => true,
        })
    }

    fn matches_evidence_branch(
        expr: &ConstraintExpr,
        bhk: u32,
        price: u64,
        has_evidence: bool,
    ) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            ConstraintTerm::Evidence { .. } => has_evidence,
            ConstraintTerm::Area { .. }
            | ConstraintTerm::Society { .. }
            | ConstraintTerm::Builder { .. } => true,
        })
    }

    fn matches_evidence_threshold(
        expr: &ConstraintExpr,
        bhk: u32,
        price: u64,
        acreage: f64,
    ) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            ConstraintTerm::Evidence { constraint, .. } => match constraint.operator {
                crate::search::intent::ConstraintOperator::Min => acreage >= constraint.value,
                crate::search::intent::ConstraintOperator::Max => acreage <= constraint.value,
            },
            ConstraintTerm::Area { .. }
            | ConstraintTerm::Society { .. }
            | ConstraintTerm::Builder { .. } => true,
        })
    }

    fn matches_cross_evidence_alternative(
        expr: &ConstraintExpr,
        bhk: u32,
        acreage: f64,
        google_rating: f64,
        open_area_pct: f64,
    ) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Evidence { constraint, .. } => {
                let actual = match constraint.field.as_str() {
                    "land_area" => acreage,
                    "google_rating" => google_rating,
                    "open_area_pct" => open_area_pct,
                    _ => return false,
                };
                match constraint.operator {
                    crate::search::intent::ConstraintOperator::Min => actual >= constraint.value,
                    crate::search::intent::ConstraintOperator::Max => actual <= constraint.value,
                }
            }
            _ => true,
        })
    }

    fn matches_home(expr: &ConstraintExpr, area: &str, bhk: u32) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Area { value, .. } => value.eq_ignore_ascii_case(area),
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            _ => true,
        })
    }

    fn matches_home_budget(expr: &ConstraintExpr, area: &str, bhk: u32, price: u64) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Area { value, .. } => value.eq_ignore_ascii_case(area),
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            _ => true,
        })
    }

    fn matches_project(expr: &ConstraintExpr, society: &str, bhk: u32) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Society { display_name, .. } => {
                display_name.eq_ignore_ascii_case(society)
            }
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            _ => true,
        })
    }

    fn matches_project_budget(expr: &ConstraintExpr, society: &str, bhk: u32, price: u64) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Society { display_name, .. } => {
                display_name.eq_ignore_ascii_case(society)
            }
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            _ => true,
        })
    }

    fn matches_builder_budget(expr: &ConstraintExpr, builder: &str, price: u64) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Builder { display_name, .. } => {
                display_name.eq_ignore_ascii_case(builder)
            }
            ConstraintTerm::Budget { min, max, .. } => {
                min.as_ref().is_none_or(|bound| price >= bound.value)
                    && max.as_ref().is_none_or(|bound| price <= bound.value)
            }
            _ => true,
        })
    }

    fn matches_mixed_entity(expr: &ConstraintExpr, builder: &str, society: &str, bhk: u32) -> bool {
        expr.evaluate(&mut |term| match term {
            ConstraintTerm::Builder { display_name, .. } => {
                display_name.eq_ignore_ascii_case(builder)
            }
            ConstraintTerm::Society { display_name, .. } => {
                display_name.eq_ignore_ascii_case(society)
            }
            ConstraintTerm::Bhk { value, .. } => *value == bhk,
            _ => true,
        })
    }
}
