use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchGuardrailFile {
    pub version: u32,
    #[serde(default)]
    pub too_short: TooShortGuardrailConfig,
    #[serde(default)]
    pub home_intent_detection: HomeIntentDetectionConfig,
    #[serde(default)]
    pub assistant_directed_question: AssistantDirectedQuestionConfig,
    #[serde(default)]
    pub decision_brief: PhraseGuardrailConfig,
    #[serde(default)]
    pub vague_home_query: PhraseGuardrailConfig,
    #[serde(default)]
    pub guidance: SearchGuardrailGuidanceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TooShortGuardrailConfig {
    #[serde(default = "default_min_tokens")]
    pub min_tokens: usize,
}

impl Default for TooShortGuardrailConfig {
    fn default() -> Self {
        Self {
            min_tokens: default_min_tokens(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PhraseGuardrailConfig {
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HomeIntentDetectionConfig {
    #[serde(default = "default_minimum_positive_score")]
    pub minimum_positive_score: i32,
    #[serde(default = "default_minimum_short_query_score")]
    pub minimum_short_query_score: i32,
    #[serde(default = "default_weak_anchor_max_tokens")]
    pub weak_anchor_max_tokens: usize,
    #[serde(default)]
    pub structured_signal_scores: StructuredSignalScores,
    #[serde(default)]
    pub term_groups: Vec<WeightedTermGroup>,
    #[serde(default)]
    pub weak_anchor_terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StructuredSignalScores {
    #[serde(default = "default_area_score")]
    pub area: i32,
    #[serde(default = "default_bhk_score")]
    pub bhk: i32,
    #[serde(default = "default_budget_score")]
    pub budget_max: i32,
    #[serde(default = "default_excluded_area_score")]
    pub excluded_area: i32,
    #[serde(default = "default_hard_constraint_score")]
    pub hard_constraint: i32,
    #[serde(default = "default_preference_score")]
    pub preference: i32,
    #[serde(default = "default_buyer_archetype_score")]
    pub buyer_archetype: i32,
    #[serde(default = "default_unsupported_inventory_score")]
    pub unsupported_inventory: i32,
}

impl Default for StructuredSignalScores {
    fn default() -> Self {
        Self {
            area: default_area_score(),
            bhk: default_bhk_score(),
            budget_max: default_budget_score(),
            excluded_area: default_excluded_area_score(),
            hard_constraint: default_hard_constraint_score(),
            preference: default_preference_score(),
            buyer_archetype: default_buyer_archetype_score(),
            unsupported_inventory: default_unsupported_inventory_score(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WeightedTermGroup {
    pub name: String,
    #[serde(default)]
    pub score: i32,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub substrings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AssistantDirectedQuestionConfig {
    #[serde(default)]
    pub question_starters: Vec<String>,
    #[serde(default)]
    pub assistant_subject_terms: Vec<String>,
    #[serde(default)]
    pub allowed_search_action_terms: Vec<String>,
    #[serde(default = "default_assistant_directed_max_structured_score")]
    pub max_structured_score: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchGuardrailGuidanceConfig {
    pub empty_query: SearchGuidanceTemplate,
    pub too_short: SearchGuidanceTemplate,
    pub out_of_scope: SearchGuidanceTemplate,
    pub decision_brief: SearchGuidanceTemplate,
    pub needs_more_specifics: SearchGuidanceTemplate,
    pub needs_home_anchor: SearchGuidanceTemplate,
    pub unsupported_inventory: SearchGuidanceTemplate,
    pub no_results: SearchGuidanceTemplate,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchGuidanceTemplate {
    pub mode: String,
    pub title: String,
    pub message: String,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

fn default_min_tokens() -> usize {
    2
}

fn default_minimum_positive_score() -> i32 {
    2
}

fn default_minimum_short_query_score() -> i32 {
    6
}

fn default_weak_anchor_max_tokens() -> usize {
    4
}

fn default_area_score() -> i32 {
    1
}

fn default_bhk_score() -> i32 {
    3
}

fn default_budget_score() -> i32 {
    3
}

fn default_excluded_area_score() -> i32 {
    2
}

fn default_hard_constraint_score() -> i32 {
    2
}

fn default_preference_score() -> i32 {
    2
}

fn default_buyer_archetype_score() -> i32 {
    2
}

fn default_unsupported_inventory_score() -> i32 {
    2
}

fn default_assistant_directed_max_structured_score() -> i32 {
    0
}

pub fn search_guardrails_path() -> std::path::PathBuf {
    dag_root().join("search_guardrails.json")
}

pub fn load_search_guardrails_from_path(
    path: &Path,
) -> Result<SearchGuardrailFile, DagConfigError> {
    load_json(path)
}

pub fn load_search_guardrails() -> Result<SearchGuardrailFile, DagConfigError> {
    load_search_guardrails_from_path(&search_guardrails_path())
}

pub fn search_guardrail_config() -> &'static SearchGuardrailFile {
    static CONFIG: OnceLock<SearchGuardrailFile> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_search_guardrails().expect("app/config/dag/search_guardrails.json is required")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_guardrails_config_loads() {
        let path = search_guardrails_path();
        if !path.exists() {
            return;
        }
        let config = load_search_guardrails().expect("search_guardrails.json should load");
        assert_eq!(config.version, 1);
        assert!(!config.home_intent_detection.term_groups.is_empty());
        assert!(!config
            .assistant_directed_question
            .assistant_subject_terms
            .is_empty());
        assert_eq!(config.guidance.out_of_scope.mode, "out_of_scope");
    }
}
