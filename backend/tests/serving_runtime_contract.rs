use backend::assets::KgSocietyViewMaterializer;
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::LakeStore;
use backend::serving::{SearchServingBundleMaterializer, ServingBundleLoader};
use chrono::Utc;
use tempfile::tempdir;

#[tokio::test]
async fn current_serving_bundle_loads_hydrates_and_recalls_entities() {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();
    let graph = mock_graph();

    let kg_materialization = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "2026-07-12T19:30Z", Vec::new(), Vec::new())
        .await
        .unwrap();
    let materialization = SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_materialization, "2026-07-12T19:30Z")
        .await
        .unwrap();
    assert_eq!(materialization.record.version, "2026-07-12T19:30Z");
    assert_eq!(
        materialization.record.parent_materializations,
        vec![kg_materialization.record.materialization_id]
    );

    let loaded = ServingBundleLoader::new(lake, cache_root.path())
        .load_current_search_bundle()
        .await
        .unwrap()
        .expect("current serving bundle should be promoted");

    assert_eq!(loaded.manifest.bundle_version, "2026-07-12T19:30Z");
    assert!(loaded.cache_dir.join("meta.json").exists());

    let hits = loaded
        .recall_index
        .search("whitefield greenery trees", 5)
        .unwrap();
    assert_eq!(hits[0].entity_id, "society:green-acre-whitefield");
}

#[tokio::test]
async fn missing_current_serving_bundle_returns_none() {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();

    let loaded = ServingBundleLoader::new(lake, cache_root.path())
        .load_current_search_bundle()
        .await
        .unwrap();

    assert!(loaded.is_none());
}

fn mock_graph() -> KnowledgeGraph {
    let mut graph = KnowledgeGraph::new();

    let mut green = Node::new(
        "society:green-acre-whitefield",
        NodeType::Society,
        "Green Acre Whitefield",
    );
    green.root_source = Some(RootSource::Rera);
    green.add_fact(fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(48_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    ));
    green.add_fact(fact(
        "resident_greenery_signal",
        FactValue::Text(
            "Residents mention many trees, calm internal roads, and open space".to_string(),
        ),
        SourceType::Reddit,
        &["greenery", "trees", "calm layout"],
    ));

    let mut dense = Node::new(
        "society:dense-tower-whitefield",
        NodeType::Society,
        "Dense Tower Whitefield",
    );
    dense.root_source = Some(RootSource::Rera);
    dense.add_fact(fact(
        "resident_traffic_signal",
        FactValue::Text("Residents complain about congestion near the gate".to_string()),
        SourceType::Reddit,
        &["traffic", "congestion"],
    ));

    graph.add_node(green);
    graph.add_node(dense);
    graph
}

fn fact(
    key: &str,
    value: FactValue,
    source_type: SourceType,
    answers_preferences: &[&str],
) -> SourcedFact {
    SourcedFact {
        key: key.to_string(),
        value,
        confidence: match source_type {
            SourceType::Rera => 1.0,
            SourceType::Reddit => 0.7,
            _ => 0.6,
        },
        source: FactSource {
            source_type,
            url: None,
            model: None,
            skill_id: None,
            triggered_by: None,
        },
        learned_at: Utc::now(),
        version: 1,
        display_template: Some(format!("{key}: {{value}}")),
        answers_preferences: answers_preferences
            .iter()
            .map(|value| value.to_string())
            .collect(),
        scoring_hint: Some(ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 1.0,
            thresholds: Vec::new(),
        }),
    }
}
