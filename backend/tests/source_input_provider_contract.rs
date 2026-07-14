use std::fs;

use backend::assets::{
    default_openestates_registry, AssetDagExecutionOptions, AssetDagExecutor, AssetDagRunManifest,
    AssetDefinition, AssetId, AssetMaterializationStore, AssetPartition, AssetPartitionPolicy,
    AssetRegistry, AssetRunStepStatus, AssetSourceInputs, AssetStage, CanonicalSocietyMaterializer,
    CommandSourceInputProvider, CostTier, GooglePlaceSnapshotMaterializer,
    GooglePlaceSnapshotRecord, GooglePlacesWeeklyInput, MaterializationId, MaterializationRecord,
    RedditThreadSnapshotMaterializer, RedditThreadSnapshotRecord, RedditThreadsDailyInput,
    RefreshCadence, ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceInputProvider,
    SourceInputProviderError, SourceInputRequest, SourceWatermark, TrustTier,
};
use backend::knowledge::KnowledgeGraph;
use backend::lake::LakeStore;
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

#[cfg(unix)]
#[tokio::test]
async fn command_provider_exchanges_typed_json_without_shell_interpolation() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let project_root = temp.path().join("project root with spaces");
    let lake = LakeStore::local(temp.path().join("lake")).unwrap();
    fs::create_dir_all(&project_root).unwrap();

    let request_path = temp.path().join("captured request.json");
    let collector_path = temp.path().join("mock collector.sh");
    fs::write(
        &collector_path,
        r#"#!/bin/sh
set -eu
cat > "$1"
printf '%s' '{"reddit_threads_daily":{"snapshot_date":"2026-07-14","subreddit":"BangaloreRealEstates","records":[]}}'
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&collector_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&collector_path, permissions).unwrap();

    let provider = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(collector_path.as_os_str().to_owned())
        .with_arg(request_path.as_os_str().to_owned());
    let request = SourceInputRequest {
        project_root: project_root.clone(),
        partition: AssetPartition::new([
            ("dt", "2026-07-14"),
            ("subreddit", "BangaloreRealEstates"),
        ]),
        planned_at: Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap(),
        requested_assets: vec![AssetId::new("reddit_threads_daily").unwrap()],
        force_refresh_assets: Vec::new(),
        source_entities: Vec::new(),
    };

    let inputs = provider
        .load(&request, &lake)
        .await
        .unwrap()
        .expect("collector returned source inputs");
    let reddit = inputs
        .reddit_threads_daily
        .expect("reddit source input returned");
    assert_eq!(reddit.snapshot_date, "2026-07-14");
    assert_eq!(reddit.subreddit, "BangaloreRealEstates");

    let captured: SourceInputRequest =
        serde_json::from_slice(&fs::read(request_path).unwrap()).unwrap();
    assert_eq!(captured, request);
}

