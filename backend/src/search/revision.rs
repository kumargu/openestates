use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

use crate::dag_config::search_parser_config;
use crate::serving::SpatialServingIndex;

use super::ast::{ConstraintExpr, PredicateFamily};
use super::compiled_plan::{
    CompiledPredicateBinding, CompiledSearchPlan, GeoCellSearchPolicy, ResolvedEntityHandle,
};
use super::intent::{SearchIntent, SourceSpan};
use super::SearchRuntimeVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchRevisionOperation {
    Initial,
    Refine,
    Rephrase,
    Expand,
    Replace,
    Exclude,
    Correct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchRevisionOutcome {
    Candidate,
    RequireClarification,
    RequireCheckpoint,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchRevisionLimits {
    pub max_active_branches: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TypedSearchRevisionPatch {
    AddPredicate {
        branch_ids: Vec<String>,
        expression: super::ast::ConstraintExpr,
    },
    ReplacePredicate {
        branch_ids: Vec<String>,
        families: Vec<PredicateFamily>,
        predicate_ids: Vec<String>,
        expression: super::ast::ConstraintExpr,
    },
    AddAlternative {
        branch_id: String,
        expression: super::ast::ConstraintExpr,
    },
    ReplaceIntent,
}

#[derive(Debug, Clone)]
pub struct TypedSearchRevision {
    pub operation: SearchRevisionOperation,
    pub outcome: SearchRevisionOutcome,
    pub patches: Vec<TypedSearchRevisionPatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedSearchContext {
    pub version: u32,
    pub revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_revision_id: Option<String>,
    pub operation: SearchRevisionOperation,
    pub depth: usize,
    pub active_query: String,
    pub semantic_fingerprint: String,
    pub runtime_lineage: SearchRuntimeVersion,
    pub intent_ast: PortableIntentAst,
    pub result_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableIntentAst {
    pub version: u32,
    pub branches: Vec<PortableIntentAstBranch>,
    pub aggregate_intent: SearchIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableIntentAstBranch {
    pub branch_id: String,
    pub predicates: ConstraintExpr,
    pub predicate_bindings: Vec<CompiledPredicateBinding>,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
}

impl PortableIntentAst {
    pub fn from_plan(plan: &CompiledSearchPlan) -> Self {
        Self {
            version: 1,
            branches: plan
                .branches
                .iter()
                .map(|branch| PortableIntentAstBranch {
                    branch_id: branch.branch_id.clone(),
                    predicates: branch.predicates.clone(),
                    predicate_bindings: branch.predicate_bindings.clone(),
                    resolved_entities: branch.resolved_entities.clone(),
                })
                .collect(),
            aggregate_intent: plan.aggregate_intent.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyerIntentBranchProjection {
    pub id: String,
    pub predicates: Vec<BuyerIntentPredicateProjection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyerIntentPredicateProjection {
    pub id: String,
    pub dimension: String,
    pub polarity: String,
    pub operator: String,
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_label: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRevisionDescriptor {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub operation: SearchRevisionOperation,
    pub depth: usize,
    pub active_query: String,
    pub semantic_fingerprint: String,
    pub state_token: String,
    pub intent_breakdown: Vec<BuyerIntentBranchProjection>,
}

/// Classify a single newly compiled utterance against an authenticated parent
/// plan. Parent buyer text is never parsed on this path.
pub fn compile_typed_revision(
    parent: &CompiledSearchPlan,
    fragment: &CompiledSearchPlan,
    utterance: &str,
    parent_revision_id: &str,
    limits: SearchRevisionLimits,
) -> TypedSearchRevision {
    let turn = utterance.trim();
    let discourse = &search_parser_config().discourse;
    if turn.is_empty() {
        return typed_clarification(SearchRevisionOperation::Refine);
    }
    let relative_fragment = relative_budget_fragment(parent, fragment, turn);
    let fragment = relative_fragment.as_ref().unwrap_or(fragment);
    let expand = strip_configured_prefix(turn, &discourse.revision_expand_prefixes).is_some();
    let correction = strip_configured_prefix(turn, &discourse.revision_correction_prefixes)
        .or_else(|| strip_configured_prefix(turn, &discourse.revision_overwrite_prefixes))
        .is_some();
    let exclusion = strip_configured_prefix(turn, &discourse.revision_exclusion_prefixes).is_some();
    if expand {
        if parent.branches.len() >= limits.max_active_branches {
            return TypedSearchRevision {
                operation: SearchRevisionOperation::Expand,
                outcome: SearchRevisionOutcome::RequireCheckpoint,
                patches: Vec::new(),
            };
        }
        let alternatives = fragment
            .branches
            .iter()
            .filter(|branch| branch.predicates.has_terms())
            .collect::<Vec<_>>();
        if alternatives.is_empty() {
            return typed_clarification(SearchRevisionOperation::Expand);
        }
        if parent.branches.len() + alternatives.len() > limits.max_active_branches {
            return TypedSearchRevision {
                operation: SearchRevisionOperation::Expand,
                outcome: SearchRevisionOutcome::RequireCheckpoint,
                patches: Vec::new(),
            };
        }
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Expand,
            outcome: SearchRevisionOutcome::Candidate,
            patches: alternatives
                .into_iter()
                .enumerate()
                .map(|(index, branch)| TypedSearchRevisionPatch::AddAlternative {
                    branch_id: format!("branch:{parent_revision_id}:{}", index + 1),
                    expression: branch.predicates.clone(),
                })
                .collect(),
        };
    }

    let Some(fragment_branch) = fragment.branches.first() else {
        return typed_clarification(SearchRevisionOperation::Refine);
    };

    let families = expression_families(&fragment_branch.predicates);
    if families.is_empty() {
        if fragment.aggregate_intent.positive_preferences.is_empty()
            && fragment.aggregate_intent.negative_preferences.is_empty()
            && fragment.aggregate_intent.ranking_priorities.is_empty()
        {
            return typed_clarification(SearchRevisionOperation::Refine);
        }
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Refine,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::AddPredicate {
                branch_ids: parent
                    .branches
                    .iter()
                    .map(|branch| branch.branch_id.clone())
                    .collect(),
                expression: ConstraintExpr::and(Vec::new()),
            }],
        };
    }
    let target_ids = if parent.branches.len() == 1 || targets_every_branch(turn) {
        parent
            .branches
            .iter()
            .map(|branch| branch.branch_id.clone())
            .collect::<Vec<_>>()
    } else if let Some(index) = targeted_branch_index(turn, parent.branches.len()) {
        vec![parent.branches[index].branch_id.clone()]
    } else if correction || families.iter().any(is_overwrite_family) {
        return typed_clarification(if correction {
            SearchRevisionOperation::Correct
        } else {
            SearchRevisionOperation::Refine
        });
    } else {
        parent
            .branches
            .iter()
            .map(|branch| branch.branch_id.clone())
            .collect::<Vec<_>>()
    };

    if exclusion {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Exclude,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::AddPredicate {
                branch_ids: target_ids,
                expression: fragment_branch.predicates.clone(),
            }],
        };
    }
    if fragment.semantic_fingerprint == parent.semantic_fingerprint {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Rephrase,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::ReplaceIntent],
        };
    }
    let explicit_replace = strip_configured_prefix(turn, &discourse.revision_switch_prefixes)
        .is_some()
        || contains_configured_phrase(turn, &discourse.revision_replace_markers);
    if explicit_replace {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Replace,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::ReplaceIntent],
        };
    }

    let replace_families = families
        .iter()
        .copied()
        .filter(|family| {
            *family != PredicateFamily::Spatial && (correction || is_overwrite_family(family))
        })
        .collect::<Vec<_>>();
    let spatial_predicate_ids = if correction && families.contains(&PredicateFamily::Spatial) {
        let predicate_ids =
            matching_spatial_predicate_ids(parent, &target_ids, &fragment_branch.predicates);
        if predicate_ids.is_empty() {
            return typed_clarification(SearchRevisionOperation::Correct);
        }
        predicate_ids
    } else {
        Vec::new()
    };
    let operation = if correction {
        SearchRevisionOperation::Correct
    } else {
        SearchRevisionOperation::Refine
    };
    let mut patches = Vec::new();
    if !replace_families.is_empty() {
        patches.push(TypedSearchRevisionPatch::ReplacePredicate {
            branch_ids: target_ids.clone(),
            families: replace_families.clone(),
            predicate_ids: Vec::new(),
            expression: fragment_branch
                .predicates
                .select_families(&replace_families),
        });
    }
    if !spatial_predicate_ids.is_empty() {
        patches.push(TypedSearchRevisionPatch::ReplacePredicate {
            branch_ids: target_ids.clone(),
            families: Vec::new(),
            predicate_ids: spatial_predicate_ids,
            expression: fragment_branch
                .predicates
                .select_families(&[PredicateFamily::Spatial]),
        });
    }
    let add_families = families
        .into_iter()
        .filter(|family| {
            !replace_families.contains(family)
                && !(*family == PredicateFamily::Spatial && correction)
        })
        .collect::<Vec<_>>();
    if !add_families.is_empty() {
        patches.push(TypedSearchRevisionPatch::AddPredicate {
            branch_ids: target_ids,
            expression: fragment_branch.predicates.select_families(&add_families),
        });
    }
    TypedSearchRevision {
        operation,
        outcome: SearchRevisionOutcome::Candidate,
        patches,
    }
}

fn relative_budget_fragment(
    parent: &CompiledSearchPlan,
    fragment: &CompiledSearchPlan,
    utterance: &str,
) -> Option<CompiledSearchPlan> {
    let discourse = &search_parser_config().discourse;
    let increase =
        strip_configured_prefix(utterance, &discourse.revision_relative_increase_prefixes)
            .is_some();
    let decrease =
        strip_configured_prefix(utterance, &discourse.revision_relative_decrease_prefixes)
            .is_some();
    if !increase && !decrease {
        return None;
    }
    let target = targeted_branch_index(utterance, parent.branches.len()).unwrap_or(0);
    let base = expression_budget_max(&parent.branches.get(target)?.predicates)?;
    let parsed = super::parser::parse_query_slots(utterance);
    let amount = parsed.budget_max.or(parsed.budget_min)?;
    let corrected = if increase {
        base.checked_add(amount.value)?
    } else {
        base.checked_sub(amount.value)?
    };
    let mut resolved = fragment.clone();
    let branch = resolved.branches.first_mut()?;
    let expression = ConstraintExpr::term(super::ast::ConstraintTerm::Budget {
        min: None,
        max: Some(super::ast::NumericBound {
            value: corrected,
            inclusive: true,
            raw_text: amount.raw_text.clone(),
        }),
        span: Some(SourceSpan {
            start: amount.start,
            end: amount.end,
            raw_text: amount.raw_text,
            source_turn_id: fragment
                .branches
                .first()
                .and_then(|branch| branch.source_spans.first())
                .map(|span| span.source_turn_id.clone())
                .unwrap_or_default(),
        }),
    });
    branch.predicates = expression.clone();
    branch.eligibility_predicates = expression;
    branch.ranking_intent.budget_min = None;
    branch.ranking_intent.budget_max = Some(corrected);
    resolved.aggregate_intent.budget_min = None;
    resolved.aggregate_intent.budget_max = Some(corrected);
    Some(resolved)
}

fn expression_budget_max(expression: &ConstraintExpr) -> Option<u64> {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            clauses.iter().find_map(expression_budget_max)
        }
        ConstraintExpr::Not { .. } => None,
        ConstraintExpr::Term {
            term: super::ast::ConstraintTerm::Budget { max, .. },
        } => max.as_ref().map(|bound| bound.value),
        ConstraintExpr::Term { .. } => None,
    }
}

