use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraDecisionLabelsFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub summary: ReraDecisionLabelSummaryConfig,
    pub labels: Vec<ReraDecisionLabelDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraDecisionLabelSummaryConfig {
    #[serde(default = "default_tile_label")]
    pub tile_label: String,
    #[serde(default = "default_primary_limit")]
    pub primary_limit: usize,
    #[serde(default = "default_groups")]
    pub groups: Vec<ReraDecisionLabelGroupDefinition>,
}

impl Default for ReraDecisionLabelSummaryConfig {
    fn default() -> Self {
        Self {
            tile_label: default_tile_label(),
            primary_limit: default_primary_limit(),
            groups: default_groups(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraDecisionLabelGroupDefinition {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraDecisionLabelDefinition {
    pub key: String,
    pub scope: String,
    pub source: ReraDecisionLabelSource,
    #[serde(default)]
    pub condition: ReraDecisionLabelCondition,
    pub severity: String,
    pub label_template: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub surfaces: Vec<String>,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub value_precision: Option<u8>,
    #[serde(default)]
    pub visual_id: Option<String>,
    #[serde(default)]
    pub notebook_labels: Vec<String>,
    #[serde(default)]
    pub compare_group: Option<String>,
    #[serde(default = "default_group_id")]
    pub group_id: String,
    #[serde(default = "default_placement")]
    pub placement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReraDecisionLabelSource {
    Fact {
        fact_key: String,
    },
    FactAny {
        fact_keys: Vec<String>,
    },
    Ratio {
        numerator_fact_key: String,
        denominator_fact_key: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReraDecisionLabelCondition {
    #[serde(default)]
    pub gt: Option<f64>,
    #[serde(default)]
    pub gte: Option<f64>,
    #[serde(default)]
    pub lt: Option<f64>,
    #[serde(default)]
    pub lte: Option<f64>,
    #[serde(default)]
    pub eq_bool: Option<bool>,
    #[serde(default)]
    pub present: Option<bool>,
}

pub fn rera_decision_labels_path() -> PathBuf {
    dag_root().join("rera_decision_labels.json")
}

pub fn load_rera_decision_labels() -> Result<ReraDecisionLabelsFile, DagConfigError> {
    load_rera_decision_labels_from_path(&rera_decision_labels_path())
}

pub fn load_rera_decision_labels_from_path(
    path: &Path,
) -> Result<ReraDecisionLabelsFile, DagConfigError> {
    let config: ReraDecisionLabelsFile = load_json(path)?;
    validate_rera_decision_labels(&config)?;
    Ok(config)
}

static RERA_DECISION_LABELS_CONFIG: OnceLock<Result<ReraDecisionLabelsFile, String>> =
    OnceLock::new();

pub fn rera_decision_labels_config() -> Result<&'static ReraDecisionLabelsFile, DagConfigError> {
    match RERA_DECISION_LABELS_CONFIG
        .get_or_init(|| load_rera_decision_labels().map_err(|err| err.to_string()))
    {
        Ok(config) => Ok(config),
        Err(err) => Err(DagConfigError::InvalidConfig(err.clone())),
    }
}

fn default_tile_label() -> String {
    "RERA".to_string()
}

fn default_primary_limit() -> usize {
    4
}

fn default_groups() -> Vec<ReraDecisionLabelGroupDefinition> {
    vec![
        ReraDecisionLabelGroupDefinition {
            id: "attention".to_string(),
            title: "Cautions".to_string(),
        },
        ReraDecisionLabelGroupDefinition {
            id: "project_facts".to_string(),
            title: "Project facts".to_string(),
        },
        ReraDecisionLabelGroupDefinition {
            id: "documents".to_string(),
            title: "Documents".to_string(),
        },
        ReraDecisionLabelGroupDefinition {
            id: "finance".to_string(),
            title: "Finance".to_string(),
        },
    ]
}

fn default_group_id() -> String {
    "project_facts".to_string()
}

fn default_placement() -> String {
    "more".to_string()
}

fn validate_rera_decision_labels(config: &ReraDecisionLabelsFile) -> Result<(), DagConfigError> {
    if config.version == 0 {
        return Err(DagConfigError::InvalidConfig(
            "rera_decision_labels.json version must be greater than zero".to_string(),
        ));
    }
    if config.labels.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "rera_decision_labels.json must define at least one label".to_string(),
        ));
    }
    if config.summary.tile_label.trim().is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "rera_decision_labels.json summary.tile_label must not be blank".to_string(),
        ));
    }
    if config.summary.primary_limit == 0 {
        return Err(DagConfigError::InvalidConfig(
            "rera_decision_labels.json summary.primary_limit must be greater than zero".to_string(),
        ));
    }

    let mut keys = HashSet::new();
    let mut group_ids = HashSet::new();
    for group in &config.summary.groups {
        if group.id.trim().is_empty() || group.title.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "rera_decision_labels.json summary groups must define id and title".to_string(),
            ));
        }
        if !group_ids.insert(group.id.clone()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "rera_decision_labels.json contains duplicate summary group {}",
                group.id
            )));
        }
    }
    for label in &config.labels {
        let key = label.key.trim();
        if key.is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "rera_decision_labels.json contains a blank label key".to_string(),
            ));
        }
        if !keys.insert(key.to_string()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "rera_decision_labels.json contains duplicate label key {key}"
            )));
        }
        if label.scope.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has a blank scope"
            )));
        }
        if label.severity.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has a blank severity"
            )));
        }
        if label.label_template.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has a blank label_template"
            )));
        }
        if !group_ids.contains(&label.group_id) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} references unknown group_id {}",
                label.group_id
            )));
        }
        if !matches!(label.placement.as_str(), "primary" | "more" | "audit") {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has unsupported placement {}",
                label.placement
            )));
        }
        validate_source(key, &label.source)?;
        validate_condition(key, &label.condition)?;
    }

    Ok(())
}

