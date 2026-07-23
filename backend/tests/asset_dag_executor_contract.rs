use std::fs::File;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, StringArray};
use axum::extract::{Query, State};
use backend::assets::{
    default_openestates_registry, rera_legal_facts_input, AssetDagExecutionOptions,
    AssetDagExecutor, AssetDagExecutorError, AssetDefinition, AssetId, AssetMaterializationStore,
    AssetPartition, AssetRegistry, AssetRunAttempt, AssetRunManifestStore, AssetRunStepStatus,
    AssetSourceInputs, AssetStage, CanonicalSocietyMaterializer, CostTier, DagRunStatus,
    ExternalImageObservationRecord, ExternalImagesWeeklyInput, ExternalListingObservationRecord,
    ExternalListingsWeeklyInput, GoogleNearbyPlaceRecord, GoogleNearbyPlacesWeeklyInput,
    GooglePlaceSnapshotRecord, GooglePlacesWeeklyInput, MaterializationId, MaterializationRecord,
    MetroStationObservationRecord, MetroStationsMonthlyInput, PrestigeInventoryObservationRecord,
    PrestigeInventoryWeeklyInput, RedditThreadSnapshotRecord, RedditThreadsDailyInput,
    RefreshCadence, ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactMaterializer, SkillFactRecord, SkillFactsInput,
    SourceWatermark, TrustTier, APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
    BUILDER_RERA_AGGREGATES_ASSET_ID, CANONICAL_ROAD_NODES_ASSET_ID,
    CANONICAL_SOCIETY_NODES_ASSET_ID, COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID,
    EXTERNAL_IMAGES_WEEKLY_ASSET_ID, EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
    EXTERNAL_LISTING_FACTS_ASSET_ID, GENERATED_CONTEXT_SUMMARIES_ASSET_ID,
    GOOGLE_PLACES_WEEKLY_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID, HOME_STATE_SIGNALS_ASSET_ID,
    IMAGE_MEDIA_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID, MARKET_PROJECT_FACTS_ASSET_ID,
    METRO_PROXIMITY_FACTS_ASSET_ID, METRO_STATIONS_MONTHLY_ASSET_ID,
    PRESTIGE_INVENTORY_WEEKLY_ASSET_ID, REDDIT_RESIDENT_FACTS_ASSET_ID,
    REDDIT_THREADS_DAILY_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};
