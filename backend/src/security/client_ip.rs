use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

use axum::http::Request;
use tower_governor::key_extractor::{KeyExtractor, PeerIpKeyExtractor};
use tower_governor::GovernorError;

const TRUSTED_PROXY_IPS_ENV: &str = "OPENESTATES_TRUSTED_PROXY_IPS";

#[derive(Clone, Debug)]
pub(super) struct ClientIpKeyExtractor {
    trusted_proxy_ips: Arc<HashSet<IpAddr>>,
}

impl ClientIpKeyExtractor {
    pub(super) fn from_env() -> Self {
        let trusted_proxy_ips = std::env::var(TRUSTED_PROXY_IPS_ENV)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<IpAddr>().unwrap_or_else(|_| {
                    panic!("invalid IP address in {TRUSTED_PROXY_IPS_ENV}: {value}")
                })
            })
            .collect();
        Self {
            trusted_proxy_ips: Arc::new(trusted_proxy_ips),
        }
    }

    #[cfg(test)]
    pub(super) fn new(trusted_proxy_ips: impl IntoIterator<Item = IpAddr>) -> Self {
        Self {
            trusted_proxy_ips: Arc::new(trusted_proxy_ips.into_iter().collect()),
        }
    }
}

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, request: &Request<T>) -> Result<Self::Key, GovernorError> {
        let peer_ip = PeerIpKeyExtractor.extract(request)?;
        if self.trusted_proxy_ips.contains(&peer_ip) {
            // Read from the right because a correctly configured one-hop proxy
            // either overwrites X-Forwarded-For or appends the address it saw.
            // A client-supplied leading value must never become the rate key.
            Ok(request
                .headers()
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    value
                        .split(',')
                        .rev()
                        .map(str::trim)
                        .find_map(|candidate| candidate.parse::<IpAddr>().ok())
                })
                .unwrap_or(peer_ip))
        } else {
            Ok(peer_ip)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::body::Body;
    use axum::extract::ConnectInfo;

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
    fn forwarded_address_is_ignored_by_default() {
        let request = request_with_peer([127, 0, 0, 1], "203.0.113.7");
        let address = ClientIpKeyExtractor::new([])
            .extract(&request)
            .expect("peer address should be available");

        assert_eq!(address, IpAddr::from([127, 0, 0, 1]));
    }

    #[test]
    fn forwarded_address_requires_explicit_trust() {
        let request = request_with_peer([127, 0, 0, 1], "203.0.113.7");
        let address = ClientIpKeyExtractor::new([IpAddr::from([127, 0, 0, 1])])
            .extract(&request)
            .expect("forwarded address should be available");

        assert_eq!(address, IpAddr::from([203, 0, 113, 7]));
    }

    #[test]
    fn trusted_proxy_uses_the_appended_address_not_a_forged_leader() {
        let request = request_with_peer([127, 0, 0, 1], "198.51.100.9, 203.0.113.7");
        let address = ClientIpKeyExtractor::new([IpAddr::from([127, 0, 0, 1])])
            .extract(&request)
            .expect("forwarded address should be available");

        assert_eq!(address, IpAddr::from([203, 0, 113, 7]));
    }
}
