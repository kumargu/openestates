use serde_json::Value;

#[test]
fn entity_context_contract_uses_generated_summary_asset() {
    let context: Value =
        serde_json::from_str(include_str!("../../app/config/dag/entity_context.json"))
            .expect("entity context json");
    let registry: Value =
        serde_json::from_str(include_str!("../../app/config/dag/asset_registry.json"))
            .expect("asset registry json");
    let facts: Value =
        serde_json::from_str(include_str!("../../app/config/dag/fact_registry.json"))
            .expect("fact registry json");

    let generated = &context["generated_summary"];
    assert_eq!(generated["asset_id"], "generated_context_summaries");
    assert_eq!(generated["summary_fact_key"], "generated_context_summary");
    assert_eq!(
        generated["metadata_fact_key"],
        "generated_context_summary_metadata"
    );
    assert_eq!(generated["quality_status_required"], "passed");
    assert_eq!(generated["provider_required"], true);
    assert!(generated["disallowed_providers"]
        .as_array()
        .expect("disallowed providers array")
        .iter()
        .any(|provider| provider == "mock"));

    let assets = registry["assets"].as_array().expect("assets array");
    let kg = assets
        .iter()
        .find(|asset| asset["id"] == "kg_society_view")
        .expect("kg society view asset");
    assert!(kg["dependencies"]
        .as_array()
        .expect("dependencies array")
        .iter()
        .any(|dependency| dependency == "generated_context_summaries"));
    for dependency in kg["dependencies"].as_array().expect("dependencies array") {
        assert!(
            !dependency.as_str().unwrap_or_default().contains("legacy"),
            "kg_society_view should not depend on legacy assets"
        );
    }
    assert!(kg["dependency_fan_in"]
        .as_array()
        .expect("fan in array")
        .iter()
        .any(|rule| rule["dependency"] == "generated_context_summaries"
            && rule["policy"] == "all_current_partitions"));

    let fact_entries = facts["facts"].as_array().expect("facts array");
    for fact_key in [
        "generated_context_summary",
        "generated_context_summary_metadata",
    ] {
        assert!(
            fact_entries
                .iter()
                .any(|entry| entry["fact_key"] == fact_key),
            "{fact_key} must be registered"
        );
    }
}