use backend::entity_context::compose_entity_context;
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::models::{Property, Society};
use backend::routes::search::{search_properties, SearchQuery};
use backend::search::{HashSemanticEmbedder, SearchIndex, SemanticEmbedder, SemanticSearchIndex};
use backend::serving::{
    ServingBundleLoader, ServingBundleManifest, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
use backend::state::AppState;
use bytes::Bytes;
use chrono::{Duration, TimeZone, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;
use tokio::sync::RwLock;

#[tokio::test]
async fn executor_runs_kg_and_serving_assets_with_dag_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

    let run_partition = source_run_partition();
    let upstreams = seed_current_upstreams_for_partition(&lake, &store, now, &run_partition).await;

    let options = AssetDagExecutionOptions::new(run_partition.clone(), now)
        .with_version("2026-07-13T06:00Z")
        .with_source_inputs(mock_source_inputs(now));
    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.planned_count, 18);
    assert_eq!(report.manifest.succeeded_count, 18);
    assert_eq!(report.manifest.failed_count, 0);
    assert_eq!(report.executed_assets.len(), 18);
    for id in [
        backend::assets::GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
        backend::assets::GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID,
        PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
        MARKET_PROJECT_FACTS_ASSET_ID,
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
        IMAGE_MEDIA_FACTS_ASSET_ID,
        METRO_STATIONS_MONTHLY_ASSET_ID,
        METRO_PROXIMITY_FACTS_ASSET_ID,
        BUILDER_RERA_AGGREGATES_ASSET_ID,
        HOME_STATE_SIGNALS_ASSET_ID,
        COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID,
        CANONICAL_ROAD_NODES_ASSET_ID,
        APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
        KG_SOCIETY_VIEW_ASSET_ID,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
    ] {
        assert!(report.executed_assets.contains(&asset_id(id)));
    }

    let kg_record = store
        .current_record(
            &asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    assert_eq!(kg_record.run_id, report.manifest.run_id);
    assert_eq!(kg_record.parent_materializations.len(), 15);
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["canonical_society_nodes"].materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["rera_legal_facts"].materialization_id));
    let home_state_record = store
        .current_record(
            &asset_id(HOME_STATE_SIGNALS_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    assert_eq!(
        home_state_record.parent_materializations,
        vec![upstreams["rera_legal_facts"].materialization_id.clone()]
    );
    assert!(kg_record
        .parent_materializations
        .contains(&home_state_record.materialization_id));
    let nearby_facts_record = store
        .current_record(
            &asset_id(backend::assets::GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID),
            &google_fact_partition(),
        )
        .await
        .unwrap();
    assert!(kg_record
        .parent_materializations
        .contains(&nearby_facts_record.materialization_id));
    let listing_facts_record = store
        .current_record(
            &asset_id(EXTERNAL_LISTING_FACTS_ASSET_ID),
            &AssetPartition::new([("source", "external_listing")]),
        )
        .await
        .unwrap();
    assert!(kg_record
        .parent_materializations
        .contains(&listing_facts_record.materialization_id));
    let image_facts_record = store
        .current_record(
            &asset_id(IMAGE_MEDIA_FACTS_ASSET_ID),
            &AssetPartition::new([("source", "external_image")]),
        )
        .await
        .unwrap();
    assert!(kg_record
        .parent_materializations
        .contains(&image_facts_record.materialization_id));
    let community_facts_record = store
        .current_record(
            &asset_id(COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID),
            &AssetPartition::new([("source", "community")]),
        )
        .await
        .unwrap();
    assert!(kg_record
        .parent_materializations
        .contains(&community_facts_record.materialization_id));

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
        7
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
    assert_eq!(report.manifest.planned_count, 22);
    assert_eq!(report.executed_assets.len(), 22);
    for id in [
        PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
        MARKET_PROJECT_FACTS_ASSET_ID,
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
        IMAGE_MEDIA_FACTS_ASSET_ID,
        METRO_STATIONS_MONTHLY_ASSET_ID,
        METRO_PROXIMITY_FACTS_ASSET_ID,
        BUILDER_RERA_AGGREGATES_ASSET_ID,
        HOME_STATE_SIGNALS_ASSET_ID,
        REDDIT_THREADS_DAILY_ASSET_ID,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        GOOGLE_PLACES_WEEKLY_ASSET_ID,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID,
        backend::assets::GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
        backend::assets::GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID,
        CANONICAL_ROAD_NODES_ASSET_ID,
        APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
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
            < executed_position(
                &report.executed_assets,
                COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID
            )
    );
    assert!(
        executed_position(
            &report.executed_assets,
            COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID
        ) < executed_position(&report.executed_assets, KG_SOCIETY_VIEW_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, EXTERNAL_IMAGES_WEEKLY_ASSET_ID)
            < executed_position(&report.executed_assets, IMAGE_MEDIA_FACTS_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, IMAGE_MEDIA_FACTS_ASSET_ID)
            < executed_position(&report.executed_assets, KG_SOCIETY_VIEW_ASSET_ID)
    );
    assert!(
        executed_position(&report.executed_assets, HOME_STATE_SIGNALS_ASSET_ID)
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

    let google_places = current_record(
        &store,
        GOOGLE_PLACES_WEEKLY_ASSET_ID,
        &google_fact_partition(),
    )
    .await;
    assert_eq!(
        parquet_rows_for_artifact(&lake, &google_places, "places/part-00000.parquet").await,
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
        vec![
            google_places.materialization_id.clone(),
            upstreams["canonical_society_nodes"]
                .materialization_id
                .clone()
        ]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &google_facts, "facts/part-00000.parquet").await,
        8,
        "Google evidence is published for both the canonical RERA entity and its stable society alias"
    );
    let image_observations = current_record(
        &store,
        EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
        &AssetPartition::new([("source", "external_image")]),
    )
    .await;
    assert_eq!(
        image_observations.partition,
        AssetPartition::new([("source", "external_image")])
    );
    assert_eq!(image_observations.run_id, report.manifest.run_id);
    assert_eq!(
        image_observations.parent_materializations,
        vec![upstreams["canonical_society_nodes"]
            .materialization_id
            .clone()]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &image_observations, "images/part-00000.parquet").await,
        1
    );
    let image_facts = current_record(
        &store,
        IMAGE_MEDIA_FACTS_ASSET_ID,
        &AssetPartition::new([("source", "external_image")]),
    )
    .await;
    assert_eq!(
        image_facts.parent_materializations,
        vec![
            image_observations.materialization_id.clone(),
            upstreams["canonical_society_nodes"]
                .materialization_id
                .clone()
        ]
    );
    assert_eq!(
        parquet_rows_for_artifact(&lake, &image_facts, "facts/part-00000.parquet").await,
        4
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
    let community_facts = current_record(
        &store,
        COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID,
        &AssetPartition::new([("source", "community")]),
    )
    .await;
    assert!(kg_record
        .parent_materializations
        .contains(&community_facts.materialization_id));
    let nearby_facts = current_record(
        &store,
        backend::assets::GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID,
        &google_fact_partition(),
    )
    .await;
    assert!(kg_record
        .parent_materializations
        .contains(&nearby_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&image_facts.materialization_id));
    assert!(!kg_record
        .parent_materializations
        .contains(&older_google_facts.materialization_id));
    assert!(kg_record
        .parent_materializations
        .contains(&upstreams["rera_legal_facts"].materialization_id));
    let home_state_record = current_record(
        &store,
        HOME_STATE_SIGNALS_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert_eq!(
        parquet_rows_for_artifact(&lake, &home_state_record, "facts/part-00000.parquet").await,
        4
    );
    assert!(kg_record
        .parent_materializations
        .contains(&home_state_record.materialization_id));
    assert_eq!(kg_record.parent_materializations.len(), 16);
    assert_eq!(
        parquet_rows_for_artifact(&lake, &kg_record, "facts/part-00000.parquet").await,
        122
    );

    let serving_record = current_record(
        &store,
        SEARCH_SERVING_BUNDLE_ASSET_ID,
        &AssetPartition::global(),
    )
    .await;
    assert_eq!(serving_fact_rows(&lake, &serving_record).await, 123);

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
async fn executor_builds_rera_proof_chain_and_serves_search_endpoint() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    let mut source_inputs = mock_source_inputs(now);
    let reddit_facts = source_inputs.reddit_resident_facts.as_mut().unwrap();
    reddit_facts.facts[0].entity_id = "society:rera-meadows".to_string();
    reddit_facts.fact_annotations[0].entity_id = "society:rera-meadows".to_string();
    source_inputs.generated_context_summaries = Some(generated_context_summary_input(
        "society:rera-meadows",
        "If you are looking at RERA Meadows, the generated context should survive the DAG and serving bundle.",
        now,
    ));
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now)
        .with_version("2026-07-13T06:00Z")
        .with_source_inputs(source_inputs);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.planned_count, 25);
    assert_eq!(report.executed_assets.len(), 25);
    for id in [
        PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
        MARKET_PROJECT_FACTS_ASSET_ID,
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
        IMAGE_MEDIA_FACTS_ASSET_ID,
        METRO_STATIONS_MONTHLY_ASSET_ID,
        METRO_PROXIMITY_FACTS_ASSET_ID,
        BUILDER_RERA_AGGREGATES_ASSET_ID,
        RERA_REGISTRY_MONTHLY_ASSET_ID,
        CANONICAL_SOCIETY_NODES_ASSET_ID,
        RERA_LEGAL_FACTS_ASSET_ID,
        HOME_STATE_SIGNALS_ASSET_ID,
        REDDIT_THREADS_DAILY_ASSET_ID,
        REDDIT_RESIDENT_FACTS_ASSET_ID,
        GENERATED_CONTEXT_SUMMARIES_ASSET_ID,
        GOOGLE_PLACES_WEEKLY_ASSET_ID,
        GOOGLE_REVIEW_FACTS_ASSET_ID,
        COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID,
        backend::assets::GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
        backend::assets::GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID,
        CANONICAL_ROAD_NODES_ASSET_ID,
        APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
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
    let context = compose_entity_context("society:rera-meadows", &loaded)
        .expect("generated context summary should be served from loaded bundle");
    assert_eq!(
        context.summary_paragraph,
        "If you are looking at RERA Meadows, the generated context should survive the DAG and serving bundle."
    );
    assert_eq!(context.anchor_entity_id, "society:rera-meadows");
    assert!(context.clauses.is_empty());
    assert!(context.category_groups.is_empty());

    let query = "3bhk with greenery in whitefield above 10 acres";
    let properties = vec![
        search_property("rera-meadows-listing", "rera-meadows", "RERA Meadows"),
        search_property(
            "unproven-listing",
            "unproven-whitefield",
            "Unproven Whitefield",
        ),
    ];
    let societies = vec![
        search_society("rera-meadows", "RERA Meadows"),
        search_society("unproven-whitefield", "Unproven Whitefield"),
    ];
    let search_index = SearchIndex::build(&properties);
    let semantic_embedder: Arc<dyn SemanticEmbedder> = Arc::new(HashSemanticEmbedder::default());
    let semantic_index =
        SemanticSearchIndex::from_serving_entities(&loaded.entities, semantic_embedder.as_ref());
    let state = Arc::new(AppState {
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
        semantic_index: RwLock::new(semantic_index),
        semantic_embedder,
        serving_bundle: RwLock::new(Some(Arc::new(loaded))),
        areas: Vec::new(),
        societies,
        sellers: RwLock::new(Vec::new()),
        discovery_config: backend::discovery::load_discovery_config(),
        knowledge: Arc::new(RwLock::new(mock_graph())),
        project_root: root.path().to_path_buf(),
        interest_counter: AtomicU64::new(0),
        interest_rate_limiter: RwLock::new((Instant::now(), 0)),
        registration_counter: AtomicU64::new(0),
        registration_rate_limiter: RwLock::new((Instant::now(), 0)),
        publish_rate_limiter: RwLock::new((Instant::now(), 0)),
    });
    let response = search_properties(
        State(state),
        Query(SearchQuery {
            q: Some(query.to_string()),
            debug: None,
        }),
    )
    .await
    .0;
    assert_eq!(response.query, query);
    assert_eq!(response.total_results, 1);
    assert_eq!(response.intent.bhk, Some(3));
    assert_eq!(response.intent.area.as_deref(), Some("Whitefield"));
    let knowledge_context = response
        .knowledge_context
        .as_ref()
        .expect("search endpoint should return knowledge context");
    assert!(knowledge_context.learning_gaps.is_empty());
    assert_eq!(knowledge_context.claims.len(), 2);
    assert!(knowledge_context
        .claims
        .iter()
        .any(|claim| claim.source_type == "Rera" && claim.claim.contains("RERA land area")));
    assert!(knowledge_context
        .claims
        .iter()
        .any(|claim| claim.source_type == "Reddit" && claim.claim.contains("trees")));
    let results = response.results;

    assert_eq!(
        results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec!["rera-meadows-listing"],
        "an unproven project must not survive the RERA acreage constraint"
    );
    let result = &results[0];
    let explanation = result
        .match_explanation
        .as_ref()
        .expect("proof-first search should explain its evidence");
    assert!(explanation.reasons.iter().any(|reason| {
        reason.preference == "above 10 acres"
            && reason.fact_key == "rera_total_land_area_sqm"
            && reason.scoring_method == "rera-proof"
            && reason.source_type == "Rera"
    }));
    assert!(explanation.reasons.iter().any(|reason| {
        reason.preference == "greenery"
            && reason.fact_key == "resident_greenery_signal"
            && reason.scoring_method == "serving-fact"
            && reason.source_type == "Reddit"
    }));
    assert_eq!(
        result.card.google_reviews_url.as_deref(),
        Some("https://maps.google.com/?cid=green-acre")
    );
    assert_eq!(result.card.google_rating, Some(4.4));
    assert_eq!(result.card.google_review_count, Some(321));
}

#[tokio::test]
async fn executor_requires_source_inputs_without_promoting_current_source_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;

    let run_partition = source_run_partition();
    let options = AssetDagExecutionOptions::new(run_partition.clone(), now)
        .with_source_inputs(generated_only_source_inputs(now));
    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(report.manifest.status, DagRunStatus::SucceededWithWarnings);
    assert!(report.manifest.failed_count > 0);
    assert_eq!(
        run_step(&report.manifest, SEARCH_SERVING_BUNDLE_ASSET_ID).status,
        AssetRunStepStatus::Succeeded
    );
    for (id, partition) in [
        (REDDIT_THREADS_DAILY_ASSET_ID, reddit_thread_partition()),
        (REDDIT_RESIDENT_FACTS_ASSET_ID, reddit_fact_partition()),
        (GOOGLE_PLACES_WEEKLY_ASSET_ID, google_fact_partition()),
    ] {
        assert!(store
            .current_record(&asset_id(id), &partition)
            .await
            .unwrap_err()
            .is_not_found());
    }
}

