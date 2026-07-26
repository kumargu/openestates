use serde::{Deserialize, Serialize};

use super::{analyzer, schema};

/// Parsed intent from a natural-language search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntent {
    pub area: Option<String>,
    /// Areas explicitly rejected by the buyer, e.g. "not Electronic City".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_areas: Vec<String>,
    pub bhk: Option<u32>,
    pub budget_max: Option<u64>,
    /// Evidence-backed constraints that must be proven by structured/local facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_constraints: Vec<HardConstraint>,
    /// Backward-compatible display list for the frontend.
    pub preferences: Vec<String>,
    /// Preferences the buyer wants to optimize for.
    #[serde(default)]
    pub positive_preferences: Vec<PreferenceSignal>,
    /// Preferences the buyer explicitly wants to avoid.
    #[serde(default)]
    pub negative_preferences: Vec<PreferenceSignal>,
    /// Risks the buyer says they can accept as a tradeoff, e.g. "bad traffic but great amenities".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_tradeoffs: Vec<String>,
    /// Inventory classes requested but not currently supported by the apartment corpus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_inventory_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_archetype: Option<BuyerArchetype>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardConstraint {
    /// Registry dimension, e.g. "land_area".
    pub field: String,
    pub operator: ConstraintOperator,
    pub value: f64,
    pub unit: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOperator {
    Min,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceSignal {
    pub raw_text: String,
    pub polarity: Polarity,
    pub expanded_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gap_keys: Vec<String>,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuyerArchetype {
    Family,
    Investor,
    RiskAverse,
    ValueBuyer,
    LuxuryBuyer,
    EndUser,
}

/// Known area names and their aliases.
/// Includes landmark/station names that map to the canonical area.
///
use crate::dag_config::area_alias_entries;
///
/// Parse a natural-language search query into structured intent.
pub fn parse_intent(query: &str) -> SearchIntent {
    let q = query.to_lowercase();

    let excluded_areas = detect_excluded_areas(&q);
    let area = detect_area(&q, &excluded_areas);
    let bhk = detect_bhk(&q);
    let budget_max = detect_budget(&q);
    let hard_constraints = detect_hard_constraints(&q);
    let positive_preferences = detect_positive_preferences(&q, bhk);
    let accepted_tradeoffs = detect_accepted_tradeoffs(&q);
    let negative_preferences: Vec<PreferenceSignal> = detect_negative_preferences(&q, bhk)
        .into_iter()
        .filter(|pref| {
            !accepted_tradeoffs
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(&pref.raw_text))
        })
        .collect();
    let positive_preferences = remove_positive_preferences_conflicting_with_negatives(
        positive_preferences,
        &negative_preferences,
    );
    let unsupported_inventory_types = detect_unsupported_inventory_types(&q);
    let buyer_archetype = detect_buyer_archetype(&q);
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

fn detect_area(q: &str, excluded_areas: &[String]) -> Option<String> {
    // Check multi-word aliases first (longer matches take priority).
    let mut best: Option<(&str, usize)> = None;
    for entry in area_alias_entries() {
        if excluded_areas
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(&entry.canonical))
        {
            continue;
        }
        for alias in &entry.aliases {
            if query_contains_pattern(q, alias) {
                let len = alias.len();
                if best.is_none() || len > best.unwrap().1 {
                    best = Some((entry.canonical.as_str(), len));
                }
            }
        }
    }
    best.map(|(name, _)| name.to_string())
        .or_else(|| super::area_alias::resolve_area_with_tantivy(q, excluded_areas))
}

fn detect_excluded_areas(q: &str) -> Vec<String> {
    let mut excluded = Vec::new();
    for entry in area_alias_entries() {
        if entry
            .aliases
            .iter()
            .any(|alias| area_alias_is_excluded(q, alias))
        {
            push_unique(&mut excluded, &entry.canonical);
        }
    }
    excluded
}

fn area_alias_is_excluded(q: &str, alias: &str) -> bool {
    let patterns = [
        format!("not {}", alias),
        format!("not in {}", alias),
        format!("avoid {}", alias),
        format!("exclude {}", alias),
        format!("excluding {}", alias),
        format!("except {}", alias),
        format!("outside {}", alias),
    ];
    patterns
        .iter()
        .any(|pattern| query_contains_pattern(q, pattern))
}

fn detect_bhk(q: &str) -> Option<u32> {
    // Match patterns like "3bhk", "3 bhk", "3-bhk", "3 BHK"
    let bytes = q.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            let digit = (b - b'0') as u32;
            if (1..=6).contains(&digit) {
                // Look ahead for "bhk" possibly with a separator
                let rest = &q[i + 1..];
                if rest.starts_with("bhk") || rest.starts_with(" bhk") || rest.starts_with("-bhk") {
                    return Some(digit);
                }
            }
        }
    }
    None
}

