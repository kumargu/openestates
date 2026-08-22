//! Search quality tests — realistic customer queries against the serving bundle.
//!
//! Run with: cargo test -p backend --test search_quality -- --nocapture
//!
//! These tests load the promoted serving bundle and properties, fire queries
//! that real customers would type, and evaluate whether the search system
//! returns sensible, well-ranked, well-explained results.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use backend::knowledge;
use backend::models::{Property, Society};
use backend::search::intent::parse_intent;
use backend::search::{SearchEngine, SearchIndex};
use backend::serving::LoadedServingBundle;

const MIN_LABELLED_QUERY_PASSES: usize = 27;

/// Load the promoted serving bundle and derive request-path data for testing.
fn load_test_data() -> (
    Vec<Property>,
    Vec<Society>,
    knowledge::KnowledgeGraph,
    HashMap<String, String>,
    SearchIndex,
    HashMap<String, usize>,
    Arc<LoadedServingBundle>,
) {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let bundle = runtime
        .block_on(backend::data_loader::load_serving_bundle(project_root))
        .expect("serving bundle loader should run")
        .expect("promoted serving bundle must exist for search quality tests");
    let graph = knowledge::KnowledgeGraph::new();
    let societies = backend::data_loader::societies_from_serving_bundle(&bundle);
    let properties = backend::data_loader::properties_from_serving_bundle(&bundle);

    let society_names: HashMap<String, String> = societies
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();

    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    let property_by_id = properties
        .iter()
        .enumerate()
        .map(|(index, property)| (property.id.clone(), index))
        .collect();

    (
        properties,
        societies,
        graph,
        society_names,
        search_index,
        property_by_id,
        bundle,
    )
}

fn run_search(
    properties: &[Property],
    society_names: &HashMap<String, String>,
    societies: &[Society],
    search_index: &SearchIndex,
    query: &str,
    graph: &knowledge::KnowledgeGraph,
    property_by_id: &HashMap<String, usize>,
    serving_bundle: &Arc<LoadedServingBundle>,
) -> Vec<backend::search::SearchResultCard> {
    run_search_output(
        properties,
        society_names,
        societies,
        search_index,
        query,
        graph,
        property_by_id,
        serving_bundle,
    )
    .results
}

fn run_search_output(
    properties: &[Property],
    society_names: &HashMap<String, String>,
    societies: &[Society],
    search_index: &SearchIndex,
    query: &str,
    graph: &knowledge::KnowledgeGraph,
    property_by_id: &HashMap<String, usize>,
    serving_bundle: &Arc<LoadedServingBundle>,
) -> backend::search::engine::SearchEngineOutput {
    SearchEngine {
        properties,
        search_index,
        serving_bundle: Some(serving_bundle.as_ref()),
        society_names,
        property_by_id: Some(property_by_id),
        societies,
        graph: Some(graph),
    }
    .search(query)
}

/// A test query with expected behavior.
struct QueryTest {
    query: &'static str,
    scenario: &'static str,
    min_results: usize,
    // Expected only after the serving-backed resolver runs. The parser must not
    // contain these named locality instances.
    expect_area: Option<&'static str>,
    expect_bhk: Option<u32>,
    expect_budget: Option<u64>,
    expect_preferences: Vec<&'static str>,
    expect_society_in_results: Option<&'static str>,
    min_score_threshold: f64,
}

