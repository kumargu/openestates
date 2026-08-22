use crate::dag_config::{
    area_alias_entries, search_guardrail_config, search_parser_config, search_resolution_config,
};

use super::intent::{BuyerArchetype, Polarity, PreferenceSignal, SearchIntent, SourceSpan};
use super::parser::{self, SlotPolarity};
use super::schema;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct QueryPlan {
    pub slots: parser::ParsedQuerySlots,
    pub tokens: Vec<QueryToken>,
    pub areas: Vec<AreaMention>,
    pub clauses: Vec<QueryRelationClause>,
    pub evidence: Vec<schema::HardConstraintSpanMatch>,
    pub owned_spans: Vec<OwnedSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlternativeClauseLayout {
    pub segments: Vec<ByteSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscourseBranchLayout {
    pub segments: Vec<ByteSpan>,
    pub shared_suffix: Option<ByteSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueryToken {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AreaMention {
    pub canonical: String,
    pub matched_text: String,
    pub span: ByteSpan,
    pub polarity: MentionPolarity,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueryRelationClause {
    pub id: String,
    pub relation: String,
    pub relation_span: ByteSpan,
    pub target_text: String,
    pub target_span: ByteSpan,
    pub place_family_id: Option<String>,
    pub distance_limit_km: Option<f64>,
    pub requirement: RelationRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MentionPolarity {
    Positive,
    Exclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationRequirement {
    Coverage,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnedSpan {
    pub span: ByteSpan,
    pub owner: SpanOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpanOwner {
    Area,
    Bhk,
}

pub(crate) fn compile_query_plan(query: &str) -> QueryPlan {
    let slots = parser::parse_query_slots(query);
    let tokens = query_tokens_with_spans(query);
    let areas = detect_area_mentions(query);
    let clauses = relation_clauses(query, &tokens, &slots);
    let evidence = schema::detect_hard_constraint_spans(query);
    let mut owned_spans = Vec::new();
    for area in &areas {
        owned_spans.push(OwnedSpan {
            span: area.span,
            owner: SpanOwner::Area,
        });
    }
    for slot in &slots.bhks {
        if slot.end > slot.start {
            owned_spans.push(OwnedSpan {
                span: ByteSpan {
                    start: slot.start,
                    end: slot.end,
                },
                owner: SpanOwner::Bhk,
            });
        }
    }
    QueryPlan {
        slots,
        tokens,
        areas,
        clauses,
        evidence,
        owned_spans,
    }
}

impl QueryPlan {
    pub(crate) fn alternative_clause_layout(
        &self,
        owner_spans: &[SourceSpan],
        branch_span_groups: &[Vec<SourceSpan>],
    ) -> Option<AlternativeClauseLayout> {
        let branch_spans = branch_span_groups.iter().flatten().collect::<Vec<_>>();
        let candidates = self
            .tokens
            .iter()
            .filter(|token| token.text.eq_ignore_ascii_case("or"))
            .filter(|token| {
                !owner_spans
                    .iter()
                    .any(|span| span.start < token.start && span.end > token.end)
            })
            .filter(|token| {
                branch_spans.iter().any(|span| span.end <= token.start)
                    && branch_spans.iter().any(|span| span.start >= token.end)
            })
            .collect::<Vec<_>>();
        let mut separators = Vec::new();
        let mut segment_start = 0;
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            let segment_has_constraint = branch_spans
                .iter()
                .any(|span| span.start >= segment_start && span.end <= candidate.start);
            let previous_token = self
                .tokens
                .iter()
                .rev()
                .find(|token| token.end <= candidate.start);
            let next_token = self
                .tokens
                .iter()
                .find(|token| token.start >= candidate.end);
            let adjacent_tokens_are_unowned =
                previous_token
                    .zip(next_token)
                    .is_some_and(|(previous, next)| {
                        !token_is_owned(previous, owner_spans) && !token_is_owned(next, owner_spans)
                    });
            let right_end = candidates
                .get(candidate_index + 1)
                .map_or(usize::MAX, |next| next.start);
            let has_common_hard_family = branch_span_groups.iter().any(|spans| {
                spans
                    .iter()
                    .any(|span| span.start >= segment_start && span.end <= candidate.start)
                    && spans
                        .iter()
                        .any(|span| span.start >= candidate.end && span.end <= right_end)
            });
            if segment_has_constraint && (!adjacent_tokens_are_unowned || has_common_hard_family) {
                segment_start = candidate.end;
                separators.push(*candidate);
            }
        }
        if separators.is_empty() {
            return None;
        }

        let mut starts = vec![0];
        starts.extend(separators.iter().map(|separator| separator.end));
        let mut ends = separators
            .iter()
            .map(|separator| separator.start)
            .collect::<Vec<_>>();
        ends.push(usize::MAX);

        Some(AlternativeClauseLayout {
            segments: starts
                .into_iter()
                .zip(ends)
                .map(|(start, end)| ByteSpan { start, end })
                .collect(),
        })
    }
}

pub(crate) fn discourse_branch_layout(query: &str) -> Option<DiscourseBranchLayout> {
    let tokens = query_tokens_with_spans(query);
    let config = &search_parser_config().discourse;
    let shared_start = first_configured_phrase(&tokens, &config.shared_suffix_markers, 0)
        .map(|span| span.start)
        .unwrap_or(query.len());
    let core_tokens = tokens
        .iter()
        .filter(|token| token.start < shared_start)
        .cloned()
        .collect::<Vec<_>>();
    let conditional_starts =
        configured_phrase_spans(&core_tokens, &config.conditional_branch_starters, 0);
    let segments = if conditional_starts.len() >= 2 {
        conditional_starts
            .iter()
            .enumerate()
            .map(|(index, start)| ByteSpan {
                start: start.start,
                end: conditional_starts
                    .get(index + 1)
                    .map_or(shared_start, |next| next.start),
            })
            .collect::<Vec<_>>()
    } else {
        let separators = configured_phrase_spans(&core_tokens, &config.branch_joiners, 0);
        if separators.is_empty() {
            return None;
        }
        let first_start = first_configured_phrase(&core_tokens, &config.alternative_prefixes, 0)
            .filter(|prefix| prefix.start < separators[0].start)
            .map_or(0, |prefix| prefix.end);
        let mut starts = vec![first_start];
        starts.extend(separators.iter().map(|separator| separator.end));
        let mut ends = separators
            .iter()
            .map(|separator| separator.start)
            .collect::<Vec<_>>();
        ends.push(shared_start);
        starts
            .into_iter()
            .zip(ends)
            .map(|(start, end)| ByteSpan { start, end })
            .collect::<Vec<_>>()
    };
    let segments = segments
        .into_iter()
        .filter(|segment| {
            segment.start < segment.end && !query[segment.start..segment.end].trim().is_empty()
        })
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }
    Some(DiscourseBranchLayout {
        segments,
        shared_suffix: (shared_start < query.len()).then_some(ByteSpan {
            start: shared_start,
            end: query.len(),
        }),
    })
}

pub(crate) fn paired_ordinal_branch_queries(query: &str) -> Option<Vec<String>> {
    let tokens = query_tokens_with_spans(query);
    let config = &search_parser_config().discourse;
    if config.branch_ordinals.len() < 2 {
        return None;
    }
    let shared_suffix = first_configured_phrase(&tokens, &config.shared_suffix_markers, 0)?;
    let core_tokens = tokens
        .iter()
        .filter(|token| token.start < shared_suffix.start)
        .cloned()
        .collect::<Vec<_>>();
    let joiners = configured_phrase_spans(&core_tokens, &config.paired_branch_joiners, 0);
    if joiners.len() != 1 {
        return None;
    }
    let ordinal_spans = config
        .branch_ordinals
        .iter()
        .map(|ordinal| {
            first_configured_phrase(&tokens, std::slice::from_ref(ordinal), shared_suffix.end)
        })
        .collect::<Option<Vec<_>>>()?;
    let plan = compile_query_plan(query);
    let branch_bhks = ordinal_spans
        .iter()
        .map(|ordinal| {
            plan.slots
                .bhks
                .iter()
                .filter(|slot| slot.start >= shared_suffix.start && slot.end <= ordinal.start)
                .max_by_key(|slot| slot.end)
                .map(|slot| slot.value)
        })
        .collect::<Option<Vec<_>>>()?;
    let spans = [
        ByteSpan {
            start: 0,
            end: joiners[0].start,
        },
        ByteSpan {
            start: joiners[0].end,
            end: shared_suffix.start,
        },
    ];
    Some(
        spans
            .into_iter()
            .zip(branch_bhks)
            .map(|(span, bhk)| {
                let branch = query[span.start..span.end].trim_matches(|character: char| {
                    character.is_whitespace() || ",;".contains(character)
                });
                format!("{branch} {bhk}BHK")
            })
            .collect(),
    )
}

fn first_configured_phrase(
    tokens: &[QueryToken],
    phrases: &[String],
    start: usize,
) -> Option<ByteSpan> {
    configured_phrase_spans(tokens, phrases, start)
        .into_iter()
        .min_by_key(|span| span.start)
}

fn configured_phrase_spans(
    tokens: &[QueryToken],
    phrases: &[String],
    start: usize,
) -> Vec<ByteSpan> {
    let mut spans = Vec::new();
    for phrase in phrases {
        let phrase_tokens = parser::query_tokens(phrase);
        if phrase_tokens.is_empty() {
            continue;
        }
        for window in tokens.windows(phrase_tokens.len()) {
            if window[0].start < start
                || !window
                    .iter()
                    .zip(&phrase_tokens)
                    .all(|(token, phrase)| token.text.eq_ignore_ascii_case(phrase))
            {
                continue;
            }
            let span = ByteSpan {
                start: window[0].start,
                end: window[window.len() - 1].end,
            };
            if !spans.contains(&span) {
                spans.push(span);
            }
        }
    }
    spans.sort_by_key(|span| span.start);
    spans
}

fn token_is_owned(token: &QueryToken, owner_spans: &[SourceSpan]) -> bool {
    owner_spans
        .iter()
        .any(|span| span.start <= token.start && span.end >= token.end)
}

pub(crate) fn project_search_intent(query: &str, plan: &QueryPlan) -> SearchIntent {
    let q = query.to_lowercase();

    let excluded_areas = plan
        .areas
        .iter()
        .filter(|area| area.polarity == MentionPolarity::Exclusion)
        .fold(Vec::new(), |mut areas, area| {
            push_unique_ci(&mut areas, &area.canonical);
            areas
        });
    let areas = plan
        .areas
        .iter()
        .filter(|area| area.polarity == MentionPolarity::Positive)
        .fold(Vec::new(), |mut areas, area| {
            push_unique_ci(&mut areas, &area.canonical);
            areas
        });
    let area = if areas.len() == 1 {
        areas.first().cloned()
    } else {
        None
    };
    let bhks = plan.slots.bhks.iter().fold(Vec::new(), |mut values, slot| {
        if slot.polarity == SlotPolarity::Include && !values.contains(&slot.value) {
            values.push(slot.value);
        }
        values
    });
    let exclude_bhks = plan.slots.bhks.iter().fold(Vec::new(), |mut values, slot| {
        if slot.polarity == SlotPolarity::Exclude && !values.contains(&slot.value) {
            values.push(slot.value);
        }
        values
    });
    let bhk = if bhks.len() == 1 {
        bhks.first().copied()
    } else {
        None
    };
    let budget_min = plan.slots.budget_min.as_ref().map(|slot| slot.value);
    let budget_max = plan.slots.budget_max.as_ref().map(|slot| slot.value);
    let hard_constraints = plan
        .evidence
        .iter()
        .map(|matched| matched.constraint.clone())
        .collect();
    let positive_preferences = detect_positive_preferences(&q, plan, bhk);
    let accepted_tradeoffs = detect_accepted_tradeoffs(&q);
    let negative_preferences = detect_negative_preferences(&q, plan, bhk)
        .into_iter()
        .filter(|pref| {
            !accepted_tradeoffs
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(&pref.raw_text))
        })
        .collect::<Vec<_>>();
    let positive_preferences = remove_positive_preferences_conflicting_with_negatives(
        positive_preferences,
        &negative_preferences,
    );
    let unsupported_inventory_types = detect_unsupported_inventory_types(&q);
    let buyer_archetype = detect_buyer_archetype(&q, plan);
    let preferences = display_preferences(&positive_preferences, &negative_preferences);
    let bhk_spans = plan
        .slots
        .bhks
        .iter()
        .filter(|slot| slot.end > slot.start)
        .map(|slot| SourceSpan {
            start: slot.start,
            end: slot.end,
            raw_text: slot.raw_text.clone(),
        })
        .collect();

    SearchIntent {
        area,
        excluded_areas,
        excluded_societies: Vec::new(),
        excluded_builders: Vec::new(),
        areas,
        bhk,
        bhks,
        exclude_bhks,
        bhk_spans,
        budget_min,
        budget_max,
        hard_constraints,
        preferences,
        positive_preferences,
        negative_preferences,
        accepted_tradeoffs,
        unsupported_inventory_types,
        buyer_archetype,
    }
}

pub(crate) fn unresolved_named_entity_clause(
    query: &str,
    plan: &QueryPlan,
    clause_has_resolution: impl Fn(&QueryRelationClause) -> bool,
    resolved_entity_in_span: impl Fn(ByteSpan) -> bool,
) -> Option<String> {
    let query_lower = query.to_ascii_lowercase();
    let budget_start = plan
        .slots
        .budget_min
        .iter()
        .chain(plan.slots.budget_max.iter())
        .flat_map(|budget| exact_pattern_match_ranges(&query_lower, &budget.raw_text))
        .map(|(start, _)| start)
        .min();
    let budget_operator_start = plan
        .slots
        .budgets
        .iter()
        .filter_map(|budget| {
            search_parser_config()
                .budget
                .operators
                .iter()
                .chain(search_parser_config().budget.min_operators.iter())
                .flat_map(|operator| exact_pattern_match_ranges(&query_lower, operator))
                .filter(|(_, end)| {
                    *end <= budget.start
                        && query[*end..budget.start]
                            .chars()
                            .all(|character| !character.is_ascii_alphanumeric())
                })
                .map(|(start, _)| start)
                .max()
        })
        .min();
    let first_relation_start = plan
        .clauses
        .iter()
        .map(|clause| clause.relation_span.start)
        .min();

    for prefix in &search_resolution_config().named_entity_scope_prefixes {
        for (prefix_start, prefix_end) in scope_prefix_match_ranges(&query_lower, prefix) {
            if match_has_exclusion_prefix(&query_lower, prefix_start) {
                continue;
            }
            if first_relation_start.is_some_and(|relation_start| prefix_end > relation_start) {
                continue;
            }
            let structured_clause_end = plan
                .slots
                .bhks
                .iter()
                .map(|slot| slot.start)
                .chain(plan.slots.budgets.iter().map(|slot| slot.start))
                .chain(plan.evidence.iter().map(|evidence| evidence.start))
                .chain(budget_start)
                .chain(budget_operator_start)
                .chain(first_relation_start)
                .filter(|end| *end > prefix_end)
                .chain(
                    plan.tokens
                        .iter()
                        .filter(|token| token.start > prefix_end)
                        .filter(|token| {
                            matches!(
                                token.text.to_ascii_lowercase().as_str(),
                                "with" | "prefer" | "but" | "and" | "or"
                            )
                        })
                        .map(|token| token.start),
                )
                .min()
                .unwrap_or(query.len());
            let punctuation_end = query[prefix_end..structured_clause_end]
                .char_indices()
                .find(|(_, character)| matches!(character, ',' | ';'))
                .map(|(offset, _)| prefix_end + offset);
            let clause_end =
                punctuation_end.map_or(structured_clause_end, |end| end.min(structured_clause_end));
            let span = ByteSpan {
                start: prefix_end,
                end: clause_end,
            };
            if unresolved_clause(query, span, &resolved_entity_in_span, false) {
                return Some(query[prefix_end..clause_end].trim().to_string());
            }
        }
    }

    for clause in &plan.clauses {
        if clause_has_resolution(clause) {
            continue;
        }
        if unresolved_clause(query, clause.target_span, &resolved_entity_in_span, true) {
            return Some(clause.target_text.clone());
        }
    }

    None
}

pub(crate) fn unresolved_residual_clause(
    query: &str,
    plan: &QueryPlan,
    resolved_entity_in_span: impl Fn(ByteSpan) -> bool,
) -> Option<String> {
    // This fallback protects direct, unqualified project names that appear
    // before the first inventory or location constraint. Later residual prose
    // belongs to ordinary preference/evidence parsing and is not, by itself,
    // evidence of an unresolved entity name.
    let direct_name_prefix_end = plan
        .areas
        .iter()
        .map(|area| area.span.start)
        .chain(plan.slots.bhks.iter().map(|slot| slot.start))
        .chain(plan.slots.budgets.iter().map(|slot| slot.start))
        .chain(plan.clauses.iter().map(|clause| clause.relation_span.start))
        .min()
        .unwrap_or(query.len());
    let mut claimed_ranges = plan
        .areas
        .iter()
        .map(|area| area.span)
        .chain(plan.slots.bhks.iter().map(|slot| ByteSpan {
            start: slot.start,
            end: slot.end,
        }))
        .chain(plan.slots.budgets.iter().map(|slot| ByteSpan {
            start: slot.start,
            end: slot.end,
        }))
        .chain(plan.evidence.iter().map(|evidence| ByteSpan {
            start: evidence.start,
            end: evidence.end,
        }))
        .chain(plan.clauses.iter().map(|clause| ByteSpan {
            start: clause.relation_span.start,
            end: clause.target_span.end,
        }))
        .collect::<Vec<_>>();
    let query_lower = query.to_ascii_lowercase();
    for pattern in schema::positive_preference_patterns()
        .iter()
        .chain(schema::negative_preference_patterns())
        .flat_map(|entry| entry.patterns.iter())
        .chain(
            schema::preference_key_overrides()
                .iter()
                .flat_map(|entry| entry.patterns.iter()),
        )
    {
        claimed_ranges.extend(
            exact_pattern_match_ranges(&query_lower, pattern)
                .into_iter()
                .map(|(start, end)| ByteSpan { start, end }),
        );
    }

    let ignored_tokens = residual_ignored_tokens();
    let mut residual = Vec::new();
    for token in plan
        .tokens
        .iter()
        .filter(|token| token.start < direct_name_prefix_end)
    {
        let span = ByteSpan {
            start: token.start,
            end: token.end,
        };
        let unexplained = token
            .text
            .chars()
            .any(|character| character.is_ascii_alphabetic())
            && !ignored_tokens
                .iter()
                .any(|ignored| ignored.eq_ignore_ascii_case(&token.text))
            && !claimed_ranges
                .iter()
                .any(|claimed| claimed.start <= token.start && claimed.end >= token.end)
            && !resolved_entity_in_span(span);
        if unexplained {
            residual.push(token);
        } else if !residual.is_empty() {
            break;
        }
    }
    if residual.len() < 2 {
        return None;
    }
    let residual_span = ByteSpan {
        start: residual[0].start,
        end: residual[residual.len() - 1].end,
    };
    if resolved_entity_in_span(residual_span) {
        return None;
    }
    Some(
        query[residual_span.start..residual_span.end]
            .trim()
            .to_string(),
    )
}

fn residual_ignored_tokens() -> Vec<String> {
    let resolution = search_resolution_config();
    let parser = search_parser_config();
    let guardrails = search_guardrail_config();
    let mut ignored = resolution
        .ignored_entity_names
        .iter()
        .chain(resolution.residual_ignored_terms.iter())
        .chain(resolution.generic_scope_nouns.iter())
        .flat_map(|value| parser::query_tokens(value))
        .collect::<Vec<_>>();
    for value in parser
        .bhk
        .unit_aliases
        .iter()
        .chain(parser.bhk.alternative_joiners.iter())
        .chain(parser.budget.operators.iter())
        .chain(parser.budget.min_operators.iter())
        .chain(parser.budget.range_connectors.iter())
        .chain(parser.distance.operators.iter())
        .chain(parser.relations.clause_joiners.iter())
        .chain(parser.discourse.shared_suffix_markers.iter())
        .chain(parser.relations.aliases.iter().map(|alias| &alias.alias))
        .chain(
            parser
                .budget
                .units
                .iter()
                .chain(parser.distance.units.iter())
                .flat_map(|unit| unit.aliases.iter()),
        )
        .chain(
            guardrails
                .home_intent_detection
                .term_groups
                .iter()
                .flat_map(|group| group.terms.iter().chain(group.substrings.iter())),
        )
        .chain(guardrails.home_intent_detection.weak_anchor_terms.iter())
    {
        for token in parser::query_tokens(value) {
            push_unique_ci(&mut ignored, &token);
        }
    }
    ignored
}

fn relation_clauses(
    query: &str,
    tokens: &[QueryToken],
    slots: &parser::ParsedQuerySlots,
) -> Vec<QueryRelationClause> {
    let token_values = tokens
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    let mut clauses = slots
        .relations
        .iter()
        .enumerate()
        .flat_map(|(index, relation)| {
            let relation_span = token_span(tokens, relation.start_token, relation.end_token)?;
            let full_target_span = trimmed_token_span(
                tokens,
                relation.target_start_token,
                relation.target_end_token,
            )?;
            let relation_config = search_parser_config()
                .relations
                .aliases
                .iter()
                .find(|alias| alias.alias.eq_ignore_ascii_case(&relation.alias));
            let distance_limit_km =
                distance_limit_for_relation(tokens, relation.end_token, relation.target_end_token)
                    .or_else(|| {
                        trailing_distance_limit_after_target(tokens, relation.target_end_token)
                    })
                    .or_else(|| trailing_distance_limit(query, slots, full_target_span))
                    .or_else(|| relation_config.and_then(|alias| alias.default_distance_limit_km));
            let requires_distance =
                relation_config.is_some_and(|alias| alias.requires_distance_limit);
            let requirement = if requires_distance || distance_limit_km.is_some() {
                RelationRequirement::Hard
            } else {
                RelationRequirement::Coverage
            };
            let segments = relation_target_segments(tokens, relation);
            let mut clauses = Vec::new();
            for (segment_index, (segment_start, segment_end)) in segments.into_iter().enumerate() {
                let Some(target_span) = trimmed_token_span(tokens, segment_start, segment_end)
                else {
                    continue;
                };
                if target_span.start < full_target_span.start
                    || target_span.end > full_target_span.end
                {
                    continue;
                }
                let target_text = token_values[segment_start..segment_end]
                    .join(" ")
                    .trim()
                    .to_string();
                if target_text.is_empty() {
                    continue;
                }
                clauses.push(QueryRelationClause {
                    id: if segment_index == 0 {
                        format!("rel:{index}")
                    } else {
                        format!("rel:{index}:{segment_index}")
                    },
                    relation: relation.alias.clone(),
                    relation_span,
                    target_text,
                    target_span,
                    place_family_id: place_family_for_target(
                        &query[target_span.start..target_span.end],
                    ),
                    distance_limit_km,
                    requirement,
                });
            }
            Some(clauses)
        })
        .flatten()
        .collect::<Vec<_>>();
    if clauses.len() >= 2
        && search_parser_config()
            .discourse
            .conjunctive_relation_markers
            .iter()
            .any(|marker| {
                !exact_pattern_match_ranges(&query.to_ascii_lowercase(), marker).is_empty()
            })
    {
        for clause in &mut clauses {
            clause.requirement = RelationRequirement::Hard;
        }
    }
    clauses
}

fn relation_target_segments(
    tokens: &[QueryToken],
    relation: &parser::RelationIntent,
) -> Vec<(usize, usize)> {
    let joiners = &search_parser_config().relations.clause_joiners;
    let mut segments = Vec::new();
    let mut start = relation.target_start_token.min(tokens.len());
    let end = relation.target_end_token.min(tokens.len());
    let mut index = start;
    while index < end {
        if joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[index].text))
        {
            if start < index {
                segments.push((start, index));
            }
            start = index + 1;
        }
        index += 1;
    }
    if start < end {
        segments.push((start, end));
    }
    if segments.is_empty() {
        vec![(relation.target_start_token, relation.target_end_token)]
    } else {
        segments
    }
}

fn distance_limit_for_relation(tokens: &[QueryToken], start: usize, end: usize) -> Option<f64> {
    let end = end.min(tokens.len());
    let mut index = start.min(end);
    while index < end {
        if let Some((value, unit_len)) = distance_at(tokens, index) {
            return Some(value * unit_len);
        }
        index += 1;
    }
    None
}

fn distance_at(tokens: &[QueryToken], index: usize) -> Option<(f64, f64)> {
    let token = tokens.get(index)?.text.as_str();
    let compact_start = token.find(|ch: char| !(ch.is_ascii_digit() || ch == '.'));
    if let Some(unit_start) = compact_start {
        let (number, unit) = token.split_at(unit_start);
        let value = number.parse::<f64>().ok()?;
        let multiplier = parser::distance_unit_multiplier(unit)?;
        return Some((value, multiplier));
    }

    let value = token.parse::<f64>().ok()?;
    let unit = tokens.get(index + 1)?;
    let multiplier = parser::distance_unit_multiplier(&unit.text)?;
    Some((value, multiplier))
}

fn trailing_distance_limit(
    query: &str,
    slots: &parser::ParsedQuerySlots,
    target_span: ByteSpan,
) -> Option<f64> {
    if slots.relations.len() != 1 {
        return None;
    }
    let distance = slots.distance_limit.as_ref()?;
    let query_lower = query.to_ascii_lowercase();
    exact_pattern_match_ranges(&query_lower, &distance.raw_text)
        .into_iter()
        .any(|(start, _)| start >= target_span.end)
        .then_some(distance.value_km)
}

fn trailing_distance_limit_after_target(tokens: &[QueryToken], start: usize) -> Option<f64> {
    let mut index = start.min(tokens.len());
    let operator_len = distance_operator_len_at(tokens, index)?;
    index += operator_len;
    let end = next_clause_joiner(tokens, index).unwrap_or(tokens.len());
    while index < end {
        if let Some((value, unit_len)) = distance_at(tokens, index) {
            return Some(value * unit_len);
        }
        index += 1;
    }
    None
}

fn distance_operator_len_at(tokens: &[QueryToken], index: usize) -> Option<usize> {
    search_parser_config()
        .distance
        .operators
        .iter()
        .filter_map(|operator| {
            let operator_tokens = parser::query_tokens(operator);
            if operator_tokens.is_empty() || index + operator_tokens.len() > tokens.len() {
                return None;
            }
            tokens[index..index + operator_tokens.len()]
                .iter()
                .zip(operator_tokens.iter())
                .all(|(token, operator)| token.text.eq_ignore_ascii_case(operator))
                .then_some(operator_tokens.len())
        })
        .max()
}

fn next_clause_joiner(tokens: &[QueryToken], start: usize) -> Option<usize> {
    let joiners = &search_parser_config().relations.clause_joiners;
    tokens
        .iter()
        .enumerate()
        .skip(start.min(tokens.len()))
        .find(|(_, token)| {
            joiners
                .iter()
                .any(|joiner| joiner.eq_ignore_ascii_case(&token.text))
        })
        .map(|(index, _)| index)
}

fn token_span(tokens: &[QueryToken], start: usize, end: usize) -> Option<ByteSpan> {
    if start >= end || end > tokens.len() {
        return None;
    }
    Some(ByteSpan {
        start: tokens[start].start,
        end: tokens[end - 1].end,
    })
}

fn trimmed_token_span(tokens: &[QueryToken], start: usize, end: usize) -> Option<ByteSpan> {
    let joiners = &search_parser_config().relations.clause_joiners;
    let mut start = start.min(tokens.len());
    let mut end = end.min(tokens.len());
    while start < end
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[start].text))
    {
        start += 1;
    }
    while end > start
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[end - 1].text))
    {
        end -= 1;
    }
    token_span(tokens, start, end)
}

