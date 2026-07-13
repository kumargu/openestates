use backend::assets::{
    default_openestates_registry, ArtifactRef, AssetDagRunManifest, AssetDefinition, AssetId,
    AssetMaterializationStore, AssetPartition, AssetPlanner, AssetRunManifestStore,
    AssetRunStepStatus, AssetStage, CostTier, DagRunStatus, FreshnessReferenceKind,
    MaterializationRecord, PlanDecision, PlanReason, RefreshCadence, RunManifestError,
    SourceWatermark, TrustTier,
};
use backend::lake::{LakeKey, LakeStore};
use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn dag_plan_captures_freshness_and_dependency_propagation() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let registry = default_openestates_registry();
    let planner = AssetPlanner::new(registry.clone(), materializations.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let partition = AssetPartition::global();

    let rera_record = materialization(
        "rera_registry_monthly",
        AssetStage::Raw,
        "2026-05",
        Vec::new(),
        now,
    )
    .with_source_watermarks(vec![SourceWatermark {
        source: "rera".to_string(),
        high_watermark: "2026-05".to_string(),
    }]);
    write_current(&materializations, &rera_record).await;

    let canonical_record = materialization(
        "canonical_society_nodes",
        AssetStage::Gold,
        "2026-05",
        vec![rera_record.materialization_id.clone()],
        now - Duration::days(2),
    );
    write_current(&materializations, &canonical_record).await;

    let plan = planner.plan_global_details(now).await.unwrap();
    assert_eq!(plan.entries.len(), registry.definitions().len());

    let rera = plan_entry(&plan, "rera_registry_monthly");
    assert_eq!(rera.decision, PlanDecision::Run);
    assert_eq!(
        rera.current_materialization_id,
        Some(rera_record.materialization_id)
    );
    assert_eq!(rera.freshness.cadence, RefreshCadence::Monthly);
    assert_eq!(
        rera.freshness.reference_kind,
        FreshnessReferenceKind::SourceWatermark
    );
    assert_eq!(
        rera.freshness.reference_value.as_deref(),
        Some("rera:2026-05")
    );
    assert_eq!(rera.freshness.max_age_seconds, Some(31 * 24 * 60 * 60));
    assert!(rera.freshness.is_stale);
    assert!(matches!(
        rera.reason,
        Some(PlanReason::Stale {
            cadence: RefreshCadence::Monthly,
            ..
        })
    ));

    let canonical = plan_entry(&plan, "canonical_society_nodes");
    assert_eq!(canonical.decision, PlanDecision::Run);
    assert_eq!(
        canonical.reason,
        Some(PlanReason::DependencyPending {
            asset_id: asset_id("rera_registry_monthly")
        })
    );
    assert_eq!(canonical.freshness.cadence, RefreshCadence::OnChange);
    assert_eq!(canonical.freshness.max_age_seconds, None);
    assert!(!canonical.freshness.is_stale);

    let missing_support = plan_entry(&plan, "reddit_threads_daily");
    assert_eq!(missing_support.decision, PlanDecision::Run);
    assert_eq!(missing_support.reason, Some(PlanReason::Missing));
    assert_eq!(missing_support.current_materialization_id, None);

    let planned_assets = planner.plan_partition(&partition, now).await.unwrap();
    assert_eq!(planned_assets.len(), plan.run_entries().count());
}

#[tokio::test]
async fn dag_plan_skips_current_assets_with_parent_lineage_intact() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let registry = two_asset_registry();
    let planner = AssetPlanner::new(registry, materializations.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

    let root_record = materialization(
        "root_snapshot",
        AssetStage::Raw,
        "2026-07",
        Vec::new(),
        now - Duration::days(1),
    );
    write_current(&materializations, &root_record).await;

    let child_record = materialization(
        "derived_view",
        AssetStage::Gold,
        "2026-07",
        vec![root_record.materialization_id.clone()],
        now - Duration::hours(12),
    );
    write_current(&materializations, &child_record).await;

    let plan = planner.plan_global_details(now).await.unwrap();

    assert_eq!(planner.plan_global(now).await.unwrap(), Vec::new());
    assert_eq!(plan.run_entries().count(), 0);
    assert_eq!(plan.skipped_entries().count(), 2);
    assert!(plan.entries.iter().all(|entry| {
        entry.decision == PlanDecision::Skip && entry.reason.is_none() && !entry.freshness.is_stale
    }));

    let child = plan_entry(&plan, "derived_view");
    assert_eq!(
        child.current_parent_materializations,
        vec![root_record.materialization_id]
    );
}

