use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::knowledge::fact::SourceType;
use crate::knowledge::FactValue;

use super::intent::{ConstraintOperator, HardConstraint, Polarity, PreferenceSignal};

#[cfg(test)]
pub const SQM_PER_ACRE: f64 = 4046.8564224;

use crate::dag_config::{dag_root, load_json};

const FACT_SCHEMA_REGISTRY_FALLBACK_JSON: &str =
    include_str!("../../../data/search/fact_schema_registry.json");

/// Search schema format version carried in serving bundles.
pub const SEARCH_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactRegistrySearchFile {
    pub version: u32,
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
    if path.exists() {
        if let Ok(file) = load_json::<FactRegistrySearchFile>(&path) {
            return SearchSchemaConfig {
                version: SEARCH_SCHEMA_VERSION,
                theme_layers: file.search_dimensions,
                numeric_constraints: file.numeric_constraints,
                positive_preference_patterns: file.preference_patterns.positive,
                negative_preference_patterns: file.preference_patterns.negative,
                text_evidence: file.text_evidence,
                numeric_evidence: file.numeric_evidence,
            };
        }
    }

    let mut config: SearchSchemaConfig =
        serde_json::from_str(FACT_SCHEMA_REGISTRY_FALLBACK_JSON).expect("valid fact schema registry");
    if config.version == 0 {
        config.version = SEARCH_SCHEMA_VERSION;
    }
    config
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
    let lower = text.to_lowercase();
    if schema
        .negative_terms
        .iter()
        .any(|term| lower.contains(term))
    {
        return None;
    }

    if !schema
        .positive_terms
        .iter()
        .any(|term| lower.contains(term))
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

fn constraint_tokens(q: &str) -> Vec<String> {
    q.replace(',', "")
        .split_whitespace()
        .filter_map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '+')
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
