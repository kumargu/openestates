use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use backend::graph::GraphIndex;
use backend::knowledge::FactValue;
use backend::models::{Property, Society};
use backend::search::geo::SpatialEntityIndex;
use backend::search::intent::parse_intent;
use backend::search::{
    CandidateEvaluationRequest, CandidateEvaluator, IntentAst, SearchEngine, SearchIndex,
    SearchResponse, SearchRuntimeVersion,
};
use backend::serving::{
    normalize_alias, DerivedEvidence, EvidenceRef, LoadedServingBundle, ReraEvidenceIndex,
    ServingBundleManifest, ServingEdgeRecord, ServingEntityAliasIndex, ServingEntityAliasRecord,
    ServingEntityRecord, ServingFactIndex, ServingFactRecord, SourceObservation,
    SpatialServingIndex, TantivyRecallIndex,
};
use backend::state::{
    CachedSearchOutput, RuntimeVersionKey, SearchCacheKey, SearchLogMessage, SearchResponseCache,
    SearchRuntimeSnapshot, SEARCH_ENGINE_VERSION,
};
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

mod search_support;
use search_support::{inventory_context, inventory_facts, inventory_options};

const MATCHING_PROPERTIES: usize = 12;
const DISTRACTORS_PER_BUCKET: usize = 800;
const MAX_RECALL_CANDIDATE_RATIO: f64 = 0.01;
const MAX_INDEXED_SEARCH_DURATION: Duration = Duration::from_millis(750);

fn inert_test_plan(query: &str, snapshot: &str) -> backend::search::CompiledSearchPlan {
    backend::search::CompiledSearchPlan::compile_for_snapshot(
        backend::search::IntentAst::from_text(query),
        snapshot,
        &[],
        &backend::graph::GraphIndex::default(),
        None,
        backend::search::GeoCellSearchPolicy {
            max_hops: 2,
            max_distance_km: 4.0,
        },
    )
}

