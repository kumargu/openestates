use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware;
use axum::routing::MethodRouter;
use axum::Router;
use governor::middleware::NoOpMiddleware;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_governor::GovernorLayer;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use super::admin_auth::require_admin_request;
use super::client_ip::ClientIpKeyExtractor;
use super::guards::{
    reject_oversized_request_target, reject_oversized_search_query, shed_overloaded_requests,
    RequestAdmission,
};
use super::security_tuning;
use super::ExecutionLanes;

const ALLOWED_ORIGINS_ENV: &str = "OPENESTATES_ALLOWED_ORIGINS";
const DEFAULT_ALLOWED_ORIGINS: &str = "http://localhost:5173,http://127.0.0.1:5173";

type RateLimitConfig = GovernorConfig<ClientIpKeyExtractor, NoOpMiddleware>;

pub struct SecurityPolicy {
    read_rate_limit: Arc<RateLimitConfig>,
    search_rate_limit: Arc<RateLimitConfig>,
    batch_rate_limit: Arc<RateLimitConfig>,
    interest_rate_limit: Arc<RateLimitConfig>,
    admin_rate_limit: Arc<RateLimitConfig>,
    global_admission: RequestAdmission,
    search_admission: RequestAdmission,
    read_admission: RequestAdmission,
    catalog_admission: RequestAdmission,
}

impl SecurityPolicy {
    pub fn from_env(execution: &ExecutionLanes) -> Self {
        let tuning = security_tuning();
        let client_ip = ClientIpKeyExtractor::from_env();
        let read_rate_limit = rate_limit_rule(client_ip.clone(), &tuning.rate_limits.read);
        let search_rate_limit = rate_limit_rule(client_ip.clone(), &tuning.rate_limits.search);
        let batch_rate_limit = rate_limit_rule(client_ip.clone(), &tuning.rate_limits.batch);
        let interest_rate_limit = rate_limit_rule(client_ip.clone(), &tuning.rate_limits.interest);
        let admin_rate_limit = rate_limit_rule(client_ip, &tuning.rate_limits.admin);

        let governor_limiters = [
            read_rate_limit.limiter().clone(),
            search_rate_limit.limiter().clone(),
            batch_rate_limit.limiter().clone(),
            interest_rate_limit.limiter().clone(),
            admin_rate_limit.limiter().clone(),
        ];
        execution.spawn_internal(async move {
            let mut cleanup = tokio::time::interval(Duration::from_secs(60));
            loop {
                cleanup.tick().await;
                for limiter in &governor_limiters {
                    limiter.retain_recent();
                }
            }
        });

        Self {
            read_rate_limit: Arc::new(read_rate_limit),
            search_rate_limit: Arc::new(search_rate_limit),
            batch_rate_limit: Arc::new(batch_rate_limit),
            interest_rate_limit: Arc::new(interest_rate_limit),
            admin_rate_limit: Arc::new(admin_rate_limit),
            global_admission: RequestAdmission::new(tuning.requests.global_concurrency),
            search_admission: RequestAdmission::new(tuning.requests.search_concurrency),
            read_admission: RequestAdmission::new(tuning.requests.read_concurrency),
            catalog_admission: RequestAdmission::new(tuning.requests.catalog_concurrency),
        }
    }

    pub fn protect_public_reads<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(public_timeout_layer())
            .layer(GovernorLayer::new(self.read_rate_limit.clone()))
            .layer(middleware::from_fn_with_state(
                self.read_admission.clone(),
                shed_overloaded_requests,
            ))
    }

    pub fn protect_catalog<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(public_timeout_layer())
            .layer(GovernorLayer::new(self.read_rate_limit.clone()))
            .layer(middleware::from_fn_with_state(
                self.catalog_admission.clone(),
                shed_overloaded_requests,
            ))
    }

    pub fn protect_search<S>(&self, route: MethodRouter<S>) -> MethodRouter<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        route
            .layer::<_, std::convert::Infallible>(middleware::from_fn_with_state(
                self.search_admission.clone(),
                shed_overloaded_requests,
            ))
            .layer::<_, std::convert::Infallible>(public_timeout_layer())
            .layer::<_, std::convert::Infallible>(GovernorLayer::new(
                self.search_rate_limit.clone(),
            ))
            .layer(middleware::from_fn(reject_oversized_search_query))
    }

    pub fn protect_batch_reads<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(DefaultBodyLimit::max(
                security_tuning().requests.batch_body_bytes,
            ))
            .layer(public_timeout_layer())
            .layer(GovernorLayer::new(self.batch_rate_limit.clone()))
    }

    pub fn protect_interest_writes<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(DefaultBodyLimit::max(
                security_tuning().requests.interest_body_bytes,
            ))
            .layer(public_timeout_layer())
            .layer(GovernorLayer::new(self.interest_rate_limit.clone()))
    }

    pub fn protect_admin<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(DefaultBodyLimit::max(
                security_tuning().requests.admin_body_bytes,
            ))
            .layer(middleware::from_fn(require_admin_request))
            .layer(GovernorLayer::new(self.admin_rate_limit.clone()))
    }

    pub fn protect_application<S>(&self, routes: Router<S>) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        routes
            .layer(cors_layer())
            .layer(SetResponseHeaderLayer::if_not_present(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ))
            .layer(middleware::from_fn_with_state(
                self.global_admission.clone(),
                shed_overloaded_requests,
            ))
            .layer(middleware::from_fn(reject_oversized_request_target))
    }
}