#[tokio::test]
async fn executor_reports_optional_failure_as_warning_and_can_resume_same_run() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;
    let run_partition = source_run_partition();
    let mut partial_inputs = mock_source_inputs(now);
    partial_inputs.reddit_threads_daily = None;
    partial_inputs.source_failures.insert(
        REDDIT_THREADS_DAILY_ASSET_ID.to_string(),
        "RedditSourceBlocked: HTTP 403".to_string(),
    );
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());

    let report = executor
        .execute(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition.clone(), now)
                .with_version("resilient-run")
                .with_source_inputs(partial_inputs),
        )
        .await
        .unwrap();
    assert_eq!(report.manifest.status, DagRunStatus::SucceededWithWarnings);

    let run_store = AssetRunManifestStore::new(lake.clone());
    let failed = run_store.current_manifest(&run_partition).await.unwrap();
    assert_eq!(failed.status, DagRunStatus::SucceededWithWarnings);
    assert_eq!(failed.failed_count, 1);
    assert_eq!(failed.blocked_count, 1);
    assert_eq!(
        run_step(&failed, REDDIT_THREADS_DAILY_ASSET_ID).status,
        AssetRunStepStatus::Failed
    );
    assert!(run_step(&failed, REDDIT_THREADS_DAILY_ASSET_ID)
        .error
        .as_deref()
        .is_some_and(|error| error.contains("HTTP 403")));
    assert_eq!(
        run_step(&failed, REDDIT_THREADS_DAILY_ASSET_ID)
            .attempts
            .len(),
        1
    );
    assert_eq!(
        run_step(&failed, REDDIT_RESIDENT_FACTS_ASSET_ID).status,
        AssetRunStepStatus::Blocked
    );
    assert_eq!(
        run_step(&failed, GOOGLE_PLACES_WEEKLY_ASSET_ID).status,
        AssetRunStepStatus::Succeeded
    );
    assert_eq!(
        run_step(&failed, GOOGLE_REVIEW_FACTS_ASSET_ID).status,
        AssetRunStepStatus::Succeeded
    );
    assert_eq!(
        run_step(&failed, KG_SOCIETY_VIEW_ASSET_ID).status,
        AssetRunStepStatus::Succeeded
    );
    assert_eq!(
        run_step(&failed, SEARCH_SERVING_BUNDLE_ASSET_ID).status,
        AssetRunStepStatus::Succeeded
    );
    let google_materialization = run_step(&failed, GOOGLE_PLACES_WEEKLY_ASSET_ID)
        .materialization_id
        .clone()
        .unwrap();
    let original_canonical_id = run_step(&failed, REDDIT_THREADS_DAILY_ASSET_ID)
        .dependency_snapshot
        .iter()
        .find_map(|materialization_id| {
            (materialization_id
                == &run_step(&failed, CANONICAL_SOCIETY_NODES_ASSET_ID)
                    .current_materialization_id
                    .clone()
                    .unwrap())
                .then_some(materialization_id.clone())
        })
        .unwrap();
    let mut advanced_canonical = store
        .record(
            &asset_id(CANONICAL_SOCIETY_NODES_ASSET_ID),
            &AssetPartition::global(),
            &original_canonical_id,
        )
        .await
        .unwrap();
    advanced_canonical.materialization_id = MaterializationId::new();
    advanced_canonical.run_id = MaterializationId::new();
    advanced_canonical.created_at = now + Duration::hours(1);
    store
        .write_materialization(&advanced_canonical)
        .await
        .unwrap();
    store
        .promote_current_for_run(&advanced_canonical, now + Duration::hours(1))
        .await
        .unwrap();

    let resumed = executor
        .resume(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition.clone(), now + Duration::days(1))
                .with_version("must-not-replace-original-version")
                .with_source_inputs(mock_source_inputs(now)),
            failed.run_id.clone(),
        )
        .await
        .unwrap();

    assert_eq!(resumed.manifest.run_id, failed.run_id);
    assert_eq!(resumed.manifest.status, DagRunStatus::Succeeded);
    assert!(resumed
        .executed_assets
        .contains(&asset_id(REDDIT_THREADS_DAILY_ASSET_ID)));
    assert!(resumed
        .executed_assets
        .contains(&asset_id(REDDIT_RESIDENT_FACTS_ASSET_ID)));
    assert!(!resumed
        .executed_assets
        .contains(&asset_id(KG_SOCIETY_VIEW_ASSET_ID)));
    assert!(!resumed
        .executed_assets
        .contains(&asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID)));
    assert!(!resumed
        .executed_assets
        .contains(&asset_id(GOOGLE_PLACES_WEEKLY_ASSET_ID)));
    assert_eq!(
        run_step(&resumed.manifest, GOOGLE_PLACES_WEEKLY_ASSET_ID).materialization_id,
        Some(google_materialization)
    );
    assert_eq!(
        run_step(&resumed.manifest, REDDIT_THREADS_DAILY_ASSET_ID)
            .attempts
            .len(),
        2
    );
    let reddit_record = store
        .record(
            &asset_id(REDDIT_THREADS_DAILY_ASSET_ID),
            &reddit_thread_partition(),
            run_step(&resumed.manifest, REDDIT_THREADS_DAILY_ASSET_ID)
                .materialization_id
                .as_ref()
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        reddit_record.parent_materializations,
        vec![original_canonical_id]
    );
    assert_eq!(
        store
            .record(
                &asset_id(KG_SOCIETY_VIEW_ASSET_ID),
                &AssetPartition::global(),
                run_step(&resumed.manifest, KG_SOCIETY_VIEW_ASSET_ID)
                    .materialization_id
                    .as_ref()
                    .unwrap(),
            )
            .await
            .unwrap()
            .version,
        "resilient-run"
    );
}

