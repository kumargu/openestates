use backend::assets::{
    default_openestates_registry, AssetDagExecutionOptions, AssetDagExecutor,
    AssetDagExecutorError, AssetId, AssetMaterializationStore, AssetPartition,
    AssetRunManifestStore, AssetRunStepStatus, AssetStage, DagRunStatus, MaterializationRecord,
    SourceWatermark, KG_SOCIETY_VIEW_ASSET_ID,
};
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::LakeStore;
use backend::serving::SEARCH_SERVING_BUNDLE_ASSET_ID;
use chrono::{Duration, TimeZone, Utc};
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