pub fn apply_typed_revision(
    parent: &CompiledSearchPlan,
    fragment: &CompiledSearchPlan,
    revision: &TypedSearchRevision,
    topology: &crate::graph::GraphIndex,
    spatial_index: Option<&SpatialServingIndex>,
    geo_cell_policy: GeoCellSearchPolicy,
) -> Option<CompiledSearchPlan> {
    if revision.outcome != SearchRevisionOutcome::Candidate {
        return None;
    }
    if revision
        .patches
        .iter()
        .any(|patch| matches!(patch, TypedSearchRevisionPatch::ReplaceIntent))
    {
        return Some(fragment.clone());
    }

    let mut branches = parent
        .branches
        .iter()
        .map(|branch| (branch.branch_id.clone(), branch.predicates.clone()))
        .collect::<Vec<_>>();
    for patch in &revision.patches {
        match patch {
            TypedSearchRevisionPatch::AddPredicate {
                branch_ids,
                expression,
            } => {
                for (branch_id, predicates) in &mut branches {
                    if branch_ids.contains(branch_id) {
                        *predicates = conjoin(predicates.clone(), expression.clone());
                    }
                }
            }
            TypedSearchRevisionPatch::ReplacePredicate {
                branch_ids,
                families,
                predicate_ids,
                expression,
            } => {
                for (branch_id, predicates) in &mut branches {
                    if branch_ids.contains(branch_id) {
                        if !families.is_empty() {
                            predicates.remove_positive_families(families);
                            *predicates = conjoin(predicates.clone(), expression.clone());
                        }
                        if !predicate_ids.is_empty() {
                            let paths = parent
                                .branches
                                .iter()
                                .find(|branch| branch.branch_id == *branch_id)
                                .map(|branch| {
                                    branch
                                        .predicate_bindings
                                        .iter()
                                        .filter(|binding| {
                                            predicate_ids.contains(&binding.predicate_id)
                                        })
                                        .map(|binding| binding.path.clone())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            predicates.replace_predicate_paths(&paths, expression.clone());
                        }
                    }
                }
            }
            TypedSearchRevisionPatch::AddAlternative {
                branch_id,
                expression,
            } => {
                let geo_families = [
                    PredicateFamily::Area,
                    PredicateFamily::Society,
                    PredicateFamily::Spatial,
                ];
                let expression_families = expression_families(expression);
                let alternative = if expression_families
                    .iter()
                    .all(|family| geo_families.contains(family))
                {
                    let mut inherited = parent.branches.first()?.predicates.clone();
                    inherited.remove_positive_families(&geo_families);
                    conjoin(inherited, expression.clone())
                } else {
                    expression.clone()
                };
                branches.push((branch_id.clone(), alternative));
            }
            TypedSearchRevisionPatch::ReplaceIntent => {}
        }
    }
    let mut resolved_entities = parent
        .branches
        .iter()
        .chain(fragment.branches.iter())
        .flat_map(|branch| branch.resolved_entities.iter().cloned())
        .fold(
            Vec::<ResolvedEntityHandle>::new(),
            |mut entities, entity| {
                if !entities
                    .iter()
                    .any(|existing| existing.entity_id == entity.entity_id)
                {
                    entities.push(entity);
                }
                entities
            },
        );
    resolved_entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    let mut aggregate_intent =
        merge_revision_intent(&parent.aggregate_intent, &fragment.aggregate_intent);
    for patch in &revision.patches {
        match patch {
            TypedSearchRevisionPatch::ReplacePredicate { families, .. } => {
                if families.contains(&PredicateFamily::Bhk) {
                    aggregate_intent.bhk = fragment.aggregate_intent.bhk;
                    aggregate_intent
                        .bhks
                        .clone_from(&fragment.aggregate_intent.bhks);
                    aggregate_intent
                        .bhk_spans
                        .clone_from(&fragment.aggregate_intent.bhk_spans);
                }
                if families.contains(&PredicateFamily::Budget) {
                    aggregate_intent.budget_min = fragment.aggregate_intent.budget_min;
                    aggregate_intent.budget_max = fragment.aggregate_intent.budget_max;
                }
            }
            TypedSearchRevisionPatch::AddAlternative { .. } => {
                for area in &fragment.aggregate_intent.areas {
                    if !aggregate_intent
                        .areas
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(area))
                    {
                        aggregate_intent.areas.push(area.clone());
                    }
                }
                aggregate_intent.area =
                    (aggregate_intent.areas.len() == 1).then(|| aggregate_intent.areas[0].clone());
                for bhk in &fragment.aggregate_intent.bhks {
                    if !aggregate_intent.bhks.contains(bhk) {
                        aggregate_intent.bhks.push(*bhk);
                    }
                }
                aggregate_intent.bhk =
                    (aggregate_intent.bhks.len() == 1).then_some(aggregate_intent.bhks[0]);
            }
            _ => {}
        }
    }
    let mut recompiled = parent.recompile_for_snapshot(
        branches,
        aggregate_intent,
        resolved_entities,
        topology,
        spatial_index,
        geo_cell_policy,
    );
    retain_replaced_predicate_ids(parent, &mut recompiled, &revision.patches);
    Some(recompiled)
}

fn retain_replaced_predicate_ids(
    parent: &CompiledSearchPlan,
    candidate: &mut CompiledSearchPlan,
    patches: &[TypedSearchRevisionPatch],
) {
    for patch in patches {
        let TypedSearchRevisionPatch::ReplacePredicate {
            branch_ids,
            families,
            ..
        } = patch
        else {
            continue;
        };
        for branch_id in branch_ids {
            let Some(previous) = parent
                .branches
                .iter()
                .find(|branch| branch.branch_id == *branch_id)
            else {
                continue;
            };
            let Some(replaced) = candidate
                .branches
                .iter_mut()
                .find(|branch| branch.branch_id == *branch_id)
            else {
                continue;
            };
            let previous_ids = previous
                .predicate_bindings
                .iter()
                .filter(|binding| {
                    binding.polarity == super::ast::PredicatePolarity::Positive
                        && families.contains(&binding.family)
                })
                .map(|binding| binding.predicate_id.clone())
                .collect::<Vec<_>>();
            for (binding, predicate_id) in replaced
                .predicate_bindings
                .iter_mut()
                .filter(|binding| {
                    binding.polarity == super::ast::PredicatePolarity::Positive
                        && families.contains(&binding.family)
                })
                .zip(previous_ids)
            {
                binding.predicate_id = predicate_id;
            }
        }
    }
}

fn expression_families(expression: &ConstraintExpr) -> Vec<PredicateFamily> {
    const ALL: [PredicateFamily; 7] = [
        PredicateFamily::Bhk,
        PredicateFamily::Area,
        PredicateFamily::Society,
        PredicateFamily::Builder,
        PredicateFamily::Budget,
        PredicateFamily::Evidence,
        PredicateFamily::Spatial,
    ];
    ALL.into_iter()
        .filter(|family| expression.contains_family(*family))
        .collect()
}

fn conjoin(left: ConstraintExpr, right: ConstraintExpr) -> ConstraintExpr {
    let mut clauses = match left {
        ConstraintExpr::And { clauses } => clauses,
        other => vec![other],
    };
    match right {
        ConstraintExpr::And {
            clauses: right_clauses,
        } => clauses.extend(right_clauses),
        other => clauses.push(other),
    }
    ConstraintExpr::and(clauses)
}

fn is_overwrite_family(family: &PredicateFamily) -> bool {
    matches!(family, PredicateFamily::Bhk | PredicateFamily::Budget)
}

fn matching_spatial_predicate_ids(
    parent: &CompiledSearchPlan,
    branch_ids: &[String],
    replacement: &ConstraintExpr,
) -> Vec<String> {
    let mut replacement_entities = Vec::new();
    replacement.evaluate(&mut |term| {
        if let super::ast::ConstraintTerm::Spatial { entity_id, .. } = term {
            replacement_entities.push(entity_id.clone());
        }
        true
    });
    replacement_entities.sort();
    replacement_entities.dedup();

    parent
        .branches
        .iter()
        .filter(|branch| branch_ids.contains(&branch.branch_id))
        .flat_map(|branch| {
            branch.predicate_bindings.iter().filter_map(|binding| {
                if binding.family != PredicateFamily::Spatial
                    || binding.polarity != super::ast::PredicatePolarity::Positive
                {
                    return None;
                }
                match branch.predicates.term_at_path(&binding.path) {
                    Some(super::ast::ConstraintTerm::Spatial { entity_id, .. })
                        if replacement_entities.contains(entity_id) =>
                    {
                        Some(binding.predicate_id.clone())
                    }
                    _ => None,
                }
            })
        })
        .collect()
}

fn typed_clarification(operation: SearchRevisionOperation) -> TypedSearchRevision {
    TypedSearchRevision {
        operation,
        outcome: SearchRevisionOutcome::RequireClarification,
        patches: Vec::new(),
    }
}

/// Render buyer-visible journey text. This string is presentation-only;
/// execution always consumes the patched `CompiledSearchPlan`.
pub fn render_revision_active_query(
    parent_active_query: &str,
    utterance: &str,
    operation: SearchRevisionOperation,
) -> String {
    let utterance = utterance.trim();
    if matches!(
        operation,
        SearchRevisionOperation::Replace | SearchRevisionOperation::Rephrase
    ) {
        return utterance.to_string();
    }
    let utterance = if operation == SearchRevisionOperation::Expand {
        strip_configured_prefix(
            utterance,
            &search_parser_config().discourse.revision_expand_prefixes,
        )
        .unwrap_or(utterance)
    } else {
        utterance
    };
    let separator = if operation == SearchRevisionOperation::Expand {
        " or "
    } else {
        " · "
    };
    format!("{parent_active_query}{separator}{utterance}")
}

fn merge_revision_intent(parent: &SearchIntent, fragment: &SearchIntent) -> SearchIntent {
    let mut merged = parent.clone();
    for preference in &fragment.positive_preferences {
        if !merged.positive_preferences.contains(preference) {
            merged.positive_preferences.push(preference.clone());
        }
    }
    for preference in &fragment.negative_preferences {
        if !merged.negative_preferences.contains(preference) {
            merged.negative_preferences.push(preference.clone());
        }
    }
    for priority in &fragment.ranking_priorities {
        if !merged.ranking_priorities.contains(priority) {
            merged.ranking_priorities.push(priority.clone());
        }
    }
    merged.positive_preferences.sort_by_key(|preference| {
        super::schema::positive_preference_patterns()
            .iter()
            .position(|pattern| pattern.label.eq_ignore_ascii_case(&preference.raw_text))
            .unwrap_or(usize::MAX)
    });
    merged.negative_preferences.sort_by_key(|preference| {
        super::schema::negative_preference_patterns()
            .iter()
            .position(|pattern| pattern.label.eq_ignore_ascii_case(&preference.raw_text))
            .unwrap_or(usize::MAX)
    });
    merged
}

fn targets_every_branch(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["all", "both", "every"]
        .iter()
        .any(|target| lower.split_whitespace().any(|word| word == *target))
}

fn targeted_branch_index(value: &str, branch_count: usize) -> Option<usize> {
    let tokens = super::parser::query_tokens(value);
    search_parser_config()
        .discourse
        .branch_ordinals
        .iter()
        .enumerate()
        .find(|(index, ordinal)| {
            *index < branch_count
                && tokens
                    .iter()
                    .any(|token| token.eq_ignore_ascii_case(ordinal))
        })
        .map(|(index, _)| index)
}

fn strip_configured_prefix<'a>(value: &'a str, prefixes: &[String]) -> Option<&'a str> {
    prefixes
        .iter()
        .filter_map(|prefix| strip_prefix_case_insensitive(value, prefix))
        .min_by_key(|remainder| remainder.len())
        .map(|remainder| remainder.trim_start_matches([',', ':', '-', ' ']))
}