#[tokio::test]
async fn dag_run_manifest_round_trips_with_counts_duration_and_current_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let run_store = AssetRunManifestStore::new(lake.clone());
    let registry = one_asset_registry();
    let planner = AssetPlanner::new(registry, materializations);
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

    let plan = planner.plan_global_details(now).await.unwrap();
    let mut manifest = AssetDagRunManifest::from_plan(&plan);

    assert_eq!(manifest.status, DagRunStatus::Planned);
    assert_eq!(manifest.total_assets, 1);
    assert_eq!(manifest.planned_count, 1);
    assert_eq!(manifest.skipped_count, 0);

    let asset_id = asset_id("root_snapshot");
    let artifact_meta = lake
        .put_json(
            &LakeKey::new("raw/source=test/dt=2026-07/run_id=test/artifact.json").unwrap(),
            &json!({"ok": true}),
        )
        .await
        .unwrap();
    let record = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Raw,
        AssetPartition::global(),
        "2026-07",
        vec![ArtifactRef::json(artifact_meta)],
    )
    .with_run_id(manifest.run_id.clone())
    .with_row_count(42);
    let started_at = now + Duration::seconds(2);
    let completed_at = started_at + Duration::milliseconds(1234);

    manifest
        .mark_step_succeeded(&asset_id, &record, started_at, completed_at)
        .unwrap();
    manifest.finish(completed_at).unwrap();

    assert_eq!(manifest.status, DagRunStatus::Succeeded);
    assert_eq!(manifest.succeeded_count, 1);
    assert_eq!(manifest.failed_count, 0);
    assert_eq!(manifest.steps[0].status, AssetRunStepStatus::Succeeded);
    assert_eq!(manifest.steps[0].row_count, Some(42));
    assert_eq!(manifest.steps[0].duration_ms, Some(1234));
    assert_eq!(
        manifest.steps[0].materialization_id,
        Some(record.materialization_id)
    );

    let manifest_meta = run_store.write_manifest(&manifest).await.unwrap();
    assert!(manifest_meta
        .key
        .as_str()
        .starts_with("manifests/runs/partition=global/"));
    assert!(manifest_meta.key.as_str().ends_with(".json"));
    run_store.promote_current(&manifest).await.unwrap();

    let current = run_store
        .current_manifest(&AssetPartition::global())
        .await
        .unwrap();
    assert_eq!(current.run_id, manifest.run_id);
    assert_eq!(current.status, DagRunStatus::Succeeded);
    assert_eq!(current.steps[0].artifacts.len(), 1);
}

#[tokio::test]
async fn dag_run_manifest_rejects_loose_step_transitions() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let registry = one_asset_registry();
    let planner = AssetPlanner::new(registry, materializations);
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let plan = planner.plan_global_details(now).await.unwrap();
    let mut manifest = AssetDagRunManifest::from_plan(&plan);
    let asset_id = asset_id("root_snapshot");

    let wrong_run_record = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Raw,
        AssetPartition::global(),
        "2026-07",
        Vec::new(),
    );
    assert!(matches!(
        manifest.mark_step_succeeded(
            &asset_id,
            &wrong_run_record,
            now,
            now + Duration::seconds(1),
        ),
        Err(RunManifestError::RunIdMismatch { .. })
    ));

    assert!(matches!(
        manifest.finish(now),
        Err(RunManifestError::IncompleteRun { .. })
    ));

    let current_record =
        materialization("root_snapshot", AssetStage::Raw, "2026-07", Vec::new(), now);
    let current_store = AssetMaterializationStore::new(lake);
    write_current(&current_store, &current_record).await;
    let current_plan = AssetPlanner::new(two_asset_registry(), current_store.clone())
        .plan_global_details(now)
        .await
        .unwrap();
    let mut current_manifest = AssetDagRunManifest::from_plan(&current_plan);

    assert!(matches!(
        current_manifest.mark_step_running(&asset_id, now),
        Err(RunManifestError::InvalidStepTransition { .. })
    ));
}

