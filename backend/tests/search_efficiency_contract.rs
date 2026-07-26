use std::collections::HashMap;
use std::time::{Duration, Instant};

use backend::models::Property;
use backend::search::intent::parse_intent;
use backend::search::{HashSemanticEmbedder, SearchIndex, SemanticSearchIndex, TextSearch};

const MATCHING_PROPERTIES: usize = 12;
const DISTRACTORS_PER_BUCKET: usize = 800;
const MAX_RECALL_CANDIDATE_RATIO: f64 = 0.01;
const MAX_INDEXED_SEARCH_DURATION: Duration = Duration::from_millis(750);
const MAX_SEMANTIC_SCAN_DURATION: Duration = Duration::from_millis(250);
const MAX_SEMANTIC_SEARCH_PIPELINE_DURATION: Duration = Duration::from_millis(900);

#[test]
fn indexed_search_prunes_large_mock_corpus_before_ranking() {
    let properties = mock_property_corpus();
    let society_names = society_names(&properties);
    let index = SearchIndex::build(&properties);
    let query = "3bhk whitefield under 2cr";
    let intent = parse_intent(query);

    assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    assert_eq!(intent.bhk, Some(3));
    assert_eq!(intent.budget_max, Some(20_000_000));

    let recall_ids = index.recall_ids(query, &intent);
    let recall_ratio = recall_ids.len() as f64 / properties.len() as f64;

    assert_eq!(
        recall_ids.len(),
        MATCHING_PROPERTIES,
        "recall should pass only properties matching area, BHK, and budget"
    );
    assert!(
        recall_ratio <= MAX_RECALL_CANDIDATE_RATIO,
        "recall ratio {recall_ratio:.4} should stay under {MAX_RECALL_CANDIDATE_RATIO:.4}"
    );

    let started = Instant::now();
    let results = TextSearch::search_with_index_and_intent_and_sellers(
        &properties,
        Some(&index),
        &society_names,
        &[],
        query,
        &intent,
        None,
        &[],
    );
    let elapsed = started.elapsed();

    assert_eq!(results.len(), MATCHING_PROPERTIES);
    assert!(
        results.iter().all(|result| {
            result.card.area == "Whitefield"
                && result.card.bhk == 3
                && result.card.price <= 20_000_000
        }),
        "indexed search returned a result outside the structured query constraints"
    );
    assert!(
        elapsed <= MAX_INDEXED_SEARCH_DURATION,
        "indexed search took {elapsed:?} for {} properties and {} recalled candidates",
        properties.len(),
        recall_ids.len()
    );
}

#[test]
fn unsupported_inventory_query_short_circuits_large_mock_corpus() {
    let properties = mock_property_corpus();
    let society_names = society_names(&properties);
    let index = SearchIndex::build(&properties);
    let query = "plot or villa style calm layout near Bagalur metro";
    let intent = parse_intent(query);

    assert_eq!(
        intent.unsupported_inventory_types,
        vec!["plot".to_string(), "villa".to_string()],
        "plot/villa asks should be explicit unsupported inventory gaps"
    );

    let started = Instant::now();
    let results = TextSearch::search_with_index_and_intent_and_sellers(
        &properties,
        Some(&index),
        &society_names,
        &[],
        query,
        &intent,
        None,
        &[],
    );
    let elapsed = started.elapsed();

    assert!(
        results.is_empty(),
        "unsupported inventory should not return apartment results"
    );
    assert!(
        elapsed <= Duration::from_millis(50),
        "unsupported inventory should short-circuit cheaply, took {elapsed:?}"
    );
}

#[test]
fn semantic_recall_scans_large_mock_corpus_under_budget() {
    let properties = mock_property_corpus();
    let embedder = HashSemanticEmbedder::default();
    let semantic_index = SemanticSearchIndex::from_properties(&properties, &embedder);

    assert_eq!(semantic_index.len(), properties.len());

    let started = Instant::now();
    let hits = semantic_index.search("peaceful home for parents near hospital", &embedder, 128);
    let elapsed = started.elapsed();

    assert!(!hits.is_empty());
    assert!(
        elapsed <= MAX_SEMANTIC_SCAN_DURATION,
        "semantic exact scan took {elapsed:?} for {} documents",
        semantic_index.len()
    );
}

