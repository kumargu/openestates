use std::fs::File;

use arrow::array::{Array, StringArray};
use backend::assets::{
    default_openestates_registry, rera_legal_facts_input, AssetDagExecutionOptions,
    AssetDagExecutor, AssetDagExecutorError, AssetDefinition, AssetId, AssetMaterializationStore,
    AssetPartition, AssetRegistry, AssetRunManifestStore, AssetRunStepStatus, AssetSourceInputs,
    AssetStage, CanonicalSocietyMaterializer, CostTier, DagRunStatus, MaterializationId,
    MaterializationRecord, RedditThreadSnapshotRecord, RedditThreadsDailyInput, RefreshCadence,
    ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactMaterializer, SkillFactRecord, SkillFactsInput,
    SourceWatermark, TrustTier, CANONICAL_SOCIETY_NODES_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID,
    KG_SOCIETY_VIEW_ASSET_ID, REDDIT_RESIDENT_FACTS_ASSET_ID, REDDIT_THREADS_DAILY_ASSET_ID,
    RERA_LEGAL_FACTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::{
    ServingBundleLoader, ServingBundleManifest, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
use bytes::Bytes;
use chrono::{Duration, TimeZone, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn executor_runs_kg_and_serving_assets_with_dag_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

    let run_partition = source_run_partition();
    let upstreams = seed_current_upstreams_for_partition(&lake, &store, now, &run_partition).await;

    let options =
        AssetDagExecutionOptions::new(run_partition.clone(), now).with_version("2026-07-13T06:00Z");
    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.planned_count, 2);
    assert_eq!(report.manifest.succeeded_count, 2);
    assert_eq!(report.manifest.failed_count, 0);
    assert_eq!(
        report.executed_assets,
        vec![
            asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID)
        ]
    );

    let kg_record = store
        .current_record(
            &asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    assert_eq!(kg_record.run_id, report.manifest.run_id);
    assert_eq!(kg_record.parent_materializations.len(), 4);
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["canonical_society_nodes"].materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["rera_legal_facts"].materialization_id));

    let serving_record = store
        .current_record(
            &asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    assert_eq!(serving_record.run_id, report.manifest.run_id);
    assert_eq!(
        serving_record.parent_materializations,
        vec![kg_record.materialization_id.clone()]
    );

    let run_store = AssetRunManifestStore::new(lake);
    let current_run = run_store.current_manifest(&run_partition).await.unwrap();
    assert_eq!(current_run.run_id, report.manifest.run_id);
    assert_eq!(current_run.status, DagRunStatus::Succeeded);
    assert_eq!(
        current_run
            .steps
            .iter()
            .filter(|step| step.status == AssetRunStepStatus::Skipped)
            .count(),
        6
    );
}

