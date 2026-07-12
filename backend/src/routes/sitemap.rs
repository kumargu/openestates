use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;

/// GET /api/sitemap.xml — dynamic XML sitemap for SEO.
pub async fn sitemap_xml(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let base_url = "https://openestates.in";

    let mut urls = Vec::new();

    // Static pages
    urls.push(format!(
        "  <url><loc>{}/</loc><changefreq>daily</changefreq><priority>1.0</priority></url>",
        base_url
    ));
    urls.push(format!(
        "  <url><loc>{}/results</loc><changefreq>daily</changefreq><priority>0.8</priority></url>",
        base_url
    ));
    urls.push(format!(
        "  <url><loc>{}/societies</loc><changefreq>daily</changefreq><priority>0.8</priority></url>",
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

    // Society pages
    for s in &state.societies {
        urls.push(format!(
            "  <url><loc>{}/societies/{}</loc><changefreq>weekly</changefreq><priority>0.6</priority></url>",
            base_url, s.id
        ));
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
