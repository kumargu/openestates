use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dag_config::ui_surfaces_config;
use crate::knowledge::FactValue;
use crate::serving::{DerivedEvidence, EvidenceId, EvidenceRef, SourceObservation};
use crate::state::SearchRuntimeSnapshot;

use super::tokens::{decode_signed, encode_signed};

const PROOF_TOKEN_PURPOSE: &str = "search-proof-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofDestination {
    pub surface_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    pub kind: String,
    pub target_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofTokenIdentity {
    version: u32,
    snapshot_identity: String,
    semantic_fingerprint: String,
    property_id: String,
    branch_id: String,
    predicate_id: String,
    subject_entity_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_entity_id: Option<String>,
    fact_key: String,
    relation: String,
    evidence_refs: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    destination: Option<ProofDestination>,
}

#[derive(Debug, Clone)]
pub struct ProofIssueRequest<'a> {
    pub snapshot_identity: &'a str,
    pub semantic_fingerprint: &'a str,
    pub property_id: &'a str,
    pub branch_id: &'a str,
    pub predicate_id: &'a str,
    pub subject_entity_id: &'a str,
    pub target_entity_id: Option<&'a str>,
    pub fact_key: &'a str,
    pub relation: &'a str,
    pub evidence_refs: &'a [EvidenceRef],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofResolution {
    pub contract_version: u32,
    pub snapshot_identity: String,
    pub semantic_fingerprint: String,
    pub property_id: String,
    pub branch_id: String,
    pub predicate_id: String,
    pub subject_entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    pub fact_key: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<FactValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub source_observations: Vec<ResolvedSourceObservation>,
    pub derivation_chain: Vec<DerivedEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Value>,
    pub resolution_status: ProofResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<ProofDestination>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSourceObservation {
    pub observation_id: String,
    pub provider: String,
    pub provider_observation_id: String,
    pub subject_entity_id: String,
    pub observed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub asset_lineage: Vec<String>,
}

impl From<&SourceObservation> for ResolvedSourceObservation {
    fn from(observation: &SourceObservation) -> Self {
        Self {
            observation_id: observation.observation_id.as_str().to_string(),
            provider: observation.provider.clone(),
            provider_observation_id: observation.provider_observation_id.clone(),
            subject_entity_id: observation.subject_entity_id.clone(),
            observed_at: observation.observed_at,
            source_url: observation.source_url.clone(),
            asset_lineage: observation.asset_lineage.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProofResolutionStatus {
    Resolved,
}

/// Server-owned focus passed from proof resolution to a configured detail
/// surface. It is serializable in the scene but cannot be client-authored.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedProofFocus {
    pub surface_id: String,
    pub layer_id: String,
    pub fact_key: String,
    pub destination_kind: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofResolutionError {
    InvalidToken,
    StaleSnapshot,
    WrongProperty,
    WrongSubject,
    MissingEvidence,
    DestinationMismatch,
}

impl ProofResolutionError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidToken => "invalid_proof_token",
            Self::StaleSnapshot => "stale_proof_snapshot",
            Self::WrongProperty => "proof_property_mismatch",
            Self::WrongSubject => "proof_subject_mismatch",
            Self::MissingEvidence => "proof_evidence_missing",
            Self::DestinationMismatch => "proof_destination_mismatch",
        }
    }
}

pub fn issue_proof_token(request: ProofIssueRequest<'_>) -> Result<String, String> {
    if request.evidence_refs.is_empty()
        || request.snapshot_identity.trim().is_empty()
        || request.semantic_fingerprint.trim().is_empty()
        || request.property_id.trim().is_empty()
        || request.branch_id.trim().is_empty()
        || request.predicate_id.trim().is_empty()
        || request.subject_entity_id.trim().is_empty()
        || request.fact_key.trim().is_empty()
        || request.relation.trim().is_empty()
    {
        return Err("proof identity requires evidence".to_string());
    }
    for evidence in request.evidence_refs {
        evidence
            .validate_for(request.subject_entity_id, request.snapshot_identity)
            .map_err(|error| format!("invalid proof evidence identity: {error}"))?;
    }
    let identity = ProofTokenIdentity {
        version: 1,
        snapshot_identity: request.snapshot_identity.to_string(),
        semantic_fingerprint: request.semantic_fingerprint.to_string(),
        property_id: request.property_id.to_string(),
        branch_id: request.branch_id.to_string(),
        predicate_id: request.predicate_id.to_string(),
        subject_entity_id: request.subject_entity_id.to_string(),
        target_entity_id: request.target_entity_id.map(str::to_string),
        fact_key: request.fact_key.to_string(),
        relation: request.relation.to_string(),
        evidence_refs: request.evidence_refs.to_vec(),
        destination: proof_destination_for_fact_key(request.fact_key),
    };
    encode_signed(
        PROOF_TOKEN_PURPOSE,
        &identity,
        crate::security::security_tuning()
            .search_journey
            .proof_token_max_bytes,
    )
}