#[tokio::test]
async fn executor_rejects_tampered_artifacts_before_resuming_successful_steps() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;
    let run_partition = source_run_partition();
    let mut partial_inputs = mock_source_inputs(now);
    partial_inputs.reddit_threads_daily = None;
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    executor
        .execute(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition.clone(), now)
                .with_version("tampered-resume")
                .with_source_inputs(partial_inputs),
        )
        .await
        .unwrap();
    let failed = AssetRunManifestStore::new(lake.clone())
        .current_manifest(&run_partition)
        .await
        .unwrap();
    let google_step = run_step(&failed, GOOGLE_PLACES_WEEKLY_ASSET_ID);
    let google_record = store
        .record(
            &google_step.asset_id,
            &google_step.partition,
            google_step.materialization_id.as_ref().unwrap(),
        )
        .await
        .unwrap();
    let artifact = google_record.artifacts.first().unwrap();
    lake.put_text(&LakeKey::new(artifact.key.clone()).unwrap(), "tampered")
        .await
        .unwrap();

    let err = executor
        .resume(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition, now + Duration::minutes(5))
                .with_source_inputs(mock_source_inputs(now)),
            failed.run_id,
        )
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AssetDagExecutorError::ResumeArtifactIntegrity {
            asset_id: restored_asset_id,
            ..
        } if restored_asset_id == asset_id(GOOGLE_PLACES_WEEKLY_ASSET_ID)
    ));
}

