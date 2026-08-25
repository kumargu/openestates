use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

type AdminAuthError = (StatusCode, Json<serde_json::Value>);

pub fn require_admin(headers: &HeaderMap) -> Result<(), AdminAuthError> {
    let expected = std::env::var("ADMIN_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty());
    require_admin_token(headers, expected.as_deref())
}

pub(super) async fn require_admin_request(request: Request<Body>, next: Next) -> Response {
    match require_admin(request.headers()) {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}

fn require_admin_token(headers: &HeaderMap, expected: Option<&str>) -> Result<(), AdminAuthError> {
    let expected = expected.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "admin access is not configured" })),
        )
    })?;
    let provided = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok());
    if provided == Some(expected) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or missing admin token" })),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_access_fails_closed_without_configuration() {
        let error = require_admin_token(&HeaderMap::new(), None).expect_err("admin must fail");
        assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn admin_access_requires_the_configured_token() {
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-token", "wrong".parse().unwrap());
        let error =
            require_admin_token(&headers, Some("expected")).expect_err("wrong token must fail");
        assert_eq!(error.0, StatusCode::UNAUTHORIZED);

        headers.insert("x-admin-token", "expected".parse().unwrap());
        assert!(require_admin_token(&headers, Some("expected")).is_ok());
    }
}
