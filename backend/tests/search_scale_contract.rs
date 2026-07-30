use std::collections::HashMap;
use std::time::{Duration, Instant};

use backend::models::Property;
use backend::search::intent::parse_intent;
use backend::search::{HashSemanticEmbedder, SearchIndex, SemanticSearchIndex, TextSearch};

#[test]
#[ignore = "synthetic local baseline; run with --ignored --nocapture"]
fn synthetic_search_scale_baseline() {
    let properties = synthetic_properties(10_000);
    let society_names = properties
        .iter()
        .map(|property| (property.society_id.clone(), property.title.clone()))
        .collect::<HashMap<_, _>>();
    let index = SearchIndex::build(&properties);
    let embedder = HashSemanticEmbedder::default();
    let semantic_index = SemanticSearchIndex::from_properties(&properties, &embedder);

    let structured_p95 = p95_duration((0..200).map(|_| {
        let intent = parse_intent("3bhk whitefield under 2cr");
        let started = Instant::now();
        let _ = TextSearch::search_with_index_and_intent(
            &properties,
            Some(&index),
            &society_names,
            &[],
            "3bhk whitefield under 2cr",
            &intent,
            None,
        );
        started.elapsed()
    }));

    let semantic_p95 = p95_duration((0..100).map(|_| {
        let query = "peaceful home for parents near hospital";
        let intent = parse_intent(query);
        let started = Instant::now();
        let hits = semantic_index.search(query, &embedder, 64);
        let scores = index.property_scores_for_semantic_hits(&hits);
        let candidate_ids = index.property_ids_for_semantic_hits(&hits);
        let _ = TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent(
            &properties,
            Some(&index),
            Some(&candidate_ids),
            Some(&scores),
            None,
            None,
            &society_names,
            &[],
            query,
            &intent,
            None,
        );
        started.elapsed()
    }));

    eprintln!("structured_p95={structured_p95:?} semantic_p95={semantic_p95:?}");
    assert!(structured_p95 < Duration::from_millis(50));
    assert!(semantic_p95 < Duration::from_millis(200));
}

fn p95_duration(values: impl Iterator<Item = Duration>) -> Duration {
    let mut values = values.collect::<Vec<_>>();
    values.sort();
    values[((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)]
}

fn synthetic_properties(count: usize) -> Vec<Property> {
    (0..count)
        .map(|i| Property {
            id: format!("property-{i}"),
            title: format!("Synthetic Whitefield {i}"),
            area: if i % 4 == 0 {
                "Whitefield".to_string()
            } else {
                "Sarjapur".to_string()
            },
            area_id: if i % 4 == 0 {
                "whitefield".to_string()
            } else {
                "sarjapur".to_string()
            },
            city: "Bengaluru".to_string(),
            society_id: format!("society-{i}"),
            builder_name: "Synthetic Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: if i % 4 == 0 { 3 } else { 2 },
            price: if i % 4 == 0 { 18_000_000 } else { 26_000_000 },
            price_per_sqft: 12_000,
            carpet_area_sqft: 1200,
            super_builtup_sqft: 1550,
            floor: 8,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready".to_string(),
            metro_distance_mins: 8,
            maintenance_cost_monthly: 6000,
            society_quality_score: Some(0.7),
            builder_quality_score: Some(0.7),
            document_completeness_score: Some(0.8),
            litigation_risk: Some(0.1),
            noise_score: Some(0.2),
            sunlight_score: Some(0.7),
            airport_noise_score: Some(0.1),
            waterlogging_risk_score: Some(0.2),
            traffic_score: Some(0.4),
            days_on_market: 20,
            greenery_score: Some(0.6),
            open_space_score: Some(0.6),
            resale_strength_score: Some(0.7),
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: "quiet senior friendly residence near hospital and metro"
                .to_string(),
            transparency_tags: Vec::new(),
            source_reference: "search-scale-contract".to_string(),
        })
        .collect()
}
