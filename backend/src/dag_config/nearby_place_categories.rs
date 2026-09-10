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
    #[serde(default)]
    pub canonical_identity: CanonicalPlaceIdentityPolicy,
    pub categories: Vec<NearbyPlaceCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalPlaceIdentityPolicy {
    pub merge_distance_m: f64,
    #[serde(default)]
    pub identity_source_priority: Vec<String>,
}

impl Default for CanonicalPlaceIdentityPolicy {
    fn default() -> Self {
        Self {
            merge_distance_m: 100.0,
            identity_source_priority: vec!["google".to_string(), "openstreetmap".to_string()],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialRole {
    Region,
    Footprint,
    #[default]
    Destination,
    NetworkNode,
    Route,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyPlaceCategory {
    pub fact_key: String,
    #[serde(default)]
    pub spatial_role: SpatialRole,
    #[serde(default)]
    pub canonical_name_suffix: Option<String>,
    #[serde(default)]
    pub category_aliases: Vec<String>,
    #[serde(default)]
    pub collection_sources: Vec<String>,
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

pub fn nearby_place_fact_key_matches_category(fact_key: &str, place_category: &str) -> bool {
    let place_category = normalize_category_value(place_category);
    !place_category.is_empty()
        && nearby_place_categories_config()
            .categories
            .iter()
            .find(|category| category.fact_key.eq_ignore_ascii_case(fact_key))
            .is_some_and(|category| {
                category
                    .category_aliases
                    .iter()
                    .any(|alias| normalize_category_value(alias) == place_category)
            })
}

pub fn requested_nearby_place_categories(query_lower: &str) -> Vec<&'static str> {
    let query_tokens = crate::search::parser::query_tokens(query_lower);
    let category_matches = nearby_place_categories_config()
        .categories
        .iter()
        .map(|category| {
            let ranges = category
                .query_terms()
                .iter()
                .flat_map(|term| matching_token_ranges(&query_tokens, term))
                .collect::<Vec<_>>();
            (category, ranges)
        })
        .collect::<Vec<_>>();

    category_matches
        .iter()
        .enumerate()
        .filter(|(category_index, (_, ranges))| {
            ranges.iter().any(|candidate| {
                !category_matches
                    .iter()
                    .enumerate()
                    .any(|(other_index, (_, other_ranges))| {
                        other_index != *category_index
                            && other_ranges.iter().any(|other_range| {
                                other_range.0 <= candidate.0
                                    && other_range.1 >= candidate.1
                                    && (other_range.1 - other_range.0) > (candidate.1 - candidate.0)
                            })
                    })
            })
        })
        .map(|(_, (category, _))| category.fact_key.as_str())
        .collect()
}

fn matching_token_ranges(tokens: &[String], term: &str) -> Vec<(usize, usize)> {
    let term_tokens = crate::search::parser::query_tokens(term);
    if term_tokens.is_empty() || term_tokens.len() > tokens.len() {
        return Vec::new();
    }
    tokens
        .windows(term_tokens.len())
        .enumerate()
        .filter(|(_, window)| {
            window
                .iter()
                .zip(term_tokens.iter())
                .all(|(token, term)| token.eq_ignore_ascii_case(term))
        })
        .map(|(start, _)| (start, start + term_tokens.len()))
        .collect()
}

impl NearbyPlaceCategory {
    fn query_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for value in self
            .category_aliases
            .iter()
            .chain(self.accepted_place_types.iter())
            .chain(self.name_markers.iter())
        {
            push_normalized_term(&mut terms, value);
        }
        push_normalized_term(&mut terms, &self.display_label);
        terms
    }

    pub fn matches_place(
        &self,
        name: &str,
        place_types: &[String],
        place_category: Option<&str>,
    ) -> bool {
        let normalized_name = normalize_category_text(name);
        if self
            .name_block_markers
            .iter()
            .any(|blocked| contains_category_text(&normalized_name, blocked))
        {
            return false;
        }
        let name_marker_match = self
            .name_markers
            .iter()
            .any(|marker| contains_category_text(&normalized_name, marker));
        if let Some(place_category) = place_category.map(normalize_category_value) {
            let category_matches = self
                .category_aliases
                .iter()
                .any(|alias| normalize_category_value(alias) == place_category);
            return category_matches && (!self.require_name_marker || name_marker_match);
        }
        if self.require_name_marker {
            return name_marker_match;
        }
        let normalized_types = place_types
            .iter()
            .map(|value| normalize_category_value(value))
            .collect::<HashSet<_>>();
        if self
            .accepted_place_types
            .iter()
            .map(|value| normalize_category_value(value))
            .any(|accepted| normalized_types.contains(&accepted))
        {
            return true;
        }
        if name_marker_match {
            return true;
        }
        self.allow_missing_place_types
            && normalized_types.is_empty()
            && self.category_aliases.iter().any(|alias| {
                contains_category_text(&normalized_name, &normalize_category_value(alias))
            })
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
    if !config.canonical_identity.merge_distance_m.is_finite()
        || config.canonical_identity.merge_distance_m <= 0.0
        || config
            .canonical_identity
            .identity_source_priority
            .iter()
            .any(|source| source.trim().is_empty())
    {
        return Err(DagConfigError::InvalidConfig(
            "canonical place identity policy must define a positive merge distance and non-empty source priorities"
                .to_string(),
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

fn normalize_category_value(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn normalize_category_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn contains_category_text(haystack: &str, needle: &str) -> bool {
    if needle.trim().is_empty() {
        return false;
    }
    haystack.contains(&needle.replace('_', " ")) || haystack.replace(' ', "_").contains(needle)
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
        let stormwater = config
            .categories
            .iter()
            .find(|category| category.fact_key == "stormwater_drain_nearby")
            .expect("stormwater should be a serving place category");
        assert_eq!(stormwater.collection_sources, ["openstreetmap"]);
        assert_eq!(stormwater.relation_class, "risk_externality");
        assert!(!stormwater.chainable);
    }

    #[test]
    fn query_category_lookup_is_config_driven() {
        let requested = requested_nearby_place_categories("walkable gym and brewery nearby");
        assert!(requested.contains(&"nearby_fitness"));
        assert!(requested.contains(&"nearby_breweries"));
        assert!(requested_nearby_place_categories("nearby").is_empty());
        assert!(requested_nearby_place_categories("clinic nearby").contains(&"nearby_hospitals"));
        assert!(requested_nearby_place_categories("purple line access")
            .contains(&"nearby_metro_stations"));
        assert_eq!(
            requested_nearby_place_categories("near a tech park"),
            vec!["nearby_tech_parks"]
        );
        let separate_categories =
            requested_nearby_place_categories("near a tech park and a public park");
        assert!(separate_categories.contains(&"nearby_tech_parks"));
        assert!(separate_categories.contains(&"nearby_public_parks"));
        assert_eq!(
            nearby_place_category_for_fact_key("nearby_lakes"),
            Some("nearby_lakes")
        );
    }

    #[test]
    fn place_category_matches_only_its_configured_fact_family() {
        assert!(nearby_place_fact_key_matches_category(
            "nearby_lakes",
            "water body"
        ));
        assert!(nearby_place_fact_key_matches_category(
            "nearby_metro_stations",
            "subway-station"
        ));
        assert!(nearby_place_fact_key_matches_category(
            "stormwater_drain_nearby",
            "stormwater-drain"
        ));
        assert!(!nearby_place_fact_key_matches_category(
            "nearby_public_parks",
            "lake"
        ));
    }
}
