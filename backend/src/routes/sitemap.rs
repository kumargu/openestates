use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;
use url::Url;

pub const SITE_URL_ENV: &str = "OPENESTATES_SITE_URL";
const DEVELOPMENT_SITE_URL: &str = "http://127.0.0.1:5173";

fn normalize_site_origin(value: &str) -> Result<String, String> {
    let parsed = Url::parse(value.trim())
        .map_err(|error| format!("{SITE_URL_ENV} must be an absolute URL: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{SITE_URL_ENV} must use http or https"));
    }
    if parsed.host_str().is_none() {
        return Err(format!("{SITE_URL_ENV} must include a host"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{SITE_URL_ENV} must not contain credentials"));
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(format!(
            "{SITE_URL_ENV} must be an origin without a path, query, or fragment"
        ));
    }
    Ok(parsed.origin().ascii_serialization())
}

pub fn configured_site_origin() -> Result<String, String> {
    let configured = std::env::var(SITE_URL_ENV).unwrap_or_else(|_| DEVELOPMENT_SITE_URL.into());
    normalize_site_origin(&configured)
}

/// GET /api/sitemap.xml — dynamic XML sitemap for SEO.
pub async fn sitemap_xml(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let base_url = configured_site_origin().unwrap_or_else(|error| panic!("{error}"));

    let mut urls = Vec::new();

    // Static pages
    urls.push(format!(
        "  <url><loc>{}/</loc><changefreq>daily</changefreq><priority>1.0</priority></url>",
        base_url
    ));

    // Property pages
    {
        let properties = state.properties.read().await;
        for p in properties.iter() {
            urls.push(format!(
                "  <url><loc>{}/property/{}</loc><changefreq>weekly</changefreq><priority>0.7</priority></url>",
                base_url, p.id
            ));
        }
    }

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
{}
</urlset>"#,
        urls.join("\n")
    );

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    )
}

#[cfg(test)]
mod tests {
    use super::normalize_site_origin;

    #[test]
    fn normalizes_site_origin_without_a_trailing_slash() {
        assert_eq!(
            normalize_site_origin("https://80feet.app/").unwrap(),
            "https://80feet.app"
        );
    }

    #[test]
    fn rejects_site_urls_with_paths_or_credentials() {
        assert!(normalize_site_origin("https://80feet.app/explore").is_err());
        assert!(normalize_site_origin("https://user:secret@80feet.app").is_err());
    }
}