#[tokio::test]
async fn facts_only_resume_replays_raw_companion_and_records_exact_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    seed_authoritative_upstreams(&lake, &store, now, &AssetPartition::global()).await;
    let run_partition = source_run_partition();
    let mut partial_inputs = mock_source_inputs(now);
    partial_inputs.reddit_resident_facts = None;
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    executor
        .execute(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition.clone(), now)
                .with_version("facts-only-resume")
                .with_source_inputs(partial_inputs),
        )
        .await
        .unwrap();
    let failed = AssetRunManifestStore::new(lake.clone())
        .current_manifest(&run_partition)
        .await
        .unwrap();
    let old_raw_id = run_step(&failed, REDDIT_THREADS_DAILY_ASSET_ID)
        .materialization_id
        .clone()
        .unwrap();
    let collection = AssetSourceInputs::resume_collection_plan(&failed);

    let resumed = executor
        .resume(
            &mock_graph(),
            AssetDagExecutionOptions::new(run_partition, now + Duration::minutes(5))
                .with_source_inputs(mock_source_inputs(now))
                .with_forced_assets(collection.force_assets),
            failed.run_id,
        )
        .await
        .unwrap();

    let raw_step = run_step(&resumed.manifest, REDDIT_THREADS_DAILY_ASSET_ID);
    let new_raw_id = raw_step.materialization_id.clone().unwrap();
    assert_ne!(new_raw_id, old_raw_id);
    assert_eq!(raw_step.attempts.len(), 2);
    assert_eq!(
        run_step(&resumed.manifest, REDDIT_RESIDENT_FACTS_ASSET_ID).parent_materializations[0],
        new_raw_id
    );
    assert_eq!(
        run_step(&resumed.manifest, GOOGLE_PLACES_WEEKLY_ASSET_ID)
            .attempts
            .len(),
        1
    );
}

