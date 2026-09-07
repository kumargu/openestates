use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::OnceLock;

use crate::dag_config::{search_parser_config, search_resolution_config};
use crate::serving::{ServingEntityAliasIndex, ServingEntityRecord};

use super::ast::{PredicateFamily, PredicatePolarity};
use super::compiled_plan::{CompiledSearchPlan, GeoBranch};
use super::intent::{parse_intent, SearchIntent};
use super::SearchRuntimeVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchRevisionOperation {
    Refine,
    Rephrase,
    Expand,
    Switch,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRevision {
    pub operation: SearchRevisionOperation,
    pub outcome: SearchRevisionOutcome,
    pub candidate_query: Option<String>,
    pub candidate_branch_count: usize,
    pub patches: Vec<SearchRevisionPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchRevisionPatch {
    AddPredicate {
        fragment: String,
    },
    ReplacePredicate {
        family: PredicateFamily,
        fragment: String,
        branch_id: Option<String>,
    },
    RemovePredicate {
        family: PredicateFamily,
    },
    AddAlternative {
        fragment: String,
    },
    ReplaceIntent {
        query: String,
    },
}

/// Compiles one conversational search turn into a full candidate query.
///
/// This layer only owns revision mechanics. The ordinary parser, serving
/// resolver, AST, and ranker remain authoritative when the candidate runs.
pub fn compile_search_revision(
    parent_query: &str,
    utterance: &str,
    active_branch_count: usize,
    limits: SearchRevisionLimits,
) -> SearchRevision {
    compile_search_revision_with_plan(
        parent_query,
        utterance,
        active_branch_count,
        limits,
        None,
        None,
    )
}

pub fn compile_search_revision_with_plan(
    parent_query: &str,
    utterance: &str,
    active_branch_count: usize,
    limits: SearchRevisionLimits,
    parent_plan: Option<&CompiledSearchPlan>,
    area_only_alternative: Option<&str>,
) -> SearchRevision {
    let parent = parent_query.trim();
    let turn = utterance.trim();
    let discourse = &search_parser_config().discourse;
    let fallback_plan = parent_plan
        .is_none()
        .then(|| compile_revision_parent_plan(parent));
    let parent_plan = parent_plan.or(fallback_plan.as_ref());

    if turn.is_empty() {
        return clarification(SearchRevisionOperation::Refine, active_branch_count);
    }

    if let Some(clause) = strip_configured_prefix(turn, &discourse.revision_expand_prefixes) {
        if active_branch_count >= limits.max_active_branches {
            return SearchRevision {
                operation: SearchRevisionOperation::Expand,
                outcome: SearchRevisionOutcome::RequireCheckpoint,
                candidate_query: None,
                candidate_branch_count: active_branch_count,
                patches: Vec::new(),
            };
        }
        if let Some(area_name) = area_only_alternative {
            let Some(query) = parent_plan.and_then(|plan| area_alternative_query(plan, area_name))
            else {
                return clarification(SearchRevisionOperation::Expand, active_branch_count);
            };
            return candidate_with_patch(
                SearchRevisionOperation::Expand,
                query,
                active_branch_count + 1,
                SearchRevisionPatch::AddAlternative {
                    fragment: area_name.to_string(),
                },
            );
        }
        if clause.is_empty() || !has_structured_search_anchor(clause) {
            return clarification(SearchRevisionOperation::Expand, active_branch_count);
        }
        return candidate_with_patch(
            SearchRevisionOperation::Expand,
            format!("{parent} or {clause}"),
            active_branch_count + 1,
            SearchRevisionPatch::AddAlternative {
                fragment: clause.to_string(),
            },
        );
    }

    let standalone_intent = parse_intent(turn);
    let is_complete = is_complete_search(&standalone_intent);
    let explicitly_switches = contains_configured_phrase(turn, &discourse.revision_switch_prefixes);
    let explicitly_replaces = contains_configured_phrase(turn, &discourse.revision_replace_markers);
    let explicitly_corrects =
        strip_configured_prefix(turn, &discourse.revision_correction_prefixes).is_some();

    if discourse
        .revision_ambiguous_spatial_refinements
        .iter()
        .any(|phrase| contains_configured_phrase(turn, std::slice::from_ref(phrase)))
        && super::parser::parse_query_slots(turn)
            .distance_limit
            .is_none()
    {
        return clarification(SearchRevisionOperation::Refine, active_branch_count);
    }

    if explicitly_corrects {
        let target_branch = if active_branch_count > 1 && !targets_every_branch(turn) {
            let Some(index) = targeted_branch_index(turn, active_branch_count) else {
                return clarification(SearchRevisionOperation::Refine, active_branch_count);
            };
            Some(index)
        } else {
            None
        };
        if let Some(query) =
            parent_plan.and_then(|plan| replace_budget_constraint(plan, turn, target_branch))
        {
            return candidate_with_patch(
                SearchRevisionOperation::Refine,
                query,
                active_branch_count,
                SearchRevisionPatch::ReplacePredicate {
                    family: PredicateFamily::Budget,
                    fragment: turn.to_string(),
                    branch_id: target_branch.map(|index| format!("branch-{}", index + 1)),
                },
            );
        }
    }

    if is_complete && explicitly_replaces {
        return candidate(
            SearchRevisionOperation::Replace,
            turn.to_string(),
            standalone_branch_count(turn),
        );
    }
    if is_complete {
        let operation =
            if explicitly_switches && !standalone_intent.unsupported_inventory_types.is_empty() {
                SearchRevisionOperation::Replace
            } else if !explicitly_switches
                && equivalent_search_shape(parent, &parse_intent(parent), turn, &standalone_intent)
            {
                SearchRevisionOperation::Rephrase
            } else {
                SearchRevisionOperation::Switch
            };
        return candidate(operation, turn.to_string(), standalone_branch_count(turn));
    }
    if explicitly_switches || explicitly_replaces || explicitly_corrects {
        let operation = if explicitly_switches {
            SearchRevisionOperation::Switch
        } else {
            SearchRevisionOperation::Replace
        };
        return clarification(operation, active_branch_count);
    }

    let refinement = strip_configured_prefixes(turn, &discourse.revision_continuity_prefixes);
    if refinement.is_empty() {
        return clarification(SearchRevisionOperation::Refine, active_branch_count);
    }
    candidate_with_patch(
        if explicitly_replaces {
            SearchRevisionOperation::Replace
        } else {
            SearchRevisionOperation::Refine
        },
        format!("{parent} {refinement}"),
        active_branch_count,
        SearchRevisionPatch::AddPredicate {
            fragment: refinement.to_string(),
        },
    )
}

pub(crate) fn revision_expansion_fragment(value: &str) -> Option<&str> {
    strip_configured_prefix(
        value.trim(),
        &search_parser_config().discourse.revision_expand_prefixes,
    )
}

pub(crate) fn resolve_area_only_alternative(
    fragment: &str,
    entities: &[ServingEntityRecord],
    aliases: &ServingEntityAliasIndex,
) -> Option<String> {
    let candidate = strip_configured_prefix(
        fragment.trim(),
        &search_resolution_config().named_entity_scope_prefixes,
    )
    .unwrap_or(fragment.trim())
    .trim_matches([',', ':', '-', ' ']);
    if candidate.is_empty() {
        return None;
    }
    let intent = parse_intent(candidate);
    if !intent.requested_bhks().is_empty()
        || intent.budget_min.is_some()
        || intent.budget_max.is_some()
        || !intent.hard_constraints.is_empty()
    {
        return None;
    }
    let mut matches = entities
        .iter()
        .filter(|entity| {
            entity.entity_type.eq_ignore_ascii_case("area")
                && entity.name.trim().eq_ignore_ascii_case(candidate)
        })
        .map(|entity| (entity.entity_id.as_str(), entity.name.as_str()))
        .collect::<Vec<_>>();
    if let Some(group) = aliases.get(candidate) {
        matches.extend(
            group
                .members
                .iter()
                .filter(|record| record.entity_type.eq_ignore_ascii_case("area"))
                .map(|record| (record.entity_id.as_str(), record.entity_name.as_str())),
        );
    }
    matches.sort_unstable();
    matches.dedup_by(|left, right| left.0 == right.0);
    match matches.as_slice() {
        [(_, name)] => Some((*name).to_string()),
        [] => intent
            .requested_areas()
            .first()
            .map(|area| (*area).to_string()),
        _ => None,
    }
}

fn area_alternative_query(parent_plan: &CompiledSearchPlan, area_name: &str) -> Option<String> {
    let [branch] = parent_plan.branches.as_slice() else {
        return None;
    };
    let (alternative, replaced) = replace_branch_predicates(
        branch,
        &[
            PredicateFamily::Area,
            PredicateFamily::Society,
            PredicateFamily::Spatial,
        ],
        area_name,
        true,
    )?;
    if replaced != 1 {
        return None;
    }
    Some(format!(
        "{} or {alternative}",
        canonical_plan_query(parent_plan)
    ))
}

fn candidate(
    operation: SearchRevisionOperation,
    query: String,
    branch_count: usize,
) -> SearchRevision {
    let patch = match operation {
        SearchRevisionOperation::Expand => SearchRevisionPatch::AddAlternative {
            fragment: query.clone(),
        },
        SearchRevisionOperation::Refine => SearchRevisionPatch::AddPredicate {
            fragment: query.clone(),
        },
        SearchRevisionOperation::Rephrase
        | SearchRevisionOperation::Switch
        | SearchRevisionOperation::Replace => SearchRevisionPatch::ReplaceIntent {
            query: query.clone(),
        },
    };
    candidate_with_patch(operation, query, branch_count, patch)
}

fn candidate_with_patch(
    operation: SearchRevisionOperation,
    query: String,
    branch_count: usize,
    patch: SearchRevisionPatch,
) -> SearchRevision {
    SearchRevision {
        operation,
        outcome: SearchRevisionOutcome::Candidate,
        candidate_query: Some(query),
        candidate_branch_count: branch_count,
        patches: vec![patch],
    }
}

fn clarification(operation: SearchRevisionOperation, branch_count: usize) -> SearchRevision {
    SearchRevision {
        operation,
        outcome: SearchRevisionOutcome::RequireClarification,
        candidate_query: None,
        candidate_branch_count: branch_count,
        patches: Vec::new(),
    }
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

fn strip_configured_prefixes<'a>(mut value: &'a str, prefixes: &[String]) -> &'a str {
    while let Some(remainder) = strip_configured_prefix(value, prefixes) {
        value = remainder;
    }
    value.trim()
}

fn strip_prefix_case_insensitive<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .and_then(|_| value.get(prefix.len()..))
}

