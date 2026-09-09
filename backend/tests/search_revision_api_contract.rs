use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
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
use backend::search::geo::SpatialEntityIndex;
use backend::search::{decode_signed_search_context, SearchCapabilityIndex, SearchIndex};
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
async fn revision_contract_activates_and_preserves_without_server_journey_state() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 21).await;
    assert_eq!(parent.0, StatusCode::OK);
    assert!(parent.1["revision"]["stateToken"].is_string());
    assert!(parent.1["revision"]["intentBreakdown"].is_array());
    assert!(parent.1["revision"].get("context").is_none());

    let activated = post_revision(
        &app,
        revision_request(&parent.1, "Make it under 2.5Cr", "budget-refine"),
        22,
    )
    .await;
    assert_eq!(activated.0, StatusCode::OK, "response={}", activated.1);
    assert_eq!(activated.1["operation"], "refine");
    assert_eq!(activated.1["outcome"], "activate");
    assert_eq!(activated.1["catalogRebased"], false);
    assert!(activated.1["candidate"]["revision"]["stateToken"].is_string());

    let relative = post_revision(
        &app,
        revision_request(&parent.1, "Stretch budget by 20 lakh", "relative-budget"),
        25,
    )
    .await;
    assert_eq!(relative.0, StatusCode::OK, "response={}", relative.1);
    assert_eq!(relative.1["operation"], "correct");
    assert_eq!(relative.1["outcome"], "activate");
    assert!(relative.1["attemptedBreakdown"][0]["predicates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|predicate| {
            predicate["dimension"] == "price" && predicate["value"]["max"] == 26_000_000_u64
        }));

    let zero = post_revision(
        &app,
        revision_request(&parent.1, "Only 4BHK", "zero-result"),
        23,
    )
    .await;
    assert_eq!(zero.0, StatusCode::OK);
    assert_eq!(zero.1["outcome"], "preserveParent");
    assert!(zero.1.get("candidate").is_none());
    assert!(zero.1["attemptedBreakdown"][0]["predicates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|predicate| predicate["dimension"] == "bhk" && predicate["value"] == 4));

    let clarification = post_revision(
        &app,
        revision_request(&parent.1, "Make it closer", "clarify"),
        24,
    )
    .await;
    assert_eq!(clarification.1["outcome"], "clarificationRequired");
    assert!(clarification.1.get("candidate").is_none());
}

#[tokio::test]
async fn compact_tokens_are_authenticated_and_bind_parent_results() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 31).await;
    let token = parent.1["revision"]["stateToken"].as_str().unwrap();
    assert!(token.len() < 64 * 1024);
    let decoded = decode_signed_search_context(token).unwrap();
    let payload = serde_json::to_value(decoded).unwrap();
    assert!(payload.get("intentAst").is_some());
    for forbidden in [
        "plan",
        "orderedResultIds",
        "evidence",
        "topology",
        "resultRows",
    ] {
        assert!(
            !payload.to_string().contains(forbidden),
            "payload={payload}"
        );
    }

    let mut corrupt = token.to_string();
    let replacement = if corrupt.ends_with('0') { "1" } else { "0" };
    corrupt.replace_range(corrupt.len() - 1.., replacement);
    let corrupt_response = post_revision(
        &app,
        json!({
            "parentToken": corrupt,
            "parentResultIds": parent.1["orderedResultIds"],
            "utterance": "Make it under 2.5Cr",
            "clientMutationId": "corrupt"
        }),
        32,
    )
    .await;
    assert_eq!(corrupt_response.0, StatusCode::BAD_REQUEST);
    assert_eq!(corrupt_response.1["code"], "invalid_parent_token");

    let mut mismatched = revision_request(&parent.1, "Make it under 2.5Cr", "mismatch");
    mismatched["parentResultIds"] = json!(["property:not-the-parent"]);
    let mismatched = post_revision(&app, mismatched, 33).await;
    assert_eq!(mismatched.0, StatusCode::CONFLICT);
    assert_eq!(mismatched.1["code"], "parent_results_mismatch");
}

#[tokio::test]
async fn duplicate_submissions_are_deterministic_and_conflicts_are_rejected() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 41).await;
    let request = revision_request(&parent.1, "Make it under 2.5Cr", "same-mutation");
    let (first, second) = tokio::join!(
        post_revision(&app, request.clone(), 42),
        post_revision(&app, request, 43),
    );
    assert_eq!(first, second);
    assert_eq!(first.1["outcome"], "activate");

    let conflict = post_revision(
        &app,
        revision_request(&parent.1, "Only 4BHK", "same-mutation"),
        44,
    )
    .await;
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    assert_eq!(conflict.1["code"], "client_mutation_id_conflict");
}

