use crate::serving::EvidenceRef;
use serde::{Deserialize, Serialize};

/// A measurement is never a display number: its basis, scope and receipt travel together.
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub maximum: Option<f64>,
    pub unit: String,
    pub basis: String,
    pub entity_id: String,
    pub evidence: EvidenceRef,
}