#[test]
fn indexed_search_prunes_large_mock_corpus_before_ranking() {
    let properties = mock_property_corpus();
    let society_names = society_names(&properties);
    let index = SearchIndex::build(&properties);
    let query = "3bhk east bangalore under 2cr";
    let intent = parse_intent(query);

    assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
    assert_eq!(intent.bhk, Some(3));
    assert_eq!(intent.budget_max, Some(20_000_000));

    let recall_ids = index.recall_ids(&IntentAst::from_text(query));
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

    let inventory_options = inventory_options(&properties);
    let started = Instant::now();
    let compiled_query = IntentAst::from_text(query);
    let results = CandidateEvaluator::search(CandidateEvaluationRequest {
        properties: &properties,
        search_index: Some(&index),
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        query: &compiled_query.raw,
        intent: &compiled_query.intent,
        constraints: &compiled_query.constraints,
        evaluation: inventory_context(&inventory_options),
    });
    let elapsed = started.elapsed();

    assert_eq!(results.len(), MATCHING_PROPERTIES);
    assert!(
        results.iter().all(|result| {
            result.card.area == "East Bengaluru"
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
fn dangling_named_place_search_evaluates_the_full_hard_eligible_corpus() {
    const CORPUS_SIZE: usize = 10_000;
    const MAX_DURATION: Duration = Duration::from_secs(2);

    let mut properties = Vec::with_capacity(CORPUS_SIZE);
    let mut entities = Vec::with_capacity(CORPUS_SIZE + 1);
    let mut facts = Vec::with_capacity(CORPUS_SIZE * 2 + 3);
    entities.push(ServingEntityRecord {
        entity_id: "place:benchmark-tech-park".to_string(),
        entity_type: "place".to_string(),
        name: "Benchmark Tech Park".to_string(),
        root_source: Some("google".to_string()),
        visibility: Default::default(),
        searchable_text: "Benchmark Tech Park".to_string(),
    });
    facts.extend([
        serving_fact(
            "place:benchmark-tech-park",
            "geo.latitude",
            FactValue::Numeric(12.97),
        ),
        serving_fact(
            "place:benchmark-tech-park",
            "geo.longitude",
            FactValue::Numeric(77.59),
        ),
        serving_fact(
            "place:benchmark-tech-park",
            "place.category",
            FactValue::Text("tech_park".to_string()),
        ),
    ]);

    for index in 0..CORPUS_SIZE {
        let id = format!("scale-society-{index:05}");
        let mut property = property(id.clone(), "Bengaluru", 3, 18_000_000);
        if index % 4 == 0 {
            property.bhk = 2;
        } else if index % 4 == 1 {
            property.price = 25_000_000;
        }
        properties.push(property);
        let entity_id = format!("society:{id}");
        entities.push(ServingEntityRecord {
            entity_id: entity_id.clone(),
            entity_type: "society".to_string(),
            name: id,
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        });
        let offset = 0.01 + index as f64 * 0.0000001;
        facts.push(serving_fact(
            &entity_id,
            "geo.latitude",
            FactValue::Numeric(12.97 + offset),
        ));
        facts.push(serving_fact(
            &entity_id,
            "geo.longitude",
            FactValue::Numeric(77.59 + offset),
        ));
    }

    let bundle = loaded_bundle(entities, facts);
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);
    let started = Instant::now();
    let output = SearchEngine::new(&snapshot).search("3bhk near Benchmark Tech Park under 2cr");
    let elapsed = started.elapsed();

    assert!(!output.results.is_empty());
    assert!(output.compiled_plan.branches[0].geo_scope.is_bundle_wide());
    assert!(output.results.len() <= 32);
    assert!(output.eligible_result_count >= output.results.len());
    assert!(output
        .results
        .iter()
        .all(|result| result.card.bhk == 3 && result.card.price <= 20_000_000));
    assert!(
        output.diagnostics.recall.structured_count < CORPUS_SIZE,
        "hard eligibility should prune before spatial recall"
    );
    assert!(
        elapsed <= MAX_DURATION,
        "full named-place search took {elapsed:?} across {CORPUS_SIZE} properties: {:?}; recall: {:?}",
        output.diagnostics.layer_timings,
        output.diagnostics.recall,
    );
}

#[test]
fn named_area_recall_uses_evidenced_geo_cells_not_coordinates() {
    let properties = vec![
        property(
            "cell-whitefield-home".to_string(),
            "Brookefield",
            3,
            18_000_000,
        ),
        property(
            "coordinate-only-whitefield-home".to_string(),
            "Brookefield",
            3,
            18_000_000,
        ),
    ];
    let entities = vec![
        ServingEntityRecord {
            entity_id: "area:whitefield".to_string(),
            entity_type: "area".to_string(),
            name: "Whitefield".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Whitefield".to_string(),
        },
        ServingEntityRecord {
            entity_id: "area:cell:whitefield".to_string(),
            entity_type: "area".to_string(),
            name: "Internal search cell".to_string(),
            root_source: Some("openstreetmap".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        },
        ServingEntityRecord {
            entity_id: "society:cell-whitefield-home".to_string(),
            entity_type: "society".to_string(),
            name: "Cell Whitefield Home".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Cell Whitefield Home".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:coordinate-only-whitefield-home".to_string(),
            entity_type: "society".to_string(),
            name: "Coordinate Only Whitefield Home".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Coordinate Only Whitefield Home".to_string(),
        },
    ];
    let facts = vec![
        topology_fact(
            "area:cell:whitefield",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.74, 77.76)),
        ),
        topology_fact(
            "society:cell-whitefield-home",
            "geo.latitude",
            FactValue::Numeric(12.9750),
        ),
        topology_fact(
            "society:cell-whitefield-home",
            "geo.longitude",
            FactValue::Numeric(77.7550),
        ),
        topology_fact(
            "society:coordinate-only-whitefield-home",
            "geo.latitude",
            FactValue::Numeric(12.9750),
        ),
        topology_fact(
            "society:coordinate-only-whitefield-home",
            "geo.longitude",
            FactValue::Numeric(77.7550),
        ),
    ];
    let edges = vec![
        topology_edge(
            "area:whitefield",
            "covers_geo_cell",
            "area:cell:whitefield",
            &facts[0],
        ),
        topology_edge(
            "society:cell-whitefield-home",
            "occupies_geo_cell",
            "area:cell:whitefield",
            &facts[1],
        ),
        topology_edge(
            "society:cell-whitefield-home",
            "in_market_locality",
            "area:whitefield",
            &facts[1],
        ),
    ];
    let bundle = loaded_bundle_with_edges(entities, facts, edges);
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);

    let output = SearchEngine::new(&snapshot).search("3BHK in Whitefield under 2Cr");

    assert_eq!(output.eligible_result_count, 1);
    assert_eq!(output.results[0].card.id, "cell-whitefield-home");
    assert!(output.results[0].geography_match.is_some());
}

#[test]
fn dangling_society_scope_fails_closed_without_relaxing_the_hard_budget() {
    let properties = vec![
        property("godrej-splendour".to_string(), "Whitefield", 3, 17_000_000),
        property(
            "budget-alternative".to_string(),
            "Whitefield",
            3,
            13_000_000,
        ),
    ];
    let entities = vec![
        ServingEntityRecord {
            entity_id: "society:godrej-splendour".to_string(),
            entity_type: "society".to_string(),
            name: "Godrej Splendour".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Godrej Splendour".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:budget-alternative".to_string(),
            entity_type: "society".to_string(),
            name: "Budget Alternative".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Budget Alternative".to_string(),
        },
    ];
    let bundle = loaded_bundle(entities, Vec::new());
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);

    let output = SearchEngine::new(&snapshot).search("Godrej Splendour 3BHK under ₹1.4Cr");

    assert_eq!(output.eligible_result_count, 0);
    assert!(output.results.is_empty());
    assert!(output
        .results
        .iter()
        .all(|result| result.card.price <= 14_000_000));
}

