use std::collections::BTreeMap;
use std::path::PathBuf;

use backend::assets::{
    openestates_registry, AssetDagExecutionOptions, AssetDagExecutor, AssetId, AssetPartition,
    DagRunStatus, APPROACH_ROAD_GRAPH_FACTS_ASSET_ID, CANONICAL_ROAD_NODES_ASSET_ID,
    KG_SOCIETY_VIEW_ASSET_ID,
};
use backend::entity_context::compose_entity_context;
use backend::knowledge::KnowledgeGraph;
use backend::lake::LakeStoreLocation;
use backend::serving::ServingBundleLoader;
use backend::serving::SEARCH_SERVING_BUNDLE_ASSET_ID;
use chrono::Utc;
use serde_json::json;

#[tokio::test]
async fn materialize_and_dump_waterford_context() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf();
    let lake = LakeStoreLocation::from_env(&project_root)
        .expect("lake location")
        .open()
        .expect("open lake");
    let executor = AssetDagExecutor::new(openestates_registry(), lake.clone());
    let partition =
        AssetPartition::new([("dt", "2026-07-20"), ("subreddit", "bangalorerealestates")]);
    let now = Utc::now();
    let version = format!(
        "waterford-rich-context-{}",
        now.format("%Y%m%dT%H%M%SZ")
            .to_string()
            .to_ascii_lowercase()
    );
    let force_assets = vec![
        AssetId::new(CANONICAL_ROAD_NODES_ASSET_ID).expect("road nodes asset id"),
        AssetId::new(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID).expect("approach road facts asset id"),
        AssetId::new(KG_SOCIETY_VIEW_ASSET_ID).expect("kg view asset id"),
        AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("serving bundle asset id"),
    ];
    let options = AssetDagExecutionOptions::new(partition, now)
        .with_version(version.clone())
        .with_forced_assets(force_assets);
    let graph = KnowledgeGraph::new();
    let report = executor
        .execute(&graph, options)
        .await
        .expect("asset DAG execute should succeed");
    assert!(
        matches!(
            report.manifest.status,
            DagRunStatus::Succeeded | DagRunStatus::SucceededWithWarnings
        ),
        "unexpected DAG status: {:?}",
        report.manifest.status
    );

    let loader = ServingBundleLoader::new(lake.clone(), project_root.join("data/cache"));
    let bundle = loader
        .load_current_search_bundle()
        .await
        .expect("load bundle")
        .expect("current serving bundle should exist after promotion");

    let society_id = "society:prestige-waterford";
    let entity_names = bundle
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity.name.clone()))
        .collect::<BTreeMap<_, _>>();

    let graph_edges = bundle
        .edges
        .iter()
        .filter(|edge| edge.from_entity_id == society_id)
        .map(|edge| {
            json!({
                "relation": edge.edge_type,
                "to_entity_id": edge.to_entity_id,
                "to_name": entity_names.get(&edge.to_entity_id),
                "confidence": edge.confidence,
                "source_type": edge.source_type,
            })
        })
        .collect::<Vec<_>>();

    let connected_ids = bundle
        .edges
        .iter()
        .filter(|edge| edge.from_entity_id == society_id)
        .map(|edge| edge.to_entity_id.clone())
        .chain(std::iter::once(society_id.to_string()))
        .collect::<Vec<_>>();

    let mut facts_by_entity = BTreeMap::new();
    for entity_id in &connected_ids {
        let Some(rows) = bundle.fact_index.entity(entity_id) else {
            continue;
        };
        let facts = rows
            .facts
            .iter()
            .map(|fact| {
                json!({
                    "fact_key": fact.fact_key,
                    "value": fact.value,
                    "confidence": fact.confidence,
                    "source_type": fact.source_type,
                })
            })
            .collect::<Vec<_>>();
        if !facts.is_empty() {
            facts_by_entity.insert(
                entity_id.clone(),
                json!({
                    "name": entity_names.get(entity_id),
                    "facts": facts,
                }),
            );
        }
    }

    let society_facts = bundle
        .fact_index
        .entity(society_id)
        .map(|rows| {
            rows.facts
                .iter()
                .map(|fact| {
                    json!({
                        "fact_key": fact.fact_key,
                        "value": fact.value,
                        "confidence": fact.confidence,
                        "source_type": fact.source_type,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let composed = compose_entity_context(society_id, &bundle);

    let dump = json!({
        "bundle_version": bundle.manifest.bundle_version,
        "forced_materialization_version": version,
        "society_id": society_id,
        "society_name": entity_names.get(society_id),
        "graph_edges_from_society": graph_edges,
        "facts_on_society": society_facts,
        "facts_on_connected_entities": facts_by_entity,
        "composed_context": composed,
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&dump).expect("json dump")
    );
}