fn detect_area_mentions(query: &str) -> Vec<AreaMention> {
    let query_lower = query.to_lowercase();
    let mut mentions = Vec::new();
    for entry in area_alias_entries() {
        for alias in &entry.aliases {
            for (start, end) in exact_pattern_match_ranges(&query_lower, alias) {
                mentions.push(AreaMention {
                    canonical: entry.canonical.clone(),
                    matched_text: query[start..end].to_string(),
                    span: ByteSpan { start, end },
                    polarity: if match_has_exclusion_prefix(&query_lower, start) {
                        MentionPolarity::Exclusion
                    } else {
                        MentionPolarity::Positive
                    },
                });
            }
        }
    }
    mentions.sort_by(|left, right| {
        right
            .span
            .end
            .saturating_sub(right.span.start)
            .cmp(&left.span.end.saturating_sub(left.span.start))
            .then_with(|| left.span.start.cmp(&right.span.start))
            .then_with(|| left.canonical.cmp(&right.canonical))
    });

    let mut selected = Vec::new();
    for mention in mentions {
        if selected
            .iter()
            .any(|existing: &AreaMention| spans_overlap(existing.span, mention.span))
        {
            continue;
        }
        selected.push(mention);
    }
    selected
}

fn detect_positive_preferences(
    q: &str,
    plan: &QueryPlan,
    bhk: Option<u32>,
) -> Vec<PreferenceSignal> {
    let mut prefs = Vec::new();
    for pattern in schema::positive_preference_patterns() {
        if !pattern
            .patterns
            .iter()
            .any(|term| query_contains_unnegated_pattern(q, term, plan))
        {
            continue;
        }

        let mut signal = schema::schema_preference_signal(pattern, Polarity::Positive);
        apply_preference_key_overrides(q, plan, &mut signal);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }

    for override_rule in schema::preference_key_overrides() {
        if !query_contains_any_pattern(q, &override_rule.patterns, plan)
            || prefs.iter().any(|pref| {
                pref.raw_text
                    .eq_ignore_ascii_case(&override_rule.preference)
            })
        {
            continue;
        }
        let Some(pattern) = schema::positive_preference_patterns()
            .iter()
            .find(|pattern| {
                pattern
                    .label
                    .eq_ignore_ascii_case(&override_rule.preference)
            })
        else {
            continue;
        };

        let mut signal = schema::schema_preference_signal(pattern, Polarity::Positive);
        apply_preference_key_overrides(q, plan, &mut signal);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }
    prefs
}