fn customer_queries() -> Vec<QueryTest> {
    vec![
        // === TIER 1: Basic structured queries ===
        QueryTest {
            query: "3bhk in whitefield",
            scenario: "Basic BHK + area",
            min_results: 1,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "2bhk sarjapur road under 1.5cr",
            scenario: "BHK + area + budget",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: Some(2),
            expect_budget: Some(15_000_000),
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "3 bhk bellandur below 2cr",
            scenario: "Spaced BHK + area + budget variant",
            min_results: 0,
            expect_area: Some("Bellandur"),
            expect_bhk: Some(3),
            expect_budget: Some(20_000_000),
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // === TIER 2: Preference-driven queries ===
        QueryTest {
            query: "ready to move 3bhk whitefield",
            scenario: "Project status preference — should match RERA-verified status",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["ready to move"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "quiet 2bhk near metro in koramangala",
            scenario: "Multiple soft preferences — tests preference stacking",
            min_results: 0,
            expect_area: Some("Koramangala"),
            expect_bhk: Some(2),
            expect_budget: None,
            expect_preferences: vec!["quiet neighborhood", "metro access"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "premium 3bhk good society whitefield under 3cr",
            scenario: "Premium lifestyle + good society",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: Some(30_000_000),
            expect_preferences: vec!["premium", "good society"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "affordable 2bhk marathahalli",
            scenario: "Value-conscious buyer — 'affordable' maps to value preference",
            min_results: 0,
            expect_area: Some("Marathahalli"),
            expect_bhk: Some(2),
            expect_budget: None,
            expect_preferences: vec!["value for money"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // === TIER 3: Builder trust queries ===
        QueryTest {
            query: "reliable builder 3bhk whitefield",
            scenario: "Builder trust preference",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["reliable builder"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "trusted builder sarjapur under 2cr",
            scenario: "Builder trust + budget",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: None,
            expect_budget: Some(20_000_000),
            expect_preferences: vec!["trusted builder"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // === TIER 4: Status-specific queries ===
        QueryTest {
            query: "under construction projects in hebbal",
            scenario: "Construction status filter",
            min_results: 0,
            expect_area: Some("North Bengaluru"),
            expect_bhk: None,
            expect_budget: None,
            expect_preferences: vec!["under construction"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "new launch 3bhk sarjapur",
            scenario: "New launch hunting — pre-launch investors",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["new launch"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // === TIER 5: Complex multi-signal queries ===
        QueryTest {
            query: "peaceful green 3bhk ready to move whitefield under 2.5cr",
            scenario: "Kitchen-sink: area + bhk + budget + 3 preferences",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: Some(25_000_000),
            expect_preferences: vec!["quiet neighborhood", "greenery", "ready to move"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "luxury 4bhk hsr layout",
            scenario: "Premium segment — fewer results but high quality",
            min_results: 0,
            expect_area: Some("HSR Layout"),
            expect_bhk: Some(4),
            expect_budget: None,
            expect_preferences: vec!["premium"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // === TIER 6: Edge cases ===
        QueryTest {
            query: "whitefield",
            scenario: "Area-only query — should return all properties in area",
            min_results: 1,
            expect_area: Some("Whitefield"),
            expect_bhk: None,
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "3bhk",
            scenario: "BHK-only query — no area constraint, citywide",
            min_results: 1,
            expect_area: None,
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "sobha whitefield",
            scenario: "Builder name search — should surface Sobha properties",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: None,
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: Some("sobha"),
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "prestige koramangala",
            scenario: "Builder name + area",
            min_results: 0,
            expect_area: Some("Koramangala"),
            expect_bhk: None,
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: Some("prestige"),
            min_score_threshold: 0.0,
        },
        // === TIER 7: Semantic/lifestyle queries (Sprint 4 enrichment targets) ===
        // These test preferences that require enriched facts from Reddit, Google
        // reviews, and society-level intelligence skills. Most will show gaps now
        // and should improve after Sprint 4 enrichment.

        // --- Family & safety ---
        QueryTest {
            query: "family friendly 3bhk whitefield under 2cr",
            scenario: "INTENT GAP: 'family friendly' not yet parsed — needs preference pattern",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: Some(20_000_000),
            expect_preferences: vec![], // GAP: "family friendly" not yet a preference
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "safe gated community 2bhk bellandur",
            scenario: "INTENT GAP: 'safe'/'gated' not yet parsed — needs preference patterns",
            min_results: 0,
            expect_area: Some("Bellandur"),
            expect_bhk: Some(2),
            expect_budget: None,
            expect_preferences: vec![], // GAP: "safe"/"gated" not yet preferences
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Commute & connectivity ---
        QueryTest {
            query: "walkable to metro 2bhk koramangala under 1.5cr",
            scenario: "ENRICHMENT TARGET: precise metro distance from maps/transit data",
            min_results: 0,
            expect_area: Some("Koramangala"),
            expect_bhk: Some(2),
            expect_budget: Some(15_000_000),
            expect_preferences: vec!["metro access"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Investment & resale ---
        QueryTest {
            query: "good resale value 3bhk sarjapur",
            scenario: "ENRICHMENT TARGET: resale strength from market data/appreciation",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["value for money"], // closest current mapping
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "ready to move rera verified 3bhk whitefield",
            scenario: "RERA trust + status — should already work well via graph",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["ready to move"], // "rera verified" not yet a preference
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Society quality & maintenance ---
        QueryTest {
            query: "well maintained society 3bhk hebbal",
            scenario: "ENRICHMENT TARGET: maintenance quality from Reddit/Google reviews",
            min_results: 0,
            expect_area: Some("North Bengaluru"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["good society"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "good amenities premium 3bhk whitefield",
            scenario: "INTENT GAP: 'good amenities' not parsed as 'good society'",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["premium"], // GAP: "good amenities" doesn't trigger "good society"
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Waterlogging & risk ---
        QueryTest {
            query: "no waterlogging 3bhk bellandur under 2cr",
            scenario: "Negative preference — avoid waterlogging risk",
            min_results: 0,
            expect_area: Some("Bellandur"),
            expect_bhk: Some(3),
            expect_budget: Some(20_000_000),
            expect_preferences: vec!["avoid waterlogging risk"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Natural language / conversational ---
        QueryTest {
            query: "peaceful 3bhk with park view in sarjapur under 1.8cr",
            scenario: "ENRICHMENT TARGET: greenery/park proximity + quiet neighborhood",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: Some(3),
            expect_budget: Some(18_000_000),
            expect_preferences: vec!["quiet neighborhood", "greenery"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "good school nearby 3bhk whitefield ready to move",
            scenario: "INTENT GAP: 'school nearby' not yet parsed — needs POI preference",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["ready to move"], // GAP: "school" not yet a preference
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Comparative / competitive queries ---
        QueryTest {
            query: "sobha vs prestige 3bhk whitefield",
            scenario: "ENRICHMENT TARGET: builder comparison — needs multi-result scoring",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: Some("sobha"), // at least one should show
            min_score_threshold: 0.0,
        },
        // --- High-intent buyer signals ---
        QueryTest {
            query: "immediate possession 2bhk hsr layout under 1cr",
            scenario: "Urgency signal — maps to ready to move",
            min_results: 0,
            expect_area: Some("HSR Layout"),
            expect_bhk: Some(2),
            expect_budget: Some(10_000_000),
            expect_preferences: vec!["ready to move"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "delivered project 3bhk sarjapur good builder",
            scenario: "Completed + builder trust combo",
            min_results: 0,
            expect_area: Some("Sarjapur Road"),
            expect_bhk: Some(3),
            expect_budget: None,
            expect_preferences: vec!["ready to move", "trusted builder"],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        // --- Area micro-market queries ---
        QueryTest {
            query: "3bhk varthur under 1.5cr",
            scenario: "Sub-area query — varthur should map to Whitefield",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(3),
            expect_budget: Some(15_000_000),
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
        QueryTest {
            query: "2bhk itpl road",
            scenario: "Landmark-based area — ITPL should map to Whitefield",
            min_results: 0,
            expect_area: Some("Whitefield"),
            expect_bhk: Some(2),
            expect_budget: None,
            expect_preferences: vec![],
            expect_society_in_results: None,
            min_score_threshold: 0.0,
        },
    ]
}

// ============================================================================
// TEST 1: Intent parsing quality
// ============================================================================

#[test]
fn test_structured_intent_parsing_without_named_locality_aliases() {
    println!();
    println!("================================================================================");
    println!("  STRUCTURED INTENT PARSING REPORT");
    println!("================================================================================");
    println!();

    let queries = customer_queries();
    let mut pass = 0;
    let mut fail = 0;

    for qt in &queries {
        let intent = parse_intent(qt.query);
        let mut errors: Vec<String> = Vec::new();

        if intent.area.is_some() {
            errors.push(format!(
                "named area {:?} bypassed serving-backed resolution as {:?}",
                qt.expect_area, intent.area,
            ));
        }
        if intent.bhk != qt.expect_bhk {
            errors.push(format!(
                "bhk: expected {:?}, got {:?}",
                qt.expect_bhk, intent.bhk
            ));
        }
        if intent.budget_max != qt.expect_budget {
            errors.push(format!(
                "budget: expected {:?}, got {:?}",
                qt.expect_budget, intent.budget_max
            ));
        }
        for pref in &qt.expect_preferences {
            if !intent.preferences.contains(&pref.to_string()) {
                errors.push(format!(
                    "missing preference '{}' (got: {:?})",
                    pref, intent.preferences
                ));
            }
        }

        if errors.is_empty() {
            pass += 1;
            println!("  PASS  \"{}\"", qt.query);
            println!(
                "        -> area={:?} bhk={:?} budget={:?} prefs={:?}",
                intent.area, intent.bhk, intent.budget_max, intent.preferences
            );
        } else {
            fail += 1;
            println!("  FAIL  \"{}\" ({})", qt.query, qt.scenario);
            for e in &errors {
                println!("        ! {}", e);
            }
        }
    }

    println!();
    println!(
        "  Structured intent parsing: {}/{} passed",
        pass,
        pass + fail
    );
    println!();
    assert_eq!(fail, 0, "{} intent parsing tests failed", fail);
}

// ============================================================================
// TEST 2: Search result quality
// ============================================================================

#[test]
fn test_search_result_quality() {
    let (properties, societies, graph, society_names, search_index, property_by_id, serving_bundle) =
        load_test_data();

    println!();
    println!("================================================================================");
    println!("  SEARCH RESULT QUALITY REPORT");
    println!(
        "  Properties: {}, Societies: {}, Graph nodes: {}",
        properties.len(),
        societies.len(),
        graph.stats().total_nodes
    );
    println!("================================================================================");
    println!();

    let queries = customer_queries();
    let mut total_pass = 0;
    let mut total_fail = 0;
    let mut quality_issues: Vec<String> = Vec::new();

    for qt in &queries {
        let intent = parse_intent(qt.query);
        let results = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            qt.query,
            &graph,
            &property_by_id,
            &serving_bundle,
        );

        let mut issues: Vec<String> = Vec::new();

        // Check minimum results
        if results.len() < qt.min_results {
            issues.push(format!(
                "expected >= {} results, got {}",
                qt.min_results,
                results.len()
            ));
        }

        // Check score threshold
        for (i, r) in results.iter().enumerate() {
            if r.match_score < qt.min_score_threshold {
                issues.push(format!(
                    "result #{} score {:.2} below threshold {:.2}",
                    i, r.match_score, qt.min_score_threshold
                ));
            }
        }

        // Check expected society in results
        if let Some(expected_soc) = qt.expect_society_in_results {
            let found = results.iter().any(|r| {
                r.card.society_name.to_lowercase().contains(expected_soc)
                    || r.card.title.to_lowercase().contains(expected_soc)
                    || r.card.builder_name.to_lowercase().contains(expected_soc)
            });
            if !found {
                issues.push(format!(
                    "expected '{}' in results, not found in {} results",
                    expected_soc,
                    results.len()
                ));
            }
        }

        // Check preference coverage
        if !qt.expect_preferences.is_empty() {
            let mut graph_scored = 0;
            let mut no_data = 0;

            for r in &results {
                if let Some(ref expl) = r.match_explanation {
                    for cov in &expl.preference_coverage {
                        if cov.status == "matched" || cov.status == "partial" {
                            graph_scored += 1;
                        } else {
                            no_data += 1;
                        }
                    }
                }
            }

            if !results.is_empty() && graph_scored == 0 && no_data > 0 {
                issues.push(format!(
                    "NO preference data: {} preferences had 'no_data' across all results",
                    no_data / results.len().max(1)
                ));
            }
        }

        let status = if issues.is_empty() { "PASS" } else { "WARN" };
        if issues.is_empty() {
            total_pass += 1;
        } else {
            total_fail += 1;
        }

        println!("  {}  \"{}\"", status, qt.query);
        println!("        Scenario: {}", qt.scenario);
        println!("        Results: {} | Top score: {:.2} | Intent: area={:?} bhk={:?} budget={:?} prefs={:?}",
            results.len(),
            results.first().map_or(0.0, |r| r.match_score),
            intent.area, intent.bhk, intent.budget_max, intent.preferences,
        );

        // Show top 3 results
        for (i, r) in results.iter().take(3).enumerate() {
            let graph_pct = r
                .match_explanation
                .as_ref()
                .map(|e| format!("{:.0}%", e.graph_driven_pct))
                .unwrap_or_else(|| "n/a".into());
            let confidence = r
                .confidence_score
                .as_ref()
                .map(|c| format!("{:.0} ({})", c.overall * 100.0, c.label))
                .unwrap_or_else(|| "n/a".into());

            println!(
                "        #{}: {} | score={:.2} | graph={} | confidence={}",
                i + 1,
                r.card.title,
                r.match_score,
                graph_pct,
                confidence
            );

            if let Some(ref expl) = r.match_explanation {
                for reason in &expl.reasons {
                    println!(
                        "             {} -> {} [{}] (conf={:.1}, src={})",
                        reason.preference,
                        reason.display,
                        reason.scoring_method,
                        reason.confidence,
                        reason.source_type
                    );
                }
                for cov in &expl.preference_coverage {
                    if cov.status == "no_data" {
                        println!("             {} -> NO DATA (gap)", cov.preference);
                    }
                }
            }
        }

        for issue in &issues {
            println!("        ! {}", issue);
            quality_issues.push(format!("\"{}\": {}", qt.query, issue));
        }
        println!();
    }

    // === Summary ===
    println!("================================================================================");
    println!(
        "  SUMMARY: {}/{} queries passed quality checks",
        total_pass,
        total_pass + total_fail
    );

    if !quality_issues.is_empty() {
        println!();
        println!("  Quality issues found:");
        for issue in &quality_issues {
            println!("    - {}", issue);
        }
    }

    // Aggregate stats
    let mut total_results = 0;
    let mut total_graph_driven = 0.0_f64;
    let mut total_with_explanation = 0;

    for qt in &queries {
        let results = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            qt.query,
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        total_results += results.len();
        for r in &results {
            if let Some(ref expl) = r.match_explanation {
                total_graph_driven += expl.graph_driven_pct as f64;
                total_with_explanation += 1;
            }
        }
    }

    let avg_graph = if total_with_explanation > 0 {
        total_graph_driven / total_with_explanation as f64
    } else {
        0.0
    };

    println!();
    println!("  Aggregate:");
    println!("    Total results across all queries: {}", total_results);
    println!("    Avg graph-driven scoring: {:.1}%", avg_graph);
    println!("    Results with explanations: {}", total_with_explanation);
    println!("================================================================================");
    println!();

    assert!(
        total_pass >= MIN_LABELLED_QUERY_PASSES,
        "labelled search quality regressed: {total_pass}/{} passed, floor is {MIN_LABELLED_QUERY_PASSES}",
        total_pass + total_fail
    );

    // Hard-fail only if queries that should have results return 0
    let critical_failures: Vec<_> = quality_issues
        .iter()
        .filter(|i| i.contains("expected >=") && i.contains("got 0"))
        .collect();
    assert!(
        critical_failures.is_empty(),
        "Critical: {} queries returned 0 results when matches were expected",
        critical_failures.len()
    );
}

// ============================================================================
// TEST 3: Ranking sanity checks
// ============================================================================

#[test]
fn test_ranking_sanity() {
    let (properties, societies, graph, society_names, search_index, property_by_id, serving_bundle) =
        load_test_data();

    println!();
    println!("================================================================================");
    println!("  RANKING SANITY CHECKS");
    println!("================================================================================");
    println!();

    // Test 1: Preferences should boost scores
    {
        let results_basic = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let results_pref = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield ready to move good society",
            &graph,
            &property_by_id,
            &serving_bundle,
        );

        println!("  Ranking shift test: basic vs preference query");
        println!("    \"3bhk whitefield\" -> {} results", results_basic.len());
        println!(
            "    \"3bhk whitefield ready to move good society\" -> {} results",
            results_pref.len()
        );

        if results_basic.len() >= 2 && results_pref.len() >= 2 {
            let top_basic_score = results_basic[0].match_score;
            let top_pref_score = results_pref[0].match_score;
            println!(
                "    Basic top: {} ({:.2})",
                results_basic[0].card.title, top_basic_score
            );
            println!(
                "    Pref top:  {} ({:.2})",
                results_pref[0].card.title, top_pref_score
            );

            if top_pref_score > top_basic_score {
                println!("    PASS: Preference query boosts top score");
            } else {
                println!("    INFO: Scores equal — preferences may not differentiate in this data");
            }
        }
    }

    // Test 2: Budget constraint filters correctly
    {
        let results_all = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let budget_output = run_search_output(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield under 1cr",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let results_budget = &budget_output.results;

        println!();
        println!("  Budget filtering test:");
        println!("    No budget: {} results", results_all.len());
        println!("    Under 1cr: {} results", results_budget.len());

        assert!(
            results_budget.len() <= results_all.len(),
            "Budget filter should not produce MORE results"
        );

        for r in results_budget
            .iter()
            .take(budget_output.eligible_result_count)
        {
            let lowest_asking_price = r.card.price_min.unwrap_or(r.card.price);
            assert!(
                lowest_asking_price <= 10_000_000,
                "Property {} has asking range starting at {} which exceeds 1cr budget",
                r.card.title,
                lowest_asking_price
            );
        }
        println!("    PASS: Budget constraint correctly filters");
    }

    // Test 3: Confidence scoring coverage
    {
        let results = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let with_confidence = results
            .iter()
            .filter(|r| r.confidence_score.is_some())
            .count();

        println!();
        println!("  Confidence scoring coverage:");
        println!(
            "    {}/{} results have confidence scores",
            with_confidence,
            results.len()
        );

        if !results.is_empty() {
            let pct = (with_confidence as f64 / results.len() as f64) * 100.0;
            println!("    Coverage: {:.0}%", pct);
            if pct >= 50.0 {
                println!("    PASS: Majority of results have confidence data");
            } else {
                println!("    WARN: Less than 50% have confidence data");
            }
        }
    }

    // Test 4: Tiered ranking remains deterministic. The configured ranking is
    // lexicographic, so aggregate match_score is not itself the sort key.
    {
        let first = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield ready to move",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let second = run_search(
            &properties,
            &society_names,
            &societies,
            &search_index,
            "3bhk whitefield ready to move",
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        let first_ids = first
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>();
        let second_ids = second
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>();
        println!();
        assert_eq!(
            first_ids, second_ids,
            "tiered ranking must be deterministic"
        );
        println!("  PASS: Tiered ranking is deterministic");
    }

    println!();
    println!("================================================================================");
    println!();
}

#[test]
fn promoted_bundle_resolves_labelled_named_places() {
    let (properties, societies, graph, society_names, search_index, property_by_id, serving_bundle) =
        load_test_data();

    for (query, expected_place) in [
        ("3bhk near Bagmane Tech Park", "Bagmane Tech Park"),
        ("3bhk near Kadugodi Metro", "Kadugodi"),
    ] {
        let output = run_search_output(
            &properties,
            &society_names,
            &societies,
            &search_index,
            query,
            &graph,
            &property_by_id,
            &serving_bundle,
        );
        assert!(
            output.diagnostics.resolved.entities.iter().any(|entity| {
                entity.entity_type == "place"
                    && entity
                        .name
                        .to_ascii_lowercase()
                        .contains(&expected_place.to_ascii_lowercase())
            }),
            "{query:?} should resolve {expected_place:?}: {:?}",
            output.diagnostics.resolved.entities
        );
        assert!(
            !output.results.is_empty(),
            "{query:?} should recall homes; warnings={:?}, recall={:?}",
            output.diagnostics.warnings,
            output.diagnostics.recall
        );
        let reasons = output
            .results
            .iter()
            .flat_map(|result| {
                result
                    .match_explanation
                    .as_ref()
                    .into_iter()
                    .flat_map(|explanation| explanation.reasons.iter())
            })
            .collect::<Vec<_>>();
        assert!(
            reasons.iter().any(|reason| {
                (reason.scoring_method == "serving-haversine"
                    || reason.scoring_method == "serving-named-place")
                    && reason.display.contains(expected_place)
            }),
            "{query:?} should carry typed proximity proof: {reasons:?}"
        );
    }
}
