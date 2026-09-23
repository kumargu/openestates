use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use backend::api::build_app_router_with_lake;
use backend::assets::{
    default_openestates_registry, load_society_gold_records, read_skill_fact_artifact_rows,
    AssetDagExecutionOptions, AssetDagExecutor, AssetMaterializationStore, AssetPartition,
    AssetSourceInputs, BengaluruMetroStationInput, BengaluruMetroStationsInput, DagRunStatus,
    EnvironmentGroundwaterPotentialInput, EnvironmentGroundwaterPotentialZone,
    EnvironmentRingPoint, ExternalImageObservationRecord, ExternalImagesWeeklyInput,
    ExternalListingObservationRecord, ExternalListingsWeeklyInput, GoogleNearbyPlaceRecord,
    GoogleNearbyPlacesWeeklyInput, GooglePlaceSnapshotRecord, GooglePlacesWeeklyInput,
    OsmLocalityBoundariesInput, OsmLocalityBoundaryInput, OsmPowerInfrastructureInput,
    OsmPowerLineObservationRecord, OsmSocietyAccessInput, ReraProjectPlanFramesInput,
    ReraProjectSnapshotRecord, ReraRegistryMonthlyInput, SkillFactAnnotationRecord,
    SkillFactRecord, SocietyGoldManifest, SourceEntitySeed, SourceWatermark,
    StormwaterDrainObservationRecord, StormwaterDrainRiskInput, BUILDER_RERA_AGGREGATES_ASSET_ID,
    EXTERNAL_LISTINGS_WEEKLY_ASSET_ID, EXTERNAL_LISTING_FACTS_ASSET_ID,
};
use backend::catalog::CatalogRecords;
use backend::knowledge::{FactValue, KnowledgeGraph};
use backend::lake::LakeStore;
use backend::serving::{BundleArtifactKind, ServingBundleBuilder, ServingBundleLoader};
use backend::state::{AppState, SearchResponseCache};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tokio::sync::{mpsc, RwLock};
use tower::ServiceExt;

#[tokio::test]
async fn materialized_weak_claims_are_excluded_by_validation_and_runtime() {
    let (root, state, app) = materialized_contract(false).await;
    let original = state.search_runtime.load_full();
    let manifest = &original.bundle.manifest;
    let mut facts = backend::serving::read_facts_parquet(
        &std::fs::read(root.path().join(&manifest.fact_parquet_key)).unwrap(),
    )
    .unwrap();
    let metadata = backend::serving::read_search_metadata_parquet(
        &std::fs::read(root.path().join(&manifest.search_metadata_parquet_key)).unwrap(),
    )
    .unwrap();
    let rera = backend::serving::read_rera_evidence_parquet(
        &std::fs::read(root.path().join(&manifest.rera_evidence_parquet_key)).unwrap(),
    )
    .unwrap();
    let mut weakened = 0;
    for fact in &mut facts {
        if fact.fact_key == "listing_3bhk" {
            fact.confidence = 0.0;
            weakened += 1;
        }
    }
    assert!(weakened >= 3);
    let mut edges = original.bundle.edges.clone();
    for edge in &mut edges {
        if edge.edge_type == "built_by" {
            edge.confidence = 0.0;
        }
    }
    let lake = LakeStore::local(root.path()).unwrap();
    let version = "materialized-admission-rejection";
    ServingBundleBuilder::new(lake.clone())
        .build_from_catalog_records(
            original.bundle.entities.clone(),
            facts,
            metadata,
            edges,
            rera,
            version,
        )
        .await
        .unwrap();
    let report = backend::serving::validate_search_serving_candidate(&lake, version)
        .await
        .unwrap();
    assert!(report
        .excluded_search_capabilities
        .iter()
        .any(|claim| claim.fact_key == "listing_3bhk"
            && claim.reason == "ineligible_inventory_source_or_confidence"));
    assert!(report
        .excluded_search_capabilities
        .iter()
        .any(|claim| claim.fact_key == "built_by"
            && claim.reason == "requires_eligible_unambiguous_identity"));
    let loaded = ServingBundleLoader::new(lake, root.path().join("admission-cache"))
        .load_search_bundle(version)
        .await
        .unwrap();
    let runtime = backend::data_loader::runtime_snapshot_from_serving_bundle(Arc::new(loaded));
    assert!(
        !runtime.properties.is_empty(),
        "societies remain browseable"
    );
    assert!(runtime
        .inventory_options
        .values()
        .all(|option| option.bhk != Some(3)));
    for edge in runtime
        .bundle
        .edges
        .iter()
        .filter(|edge| edge.edge_type == "built_by")
    {
        assert_eq!(
            runtime
                .identity_evaluation
                .evaluate(
                    &edge.from_entity_id,
                    &edge.to_entity_id,
                    runtime.bundle.manifest.proof_snapshot_identity()
                )
                .state,
            backend::search::EvaluationState::Unknown
        );
    }
    state.search_runtime.store(Arc::new(runtime));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/search?q=3BHK")
                .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 240], 41000))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["active"]["results"]["totalMatches"], 0);
}

