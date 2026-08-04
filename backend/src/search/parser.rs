use winnow::ascii::digit1;
use winnow::combinator::opt;
use winnow::prelude::ModalResult;
use winnow::Parser;

use crate::dag_config::{
    search_parser_config, BhkParserConfig, RelationParserConfig, UnitAliasConfig,
    UnitValueParserConfig,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ParsedQuerySlots {
    pub bhk: Option<BhkConstraint>,
    pub budget_max: Option<MoneyConstraint>,
    pub distance_limit: Option<DistanceConstraint>,
    pub relations: Vec<RelationIntent>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BhkConstraint {
    pub value: u32,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MoneyConstraint {
    pub value: u64,
    pub unit: String,
    pub raw_text: String,
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
    let tokens = query_tokens(query);
    let config = search_parser_config();
    let distance_limit = parse_unit_value(&tokens, &config.distance, UnitValueKind::Distance);
    ParsedQuerySlots {
        bhk: parse_bhk(&tokens, &config.bhk),
        budget_max: parse_unit_value(&tokens, &config.budget, UnitValueKind::Money),
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

fn parse_bhk(tokens: &[String], config: &BhkParserConfig) -> Option<BhkConstraint> {
    for (index, token) in tokens.iter().enumerate() {
        if let Some((value, unit)) = parse_compound_bhk_unit(token, config) {
            if bhk_in_range(value, config) {
                return Some(BhkConstraint {
                    value,
                    raw_text: format!("{value} {unit}"),
                });
            }
        }

        let Some(value) = parse_u32_or_word(token, config) else {
            continue;
        };
        if !bhk_in_range(value, config) {
            continue;
        }
        let Some(next) = tokens.get(index + 1) else {
            continue;
        };
        if let Some(unit) = matching_alias(next, &config.unit_aliases) {
            return Some(BhkConstraint {
                value,
                raw_text: format!("{value} {unit}"),
            });
        }
    }
    None
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
        matches.sort_by(|left, right| right.1.cmp(&left.1));
        let Some((relation_alias, token_count)) = matches.into_iter().next() else {
            index += 1;
            continue;
        };
        relations.push(RelationIntent {
            alias: relation_alias.alias.clone(),
            raw_text: relation_alias.alias.clone(),
            start_token: index,
            end_token: index + token_count,
            target_start_token: index + token_count,
            target_end_token: tokens.len(),
        });
        index += token_count;
    }
    for relation_index in 0..relations.len() {
        relations[relation_index].target_end_token = relations
            .get(relation_index + 1)
            .map_or(tokens.len(), |next| next.start_token);
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

pub(crate) fn relation_target_text(tokens: &[String], relation: &RelationIntent) -> Option<String> {
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
    query
        .replace(',', "")
        .split_whitespace()
        .filter_map(|token| {
            let cleaned = token
                .trim_matches(|ch: char| {
                    !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '+')
                })
                .trim_end_matches('+');
            let cleaned = cleaned
                .trim_end_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_ascii_lowercase();
            (!cleaned.is_empty()).then_some(cleaned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_budget_units_from_configured_vocab() {
        assert_eq!(
            parse_query_slots("under 1.5cr in bellandur")
                .budget_max
                .unwrap()
                .value,
            15_000_000
        );
        assert_eq!(
            parse_query_slots("budget up to 80 lakhs")
                .budget_max
                .unwrap()
                .value,
            8_000_000
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