#[tokio::test]
async fn executor_materializes_source_assets_from_local_inputs_with_parquet_and_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();

    let upstreams =
        seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;
    let older_reddit_facts = seed_skill_fact_current(
        &lake,
        &store,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        "reddit",
        "2026-07-12",
        &AssetPartition::new([("dt", "2026-07-12"), ("source", "reddit")]),
        vec![upstreams["canonical_society_nodes"]
            .materialization_id
            .clone()],
        now - Duration::days(1),
        "resident_clubhouse_signal",
        "Residents mention a maintained clubhouse",
        "Reddit",
        "legacy-reddit-clubhouse",
    )
    .await;
    let older_google_facts = seed_skill_fact_current(
        &lake,
        &store,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        "google",
        "2026-07-06",
        &AssetPartition::new([("dt", "2026-07-06"), ("source", "google")]),
        vec![upstreams["canonical_society_nodes"]
            .materialization_id
            .clone()],
        now - Duration::days(7),
        "google_rating_signal",
        "Reviews mention well maintained amenities",
        "Google",
        "legacy-google-rating",
    )
    .await;
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now)
        .with_version("2026-07-13T06:00Z")
        .with_source_inputs(mock_source_inputs(now));

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.partition, run_partition);
    assert_eq!(report.manifest.planned_count, 5);
    assert_eq!(report.executed_assets.len(), 5);
    for id in [
        REDDIT_THREADS_DAILY_ASSET_ID,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        KG_SOCIETY_VIEW_ASSET_ID,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
    ] {
        assert!(report.executed_assets.contains(&asset_id(id)));
    }
    assert!(
        executed_position(&report.executed_assets, REDDIT_THREADS_DAILY_ASSET_ID)
            < executed_position(&report.executed_assets, REDDIT_RESIDENT_FACTS_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, REDDIT_RESIDENT_FACTS_ASSET_ID)
            < executed_position(&report.executed_assets, KG_SOCIETY_VIEW_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, GOOGLE_REVIEW_FACTS_ASSET_ID)
            < executed_position(&report.executed_assets, KG_SOCIETY_VIEW_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, KG_SOCIETY_VIEW_ASSET_ID)
            < executed_position(&report.executed_assets, SEARCH_SERVING_BUNDLE_ASSET_ID)
    );

    let reddit_threads = current_record(
        &store,
        REDDIT_THREADS_DAILY_ASSET_ID,
        &reddit_thread_partition(),
    )
    .await;
    assert_eq!(reddit_threads.partition, reddit_thread_partition());
    assert_eq!(reddit_threads.run_id, report.manifest.run_id);
    assert_eq!(
        reddit_threads.parent_materializations,
        vec![upstreams["canonical_society_nodes"]
            .materialization_id
            .clone()]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &reddit_threads, "threads/part-00000.parquet").await,
        1
    );

    let reddit_facts = current_record(
        &store,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        &reddit_fact_partition(),
    )
    .await;
    assert_eq!(reddit_facts.partition, reddit_fact_partition());
    assert_eq!(reddit_facts.run_id, report.manifest.run_id);
    assert_eq!(
        reddit_facts.parent_materializations,
        vec![
            reddit_threads.materialization_id.clone(),
            upstreams["canonical_society_nodes"]
                .materialization_id
                .clone()
        ]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &reddit_facts, "facts/part-00000.parquet").await,
        1
    );

    let google_facts = current_record(
        &store,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        &google_fact_partition(),
    )
    .await;
    assert_eq!(google_facts.partition, google_fact_partition());
    assert_eq!(google_facts.run_id, report.manifest.run_id);
    assert_eq!(
        google_facts.parent_materializations,
        vec![upstreams["canonical_society_nodes"]
            .materialization_id
            .clone()]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &google_facts, "facts/part-00000.parquet").await,
        1
    );

    let kg_record =
        current_record(&store, KG_SOCIETY_VIEW_ASSET_ID, &AssetPartition::global()).await;
    assert_eq!(kg_record.partition, AssetPartition::global());
    assert!(kg_record
        .parent_materializations
        .contains(&reddit_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&older_reddit_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&google_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&older_google_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["rera_legal_facts"].materialization_id));
    assert_eq!(kg_record.parent_materializations.len(), 6);
    assert_eq!(
        parquet_rows_for_artifact(&lake, &kg_record, "facts/part-00000.parquet").await,
        39
    );

    let serving_record = current_record(
        &store,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert_eq!(serving_fact_rows(&lake, &serving_record).await, 39);

    let run_store = AssetRunManifestStore::new(lake);
    let current_run = run_store.current_manifest(&run_partition).await.unwrap();
    assert_eq!(current_run.run_id, report.manifest.run_id);
    assert_eq!(
        current_run
            .steps
            .iter()
            .find(|step| step.asset_id == asset_id(REDDIT_THREADS_DAILY_ASSET_ID))
            .unwrap()
            .partition,
        reddit_thread_partition()
    );
    assert_eq!(
        current_run
            .steps
            .iter()
            .find(|step| step.asset_id == asset_id(KG_SOCIETY_VIEW_ASSET_ID))
            .unwrap()
            .partition,
        AssetPartition::global()
    );
}