#[test]
fn dangling_grouped_society_anchors_keep_bhk_and_budget_branches_paired() {
    let mut godrej_three = property("godrej-air-3bhk".to_string(), "Whitefield", 3, 18_000_000);
    godrej_three.society_id = "godrej-air".to_string();
    let mut godrej_four = property("godrej-air-4bhk".to_string(), "Whitefield", 4, 24_000_000);
    godrej_four.society_id = "godrej-air".to_string();
    let mut waterford_three = property(
        "prestige-waterford-3bhk".to_string(),
        "Whitefield",
        3,
        19_000_000,
    );
    waterford_three.society_id = "prestige-waterford".to_string();
    let mut waterford_four = property(
        "prestige-waterford-4bhk".to_string(),
        "Whitefield",
        4,
        25_000_000,
    );
    waterford_four.society_id = "prestige-waterford".to_string();
    let mut godrej_over_budget = property(
        "godrej-air-3bhk-over-budget".to_string(),
        "Whitefield",
        3,
        21_000_000,
    );
    godrej_over_budget.society_id = "godrej-air".to_string();
    let mut waterford_over_budget = property(
        "prestige-waterford-4bhk-over-budget".to_string(),
        "Whitefield",
        4,
        41_000_000,
    );
    waterford_over_budget.society_id = "prestige-waterford".to_string();
    let properties = vec![
        godrej_three,
        godrej_four,
        waterford_three,
        waterford_four,
        godrej_over_budget,
        waterford_over_budget,
    ];
    let entities = vec![
        ServingEntityRecord {
            entity_id: "society:godrej-air".to_string(),
            entity_type: "society".to_string(),
            name: "Godrej Air".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Godrej Air".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:prestige-waterford".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Waterford".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Prestige Waterford".to_string(),
        },
        ServingEntityRecord {
            entity_id: "area:cell:shared".to_string(),
            entity_type: "area".to_string(),
            name: "Internal search cell".to_string(),
            root_source: Some("openstreetmap".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        },
    ];
    let facts = vec![
        topology_fact(
            "area:cell:shared",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.74, 77.76)),
        ),
        topology_fact(
            "society:godrej-air",
            "geo.latitude",
            FactValue::Numeric(12.975),
        ),
        topology_fact(
            "society:godrej-air",
            "geo.longitude",
            FactValue::Numeric(77.750),
        ),
        topology_fact(
            "society:prestige-waterford",
            "geo.latitude",
            FactValue::Numeric(12.976),
        ),
        topology_fact(
            "society:prestige-waterford",
            "geo.longitude",
            FactValue::Numeric(77.751),
        ),
    ];
    let edges = vec![
        topology_edge(
            "society:godrej-air",
            "occupies_geo_cell",
            "area:cell:shared",
            &facts[1],
        ),
        topology_edge(
            "society:prestige-waterford",
            "occupies_geo_cell",
            "area:cell:shared",
            &facts[3],
        ),
    ];
    let bundle = loaded_bundle_with_edges(entities, facts, edges);
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);

    let output = SearchEngine::new(&snapshot)
        .search("Godrej Air 3BHK under ₹2Cr or Prestige Waterford 4BHK under ₹4Cr");

    assert_eq!(
        output
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "godrej-air-3bhk",
            "prestige-waterford-4bhk",
            "prestige-waterford-3bhk",
            "godrej-air-4bhk"
        ]
    );
    assert_eq!(output.eligible_result_count, 4);
    assert_eq!(output.result_sets.len(), 2);
    assert_eq!(
        output
            .result_sets
            .iter()
            .map(|set| set.branch_id.as_str())
            .collect::<Vec<_>>(),
        ["branch-1", "branch-2"]
    );
    assert_eq!(
        output.result_sets[0]
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["godrej-air-3bhk", "prestige-waterford-3bhk"]
    );
    assert_eq!(
        output.result_sets[1]
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["prestige-waterford-4bhk", "godrej-air-4bhk"]
    );
    assert!(output.result_sets[0].label.contains("3 BHK"));
    assert!(output.result_sets[1].label.contains("4 BHK"));
    assert!(output
        .result_sets
        .iter()
        .all(|set| !set.label.contains("Godrej Air") && !set.label.contains("Prestige Waterford")));
    assert!(output
        .result_sets
        .iter()
        .flat_map(|set| &set.results)
        .all(|result| result.match_tier == "exact" && result.tradeoff_label.is_none()));
}