fn contains_configured_phrase(value: &str, phrases: &[String]) -> bool {
    let lower = value.to_lowercase();
    phrases
        .iter()
        .any(|phrase| lower.contains(&phrase.to_lowercase()))
}

fn has_structured_search_anchor(query: &str) -> bool {
    let intent = parse_intent(query);
    !intent.requested_bhks().is_empty()
        || intent.budget_min.is_some()
        || intent.budget_max.is_some()
        || !intent.requested_areas().is_empty()
        || !intent.hard_constraints.is_empty()
}

fn is_complete_search(intent: &SearchIntent) -> bool {
    let dimensions = [
        !intent.requested_bhks().is_empty(),
        intent.budget_min.is_some() || intent.budget_max.is_some(),
        !intent.requested_areas().is_empty(),
        !intent.hard_constraints.is_empty(),
    ];
    dimensions.into_iter().filter(|present| *present).count() >= 2
}

fn equivalent_search_shape(
    left_query: &str,
    left: &SearchIntent,
    right_query: &str,
    right: &SearchIntent,
) -> bool {
    normalized_strings(left.requested_areas()) == normalized_strings(right.requested_areas())
        && left.requested_bhks() == right.requested_bhks()
        && left.budget_min == right.budget_min
        && left.budget_max == right.budget_max
        && left.hard_constraints == right.hard_constraints
        && left.excluded_areas == right.excluded_areas
        && left.exclude_bhks == right.exclude_bhks
        && unresolved_scope_signature(left_query, left)
            == unresolved_scope_signature(right_query, right)
}

