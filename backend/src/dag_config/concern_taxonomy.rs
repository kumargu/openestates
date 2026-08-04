use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcernTaxonomyFile {
    pub version: u32,
    #[serde(default)]
    pub buckets: Vec<ConcernTaxonomyBucket>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcernTaxonomyBucket {
    pub id: String,
    #[serde(default)]
    pub leaves: Vec<ConcernTaxonomyLeaf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcernTaxonomyLeaf {
    pub fact_key: String,
    pub label: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub preferences: Vec<String>,
}

pub fn concern_taxonomy_path() -> PathBuf {
    dag_root().join("concern_taxonomy.json")
}

pub fn load_concern_taxonomy() -> Result<ConcernTaxonomyFile, DagConfigError> {
    load_concern_taxonomy_from_path(&concern_taxonomy_path())
}

pub fn load_concern_taxonomy_from_path(path: &Path) -> Result<ConcernTaxonomyFile, DagConfigError> {
    let taxonomy: ConcernTaxonomyFile = load_json(path)?;
    validate_concern_taxonomy(&taxonomy)?;
    Ok(taxonomy)
}

fn validate_concern_taxonomy(taxonomy: &ConcernTaxonomyFile) -> Result<(), DagConfigError> {
    if taxonomy.buckets.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "concern_taxonomy must define at least one bucket".to_string(),
        ));
    }
    let mut bucket_ids = HashSet::new();
    let mut fact_keys = HashSet::new();
    for bucket in &taxonomy.buckets {
        if bucket.id.trim().is_empty() || !bucket_ids.insert(bucket.id.trim().to_ascii_lowercase())
        {
            return Err(DagConfigError::InvalidConfig(
                "concern_taxonomy bucket ids must be non-empty and unique".to_string(),
            ));
        }
        for leaf in &bucket.leaves {
            let fact_key = leaf.fact_key.trim().to_ascii_lowercase();
            if fact_key.is_empty() || !fact_keys.insert(fact_key) {
                return Err(DagConfigError::InvalidConfig(
                    "concern_taxonomy fact keys must be non-empty and unique".to_string(),
                ));
            }
            if leaf.label.trim().is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "concern_taxonomy leaf {} has an empty label",
                    leaf.fact_key
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concern_taxonomy_loads_unique_leaf_ids() {
        let taxonomy = load_concern_taxonomy().expect("concern taxonomy should load");
        let leaf_count: usize = taxonomy
            .buckets
            .iter()
            .map(|bucket| bucket.leaves.len())
            .sum();
        assert!(leaf_count >= 80);
    }
}
