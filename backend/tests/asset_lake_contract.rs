use std::sync::Arc;

use arrow::array::StringArray;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use backend::assets::{
    default_openestates_registry, ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, AssetPlanner, AssetStage, CurrentAssetPointer, MaterializationRecord,
    PlanReason, SourceWatermark,
};
use backend::lake::{LakeKey, LakeStore};
use chrono::{TimeZone, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn mock_rera_to_serving_bundle_materializes_with_stable_local_keys() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());

    let rera_asset = AssetId::new("rera_registry_monthly").unwrap();
    let rera_partition = AssetPartition::new([("state", "ka"), ("dt", "2026-07")]);
    let rera_key = AssetPathBuilder::raw_snapshot_key(
        "rera",
        &rera_partition,
        "run-rera-2026-07",
        "projects/part-00000.parquet",
    );

    let rera_meta = lake
        .put_bytes(
            &rera_key,
            single_string_column_parquet("project", "Prestige Lakeside"),
        )
        .await
        .unwrap();
    let rera_record = MaterializationRecord::succeeded(
        rera_asset.clone(),
        AssetStage::Raw,
        rera_partition.clone(),
        "2026-07",
        vec![ArtifactRef::parquet(rera_meta)],
    )
    .with_source_watermarks(vec![SourceWatermark {
        source: "rera".to_string(),
        high_watermark: "2026-07".to_string(),
    }])
    .with_row_count(1);

    materializations
        .write_materialization(&rera_record)
        .await
        .unwrap();
    assert!(lake
        .get_json::<serde_json::Value>(&AssetPathBuilder::materialization_lookup_key(
            &rera_record.materialization_id,
        ))
        .await
        .is_ok());
    assert_eq!(
        materializations
            .record_by_id_for_asset(&rera_asset, &rera_record.materialization_id)
            .await
            .unwrap(),
        Some(rera_record.clone())
    );
    materializations
        .promote_current(&rera_record)
        .await
        .unwrap();

    let facts_asset = AssetId::new("rera_legal_facts").unwrap();
    let fact_partition = AssetPartition::new([("dt", "2026-07")]);
    let fact_key = AssetPathBuilder::silver_fact_key(
        "society",
        "rera_registration_number",
        "rera",
        &fact_partition,
        "part-00000.parquet",
    );
    let fact_meta = lake
        .put_bytes(
            &fact_key,
            single_string_column_parquet("rera_registration_number", "PRM/KA/RERA/1251/446"),
        )
        .await
        .unwrap();
    let fact_record = MaterializationRecord::succeeded(
        facts_asset.clone(),
        AssetStage::Silver,
        fact_partition.clone(),
        "2026-07",
        vec![ArtifactRef::parquet(fact_meta)],
    )
    .with_parent_materializations(vec![rera_record.materialization_id.clone()])
    .with_row_count(1);

    materializations
        .write_materialization(&fact_record)
        .await
        .unwrap();
    materializations
        .promote_current(&fact_record)
        .await
        .unwrap();

    let serving_asset = AssetId::new("search_serving_bundle").unwrap();
    let serving_partition = AssetPartition::global();
    let serving_key = AssetPathBuilder::serving_bundle_key("2026-07-12T10:00Z", "manifest.json");
    let serving_meta = lake
        .put_json(
            &serving_key,
            &json!({
                "bundleVersion": "2026-07-12T10:00Z",
                "kgVersion": "2026-07",
                "sourceWatermarks": {
                    "rera": "2026-07"
                }
            }),
        )
        .await
        .unwrap();
    let serving_record = MaterializationRecord::succeeded(
        serving_asset.clone(),
        AssetStage::Serving,
        serving_partition.clone(),
        "2026-07-12T10:00Z",
        vec![ArtifactRef::json(serving_meta)],
    )
    .with_parent_materializations(vec![fact_record.materialization_id.clone()])
    .with_source_watermarks(vec![SourceWatermark {
        source: "rera".to_string(),
        high_watermark: "2026-07".to_string(),
    }])
    .with_row_count(1);

    materializations
        .write_materialization(&serving_record)
        .await
        .unwrap();
    materializations
        .promote_current(&serving_record)
        .await
        .unwrap();

    let current_serving = materializations
        .current_record(&serving_asset, &serving_partition)
        .await
        .unwrap();
    assert_eq!(current_serving.version, "2026-07-12T10:00Z");
    assert_eq!(
        current_serving.parent_materializations,
        vec![fact_record.materialization_id.clone()]
    );
    assert_eq!(
        current_serving.artifacts[0].key,
        "serving/search_bundle/version=2026-07-12t10-00z/manifest.json"
    );

    let current_facts = materializations
        .current_record(&facts_asset, &fact_partition)
        .await
        .unwrap();
    assert_eq!(
        current_facts.parent_materializations,
        vec![rera_record.materialization_id.clone()]
    );

    let raw_body = lake.get_bytes(&rera_key).await.unwrap();
    assert_is_parquet(&raw_body);

    let current_pointer_key =
        LakeKey::new("manifests/assets/search_serving_bundle/partition=global/current.json")
            .unwrap();
    let pointer_body = lake.get_text(&current_pointer_key).await.unwrap();
    assert!(pointer_body.contains("2026-07-12T10:00Z"));
}

#[tokio::test]
async fn planner_returns_missing_default_assets_in_dependency_order() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let registry = default_openestates_registry();
    let expected_count = registry.definitions().len();
    let planner = AssetPlanner::new(registry, materializations);

    let partition =
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);
    let plan = planner
        .plan_partition(&partition, Utc::now())
        .await
        .unwrap();

    assert_eq!(plan.len(), expected_count);
    assert_eq!(
        plan[0].asset_id,
        AssetId::new("rera_registry_monthly").unwrap()
    );
    assert_eq!(plan[0].reason, PlanReason::Missing);
    assert_eq!(
        plan.last().unwrap().asset_id,
        AssetId::new("search_serving_bundle").unwrap()
    );
}

