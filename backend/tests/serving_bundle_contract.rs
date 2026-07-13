use std::fs::File;

use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::{
    hydrate_tantivy_index, BundleArtifactKind, ServingBundleBuilder, TantivyRecallIndex,
};
use chrono::Utc;
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn serving_bundle_writes_parquet_manifest_and_hydratable_tantivy_index() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let graph = mock_graph();

    let manifest = ServingBundleBuilder::new(lake.clone())
        .build_from_graph(&graph, "2026-07-12T18:30Z")
        .await
        .unwrap();

    assert_eq!(manifest.entity_count, 2);
    assert_eq!(manifest.fact_count, 4);
    assert_eq!(manifest.search_metadata_count, 4);
    assert_eq!(
        manifest.entity_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/entities/part-00000.parquet"
    );
    assert_eq!(
        manifest.fact_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/facts/part-00000.parquet"
    );
    assert_eq!(
        manifest.search_metadata_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/search_metadata/part-00000.parquet"
    );
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == BundleArtifactKind::TantivyIndexFile));

    let entity_bytes = lake
        .get_bytes(&LakeKey::new(manifest.entity_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let fact_bytes = lake
        .get_bytes(&LakeKey::new(manifest.fact_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let search_metadata_bytes = lake
        .get_bytes(&LakeKey::new(manifest.search_metadata_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    assert_is_parquet(&entity_bytes);
    assert_is_parquet(&fact_bytes);
    assert_is_parquet(&search_metadata_bytes);
    assert_eq!(parquet_rows(&entity_bytes), 2);
    assert_eq!(parquet_rows(&fact_bytes), 4);
    assert_eq!(parquet_rows(&search_metadata_bytes), 4);
    let fact_columns = parquet_columns(&fact_bytes);
    let search_metadata_columns = parquet_columns(&search_metadata_bytes);
    assert!(fact_columns.contains(&"value_text".to_string()));
    assert!(fact_columns.contains(&"value_number".to_string()));
    assert!(fact_columns.contains(&"value_tags".to_string()));
    assert!(!fact_columns.contains(&"value_json".to_string()));
    assert!(!fact_columns.contains(&"answers_preferences_json".to_string()));
    assert!(search_metadata_columns.contains(&"answers_preferences".to_string()));
    assert!(!search_metadata_columns.contains(&"answers_preferences_json".to_string()));

    let manifest_key =
        LakeKey::new("serving/search_bundle/version=2026-07-12t18-30z/manifest.json").unwrap();
    let manifest_body = lake.get_text(&manifest_key).await.unwrap();
    assert!(manifest_body.contains("\"format_version\": 2"));

    let hydrated = tempdir().unwrap();
    hydrate_tantivy_index(&lake, &manifest, hydrated.path())
        .await
        .unwrap();
    let recall = TantivyRecallIndex::open(hydrated.path()).unwrap();
    let hits = recall.search("whitefield greenery trees", 5).unwrap();
    assert_eq!(hits[0].entity_id, "society:green-acre-whitefield");
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
        "rera_total_land_area_sqm",
        FactValue::Numeric(8_000.0),
        SourceType::Rera,
        &["compact project"],
    ));
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
    let mut reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .unwrap()
    .build()
    .unwrap();
    let batch = reader.next().unwrap().unwrap();
    batch
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}