fn unresolved_scope_signature(query: &str, intent: &SearchIntent) -> Option<String> {
    if !intent.requested_areas().is_empty() {
        return None;
    }
    let plan = super::query_plan::compile_query_plan(query);
    super::query_plan::unresolved_named_entity_clause(query, &plan, |_| false, |_| false)
        .or_else(|| super::query_plan::unresolved_residual_clause(query, &plan, |_| false))
        .map(|scope| scope.to_lowercase())
}

fn normalized_strings(values: Vec<&str>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.to_lowercase())
        .collect::<Vec<_>>();
    values.sort_unstable();
    values
}

fn standalone_branch_count(query: &str) -> usize {
    super::query_plan::discourse_branch_layout(query).map_or(1, |layout| layout.segments.len())
}

pub fn compiled_branch_count(query: &str) -> usize {
    standalone_branch_count(query)
}

pub fn revision_id_for_query(
    query: &str,
    runtime_version: &SearchRuntimeVersion,
    depth: usize,
) -> String {
    let mut payload = Vec::new();
    payload.extend_from_slice(
        query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_bytes(),
    );
    payload.push(0);
    payload.extend_from_slice(
        &serde_json::to_vec(runtime_version).expect("runtime version is serializable"),
    );
    payload.extend_from_slice(&depth.to_be_bytes());
    let digest = hmac_sha256(revision_signing_key(), &payload);
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("rev-{depth:03}-{}", &encoded[..32])
}

