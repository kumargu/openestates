use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::knowledge::fact::SourceType;
use crate::knowledge::FactValue;

use super::intent::{ConstraintOperator, HardConstraint, Polarity, PreferenceSignal};

#[cfg(test)]
pub const SQM_PER_ACRE: f64 = 4046.8564224;

const FACT_SCHEMA_REGISTRY_JSON: &str =
    include_str!("../../../data/search/fact_schema_registry.json");

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
    pub text_evidence: Vec<TextEvidenceSchema>,
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

pub fn registry() -> &'static SearchSchemaConfig {
    static REGISTRY: OnceLock<SearchSchemaConfig> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut config: SearchSchemaConfig =
            serde_json::from_str(FACT_SCHEMA_REGISTRY_JSON).expect("valid fact schema registry");
        config
            .theme_layers
            .sort_by(|left, right| left.rank.cmp(&right.rank));
        config
            .positive_preference_patterns
            .sort_by(|left, right| left.rank.cmp(&right.rank));
        config
    })
}

pub fn positive_preference_patterns() -> &'static [PreferencePatternSpec] {
    &registry().positive_preference_patterns
}

pub fn numeric_constraint_schema(dimension: &str) -> Option<&'static NumericConstraintSchema> {
    registry()
        .numeric_constraints
        .iter()
        .find(|schema| schema.dimension.eq_ignore_ascii_case(dimension))
}

pub fn text_evidence_schema(preference: &str) -> Option<&'static TextEvidenceSchema> {
    let normalized = preference.to_lowercase();
    registry().text_evidence.iter().find(|schema| {
        schema.label.eq_ignore_ascii_case(&normalized)
            || schema.dimension.eq_ignore_ascii_case(&normalized)
            || schema.aliases.iter().any(|alias| {
                let alias = alias.to_lowercase();
                alias == normalized || alias.contains(&normalized) || normalized.contains(&alias)
            })
    })
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
}
