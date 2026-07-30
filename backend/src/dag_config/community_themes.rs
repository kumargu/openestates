use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use super::loader::{dag_root, load_json, DagConfigError};

const EMBEDDED_COMMUNITY_THEMES: &str =
    include_str!("../../../app/config/dag/community_themes.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CommunityThemesFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub themes: Vec<CommunityThemeDefinition>,
    #[serde(default)]
    pub embedding_expansions: CommunityEmbeddingExpansionConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommunityThemeDefinition {
    pub key: String,
    pub label: String,
    pub polarity: String,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub evidence_queries: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommunityEmbeddingExpansionConfig {
    #[serde(default)]
    pub token: Vec<CommunityEmbeddingExpansion>,
    #[serde(default)]
    pub phrase: Vec<CommunityEmbeddingExpansion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommunityEmbeddingExpansion {
    pub input: String,
    #[serde(default)]
    pub expanded_tokens: Vec<String>,
}

pub fn community_themes_path() -> PathBuf {
    dag_root().join("community_themes.json")
}

pub fn load_community_themes_from_path(path: &Path) -> Result<CommunityThemesFile, DagConfigError> {
    let config: CommunityThemesFile = load_json(path)?;
    validate_community_themes(&config)?;
    Ok(config)
}

pub fn load_community_themes() -> Result<CommunityThemesFile, DagConfigError> {
    load_community_themes_from_path(&community_themes_path())
}

pub fn community_themes_config() -> &'static CommunityThemesFile {
    static CONFIG: OnceLock<CommunityThemesFile> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_community_themes()
            .or_else(|_| {
                let config: CommunityThemesFile = serde_json::from_str(EMBEDDED_COMMUNITY_THEMES)?;
                validate_community_themes(&config)?;
                Ok::<CommunityThemesFile, DagConfigError>(config)
            })
            .expect("embedded community_themes.json must load and validate")
    })
}

fn validate_community_themes(config: &CommunityThemesFile) -> Result<(), DagConfigError> {
    if config.themes.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "community_themes must define at least one theme".to_string(),
        ));
    }

    let mut keys = HashSet::new();
    for theme in &config.themes {
        if theme.key.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "community_themes contains a blank theme key".to_string(),
            ));
        }
        if !keys.insert(theme.key.to_ascii_lowercase()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "community_themes contains duplicate theme key {}",
                theme.key
            )));
        }
        if !matches!(theme.polarity.as_str(), "positive" | "concern") {
            return Err(DagConfigError::InvalidConfig(format!(
                "community theme {} has unsupported polarity {}",
                theme.key, theme.polarity
            )));
        }
        if theme.terms.is_empty() || theme.evidence_queries.is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "community theme {} must define terms and evidence_queries",
                theme.key
            )));
        }
    }

    validate_expansions("token", &config.embedding_expansions.token)?;
    validate_expansions("phrase", &config.embedding_expansions.phrase)?;
    Ok(())
}

fn validate_expansions(
    group: &str,
    expansions: &[CommunityEmbeddingExpansion],
) -> Result<(), DagConfigError> {
    let mut inputs = HashSet::new();
    for expansion in expansions {
        if expansion.input.trim().is_empty() || expansion.expanded_tokens.is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "community {group} expansion must define input and expanded_tokens"
            )));
        }
        if !inputs.insert(expansion.input.to_ascii_lowercase()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "community {group} expansion duplicates {}",
                expansion.input
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_themes_load_from_dag_config() {
        let config = load_community_themes().expect("community themes should load");
        assert!(config.themes.iter().any(|theme| theme.key == "greenery"));
        assert!(config
            .embedding_expansions
            .phrase
            .iter()
            .any(|expansion| expansion.input == "tech park"));
    }
}