fn rate_limit(
    client_ip: ClientIpKeyExtractor,
    period: Duration,
    burst_size: u32,
) -> RateLimitConfig {
    GovernorConfigBuilder::default()
        .key_extractor(client_ip)
        .period(period)
        .burst_size(burst_size)
        .finish()
        .expect("security rate limit must have a non-zero period and burst")
}

fn rate_limit_rule(
    client_ip: ClientIpKeyExtractor,
    rule: &super::config::RateLimitRule,
) -> RateLimitConfig {
    rate_limit(client_ip, Duration::from_millis(rule.period_ms), rule.burst)
}

fn public_timeout_layer() -> TimeoutLayer {
    request_timeout_layer(Duration::from_millis(
        security_tuning().requests.public_timeout_ms,
    ))
}

fn request_timeout_layer(duration: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::GATEWAY_TIMEOUT, duration)
}

fn cors_layer() -> CorsLayer {
    let configured =
        std::env::var(ALLOWED_ORIGINS_ENV).unwrap_or_else(|_| DEFAULT_ALLOWED_ORIGINS.to_string());
    cors_layer_for(&configured)
}

fn cors_layer_for(configured: &str) -> CorsLayer {
    let origins = configured
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .unwrap_or_else(|_| panic!("invalid origin in {ALLOWED_ORIGINS_ENV}: {origin}"))
        })
        .collect::<Vec<_>>();
    assert!(
        !origins.is_empty(),
        "{ALLOWED_ORIGINS_ENV} must contain at least one origin"
    );

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::{ConnectInfo, Json};
    use axum::http::Request;
    use axum::routing::{get, post};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    fn request_with_peer(peer: [u8; 4], forwarded_for: &str) -> Request<Body> {
        Request::builder()
            .uri("/api/search?q=home")
            .header("x-forwarded-for", forwarded_for)
            .extension(ConnectInfo(SocketAddr::from((peer, 41000))))
            .body(Body::empty())
            .expect("security test request is valid")
    }

    #[test]
    fn search_burst_is_lenient_but_bounded() {
        let config = rate_limit(
            ClientIpKeyExtractor::new([]),
            Duration::from_millis(security_tuning().rate_limits.search.period_ms),
            security_tuning().rate_limits.search.burst,
        );
        let client = IpAddr::from([192, 0, 2, 10]);

        for _ in 0..security_tuning().rate_limits.search.burst {
            assert!(config.limiter().check_key(&client).is_ok());
        }
        assert!(config.limiter().check_key(&client).is_err());
    }

    #[tokio::test]
    async fn rate_limit_is_per_peer_and_ignores_forged_forwarding_headers() {
        let governor = rate_limit(ClientIpKeyExtractor::new([]), Duration::from_secs(60), 2);
        let app = Router::new()
            .route("/api/search", get(|| async { "ok" }))
            .layer(GovernorLayer::new(governor));

        for forwarded_for in ["203.0.113.1", "203.0.113.2"] {
            let response = app
                .clone()
                .oneshot(request_with_peer([192, 0, 2, 1], forwarded_for))
                .await
                .expect("limited route responds");
            assert_eq!(response.status(), StatusCode::OK);
        }

        let limited = app
            .clone()
            .oneshot(request_with_peer([192, 0, 2, 1], "203.0.113.3"))
            .await
            .expect("limited route responds");
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(limited.headers().contains_key(header::RETRY_AFTER));

        let other_peer = app
            .oneshot(request_with_peer([192, 0, 2, 2], "203.0.113.3"))
            .await
            .expect("limited route responds");
        assert_eq!(other_peer.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn oversized_json_body_is_rejected() {
        async fn accept_json(Json(_payload): Json<Value>) -> StatusCode {
            StatusCode::NO_CONTENT
        }

        let app = Router::new()
            .route("/", post(accept_json))
            .layer(DefaultBodyLimit::max(1024));
        let body = serde_json::json!({ "value": "a".repeat(2048) }).to_string();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("oversized-body request is valid"),
            )
            .await
            .expect("body limit responds");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn request_timeout_returns_gateway_timeout() {
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    "late"
                }),
            )
            .layer(request_timeout_layer(Duration::from_millis(5)));
        let response = app
            .oneshot(Request::new(Body::empty()))
            .await
            .expect("timeout layer responds");

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn cors_allows_configured_origin_and_omits_unconfigured_origin() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(cors_layer_for("https://openestates.example"));

        let allowed = app
            .clone()
            .oneshot(
                Request::builder()
                    .header(header::ORIGIN, "https://openestates.example")
                    .body(Body::empty())
                    .expect("allowed-origin request is valid"),
            )
            .await
            .expect("CORS route responds");
        assert_eq!(
            allowed.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("https://openestates.example"))
        );

        let denied = app
            .oneshot(
                Request::builder()
                    .header(header::ORIGIN, "https://scraper.example")
                    .body(Body::empty())
                    .expect("denied-origin request is valid"),
            )
            .await
            .expect("CORS route responds");
        assert!(!denied
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }
}
