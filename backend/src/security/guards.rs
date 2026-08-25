use axum::extract::Request;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::security_tuning;

#[derive(Clone)]
pub(super) struct RequestAdmission {
    slots: Arc<Semaphore>,
}

impl RequestAdmission {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(limit.max(1))),
        }
    }
}

pub(super) async fn shed_overloaded_requests(
    State(admission): State<RequestAdmission>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(permit) = admission.slots.clone().try_acquire_owned() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"server is busy; retry shortly"}"#,
        )
            .into_response();
    };
    let response = next.run(request).await;
    drop(permit);
    response
}

pub(super) async fn reject_oversized_request_target(request: Request, next: Next) -> Response {
    if request.uri().to_string().len() > security_tuning().requests.max_request_target_bytes {
        return (
            StatusCode::URI_TOO_LONG,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"request target is too long"}"#,
        )
            .into_response();
    }
    next.run(request).await
}

pub async fn reject_oversized_search_query(request: Request, next: Next) -> Response {
    if request
        .uri()
        .query()
        .is_some_and(|query| query.len() > security_tuning().requests.max_search_query_bytes)
    {
        return (
            StatusCode::URI_TOO_LONG,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"search query is too long"}"#,
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn oversized_search_query_is_rejected_before_the_handler() {
        let app = Router::new().route(
            "/api/search",
            get(|| async { "ok" }).layer(middleware::from_fn(reject_oversized_search_query)),
        );
        let uri = format!(
            "/api/search?q={}",
            "a".repeat(security_tuning().requests.max_search_query_bytes + 1)
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("long-query request is valid"),
            )
            .await
            .expect("query guard responds");

        assert_eq!(response.status(), StatusCode::URI_TOO_LONG);
    }

    #[tokio::test]
    async fn global_request_target_limit_rejects_non_search_amplification() {
        let app = Router::new()
            .route("/api/societies/search", get(|| async { "ok" }))
            .layer(middleware::from_fn(reject_oversized_request_target));
        let uri = format!(
            "/api/societies/search?q={}",
            "a".repeat(security_tuning().requests.max_request_target_bytes + 1)
        );
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::URI_TOO_LONG);
    }

    #[tokio::test]
    async fn request_admission_sheds_instead_of_queueing() {
        let admission = RequestAdmission::new(1);
        let held = admission.slots.clone().acquire_owned().await.unwrap();
        let app =
            Router::new()
                .route("/", get(|| async { "ok" }))
                .layer(middleware::from_fn_with_state(
                    admission,
                    shed_overloaded_requests,
                ));

        let response = app
            .oneshot(Request::new(Body::empty()))
            .await
            .expect("overload guard responds immediately");
        drop(held);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
