//! Canonical, source-qualified claims for the RERA evidence graph.
//!
//! Claims are not buyer labels or conclusions. They are immutable assertions
//! whose identity includes their source evidence (or, for derivations, every
//! input claim and the versioned rule that produced them).

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{ArrayRef, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lake::LakeStore;

use super::{
    read_rera_source_records, ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, MaterializationId, MaterializationRecord, ReraSourceRecord,
    ReraSourceRecordKind, ReraSourceRecordsError,
};

pub const RERA_CLAIMS_ASSET_ID: &str = "rera_claims";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimsQualityReport {
    pub source_record_count: u64,
    pub claim_count: u64,
    pub accepted_claim_count: u64,
    pub quarantined_claim_count: u64,
    pub claim_counts_by_predicate: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraClaimsManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub run_id: String,
    pub quality: ReraClaimsQualityReport,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Clone)]
pub struct ReraClaimsMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ReraClaimsMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_from_source_records_for_run(
        &self,
        source_records: &MaterializationRecord,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<MaterializationRecord, ReraClaimMaterializeError> {
        let records = read_rera_source_records(&self.lake, source_records).await?;
        let claims = claims_from_source_records(&records)?;
        let run_id = dag_run_id.to_string();
        let claims_key = AssetPathBuilder::gold_asset_key(
            RERA_CLAIMS_ASSET_ID,
            &run_id,
            "claims/part-00000.parquet",
        );
        let claims_artifact = ArtifactRef::parquet(
            self.lake
                .put_bytes(&claims_key, write_claims(&claims)?)
                .await?,
        );
        let quality = claims_quality(&records, &claims);
        let manifest_key =
            AssetPathBuilder::gold_asset_key(RERA_CLAIMS_ASSET_ID, &run_id, "claims/manifest.json");
        let mut artifacts = vec![claims_artifact];
        let manifest = ReraClaimsManifest {
            asset_id: RERA_CLAIMS_ASSET_ID.to_string(),
            format_version: 1,
            run_id,
            quality,
            artifacts: artifacts.clone(),
        };
        artifacts.push(ArtifactRef::json(
            self.lake.put_json(&manifest_key, &manifest).await?,
        ));
        let record = MaterializationRecord::succeeded(
            AssetId::new(RERA_CLAIMS_ASSET_ID).expect("static RERA claims asset ID is valid"),
            super::AssetStage::Gold,
            partition,
            source_records.source_watermarks.first().map_or_else(
                || "rera-claims".to_string(),
                |watermark| watermark.high_watermark.clone(),
            ),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(vec![source_records.materialization_id.clone()])
        .with_source_watermarks(source_records.source_watermarks.clone())
        .with_row_count(claims.len() as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

pub fn claims_from_source_records(
    records: &[ReraSourceRecord],
) -> Result<Vec<ReraClaimV1>, ReraClaimMaterializeError> {
    let mut claims = Vec::new();
    for record in records {
        match record.kind {
            ReraSourceRecordKind::RegistrationSummary => {
                claims.extend(registration_summary_claims(record)?);
            }
            ReraSourceRecordKind::PromoterDeclaration
            | ReraSourceRecordKind::WaterServiceDeclaration
            | ReraSourceRecordKind::Completion
            | ReraSourceRecordKind::QuarterlyProgress
            | ReraSourceRecordKind::TowerInventory => {
                claims.extend(project_detail_claims(record)?);
            }
            _ => {}
        }
    }
    claims.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
    let mut seen = HashSet::new();
    if let Some(claim) = claims
        .iter()
        .find(|claim| !seen.insert(claim.claim_id.as_str()))
    {
        return Err(ReraClaimMaterializeError::DuplicateClaimId(
            claim.claim_id.clone(),
        ));
    }
    Ok(claims)
}

fn registration_summary_claims(
    record: &ReraSourceRecord,
) -> Result<Vec<ReraClaimV1>, ReraClaimMaterializeError> {
    let fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&record.raw_value).map_err(|_| {
            ReraClaimMaterializeError::MalformedRegistrationSummary(record.record_id.clone())
        })?;
    let evidence = ReraClaimEvidence {
        source_record_id: record.record_id.clone(),
        receipt_id: record.receipt_id.clone(),
        capture_id: record.capture_id.clone(),
        locator: record.source_locator.clone(),
        parser_version: record.parser_version.clone(),
    };
    let subject = ReraClaimSubject {
        entity_id: record.registration_id.clone(),
        entity_type: "registration".to_string(),
    };
    let values = [
        ("official_registration_number", "registration_number"),
        ("registry_acknowledgement_number", "acknowledgement_number"),
        ("registry_project_name", "project_name"),
        ("registry_promoter_name", "promoter_name"),
    ];
    values
        .into_iter()
        .filter_map(|(predicate, key)| {
            fields
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| (predicate, value))
        })
        .map(|(predicate, value)| {
            ReraClaimInput {
                subject: subject.clone(),
                predicate: predicate.to_string(),
                value: ReraClaimValue::Text(value.to_string()),
                unit: None,
                effective_time: None,
                assertion_mode: ReraAssertionMode::RegistryRecord,
                source_trust: ReraSourceTrust::PrimaryAuthority,
                extraction_confidence: 0.95,
                validation_state: ReraClaimValidationState::Accepted,
                visibility: ReraClaimVisibility::Public,
                evidence: vec![evidence.clone()],
                derivation: None,
            }
            .into_source_claim()
            .map_err(ReraClaimMaterializeError::Claim)
        })
        .collect()
}

fn project_detail_claims(
    record: &ReraSourceRecord,
) -> Result<Vec<ReraClaimV1>, ReraClaimMaterializeError> {
    let fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&record.raw_value).map_err(|_| {
            ReraClaimMaterializeError::MalformedProjectDetail(record.record_id.clone())
        })?;
    let subject = ReraClaimSubject {
        entity_id: record.registration_id.clone(),
        entity_type: "registration".to_string(),
    };
    let evidence = ReraClaimEvidence {
        source_record_id: record.record_id.clone(),
        receipt_id: record.receipt_id.clone(),
        capture_id: record.capture_id.clone(),
        locator: record.source_locator.clone(),
        parser_version: record.parser_version.clone(),
    };
    let effective_time = record
        .filing_at
        .as_ref()
        .map(|filing_at| ReraClaimEffectiveTime {
            start: Some(filing_at.clone()),
            end: None,
            precision: "day".to_string(),
        });
    let declaration = |claim_subject: ReraClaimSubject,
                       predicate: &str,
                       value: ReraClaimValue,
                       unit: Option<&str>| {
        ReraClaimInput {
            subject: claim_subject,
            predicate: predicate.to_string(),
            value,
            unit: unit.map(ToString::to_string),
            effective_time: effective_time.clone(),
            assertion_mode: ReraAssertionMode::PromoterDeclaration,
            source_trust: ReraSourceTrust::PromoterFiling,
            extraction_confidence: 0.9,
            validation_state: ReraClaimValidationState::Accepted,
            visibility: ReraClaimVisibility::Public,
            evidence: vec![evidence.clone()],
            derivation: None,
        }
        .into_source_claim()
        .map_err(ReraClaimMaterializeError::Claim)
    };
    let mut claims = Vec::new();
    match record.kind {
        ReraSourceRecordKind::PromoterDeclaration => {
            if let Some(value) = fields.get("unit_count").and_then(serde_json::Value::as_f64) {
                claims.push(declaration(
                    subject.clone(),
                    "declared_unit_count",
                    ReraClaimValue::Number(value),
                    Some("units"),
                )?);
            }
            if let Some(value) = fields
                .get("total_carpet_area_sqm")
                .and_then(serde_json::Value::as_f64)
            {
                claims.push(declaration(
                    subject.clone(),
                    "declared_project_total_carpet_area",
                    ReraClaimValue::Number(value),
                    Some("square_metres"),
                )?);
            }
        }
        ReraSourceRecordKind::WaterServiceDeclaration => {
            if let Some(value) = fields.get("source").and_then(serde_json::Value::as_str) {
                claims.push(declaration(
                    subject.clone(),
                    "declared_water_source",
                    ReraClaimValue::Text(value.to_string()),
                    None,
                )?);
            }
            if let Some(value) = fields.get("authority").and_then(serde_json::Value::as_str) {
                claims.push(declaration(
                    subject.clone(),
                    "declared_water_local_authority",
                    ReraClaimValue::Text(value.to_string()),
                    None,
                )?);
            }
        }
        ReraSourceRecordKind::Completion => {
            let Some(value) = fields.get("date").and_then(serde_json::Value::as_str) else {
                return Ok(claims);
            };
            let predicate = match record.raw_label.as_str() {
                "Registration start date" => "registration_start_date",
                "Proposed completion date" => "proposed_completion_date",
                _ => return Ok(claims),
            };
            claims.push(declaration(
                subject.clone(),
                predicate,
                ReraClaimValue::Date(value.to_string()),
                None,
            )?);
        }
        ReraSourceRecordKind::QuarterlyProgress => {
            for (field, predicate) in [
                ("total_units", "quarterly_reported_total_units"),
                ("booked_units", "quarterly_reported_booked_units"),
                ("unsold_units", "quarterly_reported_unsold_units"),
            ] {
                if let Some(value) = fields.get(field).and_then(serde_json::Value::as_f64) {
                    claims.push(declaration(
                        subject.clone(),
                        predicate,
                        ReraClaimValue::Number(value),
                        Some("units"),
                    )?);
                }
            }
        }
        ReraSourceRecordKind::TowerInventory => {
            let Some(label) = fields
                .get("inventory_type")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                return Ok(claims);
            };
            if label.trim().eq_ignore_ascii_case("total") {
                for (field, predicate, unit) in [
                    (
                        "unit_count",
                        "declared_inventory_total_unit_count",
                        Some("units"),
                    ),
                    (
                        "total_carpet_area_sqm",
                        "declared_inventory_total_carpet_area",
                        Some("square_metres"),
                    ),
                    (
                        "total_balcony_verandah_area_sqm",
                        "declared_inventory_total_balcony_verandah_area",
                        Some("square_metres"),
                    ),
                    (
                        "total_open_terrace_area_sqm",
                        "declared_inventory_total_open_terrace_area",
                        Some("square_metres"),
                    ),
                ] {
                    if let Some(value) = fields.get(field).and_then(serde_json::Value::as_f64) {
                        claims.push(declaration(
                            subject.clone(),
                            predicate,
                            ReraClaimValue::Number(value),
                            unit,
                        )?);
                    }
                }
                return Ok(claims);
            }

            let configuration_subject = ReraClaimSubject {
                entity_id: inventory_configuration_id(&record.registration_id, label),
                entity_type: "inventory_configuration".to_string(),
            };
            claims.push(declaration(
                configuration_subject.clone(),
                "part_of_registration",
                ReraClaimValue::EntityRef {
                    entity_id: subject.entity_id.clone(),
                    entity_type: subject.entity_type.clone(),
                },
                None,
            )?);
            claims.push(declaration(
                configuration_subject.clone(),
                "inventory_configuration_label",
                ReraClaimValue::Text(label.trim().to_string()),
                None,
            )?);
            for (field, predicate, unit) in [
                ("unit_count", "declared_inventory_unit_count", Some("units")),
                (
                    "total_carpet_area_sqm",
                    "declared_inventory_total_carpet_area",
                    Some("square_metres"),
                ),
                (
                    "total_balcony_verandah_area_sqm",
                    "declared_inventory_total_balcony_verandah_area",
                    Some("square_metres"),
                ),
                (
                    "total_open_terrace_area_sqm",
                    "declared_inventory_total_open_terrace_area",
                    Some("square_metres"),
                ),
            ] {
                if let Some(value) = fields.get(field).and_then(serde_json::Value::as_f64) {
                    claims.push(declaration(
                        configuration_subject.clone(),
                        predicate,
                        ReraClaimValue::Number(value),
                        unit,
                    )?);
                }
            }
        }
        _ => {}
    }
    Ok(claims)
}

