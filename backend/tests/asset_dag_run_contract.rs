use backend::assets::{
    default_openestates_registry, ArtifactRef, AssetDagRunManifest, AssetDefinition, AssetId,
    AssetMaterializationStore, AssetPartition, AssetPlanner, AssetRunManifestStore,
    AssetRunStepStatus, AssetStage, CostTier, DagRunStatus, FreshnessReferenceKind,
    MaterializationId, MaterializationRecord, PartitionResolutionError, PlanDecision, PlanReason,
    PlannerError, RefreshCadence, RunManifestError, SourceWatermark, TrustTier,
};
use backend::lake::{LakeError, LakeKey, LakeStore};
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
    let partition =
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);

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

    let plan = planner
        .plan_partition_details(&partition, now)
        .await
        .unwrap();
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
async fn dag_plan_resolves_current_records_by_asset_partition() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let registry = default_openestates_registry();
    let planner = AssetPlanner::new(registry, materializations.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition =
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);
    let reddit_thread_partition =
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);
    let reddit_fact_partition = AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]);
    let google_fact_partition = AssetPartition::new([("source", "google")]);

    let rera = materialization_in_partition(
        "rera_registry_monthly",
        AssetStage::Raw,
        "2026-07",
        Vec::new(),
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &rera).await;
    let canonical = materialization_in_partition(
        "canonical_society_nodes",
        AssetStage::Gold,
        "2026-07-13",
        vec![rera.materialization_id.clone()],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &canonical).await;
    let rera_facts = materialization_in_partition(
        "rera_legal_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            rera.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &rera_facts).await;
    let reddit_threads = materialization_in_partition(
        "reddit_threads_daily",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        reddit_thread_partition.clone(),
    );
    write_current(&materializations, &reddit_threads).await;
    let reddit_facts = materialization_in_partition(
        "reddit_resident_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            reddit_threads.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        reddit_fact_partition.clone(),
    );
    write_current(&materializations, &reddit_facts).await;
    let google_places = materialization_in_partition(
        "google_places_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        google_fact_partition.clone(),
    );
    write_current(&materializations, &google_places).await;
    let google_facts = materialization_in_partition(
        "google_review_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            google_places.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        google_fact_partition.clone(),
    );
    write_current(&materializations, &google_facts).await;
    let plan = planner
        .plan_partition_details(&run_partition, now)
        .await
        .unwrap();

    let reddit_threads_entry = plan_entry(&plan, "reddit_threads_daily");
    assert_eq!(reddit_threads_entry.partition, reddit_thread_partition);
    assert_eq!(reddit_threads_entry.decision, PlanDecision::Skip);
    assert_eq!(
        reddit_threads_entry.current_materialization_id,
        Some(reddit_threads.materialization_id.clone())
    );

    let reddit_facts_entry = plan_entry(&plan, "reddit_resident_facts");
    assert_eq!(reddit_facts_entry.partition, reddit_fact_partition);
    assert_eq!(reddit_facts_entry.decision, PlanDecision::Skip);
    assert_eq!(
        reddit_facts_entry.current_parent_materializations,
        vec![
            reddit_threads.materialization_id.clone(),
            canonical.materialization_id.clone()
        ]
    );

    let google_facts_entry = plan_entry(&plan, "google_review_facts");
    assert_eq!(google_facts_entry.partition, google_fact_partition);
    assert_eq!(google_facts_entry.decision, PlanDecision::Skip);
    assert_eq!(
        google_facts_entry.current_parent_materializations,
        vec![
            google_places.materialization_id.clone(),
            canonical.materialization_id.clone()
        ]
    );

    let next_day_partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
    let next_day_plan = planner
        .plan_partition_details(&next_day_partition, now + Duration::days(1))
        .await
        .unwrap();
    assert_eq!(
        plan_entry(&next_day_plan, "google_places_weekly").decision,
        PlanDecision::Skip
    );
    assert_eq!(
        plan_entry(&next_day_plan, "google_review_facts").decision,
        PlanDecision::Skip
    );

    let kg_entry = plan_entry(&plan, "kg_society_view");
    assert_eq!(kg_entry.partition, AssetPartition::global());
    assert_eq!(kg_entry.decision, PlanDecision::Run);
    assert_eq!(kg_entry.reason, Some(PlanReason::Missing));
}

