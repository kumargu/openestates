use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::knowledge::fact::SourceType;
use crate::knowledge::FactValue;

use super::analyzer;
use super::intent::{
    BuyerArchetype, ConstraintOperator, HardConstraint, Polarity, PreferenceSignal,
};

#[cfg(test)]
pub const SQM_PER_ACRE: f64 = 4046.8564224;

use crate::dag_config::{dag_root, load_json};

/// Search schema format version carried in serving bundles.
pub const SEARCH_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactRegistrySearchFile {
    pub version: u32,
    #[serde(default)]
    pub runtime: SearchRuntimePolicy,
    #[serde(default)]
    pub search_dimensions: Vec<ThemeLayer>,
    #[serde(default)]
    pub preference_patterns: FactRegistryPreferencePatterns,
    #[serde(default)]
    pub numeric_constraints: Vec<NumericConstraintSchema>,
    #[serde(default)]
    pub text_evidence: Vec<TextEvidenceSchema>,
    #[serde(default)]
    pub numeric_evidence: Vec<NumericEvidenceSchema>,
    #[serde(default)]
    pub facts: Vec<FactSearchSemantics>,
    #[serde(default)]
    pub excluded_search_fact_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FactRegistryPreferencePatterns {
    #[serde(default)]
    pub positive: Vec<PreferencePatternSpec>,
    #[serde(default)]
    pub negative: Vec<PreferencePatternSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SearchSchemaConfig {
    pub version: u32,
    #[serde(default)]
    pub runtime: SearchRuntimePolicy,
    #[serde(default)]
    pub theme_layers: Vec<ThemeLayer>,
    #[serde(default)]
    pub numeric_constraints: Vec<NumericConstraintSchema>,
    #[serde(default)]
    pub positive_preference_patterns: Vec<PreferencePatternSpec>,
    #[serde(default)]
    pub negative_preference_patterns: Vec<PreferencePatternSpec>,
    #[serde(default)]
    pub text_evidence: Vec<TextEvidenceSchema>,
    #[serde(default)]
    pub numeric_evidence: Vec<NumericEvidenceSchema>,
    #[serde(default)]
    fact_search_semantics: HashMap<String, FactSearchSemantics>,
    #[serde(default)]
    pub excluded_search_fact_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactSearchSemantics {
    pub fact_key: String,
    #[serde(default)]
    pub polarity: String,
    #[serde(default)]
    pub answers_preferences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchRuntimePolicy {
    #[serde(default)]
    pub query_stopwords: Vec<String>,
    #[serde(default)]
    pub scoring_stopwords: Vec<String>,
    #[serde(default)]
    pub placeholder_display_values: Vec<String>,
    #[serde(default)]
    pub negative_fact_key_terms: Vec<String>,
    #[serde(default)]
    pub negative_preference_allow_terms: Vec<String>,
    #[serde(default)]
    pub fact_key_self_describe_excluded_suffixes: Vec<String>,
    #[serde(default)]
    pub fact_key_self_describe_excluded_exact: Vec<String>,
    #[serde(default)]
    pub registry_fact_key_required_preferences: Vec<String>,
    #[serde(default)]
    pub fact_key_derivations: Vec<FactKeyDerivationRule>,
    #[serde(default)]
    pub accepted_tradeoffs: Vec<IntentPhraseGroup>,
    #[serde(default)]
    pub unsupported_inventory_types: Vec<IntentPhraseGroup>,
    #[serde(default)]
    pub buyer_archetypes: Vec<BuyerArchetypePattern>,
    #[serde(default)]
    pub preference_key_overrides: Vec<PreferenceKeyOverride>,
    #[serde(default)]
    pub lifecycle_value_terms: LifecycleValueTerms,
    #[serde(default)]
    pub lifecycle_compatibility_rules: Vec<LifecycleCompatibilityRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactKeyDerivationRule {
    pub input_dimension: String,
    #[serde(default)]
    pub source_keys: Vec<String>,
    pub template: String,
    #[serde(default)]
    pub min_value: Option<u32>,
    #[serde(default)]
    pub max_value: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntentPhraseGroup {
    pub label: String,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyerArchetypePattern {
    pub archetype: BuyerArchetype,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceKeyOverride {
    pub preference: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub expanded_keys: Vec<String>,
    #[serde(default)]
    pub gap_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleValueTerms {
    #[serde(default)]
    pub ready: Vec<String>,
    #[serde(default)]
    pub under_construction: Vec<String>,
    #[serde(default)]
    pub delay: Vec<String>,
    #[serde(default)]
    pub new_age: Vec<String>,
    #[serde(default)]
    pub established_age: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleCompatibilityRule {
    #[serde(default)]
    pub preferences: Vec<String>,
    #[serde(default)]
    pub require_any_groups: Vec<String>,
    #[serde(default)]
    pub require_fact_key_any_groups: Vec<FactKeyGroupAllowance>,
    #[serde(default)]
    pub reject_any_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactKeyGroupAllowance {
    pub fact_key: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ThemeLayer {
    pub rank: u32,
    pub dimension: String,
    pub label: String,
    pub layer: String,
    #[serde(default)]
    pub intent_terms: Vec<String>,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub source_priority: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryUnit {
    pub unit: String,
    pub aliases: Vec<String>,
    pub to_canonical: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericConstraintSchema {
    pub dimension: String,
    pub label: String,
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub runtime_field: Option<String>,
    pub query_units: Vec<QueryUnit>,
    #[serde(default)]
    pub qualitative_bounds: Vec<QualitativeNumericBound>,
    #[serde(default)]
    pub value_kind: NumericFactValueKind,
    #[serde(default)]
    pub aggregation: NumericAggregation,
    #[serde(default)]
    pub zero_is_max: bool,
    pub proof_sources: Vec<SourceType>,
    pub scoring_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualitativeNumericBound {
    pub patterns: Vec<String>,
    pub operator: ConstraintOperator,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumericFactValueKind {
    #[default]
    Numeric,
    DistanceKmInText,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumericAggregation {
    #[default]
    First,
    Minimum,
    Maximum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencePatternSpec {
    pub rank: u32,
    pub patterns: Vec<String>,
    pub label: String,
    pub expanded_keys: Vec<String>,
    #[serde(default)]
    pub gap_keys: Vec<String>,
    pub weight: f32,
    #[serde(default)]
    pub require_evidence: bool,
    #[serde(default)]
    pub missing_evidence_neutral: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEvidenceSchema {
    pub dimension: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub fact_keys: Vec<String>,
    pub positive_terms: Vec<String>,
    pub negative_terms: Vec<String>,
    pub display_label: String,
    pub score_delta: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericEvidenceSchema {
    pub dimension: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub fact_keys: Vec<String>,
    pub direction: String,
    pub thresholds: Vec<f64>,
    pub display_label: String,
    pub score_delta: f64,
}

pub fn registry() -> &'static SearchSchemaConfig {
    static REGISTRY: OnceLock<SearchSchemaConfig> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut config = load_search_schema_config();
        config.theme_layers.sort_by_key(|layer| layer.rank);
        config
            .positive_preference_patterns
            .sort_by_key(|pattern| pattern.rank);
        config
            .negative_preference_patterns
            .sort_by_key(|pattern| pattern.rank);
        config
    })
}

fn load_search_schema_config() -> SearchSchemaConfig {
    let path = dag_root().join("fact_registry.json");
    let file = load_json::<FactRegistrySearchFile>(&path)
        .expect("app/config/dag/fact_registry.json is required for search schema");
    SearchSchemaConfig {
        version: SEARCH_SCHEMA_VERSION,
        runtime: file.runtime,
        theme_layers: file.search_dimensions,
        numeric_constraints: file.numeric_constraints,
        positive_preference_patterns: merge_preference_patterns(file.preference_patterns.positive),
        negative_preference_patterns: merge_preference_patterns(file.preference_patterns.negative),
        text_evidence: file.text_evidence,
        numeric_evidence: file.numeric_evidence,
        fact_search_semantics: file
            .facts
            .into_iter()
            .map(|fact| (fact.fact_key.clone(), fact))
            .collect(),
        excluded_search_fact_keys: file.excluded_search_fact_keys,
    }
}

fn merge_preference_patterns(patterns: Vec<PreferencePatternSpec>) -> Vec<PreferencePatternSpec> {
    let mut merged = Vec::<PreferencePatternSpec>::new();
    for pattern in patterns {
        let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.label.eq_ignore_ascii_case(&pattern.label))
        else {
            merged.push(pattern);
            continue;
        };
        existing.rank = existing.rank.min(pattern.rank);
        for phrase in pattern.patterns {
            if !existing.patterns.contains(&phrase) {
                existing.patterns.push(phrase);
            }
        }
        for fact_key in pattern.expanded_keys {
            if !existing.expanded_keys.contains(&fact_key) {
                existing.expanded_keys.push(fact_key);
            }
        }
        for gap_key in pattern.gap_keys {
            if !existing.gap_keys.contains(&gap_key) {
                existing.gap_keys.push(gap_key);
            }
        }
        existing.weight = existing.weight.max(pattern.weight);
        existing.require_evidence |= pattern.require_evidence;
        existing.missing_evidence_neutral |= pattern.missing_evidence_neutral;
    }
    merged
}

pub fn runtime_policy() -> &'static SearchRuntimePolicy {
    &registry().runtime
}

pub fn search_excludes_fact_key(fact_key: &str) -> bool {
    registry()
        .excluded_search_fact_keys
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(fact_key))
}

pub fn ranking_policy() -> &'static crate::scoring::SearchRankingPolicy {
    crate::scoring::search_ranking_policy()
}

pub fn query_stopwords() -> &'static [String] {
    &runtime_policy().query_stopwords
}

pub fn fact_answers_preferences(fact_key: &str, polarity: &Polarity) -> &'static [String] {
    let expected = match polarity {
        Polarity::Positive => "positive",
        Polarity::Negative => "concern",
    };
    registry()
        .fact_search_semantics
        .get(fact_key)
        .filter(|fact| fact.polarity.eq_ignore_ascii_case(expected))
        .map(|fact| fact.answers_preferences.as_slice())
        .unwrap_or_default()
}

pub fn scoring_stopwords() -> &'static [String] {
    &runtime_policy().scoring_stopwords
}

pub fn placeholder_display_values() -> &'static [String] {
    &runtime_policy().placeholder_display_values
}

pub fn negative_fact_key_terms() -> &'static [String] {
    &runtime_policy().negative_fact_key_terms
}

pub fn negative_preference_allow_terms() -> &'static [String] {
    &runtime_policy().negative_preference_allow_terms
}

pub fn fact_key_self_describe_excluded_suffixes() -> &'static [String] {
    &runtime_policy().fact_key_self_describe_excluded_suffixes
}

pub fn fact_key_self_describe_excluded_exact() -> &'static [String] {
    &runtime_policy().fact_key_self_describe_excluded_exact
}

pub fn registry_fact_key_required_preferences() -> &'static [String] {
    &runtime_policy().registry_fact_key_required_preferences
}

pub fn accepted_tradeoffs() -> &'static [IntentPhraseGroup] {
    &runtime_policy().accepted_tradeoffs
}

pub fn unsupported_inventory_types() -> &'static [IntentPhraseGroup] {
    &runtime_policy().unsupported_inventory_types
}

pub fn buyer_archetype_patterns() -> &'static [BuyerArchetypePattern] {
    &runtime_policy().buyer_archetypes
}

pub fn preference_key_overrides() -> &'static [PreferenceKeyOverride] {
    &runtime_policy().preference_key_overrides
}