#[tokio::test]
async fn executor_builds_rera_proof_chain_from_typed_parent_artifacts() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now)
        .with_version("2026-07-13T06:00Z")
        .with_source_inputs(mock_source_inputs(now));

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.planned_count, 8);
    assert_eq!(report.executed_assets.len(), 8);
    for id in [
        RERA_REGISTRY_MONTHLY_ASSET_ID,
        CANONICAL_SOCIETY_NODES_ASSET_ID,
        RERA_LEGAL_FACTS_ASSET_ID,
        REDDIT_THREADS_DAILY_ASSET_ID,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        KG_SOCIETY_VIEW_ASSET_ID,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
    ] {
        assert!(report.executed_assets.contains(&asset_id(id)));
    }

    let rera = current_record(
        &store,
        RERA_REGISTRY_MONTHLY_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert_eq!(
        parquet_rows_for_artifact(&lake, &rera, "projects/part-00000.parquet").await,
        3
    );

    let canonical = current_record(
        &store,
        CANONICAL_SOCIETY_NODES_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert_eq!(
        canonical.parent_materializations,
        vec![rera.materialization_id.clone()]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &canonical, "entities/part-00000.parquet").await,
        5
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &canonical, "edges/part-00000.parquet").await,
        6
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &canonical, "mappings/part-00000.parquet").await,
        3
    );

    let legal = current_record(&store, RERA_LEGAL_FACTS_ASSET_ID, &AssetPartition::global()).await;
    assert_eq!(
        legal.parent_materializations,
        vec![
            rera.materialization_id,
            canonical.materialization_id.clone()
        ]
    );
    assert!(parquet_rows_for_artifact(&lake, &legal, "facts/part-00000.parquet").await >= 32);

    let kg = current_record(&store, KG_SOCIETY_VIEW_ASSET_ID, &AssetPartition::global()).await;
    assert!(kg
        .parent_materializations
        .contains(&canonical.materialization_id));
    assert!(kg
        .parent_materializations
        .contains(&legal.materialization_id));
    assert!(
        parquet_contains_utf8(
            &lake,
            &kg,
            "entities/part-00000.parquet",
            "name",
            "RERA Meadows"
        )
        .await
    );
    assert!(
        parquet_contains_utf8(
            &lake,
            &kg,
            "facts/part-00000.parquet",
            "fact_key",
            "rera_total_land_area_sqm"
        )
        .await
    );

    let serving = current_record(
        &store,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert!(serving_fact_rows(&lake, &serving).await >= 34);
    let serving_cache = tempdir().unwrap();
    let loaded = ServingBundleLoader::new(lake.clone(), serving_cache.path())
        .load_current_search_bundle()
        .await
        .unwrap()
        .expect("serving bundle should load");
    let alias_rows = loaded
        .fact_index
        .entity("society:rera-meadows")
        .expect("legacy property society alias should resolve to RERA facts");
    assert!(alias_rows
        .facts
        .iter()
        .any(|fact| fact.fact_key == "rera_total_land_area_sqm"));
}

#[tokio::test]
async fn executor_requires_source_inputs_without_promoting_current_source_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;

    let run_partition = source_run_partition();
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now);
    let err = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap_err();

    let missing_asset_id = match err {
        AssetDagExecutorError::SourceInputMissing { asset_id } => asset_id,
        other => panic!("expected missing source input, got {other:?}"),
    };
    let missing_partition = match missing_asset_id.as_str() {
        REDDIT_THREADS_DAILY_ASSET_ID => reddit_thread_partition(),
        REDDIT_RESIDENT_FACTS_ASSET_ID => reddit_fact_partition(),
        GOOGLE_REVIEW_FACTS_ASSET_ID => google_fact_partition(),
        other => panic!("unexpected missing source asset {other}"),
    };
    assert!(store
        .current_record(&missing_asset_id, &missing_partition)
        .await
        .unwrap_err()
        .is_not_found());
}