#[tokio::test]
async fn three_societies_reach_serving_with_listing_and_builder_evidence() {
    let (_original_root, state, app) = materialized_contract(false).await;
    let (_updated_root, updated_state, _) = materialized_contract(true).await;
    if let Ok(address) = std::env::var("OPENESTATES_CONTRACT_SERVER_ADDR") {
        // Test-only control listener; the application still uses the production router.
        // Both snapshots have completed the same source -> Parquet -> API assertions.
        let original = state.search_runtime.load_full();
        let updated = updated_state.search_runtime.load_full();
        assert_ne!(
            original.bundle.manifest.proof_snapshot_identity(),
            updated.bundle.manifest.proof_snapshot_identity()
        );
        let control = axum::Router::new().route(
            "/snapshot/{version}",
            axum::routing::post(
                move |axum::extract::Path(version): axum::extract::Path<String>| {
                    let state = state.clone();
                    let snapshot = match version.as_str() {
                        "original" => Some(original.clone()),
                        "updated" => Some(updated.clone()),
                        _ => None,
                    };
                    async move {
                        let Some(snapshot) = snapshot else {
                            return StatusCode::BAD_REQUEST;
                        };
                        state.search_runtime.store(snapshot);
                        StatusCode::NO_CONTENT
                    }
                },
            ),
        );
        let control_listener = tokio::net::TcpListener::bind("127.0.0.1:4017")
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        tokio::try_join!(
            axum::serve(control_listener, control).into_future(),
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>()
            )
            .into_future(),
        )
        .unwrap();
    }
}

