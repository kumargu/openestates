use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::assets::{AssetDefinition, AssetRegistry, RegistryError};

const DEFAULT_DAG_ROOT: &str = "app/config/dag";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DagManifest {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRegistryFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub assets: Vec<AssetDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlPolicyFile {
    pub policy_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub skip_rules: Vec<CrawlSkipRule>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlSkipRule {
    #[serde(rename = "if")]
    pub condition: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Debug)]
pub enum DagConfigError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    Registry(RegistryError),
}

impl std::fmt::Display for DagConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read DAG config: {err}"),
            Self::Parse(err) => write!(f, "failed to parse DAG config: {err}"),
            Self::Registry(err) => write!(f, "invalid asset registry config: {err}"),
        }
    }
}

impl std::error::Error for DagConfigError {}

impl From<std::io::Error> for DagConfigError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for DagConfigError {
    fn from(value: serde_json::Error) -> Self {
        Self::Parse(value)
    }
}

impl From<RegistryError> for DagConfigError {
    fn from(value: RegistryError) -> Self {
        Self::Registry(value)
    }
}

static PROJECT_DAG_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Pin DAG config to the repo root so search and loaders work regardless of process cwd.
pub fn set_project_dag_root(project_root: &Path) {
    let _ = PROJECT_DAG_ROOT.set(project_root.join("app/config/dag"));
}

pub fn dag_root() -> PathBuf {
    if let Some(root) = PROJECT_DAG_ROOT.get() {
        return root.clone();
    }

    if let Ok(root) = std::env::var("OPENESTATES_DAG_CONFIG_ROOT") {
        return PathBuf::from(root);
    }

    for candidate in [
        PathBuf::from("app/config/dag"),
        PathBuf::from("../app/config/dag"),
        PathBuf::from("data/dag"),
        PathBuf::from("../data/dag"),
    ] {
        if candidate.join("manifest.json").exists() {
            return candidate;
        }
    }

    PathBuf::from(DEFAULT_DAG_ROOT)
}

pub fn asset_registry_path() -> PathBuf {
    dag_root().join("asset_registry.json")
}

pub fn crawl_policy_path(policy_id: &str) -> PathBuf {
    dag_root()
        .join("crawl_policies")
        .join(format!("{policy_id}.json"))
}

pub fn load_manifest() -> Result<DagManifest, DagConfigError> {
    load_json(&dag_root().join("manifest.json"))
}

pub fn load_asset_registry_from_path(path: &Path) -> Result<AssetRegistry, DagConfigError> {
    let file: AssetRegistryFile = load_json(path)?;
    Ok(AssetRegistry::new(file.assets)?)
}

pub fn load_asset_registry() -> Result<AssetRegistry, DagConfigError> {
    load_asset_registry_from_path(&asset_registry_path())
}

pub fn load_crawl_policy(policy_id: &str) -> Result<Option<CrawlPolicyFile>, DagConfigError> {
    let path = crawl_policy_path(policy_id);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(load_json(&path)?))
}

pub fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, DagConfigError> {
    let contents = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_loads_from_repo_default() {
        let manifest = load_manifest().expect("manifest.json should load");
        assert_eq!(manifest.version, 1);
        assert!(manifest.includes.contains(&"ontology.json".to_string()));
    }

    #[test]
    fn asset_registry_json_matches_embedded_topological_order() {
        let path = asset_registry_path();
        if !path.exists() {
            return;
        }

        let embedded = crate::assets::default_openestates_registry();
        let from_json = load_asset_registry_from_path(&path).expect("asset_registry.json loads");

        assert_eq!(
            embedded.topological_order().unwrap(),
            from_json.topological_order().unwrap()
        );
    }
}
