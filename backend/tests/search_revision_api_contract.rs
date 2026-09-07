use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use backend::api::build_app_router_with_lake;
use backend::graph::GraphIndex;
use backend::knowledge::{FactValue, KnowledgeGraph};
use backend::lake::LakeStore;
use backend::models::Property;
use backend::search::geo::GeoSearchIndex;
use backend::search::{SearchCapabilityIndex, SearchIndex};
use backend::security::ExecutionLanes;
use backend::serving::{
    DerivedEvidence, EvidenceRef, LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest,
    ServingEdgeRecord, ServingEntityAliasIndex, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, SourceObservation, SpatialServingIndex, TantivyRecallIndex,
};
use backend::state::{AppState, SearchResponseCache, SearchRuntimeSnapshot};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::sync::{mpsc, RwLock};
use tower::ServiceExt;

#[tokio::test]
async fn stateless_revision_route_enforces_runtime_parent_and_safe_outcomes() {
    let app = test_app().await;
    let parent_query = "3BHK in Hoodi under 2.4 Cr";
    let parent = get_search(&app, parent_query, 21).await;
    assert_eq!(parent.0, StatusCode::OK);
    let runtime = parent.1["runtimeVersion"].clone();
    let parent_revision = parent.1["revisionId"]
        .as_str()
        .expect("direct search issues a revision correlation id");
    assert!(parent_revision.starts_with("rev-001-"));
    assert!(parent.1["astFingerprint"]
        .as_str()
        .is_some_and(|fingerprint| fingerprint.starts_with("sha256:")));

    let candidate = post_revision(
        &app,
        revision_request(
            parent_revision,
            parent_query,
            1,
            runtime.clone(),
            "Make it ready to move",
        ),
        22,
    )
    .await;
    assert_eq!(candidate.0, StatusCode::OK);
    assert_eq!(candidate.1["outcome"], "candidate");
    assert_eq!(candidate.1["operation"], "refine");
    assert!(candidate.1["search"].is_object());
    assert_eq!(
        candidate.1["revisionId"], candidate.1["search"]["revisionId"],
        "nested ordinary search must expose the same server-issued revision"
    );

    let area_alternative = post_revision(
        &app,
        revision_request(
            parent_revision,
            parent_query,
            1,
            runtime.clone(),
            "Also consider Sarjapur",
        ),
        28,
    )
    .await;
    assert_eq!(area_alternative.0, StatusCode::OK);
    assert_eq!(area_alternative.1["outcome"], "candidate");
    assert_eq!(area_alternative.1["operation"], "expand");
    assert_eq!(area_alternative.1["activeBranchCount"], 2);
    assert_eq!(
        area_alternative.1["activeQuery"],
        "3BHK in Hoodi under 2.4 Cr or 3BHK in Sarjapur under 2.4 Cr"
    );

    let mut stale_runtime = runtime.clone();
    stale_runtime["searchEngineVersion"] = Value::String("stale-engine".to_string());
    let stale = post_revision(
        &app,
        revision_request(
            parent_revision,
            parent_query,
            1,
            stale_runtime,
            "Make it ready to move",
        ),
        23,
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert_eq!(stale.1["code"], "runtime_version_changed");

    let mismatch = post_revision(
        &app,
        revision_request(
            parent_revision,
            parent_query,
            2,
            runtime.clone(),
            "Make it ready to move",
        ),
        24,
    )
    .await;
    assert_eq!(mismatch.0, StatusCode::BAD_REQUEST);
    assert_eq!(mismatch.1["code"], "parent_branch_count_mismatch");

    let spoofed = post_revision(
        &app,
        revision_request(
            "rev-000-caller-controlled",
            parent_query,
            1,
            runtime.clone(),
            "Make it ready to move",
        ),
        25,
    )
    .await;
    assert_eq!(spoofed.0, StatusCode::BAD_REQUEST);
    assert_eq!(spoofed.1["code"], "invalid_parent_revision");

    let empty = post_revision(
        &app,
        revision_request(parent_revision, parent_query, 1, runtime.clone(), "   "),
        26,
    )
    .await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST);
    assert_eq!(empty.1["code"], "invalid_revision_request");

    let clarification = post_revision(
        &app,
        revision_request(parent_revision, parent_query, 1, runtime, "Make it closer"),
        27,
    )
    .await;
    assert_eq!(clarification.0, StatusCode::OK);
    assert_eq!(clarification.1["outcome"], "requireClarification");
    assert!(clarification.1.get("search").is_none());
    assert_eq!(clarification.1["activeQuery"], parent_query);

    let multi_parent_query = "3BHK in Hoodi under 2.4 Cr or 2BHK in Sarjapur under 1.8 Cr";
    let multi_parent = get_search(&app, multi_parent_query, 29).await;
    let ambiguous_area = post_revision(
        &app,
        revision_request(
            multi_parent.1["revisionId"].as_str().unwrap(),
            multi_parent_query,
            2,
            multi_parent.1["runtimeVersion"].clone(),
            "Also consider Hoodi",
        ),
        30,
    )
    .await;
    assert_eq!(ambiguous_area.0, StatusCode::OK);
    assert_eq!(ambiguous_area.1["outcome"], "requireClarification");
    assert!(ambiguous_area.1.get("search").is_none());
    assert_eq!(ambiguous_area.1["activeQuery"], multi_parent_query);
}

