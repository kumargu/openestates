use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use backend::graph::GraphIndex;
use backend::knowledge::FactValue;
use backend::models::Property;
use backend::search::geo::GeoSearchIndex;
use backend::search::intent::parse_intent;
use backend::search::{
    CompiledQuery, SearchEngine, SearchIndex, SearchResponse, TextSearch, TextSearchRequest,
};
use backend::serving::{
    normalize_alias, LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest,
    ServingEntityAliasIndex, ServingEntityAliasRecord, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, SpatialServingIndex, TantivyRecallIndex,
};
use backend::state::{
    CachedSearchOutput, RuntimeVersionKey, SearchCacheKey, SearchLogMessage, SearchResponseCache,
    SEARCH_ENGINE_VERSION,
};
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

const MATCHING_PROPERTIES: usize = 12;
const DISTRACTORS_PER_BUCKET: usize = 800;
const MAX_RECALL_CANDIDATE_RATIO: f64 = 0.01;
const MAX_INDEXED_SEARCH_DURATION: Duration = Duration::from_millis(750);

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

    let recall_ids = index.recall_ids(&CompiledQuery::from_text(query));
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
    let compiled_query = CompiledQuery::from_text(query);
    let results = TextSearch::search(TextSearchRequest {
        properties: &properties,
        search_index: Some(&index),
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        societies: &[],
        compiled_query: &compiled_query,
        graph: None,
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
fn named_place_search_uses_spatial_discovery_across_large_corpus() {
    const CORPUS_SIZE: usize = 10_000;
    const MAX_DURATION: Duration = Duration::from_millis(750);

    let mut properties = Vec::with_capacity(CORPUS_SIZE);
    let mut entities = Vec::with_capacity(CORPUS_SIZE + 1);
    let mut facts = Vec::with_capacity(CORPUS_SIZE * 2 + 3);
    entities.push(ServingEntityRecord {
        entity_id: "place:benchmark-tech-park".to_string(),
        entity_type: "place".to_string(),
        name: "Benchmark Tech Park".to_string(),
        root_source: Some("google".to_string()),
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
    let society_names = society_names(&properties);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let started = Instant::now();
    let output = SearchEngine {
        properties: &properties,
        search_index: &search_index,
        serving_bundle: Some(&bundle),
        society_names: &society_names,
        property_by_id: Some(&property_by_id),
        societies: &[],
        graph: None,
    }
    .search("3bhk near Benchmark Tech Park under 2cr");
    let elapsed = started.elapsed();

    assert!(!output.results.is_empty());
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
fn named_area_does_not_expand_to_nearby_areas() {
    let properties = vec![property(
        "nearby-whitefield-home".to_string(),
        "Brookefield",
        3,
        18_000_000,
    )];
    let entities = vec![
        ServingEntityRecord {
            entity_id: "area:whitefield".to_string(),
            entity_type: "area".to_string(),
            name: "Whitefield".to_string(),
            root_source: Some("serving_bundle".to_string()),
            searchable_text: "Whitefield".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:nearby-whitefield-home".to_string(),
            entity_type: "society".to_string(),
            name: "Nearby Whitefield Home".to_string(),
            root_source: Some("serving_bundle".to_string()),
            searchable_text: "Nearby Whitefield Home".to_string(),
        },
    ];
    let facts = vec![
        serving_fact(
            "area:whitefield",
            "geo.latitude",
            FactValue::Numeric(12.9698),
        ),
        serving_fact(
            "area:whitefield",
            "geo.longitude",
            FactValue::Numeric(77.7499),
        ),
        serving_fact(
            "society:nearby-whitefield-home",
            "geo.latitude",
            FactValue::Numeric(12.9750),
        ),
        serving_fact(
            "society:nearby-whitefield-home",
            "geo.longitude",
            FactValue::Numeric(77.7550),
        ),
    ];
    let bundle = loaded_bundle(entities, facts);
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let society_names = society_names(&properties);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect::<HashMap<_, _>>();

    let output = SearchEngine {
        properties: &properties,
        search_index: &search_index,
        serving_bundle: Some(&bundle),
        society_names: &society_names,
        property_by_id: Some(&property_by_id),
        societies: &[],
        graph: None,
    }
    .search("3BHK in Whitefield under 2Cr");

    assert_eq!(output.eligible_result_count, 0);
    assert!(output.results.is_empty());
}

#[test]
fn named_project_miss_does_not_relax_the_hard_budget() {
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
            searchable_text: "Godrej Splendour".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:budget-alternative".to_string(),
            entity_type: "society".to_string(),
            name: "Budget Alternative".to_string(),
            root_source: Some("serving_bundle".to_string()),
            searchable_text: "Budget Alternative".to_string(),
        },
    ];
    let bundle = loaded_bundle(entities, Vec::new());
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let names = society_names(&properties);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect::<HashMap<_, _>>();

    let output = SearchEngine {
        properties: &properties,
        search_index: &search_index,
        serving_bundle: Some(&bundle),
        society_names: &names,
        property_by_id: Some(&property_by_id),
        societies: &[],
        graph: None,
    }
    .search("Godrej Splendour 3BHK under ₹1.4Cr");

    assert_eq!(output.eligible_result_count, 0);
    assert!(output.results.is_empty());
    assert!(output.result_sets.is_empty());
}

#[test]
fn grouped_named_projects_keep_bhk_and_budget_branches_paired() {
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
            searchable_text: "Godrej Air".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:prestige-waterford".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Waterford".to_string(),
            root_source: Some("serving_bundle".to_string()),
            searchable_text: "Prestige Waterford".to_string(),
        },
    ];
    let bundle = loaded_bundle(entities, Vec::new());
    let search_index = SearchIndex::build_with_serving_entities(&properties, &bundle.entities);
    let names = society_names(&properties);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect::<HashMap<_, _>>();

    let output = SearchEngine {
        properties: &properties,
        search_index: &search_index,
        serving_bundle: Some(&bundle),
        society_names: &names,
        property_by_id: Some(&property_by_id),
        societies: &[],
        graph: None,
    }
    .search("Godrej Air 3BHK under ₹2Cr or Prestige Waterford 4BHK under ₹4Cr");

    assert_eq!(
        output
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        vec!["godrej-air-3bhk", "prestige-waterford-4bhk"]
    );
    assert_eq!(output.eligible_result_count, 2);
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
        output
            .result_sets
            .iter()
            .map(|set| set.results[0].card.id.as_str())
            .collect::<Vec<_>>(),
        ["godrej-air-3bhk", "prestige-waterford-4bhk"]
    );
    assert!(output.result_sets[0].label.contains("Godrej Air"));
    assert!(output.result_sets[0].label.contains("3 BHK"));
    assert!(output.result_sets[1].label.contains("Prestige Waterford"));
    assert!(output.result_sets[1].label.contains("4 BHK"));
    assert!(output
        .result_sets
        .iter()
        .flat_map(|set| &set.results)
        .all(|result| result.match_tier == "exact" && result.tradeoff_label.is_none()));
}

