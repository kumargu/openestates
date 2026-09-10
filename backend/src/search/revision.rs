use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::dag_config::search_parser_config;
use crate::serving::SpatialServingIndex;

use super::ast::{ConstraintExpr, PredicateFamily};
use super::compiled_plan::{
    CompiledPredicateBinding, CompiledSearchPlan, GeoCellSearchPolicy, ResolvedEntityHandle,
};
use super::intent::{SearchIntent, SourceSpan};
use super::tokens::{decode_signed, encode_hex, encode_signed, tagged_digest};
use super::SearchRuntimeVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchRevisionOperation {
    Initial,
    Resume,
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
    ReplacePreference {
        branch_ids: Vec<String>,
        preference_ids: Vec<String>,
        preferences: Vec<super::intent::PreferenceSignal>,
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
    pub buyer_brief: String,
    pub semantic_fingerprint: String,
    pub runtime_lineage: SearchRuntimeVersion,
    pub intent_ast: TypedIntentAst,
    pub result_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypedIntentAst {
    pub version: u32,
    pub root: super::compiled_plan::BoolExpr<String>,
    pub branches: Vec<TypedIntentAstBranch>,
    pub aggregate_intent: SearchIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypedIntentAstBranch {
    pub branch_id: String,
    pub predicates: ConstraintExpr,
    pub predicate_bindings: Vec<CompiledPredicateBinding>,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
    pub ranking_intent: SearchIntent,
}

impl TypedIntentAst {
    pub fn from_plan(plan: &CompiledSearchPlan) -> Self {
        Self {
            version: 1,
            root: plan.root.clone(),
            branches: plan
                .branches
                .iter()
                .map(|branch| TypedIntentAstBranch {
                    branch_id: branch.branch_id.clone(),
                    predicates: branch.predicates.clone(),
                    predicate_bindings: branch.predicate_bindings.clone(),
                    resolved_entities: branch.resolved_entities.clone(),
                    ranking_intent: branch.ranking_intent.clone(),
                })
                .collect(),
            aggregate_intent: plan.aggregate_intent.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IssuedSearchContext {
    pub context: SignedSearchContext,
    pub state_token: String,
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
    let fragment_ranking_intent = fragment
        .branches
        .first()
        .map(|branch| &branch.ranking_intent)
        .unwrap_or(&fragment.aggregate_intent);
    let mut branch_ranking_intents = parent
        .branches
        .iter()
        .map(|branch| (branch.branch_id.clone(), branch.ranking_intent.clone()))
        .collect::<HashMap<_, _>>();
    for patch in &revision.patches {
        match patch {
            TypedSearchRevisionPatch::AddPredicate {
                branch_ids,
                expression,
            } => {
                for (branch_id, predicates) in &mut branches {
                    if branch_ids.contains(branch_id) {
                        *predicates = conjoin(predicates.clone(), expression.clone());
                        if let Some(ranking_intent) = branch_ranking_intents.get_mut(branch_id) {
                            *ranking_intent =
                                merge_revision_intent(ranking_intent, fragment_ranking_intent);
                        }
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
                        if let Some(ranking_intent) = branch_ranking_intents.get_mut(branch_id) {
                            *ranking_intent =
                                merge_revision_intent(ranking_intent, fragment_ranking_intent);
                        }
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
            TypedSearchRevisionPatch::ReplacePreference {
                branch_ids,
                preference_ids,
                preferences,
            } => {
                for branch_id in branch_ids {
                    let Some(ranking_intent) = branch_ranking_intents.get_mut(branch_id) else {
                        continue;
                    };
                    ranking_intent.positive_preferences.retain(|preference| {
                        !preference_ids.contains(&ranking_preference_id(branch_id, preference))
                    });
                    ranking_intent.negative_preferences.retain(|preference| {
                        !preference_ids.contains(&ranking_preference_id(branch_id, preference))
                    });
                    for preference in preferences {
                        let target = match preference.polarity {
                            super::intent::Polarity::Positive => {
                                &mut ranking_intent.positive_preferences
                            }
                            super::intent::Polarity::Negative => {
                                &mut ranking_intent.negative_preferences
                            }
                        };
                        if !target.contains(preference) {
                            target.push(preference.clone());
                        }
                    }
                    ranking_intent.ranking_priorities.retain(|priority| {
                        ranking_intent
                            .positive_preferences
                            .iter()
                            .chain(ranking_intent.negative_preferences.iter())
                            .any(|preference| preference.raw_text.eq_ignore_ascii_case(priority))
                    });
                    for priority in &fragment_ranking_intent.ranking_priorities {
                        if !ranking_intent
                            .ranking_priorities
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(priority))
                        {
                            ranking_intent.ranking_priorities.push(priority.clone());
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
                let inherited_ranking = parent
                    .branches
                    .first()
                    .map(|branch| &branch.ranking_intent)
                    .unwrap_or(&parent.aggregate_intent);
                branch_ranking_intents.insert(
                    branch_id.clone(),
                    merge_revision_intent(inherited_ranking, fragment_ranking_intent),
                );
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
            TypedSearchRevisionPatch::ReplacePreference { .. } => {}
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
    for branch in &mut recompiled.branches {
        let Some(portable_ranking) = branch_ranking_intents.get(&branch.branch_id) else {
            continue;
        };
        let mut ranking_intent = branch.ranking_intent.clone();
        restore_preference_semantics(&mut ranking_intent, portable_ranking);
        branch.restore_portable_ranking_intent(ranking_intent);
    }
    retain_replaced_predicate_ids(parent, &mut recompiled, &revision.patches);
    Some(recompiled)
}

fn restore_preference_semantics(target: &mut SearchIntent, source: &SearchIntent) {
    target.preferences.clone_from(&source.preferences);
    target
        .positive_preferences
        .clone_from(&source.positive_preferences);
    target
        .negative_preferences
        .clone_from(&source.negative_preferences);
    target
        .ranking_priorities
        .clone_from(&source.ranking_priorities);
    target
        .accepted_tradeoffs
        .clone_from(&source.accepted_tradeoffs);
    target.buyer_archetype.clone_from(&source.buyer_archetype);
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

pub fn ranking_preference_id(
    branch_id: &str,
    preference: &super::intent::PreferenceSignal,
) -> String {
    let payload = serde_json::to_string(&json!({
        "polarity": preference.polarity,
        "label": preference.raw_text,
        "keys": preference.expanded_keys,
        "required": preference.required,
    }))
    .expect("preference identity serializes");
    let digest = Sha256::digest(format!("preference\0{branch_id}\0{payload}").as_bytes());
    let short = encode_hex(&digest[..12]);
    format!("preference:{branch_id}:{short}")
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

#[allow(clippy::too_many_arguments)]
pub fn issue_signed_search_context(
    parent_revision_id: Option<String>,
    client_mutation_id: &str,
    operation: SearchRevisionOperation,
    depth: usize,
    buyer_brief: String,
    plan: CompiledSearchPlan,
    runtime_version: SearchRuntimeVersion,
    ordered_result_ids: Vec<String>,
) -> Result<IssuedSearchContext, String> {
    let result_fingerprint = result_membership_fingerprint(&ordered_result_ids);
    let revision_id = revision_id_for_plan(
        parent_revision_id.as_deref(),
        client_mutation_id,
        operation,
        &plan.semantic_fingerprint,
        &runtime_version,
        depth,
    );
    let intent_ast = TypedIntentAst::from_plan(&plan);
    let context = SignedSearchContext {
        version: 1,
        revision_id: revision_id.clone(),
        parent_revision_id: parent_revision_id.clone(),
        operation,
        depth,
        buyer_brief: buyer_brief.clone(),
        semantic_fingerprint: plan.semantic_fingerprint.clone(),
        runtime_lineage: runtime_version,
        intent_ast,
        result_fingerprint,
    };
    validate_signed_search_context(&context)?;
    let state_token = encode_signed_context(&context)?;
    Ok(IssuedSearchContext {
        context,
        state_token,
    })
}

pub fn decode_signed_search_context(value: &str) -> Result<SignedSearchContext, String> {
    let max_encoded_bytes = crate::security::security_tuning()
        .search_journey
        .revision_token_max_bytes;
    if value.len() > max_encoded_bytes {
        return Err("revision token exceeds the encoded size limit".to_string());
    }
    let context: SignedSearchContext =
        decode_signed(REVISION_TOKEN_PURPOSE, value, max_encoded_bytes)?;
    validate_signed_search_context(&context)?;
    Ok(context)
}

fn encode_signed_context(context: &SignedSearchContext) -> Result<String, String> {
    encode_signed(
        REVISION_TOKEN_PURPOSE,
        context,
        crate::security::security_tuning()
            .search_journey
            .revision_token_max_bytes,
    )
}

fn validate_signed_search_context(context: &SignedSearchContext) -> Result<(), String> {
    if context.version != 1 || context.intent_ast.version != 1 {
        return Err("unsupported revision token payload version".to_string());
    }
    if context.intent_ast.branches.is_empty()
        || context.intent_ast.branches.len()
            > crate::dag_config::search_guardrail_config()
                .revisions
                .max_active_branches
    {
        return Err("revision intent branch limit exceeded".to_string());
    }
    let branch_ids = context
        .intent_ast
        .branches
        .iter()
        .map(|branch| branch.branch_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if branch_ids.len() != context.intent_ast.branches.len()
        || !portable_root_is_valid(&context.intent_ast.root, &branch_ids)
    {
        return Err("revision intent root is invalid".to_string());
    }
    let predicate_count = context
        .intent_ast
        .branches
        .iter()
        .map(|branch| branch.predicate_bindings.len())
        .sum::<usize>();
    if predicate_count
        > crate::security::security_tuning()
            .search_journey
            .max_intent_predicates
    {
        return Err("revision intent predicate limit exceeded".to_string());
    }
    Ok(())
}

fn portable_root_is_valid(
    root: &super::compiled_plan::BoolExpr<String>,
    branch_ids: &std::collections::HashSet<&str>,
) -> bool {
    match root {
        super::compiled_plan::BoolExpr::All(clauses)
        | super::compiled_plan::BoolExpr::Any(clauses) => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| portable_root_is_valid(clause, branch_ids))
        }
        super::compiled_plan::BoolExpr::Not(clause) => portable_root_is_valid(clause, branch_ids),
        super::compiled_plan::BoolExpr::Leaf(branch_id) => branch_ids.contains(branch_id.as_str()),
    }
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
    let digest = tagged_digest(REVISION_ID_PURPOSE, &payload);
    format!("rev-{depth:03}-{}", &encode_hex(&digest)[..32])
}

pub fn result_membership_fingerprint(ids: &[String]) -> String {
    let digest =
        Sha256::digest(serde_json::to_vec(ids).expect("result membership is JSON serializable"));
    format!("sha256:{}", encode_hex(&digest))
}

const REVISION_TOKEN_PURPOSE: &str = "search-revision-v1";
const REVISION_ID_PURPOSE: &str = "search-revision-id-v1";

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
