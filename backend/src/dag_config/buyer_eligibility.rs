use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

const SUPPORTED_REQUIREMENTS: [&str; 6] = [
    "identity",
    "area",
    "price",
    "configuration",
    "lifecycle",
    "trusted_media",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerEligibilityFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "statusFields")]
    pub status_fields: BTreeMap<String, Vec<String>>,
    pub requirements: BTreeMap<String, BuyerEligibilityRequirement>,
    pub surfaces: BTreeMap<String, BuyerSurfacePolicy>,
    #[serde(default)]
    pub observed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyerEligibilityRequirement {
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerSurfacePolicy {
    pub required: Vec<String>,
}

pub fn buyer_eligibility_path() -> PathBuf {
    dag_root().join("buyer_eligibility.json")
}

pub fn load_buyer_eligibility() -> Result<BuyerEligibilityFile, DagConfigError> {
    load_buyer_eligibility_from_path(&buyer_eligibility_path())
}

pub fn load_buyer_eligibility_from_path(
    path: &Path,
) -> Result<BuyerEligibilityFile, DagConfigError> {
    let config: BuyerEligibilityFile = load_json(path)?;
    validate(&config)?;
    Ok(config)
}

static BUYER_ELIGIBILITY_CONFIG: OnceLock<Result<BuyerEligibilityFile, String>> = OnceLock::new();

pub fn buyer_eligibility_config() -> Result<&'static BuyerEligibilityFile, DagConfigError> {
    match BUYER_ELIGIBILITY_CONFIG
        .get_or_init(|| load_buyer_eligibility().map_err(|error| error.to_string()))
    {
        Ok(config) => Ok(config),
        Err(error) => Err(DagConfigError::InvalidConfig(error.clone())),
    }
}

fn validate(config: &BuyerEligibilityFile) -> Result<(), DagConfigError> {
    if config.version == 0 {
        return Err(DagConfigError::InvalidConfig(
            "buyer eligibility version must be positive".to_string(),
        ));
    }
    let required_surfaces = [
        "discovery",
        "search",
        "recommendations",
        "detail",
        "compare",
        "plan",
    ];
    for field in [
        "regulatory_status",
        "lifecycle_status",
        "possession_status",
        "possession_timing",
        "age_display",
        "conflict_state",
    ] {
        if !config
            .status_fields
            .get(field)
            .is_some_and(|fact_keys| !fact_keys.is_empty())
        {
            return Err(DagConfigError::InvalidConfig(format!(
                "buyer eligibility is missing status field {field}"
            )));
        }
    }
    for surface in required_surfaces {
        if !config.surfaces.contains_key(surface) {
            return Err(DagConfigError::InvalidConfig(format!(
                "buyer eligibility is missing surface {surface}"
            )));
        }
    }
    let mut reason_codes = BTreeSet::new();
    for (field, requirement) in &config.requirements {
        if field.trim().is_empty() || requirement.reason_code.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "buyer eligibility requirements need field and reason codes".to_string(),
            ));
        }
        if !reason_codes.insert(requirement.reason_code.as_str()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "duplicate buyer eligibility reason code {}",
                requirement.reason_code
            )));
        }
        if !SUPPORTED_REQUIREMENTS.contains(&field.as_str()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "buyer eligibility requirement {field} has no runtime signal"
            )));
        }
    }
    for (surface, policy) in &config.surfaces {
        if policy.required.is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "buyer eligibility surface {surface} has no requirements"
            )));
        }
        for field in &policy.required {
            if !config.requirements.contains_key(field) {
                return Err(DagConfigError::InvalidConfig(format!(
                    "buyer eligibility surface {surface} references unknown field {field}"
                )));
            }
        }
    }
    for field in &config.observed {
        if !config.requirements.contains_key(field) {
            return Err(DagConfigError::InvalidConfig(format!(
                "buyer eligibility observes unknown field {field}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buyer_eligibility_loads_all_buyer_surfaces() {
        let config = load_buyer_eligibility().expect("buyer eligibility config loads");
        for surface in [
            "discovery",
            "search",
            "recommendations",
            "detail",
            "compare",
            "plan",
        ] {
            assert!(config.surfaces.contains_key(surface));
        }
        assert!(config.observed.contains(&"trusted_media".to_string()));
    }

    #[test]
    fn buyer_eligibility_rejects_requirements_without_runtime_signals() {
        let mut config = load_buyer_eligibility().expect("buyer eligibility config loads");
        config.requirements.insert(
            "invented_signal".to_string(),
            BuyerEligibilityRequirement {
                reason_code: "missing_invented_signal".to_string(),
            },
        );
        config
            .surfaces
            .get_mut("search")
            .expect("search policy")
            .required
            .push("invented_signal".to_string());

        let error = validate(&config).expect_err("unsupported signal must fail closed");
        assert!(error.to_string().contains("has no runtime signal"));
    }
}
