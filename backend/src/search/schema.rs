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
    pub semantic_stopwords: Vec<String>,
    #[serde(default)]
    pub accepted_tradeoffs: Vec<IntentPhraseGroup>,
    #[serde(default)]
    pub unsupported_inventory_types: Vec<IntentPhraseGroup>,
    #[serde(default)]
    pub buyer_archetypes: Vec<BuyerArchetypePattern>,
    #[serde(default)]
    pub preference_key_overrides: Vec<PreferenceKeyOverride>,
    #[serde(default)]
    pub semantic_expansions: Vec<SemanticExpansionSpec>,
    #[serde(default)]
    pub ranking: SearchRankingPolicy,
    #[serde(default)]
    pub lifecycle_value_terms: LifecycleValueTerms,
    #[serde(default)]
    pub lifecycle_compatibility_rules: Vec<LifecycleCompatibilityRule>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticExpansionSpec {
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub expanded_tokens: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRankingPolicy {
    #[serde(default = "default_min_support_evidence_confidence")]
    pub min_support_evidence_confidence: f32,
    #[serde(default = "default_min_llm_evidence_confidence")]
    pub min_llm_evidence_confidence: f32,
    #[serde(default = "default_negative_no_data_penalty_multiplier")]
    pub negative_no_data_penalty_multiplier: f64,
    #[serde(default = "default_min_semantic_recall_score")]
    pub min_semantic_recall_score: f64,
    #[serde(default = "default_semantic_candidate_fit_weight")]
    pub semantic_candidate_fit_weight: f64,
    #[serde(default = "default_semantic_candidate_fit_cap")]
    pub semantic_candidate_fit_cap: f64,
    #[serde(default = "default_broad_local_recall_multiplier")]
    pub broad_local_recall_multiplier: usize,
    #[serde(default = "default_broad_local_recall_min_extra")]
    pub broad_local_recall_min_extra: usize,
    #[serde(default = "default_positive_evidence_floor_ratio")]
    pub positive_evidence_floor_ratio: f64,
    #[serde(default = "default_no_positive_evidence_score_multiplier")]
    pub no_positive_evidence_score_multiplier: f64,
    #[serde(default = "default_nearby_area_score_penalty")]
    pub nearby_area_score_penalty: f64,
    #[serde(default = "default_graph_area_score_penalty")]
    pub graph_area_score_penalty: f64,
    #[serde(default)]
    pub geo_distance_fact_keys: Vec<String>,
    #[serde(default = "default_nearby_distance_full_score_km")]
    pub nearby_distance_full_score_km: f64,
    #[serde(default = "default_nearby_distance_zero_score_km")]
    pub nearby_distance_zero_score_km: f64,
    #[serde(default = "default_nearby_distance_bonus_cap")]
    pub nearby_distance_bonus_cap: f64,
    #[serde(default = "default_named_place_full_score_km")]
    pub named_place_full_score_km: f64,
    #[serde(default = "default_named_place_zero_score_km")]
    pub named_place_zero_score_km: f64,
    #[serde(default = "default_named_place_score_weight")]
    pub named_place_score_weight: f64,
    #[serde(default = "default_min_score_with_positive_evidence")]
    pub min_score_with_positive_evidence: f64,
    #[serde(default = "default_max_score_with_positive_evidence")]
    pub max_score_with_positive_evidence: f64,
    #[serde(default = "default_min_score_with_risk_only_evidence")]
    pub min_score_with_risk_only_evidence: f64,
    #[serde(default = "default_min_score_with_constraint_only")]
    pub min_score_with_constraint_only: f64,
    #[serde(default = "default_fact_coverage_threshold")]
    pub fact_coverage_threshold: f64,
}

impl Default for SearchRankingPolicy {
    fn default() -> Self {
        Self {
            min_support_evidence_confidence: default_min_support_evidence_confidence(),
            min_llm_evidence_confidence: default_min_llm_evidence_confidence(),
            negative_no_data_penalty_multiplier: default_negative_no_data_penalty_multiplier(),
            min_semantic_recall_score: default_min_semantic_recall_score(),
            semantic_candidate_fit_weight: default_semantic_candidate_fit_weight(),
            semantic_candidate_fit_cap: default_semantic_candidate_fit_cap(),
            broad_local_recall_multiplier: default_broad_local_recall_multiplier(),
            broad_local_recall_min_extra: default_broad_local_recall_min_extra(),
            positive_evidence_floor_ratio: default_positive_evidence_floor_ratio(),
            no_positive_evidence_score_multiplier: default_no_positive_evidence_score_multiplier(),
            nearby_area_score_penalty: default_nearby_area_score_penalty(),
            graph_area_score_penalty: default_graph_area_score_penalty(),
            geo_distance_fact_keys: Vec::new(),
            nearby_distance_full_score_km: default_nearby_distance_full_score_km(),
            nearby_distance_zero_score_km: default_nearby_distance_zero_score_km(),
            nearby_distance_bonus_cap: default_nearby_distance_bonus_cap(),
            named_place_full_score_km: default_named_place_full_score_km(),
            named_place_zero_score_km: default_named_place_zero_score_km(),
            named_place_score_weight: default_named_place_score_weight(),
            min_score_with_positive_evidence: default_min_score_with_positive_evidence(),
            max_score_with_positive_evidence: default_max_score_with_positive_evidence(),
            min_score_with_risk_only_evidence: default_min_score_with_risk_only_evidence(),
            min_score_with_constraint_only: default_min_score_with_constraint_only(),
            fact_coverage_threshold: default_fact_coverage_threshold(),
        }
    }
}

fn default_min_support_evidence_confidence() -> f32 {
    0.60
}

fn default_min_llm_evidence_confidence() -> f32 {
    0.75
}

fn default_negative_no_data_penalty_multiplier() -> f64 {
    1.2
}

fn default_min_semantic_recall_score() -> f64 {
    0.08
}

fn default_semantic_candidate_fit_weight() -> f64 {
    1.0
}

fn default_semantic_candidate_fit_cap() -> f64 {
    0.25
}

fn default_broad_local_recall_multiplier() -> usize {
    4
}

fn default_broad_local_recall_min_extra() -> usize {
    64
}

fn default_positive_evidence_floor_ratio() -> f64 {
    0.60
}

fn default_no_positive_evidence_score_multiplier() -> f64 {
    0.40
}

fn default_nearby_area_score_penalty() -> f64 {
    -0.35
}

fn default_graph_area_score_penalty() -> f64 {
    -0.25
}

fn default_nearby_distance_full_score_km() -> f64 {
    0.75
}

fn default_nearby_distance_zero_score_km() -> f64 {
    3.0
}

fn default_nearby_distance_bonus_cap() -> f64 {
    0.8
}

fn default_named_place_full_score_km() -> f64 {
    0.75
}

fn default_named_place_zero_score_km() -> f64 {
    5.0
}

fn default_named_place_score_weight() -> f64 {
    2.0
}

fn default_min_score_with_positive_evidence() -> f64 {
    0.2
}

fn default_max_score_with_positive_evidence() -> f64 {
    0.45
}

fn default_min_score_with_risk_only_evidence() -> f64 {
    0.1
}

fn default_min_score_with_constraint_only() -> f64 {
    0.01
}

fn default_fact_coverage_threshold() -> f64 {
    25.0
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
    pub query_units: Vec<QueryUnit>,
    pub proof_sources: Vec<SourceType>,
    pub scoring_method: String,
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
        config
            .theme_layers
            .sort_by(|left, right| left.rank.cmp(&right.rank));
        config
            .positive_preference_patterns
            .sort_by(|left, right| left.rank.cmp(&right.rank));
        config
            .negative_preference_patterns
            .sort_by(|left, right| left.rank.cmp(&right.rank));
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
        positive_preference_patterns: file.preference_patterns.positive,
        negative_preference_patterns: file.preference_patterns.negative,
        text_evidence: file.text_evidence,
        numeric_evidence: file.numeric_evidence,
    }
}