#[test]
fn unique_partial_society_name_is_only_a_geographic_anchor() {
    let mut waterford_four = property(
        "prestige-waterford-4bhk".to_string(),
        "Whitefield",
        4,
        25_000_000,
    );
    waterford_four.society_id = "prestige-waterford".to_string();
    let mut lakeside_four = property(
        "prestige-lakeside-4bhk".to_string(),
        "Whitefield",
        4,
        24_000_000,
    );
    lakeside_four.society_id = "prestige-lakeside".to_string();
    let properties = vec![waterford_four, lakeside_four];
    let entities = vec![
        ServingEntityRecord {
            entity_id: "society:prestige-waterford".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Waterford".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Prestige Waterford".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:prestige-lakeside".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Lakeside Habitat".to_string(),
            root_source: Some("serving_bundle".to_string()),
            visibility: Default::default(),
            searchable_text: "Prestige Lakeside Habitat".to_string(),
        },
    ];
    let bundle = loaded_bundle_with_aliases(
        entities,
        Vec::new(),
        vec![ServingEntityAliasRecord {
            alias: "Waterford".to_string(),
            normalized_alias: normalize_alias("Waterford"),
            entity_id: "society:prestige-waterford".to_string(),
            entity_type: "society".to_string(),
            entity_name: "Prestige Waterford".to_string(),
            source: "builder_prefix".to_string(),
        }],
    );
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);

    let output = SearchEngine::new(&snapshot).search("Waterford 4BHK");

    assert_eq!(
        output
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec!["prestige-waterford-4bhk"],
        "eligible={}, resolved={:?}",
        output.eligible_result_count,
        output.diagnostics.resolved.entities,
    );
    assert_eq!(output.eligible_result_count, 1);
}

