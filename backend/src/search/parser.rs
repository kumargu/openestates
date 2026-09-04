use winnow::ascii::digit1;
use winnow::combinator::opt;
use winnow::prelude::ModalResult;
use winnow::Parser;

use crate::dag_config::{
    search_parser_config, search_resolution_config, BhkParserConfig, RelationParserConfig,
    UnitAliasConfig, UnitValueParserConfig,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ParsedQuerySlots {
    pub bhk: Option<BhkConstraint>,
    pub bhks: Vec<BhkConstraint>,
    pub budgets: Vec<ParsedBudgetConstraint>,
    pub budget_min: Option<MoneyConstraint>,
    pub budget_max: Option<MoneyConstraint>,
    pub distance_limit: Option<DistanceConstraint>,
    pub relations: Vec<RelationIntent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotPolarity {
    Include,
    Exclude,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BhkConstraint {
    pub value: u32,
    pub raw_text: String,
    pub polarity: SlotPolarity,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MoneyConstraint {
    pub value: u64,
    pub unit: String,
    pub raw_text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedBudgetConstraint {
    pub min: Option<MoneyConstraint>,
    pub max: Option<MoneyConstraint>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DistanceConstraint {
    pub value_km: f64,
    pub unit: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RelationIntent {
    pub alias: String,
    pub raw_text: String,
    pub start_token: usize,
    pub end_token: usize,
    pub target_start_token: usize,
    pub target_end_token: usize,
}

pub(crate) fn parse_query_slots(query: &str) -> ParsedQuerySlots {
    let spanned = query_tokens_spanned(query);
    let tokens = spanned
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    let config = search_parser_config();
    let distance_limit = parse_unit_value(&tokens, &config.distance, UnitValueKind::Distance);
    let (budget_min, budget_max, budgets) =
        parse_budget_constraints(&tokens, &spanned, &config.budget);
    let bhks = parse_bhks(&spanned, &config.bhk);
    ParsedQuerySlots {
        bhk: if bhks.len() == 1 {
            bhks.first().cloned()
        } else {
            None
        },
        bhks,
        budgets,
        budget_min,
        budget_max,
        relations: parse_relations(&tokens, &config.relations, distance_limit.as_ref()),
        distance_limit,
    }
}

pub(crate) fn distance_unit_multiplier(alias: &str) -> Option<f64> {
    search_parser_config()
        .distance
        .units
        .iter()
        .find(|unit| {
            unit.aliases
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(alias.trim()))
        })
        .map(|unit| unit.multiplier)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitValueKind {
    Money,
    Distance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BudgetBound {
    Min,
    Max,
}

#[derive(Debug, Clone)]
struct ParsedMoneyAmount {
    start: usize,
    end: usize,
    value: u64,
    unit: String,
    raw_text: String,
}

fn parse_budget_constraints(
    tokens: &[String],
    spanned_tokens: &[SpannedToken],
    config: &UnitValueParserConfig,
) -> (
    Option<MoneyConstraint>,
    Option<MoneyConstraint>,
    Vec<ParsedBudgetConstraint>,
) {
    let amounts = collect_money_amounts(tokens, config);
    if amounts.is_empty() {
        return (None, None, Vec::new());
    }

    let mut budgets = Vec::new();
    let mut index = 0;
    while index < amounts.len() {
        if let Some(right) = amounts
            .get(index + 1)
            .filter(|right| amounts_form_budget_range(tokens, &amounts[index], right, config))
        {
            let left = amounts[index].clone();
            let right = right.clone();
            let start = left.start.min(right.start);
            let end = left.end.max(right.end);
            let (min_amount, max_amount) = if left.value <= right.value {
                (left, right)
            } else {
                (right, left)
            };
            budgets.push(ParsedBudgetConstraint {
                min: money_constraint_from_amount(min_amount, spanned_tokens),
                max: money_constraint_from_amount(max_amount, spanned_tokens),
                start: spanned_tokens.get(start).map_or(0, |token| token.start),
                end: spanned_tokens
                    .get(end.saturating_sub(1))
                    .map_or(0, |token| token.end),
            });
            index += 2;
            continue;
        }

        let amount = amounts[index].clone();
        let Some(constraint) = money_constraint_from_amount(amount.clone(), spanned_tokens) else {
            index += 1;
            continue;
        };
        let (min, max) = match nearest_budget_bound(tokens, amount.start, config) {
            Some(BudgetBound::Min) => (Some(constraint.clone()), None),
            Some(BudgetBound::Max) | None => (None, Some(constraint.clone())),
        };
        budgets.push(ParsedBudgetConstraint {
            min,
            max,
            start: constraint.start,
            end: constraint.end,
        });
        index += 1;
    }

    let budget_min = budgets.first().and_then(|budget| budget.min.clone());
    let budget_max = budgets.first().and_then(|budget| budget.max.clone());
    (budget_min, budget_max, budgets)
}

fn amounts_form_budget_range(
    tokens: &[String],
    left: &ParsedMoneyAmount,
    right: &ParsedMoneyAmount,
    config: &UnitValueParserConfig,
) -> bool {
    let between_tokens = &tokens[left.end.min(tokens.len())..right.start.min(tokens.len())];
    let has_connector = between_tokens
        .iter()
        .any(|token| is_range_connector(token, config))
        || between_tokens.is_empty();
    let has_between_prefix = tokens
        .get(..left.start)
        .is_some_and(|prefix| phrase_matches_suffix(prefix, "between"));
    has_between_prefix || has_connector
}

fn collect_money_amounts(
    tokens: &[String],
    config: &UnitValueParserConfig,
) -> Vec<ParsedMoneyAmount> {
    let mut amounts = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if let Some((left, right, consumed)) = parse_inline_money_range_at(tokens, index, config) {
            amounts.push(left);
            amounts.push(right);
            index += consumed;
            continue;
        }
        if let Some((amount, consumed)) = parse_single_money_at(tokens, index, config) {
            amounts.push(amount);
            index += consumed;
            continue;
        }
        index += 1;
    }
    prepend_unitless_range_start(tokens, &mut amounts, config);
    amounts
}

fn parse_inline_money_range_at(
    tokens: &[String],
    index: usize,
    config: &UnitValueParserConfig,
) -> Option<(ParsedMoneyAmount, ParsedMoneyAmount, usize)> {
    let token = tokens.get(index)?;
    let (left_value, right_text) = split_numeric_dash_range(token)?;
    if let Some((right_value, unit)) = parse_compound_decimal_unit(right_text, &config.units) {
        let left = money_amount_from_parts(index, index + 1, left_value, unit.clone(), token)?;
        let right = money_amount_from_parts(index, index + 1, right_value, unit, token)?;
        return Some((left, right, 1));
    }
    let right_value = parse_decimal_token(right_text)?;
    let next = tokens.get(index + 1)?;
    let unit = matching_unit_alias(next, &config.units)?;
    let raw_text = format!("{token} {next}");
    let left = money_amount_from_parts(index, index + 2, left_value, unit.clone(), &raw_text)?;
    let right = money_amount_from_parts(index, index + 2, right_value, unit, &raw_text)?;
    Some((left, right, 2))
}

fn parse_single_money_at(
    tokens: &[String],
    index: usize,
    config: &UnitValueParserConfig,
) -> Option<(ParsedMoneyAmount, usize)> {
    let token = tokens.get(index)?;
    if let Some((value, unit)) = parse_compound_decimal_unit(token, &config.units) {
        let amount = money_amount_from_parts(index, index + 1, value, unit, token)?;
        return Some((amount, 1));
    }

    let value = parse_decimal_token(token)?;
    let next = tokens.get(index + 1)?;
    let unit = matching_unit_alias(next, &config.units)?;
    let raw_text = format!("{token} {next}");
    let amount = money_amount_from_parts(index, index + 2, value, unit, &raw_text)?;
    Some((amount, 2))
}

fn prepend_unitless_range_start(
    tokens: &[String],
    amounts: &mut Vec<ParsedMoneyAmount>,
    config: &UnitValueParserConfig,
) {
    let Some(first) = amounts.first() else {
        return;
    };
    if first.start == 0 {
        return;
    }

    let mut value_index = first.start - 1;
    if is_range_connector(&tokens[value_index], config) {
        if value_index == 0 {
            return;
        }
        value_index -= 1;
    }
    if parse_compound_decimal_unit(&tokens[value_index], &config.units).is_some() {
        return;
    }
    let Some(value) = parse_decimal_token(&tokens[value_index]) else {
        return;
    };
    let Some(unit) = config
        .units
        .iter()
        .find(|candidate| candidate.unit.eq_ignore_ascii_case(&first.unit))
        .cloned()
    else {
        return;
    };
    let Some(leading) = money_amount_from_parts(
        value_index,
        value_index + 1,
        value,
        unit,
        &tokens[value_index],
    ) else {
        return;
    };
    amounts.insert(0, leading);
}

fn money_amount_from_parts(
    start: usize,
    end: usize,
    value: f64,
    unit: UnitAliasConfig,
    raw_text: &str,
) -> Option<ParsedMoneyAmount> {
    MoneyConstraint::from_parsed_unit_value(value, unit.clone(), raw_text.to_string()).map(
        |constraint| ParsedMoneyAmount {
            start,
            end,
            value: constraint.value,
            unit: constraint.unit,
            raw_text: constraint.raw_text,
        },
    )
}

fn nearest_budget_bound(
    tokens: &[String],
    value_index: usize,
    config: &UnitValueParserConfig,
) -> Option<BudgetBound> {
    let start = 0;
    let window = &tokens[start..value_index];
    let min_match = config
        .min_operators
        .iter()
        .filter(|operator| phrase_matches_suffix(window, operator))
        .max_by_key(|operator| query_tokens(operator).len());
    let max_match = config
        .operators
        .iter()
        .filter(|operator| phrase_matches_suffix(window, operator))
        .max_by_key(|operator| query_tokens(operator).len());
    let bound = match (min_match, max_match) {
        (Some(min_operator), Some(max_operator)) => {
            if query_tokens(min_operator).len() >= query_tokens(max_operator).len() {
                Some(BudgetBound::Min)
            } else {
                Some(BudgetBound::Max)
            }
        }
        (Some(_), None) => Some(BudgetBound::Min),
        (None, Some(_)) => Some(BudgetBound::Max),
        (None, None) => None,
    };
    if bound == Some(BudgetBound::Min) {
        if let Some(min_operator) = min_match {
            let operator_len = query_tokens(min_operator).len();
            let prefix = window
                .len()
                .checked_sub(operator_len)
                .map(|end| &window[..end])
                .unwrap_or(window);
            let last_contrast = prefix
                .iter()
                .rposition(|token| token.eq_ignore_ascii_case("but"))
                .map_or(0, |index| index + 1);
            let immediate_exclusion = search_resolution_config()
                .exclusion_prefixes
                .iter()
                .any(|phrase| phrase_matches_suffix(prefix, phrase));
            let scoped_exclusion = search_parser_config()
                .discourse
                .scoped_exclusion_markers
                .iter()
                .any(|phrase| {
                    let phrase_tokens = query_tokens(phrase);
                    !phrase_tokens.is_empty()
                        && prefix[last_contrast..]
                            .windows(phrase_tokens.len())
                            .any(|window| {
                                window
                                    .iter()
                                    .zip(&phrase_tokens)
                                    .all(|(left, right)| left.eq_ignore_ascii_case(right))
                            })
                });
            if immediate_exclusion || scoped_exclusion {
                return Some(BudgetBound::Max);
            }
        }
    }
    bound
}

fn is_range_connector(token: &str, config: &UnitValueParserConfig) -> bool {
    config
        .range_connectors
        .iter()
        .any(|connector| token.eq_ignore_ascii_case(connector))
}

fn split_numeric_dash_range(token: &str) -> Option<(f64, &str)> {
    for (index, ch) in token.char_indices() {
        if ch != '-' && ch != '–' {
            continue;
        }
        let left = &token[..index];
        let right = &token[index + ch.len_utf8()..];
        let Some(left_value) = parse_decimal_token(left) else {
            continue;
        };
        if right
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit())
        {
            return Some((left_value, right));
        }
    }
    None
}

fn money_constraint_from_amount(
    amount: ParsedMoneyAmount,
    tokens: &[SpannedToken],
) -> Option<MoneyConstraint> {
    let start = tokens.get(amount.start)?.start;
    let end = tokens.get(amount.end.checked_sub(1)?)?.end;
    Some(MoneyConstraint {
        value: amount.value,
        unit: amount.unit,
        raw_text: amount.raw_text,
        start,
        end,
    })
}

fn parse_bhks(tokens: &[SpannedToken], config: &BhkParserConfig) -> Vec<BhkConstraint> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match parse_bhk_cluster(tokens, index, config) {
            Some((mut constraints, consumed)) if consumed > 0 => {
                let (polarity, clause_start) = cluster_polarity(tokens, index, config);
                let raw_text = tokens[clause_start..index + consumed]
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                for constraint in &mut constraints {
                    constraint.polarity = polarity;
                    constraint.start = tokens[clause_start].start;
                    constraint.raw_text = raw_text.clone();
                    found.push(constraint.clone());
                }
                index += consumed;
            }
            _ => index += 1,
        }
    }
    found
}

fn parse_bhk_cluster(
    tokens: &[SpannedToken],
    start: usize,
    config: &BhkParserConfig,
) -> Option<(Vec<BhkConstraint>, usize)> {
    let mut values = Vec::new();
    let mut cursor = start;
    let mut saw_unit = false;

    loop {
        if cursor >= tokens.len() {
            break;
        }

        if !values.is_empty() && is_bhk_joiner(&tokens[cursor].text, config) {
            cursor += 1;
            continue;
        }

        if let Some((value, _unit)) = parse_compound_bhk_unit(&tokens[cursor].text, config) {
            if bhk_in_range(value, config) {
                push_unique_bhk(&mut values, value);
                saw_unit = true;
                cursor += 1;
                continue;
            }
        }

        if let Some((alts, unit_on_token)) =
            parse_bhk_alternative_token(&tokens[cursor].text, config)
        {
            let unit_next = tokens
                .get(cursor + 1)
                .and_then(|token| matching_alias(&token.text, &config.unit_aliases));
            if unit_on_token.is_some() || unit_next.is_some() || saw_unit {
                for value in alts {
                    push_unique_bhk(&mut values, value);
                }
                if unit_on_token.is_some() {
                    saw_unit = true;
                    cursor += 1;
                } else if unit_next.is_some() {
                    saw_unit = true;
                    cursor += 2;
                } else {
                    cursor += 1;
                }
                continue;
            }
        }

        if let Some(value) = parse_u32_or_word(&tokens[cursor].text, config)
            .filter(|value| bhk_in_range(*value, config))
        {
            let unit_next = tokens
                .get(cursor + 1)
                .and_then(|token| matching_alias(&token.text, &config.unit_aliases));
            let next_is_joiner = tokens
                .get(cursor + 1)
                .is_some_and(|token| is_bhk_joiner(&token.text, config));
            let next_is_bhk_number = tokens
                .get(cursor + 1)
                .and_then(|token| parse_u32_or_word(&token.text, config))
                .is_some_and(|next| bhk_in_range(next, config));
            let next_next_is_unit = tokens
                .get(cursor + 2)
                .and_then(|token| matching_alias(&token.text, &config.unit_aliases))
                .is_some();
            let next_next_is_joiner = tokens
                .get(cursor + 2)
                .is_some_and(|token| is_bhk_joiner(&token.text, config));

            if unit_next.is_some() {
                push_unique_bhk(&mut values, value);
                saw_unit = true;
                cursor += 2;
                continue;
            }
            if next_is_joiner
                || (next_is_bhk_number && (next_next_is_unit || next_next_is_joiner || saw_unit))
            {
                push_unique_bhk(&mut values, value);
                cursor += 1;
                continue;
            }
        }

        break;
    }

    if values.is_empty() || !saw_unit {
        return None;
    }

    let raw_text = values
        .iter()
        .map(|value| format!("{value} bhk"))
        .collect::<Vec<_>>()
        .join(" or ");
    let span_start = tokens[start].start;
    let span_end = tokens[cursor.saturating_sub(1).max(start)].end;
    let constraints = values
        .into_iter()
        .map(|value| BhkConstraint {
            value,
            raw_text: raw_text.clone(),
            polarity: SlotPolarity::Include,
            start: span_start,
            end: span_end,
        })
        .collect();
    Some((constraints, cursor - start))
}

fn cluster_polarity(
    tokens: &[SpannedToken],
    start: usize,
    config: &BhkParserConfig,
) -> (SlotPolarity, usize) {
    let prefix = tokens[..start]
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    let mut phrase_end = prefix.len();
    while phrase_end > 0
        && config
            .exclusion_gap_tokens
            .iter()
            .any(|gap| gap.eq_ignore_ascii_case(&prefix[phrase_end - 1]))
    {
        phrase_end -= 1;
    }
    let prefixes = &search_resolution_config().exclusion_prefixes;
    let matched_prefix_len = prefixes
        .iter()
        .filter_map(|phrase| {
            let phrase_tokens = query_tokens(phrase);
            phrase_matches_suffix(&prefix[..phrase_end], phrase).then_some(phrase_tokens.len())
        })
        .max();
    if let Some(prefix_len) = matched_prefix_len {
        (SlotPolarity::Exclude, phrase_end.saturating_sub(prefix_len))
    } else {
        (SlotPolarity::Include, start)
    }
}

fn is_bhk_joiner(token: &str, config: &BhkParserConfig) -> bool {
    config
        .alternative_joiners
        .iter()
        .any(|joiner| token.eq_ignore_ascii_case(joiner))
}

fn push_unique_bhk(values: &mut Vec<u32>, value: u32) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn parse_bhk_alternative_token(
    token: &str,
    config: &BhkParserConfig,
) -> Option<(Vec<u32>, Option<String>)> {
    let normalized = token.trim().to_ascii_lowercase();
    for alias in aliases_by_length_desc(&config.unit_aliases) {
        let Some(number_text) = normalized.strip_suffix(alias.as_str()) else {
            continue;
        };
        let number_text = number_text.trim_end_matches('-');
        if let Some(values) = split_bhk_alternative_numbers(number_text, config) {
            return Some((values, Some(alias)));
        }
    }
    split_bhk_alternative_numbers(&normalized, config).map(|values| (values, None))
}

fn split_bhk_alternative_numbers(token: &str, config: &BhkParserConfig) -> Option<Vec<u32>> {
    let parts = token
        .split(['/', '-', '–'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let mut values = Vec::new();
    for part in parts {
        let value = parse_u32_or_word(part, config)?;
        if !bhk_in_range(value, config) {
            return None;
        }
        if !values.contains(&value) {
            values.push(value);
        }
    }
    Some(values)
}

fn bhk_in_range(value: u32, config: &BhkParserConfig) -> bool {
    value >= config.min && value <= config.max
}

fn parse_u32_or_word(token: &str, config: &BhkParserConfig) -> Option<u32> {
    parse_unsigned_integer(token).or_else(|| {
        config
            .number_words
            .iter()
            .find(|entry| entry.word.eq_ignore_ascii_case(token))
            .map(|entry| entry.value)
    })
}

fn parse_compound_bhk_unit(token: &str, config: &BhkParserConfig) -> Option<(u32, String)> {
    let normalized = token.trim().to_ascii_lowercase();
    for alias in aliases_by_length_desc(&config.unit_aliases) {
        let Some(number_text) = normalized.strip_suffix(alias.as_str()) else {
            continue;
        };
        let number_text = number_text.trim_end_matches('-');
        let value = parse_u32_or_word(number_text, config)?;
        return Some((value, alias));
    }
    None
}

fn parse_unit_value<T>(
    tokens: &[String],
    config: &UnitValueParserConfig,
    kind: UnitValueKind,
) -> Option<T>
where
    T: FromParsedUnitValue,
{
    for (index, token) in tokens.iter().enumerate() {
        if let Some((value, unit)) = parse_compound_decimal_unit(token, &config.units) {
            if kind == UnitValueKind::Money || has_operator_context(tokens, index, config) {
                return T::from_parsed_unit_value(value, unit, token.to_string());
            }
        }

        let Some(value) = parse_decimal_token(token) else {
            continue;
        };
        let Some(next) = tokens.get(index + 1) else {
            continue;
        };
        let Some(unit) = matching_unit_alias(next, &config.units) else {
            continue;
        };
        if kind == UnitValueKind::Money || has_operator_context(tokens, index, config) {
            return T::from_parsed_unit_value(value, unit, format!("{token} {next}"));
        }
    }
    None
}

fn parse_relations(
    tokens: &[String],
    config: &RelationParserConfig,
    distance_limit: Option<&DistanceConstraint>,
) -> Vec<RelationIntent> {
    let mut relations = Vec::new();
    let mut index = 0;
    while index < tokens.len() && relations.len() < config.max_clauses {
        let mut matches = config
            .aliases
            .iter()
            .filter_map(|relation_alias| {
                if relation_alias.requires_distance_limit && distance_limit.is_none() {
                    return None;
                }
                let alias_tokens = query_tokens(&relation_alias.alias);
                if alias_tokens.is_empty() || index + alias_tokens.len() > tokens.len() {
                    return None;
                }
                tokens[index..index + alias_tokens.len()]
                    .iter()
                    .zip(alias_tokens.iter())
                    .all(|(token, alias)| token.eq_ignore_ascii_case(alias))
                    .then_some((relation_alias, alias_tokens.len()))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|item| std::cmp::Reverse(item.1));
        let Some((relation_alias, token_count)) = matches.into_iter().next() else {
            index += 1;
            continue;
        };
        let target_start_token = if relation_alias.requires_distance_limit {
            let Some(target_start_token) =
                relation_target_start_after_distance(tokens, index + token_count, config)
            else {
                index += token_count;
                continue;
            };
            target_start_token
        } else {
            index + token_count
        };
        relations.push(RelationIntent {
            alias: relation_alias.alias.clone(),
            raw_text: relation_alias.alias.clone(),
            start_token: index,
            end_token: index + token_count,
            target_start_token,
            target_end_token: tokens.len(),
        });
        index += token_count;
    }
    for relation_index in 0..relations.len() {
        let has_next_relation = relations.get(relation_index + 1).is_some();
        let next_relation_start = relations
            .get(relation_index + 1)
            .map_or(tokens.len(), |next| next.start_token);
        let repeated_relation_boundary = has_next_relation
            .then(|| {
                tokens[relations[relation_index].target_start_token..next_relation_start]
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, token)| {
                        config
                            .clause_joiners
                            .iter()
                            .any(|joiner| joiner.eq_ignore_ascii_case(token))
                    })
                    .map(|(offset, _)| relations[relation_index].target_start_token + offset)
            })
            .flatten();
        relations[relation_index].target_end_token = first_relation_target_boundary_index(
            tokens,
            relations[relation_index].target_start_token,
            next_relation_start,
        )
        .into_iter()
        .chain(repeated_relation_boundary)
        .min()
        .unwrap_or(next_relation_start);
    }
    if let Some(distance_limit) = distance_limit {
        let distance_tokens = query_tokens(&distance_limit.raw_text);
        relations.retain(|relation| {
            let requires_distance = config.aliases.iter().any(|alias| {
                alias.requires_distance_limit && alias.alias.eq_ignore_ascii_case(&relation.alias)
            });
            !requires_distance
                || trimmed_relation_target_tokens(tokens, relation, &config.clause_joiners)
                    != distance_tokens.as_slice()
        });
    }
    relations
}

fn relation_target_start_after_distance(
    tokens: &[String],
    start: usize,
    config: &RelationParserConfig,
) -> Option<usize> {
    let mut index = start;
    let mut found_distance = false;
    while index < tokens.len() {
        let current = &tokens[index];
        let compact_distance =
            parse_compound_decimal_unit(current, &search_parser_config().distance.units).is_some();
        let split_distance = parse_decimal_token(current).is_some()
            && tokens
                .get(index + 1)
                .and_then(|unit| matching_unit_alias(unit, &search_parser_config().distance.units))
                .is_some();
        if compact_distance {
            index += 1;
            found_distance = true;
            break;
        }
        if split_distance {
            index += 2;
            found_distance = true;
            break;
        }
        if config
            .clause_joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(current))
        {
            return None;
        }
        index += 1;
    }
    if !found_distance {
        return None;
    }
    while index < tokens.len()
        && matches!(
            tokens[index].as_str(),
            "of" | "from" | "to" | "near" | "nearby"
        )
    {
        index += 1;
    }
    if index >= tokens.len()
        || config
            .clause_joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[index]))
    {
        None
    } else {
        Some(index)
    }
}

fn first_unit_operator_index(
    tokens: &[String],
    start: usize,
    end: usize,
    config: &UnitValueParserConfig,
) -> Option<usize> {
    let mut index = start.min(tokens.len());
    let end = end.min(tokens.len());
    while index < end {
        for operator in unit_operator_phrases(config) {
            let operator_tokens = query_tokens(operator);
            if operator_tokens.is_empty() || index + operator_tokens.len() > end {
                continue;
            }
            if tokens[index..index + operator_tokens.len()]
                .iter()
                .zip(operator_tokens.iter())
                .all(|(token, operator)| token.eq_ignore_ascii_case(operator))
            {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn unit_operator_phrases(config: &UnitValueParserConfig) -> Vec<&str> {
    let mut phrases = config
        .operators
        .iter()
        .chain(config.min_operators.iter())
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !config.range_connectors.is_empty() {
        phrases.push("between");
    }
    phrases
}

fn first_relation_target_boundary_index(
    tokens: &[String],
    start: usize,
    end: usize,
) -> Option<usize> {
    let budget_boundary =
        first_unit_operator_index(tokens, start, end, &search_parser_config().budget);
    let exclusion_boundary = first_phrase_index(
        tokens,
        start,
        end,
        search_resolution_config()
            .exclusion_prefixes
            .iter()
            .map(String::as_str),
    );
    let contrast_boundary = first_phrase_index(tokens, start, end, ["but"].into_iter());
    let shared_suffix_boundary = first_phrase_index(
        tokens,
        start,
        end,
        search_parser_config()
            .discourse
            .shared_suffix_markers
            .iter()
            .map(String::as_str),
    );
    let sentence_boundary = tokens[start.min(tokens.len())..end.min(tokens.len())]
        .iter()
        .position(|token| matches!(token.as_str(), "." | "?" | "!"))
        .map(|offset| start.min(tokens.len()) + offset);

    [
        budget_boundary,
        exclusion_boundary,
        contrast_boundary,
        shared_suffix_boundary,
        sentence_boundary,
    ]
    .into_iter()
    .flatten()
    .min()
}

fn first_phrase_index<'a>(
    tokens: &[String],
    start: usize,
    end: usize,
    phrases: impl Iterator<Item = &'a str>,
) -> Option<usize> {
    let start = start.min(tokens.len());
    let end = end.min(tokens.len());
    phrases
        .filter_map(|phrase| {
            let phrase_tokens = query_tokens(phrase);
            if phrase_tokens.is_empty() {
                return None;
            }
            (start..end).find(|index| {
                index + phrase_tokens.len() <= end
                    && tokens[*index..*index + phrase_tokens.len()]
                        .iter()
                        .zip(phrase_tokens.iter())
                        .all(|(token, phrase_token)| token.eq_ignore_ascii_case(phrase_token))
            })
        })
        .min()
}

fn trimmed_relation_target_tokens<'a>(
    tokens: &'a [String],
    relation: &RelationIntent,
    joiners: &[String],
) -> &'a [String] {
    let mut start = relation.target_start_token.min(tokens.len());
    let mut end = relation.target_end_token.min(tokens.len());
    while start < end
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[start]))
    {
        start += 1;
    }
    while end > start
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[end - 1]))
    {
        end -= 1;
    }
    &tokens[start..end]
}