#[tokio::test]
async fn expansion_limits_and_selected_property_consequences_are_explicit() {
    let app = test_app().await;
    let selected_parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 51).await;
    let selected = selected_parent.1["orderedResultIds"][0].as_str().unwrap();
    let mut exclusion = revision_request(&selected_parent.1, "Only 4BHK", "exclude-selected");
    exclusion["selectedPropertyId"] = json!(selected);
    let exclusion = post_revision(&app, exclusion, 52).await;
    assert_eq!(exclusion.1["outcome"], "preserveParent");
    assert_eq!(exclusion.1["selectedPropertyConsequence"], "excluded");

    let parent_query = "2BHK under 1Cr or 2BHK under 1.1Cr or 2BHK under 1.2Cr or 2BHK under 1.3Cr or 2BHK under 1.4Cr or 2BHK under 1.5Cr or 2BHK under 1.6Cr or 2BHK under 1.7Cr";
    let parent = get_search(&app, parent_query, 53).await;
    let ninth = post_revision(
        &app,
        revision_request(&parent.1, "Also consider Sarjapur", "ninth-branch"),
        54,
    )
    .await;
    assert_eq!(ninth.0, StatusCode::OK);
    assert_eq!(ninth.1["outcome"], "limitReached");
    assert!(ninth.1.get("candidate").is_none());
}

#[tokio::test]
async fn revision_rebinds_portable_intent_after_catalog_generation_changes() {
    let (app, state) = test_app_with_state().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 61).await;
    assert_eq!(parent.0, StatusCode::OK);

    let next_root = tempdir().expect("next catalog fixture root").keep();
    let mut next_bundle = test_bundle(&next_root);
    next_bundle.manifest.bundle_version = "issue-118-revision-api-fixture-v2".to_string();
    let next_bundle = Arc::new(next_bundle);
    let properties = vec![test_property()];
    let search_index = SearchIndex::build_with_serving_graph(
        &properties,
        &next_bundle.entities,
        &next_bundle.edges,
    );
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            next_bundle,
            properties,
            Vec::new(),
            Vec::new(),
            search_index,
        )));

    let revised = post_revision(
        &app,
        revision_request(&parent.1, "Make it under 2.5Cr", "catalog-rebase"),
        62,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    assert_eq!(revised.1["outcome"], "activate");
    assert_eq!(revised.1["catalogRebased"], true);
    assert_eq!(
        revised.1["candidate"]["runtimeVersion"]["servingBundleVersion"],
        "issue-118-revision-api-fixture-v2"
    );
}

async fn test_app() -> Router {
    test_app_with_state().await.0
}

async fn test_app_with_state() -> (Router, Arc<AppState>) {
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
        search_revision_caches: backend::state::SearchRevisionCaches::new(8, 8),
        property_catalog_cache: tokio::sync::Mutex::new(None),
        search_event_tx,
        search_log_dropped_count: AtomicU64::new(0),
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
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
    });
    (build_app_router_with_lake(state.clone(), lake), state)
}

fn test_bundle(root: &std::path::Path) -> LoadedServingBundle {
    let entities = vec![
        serving_entity("area:hoodi", "area", "Hoodi"),
        serving_entity("area:sarjapur", "area", "Sarjapur"),
        serving_entity("area:cell:hoodi", "area", "Internal search cell"),
        serving_entity("area:cell:sarjapur", "area", "Internal search cell"),
        serving_entity("society:fixture-home", "society", "Fixture Home"),
    ];
    let mut facts = vec![
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
    facts.push(topology_fact(
        "society:fixture-home",
        "controlled_inventory_option",
        FactValue::Text(json!({"bhk": 3, "price": 23_000_000, "area_sqft": 1_550}).to_string()),
    ));
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

    let graph_index =
        GraphIndex::from_serving_bundle(&entities, &edges, "issue-118-revision-api-fixture");
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
            entity_alias_parquet_key: "aliases.parquet".to_string(),
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: "rera.parquet".to_string(),
            edge_parquet_key: "edges.parquet".to_string(),
            quarantine_report_key: "quarantine.json".to_string(),
            schema_key: "schema.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entity_alias_index: ServingEntityAliasIndex::default(),
        graph_index,
        entity_index: SpatialEntityIndex::from_serving_bundle(&entities, &fact_index),
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
        visibility: Default::default(),
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

fn revision_request(parent: &Value, utterance: &str, key: &str) -> Value {
    json!({
        "parentToken": parent["revision"]["stateToken"],
        "parentResultIds": parent["orderedResultIds"],
        "utterance": utterance,
        "clientMutationId": key
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
    let value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) if status == StatusCode::UNPROCESSABLE_ENTITY => {
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(error) => panic!(
            "response is JSON: {error}; body={}",
            String::from_utf8_lossy(&bytes)
        ),
    };
    (status, value)
}