#[test]
fn tantivy_candidates_reach_branch_ranking_without_satisfying_hard_inventory() {
    let mut lexical = property("lexical-only".to_string(), "Whitefield", 2, 18_000_000);
    lexical.society_id = "lexical-only".to_string();
    let properties = vec![lexical];
    let entities = vec![ServingEntityRecord {
        entity_id: "society:lexical-only".to_string(),
        entity_type: "society".to_string(),
        name: "Lexical Only".to_string(),
        root_source: Some("serving_bundle".to_string()),
        visibility: Default::default(),
        searchable_text: "quiet 3BHK".to_string(),
    }];
    let bundle = loaded_bundle(entities.clone(), Vec::new());
    let search_index = SearchIndex::build_with_serving_entities(&properties, &entities);
    let snapshot = search_runtime_snapshot(bundle, &properties, search_index);

    let output = SearchEngine::new(&snapshot).search("quiet 3BHK");

    assert!(
        output.diagnostics.recall.tantivy_count > 0,
        "branch={:?}, recall={:?}",
        output.compiled_plan.branches[0].recall_query,
        output.diagnostics.recall
    );
    assert_eq!(output.diagnostics.recall.tantivy_branch_additions, 1);
    assert!(
        output.results.is_empty(),
        "lexical recall must not turn the durable 2BHK option into a hard 3BHK match"
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

    let inventory_options = HashMap::new();
    let started = Instant::now();
    let compiled_query = IntentAst::from_text(query);
    let results = CandidateEvaluator::search(CandidateEvaluationRequest {
        properties: &properties,
        search_index: Some(&index),
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        query: &compiled_query.raw,
        intent: &compiled_query.intent,
        constraints: &compiled_query.constraints,
        evaluation: inventory_context(&inventory_options),
    });
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
fn candidate_ranking_preserves_order_and_corpus_tiebreaks() {
    let properties = vec![
        property_with_description(
            "alpha".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Whitefield apartment",
        ),
        property_with_description(
            "bravo".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Whitefield apartment",
        ),
        property_with_description(
            "charlie".to_string(),
            "Whitefield",
            3,
            18_500_000,
            "Whitefield apartment",
        ),
    ];
    let society_names = society_names(&properties);
    let compiled_query = IntentAst::from_text("3bhk whitefield under 2cr");
    let inventory_options = inventory_options(&properties);

    let unrestricted = CandidateEvaluator::search(CandidateEvaluationRequest {
        properties: &properties,
        search_index: None,
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        query: &compiled_query.raw,
        intent: &compiled_query.intent,
        constraints: &compiled_query.constraints,
        evaluation: inventory_context(&inventory_options),
    });
    let restricted = CandidateEvaluator::search(CandidateEvaluationRequest {
        properties: &properties,
        search_index: None,
        extra_candidate_ids: None,
        candidate_property_indexes: Some(vec![2, 1, 2, 0]),
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        query: &compiled_query.raw,
        intent: &compiled_query.intent,
        constraints: &compiled_query.constraints,
        evaluation: inventory_context(&inventory_options),
    });

    assert_eq!(
        unrestricted
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "bravo", "charlie"],
        "baseline tie-break should use original corpus order"
    );
    assert_eq!(
        restricted
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "bravo", "charlie"],
        "candidate indexes must not let caller order or duplicates override final corpus tie-breaks"
    );
}