fn validate_source(key: &str, source: &ReraDecisionLabelSource) -> Result<(), DagConfigError> {
    match source {
        ReraDecisionLabelSource::Fact { fact_key } if fact_key.trim().is_empty() => {
            Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has a blank source fact_key"
            )))
        }
        ReraDecisionLabelSource::FactAny { fact_keys }
            if fact_keys.is_empty()
                || fact_keys.iter().any(|fact_key| fact_key.trim().is_empty()) =>
        {
            Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has an incomplete fact_any source"
            )))
        }
        ReraDecisionLabelSource::Ratio {
            numerator_fact_key,
            denominator_fact_key,
        } if numerator_fact_key.trim().is_empty() || denominator_fact_key.trim().is_empty() => {
            Err(DagConfigError::InvalidConfig(format!(
                "RERA decision label {key} has an incomplete ratio source"
            )))
        }
        _ => Ok(()),
    }
}

fn validate_condition(
    key: &str,
    condition: &ReraDecisionLabelCondition,
) -> Result<(), DagConfigError> {
    if condition.eq_bool.is_some()
        && [condition.gt, condition.gte, condition.lt, condition.lte]
            .iter()
            .any(Option::is_some)
    {
        return Err(DagConfigError::InvalidConfig(format!(
            "RERA decision label {key} mixes bool and numeric conditions"
        )));
    }
    if let (Some(gt), Some(gte)) = (condition.gt, condition.gte) {
        return Err(DagConfigError::InvalidConfig(format!(
            "RERA decision label {key} defines both gt ({gt}) and gte ({gte})"
        )));
    }
    if let (Some(lt), Some(lte)) = (condition.lt, condition.lte) {
        return Err(DagConfigError::InvalidConfig(format!(
            "RERA decision label {key} defines both lt ({lt}) and lte ({lte})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rera_decision_labels_load_from_dag_config() {
        let config = load_rera_decision_labels().expect("RERA label config should load");
        assert!(config
            .labels
            .iter()
            .any(|label| label.key == "rera_land_litigation"));
        assert!(config
            .labels
            .iter()
            .any(|label| label.key == "low_parking_coverage"));
    }

    #[test]
    fn rera_decision_labels_reject_duplicate_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rera_decision_labels.json");
        std::fs::write(
            &path,
            r#"{
              "version": 1,
              "labels": [
                {
                  "key": "duplicate",
                  "scope": "project",
                  "source": {"type": "fact", "fact_key": "rera_delay_months"},
                  "condition": {"gte": 7},
                  "severity": "caution",
                  "label_template": "{value} month delay"
                },
                {
                  "key": "duplicate",
                  "scope": "project",
                  "source": {"type": "fact", "fact_key": "rera_complaints_count"},
                  "condition": {"gte": 3},
                  "severity": "caution",
                  "label_template": "{value} complaints"
                }
              ]
            }"#,
        )
        .expect("write config");

        let err = load_rera_decision_labels_from_path(&path)
            .expect_err("duplicate label key should fail");
        assert!(err.to_string().contains("duplicate label key duplicate"));
    }
}