pub fn resolve_proof_token(
    snapshot: &SearchRuntimeSnapshot,
    proof_token: &str,
    expected_property_id: Option<&str>,
) -> Result<ProofResolution, ProofResolutionError> {
    let identity: ProofTokenIdentity = decode_signed(
        PROOF_TOKEN_PURPOSE,
        proof_token,
        crate::security::security_tuning()
            .search_journey
            .proof_token_max_bytes,
    )
    .map_err(|_| ProofResolutionError::InvalidToken)?;
    if identity.version != 1
        || identity.snapshot_identity != snapshot.version_key.serving_bundle_version
    {
        return Err(ProofResolutionError::StaleSnapshot);
    }
    if expected_property_id.is_some_and(|expected| expected != identity.property_id) {
        return Err(ProofResolutionError::WrongProperty);
    }
    let property = snapshot
        .property_by_id
        .get(&identity.property_id)
        .and_then(|index| snapshot.properties.get(*index))
        .ok_or(ProofResolutionError::WrongProperty)?;
    if !subject_belongs_to_property(snapshot, property, &identity.subject_entity_id) {
        return Err(ProofResolutionError::WrongSubject);
    }
    if proof_destination_for_fact_key(&identity.fact_key) != identity.destination {
        return Err(ProofResolutionError::DestinationMismatch);
    }

    let mut value = None;
    let mut unit = None;
    let mut observations = Vec::<SourceObservation>::new();
    let mut derivations = Vec::<DerivedEvidence>::new();
    for evidence in &identity.evidence_refs {
        evidence
            .validate_for(&identity.subject_entity_id, &identity.snapshot_identity)
            .map_err(|_| ProofResolutionError::WrongSubject)?;
        match &evidence.evidence_id {
            EvidenceId::Observation(observation_id) => {
                let fact = snapshot
                    .bundle
                    .evidence_index
                    .fact(observation_id, &identity.fact_key)
                    .ok_or(ProofResolutionError::MissingEvidence)?;
                if fact.entity_id != evidence.subject_entity_id {
                    return Err(ProofResolutionError::WrongSubject);
                }
                value.get_or_insert_with(|| fact.value.clone());
                let observation = snapshot
                    .bundle
                    .evidence_index
                    .observation(observation_id)
                    .ok_or(ProofResolutionError::MissingEvidence)?;
                push_unique_observation(&mut observations, observation);
            }
            EvidenceId::Derivation(derivation_id) => {
                let recomputed;
                let derivation = if let Some(derivation) =
                    snapshot.bundle.evidence_index.derivation(derivation_id)
                {
                    derivation
                } else {
                    recomputed = recompute_search_derivation(snapshot, &identity)
                        .ok_or(ProofResolutionError::MissingEvidence)?;
                    if recomputed.derivation_id != *derivation_id {
                        return Err(ProofResolutionError::MissingEvidence);
                    }
                    &recomputed
                };
                if derivation.subject_entity_id != identity.subject_entity_id
                    || derivation.target_entity_id != identity.target_entity_id
                {
                    return Err(ProofResolutionError::WrongSubject);
                }
                value = derivation.value.map(FactValue::Numeric).or(value);
                unit = derivation.unit.clone().or(unit);
                collect_derivation_sources(snapshot, derivation, &mut observations)?;
                derivations.push(derivation.clone());
            }
        }
    }
    if observations.is_empty() {
        return Err(ProofResolutionError::MissingEvidence);
    }
    let geometry = proof_geometry(snapshot, &identity);
    let target_label = identity.target_entity_id.as_deref().and_then(|target| {
        snapshot
            .entity_by_id
            .get(target)
            .and_then(|index| snapshot.bundle.entities.get(*index))
            .map(|entity| entity.name.clone())
    });
    Ok(ProofResolution {
        contract_version: 1,
        snapshot_identity: identity.snapshot_identity,
        semantic_fingerprint: identity.semantic_fingerprint,
        property_id: identity.property_id,
        branch_id: identity.branch_id,
        predicate_id: identity.predicate_id,
        subject_entity_id: identity.subject_entity_id,
        target_entity_id: identity.target_entity_id,
        target_label,
        fact_key: identity.fact_key,
        relation: identity.relation,
        value,
        unit,
        source_observations: observations
            .iter()
            .map(ResolvedSourceObservation::from)
            .collect(),
        derivation_chain: derivations,
        geometry,
        resolution_status: ProofResolutionStatus::Resolved,
        destination: identity.destination,
    })
}

fn recompute_search_derivation(
    snapshot: &SearchRuntimeSnapshot,
    identity: &ProofTokenIdentity,
) -> Option<DerivedEvidence> {
    let target = identity.target_entity_id.as_deref()?;
    let distance = snapshot.bundle.spatial_index.distance_between(
        &identity.subject_entity_id,
        target,
        &identity.snapshot_identity,
    )?;
    let unit = Some(
        if distance.metric.contains("distance") {
            "km"
        } else {
            "boolean"
        }
        .to_string(),
    );
    DerivedEvidence::new(
        &identity.snapshot_identity,
        &identity.subject_entity_id,
        Some(target.to_string()),
        &identity.relation,
        distance.metric,
        Some(distance.distance_km),
        unit,
        "spatial-evaluator-v2",
        distance.confidence,
        distance.evidence_refs,
    )
    .ok()
}