#[tokio::test]
async fn equivalent_direct_and_revised_searches_share_ast_results_and_proofs() {
    let app = test_app().await;
    let parent_query = "3BHK in Hoodi under 2.4 Cr";
    let equivalent_query = "3 bedrooms in Hoodi costing no more than 2.4 crore";
    let parent = get_search(&app, parent_query, 31).await;
    let direct = get_search(&app, equivalent_query, 32).await;
    assert_eq!(parent.0, StatusCode::OK);
    assert_eq!(direct.0, StatusCode::OK);

    let revised = post_revision(
        &app,
        revision_request(
            parent.1["revisionId"].as_str().unwrap(),
            parent_query,
            1,
            parent.1["runtimeVersion"].clone(),
            equivalent_query,
        ),
        33,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK);
    assert_eq!(revised.1["operation"], "rephrase");
    assert_eq!(revised.1["outcome"], "candidate");
    let search = &revised.1["search"];
    assert_eq!(search["astFingerprint"], direct.1["astFingerprint"]);
    assert_eq!(search["orderedResultIds"], direct.1["orderedResultIds"]);
    assert_eq!(search["resultSets"], direct.1["resultSets"]);
}

#[tokio::test]
async fn ninth_branch_requires_checkpoint_without_executing_a_candidate() {
    let app = test_app().await;
    let parent_query = "2BHK under 1Cr or 2BHK under 1.1Cr or 2BHK under 1.2Cr or 2BHK under 1.3Cr or 2BHK under 1.4Cr or 2BHK under 1.5Cr or 2BHK under 1.6Cr or 2BHK under 1.7Cr";
    let parent = get_search(&app, parent_query, 41).await;
    assert_eq!(parent.0, StatusCode::OK);
    let checkpoint = post_revision(
        &app,
        revision_request(
            parent.1["revisionId"].as_str().unwrap(),
            parent_query,
            8,
            parent.1["runtimeVersion"].clone(),
            "Also consider Sarjapur",
        ),
        42,
    )
    .await;
    assert_eq!(checkpoint.0, StatusCode::OK, "response={}", checkpoint.1);
    assert_eq!(checkpoint.1["outcome"], "requireCheckpoint");
    assert!(checkpoint.1.get("search").is_none());
    assert_eq!(checkpoint.1["activeBranchCount"], 8);
}

