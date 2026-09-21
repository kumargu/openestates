use crate::dag_config::ui_surfaces_config;
use crate::knowledge::FactValue;
use crate::search::proof::{ProofResolution, ResolvedSourceObservation};
use crate::serving::DerivedEvidence;
use crate::state::SearchRuntimeSnapshot;
use serde::{Deserialize, Serialize};

#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofDestination {
    pub surface_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub layer_id: Option<String>,
    pub kind: String,
    pub target_id: String,
}

/// Server-owned focus passed from proof resolution to a configured detail
/// surface. It is serializable in the scene but cannot be client-authored.
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedProofFocus {
    /// Exact source identities; display text is never a substitute for these.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub observation_ids: Vec<String>,
    pub surface_id: String,
    pub layer_id: String,
    pub fact_key: String,
    pub destination_kind: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub feature_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub receipt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub matched_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub matched_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub distance_m: Option<u32>,
    #[serde(skip)]
    pub scene_evidence: Option<ResolvedSceneEvidence>,
}

/// Exact derived evidence for a scene overlay. Never reconstructed from a
/// nearby display row, and never accepted from a client-authored focus.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSceneEvidence {
    pub coordinates: [f64; 2],
    pub derivation: DerivedEvidence,
    pub source: ResolvedSourceObservation,
}

pub fn resolved_proof_focus(
    snapshot: &SearchRuntimeSnapshot,
    resolution: &ProofResolution,
) -> Option<ResolvedProofFocus> {
    let destination = proof_destination_for_fact_key(&resolution.fact_key)?;
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
    let scene_evidence = distance_m.and_then(|_| {
        let target = resolution.target_entity_id.as_deref()?;
        let rows = snapshot.bundle.fact_index.entity(target)?;
        let source = resolution.source_observations.iter().find(|source| {
            rows.facts.iter().any(|fact| {
                fact.observation
                    .as_ref()
                    .is_some_and(|observation| observation.observation_id == source.observation_id)
            })
        })?;
        let coordinates = resolution
            .geometry
            .as_ref()?
            .get("coordinates")?
            .as_array()?;
        Some(ResolvedSceneEvidence {
            coordinates: [
                coordinates.first()?.as_f64()?,
                coordinates.get(1)?.as_f64()?,
            ],
            derivation: resolution.derivation_chain.first()?.clone(),
            source: source.clone(),
        })
    });
    Some(ResolvedProofFocus {
        observation_ids: resolution
            .source_observations
            .iter()
            .map(|source| source.observation_id.as_str().to_string())
            .collect(),
        scene_evidence,
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

fn fact_value_display(value: &FactValue) -> String {
    match value {
        FactValue::Numeric(value) => value.to_string(),
        FactValue::Text(value) => value.clone(),
        FactValue::Bool(value) => value.to_string(),
        FactValue::Tags(values) => values.join(", "),
        FactValue::Score { value, .. } => value.to_string(),
    }
}