fn detect_negative_preferences(
    q: &str,
    plan: &QueryPlan,
    bhk: Option<u32>,
) -> Vec<PreferenceSignal> {
    let mut prefs = Vec::new();
    for pattern in schema::negative_preference_patterns() {
        if !pattern
            .patterns
            .iter()
            .any(|term| query_contains_pattern(q, term, plan))
        {
            continue;
        }

        let mut signal = schema::schema_preference_signal(pattern, Polarity::Negative);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }

    for pattern in schema::positive_preference_patterns() {
        let negated = pattern
            .patterns
            .iter()
            .any(|term| query_contains_negated_pattern(q, term, plan));
        if !negated {
            continue;
        }

        let mut signal = negated_positive_preference_signal(pattern);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }
    prefs
}

fn apply_bhk_fact_key_derivations(bhk: Option<u32>, signal: &mut PreferenceSignal) {
    let Some(bhk) = bhk else {
        return;
    };
    let keys = signal.expanded_keys.clone();
    for key in keys {
        let derived_keys = schema::derived_fact_keys_for_bhk(&key, bhk);
        merge_expanded_keys(signal, &derived_keys);
    }
}

fn apply_preference_key_overrides(q: &str, plan: &QueryPlan, signal: &mut PreferenceSignal) {
    let mut matching_overrides = Vec::new();
    for override_rule in schema::preference_key_overrides() {
        if !override_rule
            .preference
            .eq_ignore_ascii_case(&signal.raw_text)
        {
            continue;
        }

        let ranges = override_rule
            .patterns
            .iter()
            .flat_map(|pattern| query_pattern_match_ranges(q, pattern, plan))
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            continue;
        }
        matching_overrides.push((override_rule, ranges));
    }

    let selected = matching_overrides
        .iter()
        .enumerate()
        .filter(|(index, (_, ranges))| {
            ranges.iter().any(|candidate| {
                !matching_overrides
                    .iter()
                    .enumerate()
                    .any(|(other_index, (_, other_ranges))| {
                        other_index != *index
                            && other_ranges.iter().any(|other| {
                                other.0 <= candidate.0
                                    && other.1 >= candidate.1
                                    && (other.1 - other.0) > (candidate.1 - candidate.0)
                            })
                    })
            })
        })
        .map(|(_, (override_rule, _))| *override_rule)
        .collect::<Vec<_>>();
    if !selected.is_empty() {
        signal.expanded_keys.clear();
        signal.gap_keys.clear();
        for override_rule in selected {
            merge_expanded_keys(signal, &override_rule.expanded_keys);
            merge_gap_keys(signal, &override_rule.gap_keys);
        }
    }
}