pub fn runtime_policy() -> &'static SearchRuntimePolicy {
    &registry().runtime
}

pub fn ranking_policy() -> &'static crate::scoring::SearchRankingPolicy {
    crate::scoring::search_ranking_policy()
}

pub fn query_stopwords() -> &'static [String] {
    &runtime_policy().query_stopwords
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

pub fn semantic_stopwords() -> &'static [String] {
    &runtime_policy().semantic_stopwords
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

pub fn semantic_expansion_tokens(pattern: &str) -> Vec<&'static str> {
    runtime_policy()
        .semantic_expansions
        .iter()
        .find(|expansion| {
            expansion
                .patterns
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(pattern))
        })
        .map(|expansion| {
            expansion
                .expanded_tokens
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default()
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
    let tokens = constraint_tokens(q);
    let mut constraints = Vec::new();

    for schema in &registry().numeric_constraints {
        for unit in &schema.query_units {
            if let Some((value, raw_text)) = detect_min_unit_value(&tokens, unit) {
                constraints.push(HardConstraint {
                    field: schema.dimension.clone(),
                    operator: ConstraintOperator::Min,
                    value,
                    unit: unit.unit.clone(),
                    raw_text,
                });
                break;
            }
            if schema.dimension.eq_ignore_ascii_case("home_age_years") {
                if let Some((value, raw_text)) = detect_max_age_value(&tokens, unit) {
                    constraints.push(HardConstraint {
                        field: schema.dimension.clone(),
                        operator: ConstraintOperator::Max,
                        value,
                        unit: unit.unit.clone(),
                        raw_text,
                    });
                    break;
                }
            }
        }
    }

    constraints
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

fn detect_min_unit_value(tokens: &[String], unit: &QueryUnit) -> Option<(f64, String)> {
    for (i, token) in tokens.iter().enumerate() {
        if unit
            .aliases
            .iter()
            .any(|alias| token.eq_ignore_ascii_case(alias))
        {
            if i == 0 {
                continue;
            }
            if let Some((value, has_plus)) = parse_number_token(&tokens[i - 1]) {
                if has_plus || has_min_constraint_prefix(&tokens[..i - 1]) {
                    return Some((
                        value,
                        format!("above {} {}", format_number(value), unit.unit),
                    ));
                }
            }
            continue;
        }

        if let Some((value, has_plus)) = parse_unit_compound(token, unit) {
            if has_plus || has_min_constraint_prefix(&tokens[..i]) {
                return Some((
                    value,
                    format!("above {} {}", format_number(value), unit.unit),
                ));
            }
        }
    }

    None
}

fn detect_max_age_value(tokens: &[String], unit: &QueryUnit) -> Option<(f64, String)> {
    for (i, token) in tokens.iter().enumerate() {
        let unit_matches = unit
            .aliases
            .iter()
            .any(|alias| token.eq_ignore_ascii_case(alias));
        if unit_matches && i > 0 {
            if let Some((value, _)) = parse_number_token(&tokens[i - 1]) {
                let after = tokens.get(i + 1).map(String::as_str).unwrap_or("");
                let before = if i >= 2 { tokens[i - 2].as_str() } else { "" };
                if after.eq_ignore_ascii_case("old")
                    || before.eq_ignore_ascii_case("under")
                    || before.eq_ignore_ascii_case("below")
                    || before.eq_ignore_ascii_case("within")
                {
                    return Some((
                        value,
                        format!("up to {} {}", format_number(value), unit.unit),
                    ));
                }
            }
        }
        if let Some((value, _)) = parse_unit_compound(token, unit) {
            let after = tokens.get(i + 1).map(String::as_str).unwrap_or("");
            let before = if i >= 1 { tokens[i - 1].as_str() } else { "" };
            if after.eq_ignore_ascii_case("old")
                || before.eq_ignore_ascii_case("under")
                || before.eq_ignore_ascii_case("below")
                || before.eq_ignore_ascii_case("within")
            {
                return Some((
                    value,
                    format!("up to {} {}", format_number(value), unit.unit),
                ));
            }
        }
    }
    None
}

fn constraint_tokens(q: &str) -> Vec<String> {
    q.replace(',', "")
        .split_whitespace()
        .filter_map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '+' || *ch == '%')
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        })
        .collect()
}

