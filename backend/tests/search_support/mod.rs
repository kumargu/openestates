use std::collections::HashMap;

use backend::knowledge::FactValue;
use backend::models::Property;
use backend::search::evaluation::InventoryOption;
use backend::search::{SearchEvaluationContext, VerifiedMatch};
use backend::serving::{EvidenceRef, ServingFactRecord, SourceObservation};
use chrono::{TimeZone, Utc};

pub const SNAPSHOT_IDENTITY: &str = "search-contract-fixture";

pub fn inventory_options(properties: &[Property]) -> HashMap<String, InventoryOption> {
    properties
        .iter()
        .filter(|property| property.bhk > 0)
        .map(|property| {
            let society_id = format!("society:{}", property.society_id);
            let observation = SourceObservation::new(
                "SearchContractFixture",
                property.id.clone(),
                society_id.clone(),
                Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                Some(format!("https://example.test/{}", property.id)),
                vec!["asset:search-contract-fixture/v1".to_string()],
            )
            .unwrap();
            let exact_price = (property.price > 0).then_some(property.price);
            (
                property.id.clone(),
                InventoryOption {
                    property_id: property.id.clone(),
                    society_id,
                    bhk: Some(property.bhk),
                    price_min: property.price_min.or(exact_price),
                    price_max: property.price_max.or(exact_price),
                    size_sqft: (property.super_builtup_sqft > 0)
                        .then_some(property.super_builtup_sqft),
                    evidence_reference: Some(EvidenceRef::for_observation(
                        SNAPSHOT_IDENTITY,
                        &observation,
                    )),
                },
            )
        })
        .collect()
}

pub fn inventory_context(
    options: &HashMap<String, InventoryOption>,
) -> SearchEvaluationContext<'_> {
    static SPATIAL_MATCHES: std::sync::OnceLock<HashMap<String, Vec<VerifiedMatch>>> =
        std::sync::OnceLock::new();
    SearchEvaluationContext {
        options,
        spatial_matches: SPATIAL_MATCHES.get_or_init(HashMap::new),
        snapshot_identity: SNAPSHOT_IDENTITY,
    }
}

#[allow(dead_code)]
pub fn inventory_facts(properties: &[Property]) -> Vec<ServingFactRecord> {
    properties
        .iter()
        .filter(|property| property.bhk > 0)
        .map(|property| {
            let entity_id = format!("society:{}", property.society_id);
            let observation = SourceObservation::new(
                "SearchContractFixture",
                property.id.clone(),
                entity_id.clone(),
                Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                Some(format!("https://example.test/{}", property.id)),
                vec!["asset:search-contract-fixture/v1".to_string()],
            )
            .unwrap();
            let value = serde_json::json!({
                "bhk": property.bhk,
                "price": property.price,
                "area_sqft": property.super_builtup_sqft,
            })
            .to_string();
            ServingFactRecord {
                entity_id,
                fact_key: "controlled_inventory_option".to_string(),
                value_type: "text".to_string(),
                value_text: Some(value.clone()),
                value: FactValue::Text(value),
                confidence: 1.0,
                source_type: "ControlledInventoryReceipt".to_string(),
                source_url: observation.source_url.clone(),
                model: None,
                skill_id: Some("search_contract_fixture".to_string()),
                learned_at: observation.observed_at,
                observation: Some(observation),
            }
        })
        .collect()
}