pub fn lifecycle_compatibility_rule(
    preference: &str,
) -> Option<&'static LifecycleCompatibilityRule> {
    runtime_policy()
        .lifecycle_compatibility_rules
        .iter()
        .find(|rule| {
            rule.preferences
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(preference))
        })
}

pub fn positive_preference_patterns() -> &'static [PreferencePatternSpec] {
    &registry().positive_preference_patterns
}

pub fn negative_preference_patterns() -> &'static [PreferencePatternSpec] {
    &registry().negative_preference_patterns
}

pub fn numeric_constraint_schema(dimension: &str) -> Option<&'static NumericConstraintSchema> {
    registry()
        .numeric_constraints
        .iter()
        .find(|schema| schema.dimension.eq_ignore_ascii_case(dimension))
}

pub fn text_evidence_schema(preference: &str) -> Option<&'static TextEvidenceSchema> {
    let normalized = preference.to_lowercase();
    registry()
        .text_evidence
        .iter()
        .find(|schema| {
            schema.label.eq_ignore_ascii_case(&normalized)
                || schema.dimension.eq_ignore_ascii_case(&normalized)
        })
        .or_else(|| {
            registry().text_evidence.iter().find(|schema| {
                schema
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&normalized))
            })
        })
        .or_else(|| {
            registry().text_evidence.iter().find(|schema| {
                schema.aliases.iter().any(|alias| {
                    let alias = alias.to_lowercase();
                    alias.contains(&normalized) || normalized.contains(&alias)
                })
            })
        })
}