#[tokio::test]
async fn dag_plan_fans_all_current_support_partitions_into_global_kg_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let planner = AssetPlanner::new(default_openestates_registry(), materializations.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition =
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);

    let rera = materialization_in_partition(
        "rera_registry_monthly",
        AssetStage::Raw,
        "2026-07",
        Vec::new(),
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &rera).await;
    let canonical = materialization_in_partition(
        "canonical_society_nodes",
        AssetStage::Gold,
        "2026-07-13",
        vec![rera.materialization_id.clone()],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &canonical).await;
    let rera_facts = materialization_in_partition(
        "rera_legal_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            rera.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &rera_facts).await;

    let reddit_facts_old = materialization_in_partition(
        "reddit_resident_facts",
        AssetStage::Silver,
        "2026-07-12",
        vec![canonical.materialization_id.clone()],
        now - Duration::days(1),
        AssetPartition::new([("dt", "2026-07-12"), ("source", "reddit")]),
    );
    write_current(&materializations, &reddit_facts_old).await;
    let reddit_threads = materialization_in_partition(
        "reddit_threads_daily",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]),
    );
    write_current(&materializations, &reddit_threads).await;
    let reddit_facts_new = materialization_in_partition(
        "reddit_resident_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            reddit_threads.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]),
    );
    write_current(&materializations, &reddit_facts_new).await;
    let legacy_global_reddit_facts = materialization_in_partition(
        "reddit_resident_facts",
        AssetStage::Silver,
        "legacy-global",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &legacy_global_reddit_facts).await;
    let google_places = materialization_in_partition(
        "google_places_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("source", "google")]),
    );
    write_current(&materializations, &google_places).await;
    let google_facts = materialization_in_partition(
        "google_review_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            google_places.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "google")]),
    );
    write_current(&materializations, &google_facts).await;
    let community_facts_stale = materialization_in_partition(
        "community_review_summary_facts",
        AssetStage::Silver,
        "2026-07-12",
        vec![
            google_facts.materialization_id.clone(),
            reddit_facts_old.materialization_id.clone(),
        ],
        now - Duration::hours(1),
        AssetPartition::new([("source", "community")]),
    );
    write_current(&materializations, &community_facts_stale).await;
    let google_nearby_places = materialization_in_partition(
        "google_nearby_places_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("source", "google")]),
    );
    write_current(&materializations, &google_nearby_places).await;
    let google_nearby_facts = materialization_in_partition(
        "google_nearby_place_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            google_nearby_places.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "google")]),
    );
    write_current(&materializations, &google_nearby_facts).await;
    let prestige_inventory = materialization_in_partition(
        "prestige_inventory_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("source", "prestige")]),
    );
    write_current(&materializations, &prestige_inventory).await;
    let market_facts = materialization_in_partition(
        "market_project_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            prestige_inventory.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "prestige")]),
    );
    write_current(&materializations, &market_facts).await;
    let external_listings = materialization_in_partition(
        "external_listings_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("source", "external_listing")]),
    );
    write_current(&materializations, &external_listings).await;
    let external_listing_facts = materialization_in_partition(
        "external_listing_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            external_listings.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "external_listing")]),
    );
    write_current(&materializations, &external_listing_facts).await;
    let external_images = materialization_in_partition(
        "external_images_weekly",
        AssetStage::Raw,
        "2026-07-13",
        vec![canonical.materialization_id.clone()],
        now,
        AssetPartition::new([("source", "external_image")]),
    );
    write_current(&materializations, &external_images).await;
    let image_media_facts = materialization_in_partition(
        "image_media_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            external_images.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "external_image")]),
    );
    write_current(&materializations, &image_media_facts).await;
    let metro_stations = materialization_in_partition(
        "metro_stations_monthly",
        AssetStage::Raw,
        "2026-07-13",
        Vec::new(),
        now,
        AssetPartition::new([("source", "openstreetmap")]),
    );
    write_current(&materializations, &metro_stations).await;
    let metro_facts = materialization_in_partition(
        "metro_proximity_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            metro_stations.materialization_id.clone(),
            rera_facts.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "openstreetmap")]),
    );
    write_current(&materializations, &metro_facts).await;
    let builder_facts = materialization_in_partition(
        "builder_rera_aggregates",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            rera.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &builder_facts).await;

    let stale_kg = materialization_in_partition(
        "kg_society_view",
        AssetStage::Gold,
        "2026-07-12",
        vec![
            canonical.materialization_id.clone(),
            rera_facts.materialization_id.clone(),
            reddit_facts_old.materialization_id.clone(),
            google_facts.materialization_id.clone(),
            community_facts_stale.materialization_id.clone(),
            google_nearby_facts.materialization_id.clone(),
            market_facts.materialization_id.clone(),
            external_listing_facts.materialization_id.clone(),
            image_media_facts.materialization_id.clone(),
            metro_facts.materialization_id.clone(),
            builder_facts.materialization_id.clone(),
        ],
        now - Duration::hours(1),
        AssetPartition::global(),
    );
    write_current(&materializations, &stale_kg).await;

    let plan = planner
        .plan_partition_details(&run_partition, now)
        .await
        .unwrap();

    let community_entry = plan_entry(&plan, "community_review_summary_facts");
    assert_eq!(community_entry.decision, PlanDecision::Run);
    assert_eq!(
        community_entry.reason,
        Some(PlanReason::DependencyChanged {
            asset_id: asset_id("reddit_resident_facts")
        })
    );
    let kg_entry = plan_entry(&plan, "kg_society_view");
    assert_eq!(kg_entry.decision, PlanDecision::Run);
    assert_eq!(
        kg_entry.reason,
        Some(PlanReason::DependencyPending {
            asset_id: asset_id("community_review_summary_facts")
        })
    );

    let community_facts_fresh = materialization_in_partition(
        "community_review_summary_facts",
        AssetStage::Silver,
        "2026-07-13",
        vec![
            google_facts.materialization_id.clone(),
            reddit_facts_old.materialization_id.clone(),
            reddit_facts_new.materialization_id.clone(),
        ],
        now,
        AssetPartition::new([("source", "community")]),
    );
    write_current(&materializations, &community_facts_fresh).await;

    let fresh_kg = materialization_in_partition(
        "kg_society_view",
        AssetStage::Gold,
        "2026-07-13",
        vec![
            canonical.materialization_id.clone(),
            rera_facts.materialization_id.clone(),
            reddit_facts_old.materialization_id.clone(),
            reddit_facts_new.materialization_id.clone(),
            google_facts.materialization_id.clone(),
            community_facts_fresh.materialization_id.clone(),
            google_nearby_facts.materialization_id.clone(),
            market_facts.materialization_id.clone(),
            external_listing_facts.materialization_id.clone(),
            image_media_facts.materialization_id.clone(),
            metro_facts.materialization_id.clone(),
            builder_facts.materialization_id.clone(),
        ],
        now,
        AssetPartition::global(),
    );
    write_current(&materializations, &fresh_kg).await;

    let plan = planner
        .plan_partition_details(&run_partition, now)
        .await
        .unwrap();
    let kg_entry = plan_entry(&plan, "kg_society_view");
    assert_eq!(kg_entry.decision, PlanDecision::Skip);
    assert_eq!(
        kg_entry.current_parent_materializations,
        fresh_kg.parent_materializations
    );
    assert!(!kg_entry
        .current_parent_materializations
        .contains(&legacy_global_reddit_facts.materialization_id));
}