fn contains_configured_phrase(value: &str, phrases: &[String]) -> bool {
    let tokens = super::parser::query_tokens(value);
    phrases.iter().any(|phrase| {
        let phrase_tokens = super::parser::query_tokens(phrase);
        !phrase_tokens.is_empty()
            && tokens.windows(phrase_tokens.len()).any(|window| {
                window
                    .iter()
                    .zip(&phrase_tokens)
                    .all(|(token, expected)| token.eq_ignore_ascii_case(expected))
            })
    })
}

fn strip_prefix_case_insensitive<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .and_then(|_| value.get(prefix.len()..))
}

pub fn intent_breakdown(plan: &CompiledSearchPlan) -> Vec<BuyerIntentBranchProjection> {
    plan.branches
        .iter()
        .map(|branch| {
            let mut predicates = Vec::new();
            collect_intent_breakdown(
                &branch.predicates,
                false,
                &mut Vec::new(),
                &branch.predicate_bindings,
                &mut predicates,
            );
            BuyerIntentBranchProjection {
                id: branch.branch_id.clone(),
                predicates,
            }
        })
        .collect()
}

fn collect_intent_breakdown(
    expression: &ConstraintExpr,
    negated: bool,
    path: &mut Vec<usize>,
    bindings: &[CompiledPredicateBinding],
    output: &mut Vec<BuyerIntentPredicateProjection>,
) {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            for (index, clause) in clauses.iter().enumerate() {
                path.push(index);
                collect_intent_breakdown(clause, negated, path, bindings, output);
                path.pop();
            }
        }
        ConstraintExpr::Not { clause } => {
            path.push(0);
            collect_intent_breakdown(clause, !negated, path, bindings, output);
            path.pop();
        }
        ConstraintExpr::Term { term } => {
            let id = bindings
                .iter()
                .find(|binding| binding.path == *path)
                .map(|binding| binding.predicate_id.clone())
                .unwrap_or_else(|| {
                    format!(
                        "predicate:{}",
                        path.iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(".")
                    )
                });
            let polarity = if negated { "negative" } else { "positive" }.to_string();
            let projection = match term {
                super::ast::ConstraintTerm::Bhk { value, .. } => BuyerIntentPredicateProjection {
                    id,
                    dimension: "bhk".to_string(),
                    polarity,
                    operator: "equals".to_string(),
                    value: json!(value),
                    unit: None,
                    resolved_label: None,
                    required: true,
                },
                super::ast::ConstraintTerm::Area { value, .. } => BuyerIntentPredicateProjection {
                    id,
                    dimension: "area".to_string(),
                    polarity,
                    operator: "inside".to_string(),
                    value: json!(value),
                    unit: None,
                    resolved_label: Some(value.clone()),
                    required: true,
                },
                super::ast::ConstraintTerm::Society { display_name, .. } => {
                    BuyerIntentPredicateProjection {
                        id,
                        dimension: "society".to_string(),
                        polarity,
                        operator: "equals".to_string(),
                        value: json!(display_name),
                        unit: None,
                        resolved_label: Some(display_name.clone()),
                        required: true,
                    }
                }
                super::ast::ConstraintTerm::Builder { display_name, .. } => {
                    BuyerIntentPredicateProjection {
                        id,
                        dimension: "builder".to_string(),
                        polarity,
                        operator: "equals".to_string(),
                        value: json!(display_name),
                        unit: None,
                        resolved_label: Some(display_name.clone()),
                        required: true,
                    }
                }
                super::ast::ConstraintTerm::Budget { min, max, .. } => {
                    BuyerIntentPredicateProjection {
                        id,
                        dimension: "price".to_string(),
                        polarity,
                        operator: match (min, max) {
                            (Some(_), Some(_)) => "between",
                            (Some(_), None) => "at_least",
                            (None, Some(_)) => "at_most",
                            (None, None) => "unknown",
                        }
                        .to_string(),
                        value: json!({
                            "min": min.as_ref().map(|bound| bound.value),
                            "max": max.as_ref().map(|bound| bound.value),
                        }),
                        unit: Some("INR".to_string()),
                        resolved_label: None,
                        required: true,
                    }
                }
                super::ast::ConstraintTerm::Evidence { constraint, .. } => {
                    BuyerIntentPredicateProjection {
                        id,
                        dimension: constraint.field.clone(),
                        polarity,
                        operator: match constraint.operator {
                            super::intent::ConstraintOperator::Min => "at_least",
                            super::intent::ConstraintOperator::Max => "at_most",
                        }
                        .to_string(),
                        value: json!(constraint.value),
                        unit: Some(constraint.unit.clone()),
                        resolved_label: None,
                        required: true,
                    }
                }
                super::ast::ConstraintTerm::Spatial {
                    relation,
                    display_name,
                    required,
                    ..
                } => BuyerIntentPredicateProjection {
                    id,
                    dimension: "geography".to_string(),
                    polarity,
                    operator: relation.clone(),
                    value: json!(display_name),
                    unit: None,
                    resolved_label: Some(display_name.clone()),
                    required: *required,
                },
            };
            output.push(projection);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn issue_signed_search_context(
    parent_revision_id: Option<String>,
    client_mutation_id: &str,
    operation: SearchRevisionOperation,
    depth: usize,
    active_query: String,
    plan: CompiledSearchPlan,
    runtime_version: SearchRuntimeVersion,
    ordered_result_ids: Vec<String>,
) -> Result<(SignedSearchContext, SearchRevisionDescriptor), String> {
    let result_fingerprint = result_membership_fingerprint(&ordered_result_ids);
    let revision_id = revision_id_for_plan(
        parent_revision_id.as_deref(),
        client_mutation_id,
        operation,
        &plan.semantic_fingerprint,
        &runtime_version,
        depth,
    );
    let intent_ast = PortableIntentAst::from_plan(&plan);
    let context = SignedSearchContext {
        version: 1,
        revision_id: revision_id.clone(),
        parent_revision_id: parent_revision_id.clone(),
        operation,
        depth,
        active_query: active_query.clone(),
        semantic_fingerprint: plan.semantic_fingerprint.clone(),
        runtime_lineage: runtime_version,
        intent_ast,
        result_fingerprint,
    };
    validate_signed_search_context(&context)?;
    let encoded = encode_signed_context(&context)?;
    let descriptor = SearchRevisionDescriptor {
        id: revision_id,
        parent_id: parent_revision_id,
        operation,
        depth,
        active_query,
        semantic_fingerprint: context.semantic_fingerprint.clone(),
        state_token: encoded,
        intent_breakdown: intent_breakdown(&plan),
    };
    Ok((context, descriptor))
}

pub fn decode_signed_search_context(value: &str) -> Result<SignedSearchContext, String> {
    if value.len() > MAX_ENCODED_TOKEN_BYTES {
        return Err("revision token exceeds the encoded size limit".to_string());
    }
    let mut parts = value.split('.');
    if parts.next() != Some("v1") {
        return Err("unsupported revision context version".to_string());
    }
    let payload_hex = parts
        .next()
        .ok_or_else(|| "missing revision context payload".to_string())?;
    let signature = parts
        .next()
        .ok_or_else(|| "missing revision context signature".to_string())?;
    if parts.next().is_some() {
        return Err("invalid revision context framing".to_string());
    }
    let payload = decode_hex(payload_hex)?;
    let signature = decode_hex(signature)?;
    verify_hmac(revision_signing_key(), &payload, &signature)?;
    let context: SignedSearchContext = serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid revision context payload: {error}"))?;
    validate_signed_search_context(&context)?;
    Ok(context)
}

fn encode_signed_context(context: &SignedSearchContext) -> Result<String, String> {
    let payload = serde_json::to_vec(context).expect("revision context is serializable");
    let signature = hmac_sha256(revision_signing_key(), &payload);
    let encoded = format!("v1.{}.{}", encode_hex(&payload), encode_hex(&signature));
    if encoded.len() > MAX_ENCODED_TOKEN_BYTES {
        return Err("revision token exceeds the encoded size limit".to_string());
    }
    Ok(encoded)
}

fn validate_signed_search_context(context: &SignedSearchContext) -> Result<(), String> {
    if context.version != 1 || context.intent_ast.version != 1 {
        return Err("unsupported revision token payload version".to_string());
    }
    if context.intent_ast.branches.is_empty() || context.intent_ast.branches.len() > 8 {
        return Err("revision intent branch limit exceeded".to_string());
    }
    let predicate_count = context
        .intent_ast
        .branches
        .iter()
        .map(|branch| branch.predicate_bindings.len())
        .sum::<usize>();
    if predicate_count > 64 {
        return Err("revision intent predicate limit exceeded".to_string());
    }
    Ok(())
}

fn revision_id_for_plan(
    parent_revision_id: Option<&str>,
    client_mutation_id: &str,
    operation: SearchRevisionOperation,
    plan_fingerprint: &str,
    runtime_version: &SearchRuntimeVersion,
    depth: usize,
) -> String {
    let payload = serde_json::to_vec(&json!({
        "parentRevisionId": parent_revision_id,
        "clientMutationId": client_mutation_id,
        "operation": operation,
        "planFingerprint": plan_fingerprint,
        "runtimeVersion": runtime_version,
    }))
    .expect("revision identity payload is serializable");
    let digest = hmac_sha256(revision_signing_key(), &payload);
    format!("rev-{depth:03}-{}", &encode_hex(&digest)[..32])
}

pub fn result_membership_fingerprint(ids: &[String]) -> String {
    let digest =
        Sha256::digest(serde_json::to_vec(ids).expect("result membership is JSON serializable"));
    format!("sha256:{}", encode_hex(&digest))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("invalid revision context encoding".to_string());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "invalid revision context encoding".to_string())
        })
        .collect()
}

