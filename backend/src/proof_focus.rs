use serde::{Deserialize, Serialize};

/// Generic bridge from search proof to a richer detail surface.
///
/// This is intentionally about evidence handles, not buyer domains. Search can
/// say which configured surface/layer/fact proved the match, and detail scenes
/// can focus that proof without parsing query text or branching on categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProofFocus {
    pub surface_id: String,
    pub layer_id: String,
    pub fact_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
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
    pub requested_constraint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<u32>,
    pub reason: String,
}