fn merge_expanded_keys(signal: &mut PreferenceSignal, keys: &[String]) {
    for key in keys {
        if !signal.expanded_keys.iter().any(|existing| existing == key) {
            signal.expanded_keys.push(key.clone());
        }
    }
}

fn merge_gap_keys(signal: &mut PreferenceSignal, keys: &[String]) {
    for key in keys {
        if !signal.gap_keys.iter().any(|existing| existing == key) {
            signal.gap_keys.push(key.clone());
        }
    }
}

fn merge_or_push_preference(prefs: &mut Vec<PreferenceSignal>, signal: PreferenceSignal) {
    if let Some(existing) = prefs
        .iter_mut()
        .find(|pref| pref.raw_text.eq_ignore_ascii_case(&signal.raw_text))
    {
        merge_expanded_keys(existing, &signal.expanded_keys);
        merge_gap_keys(existing, &signal.gap_keys);
        existing.weight = existing.weight.max(signal.weight);
        existing.required |= signal.required;
        existing.missing_evidence_neutral |= signal.missing_evidence_neutral;
    } else {
        prefs.push(signal);
    }
}

fn negated_positive_preference_signal(pattern: &schema::PreferencePatternSpec) -> PreferenceSignal {
    let mut signal = schema::schema_preference_signal(pattern, Polarity::Negative);
    // An explicit negation is an exclusion, not a request to softly prefer an
    // adjacent concern. Keep the exact configured dimension and require proof
    // that the excluded value is absent.
    signal.required = true;
    signal
}

