use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::OnceLock;

use crate::dag_config::search_parser_config;
use crate::serving::SpatialServingIndex;

use super::ast::{ConstraintExpr, PredicateFamily};
use super::compiled_plan::{
    CompiledSearchPlan, GeoCellSearchPolicy, GeoTopologyIndex, ResolvedEntityHandle,
};
use super::intent::SearchIntent;
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
    Undo,
    Fresh,
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
    pub revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_revision_id: Option<String>,
    pub operation: SearchRevisionOperation,
    pub depth: usize,
    pub active_query: String,
    pub plan_fingerprint: String,
    pub runtime_version: SearchRuntimeVersion,
    pub plan: CompiledSearchPlan,
    pub ordered_result_ids: Vec<String>,
    pub result_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyerIntentBranchProjection {
    pub branch_id: String,
    pub summary: String,
    pub source_spans: Vec<super::intent::SourceSpan>,
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
    pub plan_fingerprint: String,
    pub runtime_version: SearchRuntimeVersion,
    pub buyer_intent: Vec<BuyerIntentBranchProjection>,
    pub context: String,
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
    if discourse
        .revision_undo_phrases
        .iter()
        .any(|phrase| turn.eq_ignore_ascii_case(phrase))
    {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Undo,
            outcome: SearchRevisionOutcome::Candidate,
            patches: Vec::new(),
        };
    }

    let fresh = strip_configured_prefix(turn, &discourse.revision_fresh_prefixes).is_some();
    let expand = strip_configured_prefix(turn, &discourse.revision_expand_prefixes).is_some();
    let correction = strip_configured_prefix(turn, &discourse.revision_correction_prefixes)
        .or_else(|| strip_configured_prefix(turn, &discourse.revision_overwrite_prefixes))
        .is_some();
    let exclusion = strip_configured_prefix(turn, &discourse.revision_exclusion_prefixes).is_some();
    let Some(fragment_branch) = fragment.branches.first() else {
        return typed_clarification(if fresh {
            SearchRevisionOperation::Fresh
        } else {
            SearchRevisionOperation::Refine
        });
    };
    if fresh {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Fresh,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::ReplaceIntent],
        };
    }
    if expand {
        if parent.branches.len() >= limits.max_active_branches {
            return TypedSearchRevision {
                operation: SearchRevisionOperation::Expand,
                outcome: SearchRevisionOutcome::RequireCheckpoint,
                patches: Vec::new(),
            };
        }
        if !fragment_branch.predicates.has_terms() {
            return typed_clarification(SearchRevisionOperation::Expand);
        }
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Expand,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::AddAlternative {
                branch_id: format!("branch:{parent_revision_id}:1"),
                expression: fragment_branch.predicates.clone(),
            }],
        };
    }

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
        || (families.len() >= 2
            && strip_configured_prefix(turn, &discourse.revision_continuity_prefixes).is_none()
            && !correction);
    if explicit_replace {
        return TypedSearchRevision {
            operation: SearchRevisionOperation::Replace,
            outcome: SearchRevisionOutcome::Candidate,
            patches: vec![TypedSearchRevisionPatch::ReplaceIntent],
        };
    }

    let mut replace_families = families
        .iter()
        .copied()
        .filter(|family| is_overwrite_family(family))
        .collect::<Vec<_>>();
    if correction && families.contains(&PredicateFamily::Spatial) {
        replace_families.extend([
            PredicateFamily::Area,
            PredicateFamily::Society,
            PredicateFamily::Spatial,
        ]);
        replace_families.sort_by_key(|family| *family as u8);
        replace_families.dedup();
    }
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
            expression: fragment_branch
                .predicates
                .select_families(&replace_families),
        });
    }
    let add_families = families
        .into_iter()
        .filter(|family| !replace_families.contains(family))
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