#[cfg(test)]
fn relation_target_text(tokens: &[String], relation: &RelationIntent) -> Option<String> {
    if relation.target_start_token >= relation.target_end_token
        || relation.target_end_token > tokens.len()
    {
        return None;
    }
    let joiners = &search_parser_config().relations.clause_joiners;
    let mut start = relation.target_start_token;
    let mut end = relation.target_end_token;
    while start < end
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[start]))
    {
        start += 1;
    }
    while end > start
        && joiners
            .iter()
            .any(|joiner| joiner.eq_ignore_ascii_case(&tokens[end - 1]))
    {
        end -= 1;
    }
    (start < end).then(|| tokens[start..end].join(" "))
}

trait FromParsedUnitValue: Sized {
    fn from_parsed_unit_value(value: f64, unit: UnitAliasConfig, raw_text: String) -> Option<Self>;
}

impl FromParsedUnitValue for MoneyConstraint {
    fn from_parsed_unit_value(value: f64, unit: UnitAliasConfig, raw_text: String) -> Option<Self> {
        if !value.is_finite() || value <= 0.0 || !unit.multiplier.is_finite() {
            return None;
        }
        Some(Self {
            value: (value * unit.multiplier).round() as u64,
            unit: unit.unit,
            raw_text,
            start: 0,
            end: 0,
        })
    }
}