#[cfg(unix)]
#[tokio::test]
async fn command_provider_reports_nonzero_exit() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let collector_path = temp.path().join("failing-collector.sh");
    fs::write(&collector_path, "#!/bin/sh\ncat >/dev/null\nexit 17\n").unwrap();
    let mut permissions = fs::metadata(&collector_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&collector_path, permissions).unwrap();

    let error = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(collector_path.as_os_str().to_owned())
        .load(
            &request(temp.path()),
            &LakeStore::local(temp.path()).unwrap(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        SourceInputProviderError::CommandFailed {
            exit_code: Some(17),
            ..
        }
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn command_provider_times_out_and_caps_stdout() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let collector_path = temp.path().join("busy-collector.sh");
    fs::write(
        &collector_path,
        "#!/bin/sh\ncat >/dev/null\nwhile :; do :; done\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&collector_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&collector_path, permissions).unwrap();

    let error = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(collector_path.as_os_str().to_owned())
        .with_timeout(Duration::from_millis(20))
        .with_max_stdout_bytes(128)
        .load(
            &request(temp.path()),
            &LakeStore::local(temp.path()).unwrap(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, SourceInputProviderError::TimedOut { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn command_provider_rejects_oversized_or_malformed_output() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let collector_path = temp.path().join("oversized-output.sh");
    fs::write(
        &collector_path,
        "#!/bin/sh\ncat >/dev/null\nwhile :; do printf '0123456789abcdef'; done\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&collector_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&collector_path, permissions).unwrap();

    let lake = LakeStore::local(temp.path()).unwrap();
    let provider = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(collector_path.as_os_str().to_owned())
        .with_timeout(std::time::Duration::from_secs(1))
        .with_max_stdout_bytes(7);
    let oversized = provider
        .load(&request(temp.path()), &lake)
        .await
        .unwrap_err();
    assert!(matches!(
        oversized,
        SourceInputProviderError::OutputTooLarge { .. }
    ));

    let malformed_path = temp.path().join("malformed-output.sh");
    fs::write(
        &malformed_path,
        "#!/bin/sh\ncat >/dev/null\nprintf 'not-json'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&malformed_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&malformed_path, permissions).unwrap();
    let malformed = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(malformed_path.as_os_str().to_owned())
        .with_max_stdout_bytes(128)
        .load(&request(temp.path()), &lake)
        .await
        .unwrap_err();
    assert!(matches!(malformed, SourceInputProviderError::Json(_)));
}

fn request(project_root: &std::path::Path) -> SourceInputRequest {
    SourceInputRequest {
        project_root: project_root.to_path_buf(),
        partition: AssetPartition::global(),
        planned_at: Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap(),
        requested_assets: Vec::new(),
        force_refresh_assets: Vec::new(),
        source_entities: Vec::new(),
    }
}

#[tokio::test]
async fn requested_assets_follow_the_dag_plan_and_skip_fresh_rera() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap();
    let input = ReraRegistryMonthlyInput {
        snapshot_date: "2026-07".to_string(),
        projects: vec![ReraProjectSnapshotRecord {
            ack_number: Some("ACK-1".to_string()),
            registration_number: Some("PRM-1".to_string()),
            project_name: "Fresh RERA Project".to_string(),
            promoter_name: Some("Proof Builder".to_string()),
            status: Some("Approved".to_string()),
            project_type: Some("Apartment".to_string()),
            project_address: Some("Whitefield, Bengaluru".to_string()),
            area_name: Some("Whitefield".to_string()),
            district: Some("Bengaluru Urban".to_string()),
            taluk: Some("Bengaluru East".to_string()),
            total_land_area_sqm: Some(40_468.56),
            land_litigation: Some(false),
            source_url: "https://rera.karnataka.gov.in/projectViewDetails".to_string(),
            fetched_at: now,
        }],
        detail_facts: Vec::new(),
        detail_fact_annotations: Vec::new(),
        source_watermarks: Vec::new(),
    };
    let record = ReraRegistryMaterializer::new(lake.clone())
        .materialize_for_run(&input, MaterializationId::new(), AssetPartition::global())
        .await
        .unwrap();
    AssetMaterializationStore::new(lake.clone())
        .promote_current(&record)
        .await
        .unwrap();

    let executor = AssetDagExecutor::new(default_openestates_registry(), lake);
    let partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
    let plan = executor.plan(&partition, now).await.unwrap();
    let requested = AssetSourceInputs::requested_asset_ids(&plan);

    assert!(!requested
        .iter()
        .any(|asset_id| asset_id.as_str() == "rera_registry_monthly"));
    assert!(requested
        .iter()
        .any(|asset_id| asset_id.as_str() == "reddit_threads_daily"));
}

#[tokio::test]
async fn stale_source_assets_are_marked_for_collector_cache_bypass() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let raw_id = AssetId::new("google_places_weekly").unwrap();
    let partition = AssetPartition::new([("source", "google")]);
    let store = AssetMaterializationStore::new(lake.clone());
    let old = MaterializationRecord::succeeded(
        raw_id.clone(),
        AssetStage::Raw,
        partition.clone(),
        "2026-07-01",
        Vec::new(),
    )
    .with_source_watermarks(vec![SourceWatermark {
        source: "fetch_google_review_links".to_string(),
        high_watermark: "2026-07-01T00:00:00Z".to_string(),
    }]);
    store.write_materialization(&old).await.unwrap();
    store.promote_current(&old).await.unwrap();
    let registry = AssetRegistry::new(vec![AssetDefinition::new(
        raw_id.clone(),
        AssetStage::Raw,
        "Google stale refresh fixture",
        Vec::new(),
        RefreshCadence::Weekly,
        CostTier::Cheap,
        TrustTier::Support,
    )
    .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
        &[],
        &[("source", "google")],
    ))])
    .unwrap();
    let plan = AssetDagExecutor::new(registry, lake)
        .plan(
            &AssetPartition::new([("dt", "2026-07-14")]),
            Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap(),
        )
        .await
        .unwrap();
    let collection = AssetSourceInputs::collection_plan(&plan);
    assert_eq!(collection.requested_assets, vec![raw_id.clone()]);
    assert_eq!(collection.force_refresh_assets, vec![raw_id]);
}

#[tokio::test]
async fn resume_collection_replays_only_the_raw_companion_needed_for_exact_lineage() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
    let plan = AssetDagExecutor::new(default_openestates_registry(), lake)
        .plan(
            &partition,
            Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap(),
        )
        .await
        .unwrap();
    let mut manifest = AssetDagRunManifest::from_plan(&plan);
    for step in &mut manifest.steps {
        step.status = AssetRunStepStatus::Succeeded;
    }
    manifest
        .steps
        .iter_mut()
        .find(|step| step.asset_id.as_str() == "reddit_resident_facts")
        .unwrap()
        .status = AssetRunStepStatus::Failed;

    let collection = AssetSourceInputs::resume_collection_plan(&manifest);

    assert_eq!(
        collection.requested_assets,
        vec![
            AssetId::new("reddit_resident_facts").unwrap(),
            AssetId::new("reddit_threads_daily").unwrap(),
        ]
    );
    assert_eq!(
        collection.force_assets,
        vec![AssetId::new("reddit_threads_daily").unwrap()]
    );
    assert!(collection.force_refresh_assets.is_empty());
    assert!(!collection
        .requested_assets
        .iter()
        .any(|asset_id| asset_id.as_str().starts_with("google_")));
}

#[tokio::test]
async fn resume_collection_replays_a_materialized_raw_companion_instead_of_mixing_crawls() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
    let plan = AssetDagExecutor::new(default_openestates_registry(), lake)
        .plan(
            &partition,
            Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap(),
        )
        .await
        .unwrap();
    let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "resume-v1");
    for step in &mut manifest.steps {
        step.status = AssetRunStepStatus::Succeeded;
    }
    let raw_id = AssetId::new("reddit_threads_daily").unwrap();
    let raw = manifest
        .steps
        .iter_mut()
        .find(|step| step.asset_id == raw_id)
        .unwrap();
    raw.status = AssetRunStepStatus::Materialized;
    raw.materialization_id = Some(MaterializationId::new());
    manifest
        .steps
        .iter_mut()
        .find(|step| step.asset_id.as_str() == "reddit_resident_facts")
        .unwrap()
        .status = AssetRunStepStatus::Failed;

    let collection = AssetSourceInputs::resume_collection_plan(&manifest);

    assert_eq!(
        collection.requested_assets,
        vec![
            AssetId::new("reddit_resident_facts").unwrap(),
            raw_id.clone(),
        ]
    );
    assert_eq!(collection.force_assets, vec![raw_id.clone()]);
    manifest.replay_step(&raw_id).unwrap();
    let replayed = manifest
        .steps
        .iter()
        .find(|step| step.asset_id == raw_id)
        .unwrap();
    assert_eq!(replayed.status, AssetRunStepStatus::Planned);
    assert!(replayed.materialization_id.is_none());

    let raw = manifest
        .steps
        .iter_mut()
        .find(|step| step.asset_id == raw_id)
        .unwrap();
    raw.status = AssetRunStepStatus::Running;
    let collection = AssetSourceInputs::resume_collection_plan(&manifest);
    assert!(collection.requested_assets.contains(&raw_id));
    assert!(collection.force_assets.contains(&raw_id));
}