async fn test_app() -> Router {
    let root = tempdir().expect("temporary API fixture root").keep();
    let lake = LakeStore::local(root.join("lake")).expect("temporary lake");
    let bundle = Arc::new(test_bundle(&root));
    let properties = vec![test_property()];
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    let runtime = SearchRuntimeSnapshot::new(
        bundle.clone(),
        properties.clone(),
        Vec::new(),
        Vec::new(),
        search_index.clone(),
    );
    let (search_event_tx, _search_event_rx) = mpsc::channel(8);
    let state = Arc::new(AppState {
        execution: ExecutionLanes::current(),
        search_runtime: ArcSwap::from_pointee(runtime),
        search_cache: SearchResponseCache::new(8),
        property_catalog_cache: tokio::sync::Mutex::new(None),
        search_event_tx,
        search_log_dropped_count: AtomicU64::new(0),
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
        serving_bundle: RwLock::new(Some(bundle)),
        recommendation_cache: RwLock::new(HashMap::new()),
        areas: RwLock::new(Vec::new()),
        societies: RwLock::new(Vec::new()),
        discovery_config: backend::discovery::load_discovery_config(),
        map_overlays: Arc::new(backend::routes::map_overlays::CityMapOverlays::default()),
        knowledge: Arc::new(RwLock::new(KnowledgeGraph::new())),
        project_root: root,
        process_started_at: Utc::now(),
        interest_counter: AtomicU64::new(0),
        interest_write_lock: tokio::sync::Mutex::new(()),
        asset_run_active: AtomicBool::new(false),
    });
    build_app_router_with_lake(state, lake)
}

fn test_bundle(root: &std::path::Path) -> LoadedServingBundle {
    let entities = vec![
        serving_entity("area:hoodi", "area", "Hoodi"),
        serving_entity("area:sarjapur", "area", "Sarjapur"),
        serving_entity("area:cell:hoodi", "area", "Internal search cell"),
        serving_entity("area:cell:sarjapur", "area", "Internal search cell"),
        serving_entity("society:fixture-home", "society", "Fixture Home"),
    ];
    let facts = vec![
        topology_fact(
            "area:cell:hoodi",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.70, 77.72)),
        ),
        topology_fact(
            "area:cell:sarjapur",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.77, 77.79)),
        ),
        topology_fact(
            "society:fixture-home",
            "geo.latitude",
            FactValue::Numeric(12.975),
        ),
        topology_fact(
            "society:fixture-home",
            "geo.longitude",
            FactValue::Numeric(77.715),
        ),
    ];
    let edges = vec![
        topology_edge(
            "society:fixture-home",
            "in_market_locality",
            "area:hoodi",
            &facts[2],
        ),
        topology_edge(
            "society:fixture-home",
            "occupies_geo_cell",
            "area:cell:hoodi",
            &facts[2],
        ),
        topology_edge(
            "area:hoodi",
            "covers_geo_cell",
            "area:cell:hoodi",
            &facts[0],
        ),
        topology_edge(
            "area:sarjapur",
            "covers_geo_cell",
            "area:cell:sarjapur",
            &facts[1],
        ),
    ];
    let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    let recall_dir = root.join("tantivy");
    let recall_index = TantivyRecallIndex::build_in_dir(&recall_dir, &entities, &facts, &[])
        .expect("fixture recall index");

    LoadedServingBundle {
        manifest: ServingBundleManifest {
            bundle_version: "issue-118-revision-api-fixture".to_string(),
            format_version: 1,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            entity_count: entities.len() as u64,
            entity_alias_count: 0,
            fact_count: facts.len() as u64,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: edges.len() as u64,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: Default::default(),
            entity_parquet_key: "entities.parquet".to_string(),
            entity_alias_parquet_key: None,
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: None,
            edge_parquet_key: Some("edges.parquet".to_string()),
            quarantine_report_key: None,
            schema_key: "schema.json".to_string(),
            trust_policy_key: "trust.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entity_alias_index: ServingEntityAliasIndex::default(),
        graph_index: GraphIndex::from_serving_edges(&edges),
        geo_index: GeoSearchIndex::from_serving_bundle(&entities, &fact_index),
        spatial_index: SpatialServingIndex::from_serving_bundle_with_edges(
            &entities,
            &fact_index,
            &edges,
        ),
        search_capabilities: SearchCapabilityIndex::from_bundle(&entities, &fact_index),
        rera_evidence_index: ReraEvidenceIndex::default(),
        recall_index,
        fact_index,
        entities,
        edges,
        cache_dir: recall_dir,
    }
}

fn serving_entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
    ServingEntityRecord {
        entity_id: entity_id.to_string(),
        entity_type: entity_type.to_string(),
        name: name.to_string(),
        root_source: Some("revision_api_contract".to_string()),
        searchable_text: name.to_string(),
    }
}