impl FromParsedUnitValue for DistanceConstraint {
    fn from_parsed_unit_value(value: f64, unit: UnitAliasConfig, raw_text: String) -> Option<Self> {
        if !value.is_finite() || value < 0.0 || !unit.multiplier.is_finite() {
            return None;
        }
        Some(Self {
            value_km: value * unit.multiplier,
            unit: unit.unit,
            raw_text,
        })
    }
}

fn parse_compound_decimal_unit(
    token: &str,
    units: &[UnitAliasConfig],
) -> Option<(f64, UnitAliasConfig)> {
    let normalized = token.trim().to_ascii_lowercase();
    for unit in units {
        for alias in aliases_by_length_desc(&unit.aliases) {
            let Some(number_text) = normalized.strip_suffix(alias.as_str()) else {
                continue;
            };
            let number_text = number_text.trim_end_matches('-');
            let value = parse_decimal_token(number_text)?;
            return Some((value, unit.clone()));
        }
    }
    None
}

fn matching_unit_alias(token: &str, units: &[UnitAliasConfig]) -> Option<UnitAliasConfig> {
    units.iter().find_map(|unit| {
        matching_alias(token, &unit.aliases).map(|_| UnitAliasConfig {
            unit: unit.unit.clone(),
            aliases: unit.aliases.clone(),
            multiplier: unit.multiplier,
        })
    })
}

