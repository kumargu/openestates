use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactRegistryFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub legacy_key_map: HashMap<String, String>,
    #[serde(default)]
    pub facts: Vec<FactRegistryEntry>,
    #[serde(default)]
    pub fact_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactRegistryEntry {
    pub fact_key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub answers_preferences: Vec<String>,
    #[serde(default)]
    pub display_template: Option<String>,
    #[serde(default)]
    pub scoring_hint: Option<FactRegistryScoringHint>,
    #[serde(default)]
    pub never_default: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactRegistryScoringHint {
    pub direction: String,
    #[serde(default)]
    pub numeric_direction: Option<String>,
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub thresholds: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct FactRegistryIndex {
    by_key: HashMap<String, FactRegistryEntry>,
    legacy_key_map: HashMap<String, String>,
}

impl FactRegistryIndex {
    pub fn from_file(file: &FactRegistryFile) -> Self {
        let mut by_key = HashMap::new();
        for entry in &file.facts {
            by_key.insert(entry.fact_key.clone(), entry.clone());
        }
        Self {
            by_key,
            legacy_key_map: file.legacy_key_map.clone(),
        }
    }

    pub fn lookup(&self, fact_key: &str) -> Option<&FactRegistryEntry> {
        self.by_key
            .get(fact_key)
            .or_else(|| {
                self.legacy_key_map
                    .get(fact_key)
                    .and_then(|canonical| self.by_key.get(canonical))
            })
    }

    pub fn fact_count(&self) -> usize {
        self.by_key.len()
    }
}

pub fn fact_registry_path() -> std::path::PathBuf {
    dag_root().join("fact_registry.json")
}

pub fn load_fact_registry() -> Result<FactRegistryFile, DagConfigError> {
    load_fact_registry_from_path(&fact_registry_path())
}

pub fn load_fact_registry_from_path(path: &Path) -> Result<FactRegistryFile, DagConfigError> {
    load_json(path)
}

pub fn load_fact_registry_index() -> Result<FactRegistryIndex, DagConfigError> {
    Ok(FactRegistryIndex::from_file(&load_fact_registry()?))
}

pub fn scoring_direction_from_hint(hint: &FactRegistryScoringHint) -> String {
    let direction = hint.direction.to_ascii_lowercase();
    if direction == "numeric" {
        hint.numeric_direction
            .clone()
            .unwrap_or_else(|| "LowerIsBetter".to_string())
    } else if direction == "text_match" {
        "TextMatch".to_string()
    } else {
        hint.direction.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_registry_loads_minimum_fact_count() {
        let path = fact_registry_path();
        if !path.exists() {
            return;
        }

        let registry = load_fact_registry_from_path(&path).expect("fact_registry.json loads");
        assert!(
            registry.facts.len() >= 78,
            "expected >= 78 facts, got {}",
            registry.facts.len()
        );
    }

    #[test]
    fn fact_registry_has_core_preference_patterns_in_search_schema() {
        let path = fact_registry_path();
        if !path.exists() {
            return;
        }

        let registry = crate::search::schema::registry();
        let labels: Vec<&str> = registry
            .positive_preference_patterns
            .iter()
            .map(|pattern| pattern.label.as_str())
            .collect();
        assert!(labels.contains(&"amenity quality"));
        assert!(labels.contains(&"greenery"));
        assert!(labels.contains(&"metro access"));
    }

    #[test]
    fn legacy_key_map_resolves_short_keys() {
        let path = fact_registry_path();
        if !path.exists() {
            return;
        }

        let registry = load_fact_registry_from_path(&path).expect("fact_registry.json loads");
        let index = FactRegistryIndex::from_file(&registry);
        assert!(index.lookup("operating.maintenance_charges").is_some());
        if registry.legacy_key_map.contains_key("maintenance_charges") {
            assert!(index.lookup("maintenance_charges").is_some());
        }
    }
}