#[tokio::test]
async fn executor_fails_loudly_when_planned_asset_has_no_executor() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now);
    let registry = AssetRegistry::new(vec![AssetDefinition::new(
        asset_id("unwired_asset"),
        AssetStage::Raw,
        "test asset without a built-in executor",
        Vec::new(),
        RefreshCadence::Monthly,
        CostTier::Free,
        TrustTier::Root,
    )])
    .unwrap();

    let err = AssetDagExecutor::new(registry, lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AssetDagExecutorError::NoExecutor { asset_id: ref returned_asset_id }
            if returned_asset_id == &asset_id("unwired_asset")
    ));

    let failed_run = AssetRunManifestStore::new(lake)
        .current_manifest(&run_partition)
        .await
        .unwrap();
    assert_eq!(failed_run.status, DagRunStatus::Failed);
    assert_eq!(failed_run.failed_count, 1);
    let failed_step = failed_run
        .steps
        .iter()
        .find(|step| step.asset_id == asset_id("unwired_asset"))
        .unwrap();
    assert_eq!(failed_step.status, AssetRunStepStatus::Failed);
    assert!(failed_step
        .error
        .as_deref()
        .unwrap()
        .contains("no executor registered"));
}

#[tokio::test]
async fn executor_dry_run_does_not_write_run_manifest() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now).dry_run(true);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&KnowledgeGraph::new(), options)
        .await
        .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.manifest.status, DagRunStatus::Planned);
    assert_eq!(report.executed_assets.len(), 0);
    assert!(AssetRunManifestStore::new(lake)
        .current_manifest(&run_partition)
        .await
        .is_err());
}

#[tokio::test]
async fn executor_runs_partitioned_scope_while_keeping_runtime_assets_global() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    seed_current_upstreams_for_partition(&lake, &store, now, &run_partition).await;

    let options = AssetDagExecutionOptions::new(run_partition, now);
    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(
        report.executed_assets,
        vec![
            asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID),
        ]
    );
    assert!(store
        .current_record(
            &asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            &AssetPartition::global()
        )
        .await
        .is_ok());
    assert!(store
        .current_record(
            &asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID),
            &AssetPartition::global()
        )
        .await
        .is_ok());
}

async fn seed_authoritative_upstreams(
    lake: &LakeStore,
    store: &AssetMaterializationStore,
    now: chrono::DateTime<Utc>,
    partition: &AssetPartition,
) -> std::collections::HashMap<&'static str, MaterializationRecord> {
    let rera = ReraRegistryMaterializer::new(lake.clone())
        .materialize_for_run(
            &mock_rera_input(now),
            MaterializationId::new(),
            partition.clone(),
        )
        .await
        .unwrap();
    write_current(store, &rera).await;

    let canonical = CanonicalSocietyMaterializer::new(lake.clone())
        .materialize_from_rera_for_run(
            &rera,
            "2026-07-13",
            MaterializationId::new(),
            partition.clone(),
        )
        .await
        .unwrap();
    write_current(store, &canonical).await;

    let rera_facts_input =
        rera_legal_facts_input(lake, &rera, &canonical, &MaterializationId::new())
            .await
            .unwrap();
    let rera_facts = SkillFactMaterializer::new(lake.clone())
        .materialize_for_run(
            RERA_LEGAL_FACTS_ASSET_ID,
            rera_facts_input.source,
            rera_facts_input.snapshot_date,
            "seed-rera-facts",
            &rera_facts_input.facts,
            &rera_facts_input.fact_annotations,
            vec![
                rera.materialization_id.clone(),
                canonical.materialization_id.clone(),
            ],
            rera_facts_input.source_watermarks,
            MaterializationId::new(),
            partition.clone(),
        )
        .await
        .unwrap()
        .record;
    write_current(store, &rera_facts).await;

    std::collections::HashMap::from([
        ("rera_registry_monthly", rera),
        ("canonical_society_nodes", canonical),
        ("rera_legal_facts", rera_facts),
    ])
}

