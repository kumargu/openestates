use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSectionDefinition {
    pub kind: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub constellation: String,
    #[serde(default)]
    pub surfaces: Vec<String>,
    pub title: String,
    pub subtitle: String,
    pub scope: String,
    pub relationship: String,
    #[serde(default)]
    pub derived: Option<String>,
    #[serde(default)]
    pub presentation: Option<EvidenceSectionPresentation>,
    #[serde(default)]
    pub media: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub facts: Vec<ContextFactDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFactDefinition {
    pub key: String,
    pub label: String,
    pub scope: String,
    pub relationship: String,
    #[serde(default)]
    pub max_values: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSectionPresentation {
    pub variant: String,
    pub density: String,
    pub max_preview_items: usize,
}

pub fn evidence_sections_path() -> PathBuf {
    dag_root().join("evidence_sections.json")
}

pub fn load_evidence_sections() -> Result<Vec<EvidenceSectionDefinition>, DagConfigError> {
    load_evidence_sections_from_path(&evidence_sections_path())
}

pub fn load_evidence_sections_from_path(
    path: &Path,
) -> Result<Vec<EvidenceSectionDefinition>, DagConfigError> {
    let config: Vec<EvidenceSectionDefinition> = load_json(path)?;
    validate_evidence_sections(&config)?;
    Ok(config)
}

static EVIDENCE_SECTIONS_CONFIG: OnceLock<Result<Vec<EvidenceSectionDefinition>, String>> =
    OnceLock::new();

pub fn evidence_sections_config() -> Result<&'static [EvidenceSectionDefinition], DagConfigError> {
    match EVIDENCE_SECTIONS_CONFIG
        .get_or_init(|| load_evidence_sections().map_err(|err| err.to_string()))
    {
        Ok(config) => Ok(config.as_slice()),
        Err(err) => Err(DagConfigError::InvalidConfig(err.clone())),
    }
}

fn validate_evidence_sections(config: &[EvidenceSectionDefinition]) -> Result<(), DagConfigError> {
    if config.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "evidence_sections.json must define at least one section".to_string(),
        ));
    }

    let mut kinds = HashSet::new();
    for section in config {
        let kind = section.kind.trim();
        if kind.is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "evidence_sections.json contains a blank section kind".to_string(),
            ));
        }
        if !kinds.insert(kind.to_string()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "evidence_sections.json contains duplicate section kind {kind}"
            )));
        }
        if section.title.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "evidence section {kind} has a blank title"
            )));
        }
        if section.scope.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "evidence section {kind} has a blank scope"
            )));
        }
        if let Some(derived) = section.derived.as_deref() {
            if !matches!(derived, "community_pulse") {
                return Err(DagConfigError::InvalidConfig(format!(
                    "evidence section {kind} has unsupported derived mode {derived}"
                )));
            }
        }
        if let Some(presentation) = section.presentation.as_ref() {
            if presentation.variant.trim().is_empty() || presentation.density.trim().is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "evidence section {kind} has an incomplete presentation"
                )));
            }
            if presentation.max_preview_items == 0 {
                return Err(DagConfigError::InvalidConfig(format!(
                    "evidence section {kind} max_preview_items must be greater than zero"
                )));
            }
        }
        for fact in &section.facts {
            if fact.key.trim().is_empty()
                || fact.label.trim().is_empty()
                || fact.scope.trim().is_empty()
                || fact.relationship.trim().is_empty()
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "evidence section {kind} contains an incomplete fact definition"
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
    fn evidence_sections_load_from_dag_config() {
        let sections = load_evidence_sections().expect("DAG evidence_sections.json should load");
        assert!(sections.iter().any(|section| section.kind == "rera"));
        assert!(sections.iter().any(|section| {
            section.kind == "community" && section.derived.as_deref() == Some("community_pulse")
        }));
    }

    #[test]
    fn evidence_sections_reject_duplicate_kinds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("evidence_sections.json");
        std::fs::write(
            &path,
            r#"[
              {
                "kind": "market",
                "title": "Market",
                "subtitle": "Market facts",
                "scope": "society",
                "relationship": "market",
                "facts": [{"key": "price_per_sqft", "label": "Price", "scope": "society", "relationship": "market"}]
              },
              {
                "kind": "market",
                "title": "Duplicate",
                "subtitle": "Duplicate facts",
                "scope": "society",
                "relationship": "market",
                "facts": [{"key": "price_per_sqft", "label": "Price", "scope": "society", "relationship": "market"}]
              }
            ]"#,
        )
        .expect("write config");

        let err = load_evidence_sections_from_path(&path).expect_err("duplicate kind should fail");
        assert!(err.to_string().contains("duplicate section kind market"));
    }
}
