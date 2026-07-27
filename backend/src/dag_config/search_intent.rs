use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use std::collections::HashSet;

use super::evidence_sections::load_evidence_sections;
use super::fact_registry::load_fact_registry_index;
use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchIntentFile {
    pub version: u32,
    #[serde(default)]
    pub area_aliases: AreaAliasConfig,
    #[serde(default)]
    pub resolution: SearchResolutionConfig,
    pub parser: SearchParserConfig,
    #[serde(default)]
    pub area_exclusion_prefixes: Vec<String>,
    #[serde(default)]
    pub negated_prefixes: Vec<String>,
    #[serde(default)]
    pub preference_key_derivations: PreferenceKeyDerivationConfig,
    #[serde(default)]
    pub conflict_key_policy: ConflictKeyPolicyConfig,
    #[serde(default)]
    pub hard_constraint_dimensions: Vec<HardConstraintDimensionConfig>,
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
    pub place_families: Vec<SearchPlaceFamilyAlias>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPlaceFamilyAlias {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct NumberWord {
    pub word: String,
    pub value: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitValueParserConfig {
    pub operators: Vec<String>,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationAliasConfig {
    pub alias: String,
    #[serde(default)]
    pub requires_distance_limit: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PreferenceKeyDerivationConfig {
    #[serde(default)]
    pub bhk: Vec<BhkFactKeyDerivationConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BhkFactKeyDerivationConfig {
    #[serde(default)]
    pub generic_keys: Vec<String>,
    #[serde(default)]
    pub bhk_values: Vec<u32>,
    pub derived_key_template: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConflictKeyPolicyConfig {
    #[serde(default)]
    pub excluded_exact: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HardConstraintDimensionConfig {
    pub field: String,
    #[serde(default)]
    pub registry_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ScoringRuntimeFactKeysFile {
    #[serde(default)]
    runtime_fact_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct FactRegistryNumericConstraintsFile {
    #[serde(default)]
    numeric_constraints: Vec<NumericConstraintDimensionRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct NumericConstraintDimensionRef {
    dimension: String,
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
        .get_or_init(|| {
            load_search_intent()
                .expect("search_intent.json area aliases must load and validate")
                .area_aliases
                .entries
        })
        .as_slice()
}

pub fn area_exclusion_prefixes() -> &'static [String] {
    static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();
    PREFIXES
        .get_or_init(|| {
            load_search_intent()
                .expect("search_intent.json area exclusion prefixes must load and validate")
                .area_exclusion_prefixes
        })
        .as_slice()
}

pub fn negated_prefixes() -> &'static [String] {
    static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();
    PREFIXES
        .get_or_init(|| {
            load_search_intent()
                .expect("search_intent.json negated prefixes must load and validate")
                .negated_prefixes
        })
        .as_slice()
}

pub fn bhk_fact_key_derivations() -> &'static [BhkFactKeyDerivationConfig] {
    static DERIVATIONS: OnceLock<Vec<BhkFactKeyDerivationConfig>> = OnceLock::new();
    DERIVATIONS
        .get_or_init(|| {
            load_search_intent()
                .expect("search_intent.json BHK derivations must load and validate")
                .preference_key_derivations
                .bhk
        })
        .as_slice()
}

pub fn conflict_excluded_exact_keys() -> &'static [String] {
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        load_search_intent()
            .expect("search_intent.json conflict key policy must load and validate")
            .conflict_key_policy
            .excluded_exact
    })
    .as_slice()
}

pub fn hard_constraint_dimensions() -> &'static [HardConstraintDimensionConfig] {
    static DIMENSIONS: OnceLock<Vec<HardConstraintDimensionConfig>> = OnceLock::new();
    DIMENSIONS
        .get_or_init(|| {
            load_search_intent()
                .expect("search_intent.json hard constraint dimensions must load and validate")
                .hard_constraint_dimensions
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

fn validate_search_intent_file(file: &SearchIntentFile) -> Result<(), DagConfigError> {
    validate_parser_config(&file.parser).map_err(DagConfigError::InvalidConfig)?;
    validate_search_intent_runtime(file).map_err(DagConfigError::InvalidConfig)
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
    Ok(())
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

fn validate_search_intent_runtime(file: &SearchIntentFile) -> Result<(), String> {
    validate_aliases(
        "area_exclusion_prefixes",
        file.area_exclusion_prefixes.iter().map(String::as_str),
    )?;
    validate_aliases(
        "negated_prefixes",
        file.negated_prefixes.iter().map(String::as_str),
    )?;
    validate_aliases(
        "conflict_key_policy.excluded_exact",
        file.conflict_key_policy
            .excluded_exact
            .iter()
            .map(String::as_str),
    )?;

    let registry = load_fact_registry_index().map_err(|err| err.to_string())?;
    let known_search_keys = known_search_fact_keys().map_err(|err| err.to_string())?;
    for (index, derivation) in file.preference_key_derivations.bhk.iter().enumerate() {
        if derivation.generic_keys.is_empty() {
            return Err(format!(
                "preference_key_derivations.bhk[{index}].generic_keys must not be empty"
            ));
        }
        if derivation.bhk_values.is_empty() {
            return Err(format!(
                "preference_key_derivations.bhk[{index}].bhk_values must not be empty"
            ));
        }
        if derivation
            .bhk_values
            .iter()
            .any(|value| *value < file.parser.bhk.min || *value > file.parser.bhk.max)
        {
            return Err(format!(
                "preference_key_derivations.bhk[{index}].bhk_values must be inside parser.bhk range"
            ));
        }
        if !derivation.derived_key_template.contains("{key}")
            || !derivation.derived_key_template.contains("{bhk}")
        {
            return Err(format!(
                "preference_key_derivations.bhk[{index}].derived_key_template must contain {{key}} and {{bhk}}"
            ));
        }
        for key in &derivation.generic_keys {
            if key.trim().is_empty() {
                return Err(format!(
                    "preference_key_derivations.bhk[{index}] contains an empty generic key"
                ));
            }
            for bhk in &derivation.bhk_values {
                let derived = derivation
                    .derived_key_template
                    .replace("{key}", key)
                    .replace("{bhk}", &bhk.to_string());
                if registry.lookup(&derived).is_none() && !known_search_keys.contains(&derived) {
                    return Err(format!(
                        "preference_key_derivations.bhk[{index}] derives unknown fact key {derived}"
                    ));
                }
            }
        }
    }

    if file.hard_constraint_dimensions.is_empty() {
        return Err("hard_constraint_dimensions must not be empty".to_string());
    }
    validate_aliases(
        "hard_constraint_dimensions.field",
        file.hard_constraint_dimensions
            .iter()
            .map(|dimension| dimension.field.as_str()),
    )?;
    let configured_numeric_dimensions =
        configured_numeric_constraint_dimensions().map_err(|err| err.to_string())?;
    for dimension in &file.hard_constraint_dimensions {
        if !configured_numeric_dimensions
            .iter()
            .any(|configured| configured.eq_ignore_ascii_case(&dimension.field))
        {
            return Err(format!(
                "hard_constraint_dimensions references unknown numeric constraint dimension {}",
                dimension.field
            ));
        }
    }

    Ok(())
}

fn known_search_fact_keys() -> Result<HashSet<String>, DagConfigError> {
    let mut keys = HashSet::new();
    let scoring_path = dag_root().join("scoring_policy.json");
    let scoring: ScoringRuntimeFactKeysFile = load_json(&scoring_path)?;
    keys.extend(scoring.runtime_fact_keys);
    for section in load_evidence_sections()? {
        keys.extend(section.facts.into_iter().map(|fact| fact.key));
    }
    Ok(keys)
}

fn configured_numeric_constraint_dimensions() -> Result<HashSet<String>, DagConfigError> {
    let path = dag_root().join("fact_registry.json");
    let registry: FactRegistryNumericConstraintsFile = load_json(&path)?;
    Ok(registry
        .numeric_constraints
        .into_iter()
        .map(|constraint| constraint.dimension)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_area_aliases_from_search_intent_config() {
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
            .any(|entry| entry.canonical == "Whitefield"));
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

    #[test]
    fn parser_runtime_rejects_invalid_derivation_templates_and_dimensions() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let invalid_template_path = temp_dir.path().join("invalid_template.json");
        std::fs::write(
            &invalid_template_path,
            search_intent_fixture(
                r#""preference_key_derivations": {
                    "bhk": [{
                      "generic_keys": ["listing_price"],
                      "bhk_values": [3],
                      "derived_key_template": "listing_price_static"
                    }]
                  }"#,
                r#""hard_constraint_dimensions": [{ "field": "land_area" }]"#,
            ),
        )
        .expect("write fixture");
        assert!(load_search_intent_from_path(&invalid_template_path).is_err());

        let invalid_dimension_path = temp_dir.path().join("invalid_dimension.json");
        std::fs::write(
            &invalid_dimension_path,
            search_intent_fixture(
                r#""preference_key_derivations": {
                    "bhk": [{
                      "generic_keys": ["listing_price"],
                      "bhk_values": [3],
                      "derived_key_template": "{key}_{bhk}bhk"
                    }]
                  }"#,
                r#""hard_constraint_dimensions": [{ "field": "made_up_dimension" }]"#,
            ),
        )
        .expect("write fixture");
        assert!(load_search_intent_from_path(&invalid_dimension_path).is_err());
    }

    fn search_intent_fixture(derivations: &str, dimensions: &str) -> String {
        format!(
            r#"{{
              "version": 1,
              "area_aliases": {{
                "entries": [{{
                  "canonical": "Whitefield",
                  "aliases": ["whitefield"]
                }}]
              }},
              "resolution": {{}},
              "parser": {{
                "bhk": {{
                  "unit_aliases": ["bhk"],
                  "number_words": [{{ "word": "three", "value": 3 }}],
                  "min": 1,
                  "max": 6
                }},
                "budget": {{
                  "operators": ["under"],
                  "units": [{{ "unit": "crore", "aliases": ["cr"], "multiplier": 10000000.0 }}]
                }},
                "distance": {{
                  "operators": ["within"],
                  "units": [{{ "unit": "km", "aliases": ["km"], "multiplier": 1.0 }}]
                }},
                "relations": {{ "aliases": [{{ "alias": "near" }}] }}
              }},
              "area_exclusion_prefixes": ["not"],
              "negated_prefixes": ["not"],
              "conflict_key_policy": {{ "excluded_exact": ["price_per_sqft"] }},
              {derivations},
              {dimensions}
            }}"#
        )
    }
}