#[tokio::test]
async fn dag_run_current_pointers_are_partition_scoped() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake);
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let global_plan = backend::assets::AssetDagPlan {
        run_id: backend::assets::MaterializationId::new(),
        partition: AssetPartition::global(),
        planned_at: now,
        entries: Vec::new(),
    };
    let reddit_partition = AssetPartition::new([("source", "reddit"), ("dt", "2026-07-13")]);
    let reddit_plan = backend::assets::AssetDagPlan {
        run_id: backend::assets::MaterializationId::new(),
        partition: reddit_partition.clone(),
        planned_at: now,
        entries: Vec::new(),
    };

    let mut global_manifest = AssetDagRunManifest::from_plan(&global_plan);
    global_manifest.finish(now).unwrap();
    let mut reddit_manifest = AssetDagRunManifest::from_plan(&reddit_plan);
    reddit_manifest.finish(now).unwrap();

    run_store.write_manifest(&global_manifest).await.unwrap();
    run_store.promote_current(&global_manifest).await.unwrap();
    run_store.write_manifest(&reddit_manifest).await.unwrap();
    run_store.promote_current(&reddit_manifest).await.unwrap();

    let current_global = run_store
        .current_manifest(&AssetPartition::global())
        .await
        .unwrap();
    let current_reddit = run_store.current_manifest(&reddit_partition).await.unwrap();

    assert_eq!(current_global.run_id, global_manifest.run_id);
    assert_eq!(current_reddit.run_id, reddit_manifest.run_id);
    assert_ne!(current_global.run_id, current_reddit.run_id);
}

async fn write_current(store: &AssetMaterializationStore, record: &MaterializationRecord) {
    store.write_materialization(record).await.unwrap();
    store.promote_current(record).await.unwrap();
}

fn materialization(
    id: &str,
    stage: AssetStage,
    version: &str,
    parents: Vec<backend::assets::MaterializationId>,
    created_at: chrono::DateTime<Utc>,
) -> MaterializationRecord {
    let mut record = MaterializationRecord::succeeded(
        asset_id(id),
        stage,
        AssetPartition::global(),
        version,
        Vec::new(),
    )
    .with_parent_materializations(parents)
    .with_row_count(1);
    record.created_at = created_at;
    record
}

fn one_asset_registry() -> backend::assets::AssetRegistry {
    backend::assets::AssetRegistry::new(vec![asset(
        "root_snapshot",
        AssetStage::Raw,
        &[],
        RefreshCadence::Monthly,
    )])
    .unwrap()
}

fn two_asset_registry() -> backend::assets::AssetRegistry {
    backend::assets::AssetRegistry::new(vec![
        asset(
            "root_snapshot",
            AssetStage::Raw,
            &[],
            RefreshCadence::Monthly,
        ),
        asset(
            "derived_view",
            AssetStage::Gold,
            &["root_snapshot"],
            RefreshCadence::OnChange,
        ),
    ])
    .unwrap()
}

fn asset(
    id: &str,
    stage: AssetStage,
    dependencies: &[&str],
    refresh: RefreshCadence,
) -> AssetDefinition {
    AssetDefinition::new(
        asset_id(id),
        stage,
        format!("test asset {id}"),
        dependencies
            .iter()
            .map(|dependency| asset_id(dependency))
            .collect(),
        refresh,
        CostTier::Free,
        TrustTier::Derived,
    )
}

fn asset_id(id: &str) -> AssetId {
    AssetId::new(id).unwrap()
}

fn plan_entry<'a>(
    plan: &'a backend::assets::AssetDagPlan,
    id: &str,
) -> &'a backend::assets::AssetPlanEntry {
    let asset_id = asset_id(id);
    plan.entries
        .iter()
        .find(|entry| entry.asset_id == asset_id)
        .expect("asset in plan")
}