fn remove_positive_preferences_conflicting_with_negatives(
    positive_preferences: Vec<PreferenceSignal>,
    negative_preferences: &[PreferenceSignal],
) -> Vec<PreferenceSignal> {
    if negative_preferences.is_empty() {
        return positive_preferences;
    }
    positive_preferences
        .into_iter()
        .filter(|positive| {
            !negative_preferences
                .iter()
                .any(|negative| preferences_conflict(positive, negative))
        })
        .collect()
}

fn preferences_conflict(positive: &PreferenceSignal, negative: &PreferenceSignal) -> bool {
    if positive.raw_text.eq_ignore_ascii_case(&negative.raw_text) {
        return true;
    }

    let positive_specific_keys = positive
        .expanded_keys
        .iter()
        .filter(|key| is_specific_conflict_key(key))
        .collect::<Vec<_>>();
    !positive_specific_keys.is_empty()
        && positive_specific_keys.iter().all(|positive_key| {
            negative.expanded_keys.iter().any(|negative_key| {
                is_specific_conflict_key(negative_key)
                    && positive_key.eq_ignore_ascii_case(negative_key)
            })
        })
}

fn is_specific_conflict_key(key: &str) -> bool {
    !matches!(
        key,
        "price_per_sqft"
            | "pricing_insight"
            | "legal.litigation"
            | "legal_risk"
            | "litigation_risk"
            | "resident_sentiment"
            | "google_review_snippets"
            | "sentiment_summary"
            | "water_supply"
            | "water_supply_risk"
    )
}