#[cfg(unix)]
#[tokio::test]
async fn command_provider_output_executes_through_the_rera_asset() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();
    let lake = LakeStore::local(temp.path().join("lake")).unwrap();
    let collector_path = temp.path().join("rera-collector.sh");
    fs::write(
        &collector_path,
        r#"#!/bin/sh
cat >/dev/null
printf '%s' '{"rera_registry_monthly":{"snapshot_date":"2026-07","projects":[{"ack_number":"ACK-1","registration_number":"PRM-1","project_name":"Provider Proof Project","promoter_name":"Proof Builder","status":"Approved","project_type":"Apartment","project_address":"Whitefield, Bengaluru","area_name":"Whitefield","district":"Bengaluru Urban","taluk":"Bengaluru East","total_land_area_sqm":40468.56,"land_litigation":false,"source_url":"https://rera.karnataka.gov.in/projectViewDetails","fetched_at":"2026-07-14T09:30:00Z"}]}}'
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&collector_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&collector_path, permissions).unwrap();

    let now = Utc.with_ymd_and_hms(2026, 7, 14, 9, 30, 0).unwrap();
    let source_request = SourceInputRequest {
        project_root,
        partition: AssetPartition::global(),
        planned_at: now,
        requested_assets: vec![AssetId::new("rera_registry_monthly").unwrap()],
        force_refresh_assets: Vec::new(),
        source_entities: Vec::new(),
    };
    let source_inputs = CommandSourceInputProvider::new("/bin/sh")
        .with_arg(collector_path.as_os_str().to_owned())
        .load(&source_request, &lake)
        .await
        .unwrap()
        .unwrap();
    let registry = AssetRegistry::new(vec![AssetDefinition::new(
        AssetId::new("rera_registry_monthly").unwrap(),
        AssetStage::Raw,
        "RERA provider integration fixture",
        Vec::new(),
        RefreshCadence::Monthly,
        CostTier::Free,
        TrustTier::Root,
    )])
    .unwrap();
    let report = AssetDagExecutor::new(registry, lake.clone())
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(AssetPartition::global(), now)
                .with_version("2026-07-provider-proof")
                .with_source_inputs(source_inputs),
        )
        .await
        .unwrap();

    assert_eq!(report.executed_assets[0].as_str(), "rera_registry_monthly");
    assert!(lake
        .list_keys(&backend::lake::LakePrefix::new("raw/source=rera").unwrap())
        .await
        .unwrap()
        .iter()
        .any(|key| key.as_str().ends_with("projects/part-00000.parquet")));
}