fn detect_budget(q: &str) -> Option<u64> {
    // Patterns: "under 1.5cr", "below 80L", "under 1cr", "budget 90 lakhs"
    let q = q.replace(',', "");
    let tokens: Vec<&str> = q.split_whitespace().collect();

    for i in 0..tokens.len() {
        let is_budget_prefix = matches!(
            tokens[i],
            "under" | "undr" | "below" | "budget" | "max" | "within" | "upto" | "up"
        );
        if !is_budget_prefix {
            continue;
        }
        // Try to parse the next token(s) as amount
        if let Some(amount) = parse_amount(&tokens[i + 1..]) {
            return Some(amount);
        }
    }

    // Also try standalone patterns like "1.5cr" without prefix
    for token in &tokens {
        if let Some(amount) = parse_single_amount(token) {
            let token = clean_amount_token(token);
            // Only use standalone if it looks like a budget (has cr/l/lakh suffix)
            if token.ends_with("cr")
                || token.ends_with("crore")
                || token.ends_with("crores")
                || token.ends_with('l')
                || token.ends_with("lakh")
                || token.ends_with("lakhs")
            {
                return Some(amount);
            }
        }
    }

    None
}

fn parse_amount(tokens: &[&str]) -> Option<u64> {
    if tokens.is_empty() {
        return None;
    }

    // Try "1.5 cr", "80 lakhs", "1.5cr"
    let first = clean_amount_token(tokens[0]);

    // Case: "1.5cr" or "80L" (number + suffix in one token)
    if let Some(amount) = parse_single_amount(&first) {
        return Some(amount);
    }

    // Case: "1.5 cr" or "80 lakhs" (number then suffix)
    if tokens.len() >= 2 {
        if let Ok(num) = first.parse::<f64>() {
            let suffix = clean_amount_token(tokens[1]);
            if suffix.starts_with("cr") {
                return Some((num * 10_000_000.0) as u64);
            } else if suffix.starts_with("l") {
                return Some((num * 100_000.0) as u64);
            }
        }
    }

    None
}

fn parse_single_amount(token: &str) -> Option<u64> {
    // "1.5cr" -> 15_000_000, "80l" -> 8_000_000
    let token = clean_amount_token(token);
    if token.len() < 2 {
        return None;
    }

    let (num_part, suffix) = if let Some(stripped) = token.strip_suffix("crores") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("crore") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("cr") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("lakhs") {
        (stripped, "l")
    } else if let Some(stripped) = token.strip_suffix("lakh") {
        (stripped, "l")
    } else if let Some(stripped) = token.strip_suffix('l') {
        (stripped, "l")
    } else {
        return None;
    };

    let num: f64 = num_part.parse().ok()?;
    match suffix {
        "cr" => Some((num * 10_000_000.0) as u64),
        "l" => Some((num * 100_000.0) as u64),
        _ => None,
    }
}

fn clean_amount_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| ch.is_ascii_punctuation() && ch != '+' && ch != '-')
        .to_ascii_lowercase()
}

fn detect_hard_constraints(q: &str) -> Vec<HardConstraint> {
    schema::detect_hard_constraints(q)
}

