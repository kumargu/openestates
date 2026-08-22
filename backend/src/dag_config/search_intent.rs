use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use std::collections::HashSet;

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchIntentFile {
    pub version: u32,
    #[serde(default)]
    pub area_aliases: AreaAliasConfig,
    #[serde(default)]
    pub resolution: SearchResolutionConfig,
    #[serde(default)]
    pub classifier: IntentClassifierConfig,
    pub parser: SearchParserConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AreaAliasConfig {
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub entries: Vec<AreaAliasEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AreaAliasEntry {
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchResolutionConfig {
    #[serde(default)]
    pub min_resolvable_entity_name_chars: usize,
    #[serde(default)]
    pub ignored_entity_names: Vec<String>,
    #[serde(default)]
    pub resolvable_entity_types: Vec<String>,
    #[serde(default)]
    pub min_partial_entity_name_chars: usize,
    #[serde(default)]
    pub mechanical_alias_blocked_tokens: Vec<String>,
    #[serde(default)]
    pub named_entity_scope_prefixes: Vec<String>,
    #[serde(default)]
    pub generic_scope_nouns: Vec<String>,
    #[serde(default)]
    pub exclusion_prefixes: Vec<String>,
    #[serde(default)]
    pub place_families: Vec<SearchPlaceFamilyAlias>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPlaceFamilyAlias {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntentClassifierMode {
    #[default]
    Disabled,
    Shadow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntentClassifierConfig {
    #[serde(default)]
    pub mode: IntentClassifierMode,
    #[serde(default)]
    pub artifact_path: String,
    #[serde(default)]
    pub held_out_path: String,
    #[serde(default)]
    pub minimum_probability: f32,
    #[serde(default)]
    pub minimum_margin: f32,
    #[serde(default)]
    pub minimum_training_examples_per_label: usize,
    #[serde(default)]
    pub maximum_clauses: usize,
    #[serde(default)]
    pub maximum_clause_tokens: usize,
    #[serde(default)]
    pub labels: Vec<IntentClassifierLabelConfig>,
    #[serde(default)]
    pub clause_separators: Vec<String>,
    #[serde(default)]
    pub training: IntentClassifierTrainingConfig,
}

impl Default for IntentClassifierConfig {
    fn default() -> Self {
        Self {
            mode: IntentClassifierMode::Disabled,
            artifact_path: String::new(),
            held_out_path: String::new(),
            minimum_probability: 0.72,
            minimum_margin: 0.2,
            minimum_training_examples_per_label: 3,
            maximum_clauses: 4,
            maximum_clause_tokens: 18,
            labels: Vec::new(),
            clause_separators: Vec::new(),
            training: IntentClassifierTrainingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IntentClassifierLabelConfig {
    pub preference: String,
    pub polarity: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntentClassifierTrainingConfig {
    #[serde(default = "default_classifier_epoch")]
    pub epoch: i32,
    #[serde(default = "default_classifier_learning_rate")]
    pub learning_rate: f64,
    #[serde(default = "default_classifier_dimension")]
    pub dimension: i32,
    #[serde(default = "default_classifier_word_ngrams")]
    pub word_ngrams: i32,
    #[serde(default = "default_classifier_min_character_ngrams")]
    pub min_character_ngrams: i32,
    #[serde(default = "default_classifier_max_character_ngrams")]
    pub max_character_ngrams: i32,
    #[serde(default = "default_classifier_bucket")]
    pub bucket: i32,
    #[serde(default = "default_classifier_thread")]
    pub thread: i32,
    #[serde(default = "default_classifier_seed")]
    pub seed: i32,
}

impl Default for IntentClassifierTrainingConfig {
    fn default() -> Self {
        Self {
            epoch: default_classifier_epoch(),
            learning_rate: default_classifier_learning_rate(),
            dimension: default_classifier_dimension(),
            word_ngrams: default_classifier_word_ngrams(),
            min_character_ngrams: default_classifier_min_character_ngrams(),
            max_character_ngrams: default_classifier_max_character_ngrams(),
            bucket: default_classifier_bucket(),
            thread: default_classifier_thread(),
            seed: default_classifier_seed(),
        }
    }
}

const fn default_classifier_epoch() -> i32 {
    50
}

const fn default_classifier_learning_rate() -> f64 {
    0.5
}

const fn default_classifier_dimension() -> i32 {
    48
}

const fn default_classifier_word_ngrams() -> i32 {
    2
}

const fn default_classifier_min_character_ngrams() -> i32 {
    3
}

const fn default_classifier_max_character_ngrams() -> i32 {
    6
}

const fn default_classifier_bucket() -> i32 {
    50_000
}

const fn default_classifier_thread() -> i32 {
    1
}

const fn default_classifier_seed() -> i32 {
    42
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchParserConfig {
    pub bhk: BhkParserConfig,
    pub budget: UnitValueParserConfig,
    pub distance: UnitValueParserConfig,
    pub relations: RelationParserConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BhkParserConfig {
    pub unit_aliases: Vec<String>,
    pub number_words: Vec<NumberWord>,
    pub min: u32,
    pub max: u32,
    #[serde(default)]
    pub alternative_joiners: Vec<String>,
    #[serde(default)]
    pub exclusion_gap_tokens: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NumberWord {
    pub word: String,
    pub value: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitValueParserConfig {
    pub operators: Vec<String>,
    #[serde(default)]
    pub min_operators: Vec<String>,
    #[serde(default)]
    pub range_connectors: Vec<String>,
    pub units: Vec<UnitAliasConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitAliasConfig {
    pub unit: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub multiplier: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationParserConfig {
    pub aliases: Vec<RelationAliasConfig>,
    #[serde(default = "default_relation_max_clauses")]
    pub max_clauses: usize,
    #[serde(default)]
    pub clause_joiners: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationAliasConfig {
    pub alias: String,
    #[serde(default)]
    pub requires_distance_limit: bool,
}

pub fn search_intent_path() -> std::path::PathBuf {
    dag_root().join("search_intent.json")
}

pub fn load_search_intent_from_path(path: &Path) -> Result<SearchIntentFile, DagConfigError> {
    let file: SearchIntentFile = load_json(path)?;
    validate_search_intent_file(&file)?;
    Ok(file)
}

pub fn load_search_intent() -> Result<SearchIntentFile, DagConfigError> {
    load_search_intent_from_path(&search_intent_path())
}

pub fn area_alias_entries() -> &'static [AreaAliasEntry] {
    static ENTRIES: OnceLock<Vec<AreaAliasEntry>> = OnceLock::new();
    ENTRIES
        .get_or_init(|| match load_search_intent() {
            Ok(file) if !file.area_aliases.entries.is_empty() => file.area_aliases.entries,
            _ => embedded_area_aliases(),
        })
        .as_slice()
}

pub fn search_resolution_config() -> &'static SearchResolutionConfig {
    static CONFIG: OnceLock<SearchResolutionConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_search_intent()
            .map(|file| file.resolution)
            .unwrap_or_default()
    })
}

pub fn search_parser_config() -> &'static SearchParserConfig {
    static CONFIG: OnceLock<SearchParserConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_search_intent()
            .map(|file| file.parser)
            .expect("search_intent.json parser config must load and validate")
    })
}

pub fn intent_classifier_config() -> &'static IntentClassifierConfig {
    static CONFIG: OnceLock<IntentClassifierConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_search_intent()
            .map(|file| file.classifier)
            .expect("search_intent.json classifier config must load and validate")
    })
}

fn validate_search_intent_file(file: &SearchIntentFile) -> Result<(), DagConfigError> {
    validate_aliases(
        "resolution.named_entity_scope_prefixes",
        file.resolution
            .named_entity_scope_prefixes
            .iter()
            .map(String::as_str),
    )
    .map_err(DagConfigError::InvalidConfig)?;
    validate_classifier_config(&file.classifier).map_err(DagConfigError::InvalidConfig)?;
    validate_parser_config(&file.parser).map_err(DagConfigError::InvalidConfig)
}

fn validate_classifier_config(config: &IntentClassifierConfig) -> Result<(), String> {
    if config.mode != IntentClassifierMode::Disabled {
        if config.artifact_path.trim().is_empty() {
            return Err("classifier.artifact_path must not be empty when enabled".to_string());
        }
        if config.held_out_path.trim().is_empty() {
            return Err("classifier.held_out_path must not be empty when enabled".to_string());
        }
    }
    if !(0.0..=1.0).contains(&config.minimum_probability)
        || !(0.0..=1.0).contains(&config.minimum_margin)
    {
        return Err("classifier probability and margin must be within 0..=1".to_string());
    }
    if config.maximum_clauses == 0 || config.maximum_clause_tokens == 0 {
        return Err("classifier clause limits must be positive".to_string());
    }
    let mut label_ids = HashSet::new();
    for label in &config.labels {
        let preference = label.preference.trim();
        let polarity = label.polarity.trim().to_ascii_lowercase();
        if preference.is_empty() || !matches!(polarity.as_str(), "positive" | "negative") {
            return Err(
                "classifier labels require a preference and positive/negative polarity".to_string(),
            );
        }
        if !label_ids.insert((preference.to_ascii_lowercase(), polarity)) {
            return Err(format!(
                "classifier label is duplicated: {} {}",
                label.polarity, label.preference
            ));
        }
    }
    if config.mode != IntentClassifierMode::Disabled && config.labels.is_empty() {
        return Err("classifier.labels must not be empty when enabled".to_string());
    }
    if config.training.epoch <= 0
        || config.training.dimension <= 0
        || config.training.word_ngrams <= 0
        || config.training.min_character_ngrams < 0
        || config.training.max_character_ngrams < config.training.min_character_ngrams
        || config.training.bucket <= 0
        || config.training.thread <= 0
        || config.training.learning_rate <= 0.0
    {
        return Err("classifier training settings must be positive and ordered".to_string());
    }
    validate_aliases(
        "classifier.clause_separators",
        config.clause_separators.iter().map(String::as_str),
    )
}

fn validate_parser_config(config: &SearchParserConfig) -> Result<(), String> {
    if config.bhk.min == 0 || config.bhk.max < config.bhk.min {
        return Err("parser.bhk min/max must define a positive ascending range".to_string());
    }
    validate_aliases(
        "parser.bhk.unit_aliases",
        config.bhk.unit_aliases.iter().map(String::as_str),
    )?;
    if config.bhk.unit_aliases.is_empty() {
        return Err("parser.bhk.unit_aliases must not be empty".to_string());
    }
    if config.bhk.number_words.is_empty() {
        return Err("parser.bhk.number_words must not be empty".to_string());
    }
    if config.bhk.number_words.iter().any(|entry| {
        entry.word.trim().is_empty() || entry.value < config.bhk.min || entry.value > config.bhk.max
    }) {
        return Err(
            "parser.bhk.number_words must be non-empty words inside the BHK range".to_string(),
        );
    }
    validate_aliases(
        "parser.bhk.number_words",
        config
            .bhk
            .number_words
            .iter()
            .map(|entry| entry.word.as_str()),
    )?;
    if !config.bhk.alternative_joiners.is_empty() {
        validate_aliases(
            "parser.bhk.alternative_joiners",
            config.bhk.alternative_joiners.iter().map(String::as_str),
        )?;
    }
    validate_unit_value_config("parser.budget", &config.budget, true)?;
    validate_unit_value_config("parser.distance", &config.distance, true)?;
    if config.relations.aliases.is_empty() {
        return Err("parser.relations.aliases must not be empty".to_string());
    }
    validate_aliases(
        "parser.relations.aliases",
        config
            .relations
            .aliases
            .iter()
            .map(|entry| entry.alias.as_str()),
    )?;
    if config.relations.max_clauses == 0 || config.relations.max_clauses > 16 {
        return Err("parser.relations.max_clauses must be between 1 and 16".to_string());
    }
    validate_aliases(
        "parser.relations.clause_joiners",
        config.relations.clause_joiners.iter().map(String::as_str),
    )?;
    Ok(())
}

fn default_relation_max_clauses() -> usize {
    4
}

fn validate_unit_value_config(
    label: &str,
    config: &UnitValueParserConfig,
    require_operators: bool,
) -> Result<(), String> {
    if require_operators && config.operators.iter().all(|value| value.trim().is_empty()) {
        return Err(format!("{label}.operators must not be empty"));
    }
    if config.units.is_empty() {
        return Err(format!("{label}.units must not be empty"));
    }
    if !config.min_operators.is_empty() {
        validate_aliases(
            &format!("{label}.min_operators"),
            config.min_operators.iter().map(String::as_str),
        )?;
    }
    if !config.range_connectors.is_empty() {
        validate_aliases(
            &format!("{label}.range_connectors"),
            config.range_connectors.iter().map(String::as_str),
        )?;
    }
    for unit in &config.units {
        if unit.unit.trim().is_empty() {
            return Err(format!("{label}.units contains an empty unit id"));
        }
        if unit.aliases.iter().all(|alias| alias.trim().is_empty()) {
            return Err(format!(
                "{label}.units.{} aliases must not be empty",
                unit.unit
            ));
        }
        validate_aliases(
            &format!("{label}.units.{}.aliases", unit.unit),
            unit.aliases.iter().map(String::as_str),
        )?;
        if !unit.multiplier.is_finite() || unit.multiplier <= 0.0 {
            return Err(format!(
                "{label}.units.{} multiplier must be positive and finite",
                unit.unit
            ));
        }
    }
    Ok(())
}

fn validate_aliases<'a>(label: &str, aliases: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = HashSet::new();
    let mut count = 0;
    for alias in aliases {
        count += 1;
        let normalized = normalize_alias(alias);
        if normalized.is_empty() {
            return Err(format!("{label} contains an empty alias"));
        }
        if !seen.insert(normalized) {
            return Err(format!("{label} contains a duplicate alias"));
        }
    }
    if count == 0 {
        return Err(format!("{label} must not be empty"));
    }
    Ok(())
}

fn normalize_alias(alias: &str) -> String {
    alias
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn embedded_area_aliases() -> Vec<AreaAliasEntry> {
    vec![
        AreaAliasEntry {
            canonical: "Whitefield".to_string(),
            aliases: vec![
                "whitefield".into(),
                "wf".into(),
                "kadugodi".into(),
                "varthur".into(),
                "itpl".into(),
                "hope farm".into(),
                "kundalahalli".into(),
                "pattandur agrahara".into(),
                "brookefield".into(),
                "nallurhalli".into(),
                "hagadur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Sarjapur Road".to_string(),
            aliases: vec![
                "sarjapur".into(),
                "sarjapur road".into(),
                "sjr".into(),
                "doddakannelli".into(),
                "carmelaram".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Bellandur".to_string(),
            aliases: vec![
                "bellandur".into(),
                "outer ring road".into(),
                "orr bellandur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "HSR Layout".to_string(),
            aliases: vec![
                "hsr".into(),
                "hsr layout".into(),
                "agara".into(),
                "sector 1 hsr".into(),
                "sector 2 hsr".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "North Bengaluru".to_string(),
            aliases: vec![
                "north bangalore".into(),
                "north bengaluru".into(),
                "north blr".into(),
                "devanahalli".into(),
                "hebbal".into(),
                "yelahanka".into(),
                "thanisandra".into(),
                "jakkur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Electronic City".to_string(),
            aliases: vec!["electronic city".into(), "ec".into(), "ecity".into()],
        },
        AreaAliasEntry {
            canonical: "Koramangala".to_string(),
            aliases: vec!["koramangala".into(), "koramangala 5th block".into()],
        },
        AreaAliasEntry {
            canonical: "Marathahalli".to_string(),
            aliases: vec!["marathahalli".into(), "marathon halli".into()],
        },
        AreaAliasEntry {
            canonical: "Indiranagar".to_string(),
            aliases: vec!["indiranagar".into(), "indira nagar".into()],
        },
        AreaAliasEntry {
            canonical: "Jayanagar".to_string(),
            aliases: vec!["jayanagar".into(), "jaya nagar".into()],
        },
        AreaAliasEntry {
            canonical: "Bannerghatta Road".to_string(),
            aliases: vec![
                "bannerghatta".into(),
                "bannerghatta road".into(),
                "btm".into(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_broad_region_aliases_from_search_intent_config() {
        let path = search_intent_path();
        if !path.exists() {
            return;
        }
        let file = load_search_intent().expect("search_intent.json should parse");
        assert!(!file.area_aliases.entries.is_empty());
        assert!(file
            .area_aliases
            .entries
            .iter()
            .any(|entry| entry.canonical == "East Bengaluru"));
        assert!(!file
            .area_aliases
            .entries
            .iter()
            .any(|entry| entry.canonical == "Whitefield"));
        assert_eq!(file.classifier.mode, IntentClassifierMode::Shadow);
        assert!(!file.classifier.labels.is_empty());
        assert!(file.classifier.minimum_probability >= 0.72);
    }

    #[test]
    fn parser_config_is_required_and_validated() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_parser_path = temp_dir.path().join("missing_parser.json");
        std::fs::write(
            &missing_parser_path,
            r#"{
              "version": 1,
              "area_aliases": { "entries": [] },
              "resolution": {}
            }"#,
        )
        .expect("write fixture");

        assert!(load_search_intent_from_path(&missing_parser_path).is_err());

        let invalid_parser_path = temp_dir.path().join("invalid_parser.json");
        std::fs::write(
            &invalid_parser_path,
            r#"{
              "version": 1,
              "area_aliases": { "entries": [] },
              "resolution": {},
              "parser": {
                "bhk": { "unit_aliases": [], "number_words": [], "min": 1, "max": 6 },
                "budget": { "operators": [], "units": [] },
                "distance": { "operators": ["within"], "units": [] },
                "relations": { "aliases": [] }
              }
            }"#,
        )
        .expect("write fixture");

        assert!(load_search_intent_from_path(&invalid_parser_path).is_err());
    }
}
