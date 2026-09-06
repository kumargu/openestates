use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use backend::api::build_app_router;
use backend::data_loader::load_app_state;
use serde_json::{json, Value};
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
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has a project root")
        .to_path_buf();
    build_app_router(Arc::new(load_app_state(&project_root).await), &project_root)
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