#[tokio::test]
async fn dag_plan_errors_when_partition_policy_requires_missing_run_key() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let planner = AssetPlanner::new(default_openestates_registry(), materializations);
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = AssetPartition::new([("dt", "2026-07-13")]);

    let err = planner
        .plan_partition_details(&run_partition, now)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        PlannerError::Partition(PartitionResolutionError::MissingRunPartitionKey {
            asset_id: ref missing_asset_id,
            ref key,
            ..
        }) if missing_asset_id == &asset_id("reddit_threads_daily") && key == "subreddit"
    ));
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
async fn dag_run_manifest_validates_step_partition_not_run_partition() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materializations = AssetMaterializationStore::new(lake);
    let registry = one_asset_registry();
    let planner = AssetPlanner::new(registry, materializations);
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = AssetPartition::new([("dt", "2026-07-13")]);
    let plan = planner
        .plan_partition_details(&run_partition, now)
        .await
        .unwrap();
    assert_eq!(plan.entries[0].partition, AssetPartition::global());

    let asset_id = asset_id("root_snapshot");
    let mut manifest = AssetDagRunManifest::from_plan(&plan);
    let record = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Raw,
        AssetPartition::global(),
        "2026-07",
        Vec::new(),
    )
    .with_run_id(manifest.run_id.clone());

    manifest
        .mark_step_succeeded(&asset_id, &record, now, now + Duration::milliseconds(10))
        .unwrap();

    let mut mismatch_manifest = AssetDagRunManifest::from_plan(&plan);
    let wrong_partition_record = MaterializationRecord::succeeded(
        asset_id.clone(),
        AssetStage::Raw,
        run_partition,
        "2026-07",
        Vec::new(),
    )
    .with_run_id(mismatch_manifest.run_id.clone());

    assert!(matches!(
        mismatch_manifest.mark_step_succeeded(
            &asset_id,
            &wrong_partition_record,
            now,
            now + Duration::milliseconds(10),
        ),
        Err(RunManifestError::PartitionMismatch { .. })
    ));
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
async fn dag_run_manifest_records_attempts_blocks_dependents_and_prepares_exact_resume() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let plan = AssetPlanner::new(resilience_registry(), AssetMaterializationStore::new(lake))
        .plan_global_details(now)
        .await
        .unwrap();
    let mut manifest = AssetDagRunManifest::from_plan(&plan);
    let stable = asset_id("stable_root");
    let flaky = asset_id("flaky_root");
    let joined = asset_id("joined_view");

    let first_started = now + Duration::seconds(1);
    manifest.mark_step_running(&stable, first_started).unwrap();
    manifest
        .mark_step_attempt_failed(
            &stable,
            first_started,
            first_started + Duration::milliseconds(50),
            "temporary object-store timeout",
        )
        .unwrap();
    let second_started = now + Duration::seconds(2);
    manifest.mark_step_running(&stable, second_started).unwrap();
    let stable_record = MaterializationRecord::succeeded(
        stable.clone(),
        AssetStage::Raw,
        AssetPartition::global(),
        "2026-07",
        Vec::new(),
    )
    .with_run_id(manifest.run_id.clone());
    manifest
        .mark_step_succeeded(
            &stable,
            &stable_record,
            second_started,
            second_started + Duration::milliseconds(25),
        )
        .unwrap();

    let failed_started = now + Duration::seconds(3);
    manifest.mark_step_running(&flaky, failed_started).unwrap();
    manifest
        .mark_step_failed(
            &flaky,
            failed_started,
            failed_started + Duration::milliseconds(10),
            "invalid source payload",
        )
        .unwrap();
    manifest
        .mark_step_blocked(&joined, now + Duration::seconds(4), vec![flaky.clone()])
        .unwrap();
    manifest.finish(now + Duration::seconds(5)).unwrap();

    assert_eq!(manifest.status, DagRunStatus::Failed);
    assert_eq!(manifest.failed_count, 1);
    assert_eq!(manifest.blocked_count, 1);
    let stable_step = manifest
        .steps
        .iter()
        .find(|step| step.asset_id == stable)
        .unwrap();
    assert_eq!(stable_step.attempts.len(), 2);
    assert_eq!(stable_step.attempts[0].attempt, 1);
    assert_eq!(stable_step.attempts[1].attempt, 2);
    let joined_step = manifest
        .steps
        .iter()
        .find(|step| step.asset_id == joined)
        .unwrap();
    assert_eq!(joined_step.status, AssetRunStepStatus::Blocked);
    assert_eq!(joined_step.blocked_by, vec![flaky.clone()]);

    run_store.write_manifest(&manifest).await.unwrap();
    let loaded = run_store
        .manifest(&manifest.partition, &manifest.run_id)
        .await
        .unwrap();
    assert_eq!(loaded, manifest);

    let resumed = loaded.prepare_resume(now + Duration::seconds(6)).unwrap();
    assert_eq!(resumed.run_id, manifest.run_id);
    assert_eq!(resumed.status, DagRunStatus::Running);
    let resumed_stable = resumed
        .steps
        .iter()
        .find(|step| step.asset_id == stable)
        .unwrap();
    assert_eq!(resumed_stable.status, AssetRunStepStatus::Succeeded);
    assert_eq!(resumed_stable.attempts.len(), 2);
    let resumed_flaky = resumed
        .steps
        .iter()
        .find(|step| step.asset_id == flaky)
        .unwrap();
    assert_eq!(resumed_flaky.status, AssetRunStepStatus::Planned);
    assert_eq!(resumed_flaky.attempts.len(), 1);
    let resumed_joined = resumed
        .steps
        .iter()
        .find(|step| step.asset_id == joined)
        .unwrap();
    assert_eq!(resumed_joined.status, AssetRunStepStatus::Planned);
    assert!(resumed_joined.blocked_by.is_empty());
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

