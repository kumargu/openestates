use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingEligibilityFile {
    #[serde(default)]
    pub admission_profile: ServingAdmissionProfile,
    pub version: u32,
    #[serde(default = "default_minimum_projected_properties")]
    pub minimum_projected_properties: usize,
    #[serde(default = "default_missing_projection_reason_code")]
    pub missing_projection_reason_code: String,
    #[serde(default)]
    pub property_requirements: Vec<ProjectedPropertyRequirement>,
    #[serde(default)]
    pub society_requirements: Vec<SocietyEvidenceRequirement>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServingAdmissionProfile {
    #[default]
    BuyerCatalog,
    SearchExperiment,
}

fn default_minimum_projected_properties() -> usize {
    1
}

fn default_missing_projection_reason_code() -> String {
    "missing_property_projection".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedPropertyRequirement {
    pub reason_code: String,
    pub predicate: EligibilityValuePredicate,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocietyEvidenceRequirement {
    pub reason_code: String,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub related_edge_types: Vec<String>,
    #[serde(default)]
    pub related_fact_keys: Vec<String>,
    #[serde(default)]
    pub edge_presence_counts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityValuePredicate {
    AnyNonEmpty,
    AllNonEmpty,
    AnyPositive,
}

pub fn serving_eligibility_path() -> PathBuf {
    dag_root().join("serving_eligibility.json")
}

pub fn search_experiment_eligibility_path() -> PathBuf {
    dag_root().join("search_experiment_eligibility.json")
}

pub fn load_serving_eligibility_from_path(
    path: &Path,
) -> Result<ServingEligibilityFile, DagConfigError> {
    let config: ServingEligibilityFile = load_json(path)?;
    validate_serving_eligibility(&config)?;
    Ok(config)
}

pub fn load_serving_eligibility() -> Result<ServingEligibilityFile, DagConfigError> {
    load_serving_eligibility_from_path(&serving_eligibility_path())
}

pub fn load_search_experiment_eligibility() -> Result<ServingEligibilityFile, DagConfigError> {
    load_serving_eligibility_from_path(&search_experiment_eligibility_path())
}

fn validate_serving_eligibility(config: &ServingEligibilityFile) -> Result<(), DagConfigError> {
    if config.version == 0 {
        return Err(DagConfigError::InvalidConfig(
            "serving eligibility version must be positive".to_string(),
        ));
    }

    let mut reason_codes = std::collections::BTreeSet::new();
    validate_reason_code(&config.missing_projection_reason_code, &mut reason_codes)?;
    if config.minimum_projected_properties == 0 {
        return Err(DagConfigError::InvalidConfig(
            "minimum_projected_properties must be positive".to_string(),
        ));
    }
    for requirement in &config.property_requirements {
        validate_reason_code(&requirement.reason_code, &mut reason_codes)?;
        if requirement.fields.is_empty() || requirement.fields.iter().any(|field| field.is_empty())
        {
            return Err(DagConfigError::InvalidConfig(format!(
                "serving eligibility requirement {} must declare non-empty projected fields",
                requirement.reason_code
            )));
        }
    }
    for requirement in &config.society_requirements {
        validate_reason_code(&requirement.reason_code, &mut reason_codes)?;
        let has_direct_facts = !requirement.fact_keys.is_empty();
        let has_related_evidence = !requirement.related_edge_types.is_empty()
            && (requirement.edge_presence_counts || !requirement.related_fact_keys.is_empty());
        if !has_direct_facts && !has_related_evidence {
            return Err(DagConfigError::InvalidConfig(format!(
                "serving eligibility requirement {} has no evidence source",
                requirement.reason_code
            )));
        }
    }
    Ok(())
}

fn validate_reason_code(
    reason_code: &str,
    reason_codes: &mut std::collections::BTreeSet<String>,
) -> Result<(), DagConfigError> {
    if reason_code.is_empty()
        || !reason_code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(DagConfigError::InvalidConfig(format!(
            "invalid serving eligibility reason code {reason_code:?}"
        )));
    }
    if !reason_codes.insert(reason_code.to_string()) {
        return Err(DagConfigError::InvalidConfig(format!(
            "duplicate serving eligibility reason code {reason_code}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serving_eligibility_config_loads() {
        let path = serving_eligibility_path();
        if !path.exists() {
            return;
        }
        let config = load_serving_eligibility().expect("serving_eligibility.json should load");
        assert_eq!(
            config.admission_profile,
            ServingAdmissionProfile::BuyerCatalog
        );
        assert_eq!(config.version, 1);
        assert!(!config.property_requirements.is_empty());
        assert!(!config.society_requirements.is_empty());
    }

    #[test]
    fn search_experiment_eligibility_config_loads() {
        let config = load_search_experiment_eligibility()
            .expect("search_experiment_eligibility.json should load");
        assert_eq!(
            config.admission_profile,
            ServingAdmissionProfile::SearchExperiment
        );
        assert_eq!(config.version, 1);
        assert!(config.society_requirements.is_empty());
    }
}
