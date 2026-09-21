use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::discovery::BrowsePropertyCard;
use crate::models::AreaProfile;
use crate::serving::{EvidenceId, EvidenceRef};
use crate::state::SearchRuntimeSnapshot;

use super::ast::{ConstraintExpr, ConstraintTerm};
use super::compiled_plan::{BoolExpr, CompiledPredicateBinding, CompiledSearchPlan, GeoBranch};
use super::intent::{ConstraintOperator, Polarity};
use super::proof::{issue_proof_token, ProofIssueRequest};
use super::{
    SearchExecution, SearchGuidance, SearchResultCard, SearchRuntimeVersion, VerifiedMatch,
};

pub const SEARCH_JOURNEY_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJourneyEnvelope {
    pub contract_version: u32,
    pub runtime_version: SearchRuntimeVersion,
    pub active: SearchJourneyActive,
    pub attempt: SearchJourneyAttempt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJourneyActive {
    pub revision: SearchJourneyRevision,
    pub buyer_brief: String,
    pub latest_utterance: String,
    pub intent: IntentPresentation,
    pub results: SearchJourneyResults,
    pub collections: Vec<super::collections::JourneyCollection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJourneyRevision {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub operation: super::revision::SearchRevisionOperation,
    pub depth: usize,
    pub semantic_fingerprint: String,
    pub result_fingerprint: String,
    pub state_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SearchJourneyResults {
    Current {
        result_sets: Vec<JourneyResultSet>,
        ordered_result_ids: Vec<String>,
        total_matches: usize,
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        area_context: Option<Box<AreaProfile>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        guidance: Option<SearchGuidance>,
    },
    Retained {
        ordered_result_ids: Vec<String>,
        result_fingerprint: String,
    },
}

impl SearchJourneyResults {
    pub fn ordered_result_ids(&self) -> &[String] {
        match self {
            Self::Current {
                ordered_result_ids, ..
            }
            | Self::Retained {
                ordered_result_ids, ..
            } => ordered_result_ids,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JourneyResultSet {
    pub branch_id: String,
    pub label: String,
    pub results: Vec<JourneyResultCard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JourneyResultCard {
    #[serde(flatten)]
    pub card: BrowsePropertyCard,
    pub match_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_state_display: Option<String>,
    pub reasons: Vec<SearchMatchReason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatchReason {
    pub show_on_card: bool,
    pub branch_id: String,
    pub predicate_id: String,
    pub explanation: String,
    pub proof_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchJourneyAttemptKind {
    Initial,
    Revision,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchJourneyOutcome {
    Activated,
    PreservedParent,
    ClarificationRequired,
    LimitReached,
    Resumed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJourneyAttempt {
    pub kind: SearchJourneyAttemptKind,
    pub operation: super::revision::SearchRevisionOperation,
    pub outcome: SearchJourneyOutcome,
    pub catalog_rebased: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_delta: Option<ResultDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_delta: Option<ResultDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempted_intent: Option<IntentPresentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarification: Option<JourneyClarification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_property_consequence: Option<SelectedPropertyConsequence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JourneyClarification {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedPropertyConsequence {
    pub property_id: String,
    pub outcome: SelectedPropertyOutcome,
    pub cause: SelectedPropertyCause,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branch_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_predicate_ids: Vec<String>,
    pub explanation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_references: Vec<SearchMatchReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectedPropertyOutcome {
    Retained,
    Excluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectedPropertyCause {
    CatalogRefresh,
    IntentRefinement,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub retained: Vec<String>,
    pub moved: Vec<ResultMovement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultMovement {
    pub property_id: String,
    pub from: usize,
    pub to: usize,
    pub cause: ResultMovementCause,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultMovementCause {
    CatalogRefresh,
    IntentRefinement,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentPresentation {
    pub root: BoolExpr<String>,
    pub branches: Vec<IntentBranchPresentation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentBranchPresentation {
    pub id: String,
    pub constraints: IntentExpression,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved_requirements: Vec<String>,
    pub preferences: Vec<IntentPreferencePresentation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum IntentExpression {
    All {
        clauses: Vec<IntentExpression>,
    },
    Any {
        clauses: Vec<IntentExpression>,
    },
    Not {
        clause: Box<IntentExpression>,
    },
    Predicate {
        predicate: IntentPredicatePresentation,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentPredicatePresentation {
    pub id: String,
    pub dimension: String,
    pub label: String,
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
pub struct IntentPreferencePresentation {
    pub id: String,
    pub label: String,
    pub polarity: Polarity,
    pub weight: f32,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SearchRevisionTarget {
    Branch {
        branch_id: String,
    },
    Predicate {
        branch_id: String,
        predicate_id: String,
    },
}

pub fn present_intent(plan: &CompiledSearchPlan) -> IntentPresentation {
    IntentPresentation {
        root: plan.root.clone(),
        branches: plan
            .branches
            .iter()
            .map(|branch| IntentBranchPresentation {
                id: branch.branch_id.clone(),
                constraints: present_expression(
                    &branch.predicates,
                    false,
                    &mut Vec::new(),
                    &branch.predicate_bindings,
                ),
                unresolved_requirements: branch.unresolved_requirements().to_vec(),
                preferences: present_preferences(branch),
            })
            .collect(),
    }
}

pub fn buyer_brief(presentation: &IntentPresentation) -> String {
    let config = &crate::dag_config::intent_presentation_config().brief;
    presentation
        .branches
        .iter()
        .map(|branch| {
            let mut seen_preferences = HashSet::new();
            let constraints = std::iter::once(expression_brief(&branch.constraints))
                .chain(branch.unresolved_requirements.iter().cloned())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(&config.all_separator);
            let preferences = branch
                .preferences
                .iter()
                .map(|preference| match preference.polarity {
                    Polarity::Positive => {
                        format!("{}{}", config.positive_preference_prefix, preference.label)
                    }
                    Polarity::Negative => {
                        format!("{}{}", config.negative_preference_prefix, preference.label)
                    }
                })
                .filter(|value| seen_preferences.insert(value.clone()))
                .collect::<Vec<_>>()
                .join(", ");
            match (constraints.is_empty(), preferences.is_empty()) {
                (false, false) => format!(
                    "{constraints}{}{preferences}",
                    config.constraint_preference_separator
                ),
                (false, true) => constraints,
                (true, false) => preferences,
                (true, true) => config.fallback.clone(),
            }
        })
        .collect::<Vec<_>>()
        .join(&config.branch_separator)
}

pub fn project_current_results(
    snapshot: &SearchRuntimeSnapshot,
    execution: &SearchExecution,
    plan: &CompiledSearchPlan,
) -> SearchJourneyResults {
    let branches = plan
        .branches
        .iter()
        .map(|branch| (branch.branch_id.as_str(), branch))
        .collect::<HashMap<_, _>>();
    let result_sets = execution
        .result_sets
        .iter()
        .map(|set| JourneyResultSet {
            branch_id: set.branch_id.clone(),
            label: super::collections::exact_result_set_label(
                snapshot,
                plan,
                &set.branch_id,
                &set.label,
            ),
            results: set
                .results
                .iter()
                .map(|result| {
                    let reasons = branches
                        .get(set.branch_id.as_str())
                        .map(|branch| project_reasons(snapshot, plan, branch, result))
                        .unwrap_or_default();
                    JourneyResultCard {
                        card: snapshot
                            .browse_cards
                            .get(&result.card.id)
                            .expect("eligible search results have a hydrated browse card")
                            .clone(),
                        match_tier: result.match_tier.clone(),
                        home_state_display: result.card.home_state_display.clone(),
                        reasons,
                    }
                })
                .collect(),
        })
        .collect();
    SearchJourneyResults::Current {
        result_sets,
        ordered_result_ids: execution.ordered_result_ids.clone(),
        total_matches: execution.total_matches,
        state: execution.state.clone(),
        area_context: execution.area_context.clone().map(Box::new),
        guidance: execution.search_guidance.clone(),
    }
}

pub fn result_delta(
    parent: &[String],
    candidate: &[String],
    cause: ResultMovementCause,
) -> ResultDelta {
    let parent_positions = parent
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let candidate_positions = candidate
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let retained_ids = parent
        .iter()
        .filter(|id| candidate_positions.contains_key(id.as_str()))
        .collect::<HashSet<_>>();
    let parent_retained_positions = parent
        .iter()
        .filter(|id| retained_ids.contains(id))
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let candidate_retained_positions = candidate
        .iter()
        .filter(|id| retained_ids.contains(id))
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let movement_explanation = match cause {
        ResultMovementCause::CatalogRefresh => {
            &crate::dag_config::intent_presentation_config()
                .journey_messages
                .catalog_movement
        }
        ResultMovementCause::IntentRefinement => {
            &crate::dag_config::intent_presentation_config()
                .journey_messages
                .intent_movement
        }
    };
    ResultDelta {
        added: candidate
            .iter()
            .filter(|id| !parent_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        removed: parent
            .iter()
            .filter(|id| !candidate_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        retained: candidate
            .iter()
            .filter(|id| parent_positions.contains_key(id.as_str()))
            .cloned()
            .collect(),
        moved: candidate
            .iter()
            .filter(|id| {
                parent_retained_positions
                    .get(id.as_str())
                    .zip(candidate_retained_positions.get(id.as_str()))
                    .is_some_and(|(old, new)| old != new)
            })
            .map(|id| ResultMovement {
                property_id: id.clone(),
                from: parent_positions[id.as_str()] + 1,
                to: candidate_positions[id.as_str()] + 1,
                cause,
                explanation: movement_explanation.clone(),
            })
            .collect(),
    }
}

pub fn describe_predicates(
    presentation: &IntentPresentation,
    predicate_ids: &[String],
) -> Vec<String> {
    let wanted = predicate_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut descriptions = Vec::new();
    for branch in &presentation.branches {
        collect_predicate_descriptions(&branch.constraints, &wanted, &mut descriptions);
        descriptions.extend(
            branch
                .preferences
                .iter()
                .filter(|preference| wanted.contains(preference.id.as_str()))
                .map(|preference| preference.label.clone()),
        );
    }
    descriptions
}

fn collect_predicate_descriptions(
    expression: &IntentExpression,
    wanted: &HashSet<&str>,
    descriptions: &mut Vec<String>,
) {
    match expression {
        IntentExpression::All { clauses } | IntentExpression::Any { clauses } => {
            for clause in clauses {
                collect_predicate_descriptions(clause, wanted, descriptions);
            }
        }
        IntentExpression::Not { clause } => {
            collect_predicate_descriptions(clause, wanted, descriptions);
        }
        IntentExpression::Predicate { predicate } if wanted.contains(predicate.id.as_str()) => {
            descriptions.push(signed_predicate_brief(predicate));
        }
        IntentExpression::Predicate { .. } => {}
    }
}

fn present_expression(
    expression: &ConstraintExpr,
    negated: bool,
    path: &mut Vec<usize>,
    bindings: &[CompiledPredicateBinding],
) -> IntentExpression {
    match expression {
        ConstraintExpr::And { clauses } => IntentExpression::All {
            clauses: clauses
                .iter()
                .enumerate()
                .map(|(index, clause)| {
                    path.push(index);
                    let projected = present_expression(clause, negated, path, bindings);
                    path.pop();
                    projected
                })
                .collect(),
        },
        ConstraintExpr::AnyOf { clauses } => IntentExpression::Any {
            clauses: clauses
                .iter()
                .enumerate()
                .map(|(index, clause)| {
                    path.push(index);
                    let projected = present_expression(clause, negated, path, bindings);
                    path.pop();
                    projected
                })
                .collect(),
        },
        ConstraintExpr::Not { clause } => {
            path.push(0);
            let projected = present_expression(clause, !negated, path, bindings);
            path.pop();
            IntentExpression::Not {
                clause: Box::new(projected),
            }
        }
        ConstraintExpr::Term { term } => IntentExpression::Predicate {
            predicate: present_predicate(term, negated, path, bindings),
        },
    }
}

fn present_predicate(
    term: &ConstraintTerm,
    negated: bool,
    path: &[usize],
    bindings: &[CompiledPredicateBinding],
) -> IntentPredicatePresentation {
    let id = bindings
        .iter()
        .find(|binding| binding.path == path)
        .map(|binding| binding.predicate_id.clone())
        .unwrap_or_else(|| format!("predicate:{}", path_key(path)));
    let polarity = if negated { "negative" } else { "positive" }.to_string();
    match term {
        ConstraintTerm::Bhk { value, .. } => IntentPredicatePresentation {
            id,
            dimension: "bhk".to_string(),
            label: presentation_dimension_label("bhk"),
            polarity,
            operator: "equals".to_string(),
            value: json!(value),
            unit: None,
            resolved_label: None,
            required: true,
        },
        ConstraintTerm::Area { value, .. } => IntentPredicatePresentation {
            id,
            dimension: "area".to_string(),
            label: presentation_dimension_label("area"),
            polarity,
            operator: "inside".to_string(),
            value: json!(value),
            unit: None,
            resolved_label: Some(value.clone()),
            required: true,
        },
        ConstraintTerm::Society { display_name, .. } => IntentPredicatePresentation {
            id,
            dimension: "society".to_string(),
            label: presentation_dimension_label("society"),
            polarity,
            operator: "equals".to_string(),
            value: json!(display_name),
            unit: None,
            resolved_label: Some(display_name.clone()),
            required: true,
        },
        ConstraintTerm::Builder { display_name, .. } => IntentPredicatePresentation {
            id,
            dimension: "builder".to_string(),
            label: presentation_dimension_label("builder"),
            polarity,
            operator: "equals".to_string(),
            value: json!(display_name),
            unit: None,
            resolved_label: Some(display_name.clone()),
            required: true,
        },
        ConstraintTerm::Budget { min, max, .. } => IntentPredicatePresentation {
            id,
            dimension: "price".to_string(),
            label: presentation_dimension_label("price"),
            polarity,
            operator: match (min, max) {
                (Some(_), Some(_)) => "between",
                (Some(_), None) => "atLeast",
                (None, Some(_)) => "atMost",
                (None, None) => "unknown",
            }
            .to_string(),
            value: json!({
                "min": min.as_ref().map(|bound| bound.value),
                "max": max.as_ref().map(|bound| bound.value),
            }),
            unit: Some(
                crate::dag_config::intent_presentation_config()
                    .budget_unit
                    .clone(),
            ),
            resolved_label: None,
            required: true,
        },
        ConstraintTerm::Evidence { constraint, .. } => IntentPredicatePresentation {
            id,
            dimension: constraint.field.clone(),
            label: evidence_dimension_label(&constraint.field),
            polarity,
            operator: match constraint.operator {
                ConstraintOperator::Min => "atLeast",
                ConstraintOperator::Max => "atMost",
            }
            .to_string(),
            value: json!(constraint.value),
            unit: Some(constraint.unit.clone()),
            resolved_label: None,
            required: true,
        },
        ConstraintTerm::Spatial {
            relation,
            display_name,
            required,
            distance_limit_km,
            ..
        } => IntentPredicatePresentation {
            id,
            dimension: "geography".to_string(),
            label: presentation_dimension_label("geography"),
            polarity,
            operator: relation.clone(),
            value: distance_limit_km
                .map(|distance| json!({"place": display_name, "distance": distance}))
                .unwrap_or_else(|| json!(display_name)),
            unit: distance_limit_km.map(|_| "km".to_string()),
            resolved_label: Some(display_name.clone()),
            required: *required,
        },
    }
}

fn present_preferences(branch: &GeoBranch) -> Vec<IntentPreferencePresentation> {
    branch
        .ranking_intent
        .positive_preferences
        .iter()
        .chain(branch.ranking_intent.negative_preferences.iter())
        .map(|preference| IntentPreferencePresentation {
            id: super::revision::ranking_preference_id(&branch.branch_id, preference),
            label: preference.raw_text.clone(),
            polarity: preference.polarity.clone(),
            weight: preference.weight,
            required: preference.required,
            priority: branch
                .ranking_intent
                .ranking_priorities
                .iter()
                .position(|priority| priority.eq_ignore_ascii_case(&preference.raw_text))
                .map(|index| index + 1),
        })
        .collect()
}

fn project_reasons(
    snapshot: &SearchRuntimeSnapshot,
    plan: &CompiledSearchPlan,
    branch: &GeoBranch,
    result: &SearchResultCard,
) -> Vec<SearchMatchReason> {
    let mut reasons = Vec::new();
    let mut used = HashSet::<String>::new();
    for verified in &result.verified_matches {
        let Some(binding) = binding_for_verified(branch, verified) else {
            continue;
        };
        let Some(predicate) = predicate_at_path(&branch.predicates, &binding.path) else {
            continue;
        };
        let Some((fact_key, evidence_refs)) = resolvable_evidence(snapshot, verified, predicate)
        else {
            continue;
        };
        let explanation = signed_predicate_brief(&present_predicate(
            predicate,
            binding.polarity == super::ast::PredicatePolarity::Negated,
            &binding.path,
            &branch.predicate_bindings,
        ));
        let Ok(proof_token) = issue_proof_token(ProofIssueRequest {
            constraint: verified.constraint.as_ref(),
            snapshot_identity: snapshot.bundle.manifest.proof_snapshot_identity(),
            semantic_fingerprint: &plan.semantic_fingerprint,
            property_id: &result.card.id,
            branch_id: &branch.branch_id,
            predicate_id: &binding.predicate_id,
            subject_entity_id: &verified.subject_entity_id,
            target_entity_id: verified.target_entity_id.as_deref(),
            fact_key: &fact_key,
            relation: &verified.relation,
            evidence_refs: &evidence_refs,
        }) else {
            continue;
        };
        if used.insert(binding.predicate_id.clone()) {
            reasons.push(SearchMatchReason {
                show_on_card: !crate::dag_config::intent_presentation_config()
                    .card_hidden_dimensions
                    .contains(
                        &present_predicate(
                            predicate,
                            false,
                            &binding.path,
                            &branch.predicate_bindings,
                        )
                        .dimension,
                    ),
                branch_id: branch.branch_id.clone(),
                predicate_id: binding.predicate_id.clone(),
                explanation,
                proof_token,
            });
        }
    }
    if let Some(explanation) = result.match_explanation.as_ref() {
        for matched in &explanation.reasons {
            let (predicate_id, target_entity_id) = reason_identity(branch, matched);
            if used.contains(&predicate_id) {
                continue;
            }
            let Some(identity) = matched.evidence_identity.as_ref() else {
                continue;
            };
            let EvidenceId::Observation(observation_id) = &identity.evidence_id else {
                continue;
            };
            let Some(fact) = snapshot
                .bundle
                .evidence_index
                .fact(observation_id, &matched.fact_key)
                .filter(|fact| fact.entity_id == identity.subject_entity_id)
            else {
                continue;
            };
            let evidence_refs = [EvidenceRef {
                snapshot_identity: snapshot
                    .bundle
                    .manifest
                    .proof_snapshot_identity()
                    .to_string(),
                subject_entity_id: identity.subject_entity_id.clone(),
                evidence_id: identity.evidence_id.clone(),
            }];
            let Ok(proof_token) = issue_proof_token(ProofIssueRequest {
                constraint: None,
                snapshot_identity: snapshot.bundle.manifest.proof_snapshot_identity(),
                semantic_fingerprint: &plan.semantic_fingerprint,
                property_id: &result.card.id,
                branch_id: &branch.branch_id,
                predicate_id: &predicate_id,
                subject_entity_id: &identity.subject_entity_id,
                target_entity_id: target_entity_id.as_deref(),
                fact_key: &fact.fact_key,
                relation: "supports",
                evidence_refs: &evidence_refs,
            }) else {
                continue;
            };
            used.insert(predicate_id.clone());
            reasons.push(SearchMatchReason {
                show_on_card: true,
                branch_id: branch.branch_id.clone(),
                predicate_id,
                explanation: matched.display.clone(),
                proof_token,
            });
        }
    }
    reasons
}

fn binding_for_verified<'a>(
    branch: &'a GeoBranch,
    verified: &VerifiedMatch,
) -> Option<&'a CompiledPredicateBinding> {
    branch.predicate_bindings.iter().find(|binding| {
        let Some(term) = predicate_at_path(&branch.predicates, &binding.path) else {
            return false;
        };
        match term {
            ConstraintTerm::Bhk { value, .. } => {
                verified.metric.contains("bhk") && verified.predicate == format!("{value} BHK")
            }
            ConstraintTerm::Budget { .. } => verified.metric.contains("price"),
            ConstraintTerm::Evidence { constraint, .. } => {
                verified.constraint.as_ref() == Some(constraint)
            }
            ConstraintTerm::Spatial {
                entity_id,
                relation,
                display_name,
                ..
            } => {
                verified.relation.eq_ignore_ascii_case(relation)
                    && if entity_id.is_empty() {
                        verified.predicate.eq_ignore_ascii_case(display_name)
                    } else {
                        verified.target_entity_id.as_deref() == Some(entity_id)
                    }
            }
            _ => false,
        }
    })
}

fn resolvable_evidence(
    snapshot: &SearchRuntimeSnapshot,
    verified: &VerifiedMatch,
    predicate: &ConstraintTerm,
) -> Option<(String, Vec<EvidenceRef>)> {
    let mut fact_key = verified.fact_key.clone();
    for reference in &verified.evidence_refs {
        match &reference.evidence_id {
            EvidenceId::Observation(id) => {
                let exact_fact_key = verified.fact_key.as_deref()?;
                let fact = snapshot.bundle.evidence_index.fact(id, exact_fact_key)?;
                if fact.entity_id != reference.subject_entity_id
                    || fact_key
                        .as_deref()
                        .is_some_and(|existing| !existing.eq_ignore_ascii_case(&fact.fact_key))
                {
                    return None;
                }
                fact_key = Some(fact.fact_key.clone());
            }
            EvidenceId::Derivation(id) => {
                if snapshot.bundle.evidence_index.derivation(id).is_none()
                    && verified
                        .derived_evidence
                        .as_ref()
                        .is_none_or(|derived| derived.derivation_id != *id)
                {
                    return None;
                }
                let configured_key = match predicate {
                    ConstraintTerm::Spatial {
                        category_fact_keys, ..
                    } => category_fact_keys
                        .iter()
                        .find(|key| super::proof::proof_destination_for_fact_key(key).is_some()),
                    _ => None,
                };
                fact_key.get_or_insert_with(|| {
                    configured_key
                        .cloned()
                        .unwrap_or_else(|| verified.metric.clone())
                });
            }
        }
    }
    fact_key.map(|key| (key, verified.evidence_refs.clone()))
}

fn reason_identity(branch: &GeoBranch, reason: &super::MatchReason) -> (String, Option<String>) {
    for binding in &branch.predicate_bindings {
        let Some(ConstraintTerm::Spatial {
            entity_id,
            display_name,
            ..
        }) = predicate_at_path(&branch.predicates, &binding.path)
        else {
            continue;
        };
        if reason
            .preference
            .to_ascii_lowercase()
            .contains(&display_name.to_ascii_lowercase())
        {
            return (binding.predicate_id.clone(), Some(entity_id.clone()));
        }
    }
    let preference = branch
        .ranking_intent
        .positive_preferences
        .iter()
        .chain(branch.ranking_intent.negative_preferences.iter())
        .find(|preference| {
            reason.preference.eq_ignore_ascii_case(&preference.raw_text)
                || reason
                    .preference
                    .strip_prefix("avoid ")
                    .is_some_and(|value| value.eq_ignore_ascii_case(&preference.raw_text))
                || preference
                    .expanded_keys
                    .iter()
                    .any(|key| key.eq_ignore_ascii_case(&reason.fact_key))
        });
    preference.map_or_else(
        || {
            (
                stable_id("preference", &branch.branch_id, &reason.preference),
                None,
            )
        },
        |preference| {
            (
                super::revision::ranking_preference_id(&branch.branch_id, preference),
                None,
            )
        },
    )
}

fn predicate_at_path<'a>(
    expression: &'a ConstraintExpr,
    path: &[usize],
) -> Option<&'a ConstraintTerm> {
    let mut expression = expression;
    for index in path {
        expression = match expression {
            ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
                clauses.get(*index)?
            }
            ConstraintExpr::Not { clause } if *index == 0 => clause,
            ConstraintExpr::Not { .. } | ConstraintExpr::Term { .. } => return None,
        };
    }
    match expression {
        ConstraintExpr::Term { term } => Some(term),
        _ => None,
    }
}

fn expression_brief(expression: &IntentExpression) -> String {
    let config = &crate::dag_config::intent_presentation_config().brief;
    match expression {
        IntentExpression::All { clauses } | IntentExpression::Any { clauses } => {
            let mut seen = HashSet::new();
            let parts = clauses
                .iter()
                .map(expression_brief)
                .filter(|value| !value.is_empty() && seen.insert(value.clone()))
                .collect::<Vec<_>>();
            if matches!(expression, IntentExpression::Any { .. }) && parts.len() > 1 {
                format!("({})", parts.join(&config.any_separator))
            } else {
                parts.join(&config.all_separator)
            }
        }
        IntentExpression::Not { clause } => {
            let value = expression_brief(clause);
            if value.is_empty() {
                String::new()
            } else {
                format!("{}{value}", config.not_prefix)
            }
        }
        IntentExpression::Predicate { predicate } => predicate_brief(predicate),
    }
}

fn presentation_dimension_label(dimension: &str) -> String {
    crate::dag_config::intent_presentation_config()
        .dimension_labels
        .get(dimension)
        .expect("validated intent presentation dimension")
        .clone()
}

fn predicate_brief(predicate: &IntentPredicatePresentation) -> String {
    let config = &crate::dag_config::intent_presentation_config().brief;
    let distance = predicate.value.get("distance");
    let keys = [
        distance.map(|_| format!("{}.{}.distance", predicate.dimension, predicate.operator)),
        distance.map(|_| format!("{}.distance", predicate.operator)),
        Some(format!("{}.{}", predicate.dimension, predicate.operator)),
        Some(predicate.operator.clone()),
        Some("default".to_string()),
    ];
    let template = keys
        .iter()
        .flatten()
        .find_map(|key| config.predicate_templates.get(key))
        .expect("validated predicate template");
    let value = predicate
        .resolved_label
        .clone()
        .unwrap_or_else(|| format_brief_value(&predicate.value, predicate.unit.as_deref()));
    let bound = |name: &str| {
        format_brief_value(
            predicate.value.get(name).unwrap_or(&predicate.value),
            predicate.unit.as_deref(),
        )
    };
    template
        .replace("{label}", &predicate.label)
        .replace("{operator}", &predicate.operator)
        .replace("{value}", &value)
        .replace("{min}", &bound("min"))
        .replace("{max}", &bound("max"))
        .replace(
            "{distance}",
            &distance
                .map(|value| format_brief_value(value, predicate.unit.as_deref()))
                .unwrap_or_default(),
        )
}

fn signed_predicate_brief(predicate: &IntentPredicatePresentation) -> String {
    let value = predicate_brief(predicate);
    if predicate.polarity == "negative" {
        format!(
            "{}{value}",
            crate::dag_config::intent_presentation_config()
                .brief
                .not_prefix
        )
    } else {
        value
    }
}

fn format_brief_value(value: &Value, unit: Option<&str>) -> String {
    let Some(number) = value.as_f64() else {
        return match value {
            Value::String(value) => value.clone(),
            Value::Null => String::new(),
            _ => value.to_string(),
        };
    };
    let config = &crate::dag_config::intent_presentation_config().brief;
    if let Some(format) = unit.and_then(|unit| config.unit_formats.get(unit)) {
        let scale = format
            .scales
            .iter()
            .filter(|scale| number.abs() >= scale.divisor)
            .max_by(|left, right| left.divisor.total_cmp(&right.divisor))
            .unwrap_or_else(|| {
                format
                    .scales
                    .iter()
                    .min_by(|left, right| left.divisor.total_cmp(&right.divisor))
                    .expect("validated unit scales")
            });
        return format!(
            "{}{}{}",
            format.prefix,
            number / scale.divisor,
            scale.suffix
        );
    }
    unit.map_or_else(|| number.to_string(), |unit| format!("{number} {unit}"))
}

fn evidence_dimension_label(dimension: &str) -> String {
    super::schema::registry()
        .numeric_constraints
        .iter()
        .find(|schema| schema.dimension.eq_ignore_ascii_case(dimension))
        .map(|schema| schema.label.clone())
        .unwrap_or_else(|| dimension.replace('_', " "))
}

fn stable_id(kind: &str, owner: &str, payload: &str) -> String {
    let digest = Sha256::digest(format!("{kind}\0{owner}\0{payload}").as_bytes());
    let short = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{kind}:{owner}:{short}")
}

fn path_key(path: &[usize]) -> String {
    path.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{GeoCellSearchPolicy, IntentAst};

    #[test]
    fn nested_intent_presentation_serializes_deterministically() {
        let plan = CompiledSearchPlan::compile_for_snapshot(
            IntentAst::from_text("2 or 3 BHK under 2 Cr, not 4 BHK"),
            "bundle:test",
            &[],
            &crate::graph::GraphIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        );
        let first = serde_json::to_string(&present_intent(&plan)).unwrap();
        let second = serde_json::to_string(&present_intent(&plan)).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\"kind\":\"any\"") || first.contains("\"kind\":\"all\""));
        assert!(first.contains("predicateId") || first.contains("\"id\":\"predicate:"));
    }

    #[test]
    fn result_movement_ignores_index_shifts_from_additions_and_removals() {
        let parent = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let shifted = vec!["new".to_string(), "b".to_string(), "c".to_string()];
        assert!(
            result_delta(&parent, &shifted, ResultMovementCause::CatalogRefresh)
                .moved
                .is_empty()
        );

        let reranked = vec!["c".to_string(), "b".to_string()];
        let delta = result_delta(&parent, &reranked, ResultMovementCause::IntentRefinement);
        assert_eq!(
            delta
                .moved
                .iter()
                .map(|movement| movement.property_id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "b"]
        );
        assert_eq!((delta.moved[0].from, delta.moved[0].to), (3, 1));
    }
}
