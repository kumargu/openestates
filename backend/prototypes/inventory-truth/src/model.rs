use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Preview input only. Each binding and state is an explicit fixture assertion;
// this crate does not infer physical-home identity or availability.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Confirmed,
    Likely,
    Comparable,
    Registration,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Active,
    Disappeared,
    Stale,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Home {
    pub id: String,
    pub title: String,
    pub location: String,
    pub bhk: i64,
    pub area_sqft: Option<i64>,
    pub area_basis: Option<String>,
    pub floor: Option<i64>,
    pub image: String,
    pub scenario: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub home_id: String,
    pub provider_id: String,
    pub advertisement_id: String,
    pub source_label: String,
    pub source_url: Option<String>,
    pub relation: Relation,
    pub seller: String,
    pub availability: Availability,
    pub current: bool,
    pub predecessor_id: Option<String>,
    pub amount_inr: Option<i64>,
    pub bhk: Option<i64>,
    pub area_sqft: Option<i64>,
    pub area_basis: Option<String>,
    pub floor: Option<i64>,
    pub observed_on: String,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub reliable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdentitySignal {
    pub id: String,
    pub observation_id: String,
    pub label: String,
    pub value: String,
    pub disagrees: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub kind: String,
    pub homes: Vec<Home>,
    pub observations: Vec<Observation>,
    pub signals: Vec<IdentitySignal>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub price_relation: Relation,
    pub price_availability: Availability,
    pub require_reliable_registrations: bool,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct AskRange {
    pub min_inr: i64,
    pub max_inr: i64,
    pub observation_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct HomeSummary {
    pub home: Home,
    pub ask: Option<AskRange>,
    pub advertisement_count: usize,
    pub active_count: usize,
    pub uncertain_count: usize,
    pub conflicts: Vec<IdentitySignal>,
    pub observed_on: Option<String>,
    pub price_change: Option<PriceChange>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PriceChange {
    pub observation_id: String,
    pub previous_observation_id: String,
    pub difference_inr: i64,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Advertisement {
    pub observation: Observation,
    /// Explicit predecessor order, newest first. Never sorted by timestamps.
    pub history: Vec<Observation>,
    pub identity: Vec<IdentitySignal>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct InventoryDetail {
    pub contract_version: u32,
    pub snapshot_id: String,
    pub fixture: bool,
    pub summary: HomeSummary,
    pub advertisements: Vec<Advertisement>,
    pub candidates: Vec<Advertisement>,
    pub comparables: Vec<Advertisement>,
    pub registrations: Vec<Advertisement>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct InventoryCatalog {
    pub contract_version: u32,
    pub snapshot_id: String,
    pub fixture: bool,
    pub homes: Vec<HomeSummary>,
}
