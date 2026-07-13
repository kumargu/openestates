use std::fs::File;

use backend::assets::{
    default_openestates_registry, AssetDagExecutionOptions, AssetDagExecutor,
    AssetDagExecutorError, AssetId, AssetMaterializationStore, AssetPartition,
    AssetRunManifestStore, AssetRunStepStatus, AssetSourceInputs, AssetStage, DagRunStatus,
    MaterializationRecord, RedditThreadSnapshotRecord, RedditThreadsDailyInput,
    SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark,
    GOOGLE_REVIEW_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID, REDDIT_RESIDENT_FACTS_ASSET_ID,
    REDDIT_THREADS_DAILY_ASSET_ID,
};
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::SEARCH_SERVING_BUNDLE_ASSET_ID;
use chrono::{Duration, TimeZone, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn executor_runs_kg_and_serving_assets_with_dag_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

    let upstreams = seed_current_upstreams(&store, now).await;

    let options = AssetDagExecutionOptions::new(AssetPartition::global(), now)
        .with_version("2026-07-13T06:00Z");
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
    let current_run = run_store
        .current_manifest(&AssetPartition::global())
        .await
        .unwrap();
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

    let upstreams = seed_authoritative_upstreams(&store, now, &AssetPartition::global()).await;
    let options = AssetDagExecutionOptions::new(AssetPartition::global(), now)
        .with_version("2026-07-13T06:00Z")
        .with_source_inputs(mock_source_inputs(now));

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
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

    let reddit_threads = current_record(&store, REDDIT_THREADS_DAILY_ASSET_ID).await;
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

    let reddit_facts = current_record(&store, REDDIT_RESIDENT_FACTS_ASSET_ID).await;
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

    let google_facts = current_record(&store, GOOGLE_REVIEW_FACTS_ASSET_ID).await;
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

    let kg_record = current_record(&store, KG_SOCIETY_VIEW_ASSET_ID).await;
    assert!(kg_record
        .parent_materializations
        .contains(&reddit_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&google_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["rera_legal_facts"].materialization_id));
}

#[tokio::test]
async fn executor_requires_source_inputs_without_promoting_current_source_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&store, now, &AssetPartition::global()).await;

    let options = AssetDagExecutionOptions::new(AssetPartition::global(), now);
    let err = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap_err();

    let missing_asset_id = match err {
        AssetDagExecutorError::SourceInputMissing { asset_id } => asset_id,
        other => panic!("expected missing source input, got {other:?}"),
    };
    assert!(store
        .current_record(&missing_asset_id, &AssetPartition::global())
        .await
        .unwrap_err()
        .is_not_found());
}

#[tokio::test]
async fn executor_fails_loudly_when_planned_asset_has_no_executor() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let options = AssetDagExecutionOptions::new(AssetPartition::global(), now);

    let err = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AssetDagExecutorError::NoExecutor { asset_id: ref returned_asset_id }
            if returned_asset_id == &asset_id("rera_registry_monthly")
    ));

    let failed_run = AssetRunManifestStore::new(lake)
        .current_manifest(&AssetPartition::global())
        .await
        .unwrap();
    assert_eq!(failed_run.status, DagRunStatus::Failed);
    assert_eq!(failed_run.failed_count, 1);
    let failed_step = failed_run
        .steps
        .iter()
        .find(|step| step.asset_id == asset_id("rera_registry_monthly"))
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
    let options = AssetDagExecutionOptions::new(AssetPartition::global(), now).dry_run(true);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&KnowledgeGraph::new(), options)
        .await
        .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.manifest.status, DagRunStatus::Planned);
    assert_eq!(report.executed_assets.len(), 0);
    assert!(AssetRunManifestStore::new(lake)
        .current_manifest(&AssetPartition::global())
        .await
        .is_err());
}

#[tokio::test]
async fn executor_rejects_non_global_runtime_assets_until_artifact_paths_are_partitioned() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let partition = AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]);
    seed_current_upstreams_for_partition(&store, now, &partition).await;

    let options = AssetDagExecutionOptions::new(partition, now);
    let err = AssetDagExecutor::new(default_openestates_registry(), lake)
        .execute(&mock_graph(), options)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AssetDagExecutorError::UnsupportedPartition { asset_id: ref returned_asset_id, .. }
            if returned_asset_id == &asset_id(KG_SOCIETY_VIEW_ASSET_ID)
    ));
}