fn detect_positive_preferences(q: &str, bhk: Option<u32>) -> Vec<PreferenceSignal> {
    let mut prefs: Vec<PreferenceSignal> = Vec::new();
    for pattern in schema::positive_preference_patterns() {
        if !pattern
            .patterns
            .iter()
            .any(|term| query_contains_unnegated_pattern(q, term))
        {
            continue;
        }

        let mut signal = schema::schema_preference_signal(pattern, Polarity::Positive);
        apply_preference_key_overrides(q, &mut signal);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }

    for override_rule in schema::preference_key_overrides() {
        if !query_contains_any_pattern(q, &override_rule.patterns)
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
        apply_preference_key_overrides(q, &mut signal);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }

    prefs
}

fn apply_bhk_fact_key_derivations(bhk: Option<u32>, signal: &mut PreferenceSignal) {
    let Some(bhk) = bhk.filter(|value| (1..=5).contains(value)) else {
        return;
    };

    let generic_keys = [
        "listing",
        "listing_price",
        "listing_price_range",
        "listing_price_per_sqft_range",
        "listing_area_sqft",
        "listing_source_url",
    ];
    let keys = signal.expanded_keys.clone();
    for key in keys {
        if generic_keys.iter().any(|generic| key == *generic) {
            merge_expanded_keys(signal, &[format!("{key}_{bhk}bhk")]);
        }
    }
}

