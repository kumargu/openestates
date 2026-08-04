use std::sync::Arc;

use arc_swap::ArcSwap;
use backend::assets::KgSocietyViewMaterializer;
use backend::data_loader::runtime_snapshot_from_serving_bundle;
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::LakeStore;
use backend::serving::{SearchServingBundleMaterializer, ServingBundleLoader};
use backend::state::{SearchCacheKey, SearchResponseCache};
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

    let repeated_hits = loaded
        .recall_index
        .search("whitefield greenery trees", 5)
        .unwrap();
    assert_eq!(
        repeated_hits, hits,
        "repeated queries should be stable through the reusable Tantivy reader"
    );
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

#[tokio::test]
async fn runtime_snapshot_reload_is_atomic() {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();
    let graph = mock_graph();

    let kg_v1 = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "bundle-v1", Vec::new(), Vec::new())
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_v1, "bundle-v1")
        .await
        .unwrap();
    let bundle_v1 = Arc::new(
        ServingBundleLoader::new(lake.clone(), cache_root.path())
            .load_current_search_bundle()
            .await
            .unwrap()
            .unwrap(),
    );
    let snapshot_v1 = runtime_snapshot_from_serving_bundle(bundle_v1);
    let runtime = ArcSwap::from_pointee(snapshot_v1);
    let old_request_snapshot = runtime.load_full();

    let kg_v2 = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "bundle-v2", Vec::new(), Vec::new())
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_v2, "bundle-v2")
        .await
        .unwrap();
    let bundle_v2 = Arc::new(
        ServingBundleLoader::new(lake, cache_root.path())
            .load_current_search_bundle()
            .await
            .unwrap()
            .unwrap(),
    );
    let snapshot_v2 = runtime_snapshot_from_serving_bundle(bundle_v2);
    runtime.store(Arc::new(snapshot_v2));

    assert_eq!(
        old_request_snapshot.version_key.serving_bundle_version,
        "bundle-v1"
    );
    assert_eq!(
        runtime.load_full().version_key.serving_bundle_version,
        "bundle-v2"
    );
}

#[tokio::test]
async fn dag_promotion_reload_smoke_updates_snapshot_and_cache_key() {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();
    let graph = mock_graph();
    let cache = SearchResponseCache::new(8);

    let kg_v1 = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "smoke-v1", Vec::new(), Vec::new())
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_v1, "smoke-v1")
        .await
        .unwrap();
    let snapshot_v1 = runtime_snapshot_from_serving_bundle(Arc::new(
        ServingBundleLoader::new(lake.clone(), cache_root.path())
            .load_current_search_bundle()
            .await
            .unwrap()
            .unwrap(),
    ));
    let key_v1 = SearchCacheKey::new("whitefield greenery", &snapshot_v1.version_key);

    let kg_v2 = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "smoke-v2", Vec::new(), Vec::new())
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_v2, "smoke-v2")
        .await
        .unwrap();
    let snapshot_v2 = runtime_snapshot_from_serving_bundle(Arc::new(
        ServingBundleLoader::new(lake, cache_root.path())
            .load_current_search_bundle()
            .await
            .unwrap()
            .unwrap(),
    ));
    let key_v2 = SearchCacheKey::new("whitefield greenery", &snapshot_v2.version_key);

    assert_eq!(snapshot_v1.version_key.serving_bundle_version, "smoke-v1");
    assert_eq!(snapshot_v2.version_key.serving_bundle_version, "smoke-v2");
    assert_ne!(key_v1, key_v2);
    cache.clear().await;
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