async fn materialized_contract(carpet: bool) -> (tempfile::TempDir, Arc<AppState>, axum::Router) {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let observed_at = Utc.with_ymd_and_hms(2026, 7, 14, 12, 0, 0).unwrap();
    let mut projects = fixtures();
    if carpet {
        projects[0].area_basis = "carpet";
    }
    let partition = AssetPartition::new([
        ("dt", "2026-07-14"),
        ("society", "project-enrichment-fixture"),
    ]);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(partition, observed_at)
                .with_version("2026-07-14T12:00Z")
                .with_source_inputs(source_inputs(&projects, observed_at)),
        )
        .await
        .unwrap();
    assert_eq!(report.manifest.status, DagRunStatus::Succeeded);
    assert_eq!(report.manifest.failed_count, 0);

    for asset_id in [
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        BUILDER_RERA_AGGREGATES_ASSET_ID,
        "society_gold_snapshot",
    ] {
        assert!(
            report
                .executed_assets
                .iter()
                .any(|executed| executed.as_str() == asset_id),
            "{asset_id} should execute"
        );
    }
    let listings_record = store
        .current_record(
            &asset_id(EXTERNAL_LISTINGS_WEEKLY_ASSET_ID),
            &AssetPartition::new([("source", "external_listing")]),
        )
        .await
        .unwrap();
    assert_eq!(listings_record.row_count, 5);
    assert!(listings_record.artifacts.iter().any(|artifact| {
        artifact.content_type == "application/vnd.apache.parquet"
            && artifact.key.ends_with("listings/part-00000.parquet")
    }));

    let builder_record = store
        .current_record(
            &asset_id(BUILDER_RERA_AGGREGATES_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    let builder_asset_rows = read_skill_fact_artifact_rows(&lake, &[builder_record])
        .await
        .unwrap();
    assert!(builder_asset_rows
        .facts
        .iter()
        .any(|fact| fact.entity_id == "builder:prestige-estates-projects-limited"));

    let gold_manifest_key = report
        .manifest
        .steps
        .iter()
        .find(|step| step.asset_id.as_str() == "society_gold_snapshot")
        .and_then(|step| {
            step.artifacts
                .iter()
                .find(|artifact| artifact.key.ends_with("/manifest.json"))
        })
        .expect("gold manifest")
        .key
        .clone();
    let gold_manifest: SocietyGoldManifest = lake
        .get_json(&backend::lake::LakeKey::new(gold_manifest_key).unwrap())
        .await
        .unwrap();
    let gold = load_society_gold_records(&lake, &gold_manifest)
        .await
        .unwrap();
    let bundle_version = if carpet {
        "project-enrichment-fixture-carpet"
    } else {
        "project-enrichment-fixture"
    };
    let records = CatalogRecords::from_society_gold(&gold, Vec::new()).unwrap();
    ServingBundleBuilder::new(lake.clone())
        .build_from_catalog_records(
            records.entities,
            records.facts,
            records.search_metadata,
            records.edges,
            records.rera_evidence,
            bundle_version,
        )
        .await
        .unwrap();
    let loaded = ServingBundleLoader::new(lake.clone(), root.path().join("serving-cache"))
        .load_search_bundle(bundle_version)
        .await
        .unwrap();
    for project in &projects {
        let alias = canonical_id(project.registration);
        let rows = loaded.fact_index.entity(&alias).unwrap();
        for fact_key in [
            "rera_number",
            "listing_3bhk",
            "listing_price_3bhk",
            "listing_price_range_3bhk",
            "listing_area_sqft_3bhk",
            "listing_area_sqft_range_3bhk",
            "listing_price_per_sqft_range_3bhk",
            "listing_area_type_3bhk",
            "listing_source_url_3bhk",
            "listing_source_name_3bhk",
        ] {
            assert!(
                rows.facts.iter().any(|fact| fact.fact_key == fact_key),
                "{alias} should contain {fact_key}"
            );
        }
        let listing_source_name = rows
            .facts
            .iter()
            .find(|fact| fact.fact_key == "listing_source_name_3bhk")
            .expect("external listing source name should reach serving");
        assert_eq!(
            listing_source_name.value,
            FactValue::Text("MagicBricks".to_string())
        );
        let listing_price = rows
            .facts
            .iter()
            .find(|fact| fact.fact_key == "listing_price_3bhk")
            .expect("listing price should retain its source observation");
        let listing_bhk = rows
            .facts
            .iter()
            .find(|fact| fact.fact_key == "listing_3bhk")
            .expect("listing option should retain its source observation");
        let price_observation = listing_price.observation.as_ref().unwrap();
        let bhk_observation = listing_bhk.observation.as_ref().unwrap();
        assert_eq!(price_observation.provider, "MagicBricks");
        assert!(price_observation
            .provider_observation_id
            .starts_with("external_listing_record:sha256:"));
        assert_eq!(
            price_observation.observation_id, bhk_observation.observation_id,
            "BHK and price must retain the same raw listing observation"
        );
        assert!(price_observation.validate().is_ok());
        assert!(price_observation
            .asset_lineage
            .iter()
            .any(|entry| entry.starts_with("materialization:")));
        assert!(price_observation
            .asset_lineage
            .iter()
            .any(|entry| entry.starts_with("artifact:")));
    }

    let builder_rows = loaded
        .fact_index
        .entity("builder:prestige-estates-projects-limited")
        .unwrap();
    assert!(builder_rows.facts.iter().any(|fact| {
        fact.fact_key == "builder_project_count" && fact.value == FactValue::Numeric(3.0)
    }));
    assert!(builder_rows
        .facts
        .iter()
        .any(|fact| fact.fact_key == "builder_rera_status_breakdown"));

    let topology_artifact = loaded
        .manifest
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == BundleArtifactKind::Other
                && artifact
                    .key
                    .ends_with("diagnostics/market_geo_topology.json")
        })
        .expect("normal DAG serving should emit topology diagnostics");
    let topology: Value = serde_json::from_slice(
        &lake
            .get_bytes(&backend::lake::LakeKey::new(topology_artifact.key.clone()).unwrap())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(topology["geo_cell_count"], 1);
    assert_eq!(topology["point_assignment_count"], 5);
    assert_eq!(topology["market_coverage_count"], 1);
    assert_eq!(
        topology["ambiguous_point_cell_entity_ids"],
        serde_json::json!([])
    );
    assert_eq!(topology["evidence_validation_error"], Value::Null);

    let loaded = Arc::new(loaded);
    let runtime = backend::data_loader::runtime_snapshot_from_serving_bundle(loaded.clone());
    assert_eq!(runtime.properties.len(), 4);
    let whitefield_market_id = loaded
        .entities
        .iter()
        .find(|entity| {
            entity.name == "Whitefield" && entity.root_source.as_deref() == Some("market_locality")
        })
        .map(|entity| entity.entity_id.as_str())
        .expect("source seed locality should reach the serving entity table");
    assert_eq!(
        runtime
            .bundle
            .graph_index
            .covered_cells(whitefield_market_id)
            .len(),
        1
    );
    let (search_event_tx, _search_event_rx) = mpsc::channel(8);
    let state = Arc::new(AppState {
        execution: backend::security::ExecutionLanes::current(),
        search_runtime: ArcSwap::from_pointee(runtime),
        search_cache: SearchResponseCache::new(8),
        search_revision_caches: backend::state::SearchRevisionCaches::new(
            8,
            8 * 1024 * 1024,
            8,
            8 * 1024 * 1024,
        ),
        property_catalog_cache: tokio::sync::Mutex::new(None),
        search_event_tx,
        search_log_dropped_count: AtomicU64::new(0),
        recommendation_cache: RwLock::new(std::collections::HashMap::new()),

        map_overlays: Arc::new(backend::routes::map_overlays::CityMapOverlays::default()),
        project_root: root.path().to_path_buf(),
        process_started_at: Utc::now(),
    });
    let snapshot_identity = state
        .search_runtime
        .load()
        .bundle
        .manifest
        .proof_snapshot_identity()
        .to_string();
    let app = build_app_router_with_lake(state.clone(), lake);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/search?q=3BHK%20in%20Whitefield")
                .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 1], 41000))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["active"]["results"]["totalMatches"], 3);
    assert_eq!(
        body["active"]["results"]["orderedResultIds"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // This is deliberately the production router over materialized source inputs.
    // A passing engine-only fixture cannot establish receipt resolvability.
    for query in ["3BHK", "3BHK above 10 acres", "3BHK near metro"] {
        let query: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/search?q={query}"))
                    .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 2], 41000))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let journey: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(
            journey["active"]["results"]["totalMatches"]
                .as_u64()
                .unwrap()
                > 0,
            "{query} must exercise matches"
        );
        let mut resolved_count = 0;
        let mut numeric_count = 0;
        for set in journey["active"]["results"]["resultSets"]
            .as_array()
            .unwrap()
        {
            for result in set["results"].as_array().unwrap() {
                assert!(
                    !result["reasons"].as_array().unwrap().is_empty(),
                    "matched inventory retains witnesses"
                );
                for reason in result["reasons"].as_array().unwrap() {
                    let response = app.clone().oneshot(Request::builder()
                        .method("POST").uri("/api/search/proofs/resolve")
                        .header("content-type", "application/json")
                        .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 3], 41000))))
                        .body(Body::from(serde_json::json!({"proofToken": reason["proofToken"], "propertyId": result["id"]}).to_string())).unwrap()).await.unwrap();
                    let status = response.status();
                    let receipt: Value = serde_json::from_slice(
                        &to_bytes(response.into_body(), 4 * 1024 * 1024)
                            .await
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(status, StatusCode::OK, "{query}: {receipt}");
                    resolved_count += 1;
                    if receipt["constraint"].is_object() {
                        numeric_count += 1;
                        assert_eq!(receipt["claim"]["unit"], receipt["constraint"]["unit"]);
                        assert!(receipt["claim"]["value"].is_number());
                    }
                    assert_eq!(receipt["snapshotIdentity"], snapshot_identity);
                    assert_eq!(receipt["resolutionStatus"], "resolved");
                    assert!(!receipt["sourceObservations"].as_array().unwrap().is_empty());
                }
            }
        }
        assert!(resolved_count > 0);
        if query.contains("acres") {
            assert!(
                numeric_count > 0,
                "numeric eligibility must issue its constraint and witness"
            );
        }
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/search?q=3BHK%20carpet%20area%20above%201500%20sqft")
                .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 4], 41000))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let journey: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        journey["active"]["results"]["totalMatches"], 0,
        "carpet search is deferred even when the source contains carpet measurements"
    );
    for (query, expected) in [
        ("2BHK%20under%201.5Cr%20above%201400%20sqft", 0),
        ("3BHK%20above%201500%20sqft", 3),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/search?q={query}"))
                    .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 14], 41000))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            result["active"]["results"]["totalMatches"], expected,
            "{query}"
        );
    }
    let ids = body["active"]["results"]["orderedResultIds"]
        .as_array()
        .unwrap();
    let mut context_receipt_count = 0;
    for id in ids {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/properties/{}?snapshotIdentity={snapshot_identity}",
                        id.as_str().unwrap()
                    ))
                    .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 5], 41000))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let detail: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        let evidence = detail["evidence"]["sections"].as_array().unwrap();
        for item in evidence
            .iter()
            .flat_map(|section| section["items"].as_array().unwrap())
        {
            let references = item["evidence"]
                .as_array()
                .expect("every buyer claim binds evidence");
            assert!(!references.is_empty());
            for reference in references {
                let reference: backend::serving::EvidenceRef =
                    serde_json::from_value(reference.clone()).unwrap();
                assert_eq!(reference.snapshot_identity, snapshot_identity);
                match reference.evidence_id {
                    backend::serving::EvidenceId::Observation(ref id) => {
                        assert!(loaded.evidence_index.observation(id).is_some())
                    }
                    backend::serving::EvidenceId::Derivation(ref id) => {
                        assert!(loaded.evidence_index.derivation(id).is_some())
                    }
                    backend::serving::EvidenceId::Entity(_)
                    | backend::serving::EvidenceId::Relationship(_) => {
                        panic!("source panels require observed facts")
                    }
                }
            }
        }
        let context = &detail["context"];
        assert_eq!(context["snapshotIdentity"], snapshot_identity);
        assert!(context.get("surfaceId").is_none());
        assert!(context.get("layers").is_none());
        context_receipt_count += context["features"].as_array().unwrap().len();
        for feature in context["features"].as_array().unwrap() {
            let reference: backend::serving::EvidenceRef =
                serde_json::from_value(feature["fact"]["evidence"].clone()).unwrap();
            assert_eq!(reference.snapshot_identity, snapshot_identity);
            let backend::serving::EvidenceId::Observation(id) = reference.evidence_id else {
                panic!("context fact must retain its observation");
            };
            assert!(loaded.evidence_index.observation(&id).is_some());
            if feature["fact"]["factKey"] == "nearby_schools" {
                assert_eq!(
                    feature["fact"]["value"]["type"], "Numeric",
                    "distance must be typed before serving"
                );
                let target = &feature["target"];
                assert!(
                    target["entityId"]
                        .as_str()
                        .is_some_and(|id| id.starts_with("place:canonical:")),
                    "source identity must bind the canonical school: {target}"
                );
                assert!(
                    target["point"].is_array(),
                    "sourced school coordinates must survive"
                );
                assert!(!target["geometryEvidence"].as_array().unwrap().is_empty());
            }
        }
        let measurement = &detail["property"]["area_measurement"];
        assert_eq!(
            measurement["evidence"]["snapshot_identity"],
            snapshot_identity
        );
        assert_eq!(measurement["unit"], "sqft");
        assert!(detail["property"].get("carpet_area_sqft").is_none());
    }
    assert!(
        context_receipt_count > 0,
        "materialized context must contain receipts"
    );
    for (snapshot, expected) in [
        (&snapshot_identity, StatusCode::OK),
        (&"retired".to_string(), StatusCode::CONFLICT),
    ] {
        let response = app.clone().oneshot(Request::builder().method("POST").uri("/api/properties/batch")
            .header("content-type", "application/json")
            .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 6], 41000))))
            .body(Body::from(serde_json::json!({"propertyIds": [ids[1], ids[0], ids[1], "missing"], "snapshotIdentity": snapshot}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            let batch: Value = serde_json::from_slice(
                &to_bytes(response.into_body(), 4 * 1024 * 1024)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(batch["items"].as_array().unwrap().len(), 2);
            for item in batch["items"].as_array().unwrap() {
                if item["area_measurement"]["basis"] != "carpet" {
                    assert!(
                        item.get("carpet_area_sqft").is_none(),
                        "summary must preserve measurement basis"
                    );
                }
            }
            assert_eq!(batch["items"][0]["id"], ids[1]);
            assert_eq!(batch["items"][1]["id"], ids[0]);
            assert_eq!(batch["missingIds"], serde_json::json!(["missing"]));
        }
    }
    for resource in ["", "/rera", "/recommendations", "/evidence"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/properties/{}{resource}?snapshotIdentity=retired",
                        ids[0].as_str().unwrap()
                    ))
                    .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 6], 41000))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "{resource} must reject mixed snapshots"
        );
    }
    (root, state, app)
}