fn apply_preference_key_overrides(q: &str, signal: &mut PreferenceSignal) {
    for override_rule in schema::preference_key_overrides() {
        if !override_rule
            .preference
            .eq_ignore_ascii_case(&signal.raw_text)
            || !query_contains_any_pattern(q, &override_rule.patterns)
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

fn detect_negative_preferences(q: &str, bhk: Option<u32>) -> Vec<PreferenceSignal> {
    let mut prefs: Vec<PreferenceSignal> = Vec::new();
    for pattern in schema::negative_preference_patterns() {
        if !pattern
            .patterns
            .iter()
            .any(|term| query_contains_pattern(q, term))
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
            .any(|term| query_contains_negated_pattern(q, term));
        if !negated {
            continue;
        }

        let mut signal = negated_positive_preference_signal(pattern);
        apply_bhk_fact_key_derivations(bhk, &mut signal);
        merge_or_push_preference(&mut prefs, signal);
    }
    prefs
}

fn merge_or_push_preference(prefs: &mut Vec<PreferenceSignal>, signal: PreferenceSignal) {
    if let Some(existing) = prefs
        .iter_mut()
        .find(|pref| pref.raw_text.eq_ignore_ascii_case(&signal.raw_text))
    {
        merge_expanded_keys(existing, &signal.expanded_keys);
        merge_gap_keys(existing, &signal.gap_keys);
        existing.weight = existing.weight.max(signal.weight);
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
    )
}

fn detect_accepted_tradeoffs(q: &str) -> Vec<String> {
    let mut accepted = Vec::new();
    for group in schema::accepted_tradeoffs() {
        if query_contains_any_pattern(q, &group.patterns) {
            push_unique(&mut accepted, &group.label);
        }
    }
    accepted
}

fn detect_unsupported_inventory_types(q: &str) -> Vec<String> {
    let mut inventory_types = Vec::new();
    for group in schema::unsupported_inventory_types() {
        if query_contains_any_pattern(q, &group.patterns) {
            push_unique(&mut inventory_types, &group.label);
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

fn detect_buyer_archetype(q: &str) -> Option<BuyerArchetype> {
    schema::buyer_archetype_patterns()
        .iter()
        .find(|pattern| query_contains_any_unnegated_pattern(q, &pattern.patterns))
        .map(|pattern| pattern.archetype.clone())
}

fn query_contains_any_pattern(q: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| query_contains_pattern(q, pattern))
}

fn query_contains_any_unnegated_pattern(q: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| query_contains_unnegated_pattern(q, pattern))
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn query_contains_pattern(q: &str, pattern: &str) -> bool {
    !query_pattern_match_ranges(q, pattern).is_empty()
}

fn query_contains_unnegated_pattern(q: &str, pattern: &str) -> bool {
    query_pattern_match_ranges(q, pattern)
        .into_iter()
        .any(|(start, _)| !match_has_negated_prefix(q, start))
}

fn query_contains_negated_pattern(q: &str, pattern: &str) -> bool {
    query_pattern_match_ranges(q, pattern)
        .into_iter()
        .any(|(start, _)| match_has_negated_prefix(q, start))
}

fn query_pattern_match_ranges(q: &str, pattern: &str) -> Vec<(usize, usize)> {
    let mut ranges = exact_pattern_match_ranges(q, pattern);
    if analyzer::stemmed_tokens(pattern).len() >= 2 {
        for range in analyzer::stemmed_phrase_match_ranges(q, pattern) {
            if !ranges.contains(&range) {
                ranges.push(range);
            }
        }
    }
    ranges.sort_unstable();
    ranges
}

fn exact_pattern_match_ranges(q: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern = pattern.trim();
    let pattern_len = pattern.len();
    let mut search_start = 0;
    let mut ranges = Vec::new();
    if pattern.is_empty() {
        return ranges;
    }

    while search_start < q.len() {
        let Some(relative_pos) = q[search_start..].find(pattern) else {
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

fn match_has_negated_prefix(q: &str, start: usize) -> bool {
    const NEGATED_PREFIXES: &[&str] = &[
        "not interested in",
        "not looking for",
        "do not want",
        "don't want",
        "dont want",
        "no need for",
        "without",
        "avoid",
        "not an",
        "not a",
        "not",
        "no",
    ];

    let prefix = q[..start].trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',');
    NEGATED_PREFIXES
        .iter()
        .any(|phrase| prefix_ends_with_phrase(prefix, phrase))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bhk() {
        let intent = parse_intent("3bhk in whitefield");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    }

    #[test]
    fn test_area_typo_resolves_through_tantivy_alias_index() {
        let intent = parse_intent("3bhk kadudgi under 2cr");

        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    }

    #[test]
    fn test_area_alias_does_not_match_inside_words() {
        let intent = parse_intent("avoid waterlogging and traffic but near tech parks 3bhk");

        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area, None);
        assert!(intent
            .preferences
            .contains(&"avoid waterlogging risk".to_string()));
        assert!(intent.preferences.contains(&"avoid traffic".to_string()));
        assert!(intent
            .preferences
            .contains(&"social infrastructure".to_string()));
        assert!(!intent.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn hospital_query_prioritizes_hospital_social_infra_keys() {
        let intent = parse_intent("peaceful home for parents near hospital");
        let signal = intent
            .positive_preferences
            .iter()
            .find(|signal| signal.raw_text == "social infrastructure")
            .expect("hospital should request social infrastructure");

        assert_eq!(
            &signal.expanded_keys[..2],
            [
                "hospital_access".to_string(),
                "nearby_hospitals".to_string()
            ]
        );
    }

    #[test]
    fn negated_area_is_excluded_not_selected() {
        let intent = parse_intent("near tech parks but quiet not electronic city 3bhk");

        assert_eq!(intent.area, None);
        assert_eq!(intent.excluded_areas, vec!["Electronic City".to_string()]);
        assert_eq!(intent.bhk, Some(3));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
        assert!(intent
            .preferences
            .contains(&"social infrastructure".to_string()));
    }

    #[test]
    fn accepted_traffic_tradeoff_is_not_avoid_traffic() {
        let intent =
            parse_intent("I can tolerate traffic if society amenities and clubhouse are excellent");

        assert_eq!(intent.accepted_tradeoffs, vec!["traffic".to_string()]);
        assert!(!intent.preferences.contains(&"avoid traffic".to_string()));
        assert!(intent.preferences.contains(&"amenity quality".to_string()));
    }

    #[test]
    fn unsupported_inventory_requests_are_explicit() {
        let intent = parse_intent("plot or villa style calm layout near Bagalur metro");

        assert_eq!(
            intent.unsupported_inventory_types,
            vec!["plot".to_string(), "villa".to_string()]
        );
        assert!(intent.preferences.contains(&"metro access".to_string()));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
    }

    #[test]
    fn test_parse_budget() {
        let intent = parse_intent("under 1.5cr in bellandur");
        assert_eq!(intent.budget_max, Some(15_000_000));
        assert_eq!(intent.area.as_deref(), Some("Bellandur"));
    }

    #[test]
    fn test_parse_preferences() {
        let intent = parse_intent("quiet 2bhk near metro");
        assert_eq!(intent.bhk, Some(2));
        assert!(intent.preferences.contains(&"metro access".to_string()));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
    }

    #[test]
    fn test_parse_budget_lakhs() {
        let intent = parse_intent("3 bhk below 80l");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(8_000_000));
    }

    #[test]
    fn parses_punctuated_and_typo_budget_phrases() {
        let intent = parse_intent("witefield 3bhk undr 2.5cr, gud reviews");

        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(25_000_000));
        assert!(has_positive_label(&intent, "review quality"));

        let sentence = parse_intent("Budget below 1.5Cr.");
        assert_eq!(sentence.budget_max, Some(15_000_000));
    }

    #[test]
    fn test_parse_min_land_area_constraint() {
        let intent = parse_intent("3bhk with greenery in whitefield above 10 acres");
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.hard_constraints.len(), 1);
        let constraint = &intent.hard_constraints[0];
        assert_eq!(constraint.field, "land_area");
        assert_eq!(constraint.operator, ConstraintOperator::Min);
        assert_eq!(constraint.value, 10.0);
        assert_eq!(constraint.unit, "acres");
    }

    #[test]
    fn test_parse_plus_acres_as_min_land_area_constraint() {
        let intent = parse_intent("3bhk whitefield 10+ acres");
        assert_eq!(intent.hard_constraints.len(), 1);
        assert_eq!(intent.hard_constraints[0].value, 10.0);
    }

    #[test]
    fn test_plain_acres_without_min_operator_is_not_hard_constraint() {
        let intent = parse_intent("3bhk whitefield 10 acres");
        assert!(intent.hard_constraints.is_empty());
    }

    #[test]
    fn test_avoid_waterlogging_and_traffic_extracts_both_risks() {
        let intent = parse_intent("3bhk whitefield avoid waterlogging and traffic");
        let risks: Vec<&str> = intent
            .negative_preferences
            .iter()
            .map(|preference| preference.raw_text.as_str())
            .collect();
        assert!(risks.contains(&"waterlogging risk"), "{risks:?}");
        assert!(risks.contains(&"traffic"), "{risks:?}");
    }

    // --- Day 62: Project status preference extraction tests ---

    #[test]
    fn test_ready_to_move_preference() {
        let intent = parse_intent("ready to move in whitefield");
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' preference, got: {:?}",
            intent.preferences
        );
        // Should NOT also extract "new construction"
        assert!(
            !intent.preferences.contains(&"new construction".to_string()),
            "Should not extract 'new construction' for 'ready to move' query"
        );
    }

    #[test]
    fn test_under_construction_preference() {
        let intent = parse_intent("under construction sarjapur");
        assert_eq!(intent.area.as_deref(), Some("Sarjapur Road"));
        assert!(
            intent
                .preferences
                .contains(&"under construction".to_string()),
            "Expected 'under construction' preference, got: {:?}",
            intent.preferences
        );
        // Must NOT extract "new construction" — that was the old buggy behavior
        assert!(
            !intent.preferences.contains(&"new construction".to_string()),
            "Should not extract 'new construction' for 'under construction' query"
        );
    }

    #[test]
    fn test_new_launch_preference() {
        let intent = parse_intent("new launch 3bhk whitefield");
        assert!(
            intent.preferences.contains(&"new launch".to_string()),
            "Expected 'new launch' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn home_state_queries_use_schema_backed_preferences() {
        let delivered = parse_intent("delivered society near metro whitefield");
        assert!(delivered
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "delivered society"
                && preference.expanded_keys.contains(&"home_state".to_string())));

        let new_property = parse_intent("new property in sarjapur");
        assert!(new_property
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "new property"
                && preference
                    .expanded_keys
                    .contains(&"home_age_bucket".to_string())));

        let old_society = parse_intent("old society in whitefield");
        assert!(old_society
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "established society"
                && preference
                    .expanded_keys
                    .contains(&"home_age_bucket".to_string())));
    }

    #[test]
    fn test_delayed_preference() {
        let intent = parse_intent("delayed projects in sarjapur");
        assert!(
            intent.preferences.contains(&"delayed".to_string()),
            "Expected 'delayed' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_upcoming_preference() {
        let intent = parse_intent("upcoming projects in whitefield");
        assert!(
            intent.preferences.contains(&"upcoming".to_string()),
            "Expected 'upcoming' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_immediate_possession_maps_to_ready_to_move() {
        let intent = parse_intent("immediate possession bellandur");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'immediate possession', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_completed_maps_to_ready_to_move() {
        let intent = parse_intent("completed projects hsr layout");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'completed', got: {:?}",
            intent.preferences
        );
    }

    // --- Day 63: Builder preference pattern tests ---

    #[test]
    fn test_reliable_builder_preference() {
        let intent = parse_intent("reliable builder whitefield");
        assert!(
            intent.preferences.contains(&"reliable builder".to_string()),
            "Expected 'reliable builder' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    }

    #[test]
    fn test_safe_builder_maps_to_reliable_builder() {
        let intent = parse_intent("safe builder no possession delay under 2 crore");
        assert!(
            intent.preferences.contains(&"reliable builder".to_string()),
            "Expected 'reliable builder' from 'safe builder', got: {:?}",
            intent.preferences
        );
        assert!(
            intent.preferences.contains(&"on time delivery".to_string())
                || intent.preferences.contains(&"avoid delay risk".to_string()),
            "Expected a delay signal, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_trusted_builder_preference() {
        let intent = parse_intent("trusted builder sarjapur");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_on_time_delivery_preference() {
        let intent = parse_intent("on time delivery 3bhk whitefield");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.bhk, Some(3));
    }

    #[test]
    fn test_good_builder_maps_to_trusted_builder() {
        let intent = parse_intent("good builder bellandur");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' from 'good builder', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_no_delays_maps_to_on_time_delivery() {
        let intent = parse_intent("no delays whitefield");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' from 'no delays', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_avoid_waterlogging_is_negative_preference() {
        let intent = parse_intent("3bhk whitefield avoid waterlogging");
        assert!(
            intent
                .positive_preferences
                .iter()
                .all(|preference| preference.raw_text != "waterlogging risk"),
            "waterlogging should not be parsed as a positive preference: {:?}",
            intent.positive_preferences
        );
        assert_eq!(intent.negative_preferences.len(), 1);
        assert_eq!(intent.negative_preferences[0].raw_text, "waterlogging risk");
        assert_eq!(intent.negative_preferences[0].polarity, Polarity::Negative);
        assert!(
            intent
                .preferences
                .contains(&"avoid waterlogging risk".to_string()),
            "Display preferences should include avoid-pref, got {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_less_traffic_and_not_delayed_are_negative_preferences() {
        let intent = parse_intent("family 3bhk sarjapur less traffic not delayed");
        let negative: Vec<&str> = intent
            .negative_preferences
            .iter()
            .map(|pref| pref.raw_text.as_str())
            .collect();
        assert!(
            negative.contains(&"traffic"),
            "Expected traffic negative preference: {:?}",
            negative
        );
        assert!(
            negative.contains(&"delay risk"),
            "Expected delay risk negative preference: {:?}",
            negative
        );
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
    }

    #[test]
    fn project_names_do_not_create_greenery_preferences() {
        let intent = parse_intent("Prestige Park Grove 3bhk");

        assert_eq!(intent.bhk, Some(3));
        assert!(!intent.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn review_and_resident_feedback_intents_come_from_the_schema_registry() {
        let google = parse_intent("good google reviews Prestige Park Grove");
        let google_preference = google
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "review quality")
            .expect("Google review intent should be detected");
        assert!(google_preference
            .expanded_keys
            .contains(&"google_rating".to_string()));
        assert!(!google.preferences.contains(&"greenery".to_string()));

        let long_form = parse_intent(
            "Show homes with real review receipts.\nI want Google review strength, resident snippets and community proof.",
        );
        assert!(has_positive_label(&long_form, "review quality"));

        let reddit = parse_intent("resident feedback on reddit Prestige Raintree Park");
        let resident_preference = reddit
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "reddit discussions")
            .expect("resident feedback intent should be detected");
        assert!(resident_preference
            .expanded_keys
            .contains(&"reddit_thread_count".to_string()));
        assert!(!reddit.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn legal_and_builder_language_maps_to_structured_preferences() {
        let legal = parse_intent(
            "Need safe paperwork, RERA clarity and clean legal receipts.\nAvoid projects where possession or legal status is unclear.",
        );
        assert!(has_positive_label(&legal, "legal safety"));
        assert!(has_positive_label(&legal, "ready to move"));

        let builder = parse_intent(
            "Prefer an experienced builder with a visible RERA track record and builder project count.",
        );
        assert!(has_expanded_positive_key(
            &builder,
            "rera_builder_projects_count"
        ));
        assert!(has_positive_label(&builder, "legal safety"));
    }

    #[test]
    fn listing_receipt_language_maps_to_listing_evidence() {
        let listing = parse_intent(
            "Need a larger 4BHK or premium family apartment with price proof.\nBudget can stretch, but I want listing source and area details.",
        );

        assert!(has_positive_label(&listing, "premium"));
        assert!(has_positive_label(&listing, "listing evidence"));
        assert!(has_expanded_positive_key(&listing, "listing_price_4bhk"));
        assert!(has_expanded_positive_key(
            &listing,
            "listing_area_sqft_4bhk"
        ));

        let receipts = parse_intent(
            "Prestige Waterford 3BHK. I want an explainable premium option with legal and listing receipts.",
        );
        assert!(has_positive_label(&receipts, "legal safety"));
        assert!(has_positive_label(&receipts, "listing evidence"));
        assert!(has_expanded_positive_key(&receipts, "rera_status"));
        assert!(has_expanded_positive_key(&receipts, "listing_price_3bhk"));
    }

    #[test]
    fn negated_luxury_language_is_not_positive_premium_or_luxury_buyer() {
        let intent = parse_intent("not luxury, just practical family home with receipts");

        assert!(has_negative_label(&intent, "premium"));
        assert!(!has_positive_label(&intent, "premium"));
        assert_ne!(intent.buyer_archetype, Some(BuyerArchetype::LuxuryBuyer));
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
    }

    #[test]
    fn affirmative_premium_language_still_maps_to_premium_luxury_buyer() {
        let intent = parse_intent("premium high end apartment with listing receipts");

        assert!(has_positive_label(&intent, "premium"));
        assert!(!has_negative_label(&intent, "premium"));
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::LuxuryBuyer));
    }

    #[test]
    fn data_gap_language_carries_configured_gap_keys() {
        let water = parse_intent("avoid water issues, no tanker dependency");
        let water_pref = water
            .negative_preferences
            .iter()
            .find(|preference| preference.raw_text == "water issues")
            .expect("water issues should be detected");
        assert!(water_pref
            .gap_keys
            .contains(&"operating.tanker_dependence".to_string()));
        assert!(water_pref
            .gap_keys
            .contains(&"water_supply_risk".to_string()));

        let approvals = parse_intent(
            "BBMP approval issues are a hard no. Need approval documents and OC-like confidence.",
        );
        let legal = approvals
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "legal safety")
            .expect("legal safety should be detected");
        assert_eq!(
            legal.gap_keys,
            vec![
                "bbmp_approval_status".to_string(),
                "occupancy_certificate_status".to_string()
            ]
        );
    }

    #[test]
    fn legal_risk_query_maps_to_proof_dimensions() {
        let intent = parse_intent(
            "Legal risk is a hard no: complaints, litigation and builder revocations should be checked from RERA.\nShow options with those receipts, not guesses.",
        );

        assert!(has_positive_label(&intent, "legal safety"));
        assert!(has_positive_label(&intent, "reliable builder"));
        assert!(has_expanded_positive_key(
            &intent,
            "rera_builder_revocations"
        ));
    }

    #[test]
    fn monsoon_drainage_language_maps_to_negative_risks() {
        let intent = parse_intent(
            "Concerned about monsoon flooding, bad drainage and stagnant rainwater near approach roads.",
        );

        assert!(has_negative_label(&intent, "waterlogging risk"));
        assert!(has_negative_label(&intent, "approach road"));
    }

    #[test]
    fn positive_approach_road_language_is_not_a_negative_risk() {
        let intent = parse_intent("good approach road and access");

        assert!(has_positive_label(&intent, "approach road"));
        assert!(!has_negative_label(&intent, "approach road"));
    }

    #[test]
    fn family_and_investment_query_extracts_both_preferences() {
        let intent = parse_intent("good for family AND good investment");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
        assert!(has_positive_label(&intent, "family friendly"));
        assert!(has_positive_label(&intent, "resale potential"));
    }

    #[test]
    fn water_issue_query_is_negative_not_positive_water_supply() {
        let intent = parse_intent("avoid water issues, no tanker dependency");

        assert!(has_negative_label(&intent, "water issues"));
        assert!(!has_positive_label(&intent, "water supply"));
    }

    #[test]
    fn stable_water_language_with_no_tanker_issue_is_positive() {
        let intent = parse_intent("good water supply with cauvery and no tanker issue");

        assert!(has_positive_label(&intent, "water supply"));
        assert!(!has_negative_label(&intent, "water issues"));
    }

    #[test]
    fn maintenance_and_shady_builder_query_extracts_negative_risks() {
        let intent = parse_intent("don't want maintenance headaches or shady builder");

        assert!(has_negative_label(&intent, "maintenance"));
        assert!(has_negative_label(&intent, "builder trust"));
        assert!(!has_positive_label(&intent, "maintenance"));
    }

    #[test]
    fn soft_parent_query_extracts_quiet_and_open_space() {
        let intent =
            parse_intent("something calmer for my parents, less chaos, more breathing room");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
        assert!(has_positive_label(&intent, "quiet neighborhood"));
        assert!(has_expanded_positive_key(&intent, "open_space_score"));
        assert!(!has_negative_label(&intent, "density risk"));
    }

    #[test]
    fn stemmed_phrase_matching_keeps_config_from_needing_plural_duplicates() {
        let open_space = parse_intent("need greener open spaces for parents");
        assert!(has_positive_label(&open_space, "greenery"));
        assert!(!has_negative_label(&open_space, "greenery"));

        let water = parse_intent("avoid water issues and maintenance issues");
        assert!(has_negative_label(&water, "water issues"));
        assert!(has_negative_label(&water, "maintenance"));
    }

    #[test]
    fn value_commute_query_extracts_value_buyer_and_commute() {
        let intent = parse_intent("affordable 2BHK for young couple, good commute");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::ValueBuyer));
        assert_eq!(intent.bhk, Some(2));
        assert!(has_positive_label(&intent, "commute"));
        assert!(has_positive_label(&intent, "value for money"));
    }

    fn has_positive_label(intent: &SearchIntent, label: &str) -> bool {
        intent
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == label)
    }

    fn has_negative_label(intent: &SearchIntent, label: &str) -> bool {
        intent
            .negative_preferences
            .iter()
            .any(|preference| preference.raw_text == label)
    }

    fn has_expanded_positive_key(intent: &SearchIntent, key: &str) -> bool {
        intent
            .positive_preferences
            .iter()
            .any(|preference| preference.expanded_keys.contains(&key.to_string()))
    }
}
