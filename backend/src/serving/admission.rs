//! Admission rules shared by runtime evaluation and serving validation.
use std::sync::OnceLock;

use super::{ServingEdgeRecord, ServingFactRecord};
use crate::dag_config::{load_resolution_policies, ResolutionPoliciesFile};

pub(crate) fn policies() -> &'static ResolutionPoliciesFile {
    static POLICIES: OnceLock<ResolutionPoliciesFile> = OnceLock::new();
    POLICIES.get_or_init(|| {
        load_resolution_policies().expect("evidence admission policy must validate")
    })
}

pub(crate) fn inventory_fact_key(key: &str) -> bool {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            regex::Regex::new(&policies().inventory_evidence.fact_key_pattern)
                .expect("inventory fact pattern must validate")
        })
        .is_match(key)
}

pub(crate) fn admit_inventory_fact(fact: &ServingFactRecord) -> Result<(), &'static str> {
    if !inventory_fact_key(&fact.fact_key) {
        return Err("not_inventory_evidence");
    }
    if !policies()
        .inventory_evidence
        .source
        .admits(&fact.source_type, fact.confidence, policies())
    {
        return Err("ineligible_inventory_source_or_confidence");
    }
    if fact.observation.is_none() || fact.validate_observation().is_err() {
        return Err("missing_eligible_observation");
    }
    Ok(())
}

pub(crate) fn admit_identity_edge(edge: &ServingEdgeRecord) -> bool {
    policies()
        .identity_evidence
        .admits(&edge.source_type, edge.confidence, policies())
}