pub fn numeric_evidence_schema(preference: &str) -> Option<&'static NumericEvidenceSchema> {
    let normalized = preference.to_lowercase();
    registry()
        .numeric_evidence
        .iter()
        .find(|schema| {
            schema.label.eq_ignore_ascii_case(&normalized)
                || schema.dimension.eq_ignore_ascii_case(&normalized)
        })
        .or_else(|| {
            registry().numeric_evidence.iter().find(|schema| {
                schema
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&normalized))
            })
        })
        .or_else(|| {
            registry().numeric_evidence.iter().find(|schema| {
                schema.aliases.iter().any(|alias| {
                    let alias = alias.to_lowercase();
                    alias.contains(&normalized) || normalized.contains(&alias)
                })
            })
        })
}

pub fn source_priority_for_preference(preference: &str) -> Vec<String> {
    let normalized = preference.to_lowercase();
    registry()
        .theme_layers
        .iter()
        .find(|layer| {
            layer.label.eq_ignore_ascii_case(&normalized)
                || layer.dimension.eq_ignore_ascii_case(&normalized)
                || layer.intent_terms.iter().any(|term| {
                    let term = term.to_lowercase();
                    term == normalized || term.contains(&normalized) || normalized.contains(&term)
                })
        })
        .map(|layer| layer.source_priority.clone())
        .unwrap_or_default()
}

pub fn detect_hard_constraints(q: &str) -> Vec<HardConstraint> {
    detect_hard_constraint_spans(q)
        .into_iter()
        .map(|matched| matched.constraint)
        .collect()
}