#[tokio::test]
async fn dag_run_manifest_cas_rejects_a_stale_same_run_writer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake);
    let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let plan = backend::assets::AssetDagPlan {
        run_id: MaterializationId::new(),
        partition: AssetPartition::global(),
        planned_at: now,
        entries: Vec::new(),
    };
    let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "cas-v1");

    run_store.write_manifest_cas(&mut manifest).await.unwrap();
    assert_eq!(manifest.revision, 1);

    let mut first_writer = manifest.clone();
    let mut stale_writer = manifest.clone();
    first_writer.status = DagRunStatus::Running;
    stale_writer.status = DagRunStatus::Failed;

    run_store
        .write_manifest_cas(&mut first_writer)
        .await
        .unwrap();
    let error = run_store
        .write_manifest_cas(&mut stale_writer)
        .await
        .unwrap_err();

    assert!(matches!(error, LakeError::ConcurrentModification(_)));
    assert_eq!(stale_writer.revision, 1);
    let persisted = run_store
        .manifest(&first_writer.partition, &first_writer.run_id)
        .await
        .unwrap();
    assert_eq!(persisted, first_writer);
    assert_eq!(persisted.revision, 2);
}

#[tokio::test]
async fn active_resume_lease_blocks_a_staggered_second_owner() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake);
    let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let plan = backend::assets::AssetDagPlan {
        run_id: MaterializationId::new(),
        partition: AssetPartition::global(),
        planned_at: now,
        entries: Vec::new(),
    };
    let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "lease-v1");
    manifest.status = DagRunStatus::Running;
    run_store.write_manifest_cas(&mut manifest).await.unwrap();

    let first_owner = MaterializationId::new();
    run_store
        .acquire_resume_lease(
            &mut manifest,
            first_owner.clone(),
            now,
            Duration::minutes(30),
        )
        .await
        .unwrap();
    let mut observed_after_first_claim = run_store
        .manifest(&manifest.partition, &manifest.run_id)
        .await
        .unwrap();
    let second_owner = MaterializationId::new();
    let error = run_store
        .acquire_resume_lease(
            &mut observed_after_first_claim,
            second_owner.clone(),
            now + Duration::minutes(1),
            Duration::minutes(30),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, LakeError::ConcurrentModification(_)));

    assert!(run_store
        .release_resume_lease(&manifest.partition, &manifest.run_id, &first_owner)
        .await
        .unwrap());
    let mut released = run_store
        .manifest(&manifest.partition, &manifest.run_id)
        .await
        .unwrap();
    run_store
        .acquire_resume_lease(
            &mut released,
            second_owner.clone(),
            now + Duration::minutes(2),
            Duration::minutes(30),
        )
        .await
        .unwrap();
    assert_eq!(
        released.resume_lease.as_ref().map(|lease| &lease.owner_id),
        Some(&second_owner)
    );
}

