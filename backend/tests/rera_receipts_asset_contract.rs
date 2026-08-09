use backend::assets::{
    default_openestates_registry, read_rera_receipt_records, AssetDagExecutionOptions,
    AssetDagExecutor, AssetId, AssetMaterializationStore, AssetPartition, AssetSourceInputs,
    ReraReceiptKind, ReraReceiptSourceRecord, ReraReceiptsSourceInput, RERA_RECEIPTS_ASSET_ID,
};
use backend::knowledge::KnowledgeGraph;
use backend::lake::LakeStore;
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

#[tokio::test]
async fn forced_rera_receipt_backfill_materializes_only_the_parallel_raw_asset() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
    let receipt_asset = AssetId::new(RERA_RECEIPTS_ASSET_ID).unwrap();
    let source_inputs = AssetSourceInputs {
        rera_receipts: Some(ReraReceiptsSourceInput {
            snapshot_date: "2026-08-09".to_string(),
            receipts: vec![ReraReceiptSourceRecord {
                kind: ReraReceiptKind::ProjectDetail,
                source_url: "https://rera.karnataka.gov.in/projectDetails?action=123".to_string(),
                content_type: "text/html".to_string(),
                body_hex: "3c68746d6c3e65766964656e63653c2f68746d6c3e".to_string(),
                captured_at: now,
                registration_number: Some("PRM/KA/RERA/1251/446/PR/200811/003528".to_string()),
                parent_receipt_id: None,
                crawl_run_id: "fixture-rera-backfill".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        ..AssetSourceInputs::default()
    };

    let report = executor
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(AssetPartition::global(), now)
                .with_source_inputs(source_inputs)
                .with_forced_assets(vec![receipt_asset.clone()])
                .with_only_forced_assets(true),
        )
        .await
        .unwrap();

    assert_eq!(report.executed_assets, vec![receipt_asset.clone()]);
    let materializations = AssetMaterializationStore::new(lake.clone());
    let record = materializations
        .current_record(&receipt_asset, &AssetPartition::global())
        .await
        .unwrap();
    let rows = read_rera_receipt_records(&lake, &record).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].normalized_registration_number.as_deref(),
        Some("PRM/KA/RERA/1251/446/PR/200811/003528")
    );
    assert!(rows[0]
        .body_key
        .contains("raw/receipts/source=rera/sha256="));
}
