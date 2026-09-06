use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::OnceLock;

use crate::dag_config::search_parser_config;

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
        family: String,
        fragment: String,
        branch_id: Option<String>,
    },
    RemovePredicate {
        family: String,
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
    let parent = parent_query.trim();
    let turn = utterance.trim();
    let discourse = &search_parser_config().discourse;

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
        if let Some(query) = replace_budget_constraint(parent, turn, target_branch) {
            return candidate_with_patch(
                SearchRevisionOperation::Refine,
                query,
                active_branch_count,
                SearchRevisionPatch::ReplacePredicate {
                    family: "price".to_string(),
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
    parent: &str,
    utterance: &str,
    target_branch: Option<usize>,
) -> Option<String> {
    let parent_slots = super::parser::parse_query_slots(parent);
    let replacement_slots = super::parser::parse_query_slots(utterance);
    let replacement = replacement_slots.budgets.first()?;
    let replacement_text = utterance.get(replacement.start..replacement.end)?.trim();
    if replacement_text.is_empty() || parent_slots.budgets.is_empty() {
        return None;
    }
    let targeted_span = target_branch.and_then(|target| {
        super::query_plan::discourse_branch_layout(parent)
            .and_then(|layout| layout.segments.get(target).copied())
    });
    if target_branch.is_some() && targeted_span.is_none() {
        return None;
    }
    let budgets = parent_slots
        .budgets
        .iter()
        .filter(|budget| {
            targeted_span.is_none_or(|span| budget.start >= span.start && budget.end <= span.end)
        })
        .collect::<Vec<_>>();
    if budgets.is_empty() {
        return None;
    }
    let mut query = parent.to_string();
    for budget in budgets.into_iter().rev() {
        if budget.start <= budget.end && budget.end <= query.len() {
            query.replace_range(budget.start..budget.end, replacement_text);
        }
    }
    Some(query.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: SearchRevisionLimits = SearchRevisionLimits {
        max_active_branches: 8,
    };

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
            [SearchRevisionPatch::ReplacePredicate { family, .. }] if family == "price"
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
            }] if family == "price" && branch_id == "branch-2"
        ));
    }
}