#[test]
fn exact_resume_rejects_legacy_manifests_without_a_snapshot_contract() {
    let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let plan = backend::assets::AssetDagPlan {
        run_id: MaterializationId::new(),
        partition: AssetPartition::global(),
        planned_at: now,
        entries: Vec::new(),
    };
    let mut current = AssetDagRunManifest::from_plan_with_version(&plan, "resume-v1");
    current.status = DagRunStatus::Running;
    let mut value = serde_json::to_value(current).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("format_version");
    object.remove("revision");
    let legacy: AssetDagRunManifest = serde_json::from_value(value).unwrap();

    assert_eq!(legacy.format_version, 0);
    assert_eq!(legacy.revision, 0);
    assert!(matches!(
        legacy.ensure_exact_resume(),
        Err(RunManifestError::UnsupportedResumeManifest { format_version: 0 })
    ));
}

#[tokio::test]
async fn older_completed_run_cannot_replace_newer_current_run() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake);
    let older_time = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let newer_time = older_time + Duration::hours(1);
    let partition = AssetPartition::global();
    let mut older = AssetDagRunManifest::from_plan(&backend::assets::AssetDagPlan {
        run_id: backend::assets::MaterializationId::new(),
        partition: partition.clone(),
        planned_at: older_time,
        entries: Vec::new(),
    });
    older.finish(older_time).unwrap();
    let mut newer = AssetDagRunManifest::from_plan(&backend::assets::AssetDagPlan {
        run_id: backend::assets::MaterializationId::new(),
        partition: partition.clone(),
        planned_at: newer_time,
        entries: Vec::new(),
    });
    newer.finish(newer_time).unwrap();
    run_store.write_manifest(&older).await.unwrap();
    run_store.write_manifest(&newer).await.unwrap();

    assert!(run_store.promote_current(&newer).await.unwrap());
    assert!(!run_store.promote_current(&older).await.unwrap());
    assert_eq!(
        run_store.current_manifest(&partition).await.unwrap().run_id,
        newer.run_id
    );
}