#[tokio::test]
async fn search_cache_key_changes_with_bundle_version() {
    let cache = SearchResponseCache::new(8);
    let key_v1 = SearchCacheKey::new("  3BHK   Whitefield  ", &runtime_key("bundle-v1"));
    let key_v2 = SearchCacheKey::new("3bhk whitefield", &runtime_key("bundle-v2"));

    cache
        .put(
            key_v1.clone(),
            CachedSearchOutput {
                response: Arc::new(empty_response("3bhk whitefield")),
                compiled_plan: Arc::new(inert_test_plan("3bhk whitefield", "bundle-v1")),
                log_messages: Vec::new(),
            },
        )
        .await;

    assert!(cache.get(&key_v1).await.is_some());
    assert!(
        cache.get(&key_v2).await.is_none(),
        "same normalized query under a new bundle/version key must miss"
    );
}

#[tokio::test]
async fn search_cache_hit_still_carries_log_metadata() {
    let cache = SearchResponseCache::new(8);
    let key = SearchCacheKey::new("3bhk whitefield", &runtime_key("bundle-v1"));
    let intent = parse_intent("3bhk whitefield");
    let event = backend::knowledge::SearchEvent::new("3bhk whitefield".to_string(), intent, 1);

    cache
        .put(
            key.clone(),
            CachedSearchOutput {
                response: Arc::new(empty_response("3bhk whitefield")),
                compiled_plan: Arc::new(inert_test_plan("3bhk whitefield", "bundle-v1")),
                log_messages: vec![SearchLogMessage::SearchEvent(event.clone())],
            },
        )
        .await;

    let cached = cache.get(&key).await.expect("cache should hit");
    assert_eq!(cached.response.query, "3bhk whitefield");
    assert_eq!(cached.log_messages.len(), 1);
    match &cached.log_messages[0] {
        SearchLogMessage::SearchEvent(cached_event) => {
            assert_eq!(cached_event.query, event.query);
            assert_eq!(cached_event.results_returned, event.results_returned);
        }
    }
}

#[tokio::test]
async fn search_event_queue_does_not_block_response_side_effect() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let intent = parse_intent("3bhk whitefield");
    let first = SearchLogMessage::SearchEvent(backend::knowledge::SearchEvent::new(
        "3bhk whitefield".to_string(),
        intent.clone(),
        1,
    ));
    let second = SearchLogMessage::SearchEvent(backend::knowledge::SearchEvent::new(
        "3bhk whitefield".to_string(),
        intent,
        1,
    ));

    tx.try_send(first)
        .expect("first side effect should enqueue");
    assert!(
        tx.try_send(second).is_err(),
        "full bounded queue should drop/fail the side effect without awaiting"
    );
    assert!(rx.try_recv().is_ok());
}

fn runtime_key(bundle_version: &str) -> RuntimeVersionKey {
    RuntimeVersionKey {
        serving_bundle_version: bundle_version.to_string(),
        scoring_policy_version: backend::scoring::scoring_policy().version,
        search_engine_version: SEARCH_ENGINE_VERSION.to_string(),
        semantic_contract_digest: backend::state::semantic_contract_digest().to_string(),
    }
}

fn empty_response(query: &str) -> SearchResponse {
    let version = runtime_key("test-bundle");
    SearchResponse {
        query: query.to_string(),
        revision_id: "rev-001-test".to_string(),
        revision: None,
        ast_fingerprint: "sha256:test".to_string(),
        result_sets: Vec::new(),
        ordered_result_ids: Vec::new(),
        total_matches: 0,
        runtime_version: SearchRuntimeVersion {
            serving_bundle_version: version.serving_bundle_version,
            scoring_policy_version: version.scoring_policy_version,
            search_engine_version: version.search_engine_version,
            semantic_contract_digest: version.semantic_contract_digest,
        },
        area_context: None,
        state: "no_matches".to_string(),
        search_guidance: None,
    }
}