fn detect_accepted_tradeoffs(q: &str) -> Vec<String> {
    let mut accepted = Vec::new();
    for group in schema::accepted_tradeoffs() {
        if query_contains_any_pattern_unowned(q, &group.patterns) {
            push_unique_ci(&mut accepted, &group.label);
        }
    }
    accepted
}

fn detect_unsupported_inventory_types(q: &str) -> Vec<String> {
    let mut inventory_types = Vec::new();
    for group in schema::unsupported_inventory_types() {
        if query_contains_any_pattern_unowned(q, &group.patterns) {
            push_unique_ci(&mut inventory_types, &group.label);
        }
    }
    inventory_types
}

fn display_preferences(
    positive_preferences: &[PreferenceSignal],
    negative_preferences: &[PreferenceSignal],
) -> Vec<String> {
    positive_preferences
        .iter()
        .map(|p| p.raw_text.clone())
        .chain(
            negative_preferences
                .iter()
                .map(|p| format!("avoid {}", p.raw_text)),
        )
        .collect()
}

fn detect_buyer_archetype(q: &str, plan: &QueryPlan) -> Option<BuyerArchetype> {
    let mut best: Option<(BuyerArchetype, usize)> = None;
    for pattern in schema::buyer_archetype_patterns() {
        for term in &pattern.patterns {
            if !query_contains_unnegated_pattern(q, term, plan) {
                continue;
            }
            let len = term.len();
            if best.as_ref().is_none_or(|(_, best_len)| len > *best_len) {
                best = Some((pattern.archetype.clone(), len));
            }
        }
    }
    best.map(|(archetype, _)| archetype)
}

fn query_contains_pattern(q: &str, pattern: &str, plan: &QueryPlan) -> bool {
    query_pattern_match_ranges(q, pattern, plan)
        .next()
        .is_some()
}

fn query_contains_unnegated_pattern(q: &str, pattern: &str, plan: &QueryPlan) -> bool {
    query_pattern_match_ranges(q, pattern, plan)
        .any(|(start, _)| !match_has_negated_prefix(q, start))
}

fn query_contains_negated_pattern(q: &str, pattern: &str, plan: &QueryPlan) -> bool {
    query_pattern_match_ranges(q, pattern, plan)
        .any(|(start, _)| match_has_negated_prefix(q, start))
}

fn query_contains_any_pattern(q: &str, patterns: &[String], plan: &QueryPlan) -> bool {
    patterns
        .iter()
        .any(|pattern| query_contains_pattern(q, pattern, plan))
}

fn query_contains_any_pattern_unowned(q: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| !exact_pattern_match_ranges(q, pattern).is_empty())
}

fn query_pattern_match_ranges<'a>(
    q: &'a str,
    pattern: &'a str,
    plan: &'a QueryPlan,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    let exact = exact_pattern_match_ranges(q, pattern).into_iter();
    let stemmed = if super::analyzer::stemmed_tokens(pattern).len() >= 2 {
        super::analyzer::stemmed_phrase_match_ranges(q, pattern)
    } else {
        Vec::new()
    };
    exact.chain(stemmed).filter(move |range| {
        !span_is_owned_by_entity(
            plan,
            ByteSpan {
                start: range.0,
                end: range.1,
            },
        )
    })
}

fn span_is_owned_by_entity(plan: &QueryPlan, span: ByteSpan) -> bool {
    plan.owned_spans
        .iter()
        .any(|owned| spans_overlap(owned.span, span))
}

fn match_has_negated_prefix(q: &str, start: usize) -> bool {
    let mut prefix = q[..start].trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',');
    loop {
        let Some(last_token) = prefix
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .next_back()
        else {
            break;
        };
        if !search_parser_config()
            .bhk
            .exclusion_gap_tokens
            .iter()
            .any(|gap| gap.eq_ignore_ascii_case(last_token))
        {
            break;
        }
        let Some(token_start) = prefix.rfind(last_token) else {
            break;
        };
        prefix = prefix[..token_start]
            .trim_end_matches(|character: char| !character.is_ascii_alphanumeric());
    }
    if search_resolution_config()
        .exclusion_prefixes
        .iter()
        .any(|phrase| prefix_ends_with_phrase(prefix, phrase))
    {
        return true;
    }

    let scope_start = q[..start]
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '.' | '?' | '!' | ';'))
        .map_or(0, |(index, character)| index + character.len_utf8());
    let scoped_prefix = &q[scope_start..start];
    let contrast_start = exact_pattern_match_ranges(scoped_prefix, "but")
        .into_iter()
        .map(|(start, _)| start)
        .max()
        .unwrap_or(0);
    search_resolution_config()
        .exclusion_prefixes
        .iter()
        .flat_map(|phrase| exact_pattern_match_ranges(scoped_prefix, phrase))
        .any(|(exclusion_start, _)| exclusion_start >= contrast_start)
}

fn match_has_exclusion_prefix(query_lower: &str, start: usize) -> bool {
    match_has_negated_prefix(query_lower, start)
}

fn prefix_ends_with_phrase(prefix: &str, phrase: &str) -> bool {
    let Some(before_phrase) = prefix.strip_suffix(phrase) else {
        return false;
    };
    before_phrase
        .chars()
        .next_back()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric())
}

fn unresolved_clause(
    query: &str,
    span: ByteSpan,
    resolved_entity_in_span: &impl Fn(ByteSpan) -> bool,
    allow_place_family: bool,
) -> bool {
    let clause = query[span.start..span.end].trim();
    if clause.is_empty()
        || !clause
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    if !allow_place_family && is_generic_scope_clause(clause) {
        return false;
    }
    if resolved_entity_in_span(span) {
        return false;
    }
    if allow_place_family
        && place_family_for_target(clause).is_some()
        && !clause_target_has_identity_tokens(clause)
    {
        return false;
    }
    true
}

fn clause_target_has_identity_tokens(target: &str) -> bool {
    let Some(family_id) = place_family_for_target(target) else {
        return false;
    };
    let config = search_resolution_config();
    let mut generic_tokens = config
        .ignored_entity_names
        .iter()
        .chain(config.generic_scope_nouns.iter())
        .flat_map(|term| parser::query_tokens(term))
        .collect::<Vec<_>>();
    if let Some(family) = config
        .place_families
        .iter()
        .find(|family| family.id == family_id)
    {
        for alias in &family.aliases {
            for token in parser::query_tokens(alias) {
                push_unique_ci(&mut generic_tokens, &token);
            }
        }
    }
    for stopword in &schema::ranking_policy().named_place_query_stopwords {
        for token in parser::query_tokens(stopword) {
            push_unique_ci(&mut generic_tokens, &token);
        }
    }

    parser::query_tokens(target).into_iter().any(|token| {
        token
            .chars()
            .any(|character| character.is_ascii_alphabetic())
            && !generic_tokens
                .iter()
                .any(|generic| generic.eq_ignore_ascii_case(&token))
    })
}

