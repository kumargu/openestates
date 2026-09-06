use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(String);

impl ObservationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DerivationId(String);

impl DerivationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum EvidenceId {
    Observation(ObservationId),
    Derivation(DerivationId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub snapshot_identity: String,
    pub subject_entity_id: String,
    pub evidence_id: EvidenceId,
}

impl EvidenceRef {
    pub fn for_observation(snapshot_identity: &str, observation: &SourceObservation) -> Self {
        Self {
            snapshot_identity: snapshot_identity.to_string(),
            subject_entity_id: observation.subject_entity_id.clone(),
            evidence_id: EvidenceId::Observation(observation.observation_id.clone()),
        }
    }

    pub fn for_derivation(derivation: &DerivedEvidence) -> Self {
        Self {
            snapshot_identity: derivation.snapshot_identity.clone(),
            subject_entity_id: derivation.subject_entity_id.clone(),
            evidence_id: EvidenceId::Derivation(derivation.derivation_id.clone()),
        }
    }

    pub fn validate_for(
        &self,
        expected_subject_entity_id: &str,
        expected_snapshot_identity: &str,
    ) -> Result<(), EvidenceIdentityError> {
        require_non_empty("expected subject entity id", expected_subject_entity_id)?;
        require_non_empty("expected snapshot identity", expected_snapshot_identity)?;
        if self.subject_entity_id != expected_subject_entity_id {
            return Err(EvidenceIdentityError::SubjectMismatch {
                expected: expected_subject_entity_id.to_string(),
                actual: self.subject_entity_id.clone(),
            });
        }
        if self.snapshot_identity != expected_snapshot_identity {
            return Err(EvidenceIdentityError::SnapshotMismatch {
                expected: expected_snapshot_identity.to_string(),
                actual: self.snapshot_identity.clone(),
            });
        }
        let valid_id = match &self.evidence_id {
            EvidenceId::Observation(id) => valid_content_id(id.as_str(), "obs"),
            EvidenceId::Derivation(id) => valid_content_id(id.as_str(), "drv"),
        };
        if !valid_id {
            return Err(EvidenceIdentityError::InvalidId);
        }
        Ok(())
    }

