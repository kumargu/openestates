use std::collections::HashSet;

use serde_json::Value;

#[test]
fn entity_context_relations_are_registered_in_ontology() {
    let ontology: Value = serde_json::from_str(include_str!("../../app/config/dag/ontology.json"))
        .expect("ontology json");
    let context: Value =
        serde_json::from_str(include_str!("../../app/config/dag/entity_context.json"))
            .expect("entity context json");

    let ontology_edges = ontology["relations"]
        .as_array()
        .expect("ontology relations array")
        .iter()
        .filter_map(|relation| relation["edge"].as_str())
        .collect::<HashSet<_>>();
    let context_edges = context["traversal"]["edge_priority"]
        .as_array()
        .expect("edge priority array")
        .iter()
        .filter_map(|edge| edge.as_str());

    for edge in context_edges {
        assert!(
            ontology_edges.contains(edge),
            "entity_context traversal edge {edge} must exist in ontology"
        );
    }
    assert!(ontology_edges.contains("near_place"));
}

#[test]
fn entity_context_categories_have_budgets_and_generic_inputs() {
    let context: Value =
        serde_json::from_str(include_str!("../../app/config/dag/entity_context.json"))
            .expect("entity context json");
    let categories = context["categories"].as_array().expect("categories array");

    assert!(categories.len() >= 6);
    for category in categories {
        assert!(category["id"].as_str().is_some_and(|id| !id.is_empty()));
        assert!(category["max_items"]
            .as_u64()
            .is_some_and(|items| items > 0));
        let has_edges = category["edge_types"]
            .as_array()
            .is_some_and(|edges| !edges.is_empty());
        let has_facts = category["fact_keys"]
            .as_array()
            .is_some_and(|facts| !facts.is_empty());
        assert!(
            has_edges || has_facts,
            "category {:?} must be driven by generic edges or facts",
            category["id"]
        );
    }
}
