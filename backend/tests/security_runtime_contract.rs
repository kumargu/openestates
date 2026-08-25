use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use backend::api::build_app_router;
use backend::data_loader::load_app_state;
use tower::ServiceExt;

#[tokio::test]
async fn production_router_enforces_public_security_boundaries() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has a project root")
        .to_path_buf();
    let state = Arc::new(load_app_state(&project_root).await);
    let app = build_app_router(state, &project_root);

    for attempt in 0..5 {
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/interests",
                [192, 0, 2, 10],
                Body::from("{}"),
            ))
            .await
            .expect("interest route responds");
        assert_ne!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "interest attempt {attempt} was limited before the configured burst"
        );
    }

    let limited = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/interests",
            [192, 0, 2, 10],
            Body::from("{}"),
        ))
        .await
        .expect("interest route responds");
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers().contains_key(header::RETRY_AFTER));

    let oversized_body = serde_json::json!({
        "property_id": "missing-property",
        "buyer_name": "a".repeat(17 * 1024),
    })
    .to_string();
    let oversized = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/interests",
            [192, 0, 2, 11],
            Body::from(oversized_body),
        ))
        .await
        .expect("interest route responds");
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let long_query = format!("/api/search?q={}", "a".repeat(2 * 1024 + 1));
    let guarded = app
        .clone()
        .oneshot(request(
            Method::GET,
            &long_query,
            [192, 0, 2, 12],
            Body::empty(),
        ))
        .await
        .expect("search route responds");
    assert_eq!(guarded.status(), StatusCode::URI_TOO_LONG);

    let cors_request = Request::builder()
        .method(Method::GET)
        .uri("/api/health")
        .header(header::ORIGIN, "https://not-allowed.invalid")
        .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, 13], 41000))))
        .body(Body::empty())
        .expect("CORS request is valid");
    let cors_response = app
        .clone()
        .oneshot(cors_request)
        .await
        .expect("health route responds");
    assert_eq!(cors_response.status(), StatusCode::OK);
    assert!(!cors_response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));

    let admin_guard = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/admin/asset-runs",
            [192, 0, 2, 14],
            Body::from("{"),
        ))
        .await
        .expect("admin guard responds before JSON extraction");
    assert!(matches!(
        admin_guard.status(),
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::UNAUTHORIZED
    ));

    let not_found = app
        .oneshot(request(
            Method::GET,
            "/not-a-route",
            [192, 0, 2, 15],
            Body::empty(),
        ))
        .await
        .expect("application fallback responds");
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        not_found.headers().get(header::X_CONTENT_TYPE_OPTIONS),
        Some(&header::HeaderValue::from_static("nosniff"))
    );
}

fn request(method: Method, uri: &str, peer: [u8; 4], body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::from((peer, 41000))))
        .body(body)
        .expect("security contract request is valid")
}