    fn stable_key(&self) -> String {
        serde_json::to_string(self).expect("evidence references contain only serializable fields")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceObservation {
    pub observation_id: ObservationId,
    pub provider: String,
    pub provider_observation_id: String,
    pub subject_entity_id: String,
    pub observed_at: DateTime<Utc>,
    pub source_url: Option<String>,
    pub asset_lineage: Vec<String>,
}

impl SourceObservation {
    pub fn new(
        provider: impl Into<String>,
        provider_observation_id: impl Into<String>,
        subject_entity_id: impl Into<String>,
        observed_at: DateTime<Utc>,
        source_url: Option<String>,
        asset_lineage: Vec<String>,
    ) -> Result<Self, EvidenceIdentityError> {
        let provider = provider.into();
        let provider_observation_id = provider_observation_id.into();
        let subject_entity_id = subject_entity_id.into();
        require_non_empty("provider", &provider)?;
        require_non_empty("provider observation id", &provider_observation_id)?;
        require_non_empty("subject entity id", &subject_entity_id)?;
        if asset_lineage.is_empty() || asset_lineage.iter().any(|item| item.trim().is_empty()) {
            return Err(EvidenceIdentityError::MissingField("asset lineage"));
        }
        let source_url = source_url.filter(|value| !value.trim().is_empty());
        let identity_payload = ObservationIdentityPayload {
            provider: &provider,
            provider_observation_id: &provider_observation_id,
            subject_entity_id: &subject_entity_id,
            observed_at: &observed_at,
            source_url: source_url.as_deref(),
            asset_lineage: &asset_lineage,
        };
        let observation_id = ObservationId(content_id("obs", &identity_payload));
        Ok(Self {
            observation_id,
            provider,
            provider_observation_id,
            subject_entity_id,
            observed_at,
            source_url,
            asset_lineage,
        })
    }

    pub fn validate(&self) -> Result<(), EvidenceIdentityError> {
        let expected = Self::new(
            self.provider.clone(),
            self.provider_observation_id.clone(),
            self.subject_entity_id.clone(),
            self.observed_at,
            self.source_url.clone(),
            self.asset_lineage.clone(),
        )?;
        if self.observation_id != expected.observation_id {
            return Err(EvidenceIdentityError::IdentityMismatch {
                expected: expected.observation_id.as_str().to_string(),
                actual: self.observation_id.as_str().to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ObservationIdentityPayload<'a> {
    provider: &'a str,
    provider_observation_id: &'a str,
    subject_entity_id: &'a str,
    observed_at: &'a DateTime<Utc>,
    source_url: Option<&'a str>,
    asset_lineage: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedEvidence {
    pub derivation_id: DerivationId,
    pub snapshot_identity: String,
    pub subject_entity_id: String,
    pub target_entity_id: Option<String>,
    pub relation: String,
    pub metric: String,
    pub algorithm_version: String,
    pub input_evidence: Vec<EvidenceRef>,
}

impl DerivedEvidence {
    pub fn new(
        snapshot_identity: impl Into<String>,
        subject_entity_id: impl Into<String>,
        target_entity_id: Option<String>,
        relation: impl Into<String>,
        metric: impl Into<String>,
        algorithm_version: impl Into<String>,
        mut input_evidence: Vec<EvidenceRef>,
    ) -> Result<Self, EvidenceIdentityError> {
        let snapshot_identity = snapshot_identity.into();
        let subject_entity_id = subject_entity_id.into();
        let relation = relation.into();
        let metric = metric.into();
        let algorithm_version = algorithm_version.into();
        require_non_empty("snapshot identity", &snapshot_identity)?;
        require_non_empty("subject entity id", &subject_entity_id)?;
        require_non_empty("relation", &relation)?;
        require_non_empty("metric", &metric)?;
        require_non_empty("algorithm version", &algorithm_version)?;
        if input_evidence.is_empty() {
            return Err(EvidenceIdentityError::MissingInputs);
        }
        if let Some(mismatched) = input_evidence
            .iter()
            .find(|reference| reference.snapshot_identity != snapshot_identity)
        {
            return Err(EvidenceIdentityError::SnapshotMismatch {
                expected: snapshot_identity,
                actual: mismatched.snapshot_identity.clone(),
            });
        }
        input_evidence.sort_by_key(EvidenceRef::stable_key);
        input_evidence.dedup();
        let identity_payload = DerivationIdentityPayload {
            snapshot_identity: &snapshot_identity,
            subject_entity_id: &subject_entity_id,
            target_entity_id: target_entity_id.as_deref(),
            relation: &relation,
            metric: &metric,
            algorithm_version: &algorithm_version,
            input_evidence: &input_evidence,
        };
        let derivation_id = DerivationId(content_id("drv", &identity_payload));
        Ok(Self {
            derivation_id,
            snapshot_identity,
            subject_entity_id,
            target_entity_id,
            relation,
            metric,
            algorithm_version,
            input_evidence,
        })
    }

    pub fn validate(&self) -> Result<(), EvidenceIdentityError> {
        let expected = Self::new(
            self.snapshot_identity.clone(),
            self.subject_entity_id.clone(),
            self.target_entity_id.clone(),
            self.relation.clone(),
            self.metric.clone(),
            self.algorithm_version.clone(),
            self.input_evidence.clone(),
        )?;
        if self.derivation_id != expected.derivation_id {
            return Err(EvidenceIdentityError::IdentityMismatch {
                expected: expected.derivation_id.as_str().to_string(),
                actual: self.derivation_id.as_str().to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct DerivationIdentityPayload<'a> {
    snapshot_identity: &'a str,
    subject_entity_id: &'a str,
    target_entity_id: Option<&'a str>,
    relation: &'a str,
    metric: &'a str,
    algorithm_version: &'a str,
    input_evidence: &'a [EvidenceRef],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceIdentityError {
    MissingField(&'static str),
    MissingInputs,
    InvalidId,
    IdentityMismatch { expected: String, actual: String },
    SubjectMismatch { expected: String, actual: String },
    SnapshotMismatch { expected: String, actual: String },
}

impl fmt::Display for EvidenceIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(formatter, "missing evidence identity {field}"),
            Self::MissingInputs => write!(formatter, "derived evidence requires input references"),
            Self::InvalidId => write!(formatter, "evidence reference has an invalid content id"),
            Self::IdentityMismatch { expected, actual } => write!(
                formatter,
                "evidence identity mismatch: expected {expected}, found {actual}"
            ),
            Self::SubjectMismatch { expected, actual } => write!(
                formatter,
                "evidence subject mismatch: expected {expected}, found {actual}"
            ),
            Self::SnapshotMismatch { expected, actual } => write!(
                formatter,
                "evidence snapshot mismatch: expected {expected}, found {actual}"
            ),
        }
    }
}

impl std::error::Error for EvidenceIdentityError {}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), EvidenceIdentityError> {
    if value.trim().is_empty() {
        Err(EvidenceIdentityError::MissingField(field))
    } else {
        Ok(())
    }
}

fn content_id(prefix: &str, payload: &impl Serialize) -> String {
    let encoded =
        serde_json::to_vec(payload).expect("identity payload serialization is infallible");
    let digest = Sha256::digest(encoded);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}:sha256:{hex}")
}

fn valid_content_id(value: &str, prefix: &str) -> bool {
    let Some(hex) = value.strip_prefix(&format!("{prefix}:sha256:")) else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn observation(subject: &str, provider_id: &str) -> SourceObservation {
        SourceObservation::new(
            "OpenStreetMap",
            provider_id,
            subject,
            Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            Some(format!("https://www.openstreetmap.org/{provider_id}")),
            vec!["asset:osm/version:v1".to_string()],
        )
        .unwrap()
    }

    #[test]
    fn source_observation_identity_is_stable_and_provider_qualified() {
        let first = observation("area:one", "relation/1");
        let repeated = observation("area:one", "relation/1");
        let other_provider_record = observation("area:one", "relation/2");

        assert_eq!(first.observation_id, repeated.observation_id);
        assert_ne!(first.observation_id, other_provider_record.observation_id);
        assert!(first.observation_id.as_str().starts_with("obs:sha256:"));
        assert!(first.validate().is_ok());
    }

    #[test]
    fn source_observation_validation_rejects_tampered_identity_fields() {
        let mut source = observation("area:one", "relation/1");
        source.provider_observation_id = "relation/2".to_string();

        assert!(matches!(
            source.validate(),
            Err(EvidenceIdentityError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn evidence_reference_rejects_cross_subject_and_cross_snapshot_use() {
        let source = observation("society:one", "way/1");
        let reference = EvidenceRef::for_observation("bundle:v1", &source);

        assert!(reference.validate_for("society:one", "bundle:v1").is_ok());
        assert!(matches!(
            reference.validate_for("society:two", "bundle:v1"),
            Err(EvidenceIdentityError::SubjectMismatch { .. })
        ));
        assert!(matches!(
            reference.validate_for("society:one", "bundle:v2"),
            Err(EvidenceIdentityError::SnapshotMismatch { .. })
        ));
    }

    #[test]
    fn derivation_identity_requires_pinned_inputs_and_ignores_row_order() {
        let subject =
            EvidenceRef::for_observation("bundle:v1", &observation("society:one", "way/1"));
        let target =
            EvidenceRef::for_observation("bundle:v1", &observation("area:one", "relation/1"));
        let first = DerivedEvidence::new(
            "bundle:v1",
            "society:one",
            Some("area:one".to_string()),
            "inside",
            "footprint_containment",
            "spatial-evaluator-v2",
            vec![subject.clone(), target.clone()],
        )
        .unwrap();
        let reordered = DerivedEvidence::new(
            "bundle:v1",
            "society:one",
            Some("area:one".to_string()),
            "inside",
            "footprint_containment",
            "spatial-evaluator-v2",
            vec![target, subject],
        )
        .unwrap();

        assert_eq!(first.derivation_id, reordered.derivation_id);
        assert!(first.derivation_id.as_str().starts_with("drv:sha256:"));
        assert!(EvidenceRef::for_derivation(&first)
            .validate_for("society:one", "bundle:v1")
            .is_ok());
        assert!(first.validate().is_ok());
    }

    #[test]
    fn derivation_rejects_missing_or_cross_snapshot_inputs() {
        assert!(matches!(
            DerivedEvidence::new(
                "bundle:v1",
                "society:one",
                None,
                "near",
                "footprint_distance",
                "spatial-evaluator-v2",
                Vec::new(),
            ),
            Err(EvidenceIdentityError::MissingInputs)
        ));

        let wrong_snapshot =
            EvidenceRef::for_observation("bundle:v2", &observation("society:one", "way/1"));
        assert!(matches!(
            DerivedEvidence::new(
                "bundle:v1",
                "society:one",
                None,
                "near",
                "footprint_distance",
                "spatial-evaluator-v2",
                vec![wrong_snapshot],
            ),
            Err(EvidenceIdentityError::SnapshotMismatch { .. })
        ));
    }
}
