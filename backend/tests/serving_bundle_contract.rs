use std::fs::File;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::{
    hydrate_tantivy_index, read_facts_parquet, read_search_metadata_parquet, BundleArtifactKind,
    ServingBundleBuilder, TantivyRecallIndex,
};
use chrono::Utc;
use parquet::arrow::ArrowWriter;
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
    assert_eq!(manifest.edge_count, 0);
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

    let fact_records = read_facts_parquet(&fact_bytes).unwrap();
    let land_area = fact_records
        .iter()
        .find(|fact| {
            fact.entity_id == "society:green-acre-whitefield"
                && fact.fact_key == "rera_total_land_area_sqm"
        })
        .unwrap();
    assert_eq!(land_area.value, FactValue::Numeric(48_000.0));

    let search_metadata_records = read_search_metadata_parquet(&search_metadata_bytes).unwrap();
    let greenery_metadata = search_metadata_records
        .iter()
        .find(|metadata| {
            metadata.entity_id == "society:green-acre-whitefield"
                && metadata.fact_key == "resident_greenery_signal"
        })
        .unwrap();
    assert!(greenery_metadata
        .answers_preferences
        .contains(&"greenery".to_string()));

    let manifest_key =
        LakeKey::new("serving/search_bundle/version=2026-07-12t18-30z/manifest.json").unwrap();
    let manifest_body = lake.get_text(&manifest_key).await.unwrap();
    assert!(manifest_body.contains("\"format_version\": 3"));

    let schema_key =
        LakeKey::new("serving/search_bundle/version=2026-07-12t18-30z/schema.json").unwrap();
    let schema_body = lake.get_text(&schema_key).await.unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_body).unwrap();
    assert_eq!(schema_json["storage_format"], "parquet+tantivy");
    assert!(schema_body.contains("\"value_number\""));
    assert!(schema_body.contains("\"answers_preferences\""));
    assert!(!schema_body.contains("value_json"));
    assert!(!schema_body.contains("answers_preferences_json"));

    let hydrated = tempdir().unwrap();
    hydrate_tantivy_index(&lake, &manifest, hydrated.path())
        .await
        .unwrap();
    let recall = TantivyRecallIndex::open(hydrated.path()).unwrap();
    let hits = recall.search("whitefield greenery trees", 5).unwrap();
    assert_eq!(hits[0].entity_id, "society:green-acre-whitefield");
    assert_eq!(hits[0].entity_type, "society");
    assert_eq!(hits[0].name, "Green Acre Whitefield");
    assert!(hits[0].matched_fields.iter().any(|field| field == "name"));
    assert!(hits[0].matched_fields.iter().any(|field| field == "body"));
}

#[test]
fn legacy_json_serving_parquet_reads_into_typed_runtime_records() {
    let facts = read_facts_parquet(&legacy_facts_parquet()).unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].entity_id, "society:legacy-green");
    assert_eq!(
        facts[0].value,
        FactValue::Text("Legacy residents mention trees".to_string())
    );
    assert_eq!(
        facts[0].value_text.as_deref(),
        Some("Legacy residents mention trees")
    );

    let metadata = read_search_metadata_parquet(&legacy_search_metadata_parquet()).unwrap();
    assert_eq!(metadata.len(), 1);
    assert_eq!(
        metadata[0].answers_preferences,
        vec!["greenery".to_string(), "trees".to_string()]
    );
    assert_eq!(metadata[0].scoring_direction.as_deref(), Some("TextMatch"));
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
        FactValue::Numeric(40_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    ));
    let mut corrected_land_area = fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(48_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    );
    corrected_land_area.version = 2;
    green.add_fact(corrected_land_area);
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

fn legacy_facts_parquet() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("value_type", DataType::Utf8, false),
        Field::new("value_text", DataType::Utf8, true),
        Field::new("value_json", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
    ]));
    write_legacy_parquet(
        schema,
        vec![
            string_array(["society:legacy-green"]),
            string_array(["resident_greenery_signal"]),
            string_array(["text"]),
            optional_string_array([Some("Legacy residents mention trees")]),
            string_array([r#"{"type":"Text","data":"Legacy residents mention trees"}"#]),
            Arc::new(Float32Array::from(vec![0.71])) as ArrayRef,
            string_array(["Reddit"]),
            optional_string_array([Some("https://reddit.com/r/BangaloreRealEstates/legacy")]),
            optional_string_array([None]),
            optional_string_array([Some("legacy_skill")]),
            string_array(["2026-07-13T00:00:00Z"]),
        ],
    )
}

fn legacy_search_metadata_parquet() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("display_template", DataType::Utf8, true),
        Field::new("answers_preferences_json", DataType::Utf8, false),
        Field::new("scoring_direction", DataType::Utf8, true),
        Field::new("scoring_weight", DataType::Float32, true),
    ]));
    write_legacy_parquet(
        schema,
        vec![
            string_array(["society:legacy-green"]),
            string_array(["resident_greenery_signal"]),
            optional_string_array([Some("Resident signal: {value}")]),
            string_array([r#"["greenery","trees"]"#]),
            optional_string_array([Some("TextMatch")]),
            Arc::new(Float32Array::from(vec![Some(1.4)])) as ArrayRef,
        ],
    )
}

fn write_legacy_parquet(schema: Arc<Schema>, columns: Vec<ArrayRef>) -> Vec<u8> {
    let batch = RecordBatch::try_new(schema.clone(), columns).unwrap();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    bytes
}

fn string_array<const N: usize>(values: [&str; N]) -> ArrayRef {
    Arc::new(StringArray::from(Vec::from(values)))
}

fn optional_string_array<const N: usize>(values: [Option<&str>; N]) -> ArrayRef {
    Arc::new(StringArray::from(Vec::from(values)))
}