#[derive(Debug, Clone)]
struct HardConstraintTokenMatch {
    pub constraint: HardConstraint,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HardConstraintSpanMatch {
    pub constraint: HardConstraint,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn detect_hard_constraint_spans(q: &str) -> Vec<HardConstraintSpanMatch> {
    let spanned_tokens = constraint_tokens_with_spans(q);
    let tokens = spanned_tokens
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    detect_hard_constraint_token_matches(&tokens)
        .into_iter()
        .filter_map(|matched| {
            let start = spanned_tokens.get(matched.start)?.start;
            let end = spanned_tokens.get(matched.end.checked_sub(1)?)?.end;
            Some(HardConstraintSpanMatch {
                constraint: matched.constraint,
                start,
                end,
            })
        })
        .collect()
}

fn detect_hard_constraint_token_matches(tokens: &[String]) -> Vec<HardConstraintTokenMatch> {
    let mut matches = registry()
        .numeric_constraints
        .iter()
        .flat_map(|schema| {
            detect_numeric_constraint_matches(tokens, schema)
                .into_iter()
                .chain(detect_qualitative_constraint_matches(tokens, schema))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| {
        (
            matched.start,
            std::cmp::Reverse(matched.end.saturating_sub(matched.start)),
        )
    });
    let mut selected = Vec::<HardConstraintTokenMatch>::new();
    for matched in matches {
        if selected.iter().any(|existing| {
            existing.constraint.field == matched.constraint.field
                && existing.start < matched.end
                && matched.start < existing.end
        }) {
            continue;
        }
        selected.push(matched);
    }
    selected.sort_by_key(|matched| (matched.start, matched.end));
    selected
}

pub fn schema_preference_signal(
    pattern: &PreferencePatternSpec,
    polarity: Polarity,
) -> PreferenceSignal {
    PreferenceSignal {
        raw_text: pattern.label.clone(),
        polarity,
        expanded_keys: pattern.expanded_keys.clone(),
        gap_keys: pattern.gap_keys.clone(),
        weight: pattern.weight,
        required: pattern.require_evidence,
        missing_evidence_neutral: pattern.missing_evidence_neutral,
    }
}

pub fn expanded_keys_for_preference_label(label: &str, negative: bool) -> Vec<String> {
    let patterns = if negative {
        negative_preference_patterns()
    } else {
        positive_preference_patterns()
    };
    let normalized = label.trim().to_lowercase();
    for pattern in patterns {
        if pattern.label.eq_ignore_ascii_case(label) {
            return pattern.expanded_keys.clone();
        }
        if pattern
            .patterns
            .iter()
            .any(|term| normalized.contains(&term.to_lowercase()))
        {
            return pattern.expanded_keys.clone();
        }
    }
    Vec::new()
}

pub fn legacy_display_preference_signal(display_label: &str) -> PreferenceSignal {
    let trimmed = display_label.trim();
    let Some(raw_text) = trimmed.strip_prefix("avoid ") else {
        return preference_signal_for_label(trimmed, Polarity::Positive);
    };
    preference_signal_for_label(raw_text, Polarity::Negative)
}

pub fn preference_signal_for_label(label: &str, polarity: Polarity) -> PreferenceSignal {
    let patterns = match polarity {
        Polarity::Positive => positive_preference_patterns(),
        Polarity::Negative => negative_preference_patterns(),
    };
    let normalized = label.trim().to_lowercase();
    for pattern in patterns {
        if pattern.label.eq_ignore_ascii_case(label)
            || pattern
                .patterns
                .iter()
                .any(|term| normalized.contains(&term.to_lowercase()))
        {
            return schema_preference_signal(pattern, polarity);
        }
    }
    PreferenceSignal {
        raw_text: label.trim().to_string(),
        polarity,
        expanded_keys: Vec::new(),
        gap_keys: Vec::new(),
        weight: 1.0,
        required: false,
        missing_evidence_neutral: false,
    }
}

pub fn preference_signal_for_fact_keys(
    raw_text: &str,
    polarity: Polarity,
    fact_keys: &[String],
) -> PreferenceSignal {
    let patterns = match polarity {
        Polarity::Positive => positive_preference_patterns(),
        Polarity::Negative => negative_preference_patterns(),
    };
    let policy = patterns.iter().find(|pattern| {
        pattern.expanded_keys.iter().any(|configured| {
            fact_keys
                .iter()
                .any(|selected| selected.eq_ignore_ascii_case(configured))
        })
    });
    PreferenceSignal {
        raw_text: raw_text.trim().to_string(),
        polarity,
        expanded_keys: fact_keys.to_vec(),
        gap_keys: Vec::new(),
        weight: policy.map_or(1.0, |pattern| pattern.weight),
        required: policy.is_some_and(|pattern| pattern.require_evidence),
        missing_evidence_neutral: policy.is_some_and(|pattern| pattern.missing_evidence_neutral),
    }
}

pub fn configured_polarity_for_fact_keys(fact_keys: &[String]) -> Option<Polarity> {
    if negative_preference_patterns().iter().any(|pattern| {
        pattern.expanded_keys.iter().any(|configured| {
            fact_keys
                .iter()
                .any(|selected| selected.eq_ignore_ascii_case(configured))
        })
    }) {
        return Some(Polarity::Negative);
    }
    positive_preference_patterns()
        .iter()
        .any(|pattern| {
            pattern.expanded_keys.iter().any(|configured| {
                fact_keys
                    .iter()
                    .any(|selected| selected.eq_ignore_ascii_case(configured))
            })
        })
        .then_some(Polarity::Positive)
}

pub fn derived_fact_keys_for_bhk(base_key: &str, bhk: u32) -> Vec<String> {
    runtime_policy()
        .fact_key_derivations
        .iter()
        .filter(|rule| rule.input_dimension.eq_ignore_ascii_case("bhk"))
        .filter(|rule| {
            rule.min_value.is_none_or(|min| bhk >= min)
                && rule.max_value.is_none_or(|max| bhk <= max)
        })
        .filter(|rule| {
            rule.source_keys
                .iter()
                .any(|source_key| source_key.eq_ignore_ascii_case(base_key))
        })
        .map(|rule| {
            rule.template
                .replace("{key}", base_key)
                .replace("{value}", &bhk.to_string())
                .replace("{bhk}", &bhk.to_string())
        })
        .filter(|key| !key.trim().is_empty())
        .collect()
}

pub fn fact_answers_text_schema(
    fact_key: &str,
    answers_preferences: &[String],
    schema: &TextEvidenceSchema,
) -> bool {
    schema
        .fact_keys
        .iter()
        .any(|key| fact_key.eq_ignore_ascii_case(key))
        || answers_preferences.iter().any(|answer| {
            let answer = answer.to_lowercase();
            schema.label.eq_ignore_ascii_case(&answer)
                || schema.dimension.eq_ignore_ascii_case(&answer)
                || schema.aliases.iter().any(|alias| {
                    let alias = alias.to_lowercase();
                    alias == answer || alias.contains(&answer) || answer.contains(&alias)
                })
        })
}

pub fn text_support_snippet(value: &FactValue, schema: &TextEvidenceSchema) -> Option<String> {
    match value {
        FactValue::Text(text) => snippet_if_supported(text, schema),
        FactValue::Tags(tags) => tags
            .iter()
            .find_map(|tag| snippet_if_supported(tag, schema)),
        _ => None,
    }
}

fn snippet_if_supported(text: &str, schema: &TextEvidenceSchema) -> Option<String> {
    if schema
        .negative_terms
        .iter()
        .any(|term| analyzer::contains_stemmed_phrase(text, term))
    {
        return None;
    }

    if !schema
        .positive_terms
        .iter()
        .any(|term| analyzer::contains_stemmed_phrase(text, term))
    {
        return None;
    }

    Some(truncate_display(text, 150))
}

const CONSTRAINT_ALIAS_WINDOW: usize = 5;
const MIN_CONSTRAINT_OPERATOR_PHRASES: &[&str] = &[
    "at least",
    "above",
    "over",
    "minimum",
    "min",
    "more than",
    "greater than",
    "plus",
    "or higher",
    "or more",
    "and above",
    "or greater",
];
const MAX_CONSTRAINT_OPERATOR_PHRASES: &[&str] = &[
    "at most",
    "below",
    "under",
    "maximum",
    "max",
    "up to",
    "less than",
    "no more than",
    "or lower",
    "or less",
    "or fewer",
    "old",
];
const CONSTRAINT_SYNTAX_FILLERS: &[&str] = &["of", "is", "should", "be", "must"];

fn detect_numeric_constraint_matches(
    tokens: &[String],
    schema: &NumericConstraintSchema,
) -> Vec<HardConstraintTokenMatch> {
    let mut matches = Vec::new();
    for number_start in 0..tokens.len() {
        let Some((value, number_len, has_plus)) = parse_number_phrase(tokens, number_start) else {
            continue;
        };
        let number_end = number_start + number_len;
        let mut best: Option<(usize, usize, HardConstraintTokenMatch)> = None;
        for unit in &schema.query_units {
            let Some((distance, alias_start, alias_end)) =
                nearest_unit_alias(tokens, number_start, number_end, &unit.aliases)
            else {
                continue;
            };
            if distance > CONSTRAINT_ALIAS_WINDOW {
                continue;
            }
            let operator_match = detect_constraint_operator(
                tokens,
                number_start,
                number_end,
                alias_start,
                alias_end,
                has_plus,
            )
            .or_else(|| {
                (value == 0.0 && schema.zero_is_max).then_some((
                    ConstraintOperator::Max,
                    number_start,
                    number_end,
                ))
            });
            let Some((operator, operator_start, operator_end)) = operator_match else {
                continue;
            };
            let label = match operator {
                ConstraintOperator::Min => "above",
                ConstraintOperator::Max => "up to",
            };
            let raw_text = format!("{label} {} {}", format_number(value), unit.unit);
            let constraint = HardConstraint {
                field: schema.dimension.clone(),
                operator,
                value,
                unit: unit.unit.clone(),
                raw_text,
            };
            let span_start = number_start.min(alias_start).min(operator_start);
            let span_end = number_end.max(alias_end).max(operator_end);
            let candidate = (
                distance,
                span_end - span_start,
                HardConstraintTokenMatch {
                    constraint,
                    start: span_start,
                    end: span_end,
                },
            );
            if best
                .as_ref()
                .is_none_or(|current| (candidate.0, candidate.1) < (current.0, current.1))
            {
                best = Some(candidate);
            }
        }
        if let Some((_, _, candidate)) = best {
            matches.push(candidate);
        }
    }
    matches
}

fn detect_qualitative_constraint_matches(
    tokens: &[String],
    schema: &NumericConstraintSchema,
) -> Vec<HardConstraintTokenMatch> {
    let mut matches = schema
        .qualitative_bounds
        .iter()
        .flat_map(|bound| {
            bound.patterns.iter().flat_map(move |pattern| {
                phrase_ranges(tokens, pattern)
                    .into_iter()
                    .map(move |(start, end)| HardConstraintTokenMatch {
                        constraint: HardConstraint {
                            field: schema.dimension.clone(),
                            operator: bound.operator.clone(),
                            value: bound.value,
                            unit: bound.unit.clone(),
                            raw_text: tokens[start..end].join(" "),
                        },
                        start,
                        end,
                    })
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|matched| {
        (
            matched.start,
            std::cmp::Reverse(matched.end.saturating_sub(matched.start)),
        )
    });
    matches.dedup_by(|right, left| {
        left.start == right.start
            && left.end == right.end
            && left.constraint.field == right.constraint.field
    });
    matches
}

fn nearest_unit_alias(
    tokens: &[String],
    number_start: usize,
    number_end: usize,
    aliases: &[String],
) -> Option<(usize, usize, usize)> {
    aliases
        .iter()
        .flat_map(|alias| phrase_ranges(tokens, alias))
        .filter(|(alias_start, alias_end)| {
            unit_alias_is_bound_to_number(
                tokens,
                number_start,
                number_end,
                *alias_start,
                *alias_end,
            )
        })
        .map(|(alias_start, alias_end)| {
            let distance = if alias_end <= number_start {
                number_start - alias_end
            } else {
                alias_start.saturating_sub(number_end)
            };
            (distance, alias_start, alias_end)
        })
        .min_by_key(|candidate| (candidate.0, candidate.2 - candidate.1))
}

fn unit_alias_is_bound_to_number(
    tokens: &[String],
    number_start: usize,
    number_end: usize,
    alias_start: usize,
    alias_end: usize,
) -> bool {
    let bridge = if alias_end <= number_start {
        &tokens[alias_end..number_start]
    } else if number_end <= alias_start {
        &tokens[number_end..alias_start]
    } else {
        return true;
    };
    if bridge.iter().any(|token| token.eq_ignore_ascii_case("or")) {
        let bridge_phrase = bridge.join(" ");
        let is_operator_phrase = MIN_CONSTRAINT_OPERATOR_PHRASES
            .iter()
            .chain(MAX_CONSTRAINT_OPERATOR_PHRASES)
            .any(|phrase| phrase.eq_ignore_ascii_case(&bridge_phrase));
        if !is_operator_phrase {
            return false;
        }
    }
    bridge.iter().all(|token| is_constraint_syntax(token))
}

fn phrase_ranges(tokens: &[String], phrase: &str) -> Vec<(usize, usize)> {
    let phrase_tokens = constraint_tokens(phrase);
    if phrase_tokens.is_empty() || phrase_tokens.len() > tokens.len() {
        return Vec::new();
    }
    tokens
        .windows(phrase_tokens.len())
        .enumerate()
        .filter(|(_, window)| {
            window
                .iter()
                .zip(&phrase_tokens)
                .all(|(token, phrase_token)| token.eq_ignore_ascii_case(phrase_token))
        })
        .map(|(start, _)| (start, start + phrase_tokens.len()))
        .collect()
}

fn detect_constraint_operator(
    tokens: &[String],
    number_start: usize,
    number_end: usize,
    alias_start: usize,
    alias_end: usize,
    has_plus: bool,
) -> Option<(ConstraintOperator, usize, usize)> {
    if has_plus {
        return Some((ConstraintOperator::Min, number_start, number_end));
    }
    let expression_start = number_start.min(alias_start);
    let expression_end = number_end.max(alias_end);
    let window_start = expression_start.saturating_sub(CONSTRAINT_ALIAS_WINDOW);
    let window_end = (expression_end + CONSTRAINT_ALIAS_WINDOW).min(tokens.len());
    let mut best: Option<(usize, usize, ConstraintOperator, usize, usize)> = None;

    for (operator, phrases) in [
        (ConstraintOperator::Min, MIN_CONSTRAINT_OPERATOR_PHRASES),
        (ConstraintOperator::Max, MAX_CONSTRAINT_OPERATOR_PHRASES),
    ] {
        for phrase in phrases {
            for (operator_start, operator_end) in phrase_ranges(tokens, phrase) {
                if operator_start < window_start || operator_end > window_end {
                    continue;
                }
                if !constraint_expression_is_bound(
                    tokens,
                    number_start,
                    number_end,
                    alias_start,
                    alias_end,
                    operator_start,
                    operator_end,
                ) {
                    continue;
                }
                let span_start = expression_start.min(operator_start);
                let span_end = expression_end.max(operator_end);
                let candidate = (span_end - span_start, operator_end - operator_start);
                if best.as_ref().is_none_or(|current| {
                    candidate.0 < current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
                }) {
                    best = Some((
                        candidate.0,
                        candidate.1,
                        operator.clone(),
                        operator_start,
                        operator_end,
                    ));
                }
            }
        }
    }

    best.map(|(_, _, operator, start, end)| (operator, start, end))
}

fn constraint_expression_is_bound(
    tokens: &[String],
    number_start: usize,
    number_end: usize,
    alias_start: usize,
    alias_end: usize,
    operator_start: usize,
    operator_end: usize,
) -> bool {
    let start = number_start.min(alias_start).min(operator_start);
    let end = number_end.max(alias_end).max(operator_end);
    (start..end).all(|index| {
        (number_start..number_end).contains(&index)
            || (alias_start..alias_end).contains(&index)
            || (operator_start..operator_end).contains(&index)
            || is_constraint_syntax(&tokens[index])
    })
}

fn is_constraint_syntax(token: &str) -> bool {
    CONSTRAINT_SYNTAX_FILLERS
        .iter()
        .any(|filler| token.eq_ignore_ascii_case(filler))
        || MIN_CONSTRAINT_OPERATOR_PHRASES
            .iter()
            .chain(MAX_CONSTRAINT_OPERATOR_PHRASES)
            .flat_map(|phrase| phrase.split_whitespace())
            .any(|syntax| token.eq_ignore_ascii_case(syntax))
}

#[derive(Debug, Clone)]
struct SpannedConstraintToken {
    text: String,
    start: usize,
    end: usize,
}

fn constraint_tokens(q: &str) -> Vec<String> {
    constraint_tokens_with_spans(q)
        .into_iter()
        .map(|token| token.text)
        .collect()
}

fn constraint_tokens_with_spans(q: &str) -> Vec<SpannedConstraintToken> {
    let mut mapped = Vec::<(char, usize, usize)>::new();
    let mut chars = q.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let end = start + ch.len_utf8();
        let next = chars.peek().copied();
        match (ch, next.map(|(_, next)| next)) {
            ('>', Some('=')) => {
                let (next_start, next_ch) = chars.next().expect("peeked character should exist");
                push_mapped_replacement(
                    &mut mapped,
                    " at least ",
                    start,
                    next_start + next_ch.len_utf8(),
                );
            }
            ('<', Some('=')) => {
                let (next_start, next_ch) = chars.next().expect("peeked character should exist");
                push_mapped_replacement(
                    &mut mapped,
                    " at most ",
                    start,
                    next_start + next_ch.len_utf8(),
                );
            }
            ('>', _) => push_mapped_replacement(&mut mapped, " above ", start, end),
            ('<', _) => push_mapped_replacement(&mut mapped, " below ", start, end),
            ('%', _) => push_mapped_replacement(&mut mapped, " percent ", start, end),
            ('-', _) => mapped.push((' ', start, end)),
            (',', _) => {}
            _ => mapped.push((ch, start, end)),
        }
    }

    let mut tokens = Vec::new();
    let mut index = 0;
    while index < mapped.len() {
        while index < mapped.len() && mapped[index].0.is_whitespace() {
            index += 1;
        }
        if index >= mapped.len() {
            break;
        }
        let token_start = index;
        while index < mapped.len() && !mapped[index].0.is_whitespace() {
            index += 1;
        }
        let cleaned = mapped[token_start..index]
            .iter()
            .filter(|(ch, _, _)| {
                ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '+' || *ch == '%'
            })
            .collect::<Vec<_>>();
        let (Some(first), Some(last)) = (cleaned.first(), cleaned.last()) else {
            continue;
        };
        tokens.push(SpannedConstraintToken {
            text: cleaned
                .iter()
                .map(|(ch, _, _)| ch.to_ascii_lowercase())
                .collect(),
            start: first.1,
            end: last.2,
        });
    }
    tokens
}

fn push_mapped_replacement(
    mapped: &mut Vec<(char, usize, usize)>,
    replacement: &str,
    start: usize,
    end: usize,
) {
    mapped.extend(replacement.chars().map(|ch| (ch, start, end)));
}

fn parse_number_token(token: &str) -> Option<(f64, bool)> {
    let token = token.trim();
    let has_plus = token.ends_with('+');
    let number = token.trim_end_matches('+');
    let value = number.parse::<f64>().ok()?;
    if value >= 0.0 {
        Some((value, has_plus))
    } else {
        None
    }
}

fn parse_number_phrase(tokens: &[String], start: usize) -> Option<(f64, usize, bool)> {
    let token = tokens.get(start)?;
    if let Some((value, has_plus)) = parse_number_token(token) {
        return Some((value, 1, has_plus));
    }
    let first = number_word_value(token)?;
    if token == "hundred" {
        return Some((100.0, 1, false));
    }
    if tokens
        .get(start + 1)
        .is_some_and(|token| token == "hundred")
    {
        let mut value = first * 100.0;
        let mut len = 2;
        if let Some(remainder) = tokens
            .get(start + 2)
            .and_then(|token| number_word_value(token))
        {
            value += remainder;
            len += 1;
        }
        return Some((value, len, false));
    }
    if first >= 20.0 {
        if let Some(remainder) = tokens
            .get(start + 1)
            .and_then(|token| number_word_value(token))
        {
            if remainder < 10.0 {
                return Some((first + remainder, 2, false));
            }
        }
    }
    Some((first, 1, false))
}

fn number_word_value(token: &str) -> Option<f64> {
    match token {
        "zero" => Some(0.0),
        "one" => Some(1.0),
        "two" => Some(2.0),
        "three" => Some(3.0),
        "four" => Some(4.0),
        "five" => Some(5.0),
        "six" => Some(6.0),
        "seven" => Some(7.0),
        "eight" => Some(8.0),
        "nine" => Some(9.0),
        "ten" => Some(10.0),
        "eleven" => Some(11.0),
        "twelve" => Some(12.0),
        "thirteen" => Some(13.0),
        "fourteen" => Some(14.0),
        "fifteen" => Some(15.0),
        "sixteen" => Some(16.0),
        "seventeen" => Some(17.0),
        "eighteen" => Some(18.0),
        "nineteen" => Some(19.0),
        "twenty" => Some(20.0),
        "thirty" => Some(30.0),
        "forty" => Some(40.0),
        "fifty" => Some(50.0),
        "sixty" => Some(60.0),
        "seventy" => Some(70.0),
        "eighty" => Some(80.0),
        "ninety" => Some(90.0),
        _ => None,
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as u64)
    } else {
        format!("{:.1}", value)
    }
}

fn truncate_display(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_search_schema_from_fact_registry_when_present() {
        let path = dag_root().join("fact_registry.json");
        if !path.exists() {
            return;
        }

        let config = load_search_schema_config();
        assert_eq!(config.version, SEARCH_SCHEMA_VERSION);
        assert!(config.theme_layers.len() >= 20);
        assert!(!config.positive_preference_patterns.is_empty());
        assert!(!config.negative_preference_patterns.is_empty());
        assert!(config
            .runtime
            .query_stopwords
            .contains(&"under".to_string()));
        assert!(config
            .runtime
            .scoring_stopwords
            .contains(&"property".to_string()));
        assert!(config
            .runtime
            .registry_fact_key_required_preferences
            .contains(&"delivered society".to_string()));
        assert!(!config.runtime.fact_key_derivations.is_empty());
        assert!(!config.runtime.lifecycle_compatibility_rules.is_empty());
    }

    #[test]
    fn derives_bhk_scoped_fact_keys_from_registry_policy() {
        assert_eq!(
            derived_fact_keys_for_bhk("listing_price", 3),
            vec!["listing_price_3bhk".to_string()]
        );
        assert!(derived_fact_keys_for_bhk("listing_price", 8).is_empty());
        assert!(derived_fact_keys_for_bhk("google_rating", 3).is_empty());
    }

    #[test]
    fn legacy_display_preferences_resolve_to_structured_signals() {
        let positive = legacy_display_preference_signal("listing evidence");
        assert_eq!(positive.polarity, Polarity::Positive);
        assert!(positive
            .expanded_keys
            .contains(&"listing_price".to_string()));

        let negative = legacy_display_preference_signal("avoid waterlogging risk");
        assert_eq!(negative.polarity, Polarity::Negative);
        assert!(negative
            .expanded_keys
            .contains(&"flooding_risk".to_string()));
        assert!(!negative.gap_keys.is_empty());
    }

    #[test]
    fn loads_ranked_theme_registry() {
        let registry = registry();
        assert_eq!(registry.version, 2);
        assert!(registry.theme_layers.len() >= 20);
        assert_eq!(registry.theme_layers[0].dimension, "price_discovery");
        assert!(registry
            .theme_layers
            .iter()
            .any(|theme| theme.dimension == "amenity_quality"));
    }

    #[test]
    fn detects_acre_constraints_from_registry() {
        let constraints = detect_hard_constraints("3bhk whitefield above 10 acres");
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].field, "land_area");
        assert_eq!(constraints[0].value, 10.0);
        assert_eq!(constraints[0].unit, "acres");
    }

    #[test]
    fn detects_repeated_constraints_for_the_same_dimension() {
        let constraints = detect_hard_constraints("3BHK above 10 acres or 4BHK above 5 acres");
        let acreage = constraints
            .iter()
            .filter(|constraint| constraint.field == "land_area")
            .map(|constraint| constraint.value)
            .collect::<Vec<_>>();

        assert_eq!(acreage, vec![10.0, 5.0]);
    }

    #[test]
    fn detects_configured_rating_review_and_number_word_constraints() {
        let constraints = detect_hard_constraints(
            "ready homes with Google rating >= 4.2 and at least one hundred reviews",
        );
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "google_rating"
                && constraint.operator == ConstraintOperator::Min
                && constraint.value == 4.2
        }));
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "google_review_count"
                && constraint.operator == ConstraintOperator::Min
                && constraint.value == 100.0
        }));

        let project = detect_hard_constraints(
            "projects of at least ten acres with at least seventy percent open area",
        );
        assert!(project
            .iter()
            .any(|constraint| { constraint.field == "land_area" && constraint.value == 10.0 }));
        assert!(project
            .iter()
            .any(|constraint| { constraint.field == "open_area_pct" && constraint.value == 70.0 }));
    }

    #[test]
    fn ignores_removed_zero_complaint_and_revocation_constraints() {
        let constraints =
            detect_hard_constraints("homes with zero RERA complaints and zero builder revocations");
        assert!(constraints.is_empty());
    }

    #[test]
    fn detects_property_age_max_constraints_from_registry() {
        let constraints = detect_hard_constraints("give me 1 year old property");
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].field, "home_age_years");
        assert_eq!(constraints[0].operator, ConstraintOperator::Max);
        assert_eq!(constraints[0].value, 1.0);
        assert_eq!(constraints[0].unit, "years");
    }

    #[test]
    fn ignores_plain_measurement_without_min_operator() {
        let constraints = detect_hard_constraints("3bhk whitefield 10 acres");
        assert!(constraints.is_empty());
    }

    #[test]
    fn does_not_cross_bind_budget_numbers_to_later_density_units() {
        let constraints =
            detect_hard_constraints("3bhk under 4 crore with low homes-per-acre project density");

        assert!(constraints.is_empty());

        let unqualified_acres =
            detect_hard_constraints("3bhk under 4 crore and 3 acres with low density");
        assert!(unqualified_acres.is_empty());

        let explicit =
            detect_hard_constraints("3bhk under 4 crore in a project of at least 4 acres");
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].field, "land_area");
        assert_eq!(explicit[0].operator, ConstraintOperator::Min);
        assert_eq!(explicit[0].value, 4.0);
    }

    #[test]
    fn numeric_constraints_do_not_bind_across_or_branches() {
        let constraints = detect_hard_constraints("3BHK with 10+ acres or at least 80% open space");

        assert_eq!(constraints.len(), 2);
        assert!(constraints
            .iter()
            .any(|constraint| constraint.field == "land_area" && constraint.value == 10.0));
        assert!(constraints
            .iter()
            .any(|constraint| constraint.field == "open_area_pct" && constraint.value == 80.0));
    }

    #[test]
    fn qualitative_numeric_constraints_are_owned_by_config() {
        let constraints = detect_hard_constraints(
            "4BHK with high carpet area and large open space or 3BHK far from lake bed",
        );

        assert!(constraints.iter().any(|constraint| {
            constraint.field == "carpet_area"
                && constraint.operator == ConstraintOperator::Min
                && constraint.value == 2_000.0
        }));
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "open_area_pct"
                && constraint.operator == ConstraintOperator::Min
                && constraint.value == 60.0
        }));
        assert!(constraints.iter().any(|constraint| {
            constraint.field == "lake_distance"
                && constraint.operator == ConstraintOperator::Min
                && constraint.value == 0.5
        }));
    }

    #[test]
    fn only_configured_categorical_requirements_demand_evidence() {
        let ready = preference_signal_for_label("ready to move", Polarity::Positive);
        let greenery = preference_signal_for_label("greenery", Polarity::Positive);

        assert!(ready.required);
        assert!(!greenery.required);
    }

    #[test]
    fn maps_reddit_style_terms_to_schema_preferences() {
        let post = "clubhouse and pool decently maintained with many trees near metro";
        let labels: Vec<&str> = positive_preference_patterns()
            .iter()
            .filter(|pattern| pattern.patterns.iter().any(|term| post.contains(term)))
            .map(|pattern| pattern.label.as_str())
            .collect();
        assert!(labels.contains(&"amenity quality"));
        assert!(labels.contains(&"greenery"));
        assert!(labels.contains(&"metro access"));
    }

    #[test]
    fn loads_negative_risk_preferences_from_registry() {
        let labels: Vec<&str> = negative_preference_patterns()
            .iter()
            .map(|pattern| pattern.label.as_str())
            .collect();
        assert!(labels.contains(&"waterlogging risk"));
        assert!(labels.contains(&"traffic"));
        assert!(numeric_evidence_schema("waterlogging risk").is_some());
        assert!(numeric_evidence_schema("builder trust").is_none());
        assert!(text_evidence_schema("traffic").is_some());
    }

    #[test]
    fn loads_home_state_preferences_and_evidence_from_registry() {
        let labels: Vec<&str> = positive_preference_patterns()
            .iter()
            .map(|pattern| pattern.label.as_str())
            .collect();
        assert!(labels.contains(&"delivered society"));
        assert!(labels.contains(&"new property"));
        assert!(labels.contains(&"established society"));
        assert!(labels.contains(&"under construction"));

        let delivered = text_evidence_schema("delivered society").unwrap();
        assert!(delivered.fact_keys.contains(&"home_state".to_string()));
        let new_property = text_evidence_schema("new property").unwrap();
        assert!(new_property
            .fact_keys
            .contains(&"home_age_bucket".to_string()));
        let old_society = text_evidence_schema("old society").unwrap();
        assert!(old_society
            .fact_keys
            .contains(&"home_age_bucket".to_string()));
        assert!(text_evidence_schema("delay risk")
            .unwrap()
            .fact_keys
            .contains(&"home_timeline_state".to_string()));
    }

    #[test]
    fn loads_rera_configuration_preferences_from_registry() {
        let labels: Vec<&str> = positive_preference_patterns()
            .iter()
            .map(|pattern| pattern.label.as_str())
            .collect();
        assert!(labels.contains(&"floor plan evidence"));
        assert!(labels.contains(&"3bhk configuration"));

        let config_layer = registry()
            .theme_layers
            .iter()
            .find(|layer| layer.dimension == "rera_project_configuration")
            .expect("configuration search dimension should load");
        assert!(config_layer
            .fact_keys
            .contains(&"available_configurations".to_string()));
        assert!(config_layer
            .fact_keys
            .contains(&"floor_plan_asset_count".to_string()));
    }

    #[test]
    fn loads_buyer_externality_terms_from_registry() {
        for (label, fact_key) in [
            ("approach road", "approach_road_condition"),
            ("airport access", "airport_distance_km"),
            ("lake proximity", "lake_waterlogging_context"),
            ("environment sensitivity", "environment_sensitivity"),
            ("stp concern", "stp_concern"),
            ("high tension wires", "high_tension_wire_concern"),
        ] {
            let schema = text_evidence_schema(label).unwrap();
            assert!(
                schema.fact_keys.contains(&fact_key.to_string()),
                "{label} should use {fact_key}: {:?}",
                schema.fact_keys
            );
        }
    }
}
