use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDisplayMetadata {
    pub label: String,
    #[serde(default)]
    pub buyer_visible: bool,
    #[serde(default)]
    pub provenance_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDisplayRule {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub label: String,
    #[serde(default)]
    pub feedback_label: Option<String>,
    #[serde(default)]
    pub buyer_visible: bool,
    #[serde(default)]
    pub provenance_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDisplayPolicyFile {
    pub version: u32,
    pub default: SourceDisplayDefault,
    pub sources: Vec<SourceDisplayRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDisplayDefault {
    pub label: String,
    #[serde(default)]
    pub feedback_label: Option<String>,
    #[serde(default)]
    pub buyer_visible: bool,
    #[serde(default)]
    pub provenance_visible: bool,
}

pub fn source_display_policy_path() -> PathBuf {
    dag_root().join("source_display_policy.json")
}

pub fn load_source_display_policy() -> Result<SourceDisplayPolicyFile, DagConfigError> {
    load_source_display_policy_from_path(&source_display_policy_path())
}

pub fn load_source_display_policy_from_path(
    path: &Path,
) -> Result<SourceDisplayPolicyFile, DagConfigError> {
    let policy: SourceDisplayPolicyFile = load_json(path)?;
    validate_source_display_policy(&policy)?;
    Ok(policy)
}

static SOURCE_DISPLAY_POLICY: OnceLock<Result<SourceDisplayPolicyFile, String>> = OnceLock::new();

pub fn source_display_policy() -> Result<&'static SourceDisplayPolicyFile, DagConfigError> {
    match SOURCE_DISPLAY_POLICY
        .get_or_init(|| load_source_display_policy().map_err(|err| err.to_string()))
    {
        Ok(policy) => Ok(policy),
        Err(err) => Err(DagConfigError::InvalidConfig(err.clone())),
    }
}

pub fn source_display_metadata(source_type: &str) -> SourceDisplayMetadata {
    source_display_policy()
        .map(|policy| policy.display_for(source_type))
        .unwrap_or_else(|_| SourceDisplayMetadata {
            label: "Source".to_string(),
            buyer_visible: false,
            provenance_visible: false,
        })
}

pub fn source_feedback_label_for_types(source_types: &[String]) -> String {
    source_display_policy()
        .map(|policy| policy.feedback_label_for_types(source_types))
        .unwrap_or_else(|_| "Community".to_string())
}

impl SourceDisplayPolicyFile {
    fn display_for(&self, source_type: &str) -> SourceDisplayMetadata {
        self.rule_for(source_type)
            .map(|rule| SourceDisplayMetadata {
                label: rule.label.clone(),
                buyer_visible: rule.buyer_visible,
                provenance_visible: rule.provenance_visible,
            })
            .unwrap_or_else(|| SourceDisplayMetadata {
                label: self.default.label.clone(),
                buyer_visible: self.default.buyer_visible,
                provenance_visible: self.default.provenance_visible,
            })
    }

    fn feedback_label_for_types(&self, source_types: &[String]) -> String {
        let labels = source_types
            .iter()
            .filter_map(|source_type| self.rule_for(source_type))
            .map(|rule| {
                rule.feedback_label
                    .clone()
                    .unwrap_or_else(|| rule.label.clone())
            })
            .collect::<HashSet<_>>();
        if labels.contains("Google review") {
            return "Google review".to_string();
        }
        labels.into_iter().min().unwrap_or_else(|| {
            self.default
                .feedback_label
                .clone()
                .unwrap_or_else(|| self.default.label.clone())
        })
    }

    fn rule_for(&self, source_type: &str) -> Option<&SourceDisplayRule> {
        let needle = normalize_source_type(source_type);
        self.sources.iter().find(|rule| {
            normalize_source_type(&rule.id) == needle
                || rule
                    .aliases
                    .iter()
                    .any(|alias| normalize_source_type(alias) == needle)
        })
    }
}

fn validate_source_display_policy(policy: &SourceDisplayPolicyFile) -> Result<(), DagConfigError> {
    if policy.default.label.trim().is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "source_display_policy default label is blank".to_string(),
        ));
    }
    let mut ids = HashSet::new();
    let mut names = HashMap::<String, String>::new();
    for source in &policy.sources {
        if source.id.trim().is_empty() || source.label.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "source_display_policy contains an incomplete source rule".to_string(),
            ));
        }
        let normalized_id = normalize_source_type(&source.id);
        if !ids.insert(normalized_id) {
            return Err(DagConfigError::InvalidConfig(format!(
                "source_display_policy contains duplicate source id {}",
                source.id
            )));
        }
        register_source_display_name(&mut names, &source.id, &source.id)?;
        for alias in &source.aliases {
            if normalize_source_type(alias).is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "source_display_policy source {} contains a blank alias",
                    source.id
                )));
            }
            register_source_display_name(&mut names, alias, &source.id)?;
        }
    }
    Ok(())
}

fn register_source_display_name(
    names: &mut HashMap<String, String>,
    name: &str,
    source_id: &str,
) -> Result<(), DagConfigError> {
    let normalized = normalize_source_type(name);
    if let Some(existing) = names.insert(normalized, source_id.to_string()) {
        return Err(DagConfigError::InvalidConfig(format!(
            "source_display_policy source {source_id} reuses alias {name} already claimed by {existing}"
        )));
    }
    Ok(())
}

fn normalize_source_type(source_type: &str) -> String {
    source_type
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_display_policy_loads_from_dag_config() {
        let policy = load_source_display_policy().expect("source_display_policy.json should load");
        assert!(policy.display_for("Rera").buyer_visible);
        assert_eq!(policy.display_for("Google").label, "Google");
        assert!(!policy.display_for("Computed").buyer_visible);
    }

    #[test]
    fn source_display_policy_supports_config_only_visible_source() {
        let policy = SourceDisplayPolicyFile {
            version: 1,
            default: SourceDisplayDefault {
                label: "Source".to_string(),
                feedback_label: Some("Community".to_string()),
                buyer_visible: false,
                provenance_visible: false,
            },
            sources: vec![SourceDisplayRule {
                id: "new_public_source".to_string(),
                aliases: vec!["NewPublic".to_string()],
                label: "New Public".to_string(),
                feedback_label: None,
                buyer_visible: true,
                provenance_visible: true,
            }],
        };
        assert_eq!(policy.display_for("NewPublic").label, "New Public");
        assert!(policy.display_for("NewPublic").buyer_visible);
    }

    #[test]
    fn source_display_policy_rejects_alias_collisions() {
        let policy = SourceDisplayPolicyFile {
            version: 1,
            default: SourceDisplayDefault {
                label: "Source".to_string(),
                feedback_label: None,
                buyer_visible: false,
                provenance_visible: false,
            },
            sources: vec![
                SourceDisplayRule {
                    id: "public_one".to_string(),
                    aliases: vec!["Shared".to_string()],
                    label: "Public One".to_string(),
                    feedback_label: None,
                    buyer_visible: true,
                    provenance_visible: true,
                },
                SourceDisplayRule {
                    id: "public_two".to_string(),
                    aliases: vec!["shared".to_string()],
                    label: "Public Two".to_string(),
                    feedback_label: None,
                    buyer_visible: true,
                    provenance_visible: true,
                },
            ],
        };

        assert!(validate_source_display_policy(&policy).is_err());
    }
}
