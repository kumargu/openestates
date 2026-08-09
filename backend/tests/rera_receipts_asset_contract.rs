use backend::assets::{
    default_openestates_registry, read_rera_receipt_records, read_rera_source_records,
    AssetDagExecutionOptions, AssetDagExecutor, AssetId, AssetMaterializationStore, AssetPartition,
    AssetSourceInputs, ReraReceiptKind, ReraReceiptSourceRecord, ReraReceiptsSourceInput,
    ReraSourceRecordInput, ReraSourceRecordKind, ReraSourceRecordsInput, RERA_CLAIMS_ASSET_ID,
    RERA_RECEIPTS_ASSET_ID, RERA_SOURCE_RECORDS_ASSET_ID,
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

#[tokio::test]
async fn source_records_can_only_materialize_from_the_receipt_backfill() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
    let receipts_asset = AssetId::new(RERA_RECEIPTS_ASSET_ID).unwrap();
    let source_records_asset = AssetId::new(RERA_SOURCE_RECORDS_ASSET_ID).unwrap();
    let claims_asset = AssetId::new(RERA_CLAIMS_ASSET_ID).unwrap();
    let receipt_inputs = AssetSourceInputs {
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
    executor
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(AssetPartition::global(), now)
                .with_source_inputs(receipt_inputs)
                .with_forced_assets(vec![receipts_asset.clone()])
                .with_only_forced_assets(true),
        )
        .await
        .unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let receipt_record = materializations
        .current_record(&receipts_asset, &AssetPartition::global())
        .await
        .unwrap();
    let receipt = read_rera_receipt_records(&lake, &receipt_record)
        .await
        .unwrap()
        .pop()
        .unwrap();
    let source_inputs = AssetSourceInputs {
        rera_source_records: Some(ReraSourceRecordsInput {
            snapshot_date: "2026-08-09".to_string(),
            records: vec![ReraSourceRecordInput {
                kind: ReraSourceRecordKind::RegistrationSummary,
                registration_number: "PRM/KA/RERA/1251/446/PR/200811/003528".to_string(),
                receipt_id: receipt.receipt_id,
                capture_id: receipt.capture_id,
                source_locator: "applicationNameList[0]".to_string(),
                raw_label: "K-RERA listing row".to_string(),
                raw_value: "{\"acknowledgement_number\":\"ACK-1\",\"registration_number\":\"PRM/KA/RERA/1251/446/PR/200811/003528\",\"project_name\":\"Fixture Project\",\"promoter_name\":\"Fixture Promoter\"}".to_string(),
                observed_at: now,
                effective_at: None,
                filing_at: None,
                parser_version: "fixture.v1".to_string(),
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
                .with_forced_assets(vec![source_records_asset.clone()])
                .with_only_forced_assets(true),
        )
        .await
        .unwrap();

    assert_eq!(report.executed_assets, vec![source_records_asset.clone()]);
    let record = materializations
        .current_record(&source_records_asset, &AssetPartition::global())
        .await
        .unwrap();
    assert_eq!(
        record.parent_materializations,
        vec![receipt_record.materialization_id]
    );
    assert_eq!(
        read_rera_source_records(&lake, &record)
            .await
            .unwrap()
            .len(),
        1
    );

    let report = executor
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(AssetPartition::global(), now)
                .with_forced_assets(vec![claims_asset.clone()])
                .with_only_forced_assets(true),
        )
        .await
        .unwrap();
    assert_eq!(report.executed_assets, vec![claims_asset.clone()]);
    let claims_record = materializations
        .current_record(&claims_asset, &AssetPartition::global())
        .await
        .unwrap();
    assert_eq!(claims_record.row_count, 4);
    assert_eq!(
        claims_record.parent_materializations,
        vec![record.materialization_id]
    );
}
