use serde_json::Value;

#[test]
fn dag_config_does_not_generate_property_summaries() {
    let manifest = include_str!("../../app/config/dag/manifest.json");
    let registry = include_str!("../../app/config/dag/asset_registry.json");
    let facts = include_str!("../../app/config/dag/fact_registry.json");

    for document in [manifest, registry, facts] {
        assert!(!document.contains("generated_context_summaries"));
        assert!(!document.contains("generated_context_summary"));
    }

    let registry: Value = serde_json::from_str(registry).expect("asset registry json");
    let kg = registry["assets"]
        .as_array()
        .expect("assets array")
        .iter()
        .find(|asset| asset["id"] == "kg_society_view")
        .expect("kg society view asset");
    assert!(!kg["dependencies"]
        .as_array()
        .expect("dependencies array")
        .iter()
        .any(|dependency| dependency == "generated_context_summaries"));
}
