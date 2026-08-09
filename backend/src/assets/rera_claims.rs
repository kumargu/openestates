//! Canonical, source-qualified claims for the RERA evidence graph.
//!
//! Claims are not buyer labels or conclusions. They are immutable assertions
//! whose identity includes their source evidence (or, for derivations, every
//! input claim and the versioned rule that produced them).

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimSubject {
    pub entity_id: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReraClaimValue {
    Boolean(bool),
    Number(f64),
    Text(String),
    Date(String),
    Money {
        amount: String,
        currency: String,
    },
    DocumentRef(String),
    EntityRef {
        entity_id: String,
        entity_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimEffectiveTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub precision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraAssertionMode {
    RegistryRecord,
    PromoterDeclaration,
    ComplainantAllegation,
    AuthorityOrder,
    SystemDerivation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraSourceTrust {
    PrimaryAuthority,
    PromoterFiling,
    PartyFiling,
    LicensedDataset,
    Derived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraClaimValidationState {
    Accepted,
    Quarantined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReraClaimVisibility {
    Public,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimEvidence {
    pub source_record_id: String,
    pub receipt_id: String,
    pub capture_id: String,
    pub locator: String,
    pub parser_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimDerivation {
    pub rule_id: String,
    pub rule_version: String,
    pub input_claim_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraClaimV1 {
    pub claim_id: String,
    pub subject: ReraClaimSubject,
    pub predicate: String,
    pub value: ReraClaimValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_time: Option<ReraClaimEffectiveTime>,
    pub assertion_mode: ReraAssertionMode,
    pub source_trust: ReraSourceTrust,
    pub extraction_confidence: f64,
    pub validation_state: ReraClaimValidationState,
    pub visibility: ReraClaimVisibility,
    pub evidence: Vec<ReraClaimEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation: Option<ReraClaimDerivation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraClaimInput {
    pub subject: ReraClaimSubject,
    pub predicate: String,
    pub value: ReraClaimValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_time: Option<ReraClaimEffectiveTime>,
    pub assertion_mode: ReraAssertionMode,
    pub source_trust: ReraSourceTrust,
    pub extraction_confidence: f64,
    pub validation_state: ReraClaimValidationState,
    pub visibility: ReraClaimVisibility,
    #[serde(default)]
    pub evidence: Vec<ReraClaimEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation: Option<ReraClaimDerivation>,
}

impl ReraClaimInput {
    /// Construct a source assertion. A source claim always has exactly one
    /// source-record assertion so identical values from separate filings stay
    /// separate and traceable.
    pub fn into_source_claim(self) -> Result<ReraClaimV1, ReraClaimError> {
        validate_common(&self)?;
        if self.assertion_mode == ReraAssertionMode::SystemDerivation
            || self.source_trust == ReraSourceTrust::Derived
        {
            return Err(ReraClaimError::InvalidSourceAssertion);
        }
        if self.derivation.is_some() {
            return Err(ReraClaimError::UnexpectedDerivation);
        }
        if self.evidence.len() != 1 {
            return Err(ReraClaimError::SourceEvidenceCount(self.evidence.len()));
        }
        let evidence = &self.evidence[0];
        validate_evidence(evidence)?;
        let material = format!(
            "rera_claim.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.subject.entity_type.trim(),
            self.subject.entity_id.trim(),
            self.predicate.trim(),
            canonical_json(&self.value)?,
            canonical_effective_time(self.effective_time.as_ref())?,
            assertion_mode_name(self.assertion_mode),
            evidence.receipt_id.trim(),
            evidence.locator.trim(),
            evidence.capture_id.trim(),
        );
        Ok(ReraClaimV1 {
            claim_id: format!("rera_claim:sha256:{}", sha256_hex(material.as_bytes())),
            subject: normalized_subject(&self.subject),
            predicate: self.predicate.trim().to_string(),
            value: self.value,
            unit: normalized_optional(self.unit),
            effective_time: self.effective_time,
            assertion_mode: self.assertion_mode,
            source_trust: self.source_trust,
            extraction_confidence: self.extraction_confidence,
            validation_state: self.validation_state,
            visibility: self.visibility,
            evidence: self.evidence,
            derivation: None,
        })
    }

    /// Construct a deterministic computation over already accepted claims.
    pub fn into_derived_claim(self) -> Result<ReraClaimV1, ReraClaimError> {
        validate_common(&self)?;
        if self.assertion_mode != ReraAssertionMode::SystemDerivation
            || self.source_trust != ReraSourceTrust::Derived
        {
            return Err(ReraClaimError::InvalidDerivedAssertion);
        }
        if !self.evidence.is_empty() {
            return Err(ReraClaimError::DerivedEvidenceMustBeEmpty);
        }
        let mut derivation = self.derivation.ok_or(ReraClaimError::MissingDerivation)?;
        if derivation.rule_id.trim().is_empty() || derivation.rule_version.trim().is_empty() {
            return Err(ReraClaimError::BlankDerivationRule);
        }
        let inputs = derivation
            .input_claim_ids
            .iter()
            .map(|id| id.trim().to_string())
            .collect::<Vec<_>>();
        if inputs.is_empty() || inputs.iter().any(String::is_empty) {
            return Err(ReraClaimError::MissingDerivationInputs);
        }
        let unique = inputs.iter().collect::<BTreeSet<_>>();
        if unique.len() != inputs.len() {
            return Err(ReraClaimError::DuplicateDerivationInputs);
        }
        derivation.input_claim_ids = inputs;
        derivation.input_claim_ids.sort();
        let material = format!(
            "rera_derived_claim.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            derivation.rule_id.trim(),
            derivation.rule_version.trim(),
            derivation.input_claim_ids.join("\n"),
            self.subject.entity_type.trim(),
            self.subject.entity_id.trim(),
            self.predicate.trim(),
            canonical_json(&self.value)?,
            canonical_effective_time(self.effective_time.as_ref())?,
        );
        Ok(ReraClaimV1 {
            claim_id: format!("rera_claim:sha256:{}", sha256_hex(material.as_bytes())),
            subject: normalized_subject(&self.subject),
            predicate: self.predicate.trim().to_string(),
            value: self.value,
            unit: normalized_optional(self.unit),
            effective_time: self.effective_time,
            assertion_mode: self.assertion_mode,
            source_trust: self.source_trust,
            extraction_confidence: self.extraction_confidence,
            validation_state: self.validation_state,
            visibility: self.visibility,
            evidence: Vec::new(),
            derivation: Some(derivation),
        })
    }
}

fn validate_common(input: &ReraClaimInput) -> Result<(), ReraClaimError> {
    if input.subject.entity_id.trim().is_empty() || input.subject.entity_type.trim().is_empty() {
        return Err(ReraClaimError::BlankSubject);
    }
    if matches!(
        input.subject.entity_type.trim(),
        "society" | "project_group"
    ) {
        return Err(ReraClaimError::UnscopedSubjectType(
            input.subject.entity_type.trim().to_string(),
        ));
    }
    if input.predicate.trim().is_empty() {
        return Err(ReraClaimError::BlankPredicate);
    }
    if !input.extraction_confidence.is_finite()
        || !(0.0..=1.0).contains(&input.extraction_confidence)
    {
        return Err(ReraClaimError::InvalidConfidence(
            input.extraction_confidence,
        ));
    }
    Ok(())
}

fn validate_evidence(evidence: &ReraClaimEvidence) -> Result<(), ReraClaimError> {
    if evidence.source_record_id.trim().is_empty()
        || evidence.receipt_id.trim().is_empty()
        || evidence.capture_id.trim().is_empty()
        || evidence.locator.trim().is_empty()
        || evidence.parser_version.trim().is_empty()
    {
        return Err(ReraClaimError::IncompleteEvidence);
    }
    Ok(())
}

fn normalized_subject(subject: &ReraClaimSubject) -> ReraClaimSubject {
    ReraClaimSubject {
        entity_id: subject.entity_id.trim().to_string(),
        entity_type: subject.entity_type.trim().to_string(),
    }
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_string()))
}

fn canonical_json(value: &ReraClaimValue) -> Result<String, ReraClaimError> {
    serde_json::to_string(value).map_err(ReraClaimError::Json)
}

fn canonical_effective_time(
    value: Option<&ReraClaimEffectiveTime>,
) -> Result<String, ReraClaimError> {
    value
        .map(|value| serde_json::to_string(value).map_err(ReraClaimError::Json))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn assertion_mode_name(mode: ReraAssertionMode) -> &'static str {
    match mode {
        ReraAssertionMode::RegistryRecord => "registry_record",
        ReraAssertionMode::PromoterDeclaration => "promoter_declaration",
        ReraAssertionMode::ComplainantAllegation => "complainant_allegation",
        ReraAssertionMode::AuthorityOrder => "authority_order",
        ReraAssertionMode::SystemDerivation => "system_derivation",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug)]
pub enum ReraClaimError {
    BlankDerivationRule,
    BlankPredicate,
    BlankSubject,
    DerivedEvidenceMustBeEmpty,
    DuplicateDerivationInputs,
    IncompleteEvidence,
    InvalidConfidence(f64),
    InvalidDerivedAssertion,
    InvalidSourceAssertion,
    Json(serde_json::Error),
    MissingDerivation,
    MissingDerivationInputs,
    SourceEvidenceCount(usize),
    UnexpectedDerivation,
    UnscopedSubjectType(String),
}

impl fmt::Display for ReraClaimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankDerivationRule => write!(f, "RERA claim derivation rule is blank"),
            Self::BlankPredicate => write!(f, "RERA claim predicate is blank"),
            Self::BlankSubject => write!(f, "RERA claim subject is blank"),
            Self::DerivedEvidenceMustBeEmpty => {
                write!(f, "derived RERA claims must not carry source evidence")
            }
            Self::DuplicateDerivationInputs => {
                write!(f, "derived RERA claim inputs must be unique")
            }
            Self::IncompleteEvidence => write!(f, "RERA claim evidence is incomplete"),
            Self::InvalidConfidence(value) => write!(
                f,
                "RERA claim confidence must be between zero and one, got {value}"
            ),
            Self::InvalidDerivedAssertion => write!(
                f,
                "derived RERA claim must use system_derivation and derived trust"
            ),
            Self::InvalidSourceAssertion => {
                write!(f, "source RERA claim cannot use derivation semantics")
            }
            Self::Json(error) => write!(f, "failed to serialize RERA claim identity: {error}"),
            Self::MissingDerivation => write!(f, "derived RERA claim has no derivation"),
            Self::MissingDerivationInputs => write!(f, "derived RERA claim has no input claims"),
            Self::SourceEvidenceCount(count) => write!(
                f,
                "source RERA claim needs exactly one evidence record, got {count}"
            ),
            Self::UnexpectedDerivation => {
                write!(f, "source RERA claim cannot include a derivation")
            }
            Self::UnscopedSubjectType(kind) => write!(
                f,
                "RERA claims cannot attach directly to unscoped {kind} entities"
            ),
        }
    }
}

impl std::error::Error for ReraClaimError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_input(capture_id: &str) -> ReraClaimInput {
        ReraClaimInput {
            subject: ReraClaimSubject {
                entity_id: "rera_registration:in-ka:fixture".to_string(),
                entity_type: "registration".to_string(),
            },
            predicate: "official_registration_number".to_string(),
            value: ReraClaimValue::Text("PRM/KA/RERA/1251/446/PR/200811/003528".to_string()),
            unit: None,
            effective_time: None,
            assertion_mode: ReraAssertionMode::RegistryRecord,
            source_trust: ReraSourceTrust::PrimaryAuthority,
            extraction_confidence: 0.95,
            validation_state: ReraClaimValidationState::Accepted,
            visibility: ReraClaimVisibility::Public,
            evidence: vec![ReraClaimEvidence {
                source_record_id: "rera_source_record:sha256:fixture".to_string(),
                receipt_id: "rera_receipt:sha256:fixture".to_string(),
                capture_id: capture_id.to_string(),
                locator: "listing[0]".to_string(),
                parser_version: "fixture.v1".to_string(),
            }],
            derivation: None,
        }
    }

    #[test]
    fn separate_captures_remain_separate_source_assertions() {
        let first = source_input("rera_capture:sha256:first")
            .into_source_claim()
            .unwrap();
        let second = source_input("rera_capture:sha256:second")
            .into_source_claim()
            .unwrap();
        assert_ne!(first.claim_id, second.claim_id);
        assert_eq!(first.value, second.value);
    }

    #[test]
    fn derived_claim_identity_is_stable_for_input_order() {
        let mut first = source_input("rera_capture:sha256:first");
        first.assertion_mode = ReraAssertionMode::SystemDerivation;
        first.source_trust = ReraSourceTrust::Derived;
        first.evidence.clear();
        first.derivation = Some(ReraClaimDerivation {
            rule_id: "timeline.latest_date".to_string(),
            rule_version: "1".to_string(),
            input_claim_ids: vec!["claim-b".to_string(), "claim-a".to_string()],
        });
        let mut second = first.clone();
        second
            .derivation
            .as_mut()
            .unwrap()
            .input_claim_ids
            .reverse();
        assert_eq!(
            first.into_derived_claim().unwrap().claim_id,
            second.into_derived_claim().unwrap().claim_id
        );
    }

    #[test]
    fn rejects_unscoped_society_claims() {
        let mut input = source_input("rera_capture:sha256:first");
        input.subject.entity_type = "society".to_string();
        assert!(matches!(
            input.into_source_claim(),
            Err(ReraClaimError::UnscopedSubjectType(_))
        ));
    }
}
