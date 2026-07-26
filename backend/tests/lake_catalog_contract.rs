use std::sync::Arc;

use arrow::array::{Float64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use backend::assets::{ArtifactRef, AssetId, AssetPartition, AssetStage, MaterializationRecord};
use backend::lake::{LakeCatalog, LakeKey, LakeStore};
use datafusion::assert_batches_eq;
use object_store::memory::InMemory;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

#[tokio::test]
async fn catalog_queries_only_exact_manifest_parquet_materializations() {
    let lake = LakeStore::from_object_store(Arc::new(InMemory::new()));
    let selected_older = put_materialization(
        &lake,
        "2026-07-01",
        &[
            ("society:alpha", "Whitefield", 150.0),
            ("society:beta", "Sarjapur", 90.0),
        ],
    )
    .await;
    let selected_newer = put_materialization(
        &lake,
        "2026-07-02",
        &[("society:gamma", "Whitefield", 180.0)],
    )
    .await;
    let unreferenced_latest = put_materialization(
        &lake,
        "2026-07-03",
        &[("society:should-not-appear", "Whitefield", 999.0)],
    )
    .await;

    let mut catalog = LakeCatalog::new(lake);
    let registered = catalog
        .register_parquet_table(
            "candidate_facts",
            &[selected_older.clone(), selected_newer.clone()],
            "facts/part-00000.parquet",
            &candidate_schema(),
        )
        .await
        .unwrap();

    assert_eq!(
        registered.materialization_ids,
        vec![
            selected_older.materialization_id.clone(),
            selected_newer.materialization_id.clone(),
        ]
    );
    assert_eq!(registered.artifact_keys.len(), 2);
    assert!(!registered
        .artifact_keys
        .contains(&unreferenced_latest.artifacts.first().unwrap().key.clone()));

    let batches = catalog
        .context()
        .sql(
            "SELECT locality, COUNT(*) AS matching_count, SUM(price_lakh) AS total_price_lakh \
             FROM candidate_facts WHERE price_lakh >= 100 \
             GROUP BY locality ORDER BY locality",
        )
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();

    assert_batches_eq!(
        [
            "+------------+----------------+------------------+",
            "| locality   | matching_count | total_price_lakh |",
            "+------------+----------------+------------------+",
            "| Whitefield | 2              | 330.0            |",
            "+------------+----------------+------------------+",
        ],
        &batches
    );
}

#[tokio::test]
async fn catalog_rejects_incompatible_parquet_schemas_during_registration() {
    let lake = LakeStore::from_object_store(Arc::new(InMemory::new()));
    let numeric = put_materialization(
        &lake,
        "2026-07-01",
        &[("society:alpha", "Whitefield", 150.0)],
    )
    .await;
    let key =
        LakeKey::new("silver/candidate_facts/version=2026-07-02/facts/part-00000.parquet").unwrap();
    let metadata = lake
        .put_bytes(&key, parquet_bytes_with_text_price())
        .await
        .unwrap();
    let incompatible = MaterializationRecord::succeeded(
        AssetId::new("candidate_facts").unwrap(),
        AssetStage::Silver,
        AssetPartition::new([("dt", "2026-07-02")]),
        "2026-07-02",
        vec![ArtifactRef::parquet(metadata)],
    )
    .with_row_count(1);

    let error = LakeCatalog::new(lake)
        .register_parquet_table(
            "candidate_facts",
            &[numeric, incompatible],
            "facts/part-00000.parquet",
            &candidate_schema(),
        )
        .await
        .unwrap_err();

    assert!(
        error.to_string().to_ascii_lowercase().contains("schema"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn catalog_rejects_artifact_overwritten_after_manifest_creation() {
    let lake = LakeStore::from_object_store(Arc::new(InMemory::new()));
    let materialization = put_materialization(
        &lake,
        "2026-07-01",
        &[("society:alpha", "Whitefield", 150.0)],
    )
    .await;
    let key = LakeKey::new(materialization.artifacts[0].key.clone()).unwrap();
    lake.put_bytes(
        &key,
        parquet_bytes(&[("society:tampered", "Whitefield", 999.0)]),
    )
    .await
    .unwrap();

    let error = LakeCatalog::new(lake)
        .register_parquet_table(
            "candidate_facts",
            &[materialization],
            "facts/part-00000.parquet",
            &candidate_schema(),
        )
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("does not match its manifest"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn catalog_queries_remain_pinned_after_object_is_overwritten() {
    let lake = LakeStore::from_object_store(Arc::new(InMemory::new()));
    let materialization = put_materialization(
        &lake,
        "2026-07-01",
        &[("society:alpha", "Whitefield", 150.0)],
    )
    .await;
    let key = LakeKey::new(materialization.artifacts[0].key.clone()).unwrap();
    let mut catalog = LakeCatalog::new(lake.clone());
    catalog
        .register_parquet_table(
            "candidate_facts",
            &[materialization],
            "facts/part-00000.parquet",
            &candidate_schema(),
        )
        .await
        .unwrap();

    lake.put_bytes(
        &key,
        parquet_bytes(&[("society:tampered", "Whitefield", 999.0)]),
    )
    .await
    .unwrap();

    let error = catalog
        .context()
        .sql("SELECT SUM(price_lakh) FROM candidate_facts")
        .await
        .unwrap()
        .collect()
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .to_ascii_lowercase()
            .contains("precondition"),
        "unexpected error: {error}"
    );
}

async fn put_materialization(
    lake: &LakeStore,
    version: &str,
    rows: &[(&str, &str, f64)],
) -> MaterializationRecord {
    let key = LakeKey::new(format!(
        "silver/candidate_facts/version={version}/facts/part-00000.parquet"
    ))
    .unwrap();
    let metadata = lake.put_bytes(&key, parquet_bytes(rows)).await.unwrap();

    MaterializationRecord::succeeded(
        AssetId::new("candidate_facts").unwrap(),
        AssetStage::Silver,
        AssetPartition::new([("dt", version)]),
        version,
        vec![ArtifactRef::parquet(metadata)],
    )
    .with_row_count(rows.len() as u64)
}

fn parquet_bytes(rows: &[(&str, &str, f64)]) -> Vec<u8> {
    let schema = Arc::new(candidate_schema());
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|(entity_id, _, _)| *entity_id),
            )),
            Arc::new(StringArray::from_iter_values(
                rows.iter().map(|(_, locality, _)| *locality),
            )),
            Arc::new(Float64Array::from_iter_values(
                rows.iter().map(|(_, _, price)| *price),
            )),
        ],
    )
    .unwrap();
    let mut bytes = Vec::new();
    let properties = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    bytes
}

fn candidate_schema() -> Schema {
    Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("locality", DataType::Utf8, false),
        Field::new("price_lakh", DataType::Float64, false),
    ])
}

fn parquet_bytes_with_text_price() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("locality", DataType::Utf8, false),
        Field::new("price_lakh", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["society:incompatible"])),
            Arc::new(StringArray::from(vec!["Whitefield"])),
            Arc::new(StringArray::from(vec!["not-a-number"])),
        ],
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    bytes
}