fn serving_fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: match &value {
            FactValue::Numeric(_) | FactValue::Score { .. } => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
        }
        .to_string(),
        value_text: None,
        value,
        confidence: 1.0,
        source_type: "Google".to_string(),
        source_url: None,
        model: None,
        skill_id: Some("search_efficiency_contract".to_string()),
        learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        observation: None,
    }
}

fn topology_fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
    let mut fact = serving_fact(entity_id, fact_key, value);
    let source_type = if matches!(fact_key, "geo.latitude" | "geo.longitude") {
        "Google"
    } else {
        "OpenStreetMap"
    };
    fact.source_type = source_type.to_string();
    fact.source_url = Some("https://example.test/openstreetmap".to_string());
    fact.observation = Some(
        SourceObservation::new(
            source_type,
            if matches!(fact_key, "geo.latitude" | "geo.longitude") {
                format!("{entity_id}:coordinates")
            } else {
                format!("{entity_id}:{fact_key}")
            },
            entity_id,
            fact.learned_at,
            fact.source_url.clone(),
            vec!["asset:search-efficiency-contract/v1".to_string()],
        )
        .expect("topology fact observation"),
    );
    fact
}

fn topology_edge(
    from: &str,
    relation: &str,
    to: &str,
    evidence_fact: &ServingFactRecord,
) -> ServingEdgeRecord {
    let evidence = EvidenceRef::for_observation(
        "efficiency-contract",
        evidence_fact
            .observation
            .as_ref()
            .expect("topology edge evidence observation"),
    );
    ServingEdgeRecord {
        from_entity_id: from.to_string(),
        edge_type: relation.to_string(),
        to_entity_id: to.to_string(),
        confidence: 0.9,
        source_type: "OpenStreetMap".to_string(),
        derivation: Some(
            DerivedEvidence::new(
                "efficiency-contract",
                from,
                Some(to.to_string()),
                relation,
                "controlled_topology",
                Some(1.0),
                Some("boolean".to_string()),
                "search-efficiency-contract-v1",
                0.9,
                vec![evidence],
            )
            .expect("topology edge derivation"),
        ),
    }
}

fn square_geometry(min_lon: f64, max_lon: f64) -> String {
    format!(
        "{{\"type\":\"Polygon\",\"coordinates\":[[[{min_lon},12.96],[{max_lon},12.96],[{max_lon},12.99],[{min_lon},12.99],[{min_lon},12.96]]]}}"
    )
}

fn loaded_bundle(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
) -> LoadedServingBundle {
    loaded_bundle_core(entities, facts, Vec::new(), Vec::new())
}

fn loaded_bundle_with_edges(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    edges: Vec<ServingEdgeRecord>,
) -> LoadedServingBundle {
    loaded_bundle_core(entities, facts, Vec::new(), edges)
}

fn search_runtime_snapshot(
    mut bundle: LoadedServingBundle,
    properties: &[Property],
    search_index: SearchIndex,
) -> SearchRuntimeSnapshot {
    let mut facts = bundle
        .fact_index
        .rows()
        .flat_map(|(_, rows)| rows.facts.iter().cloned())
        .collect::<Vec<_>>();
    let metadata = bundle
        .fact_index
        .rows()
        .flat_map(|(_, rows)| rows.search_metadata.iter().cloned())
        .collect::<Vec<_>>();
    facts.extend(inventory_facts(properties));
    bundle.manifest.fact_count = facts.len() as u64;
    bundle.fact_index = ServingFactIndex::from_records(facts, metadata);
    SearchRuntimeSnapshot::new(
        Arc::new(bundle),
        properties.to_vec(),
        mock_societies(properties),
        Vec::new(),
        search_index,
    )
}