#[tokio::test]
async fn facts_only_retry_rematerializes_the_exact_reddit_parent() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let now = Utc::now();
    let run_partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
    let old_raw = RedditThreadSnapshotMaterializer::new(lake.clone())
        .materialize_and_promote(
            "2026-07-14",
            "BangaloreRealEstates",
            "old-raw",
            &[reddit_record("old-thread", "Old evidence", now)],
            Vec::new(),
        )
        .await
        .unwrap();
    let raw_id = AssetId::new("reddit_threads_daily").unwrap();
    let facts_id = AssetId::new("reddit_resident_facts").unwrap();
    let registry = AssetRegistry::new(vec![
        AssetDefinition::new(
            raw_id.clone(),
            AssetStage::Raw,
            "Reddit raw retry fixture",
            Vec::new(),
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys(&["dt", "subreddit"])),
        AssetDefinition::new(
            facts_id.clone(),
            AssetStage::Silver,
            "Reddit facts retry fixture",
            vec![raw_id.clone()],
            RefreshCadence::OnChange,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &["dt"],
            &[("source", "reddit")],
        )),
    ])
    .unwrap();
    let executor = AssetDagExecutor::new(registry, lake.clone());
    let initial_plan = executor.plan(&run_partition, now).await.unwrap();
    let collection_plan = AssetSourceInputs::collection_plan(&initial_plan);

    assert_eq!(collection_plan.force_assets, vec![raw_id.clone()]);
    assert!(collection_plan
        .requested_assets
        .iter()
        .any(|asset_id| asset_id == &raw_id));
    assert!(collection_plan
        .requested_assets
        .iter()
        .any(|asset_id| asset_id == &facts_id));

    let source_inputs = AssetSourceInputs {
        reddit_threads_daily: Some(RedditThreadsDailyInput {
            snapshot_date: "2026-07-14".to_string(),
            subreddit: "BangaloreRealEstates".to_string(),
            records: vec![reddit_record("new-thread", "New retry evidence", now)],
            source_watermarks: Vec::new(),
        }),
        reddit_resident_facts: Some(SkillFactsInput {
            source: "reddit".to_string(),
            snapshot_date: "2026-07-14".to_string(),
            facts: vec![SkillFactRecord {
                entity_id: "society:retry-proof".to_string(),
                fact_key: "reddit_thread_count".to_string(),
                value_type: "numeric".to_string(),
                value_json: r#"{"type":"Numeric","data":1}"#.to_string(),
                confidence: 0.7,
                source_type: "Reddit".to_string(),
                source_url: Some("https://reddit.com/new-thread".to_string()),
                model: None,
                skill_id: Some("search_reddit".to_string()),
                triggered_by: None,
                learned_at: now,
                run_id: "retry-facts".to_string(),
                input_hash: "sha256:retry".to_string(),
            }],
            fact_annotations: vec![SkillFactAnnotationRecord {
                entity_id: "society:retry-proof".to_string(),
                fact_key: "reddit_thread_count".to_string(),
                display_template: Some("{value} Reddit discussions found".to_string()),
                answers_preferences_json: r#"["reddit"]"#.to_string(),
                scoring_direction: Some("HigherIsBetter".to_string()),
                scoring_weight: Some(1.0),
                scoring_thresholds_json: "[]".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        ..AssetSourceInputs::default()
    };
    executor
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(run_partition.clone(), now)
                .with_source_inputs(source_inputs)
                .with_forced_assets(collection_plan.force_assets),
        )
        .await
        .unwrap();

    let materializations = AssetMaterializationStore::new(lake);
    let new_raw = materializations
        .current_record(&raw_id, &run_partition)
        .await
        .unwrap();
    let facts = materializations
        .current_record(
            &facts_id,
            &AssetPartition::new([("dt", "2026-07-14"), ("source", "reddit")]),
        )
        .await
        .unwrap();
    assert_ne!(
        new_raw.materialization_id,
        old_raw.record.materialization_id
    );
    assert_eq!(
        facts.parent_materializations,
        vec![new_raw.materialization_id]
    );
}

#[tokio::test]
async fn facts_only_retry_rematerializes_the_exact_google_parent() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let now = Utc::now();
    let run_partition = AssetPartition::new([("dt", "2026-07-14")]);
    let materializations = AssetMaterializationStore::new(lake.clone());
    let rera_record = ReraRegistryMaterializer::new(lake.clone())
        .materialize_for_run(
            &ReraRegistryMonthlyInput {
                snapshot_date: "2026-07".to_string(),
                projects: vec![ReraProjectSnapshotRecord {
                    ack_number: Some("ACK-GOOGLE-RETRY".to_string()),
                    registration_number: Some("PRM-GOOGLE-RETRY".to_string()),
                    project_name: "Google Retry Proof".to_string(),
                    promoter_name: None,
                    status: Some("Approved".to_string()),
                    project_type: None,
                    project_address: None,
                    area_name: Some("Whitefield".to_string()),
                    district: None,
                    taluk: None,
                    total_land_area_sqm: None,
                    land_litigation: None,
                    source_url: "https://rera.karnataka.gov.in/retry".to_string(),
                    fetched_at: now,
                }],
                detail_facts: Vec::new(),
                detail_fact_annotations: Vec::new(),
                source_watermarks: Vec::new(),
            },
            MaterializationId::new(),
            AssetPartition::global(),
        )
        .await
        .unwrap();
    let canonical = CanonicalSocietyMaterializer::new(lake.clone())
        .materialize_from_rera_for_run(
            &rera_record,
            "google-retry-canonical",
            MaterializationId::new(),
            AssetPartition::global(),
        )
        .await
        .unwrap();
    materializations
        .promote_current(&rera_record)
        .await
        .unwrap();
    materializations.promote_current(&canonical).await.unwrap();
    let canonical_rows = backend::assets::read_canonical_society_rows(&lake, &canonical)
        .await
        .unwrap();
    let canonical_id = canonical_rows.mappings[0].canonical_entity_id.clone();
    let old_raw = GooglePlaceSnapshotMaterializer::new(lake.clone())
        .materialize_for_run(
            &GooglePlacesWeeklyInput {
                snapshot_date: "2026-07-14".to_string(),
                records: vec![google_record(&canonical_id, "old-place", now)],
                source_watermarks: Vec::new(),
            },
            "old-google-raw",
            vec![canonical.materialization_id.clone()],
            MaterializationId::new(),
            AssetPartition::new([("source", "google")]),
        )
        .await
        .unwrap();
    materializations
        .promote_current(&old_raw.record)
        .await
        .unwrap();
    let rera_id_asset = AssetId::new("rera_registry_monthly").unwrap();
    let canonical_id_asset = AssetId::new("canonical_society_nodes").unwrap();
    let raw_id = AssetId::new("google_places_weekly").unwrap();
    let facts_id = AssetId::new("google_review_facts").unwrap();
    let registry = AssetRegistry::new(vec![
        AssetDefinition::new(
            rera_id_asset.clone(),
            AssetStage::Raw,
            "RERA retry fixture",
            Vec::new(),
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Root,
        ),
        AssetDefinition::new(
            canonical_id_asset.clone(),
            AssetStage::Gold,
            "Canonical society retry fixture",
            vec![rera_id_asset],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Authoritative,
        ),
        AssetDefinition::new(
            raw_id.clone(),
            AssetStage::Raw,
            "Google raw retry fixture",
            vec![canonical_id_asset.clone()],
            RefreshCadence::Weekly,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
        AssetDefinition::new(
            facts_id.clone(),
            AssetStage::Silver,
            "Google facts retry fixture",
            vec![raw_id.clone(), canonical_id_asset],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
    ])
    .unwrap();
    let executor = AssetDagExecutor::new(registry, lake.clone());
    let plan = executor.plan(&run_partition, now).await.unwrap();
    let collection_plan = AssetSourceInputs::collection_plan(&plan);

    assert_eq!(collection_plan.force_assets, vec![raw_id.clone()]);
    executor
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(run_partition, now)
                .with_source_inputs(AssetSourceInputs {
                    google_places_weekly: Some(GooglePlacesWeeklyInput {
                        snapshot_date: "2026-07-14".to_string(),
                        records: vec![google_record(&canonical_id, "new-place", now)],
                        source_watermarks: Vec::new(),
                    }),
                    ..AssetSourceInputs::default()
                })
                .with_forced_assets(collection_plan.force_assets),
        )
        .await
        .unwrap();

    let materializations = AssetMaterializationStore::new(lake);
    let partition = AssetPartition::new([("source", "google")]);
    let new_raw = materializations
        .current_record(&raw_id, &partition)
        .await
        .unwrap();
    let facts = materializations
        .current_record(&facts_id, &partition)
        .await
        .unwrap();
    assert_ne!(
        new_raw.materialization_id,
        old_raw.record.materialization_id
    );
    assert_eq!(
        facts.parent_materializations,
        vec![new_raw.materialization_id, canonical.materialization_id]
    );
}

fn reddit_record(
    thread_id: &str,
    title: &str,
    fetched_at: chrono::DateTime<Utc>,
) -> RedditThreadSnapshotRecord {
    RedditThreadSnapshotRecord {
        thread_id: thread_id.to_string(),
        subreddit: "BangaloreRealEstates".to_string(),
        query: "retry proof".to_string(),
        title: title.to_string(),
        url: Some(format!("https://reddit.com/{thread_id}")),
        score: 1,
        num_comments: 1,
        created_utc: Some(fetched_at.timestamp()),
        selftext: Some(title.to_string()),
        fetched_at,
        fetch_source: "mock_reddit".to_string(),
    }
}

fn google_record(
    entity_id: &str,
    place_id: &str,
    fetched_at: chrono::DateTime<Utc>,
) -> GooglePlaceSnapshotRecord {
    GooglePlaceSnapshotRecord {
        entity_id: entity_id.to_string(),
        project_key: None,
        query: "retry proof whitefield".to_string(),
        place_name: Some("Retry Proof".to_string()),
        place_id: Some(place_id.to_string()),
        reviews_url: format!("https://www.google.com/maps/search/?query_place_id={place_id}"),
        rating: Some(4.3),
        review_count: Some(100),
        address: None,
        confidence: 0.8,
        fetched_at,
        fetch_source: "mock_google".to_string(),
    }
}