async fn seed_current_upstreams(
    store: &AssetMaterializationStore,
    now: chrono::DateTime<Utc>,
) -> std::collections::HashMap<&'static str, MaterializationRecord> {
    seed_current_upstreams_for_partition(store, now, &AssetPartition::global()).await
}

async fn seed_authoritative_upstreams(
    store: &AssetMaterializationStore,
    now: chrono::DateTime<Utc>,
    partition: &AssetPartition,
) -> std::collections::HashMap<&'static str, MaterializationRecord> {
    let rera = materialization(
        "rera_registry_monthly",
        AssetStage::Raw,
        "2026-07",
        now,
        partition,
    )
    .with_source_watermarks(vec![SourceWatermark {
        source: "rera".to_string(),
        high_watermark: "2026-07".to_string(),
    }]);
    write_current(store, &rera).await;

    let canonical = materialization(
        "canonical_society_nodes",
        AssetStage::Gold,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![rera.materialization_id.clone()]);
    write_current(store, &canonical).await;

    let rera_facts = materialization(
        "rera_legal_facts",
        AssetStage::Silver,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![
        rera.materialization_id.clone(),
        canonical.materialization_id.clone(),
    ]);
    write_current(store, &rera_facts).await;

    std::collections::HashMap::from([
        ("rera_registry_monthly", rera),
        ("canonical_society_nodes", canonical),
        ("rera_legal_facts", rera_facts),
    ])
}

async fn seed_current_upstreams_for_partition(
    store: &AssetMaterializationStore,
    now: chrono::DateTime<Utc>,
    partition: &AssetPartition,
) -> std::collections::HashMap<&'static str, MaterializationRecord> {
    let rera = materialization(
        "rera_registry_monthly",
        AssetStage::Raw,
        "2026-07",
        now,
        partition,
    )
    .with_source_watermarks(vec![SourceWatermark {
        source: "rera".to_string(),
        high_watermark: "2026-07".to_string(),
    }]);
    write_current(store, &rera).await;

    let canonical = materialization(
        "canonical_society_nodes",
        AssetStage::Gold,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![rera.materialization_id.clone()]);
    write_current(store, &canonical).await;

    let rera_facts = materialization(
        "rera_legal_facts",
        AssetStage::Silver,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![
        rera.materialization_id.clone(),
        canonical.materialization_id.clone(),
    ]);
    write_current(store, &rera_facts).await;

    let reddit_threads = materialization(
        "reddit_threads_daily",
        AssetStage::Raw,
        "2026-07-13",
        now - Duration::hours(1),
        partition,
    )
    .with_parent_materializations(vec![canonical.materialization_id.clone()])
    .with_source_watermarks(vec![SourceWatermark {
        source: "reddit:BangaloreRealEstates".to_string(),
        high_watermark: "2026-07-13T05:00:00Z".to_string(),
    }]);
    write_current(store, &reddit_threads).await;

    let reddit_facts = materialization(
        "reddit_resident_facts",
        AssetStage::Silver,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![
        reddit_threads.materialization_id.clone(),
        canonical.materialization_id.clone(),
    ]);
    write_current(store, &reddit_facts).await;

    let google_facts = materialization(
        "google_review_facts",
        AssetStage::Silver,
        "2026-07-13",
        now,
        partition,
    )
    .with_parent_materializations(vec![canonical.materialization_id.clone()]);
    write_current(store, &google_facts).await;

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
) -> MaterializationRecord {
    store
        .current_record(&asset_id(asset_id_value), &AssetPartition::global())
        .await
        .unwrap()
}

async fn write_current(store: &AssetMaterializationStore, record: &MaterializationRecord) {
    store.write_materialization(record).await.unwrap();
    store.promote_current(record).await.unwrap();
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

fn executed_position(executed_assets: &[AssetId], id: &str) -> usize {
    executed_assets
        .iter()
        .position(|executed_asset_id| executed_asset_id == &asset_id(id))
        .unwrap_or_else(|| panic!("missing executed asset {id}"))
}

fn mock_source_inputs(now: chrono::DateTime<Utc>) -> AssetSourceInputs {
    AssetSourceInputs {
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
