use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::knowledge::FactValue;
use crate::routes::enrichment::society_node_id;
use crate::serving::{LoadedServingBundle, ServingFactRecord};

const GENERATED_CONTEXT_SUMMARY_FACT_KEY: &str = "generated_context_summary";
const GENERATED_CONTEXT_SUMMARY_METADATA_FACT_KEY: &str = "generated_context_summary_metadata";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextClause {
    pub text: String,
    pub traversal: Vec<String>,
    pub target_entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polarity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextCategoryGroup {
    pub id: String,
    pub label: String,
    pub items: Vec<EntityContextClause>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextResponse {
    pub anchor_entity_id: String,
    pub summary_paragraph: String,
    pub clauses: Vec<EntityContextClause>,
    pub category_groups: Vec<EntityContextCategoryGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learned_at: Option<String>,
}

pub fn compose_entity_context(
    anchor_entity_id: &str,
    bundle: &LoadedServingBundle,
) -> Option<EntityContextResponse> {
    context_entity_candidates(anchor_entity_id, bundle)
        .into_iter()
        .find_map(|entity_id| {
            let summary = latest_summary_fact(bundle, &entity_id)?;
            generated_summary_is_servable(bundle, &entity_id).then_some((entity_id, summary))
        })
        .and_then(|(entity_id, summary)| {
            let text = fact_text(&summary.value)?;
            Some(EntityContextResponse {
                anchor_entity_id: entity_id,
                summary_paragraph: text,
                clauses: Vec::new(),
                category_groups: Vec::new(),
                source_type: Some(summary.source_type.clone()),
                confidence: Some(summary.confidence),
                learned_at: Some(summary.learned_at.to_rfc3339()),
            })
        })
}

pub fn society_anchor_for_property_slug(
    property_slug: &str,
    bundle: &LoadedServingBundle,
) -> Option<String> {
    let property_anchor = if property_slug.starts_with("property:") {
        property_slug.to_string()
    } else {
        format!("property:{property_slug}")
    };
    linked_society_anchor(&property_anchor, bundle).or_else(|| {
        let society_guess = society_node_id(property_slug.trim_start_matches("discovered-"));
        entity_exists_or_has_facts(bundle, &society_guess).then_some(society_guess)
    })
}

fn context_entity_candidates(anchor_entity_id: &str, bundle: &LoadedServingBundle) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_if_known(&mut candidates, bundle, anchor_entity_id);

    if anchor_entity_id.starts_with("property:") {
        if let Some(society_id) = linked_society_anchor(anchor_entity_id, bundle) {
            push_unique(&mut candidates, society_id);
        }
    }

    candidates
}

fn linked_society_anchor(property_anchor: &str, bundle: &LoadedServingBundle) -> Option<String> {
    bundle
        .graph_index
        .walk_out(property_anchor, &["in_society"], 1)
        .first()
        .map(|step| step.to_entity_id.clone())
}

fn push_unique_if_known(
    candidates: &mut Vec<String>,
    bundle: &LoadedServingBundle,
    entity_id: &str,
) {
    if entity_exists_or_has_facts(bundle, entity_id) {
        push_unique(candidates, entity_id.to_string());
    }
}

fn push_unique(candidates: &mut Vec<String>, entity_id: String) {
    if !candidates.iter().any(|existing| existing == &entity_id) {
        candidates.push(entity_id);
    }
}

fn entity_exists_or_has_facts(bundle: &LoadedServingBundle, entity_id: &str) -> bool {
    bundle
        .entities
        .iter()
        .any(|entity| entity.entity_id == entity_id)
        || bundle.fact_index.entity(entity_id).is_some()
}

fn latest_summary_fact<'a>(
    bundle: &'a LoadedServingBundle,
    entity_id: &str,
) -> Option<&'a ServingFactRecord> {
    latest_fact(bundle, entity_id, GENERATED_CONTEXT_SUMMARY_FACT_KEY)
        .filter(|fact| fact_text(&fact.value).is_some())
}

fn generated_summary_is_servable(bundle: &LoadedServingBundle, entity_id: &str) -> bool {
    let Some(metadata) = latest_fact(
        bundle,
        entity_id,
        GENERATED_CONTEXT_SUMMARY_METADATA_FACT_KEY,
    ) else {
        return false;
    };
    let Some(text) = fact_text(&metadata.value) else {
        return false;
    };
    let Ok(payload) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    summary_metadata_is_passed(&payload) && summary_metadata_provider_is_servable(&payload)
}

fn summary_metadata_is_passed(payload: &Value) -> bool {
    payload
        .get("quality_status")
        .and_then(Value::as_str)
        .map(|status| status.eq_ignore_ascii_case("passed"))
        .unwrap_or(true)
}

fn summary_metadata_provider_is_servable(payload: &Value) -> bool {
    payload
        .get("provider")
        .and_then(Value::as_str)
        .map(|provider| !provider.eq_ignore_ascii_case("mock"))
        .unwrap_or(false)
}

fn latest_fact<'a>(
    bundle: &'a LoadedServingBundle,
    entity_id: &str,
    fact_key: &str,
) -> Option<&'a ServingFactRecord> {
    bundle
        .fact_index
        .entity(entity_id)?
        .facts
        .iter()
        .filter(|fact| fact.fact_key == fact_key)
        .max_by_key(|fact| fact.learned_at)
}

fn fact_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_generated_summary_metadata_is_not_servable() {
        let metadata = serde_json::json!({
            "provider": "mock",
            "quality_status": "passed"
        });

        assert!(summary_metadata_is_passed(&metadata));
        assert!(!summary_metadata_provider_is_servable(&metadata));
    }

    #[test]
    fn real_generated_summary_metadata_is_servable_when_passed() {
        let metadata = serde_json::json!({
            "provider": "openai-compatible",
            "quality_status": "passed"
        });

        assert!(summary_metadata_is_passed(&metadata));
        assert!(summary_metadata_provider_is_servable(&metadata));
    }

    #[test]
    fn generated_summary_metadata_without_provider_is_not_servable() {
        let metadata = serde_json::json!({
            "quality_status": "passed"
        });

        assert!(summary_metadata_is_passed(&metadata));
        assert!(!summary_metadata_provider_is_servable(&metadata));
    }
}