fn topology_fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
    let learned_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let source_url = Some("https://example.test/openstreetmap".to_string());
    let source_type = if matches!(fact_key, "geo.latitude" | "geo.longitude") {
        "Google"
    } else {
        "OpenStreetMap"
    };
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: match &value {
            FactValue::Numeric(_) | FactValue::Score { .. } => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
        }
        .to_string(),
        value_text: None,
        value,
        confidence: 0.9,
        source_type: source_type.to_string(),
        source_url: source_url.clone(),
        model: None,
        skill_id: Some("search_revision_api_contract".to_string()),
        learned_at,
        observation: Some(
            SourceObservation::new(
                source_type,
                if matches!(fact_key, "geo.latitude" | "geo.longitude") {
                    format!("{entity_id}:coordinates")
                } else {
                    format!("{entity_id}:{fact_key}")
                },
                entity_id,
                learned_at,
                source_url,
                vec!["asset:search-revision-api-contract/v1".to_string()],
            )
            .expect("revision topology observation"),
        ),
    }
}

fn topology_edge(
    from: &str,
    relation: &str,
    to: &str,
    evidence_fact: &ServingFactRecord,
) -> ServingEdgeRecord {
    let evidence = EvidenceRef::for_observation(
        "issue-118-revision-api-fixture",
        evidence_fact
            .observation
            .as_ref()
            .expect("revision topology edge evidence"),
    );
    ServingEdgeRecord {
        from_entity_id: from.to_string(),
        edge_type: relation.to_string(),
        to_entity_id: to.to_string(),
        confidence: 0.9,
        source_type: "OpenStreetMap".to_string(),
        derivation: Some(
            DerivedEvidence::new(
                "issue-118-revision-api-fixture",
                from,
                Some(to.to_string()),
                relation,
                "controlled_topology",
                Some(1.0),
                Some("boolean".to_string()),
                "search-revision-api-contract-v1",
                0.9,
                vec![evidence],
            )
            .expect("revision topology derivation"),
        ),
    }
}

fn square_geometry(min_lon: f64, max_lon: f64) -> String {
    format!(
        "{{\"type\":\"Polygon\",\"coordinates\":[[[{min_lon},12.96],[{max_lon},12.96],[{max_lon},12.99],[{min_lon},12.99],[{min_lon},12.96]]]}}"
    )
}

fn test_property() -> Property {
    Property {
        id: "fixture-home-3bhk".to_string(),
        title: "Fixture Home".to_string(),
        area: "Hoodi".to_string(),
        area_id: "hoodi".to_string(),
        city: "Bengaluru".to_string(),
        society_id: "fixture-home".to_string(),
        builder_name: "Fixture Builder".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk: 3,
        price: 23_000_000,
        price_min: None,
        price_max: None,
        price_per_sqft: 12_000,
        carpet_area_sqft: 1_200,
        super_builtup_sqft: 1_550,
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: "Ready to Move".to_string(),
        metro_distance_mins: 8,
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
        greenery_score: Some(0.6),
        open_space_score: Some(0.6),
        resale_strength_score: Some(0.7),
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: "Revision API contract fixture".to_string(),
        transparency_tags: Vec::new(),
        source_reference: "revision_api_contract".to_string(),
    }
}

fn revision_request(
    parent_revision_id: &str,
    parent_query: &str,
    parent_branch_count: usize,
    runtime: Value,
    utterance: &str,
) -> Value {
    json!({
        "parentRevisionId": parent_revision_id,
        "parentQuery": parent_query,
        "parentBranchCount": parent_branch_count,
        "expectedRuntimeVersion": runtime,
        "utterance": utterance,
        "clientIdempotencyKey": "contract-turn"
    })
}

async fn get_search(app: &Router, query: &str, peer: u8) -> (StatusCode, Value) {
    let encoded = query.replace(' ', "%20");
    send(
        app,
        Method::GET,
        &format!("/api/search?q={encoded}"),
        Body::empty(),
        peer,
    )
    .await
}

async fn post_revision(app: &Router, payload: Value, peer: u8) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/api/search/revisions",
        Body::from(payload.to_string()),
        peer,
    )
    .await
}

async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    body: Body,
    peer: u8,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, peer], 41000))))
        .body(body)
        .expect("contract request is valid");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("revision route responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("response body is readable");
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "response is JSON: {error}; body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}