fn matching_alias(token: &str, aliases: &[String]) -> Option<String> {
    let normalized = token.trim().to_ascii_lowercase();
    aliases
        .iter()
        .find(|alias| normalized.eq_ignore_ascii_case(alias))
        .cloned()
}

fn aliases_by_length_desc(aliases: &[String]) -> Vec<String> {
    let mut sorted = aliases
        .iter()
        .map(|alias| alias.to_ascii_lowercase())
        .collect::<Vec<_>>();
    sorted.sort_by(|left, right| right.len().cmp(&left.len()).then(left.cmp(right)));
    sorted
}

fn has_operator_context(
    tokens: &[String],
    value_index: usize,
    config: &UnitValueParserConfig,
) -> bool {
    let start = value_index.saturating_sub(4);
    let window = &tokens[start..value_index];
    config
        .operators
        .iter()
        .any(|operator| phrase_matches_suffix(window, operator))
}

fn phrase_matches_suffix(tokens: &[String], phrase: &str) -> bool {
    let phrase_tokens = query_tokens(phrase);
    if phrase_tokens.is_empty() || phrase_tokens.len() > tokens.len() {
        return false;
    }
    let start = tokens.len() - phrase_tokens.len();
    tokens[start..]
        .iter()
        .zip(phrase_tokens.iter())
        .all(|(token, phrase_token)| token.eq_ignore_ascii_case(phrase_token))
}