async fn seed_current_upstreams_for_partition(
    lake: &LakeStore,
    store: &AssetMaterializationStore,
    now: chrono::DateTime<Utc>,
    run_partition: &AssetPartition,
) -> std::collections::HashMap<&'static str, MaterializationRecord> {
    let authoritative =
        seed_authoritative_upstreams(lake, store, now, &AssetPartition::global()).await;
    let rera = authoritative[RERA_REGISTRY_MONTHLY_ASSET_ID].clone();
    let canonical = authoritative[CANONICAL_SOCIETY_NODES_ASSET_ID].clone();
    let rera_facts = authoritative[RERA_LEGAL_FACTS_ASSET_ID].clone();

    let reddit_threads = materialization(
        "reddit_threads_daily",
        AssetStage::Raw,
        "2026-07-13",
        now - Duration::hours(1),
        &reddit_thread_partition_for(run_partition),
    )
    .with_parent_materializations(vec![canonical.materialization_id.clone()])
    .with_source_watermarks(vec![SourceWatermark {
        source: "reddit:BangaloreRealEstates".to_string(),
        high_watermark: "2026-07-13T05:00:00Z".to_string(),
    }]);
    write_current(store, &reddit_threads).await;

    let reddit_facts = seed_skill_fact_current(
        lake,
        store,
        "reddit_resident_facts",
        "reddit",
        "2026-07-13",
        &reddit_fact_partition_for(run_partition),
        vec![
            reddit_threads.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        "resident_greenery_signal",
        "Residents mention trees and open space",
        "Reddit",
        "seed-reddit-greenery",
    )
    .await;

    let google_facts = seed_skill_fact_current(
        lake,
        store,
        "google_review_facts",
        "google",
        "2026-07-13",
        &google_fact_partition_for(run_partition),
        vec![canonical.materialization_id.clone()],
        now,
        "google_reviews_url",
        "https://maps.google.com/?cid=green-acre",
        "Google",
        "seed-google-review-link",
    )
    .await;

    std::collections::HashMap::from([
        ("rera_registry_monthly", rera),
        ("canonical_society_nodes", canonical),
        ("rera_legal_facts", rera_facts),
        ("reddit_threads_daily", reddit_threads),
        ("reddit_resident_facts", reddit_facts),
        ("google_review_facts", google_facts),
    ])
}

async fn current_record(
    store: &AssetMaterializationStore,
    asset_id_value: &str,
    partition: &AssetPartition,
) -> MaterializationRecord {
    store
        .current_record(&asset_id(asset_id_value), partition)
        .await
        .unwrap()
}