pub fn apply_typed_revision(
    parent: &CompiledSearchPlan,
    fragment: &CompiledSearchPlan,
    revision: &TypedSearchRevision,
    topology: &GeoTopologyIndex,
    spatial_index: Option<&SpatialServingIndex>,
    geo_cell_policy: GeoCellSearchPolicy,
) -> Option<CompiledSearchPlan> {
    if revision.outcome != SearchRevisionOutcome::Candidate {
        return None;
    }
    if revision.operation == SearchRevisionOperation::Undo {
        return Some(parent.clone());
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
                expression,
            } => {
                for (branch_id, predicates) in &mut branches {
                    if branch_ids.contains(branch_id) {
                        predicates.remove_positive_families(families);
                        *predicates = conjoin(predicates.clone(), expression.clone());
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
    matches!(
        family,
        PredicateFamily::Bhk | PredicateFamily::Budget | PredicateFamily::Spatial
    )
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
        SearchRevisionOperation::Fresh
            | SearchRevisionOperation::Replace
            | SearchRevisionOperation::Rephrase
    ) {
        return search_parser_config()
            .discourse
            .revision_fresh_prefixes
            .iter()
            .filter_map(|prefix| strip_prefix_case_insensitive(utterance, prefix))
            .next()
            .unwrap_or(utterance)
            .trim_matches([',', ':', '-', ' '])
            .to_string();
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

fn strip_prefix_case_insensitive<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .and_then(|_| value.get(prefix.len()..))
}

pub fn issue_signed_search_context(
    parent_revision_id: Option<String>,
    operation: SearchRevisionOperation,
    depth: usize,
    active_query: String,
    plan: CompiledSearchPlan,
    runtime_version: SearchRuntimeVersion,
    ordered_result_ids: Vec<String>,
) -> (SignedSearchContext, SearchRevisionDescriptor) {
    let result_fingerprint = result_membership_fingerprint(&ordered_result_ids);
    let revision_id = revision_id_for_plan(
        parent_revision_id.as_deref(),
        operation,
        &plan.semantic_fingerprint,
        &runtime_version,
        &result_fingerprint,
        depth,
    );
    let context = SignedSearchContext {
        revision_id: revision_id.clone(),
        parent_revision_id: parent_revision_id.clone(),
        operation,
        depth,
        active_query: active_query.clone(),
        plan_fingerprint: plan.semantic_fingerprint.clone(),
        runtime_version: runtime_version.clone(),
        plan,
        ordered_result_ids,
        result_fingerprint,
    };
    let encoded = encode_signed_context(&context);
    let descriptor = SearchRevisionDescriptor {
        id: revision_id,
        parent_id: parent_revision_id,
        operation,
        depth,
        active_query,
        plan_fingerprint: context.plan_fingerprint.clone(),
        runtime_version,
        buyer_intent: context
            .plan
            .branches
            .iter()
            .map(|branch| BuyerIntentBranchProjection {
                branch_id: branch.branch_id.clone(),
                summary: branch.buyer_summary.clone(),
                source_spans: branch.source_spans.clone(),
            })
            .collect(),
        context: encoded,
    };
    (context, descriptor)
}

pub fn decode_signed_search_context(value: &str) -> Result<SignedSearchContext, String> {
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
    let expected = encode_hex(&hmac_sha256(revision_signing_key(), &payload));
    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        return Err("invalid revision context signature".to_string());
    }
    let context: SignedSearchContext = serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid revision context payload: {error}"))?;
    validate_signed_search_context(&context)?;
    Ok(context)
}

fn encode_signed_context(context: &SignedSearchContext) -> String {
    let payload = serde_json::to_vec(context).expect("revision context is serializable");
    let signature = hmac_sha256(revision_signing_key(), &payload);
    format!("v1.{}.{}", encode_hex(&payload), encode_hex(&signature))
}

fn validate_signed_search_context(context: &SignedSearchContext) -> Result<(), String> {
    let mut validated_plan = context.plan.clone();
    validated_plan.refresh_semantic_fingerprint();
    let fingerprint = validated_plan.semantic_fingerprint;
    if fingerprint != context.plan.semantic_fingerprint || fingerprint != context.plan_fingerprint {
        return Err("revision plan fingerprint mismatch".to_string());
    }
    if result_membership_fingerprint(&context.ordered_result_ids) != context.result_fingerprint {
        return Err("revision result membership mismatch".to_string());
    }
    let expected_revision = revision_id_for_plan(
        context.parent_revision_id.as_deref(),
        context.operation,
        &context.plan_fingerprint,
        &context.runtime_version,
        &context.result_fingerprint,
        context.depth,
    );
    if !constant_time_eq(context.revision_id.as_bytes(), expected_revision.as_bytes()) {
        return Err("revision identity mismatch".to_string());
    }
    validate_plan_evidence(&context.plan)
}

fn validate_plan_evidence(plan: &CompiledSearchPlan) -> Result<(), String> {
    for reference in plan
        .branches
        .iter()
        .flat_map(|branch| match &branch.geo_scope {
            super::compiled_plan::GeoScope::Scoped {
                seed_cells,
                expanded_cell_paths,
                supporting_evidence,
                ..
            } => supporting_evidence
                .iter()
                .chain(
                    seed_cells
                        .iter()
                        .flat_map(|seed| seed.supporting_evidence.iter()),
                )
                .chain(
                    expanded_cell_paths
                        .iter()
                        .flat_map(|path| path.supporting_evidence.iter()),
                )
                .collect::<Vec<_>>(),
            super::compiled_plan::GeoScope::Unresolved { .. } => Vec::new(),
            super::compiled_plan::GeoScope::BundleWide => Vec::new(),
        })
    {
        reference
            .validate_for(&reference.subject_entity_id, &plan.snapshot_identity)
            .map_err(|error| format!("invalid revision evidence identity: {error}"))?;
    }
    Ok(())
}

fn revision_id_for_plan(
    parent_revision_id: Option<&str>,
    operation: SearchRevisionOperation,
    plan_fingerprint: &str,
    runtime_version: &SearchRuntimeVersion,
    result_fingerprint: &str,
    depth: usize,
) -> String {
    let payload = serde_json::to_vec(&serde_json::json!({
        "parentRevisionId": parent_revision_id,
        "operation": operation,
        "planFingerprint": plan_fingerprint,
        "runtimeVersion": runtime_version,
        "resultFingerprint": result_fingerprint,
        "depth": depth,
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

fn revision_signing_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        if let Ok(configured) = std::env::var(REVISION_SIGNING_KEY_ENV) {
            if !configured.trim().is_empty() {
                return Sha256::digest(configured.as_bytes()).into();
            }
        }
        let mut key = [0_u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut source| source.read_exact(&mut key))
            .expect("secure randomness is required when OPENESTATES_REVISION_SIGNING_KEY is unset");
        key
    })
}

fn hmac_sha256(key: &[u8], payload: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK_SIZE];
    let mut outer_pad = [0x5c_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(payload);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::ast::{ConstraintExpr, ConstraintTerm};

    const LIMITS: SearchRevisionLimits = SearchRevisionLimits {
        max_active_branches: 8,
    };

    fn test_plan(compiled_query: super::super::CompiledQuery) -> CompiledSearchPlan {
        CompiledSearchPlan::compile_for_snapshot(
            compiled_query,
            "test-snapshot",
            &[],
            &GeoTopologyIndex::default(),
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
        let base = super::super::CompiledQuery::from_text(parent_query);
        let parent = test_plan(super::super::CompiledQuery::with_constraints(
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
        let mut fragment = test_plan(super::super::CompiledQuery::from_text(
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
            &GeoTopologyIndex::default(),
            None,
            GeoCellSearchPolicy {
                max_hops: 2,
                max_distance_km: 4.0,
            },
        )
        .expect("typed patch applies");
        let predicates = &candidate.branches[0].predicates;
        assert!(predicates.contains_family(PredicateFamily::Area));
        assert_eq!(candidate.branches[0].constraints.requested_bhks(), [2]);
        assert_eq!(
            candidate.branches[0].constraints.budget_max,
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
        let parent = test_plan(super::super::CompiledQuery::from_text(&query));
        let fragment = test_plan(super::super::CompiledQuery::from_text(
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
}
