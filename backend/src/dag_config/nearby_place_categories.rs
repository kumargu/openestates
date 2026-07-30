use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

const EMBEDDED_NEARBY_PLACE_CATEGORIES: &str =
    include_str!("../../../app/config/dag/nearby_place_categories.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyPlaceCategoriesFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub categories: Vec<NearbyPlaceCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyPlaceCategory {
    pub fact_key: String,
    #[serde(default)]
    pub category_aliases: Vec<String>,
    #[serde(default)]
    pub display_label: String,
    #[serde(default)]
    pub answers_preferences: Vec<String>,
    #[serde(default)]
    pub relation_class: String,
    #[serde(default)]
    pub chainable: bool,
    #[serde(default)]
    pub max_distance_km: Option<f64>,
    #[serde(default)]
    pub allow_missing_place_types: bool,
    #[serde(default)]
    pub accepted_place_types: Vec<String>,
    #[serde(default)]
    pub require_name_marker: bool,
    #[serde(default)]
    pub name_markers: Vec<String>,
    #[serde(default)]
    pub name_block_markers: Vec<String>,
    #[serde(default)]
    pub derived_distance_risks: Vec<DerivedDistanceRisk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedDistanceRisk {
    pub fact_key: String,
    #[serde(default)]
    pub display_label: String,
    #[serde(default)]
    pub answers_preferences: Vec<String>,
    #[serde(default)]
    pub max_distance_km: Option<f64>,
    #[serde(default)]
    pub scoring_weight: Option<f64>,
}

pub fn nearby_place_categories_path() -> PathBuf {
    dag_root().join("nearby_place_categories.json")
}

pub fn load_nearby_place_categories_from_path(
    path: &Path,
) -> Result<NearbyPlaceCategoriesFile, DagConfigError> {
    let config: NearbyPlaceCategoriesFile = load_json(path)?;
    validate_nearby_place_categories(&config)?;
    Ok(config)
}

pub fn load_nearby_place_categories() -> Result<NearbyPlaceCategoriesFile, DagConfigError> {
    load_nearby_place_categories_from_path(&nearby_place_categories_path())
}

pub fn nearby_place_categories_config() -> &'static NearbyPlaceCategoriesFile {
    static CONFIG: OnceLock<NearbyPlaceCategoriesFile> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_nearby_place_categories()
            .or_else(|_| {
                let config: NearbyPlaceCategoriesFile =
                    serde_json::from_str(EMBEDDED_NEARBY_PLACE_CATEGORIES)?;
                validate_nearby_place_categories(&config)?;
                Ok::<NearbyPlaceCategoriesFile, DagConfigError>(config)
            })
            .expect("embedded nearby_place_categories.json must load and validate")
    })
}

pub fn nearby_place_category_for_fact_key(fact_key: &str) -> Option<&'static str> {
    nearby_place_categories_config()
        .categories
        .iter()
        .find(|category| category.fact_key.eq_ignore_ascii_case(fact_key))
        .map(|category| category.fact_key.as_str())
}

pub fn requested_nearby_place_categories(query_lower: &str) -> Vec<&'static str> {
    let mut requested = Vec::new();
    for category in &nearby_place_categories_config().categories {
        if category
            .query_terms()
            .iter()
            .any(|term| crate::search::resolver::query_contains_lower_text(query_lower, term))
            && !requested
                .iter()
                .any(|existing| *existing == category.fact_key.as_str())
        {
            requested.push(category.fact_key.as_str());
        }
    }
    requested
}

impl NearbyPlaceCategory {
    fn query_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for value in self
            .category_aliases
            .iter()
            .chain(self.answers_preferences.iter())
            .chain(self.accepted_place_types.iter())
            .chain(self.name_markers.iter())
        {
            push_normalized_term(&mut terms, value);
        }
        push_normalized_term(&mut terms, &self.display_label);
        terms
    }
}

fn validate_nearby_place_categories(
    config: &NearbyPlaceCategoriesFile,
) -> Result<(), DagConfigError> {
    if config.categories.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "nearby_place_categories must define at least one category".to_string(),
        ));
    }
    let mut fact_keys = HashSet::new();
    for category in &config.categories {
        if category.fact_key.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "nearby_place_categories contains a blank fact_key".to_string(),
            ));
        }
        if !fact_keys.insert(category.fact_key.to_ascii_lowercase()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "nearby_place_categories contains duplicate fact_key {}",
                category.fact_key
            )));
        }
        if category.category_aliases.is_empty() && category.answers_preferences.is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "nearby category {} must define aliases or answer preferences",
                category.fact_key
            )));
        }
    }
    Ok(())
}

fn push_normalized_term(terms: &mut Vec<String>, value: &str) {
    let term = value
        .trim()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if !term.is_empty() && !terms.iter().any(|existing| existing == &term) {
        terms.push(term);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_place_categories_load_from_dag_config() {
        let config = load_nearby_place_categories().expect("nearby categories should load");
        assert!(config
            .categories
            .iter()
            .any(|category| category.fact_key == "nearby_breweries"));
    }

    #[test]
    fn query_category_lookup_is_config_driven() {
        let requested = requested_nearby_place_categories("walkable gym and brewery nearby");
        assert!(requested.contains(&"nearby_fitness"));
        assert!(requested.contains(&"nearby_breweries"));
        assert_eq!(
            nearby_place_category_for_fact_key("nearby_lakes"),
            Some("nearby_lakes")
        );
    }
}