fn is_generic_scope_clause(clause: &str) -> bool {
    let config = search_resolution_config();
    parser::query_tokens(clause)
        .into_iter()
        .find(|token| {
            !config
                .ignored_entity_names
                .iter()
                .any(|ignored| ignored.eq_ignore_ascii_case(token))
        })
        .is_some_and(|token| {
            config
                .generic_scope_nouns
                .iter()
                .any(|noun| noun.eq_ignore_ascii_case(&token))
        })
}

fn place_family_for_target(target: &str) -> Option<String> {
    let target_lower = target.to_ascii_lowercase();
    search_resolution_config()
        .place_families
        .iter()
        .find(|family| {
            family
                .aliases
                .iter()
                .any(|alias| !exact_pattern_match_ranges(&target_lower, alias).is_empty())
        })
        .map(|family| family.id.clone())
}

fn scope_prefix_match_ranges(query_lower: &str, prefix: &str) -> Vec<(usize, usize)> {
    exact_pattern_match_ranges(query_lower, prefix)
        .into_iter()
        .filter(|(start, end)| {
            !query_lower[..*start].ends_with('-') && !query_lower[*end..].starts_with('-')
        })
        .collect()
}

fn exact_pattern_match_ranges(q: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern = pattern.trim().to_ascii_lowercase();
    let pattern_len = pattern.len();
    let mut search_start = 0;
    let mut ranges = Vec::new();
    if pattern.is_empty() {
        return ranges;
    }

    while search_start < q.len() {
        let Some(relative_pos) = q[search_start..].find(&pattern) else {
            break;
        };
        let start = search_start + relative_pos;
        let end = start + pattern_len;
        search_start = end;

        let before_ok = q[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        let after_ok = q[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());

        if before_ok && after_ok {
            ranges.push((start, end));
        }
    }

    ranges
}

fn spans_overlap(left: ByteSpan, right: ByteSpan) -> bool {
    left.start < right.end && right.start < left.end
}

fn push_unique_ci(values: &mut Vec<String>, value: &str) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

fn query_tokens_with_spans(query: &str) -> Vec<QueryToken> {
    parser::query_tokens_spanned(query)
        .into_iter()
        .map(|token| QueryToken {
            text: token.text,
            start: token.start,
            end: token.end,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestone_m1_region_and_facing_spans_do_not_conflict() {
        let cases = [
            ("3BHK in East Bengaluru", Some("East Bengaluru")),
            ("east-facing 3BHK in East Bengaluru", Some("East Bengaluru")),
            ("west-facing 3BHK in East Bengaluru", Some("East Bengaluru")),
            ("3BHK near Whitefield", None),
            ("3BHK in Whitefield", None),
        ];

        for (query, expected_area) in cases {
            let plan = compile_query_plan(query);
            let intent = project_search_intent(query, &plan);
            assert_eq!(intent.bhk, Some(3), "{query}");
            assert_eq!(intent.area.as_deref(), expected_area, "{query}");
        }
    }

    #[test]
    fn projects_multiple_positive_areas_as_any_of() {
        let intent = project_search_intent(
            "3BHK in East Bengaluru or South Bengaluru",
            &compile_query_plan("3BHK in East Bengaluru or South Bengaluru"),
        );
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area, None);
        assert_eq!(intent.areas.len(), 2);
        assert!(intent.areas.iter().any(|area| area == "East Bengaluru"));
        assert!(intent.areas.iter().any(|area| area == "South Bengaluru"));
    }

    #[test]
    fn compiles_relation_clauses_with_distance_binding_and_place_roles() {
        let query = "3BHK near Whitefield close to kids school and near my wife's office in Marathahalli under 4 crore";
        let plan = compile_query_plan(query);

        assert_eq!(plan.clauses.len(), 3);
        assert_eq!(plan.clauses[0].target_text, "whitefield");
        assert_eq!(plan.clauses[1].target_text, "kids school");
        assert_eq!(plan.clauses[1].place_family_id.as_deref(), Some("school"));
        assert_eq!(
            plan.clauses[2].target_text,
            "my wife's office in marathahalli"
        );
        assert_eq!(
            plan.clauses[2].place_family_id.as_deref(),
            Some("tech_park")
        );
        assert_eq!(
            plan.slots.budget_max.as_ref().map(|budget| budget.value),
            Some(40_000_000)
        );
    }

    #[test]
    fn compiles_conjoined_relation_targets_as_separate_clauses() {
        let plan = compile_query_plan(
            "3BHK near Manipal Hospital Whitefield and Aster Hospital under 4 crore",
        );

        assert_eq!(plan.clauses.len(), 2);
        assert_eq!(plan.clauses[0].target_text, "manipal hospital whitefield");
        assert_eq!(plan.clauses[0].place_family_id.as_deref(), Some("hospital"));
        assert_eq!(plan.clauses[0].requirement, RelationRequirement::Coverage);
        assert_eq!(plan.clauses[1].target_text, "aster hospital");
        assert_eq!(plan.clauses[1].place_family_id.as_deref(), Some("hospital"));
        assert_eq!(plan.clauses[1].requirement, RelationRequirement::Coverage);
        assert_eq!(
            plan.slots.budget_max.as_ref().map(|budget| budget.value),
            Some(40_000_000)
        );
    }

    #[test]
    fn compiles_within_distances_as_hard_relation_clauses() {
        let plan =
            compile_query_plan("3BHK within 1 km of Manipal Hospital and within 3 km of ITPB");

        assert_eq!(plan.clauses.len(), 2);
        assert_eq!(plan.clauses[0].target_text, "manipal hospital");
        assert_eq!(plan.clauses[0].distance_limit_km, Some(1.0));
        assert_eq!(plan.clauses[0].requirement, RelationRequirement::Hard);
        assert_eq!(plan.clauses[1].target_text, "itpb");
        assert_eq!(plan.clauses[1].distance_limit_km, Some(3.0));
        assert_eq!(plan.clauses[1].requirement, RelationRequirement::Hard);
    }

    #[test]
    fn compiles_repeated_trailing_distance_modifiers_per_clause() {
        let plan =
            compile_query_plan("3BHK near Deens Academy within 1 km and near ITPB within 3 km");

        assert_eq!(plan.clauses.len(), 2);
        assert_eq!(plan.clauses[0].target_text, "deens academy");
        assert_eq!(plan.clauses[0].distance_limit_km, Some(1.0));
        assert_eq!(plan.clauses[0].requirement, RelationRequirement::Hard);
        assert_eq!(plan.clauses[1].target_text, "itpb");
        assert_eq!(plan.clauses[1].distance_limit_km, Some(3.0));
        assert_eq!(plan.clauses[1].requirement, RelationRequirement::Hard);
    }

    #[test]
    fn positive_relation_clause_does_not_swallow_away_from_risk_clause() {
        let plan = compile_query_plan("3BHK near Marathahalli but away from a stormwater drain");

        assert_eq!(plan.clauses.len(), 1);
        assert_eq!(plan.clauses[0].target_text, "marathahalli");
    }

    #[test]
    fn undistanced_within_phrase_does_not_create_hard_clause() {
        let plan = compile_query_plan("within a gated community and near school within 2 km");

        assert_eq!(plan.clauses.len(), 1);
        assert_eq!(plan.clauses[0].target_text, "school");
        assert_eq!(plan.clauses[0].distance_limit_km, Some(2.0));
        assert_eq!(plan.clauses[0].requirement, RelationRequirement::Hard);
    }

    #[test]
    fn owned_entity_spans_do_not_become_preferences() {
        let query = "3BHK in East Bengaluru";
        let plan = compile_query_plan(query);
        let intent = project_search_intent(query, &plan);

        assert!(!intent
            .preferences
            .iter()
            .any(|preference| preference.eq_ignore_ascii_case("east facing")));
    }

    #[test]
    fn owned_bhk_spans_do_not_become_configuration_preferences() {
        let query = "2 or 3 BHK in Whitefield";
        let plan = compile_query_plan(query);
        assert!(plan
            .owned_spans
            .iter()
            .any(|owned| owned.owner == SpanOwner::Bhk));
        let intent = project_search_intent(query, &plan);
        assert!(intent.preferences.iter().all(|preference| {
            !preference.eq_ignore_ascii_case("2bhk configuration")
                && !preference.eq_ignore_ascii_case("3bhk configuration")
        }));
    }

    #[test]
    fn named_entity_scope_stops_before_structured_constraints() {
        for query in [
            "3bhk with greenery in whitefield above 10 acres",
            "in whitefield 3bhk under 2 crore",
            "under construction 3bhk in whitefield under 3.1cr",
        ] {
            let plan = compile_query_plan(query);
            let unresolved = unresolved_named_entity_clause(
                query,
                &plan,
                |_| false,
                |span| {
                    query[span.start..span.end]
                        .trim()
                        .eq_ignore_ascii_case("whitefield")
                },
            );

            assert_eq!(unresolved, None, "{query}");
        }
    }

    #[test]
    fn alternative_layout_ignores_or_inside_a_single_constraint_clause() {
        let query = "2 or 3 BHK under 2Cr or 4BHK under 4Cr";
        let plan = compile_query_plan(query);
        let occupied_spans = plan
            .slots
            .bhks
            .iter()
            .map(|slot| SourceSpan {
                start: slot.start,
                end: slot.end,
                raw_text: slot.raw_text.clone(),
            })
            .collect::<Vec<_>>();

        let layout = plan
            .alternative_clause_layout(&occupied_spans, &[occupied_spans.clone()])
            .expect("the cross-clause or should remain");

        assert_eq!(layout.segments.len(), 2);
        assert_eq!(
            &query[layout.segments[0].start..layout.segments[0].end],
            "2 or 3 BHK under 2Cr "
        );
        assert_eq!(query[layout.segments[1].start..].trim(), "4BHK under 4Cr");
    }

    #[test]
    fn alternative_layout_abstains_without_a_top_level_or() {
        let plan = compile_query_plan("3BHK under 2Cr in East Bengaluru");

        assert!(plan.alternative_clause_layout(&[], &[]).is_none());
    }

    #[test]
    fn most_specific_preference_override_wins_for_overlapping_phrases() {
        let query = "3BHK near Bagmane Tech Park";
        let plan = compile_query_plan(query);
        let intent = project_search_intent(query, &plan);
        let social_infrastructure = intent
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "social infrastructure")
            .expect("tech park should create a social infrastructure preference");

        assert!(social_infrastructure
            .expanded_keys
            .iter()
            .any(|key| key == "nearby_tech_parks"));
        assert!(!social_infrastructure
            .expanded_keys
            .iter()
            .any(|key| key == "nearby_public_parks"));
    }

    #[test]
    fn compiles_realistic_buyer_paraphrases_from_configured_families() {
        let cases = [
            (
                "I want to sleep without horns outside",
                "quiet neighborhood",
                Polarity::Positive,
            ),
            (
                "daily gridlock is a deal breaker",
                "traffic",
                Polarity::Negative,
            ),
            (
                "the place should feel consistently cared for",
                "maintenance",
                Polarity::Positive,
            ),
            (
                "show places residents consistently speak well of",
                "review quality",
                Polarity::Positive,
            ),
            (
                "I do not want to lose hours travelling every day",
                "commute",
                Polarity::Positive,
            ),
            (
                "I care about how solidly the homes are built",
                "construction quality",
                Polarity::Positive,
            ),
            (
                "I dislike towers packed tightly together",
                "density risk",
                Polarity::Negative,
            ),
            (
                "rooms should not feel stuffy",
                "ventilation",
                Polarity::Positive,
            ),
            (
                "avoid places where monsoons leave standing water",
                "waterlogging risk",
                Polarity::Negative,
            ),
            (
                "I cannot wait through an uncertain handover",
                "delay risk",
                Polarity::Negative,
            ),
        ];

        for (query, expected_label, polarity) in cases {
            let plan = compile_query_plan(query);
            let intent = project_search_intent(query, &plan);
            let preferences = if polarity == Polarity::Positive {
                &intent.positive_preferences
            } else {
                &intent.negative_preferences
            };

            assert!(
                preferences
                    .iter()
                    .any(|preference| preference.raw_text == expected_label),
                "{query} should compile to {polarity:?} {expected_label}; got {preferences:?}"
            );
        }
    }

    #[test]
    fn preference_sentence_does_not_extend_named_place_relation() {
        let query = "I want a ready-to-move 3BHK near a tech park. Prefer a quieter society with stronger reviews, but don’t hide homes just because noise evidence is missing.";
        let plan = compile_query_plan(query);

        assert_eq!(
            plan.clauses
                .iter()
                .map(|clause| clause.target_text.as_str())
                .collect::<Vec<_>>(),
            vec!["a tech park"]
        );
    }

    #[test]
    fn repeated_named_place_relations_do_not_emit_bridge_prose() {
        let query = "My office is near Bagmane Tech Park and my partner works near Manipal Hospital Whitefield. Find homes with measured distance evidence to both, prioritizing the better-balanced commute.";
        let plan = compile_query_plan(query);

        assert_eq!(
            plan.clauses
                .iter()
                .map(|clause| clause.target_text.as_str())
                .collect::<Vec<_>>(),
            vec!["bagmane tech park", "manipal hospital whitefield"]
        );
    }

    #[test]
    fn pairs_ordinal_constraints_with_comparison_branches() {
        let query = "Compare Godrej Air under ₹2.6 Cr with Godrej Lakeside Orchard under ₹3.1 Cr, but only show 3BHKs in the first and 4BHKs in the second.";

        assert_eq!(
            paired_ordinal_branch_queries(query),
            Some(vec![
                "Compare Godrej Air under ₹2.6 Cr 3BHK".to_string(),
                "Godrej Lakeside Orchard under ₹3.1 Cr 4BHK".to_string(),
            ])
        );
    }
}