fn inventory_configuration_id(registration_id: &str, label: &str) -> String {
    let normalized_label = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase();
    let material = format!(
        "rera_inventory_configuration.v1\n{}\n{}",
        registration_id.trim(),
        normalized_label
    );
    format!(
        "rera_inventory_configuration:sha256:{}",
        sha256_hex(material.as_bytes())
    )
}

fn claims_quality(records: &[ReraSourceRecord], claims: &[ReraClaimV1]) -> ReraClaimsQualityReport {
    let mut claim_counts_by_predicate = std::collections::BTreeMap::new();
    for claim in claims {
        *claim_counts_by_predicate
            .entry(claim.predicate.clone())
            .or_insert(0) += 1;
    }
    ReraClaimsQualityReport {
        source_record_count: records.len() as u64,
        claim_count: claims.len() as u64,
        accepted_claim_count: claims
            .iter()
            .filter(|claim| claim.validation_state == ReraClaimValidationState::Accepted)
            .count() as u64,
        quarantined_claim_count: claims
            .iter()
            .filter(|claim| claim.validation_state == ReraClaimValidationState::Quarantined)
            .count() as u64,
        claim_counts_by_predicate,
    }
}

fn write_claims(claims: &[ReraClaimV1]) -> Result<Vec<u8>, ReraClaimMaterializeError> {
    let value_json = claims
        .iter()
        .map(|claim| serde_json::to_string(&claim.value))
        .collect::<Result<Vec<_>, _>>()?;
    let effective_time_json = claims
        .iter()
        .map(|claim| {
            claim
                .effective_time
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence_json = claims
        .iter()
        .map(|claim| serde_json::to_string(&claim.evidence))
        .collect::<Result<Vec<_>, _>>()?;
    let derivation_json = claims
        .iter()
        .map(|claim| {
            claim
                .derivation
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let schema = Arc::new(Schema::new(vec![
        Field::new("claim_id", DataType::Utf8, false),
        Field::new("subject_id", DataType::Utf8, false),
        Field::new("subject_type", DataType::Utf8, false),
        Field::new("predicate", DataType::Utf8, false),
        Field::new("value_json", DataType::Utf8, false),
        Field::new("unit", DataType::Utf8, true),
        Field::new("effective_time_json", DataType::Utf8, true),
        Field::new("assertion_mode", DataType::Utf8, false),
        Field::new("source_trust", DataType::Utf8, false),
        Field::new("extraction_confidence", DataType::Utf8, false),
        Field::new("validation_state", DataType::Utf8, false),
        Field::new("visibility", DataType::Utf8, false),
        Field::new("evidence_json", DataType::Utf8, false),
        Field::new("derivation_json", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            strings(claims.iter().map(|claim| claim.claim_id.clone())),
            strings(claims.iter().map(|claim| claim.subject.entity_id.clone())),
            strings(claims.iter().map(|claim| claim.subject.entity_type.clone())),
            strings(claims.iter().map(|claim| claim.predicate.clone())),
            strings(value_json.into_iter()),
            optional_strings(claims.iter().map(|claim| claim.unit.clone())),
            optional_strings(effective_time_json.into_iter()),
            strings(
                claims
                    .iter()
                    .map(|claim| assertion_mode_name(claim.assertion_mode).to_string()),
            ),
            strings(
                claims
                    .iter()
                    .map(|claim| source_trust_name(claim.source_trust).to_string()),
            ),
            strings(
                claims
                    .iter()
                    .map(|claim| claim.extraction_confidence.to_string()),
            ),
            strings(
                claims
                    .iter()
                    .map(|claim| validation_state_name(claim.validation_state).to_string()),
            ),
            strings(
                claims
                    .iter()
                    .map(|claim| visibility_name(claim.visibility).to_string()),
            ),
            strings(evidence_json.into_iter()),
            optional_strings(derivation_json.into_iter()),
        ],
    )?;
    let mut bytes = Vec::new();
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(bytes)
}

fn strings(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_strings(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn source_trust_name(value: ReraSourceTrust) -> &'static str {
    match value {
        ReraSourceTrust::PrimaryAuthority => "primary_authority",
        ReraSourceTrust::PromoterFiling => "promoter_filing",
        ReraSourceTrust::PartyFiling => "party_filing",
        ReraSourceTrust::LicensedDataset => "licensed_dataset",
        ReraSourceTrust::Derived => "derived",
    }
}

fn validation_state_name(value: ReraClaimValidationState) -> &'static str {
    match value {
        ReraClaimValidationState::Accepted => "accepted",
        ReraClaimValidationState::Quarantined => "quarantined",
    }
}

fn visibility_name(value: ReraClaimVisibility) -> &'static str {
    match value {
        ReraClaimVisibility::Public => "public",
        ReraClaimVisibility::Restricted => "restricted",
    }
}

#[derive(Debug)]
pub enum ReraClaimMaterializeError {
    Arrow(arrow::error::ArrowError),
    Claim(ReraClaimError),
    DuplicateClaimId(String),
    Json(serde_json::Error),
    Lake(crate::lake::LakeError),
    MalformedRegistrationSummary(String),
    MalformedProjectDetail(String),
    Parquet(parquet::errors::ParquetError),
    SourceRecords(ReraSourceRecordsError),
}

impl fmt::Display for ReraClaimMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(error) => write!(f, "RERA claim Arrow error: {error}"),
            Self::Claim(error) => write!(f, "invalid RERA claim: {error}"),
            Self::DuplicateClaimId(claim_id) => write!(f, "duplicate RERA claim ID {claim_id}"),
            Self::Json(error) => write!(f, "RERA claim JSON serialization failed: {error}"),
            Self::Lake(error) => write!(f, "RERA claim lake error: {error}"),
            Self::MalformedRegistrationSummary(record_id) => write!(
                f,
                "RERA registration summary {record_id} is not structured JSON"
            ),
            Self::MalformedProjectDetail(record_id) => {
                write!(f, "RERA project detail {record_id} is not structured JSON")
            }
            Self::Parquet(error) => write!(f, "RERA claim Parquet error: {error}"),
            Self::SourceRecords(error) => write!(f, "RERA source record read failed: {error}"),
        }
    }
}

impl std::error::Error for ReraClaimMaterializeError {}

impl From<ReraClaimError> for ReraClaimMaterializeError {
    fn from(value: ReraClaimError) -> Self {
        Self::Claim(value)
    }
}
impl From<arrow::error::ArrowError> for ReraClaimMaterializeError {
    fn from(value: arrow::error::ArrowError) -> Self {
        Self::Arrow(value)
    }
}
impl From<serde_json::Error> for ReraClaimMaterializeError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
impl From<crate::lake::LakeError> for ReraClaimMaterializeError {
    fn from(value: crate::lake::LakeError) -> Self {
        Self::Lake(value)
    }
}
impl From<parquet::errors::ParquetError> for ReraClaimMaterializeError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        Self::Parquet(value)
    }
}
impl From<ReraSourceRecordsError> for ReraClaimMaterializeError {
    fn from(value: ReraSourceRecordsError) -> Self {
        Self::SourceRecords(value)
    }
}

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

    #[test]
    fn project_detail_claims_keep_promoter_filing_context() {
        let record = ReraSourceRecord {
            record_id: "rera_source_record:sha256:qpr".to_string(),
            kind: ReraSourceRecordKind::QuarterlyProgress,
            registration_id: "rera_registration:in-ka:fixture".to_string(),
            normalized_registration_number: "PRM/KA/RERA/FIXTURE".to_string(),
            receipt_id: "rera_receipt:sha256:fixture".to_string(),
            capture_id: "rera_capture:sha256:fixture".to_string(),
            source_locator: "#quarterly-update/q1-2026-27".to_string(),
            raw_label: "Quarterly inventory totals".to_string(),
            raw_value:
                r#"{"quarter":"Q1","total_units":970,"booked_units":837,"unsold_units":133}"#
                    .to_string(),
            observed_at: chrono::Utc::now(),
            effective_at: None,
            filing_at: Some("2026-07-13".to_string()),
            parser_version: "fixture.v1".to_string(),
        };

        let claims = project_detail_claims(&record).unwrap();

        assert_eq!(claims.len(), 3);
        assert!(claims.iter().all(|claim| {
            claim.assertion_mode == ReraAssertionMode::PromoterDeclaration
                && claim.source_trust == ReraSourceTrust::PromoterFiling
                && claim.unit.as_deref() == Some("units")
                && claim
                    .effective_time
                    .as_ref()
                    .and_then(|time| time.start.as_deref())
                    == Some("2026-07-13")
        }));
    }

    #[test]
    fn inventory_claims_keep_configuration_scope_and_optional_areas() {
        let registration_id = "rera_registration:in-ka:fixture";
        let configuration = ReraSourceRecord {
            record_id: "rera_source_record:sha256:inventory-configuration".to_string(),
            kind: ReraSourceRecordKind::TowerInventory,
            registration_id: registration_id.to_string(),
            normalized_registration_number: "PRM/KA/RERA/FIXTURE".to_string(),
            receipt_id: "rera_receipt:sha256:fixture".to_string(),
            capture_id: "rera_capture:sha256:fixture".to_string(),
            source_locator: "#menu2/development-inventory/row-1".to_string(),
            raw_label: "Declared inventory configuration".to_string(),
            raw_value:
                r#"{"inventory_type":"3BHK+3T","unit_count":135,"total_carpet_area_sqm":14832}"#
                    .to_string(),
            observed_at: chrono::Utc::now(),
            effective_at: None,
            filing_at: None,
            parser_version: "fixture.v1".to_string(),
        };

        let claims = project_detail_claims(&configuration).unwrap();
        let configuration_id = inventory_configuration_id(registration_id, " 3BHK+3T ");
        assert_eq!(claims.len(), 4);
        assert!(claims.iter().all(|claim| {
            claim.assertion_mode == ReraAssertionMode::PromoterDeclaration
                && claim.source_trust == ReraSourceTrust::PromoterFiling
                && claim.subject.entity_type == "inventory_configuration"
                && claim.subject.entity_id == configuration_id
        }));
        assert!(claims.iter().any(|claim| {
            claim.predicate == "part_of_registration"
                && claim.value
                    == ReraClaimValue::EntityRef {
                        entity_id: registration_id.to_string(),
                        entity_type: "registration".to_string(),
                    }
        }));
        assert!(claims.iter().any(|claim| {
            claim.predicate == "declared_inventory_total_carpet_area"
                && claim.unit.as_deref() == Some("square_metres")
        }));
        assert!(!claims.iter().any(|claim| {
            matches!(
                claim.predicate.as_str(),
                "declared_inventory_total_balcony_verandah_area"
                    | "declared_inventory_total_open_terrace_area"
            )
        }));

        let aggregate = ReraSourceRecord {
            record_id: "rera_source_record:sha256:inventory-total".to_string(),
            source_locator: "#menu2/development-inventory/total".to_string(),
            raw_label: "Declared inventory aggregate".to_string(),
            raw_value:
                r#"{"inventory_type":"TOTAL","unit_count":698,"total_carpet_area_sqm":65096}"#
                    .to_string(),
            ..configuration
        };
        let aggregate_claims = project_detail_claims(&aggregate).unwrap();
        assert!(aggregate_claims
            .iter()
            .all(|claim| claim.subject.entity_type == "registration"));
        assert!(aggregate_claims.iter().any(|claim| {
            claim.predicate == "declared_inventory_total_unit_count"
                && claim.value == ReraClaimValue::Number(698.0)
        }));
    }
}