#[tokio::test]
async fn executor_resumes_interrupted_serving_step_from_exact_succeeded_kg_view() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();
    let run_partition = source_run_partition();
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    let graph = mock_graph();
    let completed = executor
        .execute(
            &graph,
            AssetDagExecutionOptions::new(run_partition.clone(), now)
                .with_version("interrupted-serving")
                .with_source_inputs(mock_source_inputs(now)),
        )
        .await
        .unwrap();
    let kg_materialization = run_step(&completed.manifest, KG_SOCIETY_VIEW_ASSET_ID)
        .materialization_id
        .clone()
        .unwrap();
    let serving_materialization = run_step(&completed.manifest, SEARCH_SERVING_BUNDLE_ASSET_ID)
        .materialization_id
        .clone();
    let mut interrupted = completed.manifest;
    interrupted.status = DagRunStatus::Running;
    interrupted.completed_at = None;
    let interrupted_at = now + Duration::minutes(10);
    let serving_step = interrupted
        .steps
        .iter_mut()
        .find(|step| step.asset_id == asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID))
        .unwrap();
    serving_step.status = AssetRunStepStatus::Running;
    serving_step.materialization_id = None;
    serving_step.row_count = None;
    serving_step.artifacts.clear();
    serving_step.completed_at = None;
    serving_step.duration_ms = None;
    serving_step.error = None;
    serving_step.attempts.push(AssetRunAttempt {
        attempt: serving_step.attempts.len() as u32 + 1,
        started_at: interrupted_at,
        completed_at: None,
        error: None,
    });
    let run_store = AssetRunManifestStore::new(lake);
    run_store.write_manifest(&interrupted).await.unwrap();
    run_store.promote_current(&interrupted).await.unwrap();

    let resumed = executor
        .resume(
            &graph,
            AssetDagExecutionOptions::new(run_partition, interrupted_at + Duration::minutes(1))
                .with_version("interrupted-serving"),
            interrupted.run_id.clone(),
        )
        .await
        .unwrap();

    assert_eq!(resumed.manifest.status, DagRunStatus::Succeeded);
    assert!(resumed.executed_assets.is_empty());
    assert_eq!(
        run_step(&resumed.manifest, KG_SOCIETY_VIEW_ASSET_ID).materialization_id,
        Some(kg_materialization)
    );
    let serving = run_step(&resumed.manifest, SEARCH_SERVING_BUNDLE_ASSET_ID);
    assert_eq!(serving.status, AssetRunStepStatus::Succeeded);
    assert_eq!(serving.materialization_id, serving_materialization);
    assert_eq!(
        serving
            .attempts
            .last()
            .and_then(|attempt| attempt.error.as_deref()),
        None
    );
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
    assert_eq!(failed_step.attempts.len(), 1);
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

    let options = AssetDagExecutionOptions::new(run_partition, now)
        .with_source_inputs(generated_only_source_inputs(now));
    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(&mock_graph(), options)
        .await
        .unwrap();

    assert_eq!(
        report.executed_assets,
        vec![
            asset_id(GENERATED_CONTEXT_SUMMARIES_ASSET_ID),
            asset_id(CANONICAL_ROAD_NODES_ASSET_ID),
            asset_id(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID),
            asset_id(BUILDER_RERA_AGGREGATES_ASSET_ID),
            asset_id(COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID),
            asset_id(HOME_STATE_SIGNALS_ASSET_ID),
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

    let google_places = materialization(
        GOOGLE_PLACES_WEEKLY_ASSET_ID,
        AssetStage::Raw,
        "2026-07-13",
        now,
        &google_fact_partition_for(run_partition),
    )
    .with_parent_materializations(vec![canonical.materialization_id.clone()])
    .with_source_watermarks(vec![SourceWatermark {
        source: "fetch_google_review_links".to_string(),
        high_watermark: now.to_rfc3339(),
    }]);
    write_current(store, &google_places).await;

    let google_facts = seed_skill_fact_current(
        lake,
        store,
        "google_review_facts",
        "google",
        "2026-07-13",
        &google_fact_partition_for(run_partition),
        vec![
            google_places.materialization_id.clone(),
            canonical.materialization_id.clone(),
        ],
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
        ("google_places_weekly", google_places),
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

fn search_property(id: &str, society_id: &str, society_name: &str) -> Property {
    Property {
        id: id.to_string(),
        title: format!("3 BHK in {society_name}"),
        area: "Whitefield".to_string(),
        area_id: "whitefield".to_string(),
        city: "Bengaluru".to_string(),
        society_id: society_id.to_string(),
        builder_name: "Proof Homes".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk: 3,
        price: 18_000_000,
        price_per_sqft: 12_000,
        carpet_area_sqft: 1_200,
        super_builtup_sqft: 1_500,
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: "Ready to Move".to_string(),
        metro_distance_mins: 10,
        maintenance_cost_monthly: 6_000,
        society_quality_score: Some(0.7),
        builder_quality_score: Some(0.7),
        document_completeness_score: Some(0.8),
        litigation_risk: Some(0.1),
        noise_score: Some(0.2),
        sunlight_score: Some(0.7),
        airport_noise_score: Some(0.1),
        waterlogging_risk_score: Some(0.2),
        traffic_score: Some(0.4),
        days_on_market: 20,
        greenery_score: None,
        open_space_score: None,
        resale_strength_score: None,
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: "Proof-first search fixture".to_string(),
        transparency_tags: Vec::new(),
        source_reference: "asset-dag-product-proof".to_string(),
        seller_id: None,
    }
}

fn search_society(id: &str, name: &str) -> Society {
    Society {
        id: id.to_string(),
        name: name.to_string(),
        area: "Whitefield".to_string(),
        city: "Bengaluru".to_string(),
        builder_name: "Proof Homes".to_string(),
        year_built: 2024,
        total_units: 500,
        summary: String::new(),
        maintenance_sentiment: String::new(),
        livability_sentiment: String::new(),
        common_positives: Vec::new(),
        common_complaints: Vec::new(),
        review_summary: String::new(),
        google_reviews_url: None,
        future_google_place_name: String::new(),
        future_google_place_id: None,
        future_review_enrichment_status: String::new(),
    }
}

fn asset_id(id: &str) -> AssetId {
    AssetId::new(id).unwrap()
}

fn run_step<'a>(
    manifest: &'a backend::assets::AssetDagRunManifest,
    id: &str,
) -> &'a backend::assets::AssetRunStep {
    manifest
        .steps
        .iter()
        .find(|step| step.asset_id == asset_id(id))
        .expect("asset step should be present")
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
    AssetPartition::new([("source", "google")])
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
    let _ = run_partition;
    AssetPartition::new([("source", "google")])
}

fn executed_position(executed_assets: &[AssetId], id: &str) -> usize {
    executed_assets
        .iter()
        .position(|executed_asset_id| executed_asset_id == &asset_id(id))
        .unwrap_or_else(|| panic!("missing executed asset {id}"))
}

fn generated_only_source_inputs(now: chrono::DateTime<Utc>) -> AssetSourceInputs {
    AssetSourceInputs {
        generated_context_summaries: Some(generated_context_summary_input(
            "society:green-acre-whitefield",
            "If you are looking at Green Acre Whitefield, the generated context should travel through the DAG like any other sourced fact.",
            now + Duration::minutes(2),
        )),
        ..Default::default()
    }
}

fn mock_source_inputs(now: chrono::DateTime<Utc>) -> AssetSourceInputs {
    AssetSourceInputs {
        source_failures: Default::default(),
        rera_registry_monthly: Some(mock_rera_input(now)),
        prestige_inventory_weekly: Some(PrestigeInventoryWeeklyInput {
            snapshot_date: "2026-07-13".to_string(),
            records: vec![PrestigeInventoryObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                source_project_id: "prestige-green-acre".to_string(),
                source_project_name: "Green Acre Whitefield".to_string(),
                source_project_slug: "green-acre-whitefield".to_string(),
                source_url: "https://www.prestigeconstructions.com/residential-projects/bangalore/green-acre-whitefield".to_string(),
                status: Some("Under Construction".to_string()),
                land_area_acres: Some(10.0),
                starting_price_inr: Some(25_000_000.0),
                price_display: Some("INR 2.5 Cr onwards".to_string()),
                bhk_options: vec!["3 BHK".to_string()],
                total_units: Some(600),
                latitude: Some(12.9698),
                longitude: Some(77.7500),
                maps_url: Some("https://maps.google.com/?q=12.9698,77.7500".to_string()),
                address: Some("Whitefield, Bengaluru".to_string()),
                observed_at: now + Duration::minutes(2),
            }],
            source_watermarks: Vec::new(),
        }),
        external_listings_weekly: Some(ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-13".to_string(),
            records: vec![ExternalListingObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                source_name: "fixture_portal".to_string(),
                source_url: Some("https://example.com/green-acre-3bhk".to_string()),
                price: Some(31_000_000.0),
                price_min: Some(30_000_000.0),
                price_max: Some(32_000_000.0),
                area_sqft: Some(1_900.0),
                area_sqft_min: Some(1_850.0),
                area_sqft_max: Some(1_930.0),
                price_per_sqft_min: Some(15_385.0),
                price_per_sqft_max: Some(17_297.0),
                price_display: Some("INR 3.0-3.2 Cr".to_string()),
                area_display: Some("1850-1930".to_string()),
                price_per_sqft_display: Some("15385-17297".to_string()),
                configuration: Some("3BHK".to_string()),
                area_type: Some("super built-up".to_string()),
                bhk: Some(3.0),
                bathrooms: Some(3.0),
                floor: Some("12".to_string()),
                society: Some("Green Acre Whitefield".to_string()),
                locality: Some("Whitefield".to_string()),
                observed_at: now + Duration::minutes(2),
            }],
            source_watermarks: Vec::new(),
        }),
        external_images_weekly: Some(ExternalImagesWeeklyInput {
            snapshot_date: "2026-07-13".to_string(),
            records: vec![ExternalImageObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                source_name: "magicbricks".to_string(),
                source_page_url: "https://www.magicbricks.com/green-acre-whitefield".to_string(),
                image_url: "https://img.staticmb.com/mbimages/project/green-acre-elevation.jpg"
                    .to_string(),
                image_kind: Some("exterior".to_string()),
                width: Some(1200),
                height: Some(800),
                rank: Some(1),
                score: Some(94.0),
                alt_text: Some("Green Acre Whitefield elevation".to_string()),
                storage_policy: Some("link_only".to_string()),
                content_sha256: None,
                observed_at: now + Duration::minutes(2),
            }],
            source_watermarks: Vec::new(),
        }),
        metro_stations_monthly: Some(MetroStationsMonthlyInput {
            snapshot_date: "2026-07".to_string(),
            records: vec![MetroStationObservationRecord {
                station_id: "osm:station:hopefarm".to_string(),
                name: "Hopefarm Channasandra".to_string(),
                network: Some("Namma Metro".to_string()),
                operator: Some("BMRCL".to_string()),
                status: "operational".to_string(),
                latitude: 12.9876,
                longitude: 77.7521,
                source_url: "https://www.openstreetmap.org/node/hopefarm".to_string(),
                observed_at: now + Duration::minutes(2),
            }],
            source_watermarks: Vec::new(),
        }),
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
        google_places_weekly: Some(GooglePlacesWeeklyInput {
            snapshot_date: "2026-07-13".to_string(),
            records: vec![GooglePlaceSnapshotRecord {
                entity_id: String::new(),
                project_key: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                query: "green acre whitefield reviews".to_string(),
                place_name: Some("Green Acre Whitefield".to_string()),
                place_id: Some("green-acre".to_string()),
                reviews_url: "https://maps.google.com/?cid=green-acre".to_string(),
                rating: Some(4.4),
                review_count: Some(321),
                review_snippets: Vec::new(),
                address: Some("Whitefield, Bengaluru".to_string()),
                confidence: 0.82,
                fetched_at: now + Duration::minutes(2),
                fetch_source: "mock_google_places".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        google_nearby_places_weekly: Some(GoogleNearbyPlacesWeeklyInput {
            snapshot_date: "2026-07-13".to_string(),
            records: vec![GoogleNearbyPlaceRecord {
                entity_id: String::new(),
                project_key: Some("PRM/KA/RERA/1251/446/PR/130726/008888".to_string()),
                query: "schools near green acre whitefield".to_string(),
                category: "school".to_string(),
                place_name: "Greenwood High".to_string(),
                place_id: Some("greenwood-high".to_string()),
                place_url: "https://maps.google.com/?cid=greenwood-high".to_string(),
                distance_km: Some(1.2),
                latitude: Some(12.9720),
                longitude: Some(77.5960),
                rating: Some(4.3),
                review_count: Some(420),
                primary_type: Some("school".to_string()),
                place_types: vec!["school".to_string()],
                confidence: 0.82,
                fetched_at: now + Duration::minutes(2),
                fetch_source: "mock_google_nearby".to_string(),
            }],
            source_watermarks: Vec::new(),
        }),
        generated_context_summaries: Some(generated_context_summary_input(
            "society:green-acre-whitefield",
            "If you are looking at Green Acre Whitefield, the generated context should travel through the DAG like any other sourced fact.",
            now + Duration::minutes(2),
        )),
    }
}

fn generated_context_summary_input(
    entity_id: &str,
    summary: &str,
    learned_at: chrono::DateTime<Utc>,
) -> SkillFactsInput {
    let metadata = r#"{"provider":"openai-compatible","quality_status":"passed"}"#.to_string();
    SkillFactsInput {
        source: "local_summary".to_string(),
        snapshot_date: "2026-07-13".to_string(),
        facts: vec![
            SkillFactRecord {
                entity_id: entity_id.to_string(),
                fact_key: "generated_context_summary".to_string(),
                value_type: "text".to_string(),
                value_json: serde_json::to_string(&FactValue::Text(summary.to_string())).unwrap(),
                confidence: 0.72,
                source_type: "Computed".to_string(),
                source_url: None,
                model: Some("fixture-local-summary".to_string()),
                skill_id: Some("generated_context_summary_local".to_string()),
                triggered_by: Some("asset_dag".to_string()),
                learned_at,
                run_id: "generated-summary-fixture".to_string(),
                input_hash: "sha256:generated-summary-fixture".to_string(),
            },
            SkillFactRecord {
                entity_id: entity_id.to_string(),
                fact_key: "generated_context_summary_metadata".to_string(),
                value_type: "text".to_string(),
                value_json: serde_json::to_string(&FactValue::Text(metadata)).unwrap(),
                confidence: 0.72,
                source_type: "Computed".to_string(),
                source_url: None,
                model: Some("fixture-local-summary".to_string()),
                skill_id: Some("generated_context_summary_local".to_string()),
                triggered_by: Some("asset_dag".to_string()),
                learned_at,
                run_id: "generated-summary-fixture".to_string(),
                input_hash: "sha256:generated-summary-fixture".to_string(),
            },
        ],
        fact_annotations: vec![],
        source_watermarks: vec![SourceWatermark {
            source: "generated_context_summaries".to_string(),
            high_watermark: learned_at.to_rfc3339(),
        }],
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
                total_land_area_sqm: Some(40_500.0),
                land_litigation: Some(false),
                source_url: "https://rera.karnataka.gov.in/projectViewDetails".to_string(),
                fetched_at: now,
            },
        ],
        detail_facts: vec![SkillFactRecord {
            entity_id: "society:green-acre-whitefield".to_string(),
            fact_key: "rera_lat_lng".to_string(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text("12.9698,77.7500".to_string()))
                .unwrap(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
            source_url: Some("https://rera.karnataka.gov.in/projectViewDetails".to_string()),
            model: None,
            skill_id: Some("fetch_rera".to_string()),
            triggered_by: Some("asset_dag_fixture".to_string()),
            learned_at: now,
            run_id: "rera-fixture".to_string(),
            input_hash: "sha256:rera-green-acre-coordinate".to_string(),
        }],
        detail_fact_annotations: vec![SkillFactAnnotationRecord {
            entity_id: "society:green-acre-whitefield".to_string(),
            fact_key: "rera_lat_lng".to_string(),
            display_template: Some("RERA coordinates: {value}".to_string()),
            answers_preferences_json: "[]".to_string(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "[]".to_string(),
        }],
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