async fn write_current(store: &AssetMaterializationStore, record: &MaterializationRecord) {
    store.write_materialization(record).await.unwrap();
    store.promote_current(record).await.unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn seed_skill_fact_current(
    lake: &LakeStore,
    store: &AssetMaterializationStore,
    asset_id_value: &str,
    source: &str,
    snapshot_date: &str,
    partition: &AssetPartition,
    parent_materializations: Vec<backend::assets::MaterializationId>,
    learned_at: chrono::DateTime<Utc>,
    fact_key: &str,
    value: &str,
    source_type: &str,
    run_id: &str,
) -> MaterializationRecord {
    let fact = SkillFactRecord {
        entity_id: "society:green-acre-whitefield".to_string(),
        fact_key: fact_key.to_string(),
        value_type: "text".to_string(),
        value_json: serde_json::to_string(&FactValue::Text(value.to_string())).unwrap(),
        confidence: if source == "google" { 0.82 } else { 0.72 },
        source_type: source_type.to_string(),
        source_url: Some(format!("https://example.com/{run_id}")),
        model: None,
        skill_id: Some(format!("{source}_support_fact_extractor")),
        triggered_by: Some("3bhk whitefield greenery".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: format!("sha256:{run_id}"),
    };
    let annotation = SkillFactAnnotationRecord {
        entity_id: "society:green-acre-whitefield".to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(format!("{fact_key}: {{value}}")),
        answers_preferences_json: r#"["greenery","amenities","reviews"]"#.to_string(),
        scoring_direction: Some("TextMatch".to_string()),
        scoring_weight: Some(1.0),
        scoring_thresholds_json: "[]".to_string(),
    };
    let materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_for_run(
            asset_id_value,
            source,
            snapshot_date,
            run_id,
            &[fact],
            &[annotation],
            parent_materializations,
            Vec::new(),
            backend::assets::MaterializationId::new(),
            partition.clone(),
        )
        .await
        .unwrap();
    store
        .promote_current(&materialization.record)
        .await
        .unwrap();
    materialization.record
}

fn materialization(
    id: &str,
    stage: AssetStage,
    version: &str,
    created_at: chrono::DateTime<Utc>,
    partition: &AssetPartition,
) -> MaterializationRecord {
    let mut record = MaterializationRecord::succeeded(
        asset_id(id),
        stage,
        partition.clone(),
        version,
        Vec::new(),
    )
    .with_row_count(1);
    record.created_at = created_at;
    record
}

fn mock_graph() -> KnowledgeGraph {
    let mut graph = KnowledgeGraph::new();

    let mut society = Node::new(
        "society:green-acre-whitefield",
        NodeType::Society,
        "Green Acre Whitefield",
    );
    society.root_source = Some(RootSource::Rera);
    society.add_fact(fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(48_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    ));
    society.add_fact(fact(
        "resident_greenery_signal",
        FactValue::Text("Residents mention trees and open space".to_string()),
        SourceType::Reddit,
        &["greenery", "trees"],
    ));

    let mut builder = Node::new("builder:test-builder", NodeType::Builder, "Test Builder");
    builder.add_fact(fact(
        "delivery_track_record",
        FactValue::Text("Delivered prior projects on time".to_string()),
        SourceType::Rera,
        &["trusted builder"],
    ));

    graph.add_node(society);
    let mut rera_alias = Node::new("society:rera-meadows", NodeType::Society, "RERA Meadows");
    rera_alias.root_source = Some(RootSource::Legacy);
    graph.add_node(rera_alias);
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

fn asset_id(id: &str) -> AssetId {
    AssetId::new(id).unwrap()
}

fn source_run_partition() -> AssetPartition {
    AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")])
}

fn reddit_thread_partition() -> AssetPartition {
    AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")])
}

fn reddit_fact_partition() -> AssetPartition {
    AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")])
}

fn google_fact_partition() -> AssetPartition {
    AssetPartition::new([("dt", "2026-07-13"), ("source", "google")])
}

fn reddit_thread_partition_for(run_partition: &AssetPartition) -> AssetPartition {
    match (run_partition.value("dt"), run_partition.value("subreddit")) {
        (Some(dt), Some(subreddit)) => AssetPartition::new([("dt", dt), ("subreddit", subreddit)]),
        _ => AssetPartition::global(),
    }
}

fn reddit_fact_partition_for(run_partition: &AssetPartition) -> AssetPartition {
    match run_partition.value("dt") {
        Some(dt) => AssetPartition::new([("dt", dt), ("source", "reddit")]),
        None => AssetPartition::global(),
    }
}

fn google_fact_partition_for(run_partition: &AssetPartition) -> AssetPartition {
    match run_partition.value("dt") {
        Some(dt) => AssetPartition::new([("dt", dt), ("source", "google")]),
        None => AssetPartition::global(),
    }
}

fn executed_position(executed_assets: &[AssetId], id: &str) -> usize {
    executed_assets
        .iter()
        .position(|executed_asset_id| executed_asset_id == &asset_id(id))
        .unwrap_or_else(|| panic!("missing executed asset {id}"))
}

fn mock_source_inputs(now: chrono::DateTime<Utc>) -> AssetSourceInputs {
    AssetSourceInputs {
        rera_registry_monthly: Some(mock_rera_input(now)),
        reddit_threads_daily: Some(RedditThreadsDailyInput {
            snapshot_date: "2026-07-13".to_string(),
            subreddit: "BangaloreRealEstates".to_string(),
            records: vec![RedditThreadSnapshotRecord {
                thread_id: "t3_greenery".to_string(),
                subreddit: "BangaloreRealEstates".to_string(),
                query: "whitefield greenery large campus".to_string(),
                title: "Whitefield society with good tree cover?".to_string(),
                url: Some(
                    "https://reddit.com/r/BangaloreRealEstates/comments/greenery".to_string(),
                ),
                score: 31,
                num_comments: 9,
                created_utc: Some(1_776_000_000),
                selftext: Some("Residents discuss trees, clubhouse and metro access.".to_string()),
                fetched_at: now,
                fetch_source: "mock_reddit_api".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        reddit_resident_facts: Some(SkillFactsInput {
            source: "reddit".to_string(),
            snapshot_date: "2026-07-13".to_string(),
            facts: vec![SkillFactRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                value_type: "text".to_string(),
                value_json: r#"{"type":"Text","data":"Residents mention trees and open space"}"#
                    .to_string(),
                confidence: 0.72,
                source_type: "Reddit".to_string(),
                source_url: Some(
                    "https://reddit.com/r/BangaloreRealEstates/comments/greenery".to_string(),
                ),
                model: None,
                skill_id: Some("reddit_resident_fact_extractor".to_string()),
                triggered_by: Some("3bhk whitefield greenery".to_string()),
                learned_at: now + Duration::minutes(1),
                run_id: "skill-run-reddit-greenery".to_string(),
                input_hash: "sha256:reddit-greenery".to_string(),
            }],
            fact_annotations: vec![SkillFactAnnotationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                display_template: Some("Residents mention {value}".to_string()),
                answers_preferences_json: r#"["greenery","trees","open space"]"#.to_string(),
                scoring_direction: Some("TextMatch".to_string()),
                scoring_weight: Some(1.4),
                scoring_thresholds_json: "[]".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        google_review_facts: Some(SkillFactsInput {
            source: "google".to_string(),
            snapshot_date: "2026-07-13".to_string(),
            facts: vec![SkillFactRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                fact_key: "google_reviews_url".to_string(),
                value_type: "text".to_string(),
                value_json: r#"{"type":"Text","data":"https://maps.google.com/?cid=green-acre"}"#
                    .to_string(),
                confidence: 0.82,
                source_type: "Google".to_string(),
                source_url: Some("https://maps.google.com/?cid=green-acre".to_string()),
                model: None,
                skill_id: Some("fetch_google_review_links".to_string()),
                triggered_by: Some("green acre whitefield reviews".to_string()),
                learned_at: now + Duration::minutes(2),
                run_id: "skill-run-google-review-link".to_string(),
                input_hash: "sha256:google-green-acre".to_string(),
            }],
            fact_annotations: vec![SkillFactAnnotationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                fact_key: "google_reviews_url".to_string(),
                display_template: Some("Google reviews: {value}".to_string()),
                answers_preferences_json: r#"["google reviews","resident reviews"]"#.to_string(),
                scoring_direction: Some("TextMatch".to_string()),
                scoring_weight: Some(0.8),
                scoring_thresholds_json: "[]".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
    }
}

fn mock_rera_input(now: chrono::DateTime<Utc>) -> ReraRegistryMonthlyInput {
    ReraRegistryMonthlyInput {
        snapshot_date: "2026-07".to_string(),
        projects: vec![
            ReraProjectSnapshotRecord {
                ack_number: Some("ACK-RERA-MEADOWS-A".to_string()),
                registration_number: Some("PRM/KA/RERA/1251/446/PR/130726/009999".to_string()),
                project_name: "Duplicate Heights".to_string(),
                promoter_name: Some("Proof Homes Private Limited".to_string()),
                status: Some("Approved".to_string()),
                project_type: Some("Residential Apartment".to_string()),
                project_address: Some("Whitefield Main Road, Bengaluru".to_string()),
                area_name: Some("Whitefield".to_string()),
                district: Some("Bengaluru Urban".to_string()),
                taluk: Some("Bengaluru East".to_string()),
                total_land_area_sqm: Some(48_562.28),
                land_litigation: Some(false),
                source_url: "https://rera.karnataka.gov.in/projectViewDetails".to_string(),
                fetched_at: now,
            },
            ReraProjectSnapshotRecord {
                ack_number: Some("ACK-DUPLICATE-HEIGHTS-C".to_string()),
                registration_number: Some("PRM/KA/RERA/1251/446/PR/130726/007777".to_string()),
                project_name: "Duplicate Heights".to_string(),
                promoter_name: Some("Proof Homes Private Limited".to_string()),
                status: Some("Approved".to_string()),
                project_type: Some("Residential Apartment".to_string()),
                project_address: Some("Whitefield Main Road, Bengaluru".to_string()),
                area_name: Some("Whitefield".to_string()),
                district: Some("Bengaluru Urban".to_string()),
                taluk: Some("Bengaluru East".to_string()),
                total_land_area_sqm: Some(36_421.0),
                land_litigation: Some(false),
                source_url: "https://rera.karnataka.gov.in/projectViewDetails".to_string(),
                fetched_at: now,
            },
            ReraProjectSnapshotRecord {
                ack_number: Some("ACK-RERA-MEADOWS-B".to_string()),
                registration_number: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                project_name: "RERA Meadows".to_string(),
                promoter_name: Some("Proof Homes Private Limited".to_string()),
                status: Some("Approved".to_string()),
                project_type: Some("Residential Apartment".to_string()),
                project_address: Some("Whitefield Main Road, Bengaluru".to_string()),
                area_name: Some("Whitefield".to_string()),
                district: Some("Bengaluru Urban".to_string()),
                taluk: Some("Bengaluru East".to_string()),
                total_land_area_sqm: Some(40_468.56),
                land_litigation: Some(false),
                source_url: "https://rera.karnataka.gov.in/projectViewDetails".to_string(),
                fetched_at: now,
            },
        ],
        source_watermarks: Vec::new(),
    }
}

async fn parquet_rows_for_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
) -> i64 {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .unwrap_or_else(|| panic!("missing artifact ending in {suffix}"));
    let bytes = lake
        .get_bytes(&LakeKey::new(artifact.key.clone()).unwrap())
        .await
        .unwrap();
    parquet_rows(&bytes)
}

fn parquet_rows(bytes: &[u8]) -> i64 {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let reader = SerializedFileReader::new(File::open(file.path()).unwrap()).unwrap();
    reader.metadata().file_metadata().num_rows()
}

async fn serving_fact_rows(lake: &LakeStore, record: &MaterializationRecord) -> i64 {
    let manifest_artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("manifest.json"))
        .expect("serving record has manifest artifact");
    let manifest: ServingBundleManifest = lake
        .get_json(&LakeKey::new(manifest_artifact.key.clone()).unwrap())
        .await
        .unwrap();
    let bytes = lake
        .get_bytes(&LakeKey::new(manifest.fact_parquet_key).unwrap())
        .await
        .unwrap();
    parquet_rows(&bytes)
}

async fn parquet_contains_utf8(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
    column: &str,
    expected: &str,
) -> bool {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .unwrap_or_else(|| panic!("missing artifact ending in {suffix}"));
    let bytes = lake
        .get_bytes(&LakeKey::new(artifact.key.clone()).unwrap())
        .await
        .unwrap();
    let mut reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))
        .unwrap()
        .build()
        .unwrap();
    reader.any(|batch| {
        let batch = batch.unwrap();
        let values = batch
            .column_by_name(column)
            .unwrap_or_else(|| panic!("missing Parquet column {column}"))
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap_or_else(|| panic!("Parquet column {column} is not UTF-8"));
        (0..values.len()).any(|row| !values.is_null(row) && values.value(row) == expected)
    })
}