const REVISION_SIGNING_KEY_ENV: &str = "OPENESTATES_REVISION_SIGNING_KEY";
const MAX_ENCODED_TOKEN_BYTES: usize = 64 * 1024;
type HmacSha256 = Hmac<Sha256>;

fn revision_signing_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        if let Ok(configured) = std::env::var(REVISION_SIGNING_KEY_ENV) {
            if !configured.trim().is_empty() {
                return Sha256::digest(configured.as_bytes()).into();
            }
        }
        if cfg!(debug_assertions) {
            return Sha256::digest(b"openestates-local-revision-signing-key-v1").into();
        }
        panic!("OPENESTATES_REVISION_SIGNING_KEY is required for revision token continuity");
    })
}

fn hmac_sha256(key: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

fn verify_hmac(key: &[u8], payload: &[u8], signature: &[u8]) -> Result<(), String> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(payload);
    mac.verify_slice(signature)
        .map_err(|_| "invalid revision context signature".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::ast::{ConstraintExpr, ConstraintTerm};

    const LIMITS: SearchRevisionLimits = SearchRevisionLimits {
        max_active_branches: 8,
    };

    fn test_plan(compiled_query: super::super::IntentAst) -> CompiledSearchPlan {
        CompiledSearchPlan::compile_for_snapshot(
            compiled_query,
            "test-snapshot",
            &[],
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
    }

    #[test]
    fn typed_overwrite_replaces_bhk_and_budget_without_dropping_parent_area() {
        let parent_query = "3BHK in Whitefield under 2.5Cr";
        let base = super::super::IntentAst::from_text(parent_query);
        let parent = test_plan(super::super::IntentAst::with_constraints(
            parent_query,
            ConstraintExpr::and(vec![
                base.constraints,
                ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: Some("area:whitefield".to_string()),
                    value: "Whitefield".to_string(),
                    span: None,
                }),
            ]),
            base.intent,
        ));
        let mut fragment = test_plan(super::super::IntentAst::from_text(
            "Make it 2BHK under 1.6Cr",
        ));
        fragment.assign_source_turn("turn-2");
        let parent_predicate_ids = parent.branches[0]
            .predicate_bindings
            .iter()
            .map(|binding| (binding.family, binding.predicate_id.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "Make it 2BHK under 1.6Cr",
            "rev-001-parent",
            LIMITS,
        );
        let candidate = apply_typed_revision(
            &parent,
            &fragment,
            &revision,
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .expect("typed patch applies");
        let predicates = &candidate.branches[0].predicates;
        assert!(predicates.contains_family(PredicateFamily::Area));
        assert_eq!(candidate.branches[0].ranking_intent.requested_bhks(), [2]);
        assert_eq!(
            candidate.branches[0].ranking_intent.budget_max,
            Some(16_000_000)
        );
        assert!(candidate.branches[0]
            .source_spans
            .iter()
            .any(|span| span.source_turn_id == "turn-2"));
        for family in [
            PredicateFamily::Bhk,
            PredicateFamily::Budget,
            PredicateFamily::Area,
        ] {
            let candidate_id = candidate.branches[0]
                .predicate_bindings
                .iter()
                .find(|binding| binding.family == family)
                .map(|binding| binding.predicate_id.as_str());
            assert_eq!(
                candidate_id,
                parent_predicate_ids.get(&family).map(String::as_str),
                "{family:?} keeps its stable predicate id"
            );
        }
    }

    #[test]
    fn typed_correction_targets_the_configured_eighth_branch() {
        let query = (1..=8)
            .map(|index| format!("{index}BHK under 2Cr"))
            .collect::<Vec<_>>()
            .join(" or ");
        let parent = test_plan(super::super::IntentAst::from_text(&query));
        let fragment = test_plan(super::super::IntentAst::from_text(
            "Increase the eighth budget to 3Cr",
        ));
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "Increase the eighth budget to 3Cr",
            "rev-001-parent",
            LIMITS,
        );
        assert!(matches!(
            revision.patches.as_slice(),
            [TypedSearchRevisionPatch::ReplacePredicate { branch_ids, families, .. }]
                if branch_ids == &["branch-8"] && families == &[PredicateFamily::Budget]
        ));
    }

    #[test]
    fn distinct_spatial_refinement_adds_without_replacing_existing_anchor() {
        let parent = test_plan(super::super::IntentAst::with_constraints(
            "within 1 km of School A",
            ConstraintExpr::term(ConstraintTerm::Spatial {
                relation: "near".to_string(),
                entity_id: "place:school-a".to_string(),
                display_name: "School A".to_string(),
                required: true,
                category_fact_keys: Vec::new(),
                distance_limit_km: Some(1.0),
                span: None,
            }),
            SearchIntent::default(),
        ));
        let fragment = test_plan(super::super::IntentAst::with_constraints(
            "and within 2 km of Metro B",
            ConstraintExpr::term(ConstraintTerm::Spatial {
                relation: "near".to_string(),
                entity_id: "place:metro-b".to_string(),
                display_name: "Metro B".to_string(),
                required: true,
                category_fact_keys: Vec::new(),
                distance_limit_km: Some(2.0),
                span: None,
            }),
            SearchIntent::default(),
        ));
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "and within 2 km of Metro B",
            "rev-spatial-add",
            LIMITS,
        );
        let candidate = apply_typed_revision(
            &parent,
            &fragment,
            &revision,
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .unwrap();
        let mut entities = Vec::new();
        candidate.branches[0].predicates.evaluate(&mut |term| {
            if let ConstraintTerm::Spatial { entity_id, .. } = term {
                entities.push(entity_id.clone());
            }
            true
        });
        assert_eq!(entities, ["place:school-a", "place:metro-b"]);
    }

    #[test]
    fn spatial_correction_replaces_only_the_matching_predicate_identity() {
        let parent = test_plan(super::super::IntentAst::with_constraints(
            "near School A and Metro B",
            ConstraintExpr::and(vec![
                ConstraintExpr::term(ConstraintTerm::Spatial {
                    relation: "near".to_string(),
                    entity_id: "place:school-a".to_string(),
                    display_name: "School A".to_string(),
                    required: true,
                    category_fact_keys: Vec::new(),
                    distance_limit_km: Some(1.0),
                    span: None,
                }),
                ConstraintExpr::term(ConstraintTerm::Spatial {
                    relation: "near".to_string(),
                    entity_id: "place:metro-b".to_string(),
                    display_name: "Metro B".to_string(),
                    required: true,
                    category_fact_keys: Vec::new(),
                    distance_limit_km: Some(1.0),
                    span: None,
                }),
            ]),
            SearchIntent::default(),
        ));
        let previous_metro_id = parent.branches[0]
            .predicate_bindings
            .iter()
            .find(|binding| binding.semantic_key.contains("place:metro-b"))
            .unwrap()
            .predicate_id
            .clone();
        let fragment = test_plan(super::super::IntentAst::with_constraints(
            "change the Metro B distance to 2 km",
            ConstraintExpr::term(ConstraintTerm::Spatial {
                relation: "near".to_string(),
                entity_id: "place:metro-b".to_string(),
                display_name: "Metro B".to_string(),
                required: true,
                category_fact_keys: Vec::new(),
                distance_limit_km: Some(2.0),
                span: None,
            }),
            SearchIntent::default(),
        ));
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "change the Metro B distance to 2 km",
            "rev-spatial-correct",
            LIMITS,
        );
        let candidate = apply_typed_revision(
            &parent,
            &fragment,
            &revision,
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .unwrap();
        let mut spatial = Vec::new();
        candidate.branches[0].predicates.evaluate(&mut |term| {
            if let ConstraintTerm::Spatial {
                entity_id,
                distance_limit_km,
                ..
            } = term
            {
                spatial.push((entity_id.clone(), *distance_limit_km));
            }
            true
        });
        assert_eq!(
            spatial,
            [
                ("place:school-a".to_string(), Some(1.0)),
                ("place:metro-b".to_string(), Some(2.0)),
            ]
        );
        assert_eq!(
            candidate.branches[0]
                .predicate_bindings
                .iter()
                .find(|binding| binding.semantic_key.contains("place:metro-b"))
                .unwrap()
                .predicate_id,
            previous_metro_id
        );
    }

    #[test]
    fn expansion_applies_every_compiled_alternative_atomically() {
        let parent = test_plan(super::super::IntentAst::from_text("3BHK under 2.5Cr"));
        let fragment = test_plan(super::super::IntentAst::from_text(
            "2BHK under 2Cr or 3BHK under 3Cr",
        ));
        assert_eq!(fragment.branches.len(), 2);
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "Also consider 2BHK under 2Cr or 3BHK under 3Cr",
            "rev-multi-expand",
            LIMITS,
        );
        assert_eq!(revision.patches.len(), 2);
        let candidate = apply_typed_revision(
            &parent,
            &fragment,
            &revision,
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .unwrap();
        assert_eq!(candidate.branches.len(), 3);
    }

    #[test]
    fn short_multi_family_followup_preserves_parent_context_without_reset_phrase() {
        let base = super::super::IntentAst::from_text("3BHK in Hoodi under 2.4Cr");
        let parent = test_plan(super::super::IntentAst::with_constraints(
            "3BHK in Hoodi under 2.4Cr",
            ConstraintExpr::and(vec![
                base.constraints,
                ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: Some("area:hoodi".to_string()),
                    value: "Hoodi".to_string(),
                    span: None,
                }),
            ]),
            base.intent,
        ));
        let fragment = test_plan(super::super::IntentAst::from_text("2BHK under 1.8Cr"));
        let revision = compile_typed_revision(
            &parent,
            &fragment,
            "2BHK under 1.8Cr",
            "rev-context",
            LIMITS,
        );
        assert_eq!(revision.operation, SearchRevisionOperation::Refine);
        let candidate = apply_typed_revision(
            &parent,
            &fragment,
            &revision,
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .unwrap();
        assert!(candidate.branches[0]
            .predicates
            .contains_family(PredicateFamily::Area));
        assert_eq!(candidate.branches[0].ranking_intent.requested_bhks(), [2]);
        assert_eq!(
            candidate.branches[0].ranking_intent.budget_max,
            Some(18_000_000)
        );
    }
}
