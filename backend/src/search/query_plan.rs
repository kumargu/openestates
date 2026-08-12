use crate::dag_config::{area_alias_entries, search_parser_config, search_resolution_config};

use super::intent::{BuyerArchetype, Polarity, PreferenceSignal, SearchIntent};
use super::{parser, schema};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct QueryPlan {
    pub slots: parser::ParsedQuerySlots,
    pub tokens: Vec<QueryToken>,
    pub areas: Vec<AreaMention>,
    pub clauses: Vec<QueryRelationClause>,
    pub owned_spans: Vec<OwnedSpan>,
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
}

pub(crate) fn compile_query_plan(query: &str) -> QueryPlan {
    let slots = parser::parse_query_slots(query);
    let tokens = query_tokens_with_spans(query);
    let areas = detect_area_mentions(query);
    let clauses = relation_clauses(query, &tokens, &slots);
    let mut owned_spans = Vec::new();
    for area in &areas {
        owned_spans.push(OwnedSpan {
            span: area.span,
            owner: SpanOwner::Area,
        });
    }
    QueryPlan {
        slots,
        tokens,
        areas,
        clauses,
        owned_spans,
    }
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
    let area = plan
        .areas
        .iter()
        .filter(|area| area.polarity == MentionPolarity::Positive)
        .max_by(|left, right| {
            (left.span.end - left.span.start)
                .cmp(&(right.span.end - right.span.start))
                .then_with(|| right.span.start.cmp(&left.span.start))
        })
        .map(|area| area.canonical.clone());
    let bhk = plan.slots.bhk.as_ref().map(|slot| slot.value);
    let budget_max = plan.slots.budget_max.as_ref().map(|slot| slot.value);
    let hard_constraints = schema::detect_hard_constraints(&q);
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

    SearchIntent {
        area,
        excluded_areas,
        bhk,
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
    let budget_start = plan.slots.budget_max.as_ref().and_then(|budget| {
        exact_pattern_match_ranges(&query_lower, &budget.raw_text)
            .into_iter()
            .map(|(start, _)| start)
            .next()
    });
    let first_relation_start = plan
        .clauses
        .iter()
        .map(|clause| clause.relation_span.start)
        .min();

    for prefix in &search_resolution_config().named_entity_scope_prefixes {
        for (_, prefix_end) in scope_prefix_match_ranges(&query_lower, prefix) {
            if first_relation_start.is_some_and(|relation_start| prefix_end > relation_start) {
                continue;
            }
            let clause_end = [budget_start, first_relation_start]
                .into_iter()
                .flatten()
                .filter(|end| *end > prefix_end)
                .min()
                .unwrap_or(query.len());
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

fn relation_clauses(
    query: &str,
    tokens: &[QueryToken],
    slots: &parser::ParsedQuerySlots,
) -> Vec<QueryRelationClause> {
    let token_values = tokens
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    slots
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
            let distance_limit_km =
                distance_limit_for_relation(tokens, relation.end_token, relation.target_end_token)
                    .or_else(|| {
                        trailing_distance_limit_after_target(tokens, relation.target_end_token)
                    })
                    .or_else(|| trailing_distance_limit(query, slots, full_target_span));
            let requires_distance = search_parser_config()
                .relations
                .aliases
                .iter()
                .any(|alias| {
                    alias.alias.eq_ignore_ascii_case(&relation.alias)
                        && alias.requires_distance_limit
                });
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
        .collect()
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
    for override_rule in schema::preference_key_overrides() {
        if !override_rule
            .preference
            .eq_ignore_ascii_case(&signal.raw_text)
            || !query_contains_any_pattern(q, &override_rule.patterns, plan)
        {
            continue;
        }

        signal.expanded_keys = override_rule.expanded_keys.clone();
        signal.gap_keys = override_rule.gap_keys.clone();
        return;
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
        existing.missing_evidence_neutral |= signal.missing_evidence_neutral;
    } else {
        prefs.push(signal);
    }
}

fn negated_positive_preference_signal(pattern: &schema::PreferencePatternSpec) -> PreferenceSignal {
    if let Some(negative_pattern) = matching_negative_pattern_for_positive(pattern) {
        schema::schema_preference_signal(negative_pattern, Polarity::Negative)
    } else {
        schema::schema_preference_signal(pattern, Polarity::Negative)
    }
}

fn matching_negative_pattern_for_positive(
    pattern: &schema::PreferencePatternSpec,
) -> Option<&'static schema::PreferencePatternSpec> {
    let positive_signal = schema::schema_preference_signal(pattern, Polarity::Positive);
    schema::negative_preference_patterns()
        .iter()
        .find(|negative| negative.label.eq_ignore_ascii_case(&pattern.label))
        .or_else(|| {
            schema::negative_preference_patterns()
                .iter()
                .find(|negative| {
                    let negative_signal =
                        schema::schema_preference_signal(negative, Polarity::Negative);
                    preferences_conflict(&positive_signal, &negative_signal)
                })
        })
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

    positive.expanded_keys.iter().any(|positive_key| {
        is_specific_conflict_key(positive_key)
            && negative
                .expanded_keys
                .iter()
                .any(|negative_key| positive_key.eq_ignore_ascii_case(negative_key))
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
    let prefix = q[..start].trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',');
    search_resolution_config()
        .exclusion_prefixes
        .iter()
        .any(|phrase| prefix_ends_with_phrase(prefix, phrase))
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
    let mut tokens = Vec::new();
    let mut token_start: Option<usize> = None;
    for (index, ch) in query.char_indices() {
        if ch.is_ascii_whitespace() || ch == ',' {
            if let Some(start) = token_start.take() {
                push_query_token(query, start, index, &mut tokens);
            }
        } else if token_start.is_none() {
            token_start = Some(index);
        }
    }
    if let Some(start) = token_start {
        push_query_token(query, start, query.len(), &mut tokens);
    }
    tokens
}

fn push_query_token(query: &str, raw_start: usize, raw_end: usize, tokens: &mut Vec<QueryToken>) {
    let raw = &query[raw_start..raw_end];
    let leading_trim = raw
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-' || *ch == '+')
        .map(|(index, _)| index)
        .unwrap_or(raw.len());
    let trailing_trim = raw
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-' || *ch == '+')
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(leading_trim);
    if leading_trim >= trailing_trim {
        return;
    }
    let start = raw_start + leading_trim;
    let mut end = raw_start + trailing_trim;
    while start < end && query[end - 1..end].chars().all(|ch| ch == '+') {
        end -= 1;
    }
    while start < end
        && !query[end - 1..end]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric())
    {
        end -= 1;
    }
    if start >= end {
        return;
    }
    let text = query[start..end].to_ascii_lowercase();
    if !text.is_empty() {
        tokens.push(QueryToken { text, start, end });
    }
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
}