struct ProjectFixture {
    name: &'static str,
    area_basis: &'static str,
    registration: &'static str,
    status: &'static str,
    acres: f64,
    price: f64,
    latitude: f64,
    longitude: f64,
}

fn fixtures() -> Vec<ProjectFixture> {
    vec![
        ProjectFixture {
            name: "Prestige Raintree Park",
            area_basis: "mixed",
            registration: "PRM/KA/RERA/1251/446/PR/270824/006981",
            status: "Under Construction",
            acres: 21.0,
            price: 37_000_000.0,
            latitude: 12.95384,
            longitude: 77.74546,
        },
        ProjectFixture {
            name: "Prestige Park Grove",
            area_basis: "built_up",
            registration: "PRM/KA/RERA/1251/446/PR/100823/006141",
            status: "Sold Out",
            acres: 71.41,
            price: 36_000_000.0,
            latitude: 13.01903,
            longitude: 77.75745,
        },
        ProjectFixture {
            name: "Prestige Lavender Fields",
            area_basis: "super_builtup",
            registration: "PRM/KA/RERA/1251/446/PR/290423/005906",
            status: "Sold Out",
            acres: 18.2,
            price: 24_300_000.0,
            latitude: 12.93405,
            longitude: 77.73904,
        },
    ]
}

