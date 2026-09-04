use crate::dag_config::search_parser_config;

use super::intent::{parse_intent, SearchIntent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRevisionOperation {
    Refine,
    Rephrase,
    Expand,
    Switch,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        if clause.is_empty() || !has_structured_search_anchor(clause) {
            return clarification(SearchRevisionOperation::Expand, active_branch_count);
        }
        if active_branch_count >= limits.max_active_branches {
            return SearchRevision {
                operation: SearchRevisionOperation::Expand,
                outcome: SearchRevisionOutcome::RequireCheckpoint,
                candidate_query: None,
                candidate_branch_count: active_branch_count,
            };
        }
        return candidate(
            SearchRevisionOperation::Expand,
            format!("{parent} or {clause}"),
            active_branch_count + 1,
        );
    }

    let standalone_intent = parse_intent(turn);
    let is_complete = is_complete_search(&standalone_intent);
    let explicitly_switches = contains_configured_phrase(turn, &discourse.revision_switch_prefixes);
    let explicitly_replaces = contains_configured_phrase(turn, &discourse.revision_replace_markers);
    let explicitly_corrects =
        strip_configured_prefix(turn, &discourse.revision_correction_prefixes).is_some();

    if is_complete && explicitly_replaces {
        return candidate(
            SearchRevisionOperation::Replace,
            turn.to_string(),
            standalone_branch_count(turn),
        );
    }
    if is_complete {
        let operation = if !explicitly_switches
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
    candidate(
        if explicitly_replaces {
            SearchRevisionOperation::Replace
        } else {
            SearchRevisionOperation::Refine
        },
        format!("{parent} {refinement}"),
        active_branch_count,
    )
}

fn candidate(
    operation: SearchRevisionOperation,
    query: String,
    branch_count: usize,
) -> SearchRevision {
    SearchRevision {
        operation,
        outcome: SearchRevisionOutcome::Candidate,
        candidate_query: Some(query),
        candidate_branch_count: branch_count,
    }
}

fn clarification(operation: SearchRevisionOperation, branch_count: usize) -> SearchRevision {
    SearchRevision {
        operation,
        outcome: SearchRevisionOutcome::RequireClarification,
        candidate_query: None,
        candidate_branch_count: branch_count,
    }
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
    fn partial_budget_correction_does_not_mix_conflicting_limits() {
        let revision = compile_search_revision(
            "3BHK in Hoodi under 2.4 Cr",
            "Increase the budget to 2.8 Cr",
            1,
            LIMITS,
        );

        assert_eq!(revision.operation, SearchRevisionOperation::Replace);
        assert_eq!(
            revision.outcome,
            SearchRevisionOutcome::RequireClarification
        );
        assert!(revision.candidate_query.is_none());
    }
}
