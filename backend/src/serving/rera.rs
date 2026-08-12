use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::assets::{
    inventory_reconciliations, ReraClaimV1, ReraClaimValidationState, ReraClaimValue,
    ReraClaimVisibility, ReraInventoryReconciliationV1, ReraReceiptRecord, ReraSourceRecord,
    ReraSourceRecordKind,
};

pub const RERA_EVIDENCE_SCHEMA_VERSION: &str = "rera_evidence_projection.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServingReraEvidenceRecord {
    pub society_id: String,
    pub registration_ids: Vec<String>,
    pub entities: Vec<ReraEvidenceEntity>,
    pub claims: Vec<ReraClaimV1>,
    pub events: Vec<ReraEvidenceEvent>,
    pub series: Vec<ReraEvidenceSeries>,
    pub discrepancies: Vec<ReraInventoryReconciliationV1>,
    pub coverage: Vec<ReraEvidenceCoverage>,
    pub source_index: Vec<ReraEvidenceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraEvidenceEntity {
    pub entity_id: String,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraEvidenceEvent {
    pub event_id: String,
    pub registration_id: String,
    pub event_type: String,
    pub date: String,
    pub claim_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraEvidenceSeries {
    pub series_id: String,
    pub registration_id: String,
    pub series_type: String,
    pub points: Vec<ReraEvidenceSeriesPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraEvidenceSeriesPoint {
    pub point_id: String,
    pub effective_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quarter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub financial_year: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tower_count: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_units: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booked_units: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsold_units: Option<f64>,
    pub claim_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraEvidenceCoverage {
    pub source_section: String,
    pub record_count: u64,
    pub latest_observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraEvidenceSource {
    pub receipt_id: String,
    pub capture_id: String,
    pub source_url: String,
    pub captured_at: DateTime<Utc>,
    pub content_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReraEvidenceIndex {
    by_society: HashMap<String, ServingReraEvidenceRecord>,
}

impl ReraEvidenceIndex {
    pub fn from_records(records: Vec<ServingReraEvidenceRecord>) -> Self {
        Self {
            by_society: records
                .into_iter()
                .map(|record| (record.society_id.clone(), record))
                .collect(),
        }
    }

    pub fn society(&self, society_id: &str) -> Option<&ServingReraEvidenceRecord> {
        self.by_society.get(society_id)
    }

    pub fn add_aliases(&mut self, aliases: &[(String, String)]) {
        for (alias, canonical_id) in aliases {
            if self.by_society.contains_key(alias) {
                continue;
            }
            if let Some(record) = self.by_society.get(canonical_id).cloned() {
                self.by_society.insert(alias.clone(), record);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.by_society.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_society.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct RegistrationRelation {
    entity_id: String,
    entity_type: String,
    resolution_method: String,
    resolution_confidence: f64,
}

pub fn project_rera_evidence(
    source_records: &[ReraSourceRecord],
    claims: &[ReraClaimV1],
    receipts: &[ReraReceiptRecord],
) -> Result<Vec<ServingReraEvidenceRecord>, ReraServingProjectionError> {
    let relations = registration_relations(source_records)?;
    let accepted_public_claims = claims
        .iter()
        .filter(|claim| {
            claim.validation_state == ReraClaimValidationState::Accepted
                && claim.visibility == ReraClaimVisibility::Public
        })
        .cloned()
        .collect::<Vec<_>>();
    let entity_registrations = entity_registrations(&accepted_public_claims);
    let reconciliations = inventory_reconciliations(&accepted_public_claims);
    let receipt_by_evidence = receipts
        .iter()
        .map(|receipt| {
            (
                (receipt.receipt_id.as_str(), receipt.capture_id.as_str()),
                receipt,
            )
        })
        .collect::<HashMap<_, _>>();

    let mut records = Vec::new();
    for (society_id, registration_ids) in relations {
        let claims = accepted_public_claims
            .iter()
            .filter(|claim| {
                registration_for_claim(claim, &entity_registrations)
                    .is_some_and(|registration_id| registration_ids.contains(registration_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let sources = sources_for_claims(&claims, &receipt_by_evidence)?;
        let source_records = source_records
            .iter()
            .filter(|record| registration_ids.contains(&record.registration_id))
            .collect::<Vec<_>>();
        let discrepancies = reconciliations
            .iter()
            .filter(|item| registration_ids.contains(&item.registration_id))
            .cloned()
            .collect::<Vec<_>>();
        records.push(ServingReraEvidenceRecord {
            society_id,
            registration_ids: registration_ids.into_iter().collect(),
            entities: evidence_entities(&claims, &entity_registrations),
            events: evidence_events(&claims),
            series: quarterly_series(&claims),
            discrepancies,
            coverage: evidence_coverage(&source_records),
            source_index: sources,
            claims,
        });
    }
    records.sort_by(|left, right| left.society_id.cmp(&right.society_id));
    Ok(records)
}

fn registration_relations(
    records: &[ReraSourceRecord],
) -> Result<BTreeMap<String, BTreeSet<String>>, ReraServingProjectionError> {
    let mut relations = BTreeMap::<String, BTreeSet<String>>::new();
    for record in records
        .iter()
        .filter(|record| record.kind == ReraSourceRecordKind::RegistrationRelation)
    {
        let relation: RegistrationRelation =
            serde_json::from_str(&record.raw_value).map_err(|_| {
                ReraServingProjectionError::MalformedRegistrationRelation(record.record_id.clone())
            })?;
        if relation.entity_type != "society"
            || relation.resolution_method != "catalog_project_key_exact"
            || relation.resolution_confidence != 1.0
            || relation.entity_id.trim().is_empty()
        {
            return Err(ReraServingProjectionError::UnsafeRegistrationRelation(
                record.record_id.clone(),
            ));
        }
        relations
            .entry(relation.entity_id)
            .or_default()
            .insert(record.registration_id.clone());
    }
    Ok(relations)
}

fn entity_registrations(claims: &[ReraClaimV1]) -> HashMap<String, String> {
    let mut registrations = HashMap::new();
    for claim in claims {
        if claim.subject.entity_type == "registration" {
            registrations.insert(
                claim.subject.entity_id.clone(),
                claim.subject.entity_id.clone(),
            );
        }
        if claim.predicate == "part_of_registration" {
            if let ReraClaimValue::EntityRef {
                entity_id,
                entity_type,
            } = &claim.value
            {
                if entity_type == "registration" {
                    registrations.insert(claim.subject.entity_id.clone(), entity_id.clone());
                }
            }
        }
    }
    registrations
}

fn registration_for_claim<'a>(
    claim: &'a ReraClaimV1,
    registrations: &'a HashMap<String, String>,
) -> Option<&'a str> {
    registrations
        .get(&claim.subject.entity_id)
        .map(String::as_str)
}

fn evidence_entities(
    claims: &[ReraClaimV1],
    registrations: &HashMap<String, String>,
) -> Vec<ReraEvidenceEntity> {
    let mut entities = BTreeMap::<String, ReraEvidenceEntity>::new();
    for claim in claims {
        let registration_id = registration_for_claim(claim, registrations).map(str::to_string);
        let entry = entities
            .entry(claim.subject.entity_id.clone())
            .or_insert_with(|| ReraEvidenceEntity {
                entity_id: claim.subject.entity_id.clone(),
                entity_type: claim.subject.entity_type.clone(),
                label: None,
                registration_id,
            });
        if matches!(
            claim.predicate.as_str(),
            "registry_project_name" | "inventory_configuration_label" | "document_label"
        ) {
            if let ReraClaimValue::Text(label) = &claim.value {
                entry.label = Some(label.clone());
            }
        }
    }
    entities.into_values().collect()
}

fn evidence_events(claims: &[ReraClaimV1]) -> Vec<ReraEvidenceEvent> {
    let mut events = claims
        .iter()
        .filter_map(|claim| {
            let ReraClaimValue::Date(date) = &claim.value else {
                return None;
            };
            (claim.subject.entity_type == "registration").then(|| ReraEvidenceEvent {
                event_id: format!("rera_event:{}", claim.claim_id),
                registration_id: claim.subject.entity_id.clone(),
                event_type: claim.predicate.clone(),
                date: date.clone(),
                claim_ids: vec![claim.claim_id.clone()],
            })
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    events
}

fn quarterly_series(claims: &[ReraClaimV1]) -> Vec<ReraEvidenceSeries> {
    let mut by_registration = BTreeMap::<String, BTreeMap<String, Vec<&ReraClaimV1>>>::new();
    for claim in claims.iter().filter(|claim| {
        claim.subject.entity_type == "registration" && claim.predicate.starts_with("quarterly_")
    }) {
        let Some(evidence) = claim.evidence.first() else {
            continue;
        };
        by_registration
            .entry(claim.subject.entity_id.clone())
            .or_default()
            .entry(evidence.source_record_id.clone())
            .or_default()
            .push(claim);
    }

    by_registration
        .into_iter()
        .filter_map(|(registration_id, filings)| {
            let mut points = filings
                .into_iter()
                .filter_map(|(source_record_id, claims)| quarterly_point(source_record_id, claims))
                .collect::<Vec<_>>();
            points.sort_by(|left, right| {
                left.effective_at
                    .cmp(&right.effective_at)
                    .then_with(|| left.point_id.cmp(&right.point_id))
            });
            (!points.is_empty()).then(|| ReraEvidenceSeries {
                series_id: format!("rera_qpr_series:{registration_id}"),
                registration_id,
                series_type: "quarterly_inventory".to_string(),
                points,
            })
        })
        .collect()
}

fn quarterly_point(
    source_record_id: String,
    mut claims: Vec<&ReraClaimV1>,
) -> Option<ReraEvidenceSeriesPoint> {
    claims.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
    let effective_at = claims
        .iter()
        .find_map(|claim| claim.effective_time.as_ref()?.start.clone())?;
    let text = |predicate: &str| {
        claims.iter().find_map(|claim| {
            (claim.predicate == predicate)
                .then_some(&claim.value)
                .and_then(|value| {
                    if let ReraClaimValue::Text(value) = value {
                        Some(value.clone())
                    } else {
                        None
                    }
                })
        })
    };
    let number = |predicate: &str| {
        claims.iter().find_map(|claim| {
            (claim.predicate == predicate)
                .then_some(&claim.value)
                .and_then(|value| {
                    if let ReraClaimValue::Number(value) = value {
                        Some(*value)
                    } else {
                        None
                    }
                })
        })
    };
    Some(ReraEvidenceSeriesPoint {
        point_id: source_record_id,
        effective_at,
        quarter: text("quarterly_filing_quarter"),
        financial_year: text("quarterly_filing_financial_year"),
        tower_count: number("quarterly_reported_tower_count"),
        total_units: number("quarterly_reported_total_units"),
        booked_units: number("quarterly_reported_booked_units"),
        unsold_units: number("quarterly_reported_unsold_units"),
        claim_ids: claims
            .into_iter()
            .map(|claim| claim.claim_id.clone())
            .collect(),
    })
}

fn evidence_coverage(records: &[&ReraSourceRecord]) -> Vec<ReraEvidenceCoverage> {
    let mut by_kind = BTreeMap::<String, (u64, DateTime<Utc>)>::new();
    for record in records {
        let kind = serde_json::to_value(record.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let entry = by_kind.entry(kind).or_insert((0, record.observed_at));
        entry.0 += 1;
        entry.1 = entry.1.max(record.observed_at);
    }
    by_kind
        .into_iter()
        .map(
            |(source_section, (record_count, latest_observed_at))| ReraEvidenceCoverage {
                source_section,
                record_count,
                latest_observed_at,
            },
        )
        .collect()
}

fn sources_for_claims(
    claims: &[ReraClaimV1],
    receipt_by_evidence: &HashMap<(&str, &str), &ReraReceiptRecord>,
) -> Result<Vec<ReraEvidenceSource>, ReraServingProjectionError> {
    let mut sources = BTreeMap::<(String, String), ReraEvidenceSource>::new();
    for evidence in claims.iter().flat_map(|claim| &claim.evidence) {
        let receipt = receipt_by_evidence
            .get(&(evidence.receipt_id.as_str(), evidence.capture_id.as_str()))
            .ok_or_else(|| {
                ReraServingProjectionError::MissingReceiptLineage(evidence.source_record_id.clone())
            })?;
        sources
            .entry((evidence.receipt_id.clone(), evidence.capture_id.clone()))
            .or_insert_with(|| ReraEvidenceSource {
                receipt_id: evidence.receipt_id.clone(),
                capture_id: evidence.capture_id.clone(),
                source_url: receipt.source_url.clone(),
                captured_at: receipt.captured_at,
                content_type: receipt.content_type.clone(),
            });
    }
    Ok(sources.into_values().collect())
}

#[derive(Debug)]
pub enum ReraServingProjectionError {
    MalformedRegistrationRelation(String),
    MissingReceiptLineage(String),
    UnsafeRegistrationRelation(String),
}

impl fmt::Display for ReraServingProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedRegistrationRelation(record_id) => {
                write!(f, "RERA registration relation {record_id} is malformed")
            }
            Self::MissingReceiptLineage(record_id) => {
                write!(f, "RERA claim {record_id} has no receipt lineage")
            }
            Self::UnsafeRegistrationRelation(record_id) => write!(
                f,
                "RERA registration relation {record_id} is not an exact society mapping"
            ),
        }
    }
}

impl std::error::Error for ReraServingProjectionError {}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::assets::{
        claims_from_source_records, ReraReceiptKind, ReraSourceRecord, ReraSourceRecordKind,
    };
    use crate::serving::{read_rera_evidence_parquet, write_rera_evidence_parquet};

    use super::*;

    #[test]
    fn projection_keeps_exact_mapping_series_lineage_and_privacy_gate() {
        let records = fixture_source_records("catalog_project_key_exact", 1.0);
        let mut claims = claims_from_source_records(&records).unwrap();
        let mut restricted = claims[0].clone();
        restricted.claim_id = "restricted-claim".to_string();
        restricted.visibility = ReraClaimVisibility::Restricted;
        claims.push(restricted);
        let receipts = vec![fixture_receipt()];

        let projected = project_rera_evidence(&records, &claims, &receipts).unwrap();

        assert_eq!(projected.len(), 1);
        let record = &projected[0];
        assert_eq!(record.society_id, "society:fixture");
        assert_eq!(record.registration_ids, vec!["registration:fixture"]);
        assert!(record
            .claims
            .iter()
            .all(|claim| claim.claim_id != "restricted-claim"));
        assert_eq!(record.series.len(), 1);
        assert_eq!(record.series[0].points.len(), 1);
        assert_eq!(record.series[0].points[0].quarter.as_deref(), Some("Q1"));
        assert_eq!(record.series[0].points[0].booked_units, Some(837.0));
        assert_eq!(record.source_index.len(), 1);
        assert!(record
            .coverage
            .iter()
            .any(|coverage| coverage.source_section == "registration_relation"));

        let bytes = write_rera_evidence_parquet(&projected).unwrap();
        assert_eq!(read_rera_evidence_parquet(&bytes).unwrap(), projected);
    }

    #[test]
    fn projection_rejects_non_exact_society_resolution() {
        let records = fixture_source_records("name_similarity", 0.8);
        let claims = claims_from_source_records(&records).unwrap();

        assert!(matches!(
            project_rera_evidence(&records, &claims, &[fixture_receipt()]),
            Err(ReraServingProjectionError::UnsafeRegistrationRelation(_))
        ));
    }

    fn fixture_source_records(method: &str, confidence: f64) -> Vec<ReraSourceRecord> {
        let observed_at = Utc.with_ymd_and_hms(2026, 8, 9, 10, 30, 0).unwrap();
        vec![
            ReraSourceRecord {
                record_id: "relation-record".to_string(),
                kind: ReraSourceRecordKind::RegistrationRelation,
                registration_id: "registration:fixture".to_string(),
                normalized_registration_number: "PRM/FIXTURE".to_string(),
                receipt_id: "receipt:fixture".to_string(),
                capture_id: "capture:fixture".to_string(),
                source_locator: "listing[0]".to_string(),
                raw_label: "Exact catalog match".to_string(),
                raw_value: serde_json::json!({
                    "entity_id": "society:fixture",
                    "entity_type": "society",
                    "resolution_method": method,
                    "resolution_confidence": confidence,
                })
                .to_string(),
                observed_at,
                effective_at: None,
                filing_at: None,
                parser_version: "fixture.v1".to_string(),
            },
            ReraSourceRecord {
                record_id: "qpr-record".to_string(),
                kind: ReraSourceRecordKind::QuarterlyProgress,
                registration_id: "registration:fixture".to_string(),
                normalized_registration_number: "PRM/FIXTURE".to_string(),
                receipt_id: "receipt:fixture".to_string(),
                capture_id: "capture:fixture".to_string(),
                source_locator: "#qpr/q1".to_string(),
                raw_label: "Quarterly inventory totals".to_string(),
                raw_value: r#"{"quarter":"Q1","financial_year":"2026-27","tower_count":1,"total_units":970,"booked_units":837,"unsold_units":133}"#.to_string(),
                observed_at,
                effective_at: None,
                filing_at: Some("2026-07-13".to_string()),
                parser_version: "fixture.v1".to_string(),
            },
        ]
    }

    fn fixture_receipt() -> ReraReceiptRecord {
        ReraReceiptRecord {
            receipt_id: "receipt:fixture".to_string(),
            capture_id: "capture:fixture".to_string(),
            kind: ReraReceiptKind::ProjectDetail,
            source_url: "https://rera.example/project".to_string(),
            content_type: "text/html".to_string(),
            content_sha256: "fixture".to_string(),
            body_key: "raw/rera/fixture/body".to_string(),
            captured_at: Utc.with_ymd_and_hms(2026, 8, 9, 10, 30, 0).unwrap(),
            registration_id: Some("registration:fixture".to_string()),
            normalized_registration_number: Some("PRM/FIXTURE".to_string()),
            parent_receipt_id: None,
            crawl_run_id: "fixture".to_string(),
        }
    }
}