pub fn validated_revision_depth(
    revision_id: &str,
    query: &str,
    runtime_version: &SearchRuntimeVersion,
) -> Option<usize> {
    let depth = revision_id
        .strip_prefix("rev-")?
        .split('-')
        .next()?
        .parse::<usize>()
        .ok()?;
    let expected = revision_id_for_query(query, runtime_version, depth);
    (depth > 0 && constant_time_eq(revision_id.as_bytes(), expected.as_bytes())).then_some(depth)
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

fn replace_budget_constraint(
    parent_plan: &CompiledSearchPlan,
    utterance: &str,
    target_branch: Option<usize>,
) -> Option<String> {
    let replacement_slots = super::parser::parse_query_slots(utterance);
    let replacement = replacement_slots.budgets.first()?;
    let replacement_text = utterance.get(replacement.start..replacement.end)?.trim();
    if replacement_text.is_empty()
        || target_branch.is_some_and(|index| index >= parent_plan.branches.len())
    {
        return None;
    }

    let mut branch_queries = Vec::with_capacity(parent_plan.branches.len());
    for (index, branch) in parent_plan.branches.iter().enumerate() {
        if target_branch.is_none_or(|target| target == index) {
            let (query, replaced) = replace_branch_predicates(
                branch,
                &[PredicateFamily::Budget],
                replacement_text,
                parent_plan.branches.len() == 1,
            )?;
            if replaced == 0 {
                return None;
            }
            branch_queries.push(query);
        } else {
            branch_queries.push(branch_source_query(branch)?);
        }
    }
    Some(branch_queries.join(" or "))
}

fn replace_branch_predicates(
    branch: &GeoBranch,
    families: &[PredicateFamily],
    replacement: &str,
    use_full_source_query: bool,
) -> Option<(String, usize)> {
    let spans = branch
        .predicates
        .source_spans_for(families, PredicatePolarity::Positive);
    let (branch_start, branch_end) = if use_full_source_query {
        (0, branch.source_query.len())
    } else {
        branch_source_bounds(branch)?
    };
    let mut query = branch
        .source_query
        .get(branch_start..branch_end)?
        .to_string();
    for span in spans.iter().rev() {
        if span.start < branch_start || span.end > branch_end || span.start > span.end {
            return None;
        }
        query.replace_range(
            span.start - branch_start..span.end - branch_start,
            replacement,
        );
    }
    Some((normalize_query(&query), spans.len()))
}

fn canonical_plan_query(plan: &CompiledSearchPlan) -> String {
    if let [branch] = plan.branches.as_slice() {
        return normalize_query(&branch.source_query);
    }
    plan.branches
        .iter()
        .filter_map(branch_source_query)
        .collect::<Vec<_>>()
        .join(" or ")
}

fn branch_source_query(branch: &GeoBranch) -> Option<String> {
    let (start, end) = branch_source_bounds(branch)?;
    branch.source_query.get(start..end).map(normalize_query)
}

fn branch_source_bounds(branch: &GeoBranch) -> Option<(usize, usize)> {
    Some((
        branch.source_spans.iter().map(|span| span.start).min()?,
        branch.source_spans.iter().map(|span| span.end).max()?,
    ))
}

fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compile_revision_parent_plan(query: &str) -> CompiledSearchPlan {
    CompiledSearchPlan::compile(super::CompiledQuery::from_text(query), "revision-parent")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::ast::{ConstraintExpr, ConstraintTerm};
    use crate::search::intent::SourceSpan;

    const LIMITS: SearchRevisionLimits = SearchRevisionLimits {
        max_active_branches: 8,
    };

    fn area_parent_plan(query: &str, area: &str) -> CompiledSearchPlan {
        let start = query.find(area).expect("area occurs in parent query");
        let end = start + area.len();
        CompiledSearchPlan::compile(
            super::super::CompiledQuery::with_constraints(
                query,
                ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: Some(format!("area:{}", area.to_ascii_lowercase())),
                    value: area.to_string(),
                    span: Some(SourceSpan {
                        start,
                        end,
                        raw_text: area.to_string(),
                    }),
                }),
                parse_intent(query),
            ),
            "test-snapshot",
        )
    }

    fn spatial_parent_plan(query: &str, target: &str) -> CompiledSearchPlan {
        let start = query.find(target).expect("spatial target occurs in query");
        let end = start + target.len();
        CompiledSearchPlan::compile(
            super::super::CompiledQuery::with_constraints(
                query,
                ConstraintExpr::term(ConstraintTerm::Spatial {
                    relation: "near".to_string(),
                    entity_id: format!("area:{}", target.to_ascii_lowercase()),
                    display_name: target.to_string(),
                    required: false,
                    span: Some(SourceSpan {
                        start,
                        end,
                        raw_text: target.to_string(),
                    }),
                }),
                parse_intent(query),
            ),
            "test-snapshot",
        )
    }

    #[test]
    fn additive_refinement_retains_parent_scope() {
        let revision = compile_search_revision(
            "3BHK in East Bengaluru under 2.4 Cr",
            "Keep the same search, but make it ready to move",
            1,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Refine);
        assert_eq!(revision.outcome, SearchRevisionOutcome::Candidate);
        assert_eq!(
            revision.candidate_query.as_deref(),
            Some("3BHK in East Bengaluru under 2.4 Cr ready to move")
        );
    }

    #[test]
    fn complete_equivalent_query_is_a_rephrase() {
        let revision = compile_search_revision(
            "3BHK in East Bengaluru under 2.4 Cr",
            "3 bedrooms in East Bangalore costing no more than 2.4 crore",
            1,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Rephrase);
        assert_eq!(revision.candidate_branch_count, 1);
    }

    #[test]
    fn same_constraints_in_a_different_named_area_are_a_switch() {
        let revision = compile_search_revision(
            "3BHK in Hoodi under 2.4 Cr",
            "3BHK in Whitefield under 2.4 Cr",
            1,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Switch);
    }

    #[test]
    fn ninth_branch_requires_a_checkpoint() {
        let revision = compile_search_revision(
            "2BHK in Whitefield under 1.6 Cr",
            "Also consider 3BHK in South Bengaluru under 2 Cr",
            8,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Expand);
        assert_eq!(revision.outcome, SearchRevisionOutcome::RequireCheckpoint);
        assert!(revision.candidate_query.is_none());
    }

    #[test]
    fn vague_branch_requires_clarification_before_search() {
        let revision = compile_search_revision(
            "2BHK in Whitefield under 1.6 Cr",
            "Also consider another independent area",
            2,
            LIMITS,
        );

        assert_eq!(
            revision.outcome,
            SearchRevisionOutcome::RequireClarification
        );
    }

    #[test]
    fn partial_budget_correction_replaces_the_previous_limit() {
        let revision = compile_search_revision(
            "3BHK in Hoodi under 2.4 Cr",
            "Increase the budget to 2.8 Cr",
            1,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Refine);
        assert_eq!(revision.outcome, SearchRevisionOutcome::Candidate);
        assert_eq!(
            revision.candidate_query.as_deref(),
            Some("3BHK in Hoodi under 2.8 Cr")
        );
    }

    #[test]
    fn vague_relative_distance_requires_clarification() {
        let revision =
            compile_search_revision("3BHK near Hoodi under 2.5 Cr", "Make it closer", 1, LIMITS);
        assert_eq!(
            revision.outcome,
            SearchRevisionOutcome::RequireClarification
        );
    }

    #[test]
    fn issue_118_revision_scenarios_compile_without_server_history() {
        let parent = "3BHK in Whitefield under 2.5Cr";
        let ready = compile_search_revision(parent, "Make it ready to move", 1, LIMITS);
        assert_eq!(ready.operation, SearchRevisionOperation::Refine);
        assert!(ready
            .candidate_query
            .as_deref()
            .is_some_and(|query| query.contains("ready to move")));

        let both = compile_search_revision(
            parent,
            "Near both Hoodi Metro and Manipal Hospital",
            1,
            LIMITS,
        );
        assert!(both
            .candidate_query
            .as_deref()
            .is_some_and(|query| query.contains("both Hoodi Metro and Manipal Hospital")));

        let budget = compile_search_revision(parent, "Increase the budget to 3Cr", 1, LIMITS);
        assert_eq!(
            budget.candidate_query.as_deref(),
            Some("3BHK in Whitefield under 3Cr")
        );

        let alternative = compile_search_revision(
            parent,
            "Alternatively consider 3BHK in Kadugodi under 2.7Cr",
            1,
            LIMITS,
        );
        assert_eq!(alternative.operation, SearchRevisionOperation::Expand);
        assert_eq!(alternative.candidate_branch_count, 2);

        let branch_local =
            compile_search_revision(parent, "Also consider 2BHK in Hoodi under 2Cr", 1, LIMITS);
        assert!(branch_local
            .candidate_query
            .as_deref()
            .is_some_and(|query| {
                query == "3BHK in Whitefield under 2.5Cr or 2BHK in Hoodi under 2Cr"
            }));

        let excluded = compile_search_revision(
            "3BHK around Whitefield under 3Cr",
            "Exclude Varthur",
            1,
            LIMITS,
        );
        assert!(excluded
            .candidate_query
            .as_deref()
            .is_some_and(|query| query.ends_with("Exclude Varthur")));

        let replacement = compile_search_revision(
            "3BHK around Whitefield under 3Cr",
            "Instead, search for plots in North Bengaluru under 1.5Cr",
            1,
            LIMITS,
        );
        assert_eq!(replacement.operation, SearchRevisionOperation::Replace);
        assert_eq!(replacement.candidate_branch_count, 1);
    }

    #[test]
    fn branch_cohorts_keep_eight_as_the_active_limit() {
        let query = |count: usize| {
            (1..=count)
                .map(|index| format!("2BHK in Test Area {index} under 2Cr"))
                .collect::<Vec<_>>()
                .join(" or ")
        };
        assert_eq!(compiled_branch_count(&query(3)), 3);
        assert_eq!(compiled_branch_count(&query(8)), 8);
        assert_eq!(compiled_branch_count(&query(16)), 16);
        assert_eq!(LIMITS.max_active_branches, 8);
    }

    #[test]
    fn untargeted_multi_branch_budget_correction_requires_clarification() {
        let parent = "3BHK in Whitefield under 2.5Cr or 2BHK in Hoodi under 2Cr";
        let ambiguous = compile_search_revision(parent, "Increase the budget to 3Cr", 2, LIMITS);
        assert_eq!(
            ambiguous.outcome,
            SearchRevisionOutcome::RequireClarification
        );
        assert!(ambiguous.candidate_query.is_none());

        let all = compile_search_revision(parent, "Increase the budget for both to 3Cr", 2, LIMITS);
        assert_eq!(all.outcome, SearchRevisionOutcome::Candidate);
        assert!(matches!(
            all.patches.as_slice(),
            [SearchRevisionPatch::ReplacePredicate { family, .. }]
                if *family == PredicateFamily::Budget
        ));

        let second =
            compile_search_revision(parent, "Increase the second budget to 2.4Cr", 2, LIMITS);
        assert_eq!(second.outcome, SearchRevisionOutcome::Candidate);
        assert_eq!(
            second.candidate_query.as_deref(),
            Some("3BHK in Whitefield under 2.5Cr or 2BHK in Hoodi under 2.4Cr")
        );
        assert!(matches!(
            second.patches.as_slice(),
            [SearchRevisionPatch::ReplacePredicate {
                family,
                branch_id: Some(branch_id),
                ..
            }] if *family == PredicateFamily::Budget && branch_id == "branch-2"
        ));
    }

    #[test]
    fn area_only_alternative_copies_one_parent_branches_nonspatial_constraints() {
        let parent = "3BHK in Whitefield under 2.5Cr";
        let plan = area_parent_plan(parent, "Whitefield");
        let entities = vec![ServingEntityRecord {
            entity_id: "area:kadugodi".to_string(),
            entity_type: "area".to_string(),
            name: "Kadugodi".to_string(),
            root_source: Some("openstreetmap".to_string()),
            visibility: Default::default(),
            searchable_text: "Kadugodi".to_string(),
        }];
        let area = resolve_area_only_alternative(
            "Kadugodi",
            &entities,
            &ServingEntityAliasIndex::default(),
        );

        let revision = compile_search_revision_with_plan(
            parent,
            "Also consider Kadugodi",
            1,
            LIMITS,
            Some(&plan),
            area.as_deref(),
        );

        assert_eq!(revision.outcome, SearchRevisionOutcome::Candidate);
        assert_eq!(
            revision.candidate_query.as_deref(),
            Some("3BHK in Whitefield under 2.5Cr or 3BHK in Kadugodi under 2.5Cr")
        );
    }

    #[test]
    fn area_only_alternative_reuses_generic_spatial_scope_span() {
        let parent = "3BHK near Hoodi under 2.5Cr";
        let plan = spatial_parent_plan(parent, "Hoodi");

        let revision = compile_search_revision_with_plan(
            parent,
            "Also consider Kadugodi",
            1,
            LIMITS,
            Some(&plan),
            Some("Kadugodi"),
        );

        assert_eq!(revision.outcome, SearchRevisionOutcome::Candidate);
        assert_eq!(
            revision.candidate_query.as_deref(),
            Some("3BHK near Hoodi under 2.5Cr or 3BHK near Kadugodi under 2.5Cr")
        );
    }

    #[test]
    fn area_only_alternative_clarifies_for_multiple_scopes_in_one_branch() {
        let parent = "3BHK near Hoodi and ITPL under 2.5Cr";
        let hoodi = spatial_parent_plan(parent, "Hoodi");
        let itpl = spatial_parent_plan(parent, "ITPL");
        let plan = CompiledSearchPlan::compile(
            super::super::CompiledQuery::with_constraints(
                parent,
                ConstraintExpr::and(vec![
                    hoodi.branches[0].predicates.clone(),
                    itpl.branches[0].predicates.clone(),
                ]),
                parse_intent(parent),
            ),
            "test-snapshot",
        );

        let revision = compile_search_revision_with_plan(
            parent,
            "Also consider Kadugodi",
            1,
            LIMITS,
            Some(&plan),
            Some("Kadugodi"),
        );

        assert_eq!(
            revision.outcome,
            SearchRevisionOutcome::RequireClarification
        );
        assert!(revision.candidate_query.is_none());
    }

    #[test]
    fn area_only_alternative_clarifies_when_multiple_parent_branches_could_supply_constraints() {
        let parent = "3BHK in Whitefield under 2.5Cr or 2BHK in Hoodi under 2Cr";
        let plan = CompiledSearchPlan::compile(
            super::super::CompiledQuery::from_text(parent),
            "test-snapshot",
        );

        let revision = compile_search_revision_with_plan(
            parent,
            "Also consider Kadugodi",
            2,
            LIMITS,
            Some(&plan),
            Some("Kadugodi"),
        );

        assert_eq!(
            revision.outcome,
            SearchRevisionOutcome::RequireClarification
        );
        assert!(revision.candidate_query.is_none());
    }
}
