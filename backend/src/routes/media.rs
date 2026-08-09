//! Buyer-facing media delivery from the OpenEstates lake.
//!
//! This route deliberately sits behind the `LakeStore` abstraction instead of
//! reading a frontend `public/` directory. In the normal local-first setup the
//! lake is a persistent directory on the application server. The same keys can
//! later be backed by S3 without changing property facts, serving bundles, API
//! payloads, or browser URLs. S3 is therefore an optional durability/replication
//! backend, not a requirement for serving images efficiently.
//!
//! Promoted property images use content-addressed keys of the form
//! `media/images/sha256/<prefix>/<sha256>.<extension>`. The hash makes a URL
//! immutable: new bytes must produce a new URL. That lets this route return a
//! one-year immutable cache policy and a strong ETag safely, so a browser
//! normally downloads each image only once. The response body is streamed from
//! the configured lake store; image bytes are not loaded while assembling the
//! property/search JSON response and are not buffered in this handler.
//!
//! Operational contract:
//! - Keep the local lake on a persistent volume and include it in backups.
//! - Never restore guessed `/societies/...` paths or Git-packaged image trees.
//! - Serving bundles must carry the promoted `/media/...` URL explicitly.
//! - Preserve streaming, immutable caching, content length, MIME type, and ETag
//!   behavior when adding another storage backend or a reverse proxy.
//! - A CDN may be added later, but correctness and no-recrawl durability come
//!   from preserving the lake objects and bundle manifests.

use axum::body::Body;
use axum::extract::{Extension, Path};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;

use crate::lake::{LakeKey, LakeStore};

const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const DEFAULT_CACHE_CONTROL: &str = "public, max-age=3600";

pub async fn get_media(
    Extension(lake): Extension<LakeStore>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (key, content_hash) = match media_key(&path) {
        Ok(value) => value,
        Err(message) => return text_response(StatusCode::BAD_REQUEST, message),
    };
    let cache_control = if content_hash.is_some() {
        IMMUTABLE_CACHE_CONTROL
    } else {
        DEFAULT_CACHE_CONTROL
    };
    let expected_etag = content_hash.map(|hash| format!("\"{hash}\""));
    if expected_etag.as_deref().is_some_and(|etag| {
        headers
            .get(IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| etag_matches(value, etag))
    }) {
        return response_with_headers(
            StatusCode::NOT_MODIFIED,
            Body::empty(),
            cache_control,
            expected_etag.as_deref(),
            None,
            None,
        );
    }

    let object = match lake.get_stream(&key).await {
        Ok(object) => object,
        Err(error) if error.is_not_found() => {
            return text_response(StatusCode::NOT_FOUND, "media object not found")
        }
        Err(error) => {
            eprintln!("WARN: failed to stream lake media {key}: {error}");
            return text_response(StatusCode::BAD_GATEWAY, "media store unavailable");
        }
    };
    let object_etag = object.e_tag.clone();
    let etag = expected_etag.as_deref().or(object_etag.as_deref());
    response_with_headers(
        StatusCode::OK,
        Body::from_stream(object.stream),
        cache_control,
        etag,
        Some(content_type(&path)),
        Some(object.size_bytes),
    )
}

fn media_key(path: &str) -> Result<(LakeKey, Option<&str>), &'static str> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("invalid media path");
    }
    let content_hash = content_addressed_hash(path)?;
    let key = LakeKey::new(format!("media/{path}")).map_err(|_| "invalid media path")?;
    Ok((key, content_hash))
}

fn content_addressed_hash(path: &str) -> Result<Option<&str>, &'static str> {
    if !path.starts_with("images/sha256/") {
        return Ok(None);
    }
    let mut parts = path.split('/');
    if parts.next() != Some("images") || parts.next() != Some("sha256") {
        return Err("invalid content-addressed media path");
    }
    let prefix = parts
        .next()
        .filter(|value| value.len() == 2 && value.bytes().all(is_lower_hex))
        .ok_or("invalid content-addressed media path")?;
    let filename = parts.next().ok_or("invalid content-addressed media path")?;
    if parts.next().is_some() {
        return Err("invalid content-addressed media path");
    }
    let (hash, extension) = filename
        .rsplit_once('.')
        .ok_or("invalid content-addressed media path")?;
    if hash.len() != 64
        || !hash.bytes().all(is_lower_hex)
        || &hash[..2] != prefix
        || !matches!(extension, "jpg" | "png" | "webp" | "gif" | "avif")
    {
        return Err("invalid content-addressed media path");
    }
    Ok(Some(hash))
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn etag_matches(header: &str, expected: &str) -> bool {
    header
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate == expected)
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn response_with_headers(
    status: StatusCode,
    body: Body,
    cache_control: &'static str,
    etag: Option<&str>,
    content_type: Option<&'static str>,
    content_length: Option<usize>,
) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
    if let Some(etag) = etag.and_then(|value| HeaderValue::from_str(value).ok()) {
        response.headers_mut().insert(ETAG, etag);
    }
    if let Some(content_type) = content_type {
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    }
    if let Some(content_length) = content_length {
        if let Ok(value) = HeaderValue::from_str(&content_length.to_string()) {
            response.headers_mut().insert(CONTENT_LENGTH, value);
        }
    }
    response
}

fn text_response(status: StatusCode, message: &'static str) -> Response {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::{Extension, Router};
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn accepts_only_canonical_content_addressed_paths() {
        let hash = "a".repeat(64);
        let valid = format!("images/sha256/aa/{hash}.webp");
        assert_eq!(content_addressed_hash(&valid).unwrap(), Some(hash.as_str()));
        assert!(media_key("images/sha256/aa/../secret.webp").is_err());
        assert!(media_key("images/sha256/ab/aaaaaaaa.webp").is_err());
        assert!(media_key("../secret").is_err());
    }

    #[tokio::test]
    async fn streams_immutable_media_with_browser_and_cdn_cache_headers() {
        let lake = LakeStore::from_object_store(std::sync::Arc::new(
            object_store::memory::InMemory::new(),
        ));
        let bytes = b"\x89PNG\r\n\x1a\nfast-image".to_vec();
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = format!("images/sha256/{}/{}.png", &hash[..2], hash);
        lake.put_bytes(
            &LakeKey::new(format!("media/{path}")).unwrap(),
            bytes.clone(),
        )
        .await
        .unwrap();
        let app = Router::new()
            .route("/media/{*path}", get(get_media))
            .layer(Extension(lake));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/media/{path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "image/png");
        assert_eq!(response.headers()[CACHE_CONTROL], IMMUTABLE_CACHE_CONTROL);
        assert_eq!(response.headers()[ETAG], format!("\"{hash}\""));
        assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), bytes);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/media/{path}"))
                    .header(IF_NONE_MATCH, format!("\"{hash}\""))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }
}
