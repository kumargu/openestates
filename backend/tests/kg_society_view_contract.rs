use std::fs::File;

use backend::assets::{
    AssetId, AssetMaterializationStore, AssetPartition, KgSocietyViewMaterializer,
    KG_SOCIETY_VIEW_ASSET_ID,
};
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::SearchServingBundleMaterializer;
use chrono::Utc;
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn kg_society_view_materializes_gold_parquet_and_serving_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let graph = mock_graph();

    let kg_materialization = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote(&graph, "2026-07-13T00:00Z", Vec::new(), Vec::new())
        .await
        .unwrap();

    assert_eq!(kg_materialization.manifest.entity_count, 2);
    assert_eq!(kg_materialization.manifest.fact_count, 3);
    assert_eq!(kg_materialization.manifest.fact_annotation_count, 3);
    assert_eq!(kg_materialization.manifest.edge_count, 1);
    assert_eq!(kg_materialization.manifest.graph_content_hash.len(), 64);
    assert_eq!(
        kg_materialization.manifest.entity_parquet_key,
        "gold/kg_society_view/version=2026-07-13t00-00z/entities/part-00000.parquet"
    );
    assert_eq!(
        kg_materialization.manifest.fact_parquet_key,
        "gold/kg_society_view/version=2026-07-13t00-00z/facts/part-00000.parquet"
    );
    assert_eq!(
        kg_materialization.manifest.fact_annotation_parquet_key,
        "gold/kg_society_view/version=2026-07-13t00-00z/fact_annotations/part-00000.parquet"
    );
    assert_eq!(
        kg_materialization.manifest.edge_parquet_key,
        "gold/kg_society_view/version=2026-07-13t00-00z/edges/part-00000.parquet"
    );

    let entity_bytes = lake
        .get_bytes(&LakeKey::new(kg_materialization.manifest.entity_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let fact_bytes = lake
        .get_bytes(&LakeKey::new(kg_materialization.manifest.fact_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let fact_annotation_bytes = lake
        .get_bytes(
            &LakeKey::new(
                kg_materialization
                    .manifest
                    .fact_annotation_parquet_key
                    .clone(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let edge_bytes = lake
        .get_bytes(&LakeKey::new(kg_materialization.manifest.edge_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    assert_is_parquet(&entity_bytes);
    assert_is_parquet(&fact_bytes);
    assert_is_parquet(&fact_annotation_bytes);
    assert_is_parquet(&edge_bytes);
    assert_eq!(parquet_rows(&entity_bytes), 2);
    assert_eq!(parquet_rows(&fact_bytes), 3);
    assert_eq!(parquet_rows(&fact_annotation_bytes), 3);
    assert_eq!(parquet_rows(&edge_bytes), 1);
    assert!(parquet_columns(&fact_bytes).contains(&"triggered_by".to_string()));
    assert!(!parquet_columns(&fact_bytes).contains(&"answers_preferences_json".to_string()));
    assert!(
        parquet_columns(&fact_annotation_bytes).contains(&"answers_preferences_json".to_string())
    );

    let materializations = AssetMaterializationStore::new(lake.clone());
    let current_kg = materializations
        .current_record(
            &AssetId::new(KG_SOCIETY_VIEW_ASSET_ID).unwrap(),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    assert_eq!(
        current_kg.materialization_id,
        kg_materialization.record.materialization_id
    );
    assert_eq!(current_kg.row_count, 9);
    assert!(current_kg.source_watermarks.iter().any(|watermark| {
        watermark.source == "knowledge_graph_content_hash"
            && watermark.high_watermark == kg_materialization.manifest.graph_content_hash
    }));

    let search_materialization = SearchServingBundleMaterializer::new(lake)
        .materialize_and_promote_from_kg_view(&kg_materialization, "2026-07-13T00:00Z")
        .await
        .unwrap();

    assert_eq!(
        search_materialization.record.parent_materializations,
        vec![kg_materialization.record.materialization_id.clone()]
    );
    assert!(search_materialization
        .record
        .source_watermarks
        .iter()
        .any(|watermark| {
            watermark.source == KG_SOCIETY_VIEW_ASSET_ID
                && watermark.high_watermark
                    == kg_materialization.record.materialization_id.to_string()
        }));
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
        FactValue::Text("Residents mention many trees and open space".to_string()),
        SourceType::Reddit,
        &["greenery", "trees"],
    ));

    let mut builder = Node::new("builder:test-builder", NodeType::Builder, "Test Builder");
    builder.add_fact(fact(
        "delivery_track_record",
        FactValue::Text("Delivered prior projects on time".to_string()),
        SourceType::Manual,
        &["reliable builder"],
    ));

    graph.add_node(green);
    graph.add_node(builder);
    graph.add_edge(Edge {
        from: "society:green-acre-whitefield".to_string(),
        to: "builder:test-builder".to_string(),
        relation: Relation::BuiltBy,
        weight: 1.0,
        metadata: std::collections::HashMap::new(),
        source: FactSource {
            source_type: SourceType::Manual,
            url: None,
            model: None,
            skill_id: None,
            triggered_by: None,
        },
    });
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
            _ => 0.8,
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

fn assert_is_parquet(bytes: &[u8]) {
    assert!(bytes.len() > 8);
    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
}

fn parquet_rows(bytes: &[u8]) -> i64 {
    let dir = tempdir().unwrap();
    let path = dir.path().join("table.parquet");
    std::fs::write(&path, bytes).unwrap();
    let file = File::open(path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    reader.metadata().file_metadata().num_rows()
}

fn parquet_columns(bytes: &[u8]) -> Vec<String> {
    let dir = tempdir().unwrap();
    let path = dir.path().join("table.parquet");
    std::fs::write(&path, bytes).unwrap();
    let file = File::open(path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    reader
        .metadata()
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| column.name().to_string())
        .collect()
}