#[tokio::test]
async fn resumed_run_can_replace_its_own_failed_current_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let run_store = AssetRunManifestStore::new(lake);
    let created_at = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let partition = AssetPartition::global();
    let mut manifest = AssetDagRunManifest::from_plan(&backend::assets::AssetDagPlan {
        run_id: backend::assets::MaterializationId::new(),
        partition: partition.clone(),
        planned_at: created_at,
        entries: Vec::new(),
    });
    manifest.status = DagRunStatus::Failed;
    manifest.completed_at = Some(created_at);
    run_store.write_manifest(&manifest).await.unwrap();
    assert!(run_store.promote_current(&manifest).await.unwrap());

    manifest.status = DagRunStatus::Succeeded;
    run_store.write_manifest(&manifest).await.unwrap();
    assert!(run_store.promote_current(&manifest).await.unwrap());
    assert_eq!(
        run_store.current_manifest(&partition).await.unwrap().status,
        DagRunStatus::Succeeded
    );
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
    materialization_in_partition(
        id,
        stage,
        version,
        parents,
        created_at,
        AssetPartition::global(),
    )
}

fn materialization_in_partition(
    id: &str,
    stage: AssetStage,
    version: &str,
    parents: Vec<backend::assets::MaterializationId>,
    created_at: chrono::DateTime<Utc>,
    partition: AssetPartition,
) -> MaterializationRecord {
    let mut record =
        MaterializationRecord::succeeded(asset_id(id), stage, partition, version, Vec::new())
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

fn resilience_registry() -> backend::assets::AssetRegistry {
    backend::assets::AssetRegistry::new(vec![
        asset("stable_root", AssetStage::Raw, &[], RefreshCadence::Daily),
        asset("flaky_root", AssetStage::Raw, &[], RefreshCadence::Daily),
        asset(
            "joined_view",
            AssetStage::Gold,
            &["stable_root", "flaky_root"],
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