#[test]
fn semantic_recall_bridges_buyer_language_without_claiming_proof() {
    let properties = vec![
        property_with_description(
            "senior-healthcare-fit".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Senior friendly apartment with quiet low-noise blocks, family safety, and quick hospital access.",
        ),
        property_with_description(
            "amenity-fit".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Clubhouse, pool, gym, and active weekend sports programming for residents.",
        ),
        property_with_description(
            "commute-fit".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Office commute convenience with tech park access and metro connectivity.",
        ),
    ];
    let society_names = society_names(&properties);
    let index = SearchIndex::build(&properties);
    let embedder = HashSemanticEmbedder::default();
    let semantic_index = SemanticSearchIndex::from_properties(&properties, &embedder);
    let query = "peaceful home for parents";
    let intent = parse_intent(query);

    assert_eq!(
        index.recall_ids(query, &intent).len(),
        properties.len(),
        "lexical recall should be broad when buyer language uses synonyms"
    );

    let semantic_hits = semantic_index.search(query, &embedder, 8);
    let semantic_scores = index.property_scores_for_semantic_hits(&semantic_hits);
    let semantic_candidate_ids = semantic_scores.keys().cloned().collect::<Vec<_>>();

    assert!(
        semantic_scores
            .get("senior-healthcare-fit")
            .is_some_and(|score| *score > 0.0),
        "semantic recall should map parent/peaceful language to senior, quiet, and hospital text: {semantic_scores:?}"
    );

    let results = TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
        &properties,
        Some(&index),
        Some(&semantic_candidate_ids),
        Some(&semantic_scores),
        None,
        None,
        &society_names,
        &[],
        query,
        &intent,
        None,
        &[],
    );

    assert_eq!(results[0].card.id, "senior-healthcare-fit");
    assert!(
        results[0].semantic_score.is_some(),
        "semantic fit should be visible as metadata on the ranked result"
    );
    let explanation = results[0]
        .match_explanation
        .as_ref()
        .expect("buyer-language preferences should produce coverage metadata");
    assert!(
        explanation.preference_coverage.iter().any(|coverage| {
            coverage.preference == "quiet neighborhood" && coverage.status == "no_data"
        }),
        "semantic recall must not turn a soft vector hit into evidence proof: {:?}",
        explanation.preference_coverage
    );
}

#[test]
fn semantic_plus_text_search_pipeline_stays_under_latency_budget() {
    let properties = mock_property_corpus();
    let society_names = society_names(&properties);
    let index = SearchIndex::build(&properties);
    let embedder = HashSemanticEmbedder::default();
    let semantic_index = SemanticSearchIndex::from_properties(&properties, &embedder);

    for query in [
        "3BHK Whitefield under 2Cr",
        "near metro low traffic",
        "peaceful home for parents near hospital",
    ] {
        let intent = parse_intent(query);
        let started = Instant::now();
        let semantic_hits = semantic_index.search(query, &embedder, 128);
        let semantic_scores = index.property_scores_for_semantic_hits(&semantic_hits);
        let semantic_candidate_ids = semantic_scores.keys().cloned().collect::<Vec<_>>();
        let results = TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
            &properties,
            Some(&index),
            Some(&semantic_candidate_ids),
            Some(&semantic_scores),
            None,
            None,
            &society_names,
            &[],
            query,
            &intent,
            None,
            &[],
        );
        let elapsed = started.elapsed();

        assert!(
            !results.is_empty(),
            "semantic pipeline returned no results for {query}"
        );
        assert!(
            elapsed <= MAX_SEMANTIC_SEARCH_PIPELINE_DURATION,
            "semantic+text search pipeline took {elapsed:?} for {query:?} over {} properties",
            properties.len()
        );
    }
}

fn mock_property_corpus() -> Vec<Property> {
    let mut properties = Vec::new();

    for i in 0..MATCHING_PROPERTIES {
        let mut matched = property_with_description(
            format!("match-whitefield-3bhk-{i}"),
            "Whitefield",
            3,
            18_000_000,
            "Whitefield apartment with metro connectivity, low traffic access, quiet blocks, and family-friendly healthcare reach.",
        );
        matched.traffic_score = Some(0.1);
        properties.push(matched);
    }

    for i in 0..DISTRACTORS_PER_BUCKET {
        properties.push(property(
            format!("area-distractor-sarjapur-{i}"),
            "Sarjapur Road",
            3,
            18_000_000,
        ));
        properties.push(property(
            format!("bhk-distractor-whitefield-2bhk-{i}"),
            "Whitefield",
            2,
            18_000_000,
        ));
        properties.push(property(
            format!("budget-distractor-whitefield-3bhk-{i}"),
            "Whitefield",
            3,
            25_000_000,
        ));
        properties.push(property(
            format!("all-distractor-koramangala-{i}"),
            "Koramangala",
            4,
            32_000_000,
        ));
    }

    properties
}

fn society_names(properties: &[Property]) -> HashMap<String, String> {
    properties
        .iter()
        .map(|property| {
            (
                property.society_id.clone(),
                format!("{} Society", property.society_id),
            )
        })
        .collect()
}

fn property(id: String, area: &str, bhk: u32, price: u64) -> Property {
    property_with_description(
        id,
        area,
        bhk,
        price,
        "Generated mock property for search efficiency contract.",
    )
}

fn property_with_description(
    id: String,
    area: &str,
    bhk: u32,
    price: u64,
    description_summary: &str,
) -> Property {
    Property {
        id: id.clone(),
        title: format!("{bhk} BHK efficiency test home in {area}"),
        area: area.to_string(),
        area_id: area.to_lowercase().replace(' ', "-"),
        city: "Bengaluru".to_string(),
        society_id: id,
        builder_name: "Efficiency Builder".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk,
        price,
        price_per_sqft: 12_000,
        carpet_area_sqft: 1_200,
        super_builtup_sqft: 1_550,
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: "Ready to Move".to_string(),
        metro_distance_mins: 8,
        maintenance_cost_monthly: 6_000,
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
        description_summary: description_summary.to_string(),
        transparency_tags: Vec::new(),
        source_reference: "search-efficiency-contract".to_string(),
        seller_id: None,
    }
}