pub fn resolved_proof_focus(resolution: &ProofResolution) -> Option<ResolvedProofFocus> {
    let destination = resolution.destination.as_ref()?;
    let layer_id = destination.layer_id.clone()?;
    let matched_value = resolution.value.as_ref().map(fact_value_display);
    let distance_m = match (&resolution.value, resolution.unit.as_deref()) {
        (Some(FactValue::Numeric(value)), Some("km")) if value.is_finite() && *value >= 0.0 => {
            Some((value * 1000.0).round() as u32)
        }
        (Some(FactValue::Numeric(value)), Some("m")) if value.is_finite() && *value >= 0.0 => {
            Some(value.round() as u32)
        }
        _ => None,
    };
    Some(ResolvedProofFocus {
        surface_id: destination.surface_id.clone(),
        layer_id,
        fact_key: resolution.fact_key.clone(),
        destination_kind: destination.kind.clone(),
        target_id: destination.target_id.clone(),
        entity_id: resolution.target_entity_id.clone(),
        feature_id: None,
        receipt_id: None,
        matched_label: resolution.target_label.clone(),
        matched_value,
        distance_m,
    })
}

pub fn proof_destination_for_fact_key(fact_key: &str) -> Option<ProofDestination> {
    let config = ui_surfaces_config().ok()?;
    for surface in &config.surfaces {
        let Some(handoff) = surface.proof_handoff.as_ref() else {
            continue;
        };
        if let Some(layer) = surface.scene.as_ref().and_then(|scene| {
            scene.layers.iter().find(|layer| {
                layer
                    .fact_keys
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(fact_key))
            })
        }) {
            return Some(ProofDestination {
                surface_id: surface.id.clone(),
                layer_id: Some(layer.id.clone()),
                kind: handoff.kind.clone(),
                target_id: handoff.target_id.clone(),
            });
        }
        if handoff
            .fact_keys
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(fact_key))
        {
            return Some(ProofDestination {
                surface_id: surface.id.clone(),
                layer_id: None,
                kind: handoff.kind.clone(),
                target_id: handoff.target_id.clone(),
            });
        }
    }
    None
}

fn subject_belongs_to_property(
    snapshot: &SearchRuntimeSnapshot,
    property: &crate::models::Property,
    subject: &str,
) -> bool {
    let mut allowed = HashSet::<String>::from([
        property.id.clone(),
        format!("property:{}", property.id),
        format!("society:{}", property.society_id),
        format!("area:{}", property.area_id),
    ]);
    for candidate in [
        snapshot
            .search_index
            .society_entity_id_for_property(&property.id),
        snapshot
            .search_index
            .area_entity_id_for_property(&property.id),
        snapshot
            .search_index
            .builder_entity_id_for_property(&property.id),
    ]
    .into_iter()
    .flatten()
    {
        allowed.insert(candidate.to_string());
    }
    allowed.contains(subject)
}

fn collect_derivation_sources(
    snapshot: &SearchRuntimeSnapshot,
    derivation: &DerivedEvidence,
    observations: &mut Vec<SourceObservation>,
) -> Result<(), ProofResolutionError> {
    for input in &derivation.input_evidence {
        match &input.evidence_id {
            EvidenceId::Observation(id) => {
                let observation = snapshot
                    .bundle
                    .evidence_index
                    .observation(id)
                    .ok_or(ProofResolutionError::MissingEvidence)?;
                push_unique_observation(observations, observation);
            }
            EvidenceId::Derivation(id) => {
                let nested = snapshot
                    .bundle
                    .evidence_index
                    .derivation(id)
                    .ok_or(ProofResolutionError::MissingEvidence)?;
                collect_derivation_sources(snapshot, nested, observations)?;
            }
        }
    }
    Ok(())
}

fn push_unique_observation(values: &mut Vec<SourceObservation>, observation: &SourceObservation) {
    if !values
        .iter()
        .any(|existing| existing.observation_id == observation.observation_id)
    {
        values.push(observation.clone());
    }
}

fn proof_geometry(
    snapshot: &SearchRuntimeSnapshot,
    identity: &ProofTokenIdentity,
) -> Option<Value> {
    let entity_id = identity
        .target_entity_id
        .as_deref()
        .unwrap_or(&identity.subject_entity_id);
    let (latitude, longitude) = snapshot
        .bundle
        .spatial_index
        .point_for_entity(entity_id)
        .map(|point| (point.latitude, point.longitude))
        .or_else(|| {
            snapshot
                .bundle
                .spatial_index
                .geometry()
                .representative_coordinate(entity_id)
                .map(|(latitude, longitude, _)| (latitude, longitude))
        })?;
    Some(serde_json::json!({
        "type": "Point",
        "coordinates": [longitude, latitude],
    }))
}

fn fact_value_display(value: &FactValue) -> String {
    match value {
        FactValue::Numeric(value) => value.to_string(),
        FactValue::Text(value) => value.clone(),
        FactValue::Bool(value) => value.to_string(),
        FactValue::Tags(values) => values.join(", "),
        FactValue::Score { value, .. } => value.to_string(),
    }
}