fn parse_unsigned_integer(token: &str) -> Option<u32> {
    let value = parse_decimal_token(token)?;
    if value.fract() != 0.0 {
        return None;
    }
    Some(value as u32)
}

fn parse_decimal_token(token: &str) -> Option<f64> {
    let mut input = token.trim();
    let parsed = decimal_token.parse_next(&mut input).ok()?;
    if !input.is_empty() {
        return None;
    }
    parsed.parse::<f64>().ok()
}

fn decimal_token<'input>(input: &mut &'input str) -> ModalResult<&'input str> {
    (digit1, opt((".", digit1))).take().parse_next(input)
}

pub(crate) fn query_tokens(query: &str) -> Vec<String> {
    query_tokens_spanned(query)
        .into_iter()
        .map(|token| token.text)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpannedToken {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Same tokens as [`query_tokens`], with byte spans in the original query.
pub(crate) fn query_tokens_spanned(query: &str) -> Vec<SpannedToken> {
    let mut tokens = Vec::new();
    let mut token_start: Option<usize> = None;
    for (index, ch) in query.char_indices() {
        let sentence_boundary = matches!(ch, '.' | '?' | '!')
            && !(ch == '.'
                && query[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|previous| previous.is_ascii_digit())
                && query[index + ch.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_digit()));
        if ch.is_ascii_whitespace()
            || matches!(ch, ',' | ';')
            || (ch == '-' && !ascii_hyphen_joins_numeric_range(query, index))
        {
            if let Some(start) = token_start.take() {
                push_spanned_token(query, start, index, &mut tokens);
            }
        } else if sentence_boundary {
            if let Some(start) = token_start.take() {
                push_spanned_token(query, start, index, &mut tokens);
            }
            tokens.push(SpannedToken {
                text: ch.to_string(),
                start: index,
                end: index + ch.len_utf8(),
            });
        } else if ch == '–' || ch == '—' {
            if let Some(start) = token_start.take() {
                push_spanned_token(query, start, index, &mut tokens);
            }
            tokens.push(SpannedToken {
                text: "-".to_string(),
                start: index,
                end: index + ch.len_utf8(),
            });
        } else if token_start.is_none() {
            token_start = Some(index);
        }
    }
    if let Some(start) = token_start {
        push_spanned_token(query, start, query.len(), &mut tokens);
    }
    tokens
}

fn ascii_hyphen_joins_numeric_range(query: &str, index: usize) -> bool {
    query[..index]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_ascii_digit())
        && query[index + 1..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
}

fn push_spanned_token(
    query: &str,
    raw_start: usize,
    raw_end: usize,
    tokens: &mut Vec<SpannedToken>,
) {
    let raw = &query[raw_start..raw_end];
    let leading_trim = raw
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-' || *ch == '+')
        .map(|(index, _)| index)
        .unwrap_or(raw.len());
    let mut trailing = raw
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-' || *ch == '+')
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(leading_trim);
    if leading_trim >= trailing {
        return;
    }
    while leading_trim < trailing && raw.as_bytes().get(trailing - 1) == Some(&b'+') {
        trailing -= 1;
    }
    while leading_trim < trailing {
        let last = raw[leading_trim..trailing].chars().next_back();
        match last {
            Some(ch) if ch.is_ascii_alphanumeric() => break,
            Some(ch) => trailing -= ch.len_utf8(),
            None => break,
        }
    }
    if leading_trim >= trailing {
        return;
    }
    let start = raw_start + leading_trim;
    let end = raw_start + trailing;
    let text = query[start..end].to_ascii_lowercase();
    if !text.is_empty() {
        tokens.push(SpannedToken { text, start, end });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanned_tokens_keep_source_offsets() {
        for query in [
            "2 or 3 BHK, not 4 BHK",
            "1.5–2Cr budget",
            "3BHK near Manipal Hospital Whitefield",
            "three bhk up to 80 lakhs",
        ] {
            for token in query_tokens_spanned(query) {
                let slice = query[token.start..token.end].to_ascii_lowercase();
                if token.text == "-" && (slice == "–" || slice == "—") {
                    continue;
                }
                assert_eq!(
                    slice, token.text,
                    "{query} span {}..{}",
                    token.start, token.end
                );
            }
        }
        assert_eq!(
            query_tokens("1.5–2Cr budget"),
            vec!["1.5", "-", "2cr", "budget"]
        );
        assert_eq!(
            query_tokens("low commute-pain, move-in-ready home"),
            vec!["low", "commute", "pain", "move", "in", "ready", "home"]
        );
        assert_eq!(query_tokens("2-3 BHK"), vec!["2-3", "bhk"]);
    }

    #[test]
    fn parses_bhk_spacing_and_hyphen_variants() {
        assert_eq!(
            parse_query_slots("1  bhk near manipal").bhk.unwrap().value,
            1
        );
        assert_eq!(parse_query_slots("3-bhk near metro").bhk.unwrap().value, 3);
        assert_eq!(
            parse_query_slots("three bhk near school")
                .bhk
                .unwrap()
                .value,
            3
        );
        assert_eq!(
            parse_query_slots("three-bedroom inventory")
                .bhk
                .unwrap()
                .value,
            3
        );
        assert_eq!(
            parse_query_slots("move-in-ready three bedroom home")
                .bhk
                .unwrap()
                .value,
            3
        );
    }

    #[test]
    fn parses_bhk_alternatives_as_a_set() {
        let or_query = parse_query_slots("2 or 3 BHK in Whitefield");
        assert!(or_query.bhk.is_none());
        assert_eq!(
            or_query
                .bhks
                .iter()
                .map(|slot| slot.value)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        let compound = parse_query_slots("2bhk or 3bhk");
        assert_eq!(
            compound
                .bhks
                .iter()
                .map(|slot| slot.value)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );

        let slash = parse_query_slots("2/3 BHK");
        assert_eq!(
            slash.bhks.iter().map(|slot| slot.value).collect::<Vec<_>>(),
            vec![2, 3]
        );

        let dash = parse_query_slots("2-3 BHK");
        assert_eq!(
            dash.bhks.iter().map(|slot| slot.value).collect::<Vec<_>>(),
            vec![2, 3]
        );

        let words = parse_query_slots("two or three bhk");
        assert_eq!(
            words.bhks.iter().map(|slot| slot.value).collect::<Vec<_>>(),
            vec![2, 3]
        );

        let single = parse_query_slots("3 BHK in Whitefield");
        assert_eq!(single.bhk.as_ref().map(|slot| slot.value), Some(3));
        assert_eq!(single.bhks.len(), 1);

        let excluded = parse_query_slots("2 or 3 BHK, not 4 BHK");
        assert_eq!(
            excluded
                .bhks
                .iter()
                .filter(|slot| slot.polarity == SlotPolarity::Include)
                .map(|slot| slot.value)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(
            excluded
                .bhks
                .iter()
                .filter(|slot| slot.polarity == SlotPolarity::Exclude)
                .map(|slot| slot.value)
                .collect::<Vec<_>>(),
            vec![4]
        );
    }

    #[test]
    fn preserves_repeated_bhk_occurrences_for_grouped_compilation() {
        let query = "Whitefield 3BHK under 2Cr or Bellandur 3BHK under 3Cr";
        let slots = parse_query_slots(query);
        let three_bhk_spans = slots
            .bhks
            .iter()
            .filter(|slot| slot.value == 3 && slot.polarity == SlotPolarity::Include)
            .map(|slot| &query[slot.start..slot.end])
            .collect::<Vec<_>>();

        assert_eq!(three_bhk_spans, vec!["3BHK", "3BHK"]);
        assert_ne!(slots.bhks[0].start, slots.bhks[1].start);
    }

    #[test]
    fn bhk_exclusions_allow_natural_articles_and_keep_clause_spans() {
        for query in [
            "2 BHK, avoid a 4 BHK",
            "2 BHK, don't want a 4 BHK",
            "2 BHK, not interested in a 4 BHK",
        ] {
            let slots = parse_query_slots(query);
            let excluded = slots
                .bhks
                .iter()
                .find(|slot| slot.value == 4)
                .expect("4 BHK clause should be parsed");
            assert_eq!(excluded.polarity, SlotPolarity::Exclude, "{query}");
            assert!(
                query[excluded.start..excluded.end].eq_ignore_ascii_case(&excluded.raw_text),
                "{query}"
            );
        }
    }

    #[test]
    fn parses_budget_units_from_configured_vocab() {
        let query = "under 1.5cr in bellandur";
        let budget = parse_query_slots(query).budget_max.unwrap();
        assert_eq!(budget.value, 15_000_000);
        assert_eq!(&query[budget.start..budget.end], "1.5cr");
        assert_eq!(
            parse_query_slots("budget up to 80 lakhs")
                .budget_max
                .unwrap()
                .value,
            8_000_000
        );
    }

    #[test]
    fn parses_budget_minimum_and_range_operators() {
        let above = parse_query_slots("Budget above 1.5Cr");
        assert_eq!(
            above.budget_min.as_ref().map(|slot| slot.value),
            Some(15_000_000)
        );
        assert!(above.budget_max.is_none());

        let dash_range = parse_query_slots("1.5–2Cr budget");
        assert_eq!(
            dash_range.budget_min.as_ref().map(|slot| slot.value),
            Some(15_000_000)
        );
        assert_eq!(
            dash_range.budget_max.as_ref().map(|slot| slot.value),
            Some(20_000_000)
        );

        let between = parse_query_slots("Between 1.8Cr and 2.2Cr");
        assert_eq!(
            between.budget_min.as_ref().map(|slot| slot.value),
            Some(18_000_000)
        );
        assert_eq!(
            between.budget_max.as_ref().map(|slot| slot.value),
            Some(22_000_000)
        );

        let not_over = parse_query_slots("budget not over 2Cr");
        assert!(not_over.budget_min.is_none());
        assert_eq!(
            not_over.budget_max.as_ref().map(|slot| slot.value),
            Some(20_000_000)
        );

        let spaced = parse_query_slots("1.5 to 2 Cr");
        assert_eq!(
            spaced.budget_min.as_ref().map(|slot| slot.value),
            Some(15_000_000)
        );
        assert_eq!(
            spaced.budget_max.as_ref().map(|slot| slot.value),
            Some(20_000_000)
        );
    }

    #[test]
    fn keeps_separate_budget_clauses_with_source_spans() {
        let query = "Godrej Air 3BHK under ₹2Cr or Prestige Waterford 4BHK under ₹4Cr";
        let slots = parse_query_slots(query);

        assert_eq!(slots.budgets.len(), 2);
        assert_eq!(
            slots
                .budgets
                .iter()
                .map(|budget| budget.max.as_ref().map(|bound| bound.value))
                .collect::<Vec<_>>(),
            vec![Some(20_000_000), Some(40_000_000)]
        );
        assert_eq!(
            slots
                .budgets
                .iter()
                .map(|budget| &query[budget.start..budget.end])
                .collect::<Vec<_>>(),
            vec!["2Cr", "4Cr"]
        );
    }

    #[test]
    fn parses_distance_only_with_limit_context() {
        assert_eq!(
            parse_query_slots("homes within 500m of Deens Academy")
                .distance_limit
                .unwrap()
                .value_km,
            0.5
        );
        assert_eq!(
            parse_query_slots("2 bhk near manipal hospital within 3 km")
                .distance_limit
                .unwrap()
                .value_km,
            3.0
        );
        assert!(parse_query_slots("3bhk near metro")
            .distance_limit
            .is_none());
    }

    #[test]
    fn parses_relation_aliases_from_configured_vocab() {
        assert_eq!(
            parse_query_slots("3bhk close to deens academy")
                .relations
                .first()
                .unwrap()
                .alias,
            "close to"
        );
        assert!(parse_query_slots("reviews for deens academy")
            .relations
            .is_empty());
        assert!(parse_query_slots("budget within 2cr for deens academy")
            .relations
            .is_empty());
        assert_eq!(
            parse_query_slots("school within 500m of deens academy")
                .relations
                .first()
                .unwrap()
                .alias,
            "within"
        );
    }

    #[test]
    fn parses_independent_proximity_clause_targets() {
        let slots = parse_query_slots(
            "3bhk near Whitefield close to kids school and near office in Marathahalli",
        );
        let tokens = query_tokens(
            "3bhk near Whitefield close to kids school and near office in Marathahalli",
        );
        let targets = slots
            .relations
            .iter()
            .filter_map(|relation| relation_target_text(&tokens, relation))
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec!["whitefield", "kids school", "office in marathahalli"]
        );
    }

    #[test]
    fn terminal_distance_modifier_stays_attached_to_the_previous_anchor() {
        let slots = parse_query_slots("3bhk near Deens Academy within 500m");
        let tokens = query_tokens("3bhk near Deens Academy within 500m");

        assert_eq!(slots.relations.len(), 1);
        assert_eq!(
            relation_target_text(&tokens, &slots.relations[0]).as_deref(),
            Some("deens academy")
        );
        assert_eq!(
            slots.distance_limit.map(|distance| distance.value_km),
            Some(0.5)
        );
    }

    #[test]
    fn trailing_distance_modifiers_do_not_become_empty_relations() {
        let query = "near Deens Academy within 1 km and near ITPB within 3 km";
        let slots = parse_query_slots(query);
        let tokens = query_tokens(query);
        let targets = slots
            .relations
            .iter()
            .filter_map(|relation| relation_target_text(&tokens, relation))
            .collect::<Vec<_>>();

        assert_eq!(targets, vec!["deens academy", "itpb"]);
    }

    #[test]
    fn relation_targets_stop_before_exclusion_boundaries() {
        let query = "3bhk near Marathahalli but away from a stormwater drain";
        let slots = parse_query_slots(query);
        let tokens = query_tokens(query);
        let targets = slots
            .relations
            .iter()
            .filter_map(|relation| relation_target_text(&tokens, relation))
            .collect::<Vec<_>>();

        assert_eq!(targets, vec!["marathahalli"]);
    }

    #[test]
    fn undistanced_within_phrase_is_not_a_hard_relation() {
        let query = "within a gated community and near school within 2 km";
        let slots = parse_query_slots(query);
        let tokens = query_tokens(query);
        let targets = slots
            .relations
            .iter()
            .filter_map(|relation| relation_target_text(&tokens, relation))
            .collect::<Vec<_>>();

        assert_eq!(targets, vec!["school"]);
    }

    #[test]
    fn parses_hyphenated_money_and_distance_units() {
        assert_eq!(
            parse_query_slots("3bhk under 1.5-cr near metro")
                .budget_max
                .unwrap()
                .value,
            15_000_000
        );
        assert_eq!(
            parse_query_slots("school within 500-m")
                .distance_limit
                .unwrap()
                .value_km,
            0.5
        );
    }

    #[test]
    fn parses_regression_query_slots() {
        let spaced = parse_query_slots("1  bhk near manipal  hospital within 3 km");
        assert_eq!(spaced.bhk.unwrap().value, 1);
        assert_eq!(spaced.distance_limit.unwrap().value_km, 3.0);

        let compact = parse_query_slots("1 bhk near manipal hospital within 3 km");
        assert_eq!(compact.bhk.unwrap().value, 1);
        assert_eq!(compact.distance_limit.unwrap().value_km, 3.0);

        let named_place = parse_query_slots("2 bhk near manipal within 3 km");
        assert_eq!(named_place.bhk.unwrap().value, 2);
        assert_eq!(named_place.distance_limit.unwrap().value_km, 3.0);

        let tech_park = parse_query_slots("3bhk near bagmane tech park whitefield");
        assert_eq!(tech_park.bhk.unwrap().value, 3);

        let generic_tech_park = parse_query_slots("3bhk near tech park");
        assert_eq!(generic_tech_park.bhk.unwrap().value, 3);
        assert!(generic_tech_park.distance_limit.is_none());
    }
}
