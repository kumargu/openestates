use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraReportSurfaceFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub coverage_note: String,
    pub regulatory_event_order: Vec<String>,
    #[serde(default)]
    pub candidate_rules: ReraReportCandidateRules,
    #[serde(default)]
    pub value_rules: ReraReportValueRules,
    pub sections: Vec<ReraReportSectionRule>,
    #[serde(default)]
    pub display_rules: Vec<ReraReportDisplayRule>,
    #[serde(default)]
    pub tone_rules: Vec<ReraReportToneRule>,
    #[serde(default)]
    pub notebook_label_rules: Vec<ReraReportNotebookLabelRule>,
    #[serde(default)]
    pub document_group_labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportCandidateRules {
    #[serde(default)]
    pub include_source_types: Vec<String>,
    #[serde(default)]
    pub include_fact_keys: Vec<String>,
    #[serde(default)]
    pub include_key_prefixes: Vec<String>,
    #[serde(default)]
    pub exclude_key_contains: Vec<String>,
    #[serde(default)]
    pub exclude_key_suffixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportValueRules {
    #[serde(default = "default_skip_json_containers")]
    pub skip_json_containers: bool,
    #[serde(default = "default_text_skip_values")]
    pub skip_text_values: Vec<String>,
    #[serde(default)]
    pub numeric_units: Vec<ReraReportNumericUnitRule>,
}

impl Default for ReraReportValueRules {
    fn default() -> Self {
        Self {
            skip_json_containers: default_skip_json_containers(),
            skip_text_values: default_text_skip_values(),
            numeric_units: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportNumericUnitRule {
    #[serde(default)]
    pub key_suffixes: Vec<String>,
    #[serde(default)]
    pub key_contains: Vec<String>,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportSectionRule {
    pub id: String,
    pub title: String,
    pub rank: u32,
    #[serde(default = "default_rera_renderer")]
    pub renderer: String,
    #[serde(default)]
    pub selectors: Vec<ReraReportSelectorRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,
    #[serde(default)]
    pub preview_kinds: Vec<String>,
    #[serde(default = "default_empty_behavior")]
    pub empty_behavior: String,
    #[serde(default)]
    pub key_contains: Vec<String>,
    #[serde(default)]
    pub key_prefixes: Vec<String>,
    #[serde(default)]
    pub key_suffixes: Vec<String>,
    #[serde(default)]
    pub fact_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportSelectorRule {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

fn default_rera_renderer() -> String {
    "fact_list".to_string()
}

fn default_empty_behavior() -> String {
    "omit".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportDisplayRule {
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub key_contains: Vec<String>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraReportToneRule {
    pub tone: String,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub key_contains: Vec<String>,
    #[serde(default)]
    pub key_prefixes: Vec<String>,
    #[serde(default)]
    pub when: ReraReportToneCondition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReraReportToneCondition {
    #[serde(default)]
    pub bool_value: Option<bool>,
    #[serde(default)]
    pub numeric_gt: Option<f64>,
    #[serde(default)]
    pub numeric_lte: Option<f64>,
    #[serde(default)]
    pub present: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraReportNotebookLabelRule {
    pub labels: Vec<String>,
    #[serde(default)]
    pub section_ids: Vec<String>,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub key_contains: Vec<String>,
}

pub fn rera_report_surface_path() -> PathBuf {
    dag_root().join("rera_report_surface.json")
}

pub fn load_rera_report_surface() -> Result<ReraReportSurfaceFile, DagConfigError> {
    load_rera_report_surface_from_path(&rera_report_surface_path())
}

pub fn load_rera_report_surface_from_path(
    path: &Path,
) -> Result<ReraReportSurfaceFile, DagConfigError> {
    let config: ReraReportSurfaceFile = load_json(path)?;
    validate_rera_report_surface(&config)?;
    Ok(config)
}

static RERA_REPORT_SURFACE_CONFIG: OnceLock<Result<ReraReportSurfaceFile, String>> =
    OnceLock::new();

pub fn rera_report_surface_config() -> Result<&'static ReraReportSurfaceFile, DagConfigError> {
    match RERA_REPORT_SURFACE_CONFIG
        .get_or_init(|| load_rera_report_surface().map_err(|err| err.to_string()))
    {
        Ok(config) => Ok(config),
        Err(err) => Err(DagConfigError::InvalidConfig(err.clone())),
    }
}

fn default_skip_json_containers() -> bool {
    true
}

fn default_text_skip_values() -> Vec<String> {
    ["unknown", "not specified", "n/a", "na", "none", "null"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn validate_rera_report_surface(config: &ReraReportSurfaceFile) -> Result<(), DagConfigError> {
    if config.version == 0 {
        return Err(DagConfigError::InvalidConfig(
            "rera_report_surface.json version must be greater than zero".to_string(),
        ));
    }
    if config.sections.is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "rera_report_surface.json must define at least one section".to_string(),
        ));
    }
    if config.coverage_note.trim().is_empty() {
        return Err(DagConfigError::InvalidConfig(
            "rera_report_surface.json coverage_note must be non-blank".to_string(),
        ));
    }
    if config.regulatory_event_order.is_empty()
        || config
            .regulatory_event_order
            .iter()
            .any(|value| value.trim().is_empty())
    {
        return Err(DagConfigError::InvalidConfig(
            "rera_report_surface.json regulatory_event_order must be non-empty".to_string(),
        ));
    }
    let mut ids = HashSet::new();
    for section in &config.sections {
        validate_section_identity(&section.id, &section.title, "section")?;
        if !ids.insert(section.id.clone()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "rera_report_surface.json contains duplicate section {}",
                section.id
            )));
        }
        if !has_any_matcher(
            &section.fact_keys,
            &section.key_prefixes,
            &section.key_contains,
            &section.key_suffixes,
        ) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report section {} must define at least one matcher",
                section.id
            )));
        }
        if section.selectors.is_empty()
            || section
                .selectors
                .iter()
                .any(|selector| selector.key.trim().is_empty() || selector.label.trim().is_empty())
        {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report section {} must define non-blank selectors",
                section.id
            )));
        }
        if section.items_per_page == Some(0) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report section {} item counts must be greater than zero",
                section.id
            )));
        }
        if !matches!(
            section.renderer.as_str(),
            "fact_list"
                | "timeline"
                | "series"
                | "table"
                | "dual_bar_chart"
                | "documents"
                | "regulatory_record"
        ) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report section {} has unsupported renderer {}",
                section.id, section.renderer
            )));
        }
        if section.empty_behavior != "omit" {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report section {} must omit empty evidence",
                section.id
            )));
        }
    }

    for rule in &config.display_rules {
        if rule.label.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "rera_report_surface.json contains a blank display label".to_string(),
            ));
        }
        if !has_any_matcher(&rule.fact_keys, &[], &rule.key_contains, &[]) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report display rule {} must define fact_keys or key_contains",
                rule.label
            )));
        }
    }

    for rule in &config.tone_rules {
        if rule.tone.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "rera_report_surface.json contains a blank tone".to_string(),
            ));
        }
        if !has_any_matcher(&rule.fact_keys, &rule.key_prefixes, &rule.key_contains, &[]) {
            return Err(DagConfigError::InvalidConfig(format!(
                "RERA report tone rule {} must define a matcher",
                rule.tone
            )));
        }
    }

    if config
        .document_group_labels
        .iter()
        .any(|(key, label)| key.trim().is_empty() || label.trim().is_empty())
    {
        return Err(DagConfigError::InvalidConfig(
            "rera_report_surface.json document group labels must be non-blank".to_string(),
        ));
    }

    Ok(())
}

fn validate_section_identity(id: &str, title: &str, context: &str) -> Result<(), DagConfigError> {
    if id.trim().is_empty() || title.trim().is_empty() {
        return Err(DagConfigError::InvalidConfig(format!(
            "rera_report_surface.json {context} must define id and title"
        )));
    }
    Ok(())
}

fn has_any_matcher(
    fact_keys: &[String],
    key_prefixes: &[String],
    key_contains: &[String],
    key_suffixes: &[String],
) -> bool {
    [&fact_keys, &key_prefixes, &key_contains, &key_suffixes]
        .into_iter()
        .any(|values| values.iter().any(|value| !value.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rera_report_surface_loads_from_dag_config() {
        let config = load_rera_report_surface().expect("RERA report surface config should load");
        assert!(config
            .sections
            .iter()
            .any(|section| section.id == "regulatory_record"));
        assert!(config
            .sections
            .iter()
            .all(|section| { !section.selectors.is_empty() && section.empty_behavior == "omit" }));
    }
}