fn source_inputs(
    projects: &[ProjectFixture],
    observed_at: chrono::DateTime<Utc>,
) -> AssetSourceInputs {
    let mut detail_facts = Vec::new();
    let mut detail_fact_annotations = Vec::new();
    let rera_projects = projects
        .iter()
        .map(|project| {
            let canonical = canonical_id(project.registration);
            let alias = format!("society:{}", slug(project.name));
            for entity_id in [canonical, alias] {
                detail_facts.push(SkillFactRecord {
                    entity_id: entity_id.clone(),
                    fact_key: "rera_lat_lng".to_string(),
                    value_type: "text".to_string(),
                    value_json: serde_json::to_string(&FactValue::Text(format!(
                        "{},{}",
                        project.latitude, project.longitude
                    )))
                    .unwrap(),
                    confidence: 1.0,
                    source_type: "Rera".to_string(),
                    source_url: Some(format!("https://rera.example/{}", project.registration)),
                    model: None,
                    skill_id: Some("fetch_rera".to_string()),
                    triggered_by: Some("asset_dag".to_string()),
                    learned_at: observed_at,
                    run_id: "rera-fixture".to_string(),
                    input_hash: format!("sha256:{}", project.registration),
                    observation_provider: None,
                    provider_observation_id: None,
                    asset_lineage: Vec::new(),
                });
                let mut documents = detail_facts.last().unwrap().clone();
                documents.fact_key = "rera_document_manifest".to_string();
                documents.value_json = serde_json::to_string(&FactValue::Text(
                    serde_json::json!([{
                        "artifact_id": "fixture-sanctioned-plan", "kind": "sanctioned_plan",
                        "label": "Sanctioned plan", "document_group": "plans",
                        "source_url": "https://rera.example/documents/sanctioned-plan.pdf",
                        "buyer_visibility": "public", "confidence": 1.0
                    }])
                    .to_string(),
                ))
                .unwrap();
                documents.observation_provider = Some("Rera".to_string());
                documents.provider_observation_id =
                    Some(format!("{}:documents", project.registration));
                documents.asset_lineage = vec!["rera_registry_monthly".to_string()];
                detail_facts.push(documents);
                detail_fact_annotations.push(SkillFactAnnotationRecord {
                    entity_id: entity_id.clone(),
                    fact_key: "rera_document_manifest".to_string(),
                    display_template: None,
                    answers_preferences_json: "[]".to_string(),
                    scoring_direction: None,
                    scoring_weight: None,
                    scoring_thresholds_json: "[]".to_string(),
                });
                detail_fact_annotations.push(SkillFactAnnotationRecord {
                    entity_id,
                    fact_key: "rera_lat_lng".to_string(),
                    display_template: Some("RERA coordinates: {value}".to_string()),
                    answers_preferences_json: "[]".to_string(),
                    scoring_direction: None,
                    scoring_weight: None,
                    scoring_thresholds_json: "[]".to_string(),
                });
            }
            ReraProjectSnapshotRecord {
                ack_number: None,
                registration_number: Some(project.registration.to_string()),
                project_name: project.name.to_string(),
                promoter_name: Some("PRESTIGE ESTATES PROJECTS LIMITED".to_string()),
                status: Some(project.status.to_string()),
                project_type: Some("Residential".to_string()),
                project_address: Some("Whitefield, Bengaluru".to_string()),
                area_name: Some("Whitefield".to_string()),
                district: Some("Bengaluru Urban".to_string()),
                taluk: None,
                total_land_area_sqm: Some(project.acres * 4_046.856_422_4),
                land_litigation: Some(false),
                source_url: format!("https://rera.example/{}", project.registration),
                fetched_at: observed_at,
            }
        })
        .collect();
    let mut external_listing_records: Vec<_> = projects
        .iter()
        .map(|project| ExternalListingObservationRecord {
            entity_id: canonical_id(project.registration),
            property_id: Some(format!(
                "discovered-{}-3bhk",
                canonical_id(project.registration)
                    .strip_prefix("society:")
                    .unwrap()
            )),
            project_key: Some(project.registration.to_string()),
            source_name: "MagicBricks".to_string(),
            source_url: Some(format!("https://listings.example/{}", slug(project.name))),
            listing_type: Some("sale".to_string()),
            price: Some(project.price),
            price_min: Some(project.price),
            price_max: Some(project.price),
            area_sqft: Some(2_000.0),
            area_sqft_min: Some(2_000.0),
            area_sqft_max: Some(2_000.0),
            price_per_sqft_min: Some(project.price / 2_000.0),
            price_per_sqft_max: Some(project.price / 2_000.0),
            price_display: None,
            area_display: None,
            price_per_sqft_display: None,
            configuration: Some("3BHK".to_string()),
            area_type: Some(project.area_basis.to_string()),
            bhk: Some(3.0),
            bathrooms: Some(3.0),
            floor: Some("12".to_string()),
            society: Some(project.name.to_string()),
            locality: Some("Whitefield".to_string()),
            observed_at,
        })
        .collect();
    // Two actual offers must never be combined into one qualifying home.
    for (price, area) in [(10_000_000.0, 1_000.0), (30_000_000.0, 2_000.0)] {
        let mut offer = external_listing_records[0].clone();
        offer.property_id = None;
        offer.bhk = Some(2.0);
        offer.configuration = Some("2BHK".into());
        offer.price = Some(price);
        offer.price_min = Some(price);
        offer.price_max = Some(price);
        offer.area_sqft = Some(area);
        offer.area_sqft_min = Some(area);
        offer.area_sqft_max = Some(area);
        offer.area_type = Some("built-up".into());
        offer.source_url = Some(format!("https://listings.example/offer-{price}"));
        external_listing_records.push(offer);
    }
    let osm_power_records = projects
        .iter()
        .enumerate()
        .map(|(index, project)| OsmPowerLineObservationRecord {
            entity_id: canonical_id(project.registration),
            project_key: Some(project.registration.to_string()),
            query: format!("power=line around {}", project.name),
            osm_id: format!("way/power-{}", slug(project.name)),
            name: Some(format!("{} fixture transmission line", project.name)),
            power: "line".to_string(),
            voltage_kv: Some(220.0),
            distance_meters: 0.0,
            subject_latitude: Some(project.latitude),
            subject_longitude: Some(project.longitude),
            latitude: project.latitude,
            longitude: project.longitude,
            geometry_geojson: format!(
                r#"{{"type":"LineString","coordinates":[[{lon1},{lat1}],[{lon2},{lat2}]]}}"#,
                lon1 = project.longitude - 0.001,
                lat1 = project.latitude - 0.001,
                lon2 = project.longitude + 0.001,
                lat2 = project.latitude + 0.001
            ),
            source_tags: BTreeMap::from([
                ("power".to_string(), "line".to_string()),
                ("voltage".to_string(), "220000".to_string()),
            ]),
            source_url: Some(format!("https://www.openstreetmap.org/way/power-{index}")),
            confidence: 0.82,
            fetched_at: observed_at,
            fetch_source: "fixture_overpass_power".to_string(),
        })
        .collect();
    let stormwater_records = projects
        .iter()
        .enumerate()
        .map(|(index, project)| StormwaterDrainObservationRecord {
            entity_id: canonical_id(project.registration),
            project_key: Some(project.registration.to_string()),
            query: format!("stormwater drain around {}", project.name),
            drain_id: format!("swd/{}", slug(project.name)),
            name: Some(format!("{} fixture rajakaluve", project.name)),
            drain_type: "rajakaluve".to_string(),
            hierarchy: Some("primary_swd".to_string()),
            distance_meters: 0.0,
            intersects_property: false,
            subject_latitude: Some(project.latitude),
            subject_longitude: Some(project.longitude),
            latitude: project.latitude,
            longitude: project.longitude,
            geometry_geojson: format!(
                r#"{{"type":"LineString","coordinates":[[{lon1},{lat1}],[{lon2},{lat2}]]}}"#,
                lon1 = project.longitude - 0.001,
                lat1 = project.latitude - 0.001,
                lon2 = project.longitude + 0.001,
                lat2 = project.latitude + 0.001
            ),
            encroachment_record: None,
            source_tags: BTreeMap::from([("waterway".to_string(), "drain".to_string())]),
            source_url: Some(format!("https://data.opencity.in/swd/{index}")),
            source_type: Some("OpenCity".to_string()),
            confidence: 0.8,
            fetched_at: observed_at,
            fetch_source: "fixture_opencity_stormwater".to_string(),
        })
        .collect();
    let watermark = vec![SourceWatermark {
        source: "fixture".to_string(),
        high_watermark: observed_at.to_rfc3339(),
    }];
    let first_project = &projects[0];

    AssetSourceInputs {
        source_entities: projects
            .iter()
            .map(|project| SourceEntitySeed {
                entity_id: canonical_id(project.registration),
                alias_entity_id: Some(format!("society:{}", slug(project.name))),
                name: project.name.to_string(),
                area: Some("Whitefield".to_string()),
                city: Some("Bengaluru".to_string()),
                project_key: Some(project.registration.to_string()),
                latitude: Some(project.latitude),
                longitude: Some(project.longitude),
            })
            .collect(),
        rera_registry_monthly: Some(ReraRegistryMonthlyInput {
            snapshot_date: "2026-07".to_string(),
            projects: rera_projects,
            detail_facts,
            detail_fact_annotations,
            source_watermarks: watermark.clone(),
        }),
        rera_project_plan_frames: Some(ReraProjectPlanFramesInput {
            source: "fixture_rera_plans".to_string(),
            snapshot_date: "2026-07-14".to_string(),
            catalog_entity_count: projects.len(),
            exact_registration_count: projects.len(),
            projects: Vec::new(),
            failures: Vec::new(),
            source_watermarks: vec![SourceWatermark {
                source: "fixture_rera_plans_empty".to_string(),
                high_watermark: observed_at.to_rfc3339(),
            }],
        }),
        google_places_weekly: Some(GooglePlacesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![GooglePlaceSnapshotRecord {
                entity_id: canonical_id(first_project.registration),
                project_key: Some(first_project.registration.to_string()),
                query: "Prestige Raintree Park Whitefield".to_string(),
                place_name: Some("Prestige Raintree Park".to_string()),
                place_id: None,
                reviews_url: "https://maps.example/raintree/reviews".to_string(),
                rating: None,
                review_count: None,
                review_snippets: Vec::new(),
                address: Some("Whitefield".to_string()),
                latitude: None,
                longitude: None,
                confidence: 0.9,
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        google_nearby_places_weekly: Some(GoogleNearbyPlacesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![GoogleNearbyPlaceRecord {
                entity_id: canonical_id(first_project.registration),
                project_key: Some(first_project.registration.to_string()),
                query: "schools near Prestige Raintree Park".to_string(),
                category: "school".to_string(),
                place_name: "Greenwood High".to_string(),
                place_id: Some("greenwood-high".to_string()),
                place_url: "https://maps.example/greenwood-high".to_string(),
                distance_km: Some(1.2),
                latitude: Some(first_project.latitude + 0.0108),
                longitude: Some(first_project.longitude),
                rating: Some(4.3),
                review_count: Some(420),
                primary_type: Some("school".to_string()),
                place_types: vec!["school".to_string()],
                confidence: 0.82,
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        external_listings_weekly: Some(ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: external_listing_records,
            source_watermarks: watermark.clone(),
        }),
        external_images_weekly: Some(ExternalImagesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: projects
                .iter()
                .map(|project| ExternalImageObservationRecord {
                    entity_id: canonical_id(project.registration),
                    project_key: Some(project.registration.to_string()),
                    source_name: "MagicBricks".to_string(),
                    source_page_url: format!("https://images.example/{}", slug(project.name)),
                    image_url: format!("https://images.example/{}/hero.jpg", slug(project.name)),
                    original_image_url: Some(format!(
                        "https://images.example/{}/hero.jpg",
                        slug(project.name)
                    )),
                    image_kind: Some("exterior".to_string()),
                    source_bucket: Some("Project Image".to_string()),
                    candidate_kind: Some("exterior".to_string()),
                    quality_score: Some(0.92),
                    relevance_score: Some(0.88),
                    reject_reason: None,
                    allowed_slots: vec!["hero".to_string(), "gallery".to_string()],
                    dedupe_key: Some(format!(
                        "url:https://images.example/{}/hero.jpg",
                        slug(project.name)
                    )),
                    classification_method: Some("heuristic".to_string()),
                    gallery_order: None,
                    curation_confidence: None,
                    width: Some(1200),
                    height: Some(800),
                    rank: Some(1),
                    score: Some(90.0),
                    alt_text: Some(format!("{} exterior", project.name)),
                    storage_policy: Some("link_only".to_string()),
                    content_sha256: None,
                    observed_at,
                })
                .collect(),
            max_promoted_gallery_frames: None,
            source_health: Vec::new(),
            media_qa_report: None,
            source_watermarks: watermark.clone(),
        }),
        osm_locality_boundaries: Some(OsmLocalityBoundariesInput {
            snapshot_date: "2026-07-14".to_string(),
            source_url: "https://www.openstreetmap.org/relation/fixture-whitefield".to_string(),
            boundaries: vec![OsmLocalityBoundaryInput {
                osm_id: "relation/fixture-whitefield".to_string(),
                name: "Fixture Whitefield Cell".to_string(),
                geometry_geojson: r#"{"type":"Polygon","coordinates":[[[77.70,12.90],[77.80,12.90],[77.80,13.05],[77.70,13.05],[77.70,12.90]]]}"#.to_string(),
                source_url: "https://www.openstreetmap.org/relation/fixture-whitefield".to_string(),
                admin_level: Some("10".to_string()),
                members: Vec::new(),
            }],
            source_watermarks: watermark.clone(),
        }),
        environment_groundwater_potential: Some(EnvironmentGroundwaterPotentialInput {
            snapshot_date: "2026-07-14".to_string(),
            source_url: "https://example.com/groundwater.kml".to_string(),
            zones: vec![EnvironmentGroundwaterPotentialZone {
                zone_id: "whitefield-good-groundwater".to_string(),
                groundwater_potential_class: "Good".to_string(),
                rings: vec![vec![
                    EnvironmentRingPoint {
                        latitude: 12.90,
                        longitude: 77.70,
                    },
                    EnvironmentRingPoint {
                        latitude: 12.90,
                        longitude: 77.80,
                    },
                    EnvironmentRingPoint {
                        latitude: 13.05,
                        longitude: 77.80,
                    },
                    EnvironmentRingPoint {
                        latitude: 13.05,
                        longitude: 77.70,
                    },
                    EnvironmentRingPoint {
                        latitude: 12.90,
                        longitude: 77.70,
                    },
                ]],
                source_fields: BTreeMap::new(),
            }],
            source_watermarks: watermark.clone(),
        }),
        bengaluru_metro_stations: Some(BengaluruMetroStationsInput {
            snapshot_date: "2026-07-14".to_string(),
            source_url: "https://www.openstreetmap.org".to_string(),
            stations: vec![BengaluruMetroStationInput {
                station_id: "node/fixture-metro".to_string(),
                name: "Fixture Metro".to_string(),
                latitude: 12.97,
                longitude: 77.75,
                lines: vec!["Purple Line".to_string()],
                network: Some("Namma Metro".to_string()),
                operator: Some("BMRCL".to_string()),
                operational_status: Some("operational".to_string()),
                source_url: Some("https://www.openstreetmap.org/node/fixture-metro".to_string()),
                source_tags: BTreeMap::new(),
            }],
            source_watermarks: watermark.clone(),
        }),
        osm_society_access: Some(OsmSocietyAccessInput {
            snapshot_date: "2026-07-14".to_string(),
            records: Vec::new(),
            source_watermarks: vec![SourceWatermark {
                source: "fixture_osm_society_access_empty".to_string(),
                high_watermark: "records=0".to_string(),
            }],
        }),
        osm_power_infrastructure: Some(OsmPowerInfrastructureInput {
            snapshot_date: "2026-07-14".to_string(),
            records: osm_power_records,
            source_watermarks: watermark.clone(),
        }),
        stormwater_drains: Some(StormwaterDrainRiskInput {
            snapshot_date: "2026-07-14".to_string(),
            records: stormwater_records,
            source_watermarks: watermark.clone(),
        }),
        ..AssetSourceInputs::default()
    }
}

fn canonical_id(project_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_key.as_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("society:rera-{}", &digest[..16])
}

fn asset_id(value: &str) -> backend::assets::AssetId {
    backend::assets::AssetId::new(value).unwrap()
}

fn slug(value: &str) -> String {
    value
        .to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