fn has_min_constraint_prefix(tokens_before_number: &[String]) -> bool {
    let start = tokens_before_number.len().saturating_sub(4);
    let window = &tokens_before_number[start..];

    if window
        .iter()
        .any(|token| matches!(token.as_str(), "above" | "over" | "minimum" | "min"))
    {
        return true;
    }

    window.windows(2).any(|pair| {
        matches!(
            (pair[0].as_str(), pair[1].as_str()),
            ("at", "least") | ("more", "than") | ("greater", "than")
        )
    })
}

fn parse_unit_compound(token: &str, unit: &QueryUnit) -> Option<(f64, bool)> {
    for alias in &unit.aliases {
        if let Some(num) = token.strip_suffix(alias.as_str()) {
            return parse_number_token(num);
        }
    }
    None
}

fn parse_number_token(token: &str) -> Option<(f64, bool)> {
    let token = token.trim();
    let has_plus = token.ends_with('+');
    let number = token.trim_end_matches('+');
    let value = number.parse::<f64>().ok()?;
    if value > 0.0 {
        Some((value, has_plus))
    } else {
        None
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
        assert!(config
            .runtime
            .semantic_stopwords
            .contains(&"want".to_string()));
        assert!(config.runtime.ranking.semantic_candidate_fit_cap > 0.0);
        assert!(config.runtime.ranking.fact_coverage_threshold >= 1.0);
        assert!(!config.runtime.lifecycle_compatibility_rules.is_empty());
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
        let builder_trust = numeric_evidence_schema("builder trust")
            .expect("builder trust should have RERA numeric risk evidence");
        assert!(builder_trust
            .fact_keys
            .contains(&"rera_builder_revocations".to_string()));
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
