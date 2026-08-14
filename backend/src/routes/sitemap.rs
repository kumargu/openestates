use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;

fn property_urls(properties: &[crate::models::Property], base_url: &str) -> Vec<String> {
    properties
        .iter()
        .filter(|property| property.is_eligible_for(crate::buyer_eligibility::DETAIL_SURFACE))
        .map(|property| {
            format!(
                "  <url><loc>{}/property/{}</loc><changefreq>weekly</changefreq><priority>0.7</priority></url>",
                base_url, property.id
            )
        })
        .collect()
}

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
        urls.extend(property_urls(&properties, base_url));
    }

    // Society pages
    let societies = state.societies.read().await;
    for s in societies.iter() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_excludes_known_incomplete_property_urls() {
        let mut eligible = property("ready-home");
        eligible.buyer_eligibility = crate::buyer_eligibility::evaluate_property(&eligible);
        let mut incomplete = eligible.clone();
        incomplete.id = "incomplete-home".to_string();
        incomplete.price = 0;
        incomplete.buyer_eligibility = crate::buyer_eligibility::evaluate_property(&incomplete);

        let urls = property_urls(&[eligible, incomplete], "https://openestates.in");
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("ready-home"));
        assert!(!urls[0].contains("incomplete-home"));
    }

    fn property(id: &str) -> crate::models::Property {
        crate::models::Property {
            id: id.to_string(),
            title: "Ready home".to_string(),
            area: "Whitefield".to_string(),
            area_id: "area-whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "ready-home".to_string(),
            builder_name: String::new(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 10_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_000,
            super_builtup_sqft: 1_200,
            floor: 0,
            total_floors: 0,
            facing: String::new(),
            possession_status: String::new(),
            status: Default::default(),
            buyer_eligibility: Default::default(),
            metro_distance_mins: 0,
            maintenance_cost_monthly: 0,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 0,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            media: Vec::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        }
    }
}