#[tokio::test]
async fn materialization_store_lists_current_records_for_all_asset_partitions() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let asset_id = AssetId::new("reddit_resident_facts").unwrap();

    let older_partition = AssetPartition::new([("dt", "2026-07-12"), ("source", "reddit")]);
    let newer_partition = AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]);
    let older = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Silver,
        older_partition,
        "2026-07-12",
        Vec::new(),
    );
    let newer = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Silver,
        newer_partition,
        "2026-07-13",
        Vec::new(),
    );
    let other_asset = MaterializationRecord::succeeded(
        AssetId::new("google_review_facts").unwrap(),
        AssetStage::Silver,
        AssetPartition::new([("dt", "2026-07-13"), ("source", "google")]),
        "2026-07-13",
        Vec::new(),
    );
    let prefix_collision_asset = MaterializationRecord::succeeded(
        AssetId::new("reddit_resident_facts_extra").unwrap(),
        AssetStage::Silver,
        AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]),
        "2026-07-13-extra",
        Vec::new(),
    );

    materializations
        .write_materialization(&newer)
        .await
        .unwrap();
    materializations.promote_current(&newer).await.unwrap();
    materializations
        .write_materialization(&older)
        .await
        .unwrap();
    materializations.promote_current(&older).await.unwrap();
    materializations
        .write_materialization(&other_asset)
        .await
        .unwrap();
    materializations
        .promote_current(&other_asset)
        .await
        .unwrap();
    materializations
        .write_materialization(&prefix_collision_asset)
        .await
        .unwrap();
    materializations
        .promote_current(&prefix_collision_asset)
        .await
        .unwrap();

    let current_records = materializations
        .current_records_for_asset(&asset_id)
        .await
        .unwrap();

    assert_eq!(
        current_records
            .iter()
            .map(|record| record.version.as_str())
            .collect::<Vec<_>>(),
        vec!["2026-07-12", "2026-07-13"]
    );
    assert!(current_records
        .iter()
        .all(|record| record.asset_id == asset_id));

    let bad_pointer_key = LakeKey::new(
        "manifests/assets/reddit_resident_facts/dt=2026-07-14/source=reddit/current.json",
    )
    .unwrap();
    lake.put_json(
        &bad_pointer_key,
        &CurrentAssetPointer {
            asset_id: AssetId::new("google_review_facts").unwrap(),
            partition: AssetPartition::new([("dt", "2026-07-14"), ("source", "reddit")]),
            materialization_id: other_asset.materialization_id.clone(),
            materialization_key: AssetPathBuilder::materialization_record_key(
                &other_asset.asset_id,
                &other_asset.partition,
                &other_asset.materialization_id,
            )
            .to_string(),
            version: other_asset.version.clone(),
            run_id: None,
            run_created_at: None,
            updated_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let err = materializations
        .current_records_for_asset(&asset_id)
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("belongs to asset google_review_facts"));
}

#[tokio::test]
async fn older_run_cannot_roll_back_current_asset_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake);
    let asset_id = AssetId::new("kg_society_view").unwrap();
    let partition = AssetPartition::global();
    let older_time = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let newer_time = older_time + chrono::Duration::hours(1);
    let older = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Gold,
        partition.clone(),
        "older",
        Vec::new(),
    );
    let newer = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Gold,
        partition.clone(),
        "newer",
        Vec::new(),
    );
    store.write_materialization(&older).await.unwrap();
    store.write_materialization(&newer).await.unwrap();

    assert!(store
        .promote_current_for_run(&newer, newer_time)
        .await
        .unwrap());
    assert!(!store
        .promote_current_for_run(&older, older_time)
        .await
        .unwrap());
    assert_eq!(
        store
            .current_record(&asset_id, &partition)
            .await
            .unwrap()
            .materialization_id,
        newer.materialization_id
    );
}

#[tokio::test]
async fn materialization_store_rejects_pointer_to_wrong_asset_record() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let asset_id = AssetId::new("reddit_resident_facts").unwrap();
    let partition = AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]);
    let other_record = MaterializationRecord::succeeded(
        AssetId::new("google_review_facts").unwrap(),
        AssetStage::Silver,
        AssetPartition::new([("dt", "2026-07-13"), ("source", "google")]),
        "2026-07-13",
        Vec::new(),
    );

    materializations
        .write_materialization(&other_record)
        .await
        .unwrap();
    lake.put_json(
        &AssetPathBuilder::current_pointer_key(&asset_id, &partition),
        &CurrentAssetPointer {
            asset_id: asset_id.clone(),
            partition: partition.clone(),
            materialization_id: other_record.materialization_id.clone(),
            materialization_key: AssetPathBuilder::materialization_record_key(
                &other_record.asset_id,
                &other_record.partition,
                &other_record.materialization_id,
            )
            .to_string(),
            version: other_record.version.clone(),
            run_id: None,
            run_created_at: None,
            updated_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let err = materializations
        .current_record(&asset_id, &partition)
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("points to materialization for asset google_review_facts"));
}

fn single_string_column_parquet(column_name: &str, value: &str) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        column_name,
        DataType::Utf8,
        false,
    )]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(StringArray::from(vec![value.to_string()]))],
    )
    .unwrap();
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(props)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    bytes
}

fn assert_is_parquet(bytes: &[u8]) {
    assert!(bytes.len() > 8);
    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
}