fn mock_societies(properties: &[Property]) -> Vec<Society> {
    properties
        .iter()
        .map(|property| Society {
            id: property.society_id.clone(),
            name: property.title.clone(),
            area: property.area.clone(),
            city: property.city.clone(),
            builder_name: property.builder_name.clone(),
            year_built: 0,
            total_units: 0,
            summary: String::new(),
            maintenance_sentiment: String::new(),
            livability_sentiment: String::new(),
            common_positives: Vec::new(),
            common_complaints: Vec::new(),
            review_summary: String::new(),
            google_reviews_url: None,
            future_google_place_name: String::new(),
            future_google_place_id: None,
            future_review_enrichment_status: String::new(),
        })
        .collect()
}

fn loaded_bundle_with_aliases(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    aliases: Vec<ServingEntityAliasRecord>,
) -> LoadedServingBundle {
    loaded_bundle_core(entities, facts, aliases, Vec::new())
}

fn loaded_bundle_core(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    aliases: Vec<ServingEntityAliasRecord>,
    edges: Vec<ServingEdgeRecord>,
) -> LoadedServingBundle {
    let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    let entity_alias_index = ServingEntityAliasIndex::from_records(aliases).unwrap();
    let temp_dir = tempdir().unwrap();
    let recall_index =
        TantivyRecallIndex::build_in_dir(temp_dir.path(), &entities, &facts, &[]).unwrap();
    let entity_index = SpatialEntityIndex::from_serving_bundle(&entities, &fact_index);
    let spatial_index =
        SpatialServingIndex::from_serving_bundle_with_edges(&entities, &fact_index, &edges);
    let mut graph_index = GraphIndex::from_serving_bundle(&entities, &edges, "efficiency-contract");
    graph_index.add_entity_aliases(&backend::serving::unique_society_aliases(&entities));
    LoadedServingBundle {
        manifest: ServingBundleManifest {
            bundle_version: "efficiency-contract".to_string(),
            format_version: 1,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            entity_count: entities.len() as u64,
            entity_alias_count: entity_alias_index.len() as u64,
            fact_count: facts.len() as u64,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: edges.len() as u64,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: Default::default(),
            entity_parquet_key: "entities.parquet".to_string(),
            entity_alias_parquet_key: "aliases.parquet".to_string(),
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: "rera.parquet".to_string(),
            edge_parquet_key: "edges.parquet".to_string(),
            quarantine_report_key: "quarantine.json".to_string(),
            schema_key: "schema.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entities,
        entity_alias_index,
        graph_index,
        edges,
        recall_index,
        fact_index,
        rera_evidence_index: ReraEvidenceIndex::default(),
        entity_index,
        spatial_index,
        search_capabilities: backend::search::SearchCapabilityIndex::default(),
        cache_dir: temp_dir.keep(),
    }
}

fn mock_property_corpus() -> Vec<Property> {
    let mut properties = Vec::new();

    for i in 0..MATCHING_PROPERTIES {
        let mut matched = property_with_description(
            format!("match-whitefield-3bhk-{i}"),
            "East Bengaluru",
            3,
            18_000_000,
            "East Bengaluru apartment with metro connectivity, low traffic access, quiet blocks, and family-friendly healthcare reach.",
        );
        matched.traffic_score = Some(0.1);
        properties.push(matched);
    }

    for i in 0..DISTRACTORS_PER_BUCKET {
        properties.push(property(
            format!("area-distractor-sarjapur-{i}"),
            "South Bengaluru",
            3,
            18_000_000,
        ));
        properties.push(property(
            format!("bhk-distractor-whitefield-2bhk-{i}"),
            "East Bengaluru",
            2,
            18_000_000,
        ));
        properties.push(property(
            format!("budget-distractor-whitefield-3bhk-{i}"),
            "East Bengaluru",
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
        price_min: None,
        price_max: None,
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
    }
}