#[test]
fn unique_partial_society_name_is_a_hard_constraint() {
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
            searchable_text: "Prestige Waterford".to_string(),
        },
        ServingEntityRecord {
            entity_id: "society:prestige-lakeside".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Lakeside Habitat".to_string(),
            root_source: Some("serving_bundle".to_string()),
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
    let names = society_names(&properties);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect::<HashMap<_, _>>();

    let output = SearchEngine {
        properties: &properties,
        search_index: &search_index,
        serving_bundle: Some(&bundle),
        society_names: &names,
        property_by_id: Some(&property_by_id),
        societies: &[],
        graph: None,
    }
    .search("Waterford 4BHK");

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
    let compiled_query = CompiledQuery::from_text(query);
    let results = TextSearch::search(TextSearchRequest {
        properties: &properties,
        search_index: Some(&index),
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        societies: &[],
        compiled_query: &compiled_query,
        graph: None,
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
    let compiled_query = CompiledQuery::from_text("3bhk whitefield under 2cr");

    let unrestricted = TextSearch::search(TextSearchRequest {
        properties: &properties,
        search_index: None,
        extra_candidate_ids: None,
        candidate_property_indexes: None,
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        societies: &[],
        compiled_query: &compiled_query,
        graph: None,
    });
    let restricted = TextSearch::search(TextSearchRequest {
        properties: &properties,
        search_index: None,
        extra_candidate_ids: None,
        candidate_property_indexes: Some(vec![2, 1, 2, 0]),
        geo_query: None,
        serving_facts: None,
        society_names: &society_names,
        societies: &[],
        compiled_query: &compiled_query,
        graph: None,
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
        SearchLogMessage::PersistEnrichmentGaps(_) => {
            panic!("expected search-event metadata")
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
    }
}

fn empty_response(query: &str) -> SearchResponse {
    SearchResponse {
        query: query.to_string(),
        result_sets: Vec::new(),
        total_matches: 0,
        area_context: None,
        state: "no_matches".to_string(),
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
    }
}

fn loaded_bundle(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
) -> LoadedServingBundle {
    loaded_bundle_with_aliases(entities, facts, Vec::new())
}

fn loaded_bundle_with_aliases(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    aliases: Vec<ServingEntityAliasRecord>,
) -> LoadedServingBundle {
    let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    let entity_alias_index = ServingEntityAliasIndex::from_records(aliases).unwrap();
    let temp_dir = tempdir().unwrap();
    let recall_index =
        TantivyRecallIndex::build_in_dir(temp_dir.path(), &entities, &facts, &[]).unwrap();
    let geo_index = GeoSearchIndex::from_serving_bundle(&entities, &fact_index);
    let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
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
            edge_count: 0,
            admission_profile: backend::dag_config::ServingAdmissionProfile::BuyerCatalog,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: Default::default(),
            entity_parquet_key: "entities.parquet".to_string(),
            entity_alias_parquet_key: None,
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: None,
            edge_parquet_key: None,
            quarantine_report_key: None,
            schema_key: "schema.json".to_string(),
            trust_policy_key: "trust.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entities,
        entity_alias_index,
        edges: Vec::new(),
        graph_index: GraphIndex::default(),
        recall_index,
        fact_index,
        rera_evidence_index: ReraEvidenceIndex::default(),
        geo_index,
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
