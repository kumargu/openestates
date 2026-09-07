use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use backend::graph::GraphIndex;
use backend::knowledge::FactValue;
use backend::models::{Property, Society};
use backend::search::geo::GeoSearchIndex;
use backend::search::{
    compile_search_revision, SearchCapabilityIndex, SearchEngine, SearchIndex,
    SearchRevisionLimits, SearchRevisionOperation, SearchRevisionOutcome,
};
use backend::serving::{
    derive_proximity_records, materialize_canonical_spatial_identities, DerivedEvidence,
    EvidenceId, EvidenceRef, LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest,
    ServingEntityAliasIndex, ServingEntityAliasRecord, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, ServingSearchMetadataRecord, SourceObservation, SpatialServingIndex,
    TantivyRecallIndex, PROVIDER_BINDING_EDGE,
};
use backend::state::SearchRuntimeSnapshot;
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use tempfile::tempdir;

const SEARCH_QUERY_BANK: &str = include_str!("../../data/validation/search_query_bank.json");
const CONTROLLED_SUITE_ID: &str = "controlled_product";
const CONTROLLED_JOURNEY_SUITE_ID: &str = "controlled_journey";
const SPATIAL_REVISION_SUITE_ID: &str = "spatial_revision";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchQueryBank {
    version: u32,
    generated_at: String,
    description: String,
    case_groups: Vec<QueryCaseGroup>,
    suites: Vec<QuerySuite>,
    cases: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct QueryCaseGroup {
    id: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Deserialize)]
struct QuerySuite {
    id: String,
    runner: String,
    case_groups: Vec<String>,
    limits: Option<JourneyLimits>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlledQueryCase {
    id: String,
    #[serde(rename = "group")]
    _group: String,
    query: String,
    #[serde(rename = "category")]
    _category: String,
    fixture: FixtureKind,
    expected_semantics: ExpectedSemantics,
    fixture_expectation: FixtureExpectation,
}

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FixtureKind {
    Core,
    BuyerLanguage,
    DecisionRanking,
    MultiOr,
    Proximity,
    Regional,
    GeoCells,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedSemantics {
    area: Option<String>,
    bhk: Option<u32>,
    bhks: Option<Vec<u32>>,
    budget_max: Option<u64>,
    society: Option<String>,
    home_state: Option<String>,
    exclude_home_state: Option<String>,
    near: Option<String>,
    place_family: Option<String>,
    distance_max_km: Option<f64>,
    abstain: Option<bool>,
    unresolved_society: Option<String>,
    branches: Option<Vec<ExpectedBranch>>,
    positive_preferences: Option<Vec<String>>,
    negative_preferences: Option<Vec<String>>,
    ranking_priorities: Option<Vec<String>>,
    accepted_tradeoffs: Option<Vec<String>>,
    missing_optional_evidence: Option<MissingOptionalEvidence>,
    numeric_min: Option<NumericExpectation>,
    allows_cell_nearby: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericExpectation {
    field: String,
    value: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedBranch {
    area: Option<String>,
    society: Option<String>,
    bhk: Option<u32>,
    budget_max: Option<u64>,
    allows_cell_nearby: Option<bool>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MissingOptionalEvidence {
    IncludeWithoutClaim,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureExpectation {
    result_sets: Option<Vec<Vec<String>>>,
    first_id: Option<String>,
    #[serde(default)]
    includes: Vec<String>,
    #[serde(default)]
    excludes: Vec<String>,
    #[serde(default)]
    forbidden_proof_labels: Vec<String>,
    #[serde(default)]
    ordered_prefix: Vec<String>,
    max_total_results: Option<usize>,
    non_empty_result_sets: Option<usize>,
    unique_result_ids: Option<bool>,
    #[serde(default)]
    result_set_counts: Vec<usize>,
    #[serde(default)]
    result_set_ordered_prefixes: Vec<Vec<String>>,
    #[serde(default)]
    required_proof_labels: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlledJourneyCase {
    id: String,
    #[serde(rename = "group")]
    _group: String,
    fixture: FixtureKind,
    initial_case_id: String,
    turns: Vec<JourneyTurn>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JourneyTurn {
    utterance: String,
    operation: JourneyOperation,
    candidate_case_id: Option<String>,
    outcome: JourneyOutcome,
    active_case_id: String,
    candidate_effect: Option<CandidateEffect>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JourneyOperation {
    Refine,
    Rephrase,
    Expand,
    Switch,
    Replace,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JourneyOutcome {
    ActivateCandidate,
    PreserveParent,
    RequireClarification,
    RequireCheckpoint,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CandidateEffect {
    Narrower,
    SameMembership,
    SameOrder,
    AddsBranch,
    DifferentMembership,
    ZeroResults,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct JourneyLimits {
    max_active_branches: usize,
    max_revision_depth_before_checkpoint: usize,
    measured_branch_quality_cohorts: Vec<usize>,
    deferred_branch_stress_cohort: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpatialRevisionCase {
    id: String,
    #[serde(rename = "group")]
    _group: String,
    parent_query: String,
    parent_branch_count: usize,
    utterance: String,
    expected_operation: JourneyOperation,
    expected_outcome: SpatialRevisionExpectedOutcome,
    expected_active_query: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SpatialRevisionExpectedOutcome {
    Candidate,
    RequireClarification,
    RequireCheckpoint,
}

#[test]
fn frozen_product_scenarios_execute_against_controlled_inventory() {
    let bank: SearchQueryBank =
        serde_json::from_str(SEARCH_QUERY_BANK).expect("unified search query bank is valid");
    assert_eq!(bank.version, 1, "unsupported search query bank version");
    assert!(
        !bank.generated_at.trim().is_empty(),
        "bank date is required"
    );
    assert!(
        !bank.description.trim().is_empty(),
        "bank purpose is required"
    );
    let suite = bank
        .suites
        .iter()
        .find(|suite| suite.id == CONTROLLED_SUITE_ID)
        .expect("controlled product suite is required");
    assert_eq!(suite.runner, "rust_controlled");
    assert_eq!(suite.case_groups, [CONTROLLED_SUITE_ID]);
    let group = bank
        .case_groups
        .iter()
        .find(|group| group.id == CONTROLLED_SUITE_ID)
        .expect("controlled product group is required");
    assert!(
        !group.notes.is_empty(),
        "controlled group purpose must remain documented"
    );
    let cases = bank
        .cases
        .into_iter()
        .filter(|case| {
            case.get("group").and_then(serde_json::Value::as_str) == Some(CONTROLLED_SUITE_ID)
        })
        .map(|case| {
            serde_json::from_value::<ControlledQueryCase>(case)
                .expect("controlled search case follows the typed contract")
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 67, "controlled bank size changed");

    let unique_ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(unique_ids.len(), cases.len(), "duplicate scenario IDs");

    let represented_fixtures = cases
        .iter()
        .map(|case| case.fixture)
        .collect::<HashSet<_>>();
    assert_eq!(
        represented_fixtures,
        HashSet::from([
            FixtureKind::Core,
            FixtureKind::BuyerLanguage,
            FixtureKind::DecisionRanking,
            FixtureKind::MultiOr,
            FixtureKind::Proximity,
            FixtureKind::Regional,
            FixtureKind::GeoCells,
        ]),
        "every controlled fixture profile must remain represented"
    );

    let fixtures = represented_fixtures
        .into_iter()
        .map(|kind| (kind, MockSearchFixture::for_kind(kind)))
        .collect::<HashMap<_, _>>();
    for case in &cases {
        let output = fixtures[&case.fixture].search(&case.query);
        if std::env::var_os("OPENESTATES_TRACE_SEARCH_SCENARIOS").is_some() {
            let observed = output
                .result_sets
                .iter()
                .map(|result_set| {
                    result_set
                        .iter()
                        .map(|result| (result.id.as_str(), result.proof_labels.as_slice()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            eprintln!("{} | {} | {observed:?}", case.id, case.query);
        }
        assert_controlled_expectation(case, &output);
    }
}

#[test]
fn nested_journeys_reuse_atomic_cases_and_the_same_search_path() {
    let bank: SearchQueryBank =
        serde_json::from_str(SEARCH_QUERY_BANK).expect("unified search query bank is valid");
    let suite = bank
        .suites
        .iter()
        .find(|suite| suite.id == CONTROLLED_JOURNEY_SUITE_ID)
        .expect("controlled journey suite is required");
    assert_eq!(suite.runner, "rust_controlled_journey");
    assert_eq!(
        suite.case_groups,
        [CONTROLLED_SUITE_ID, CONTROLLED_JOURNEY_SUITE_ID]
    );
    let limits = suite.limits.clone().expect("journey limits are required");
    assert!(limits.max_active_branches > 1);
    assert!(limits.max_revision_depth_before_checkpoint > 1);
    assert_eq!(limits.measured_branch_quality_cohorts, [3, 8, 16]);
    assert_eq!(limits.deferred_branch_stress_cohort, 16);
    assert_eq!(limits.max_active_branches, 8);

    let atomic_cases = bank
        .cases
        .iter()
        .filter(|case| {
            case.get("group").and_then(serde_json::Value::as_str) == Some(CONTROLLED_SUITE_ID)
        })
        .map(|case| {
            let parsed = serde_json::from_value::<ControlledQueryCase>(case.clone())
                .expect("controlled search case follows the typed contract");
            (parsed.id.clone(), parsed)
        })
        .collect::<HashMap<_, _>>();
    let journeys = bank
        .cases
        .iter()
        .filter(|case| {
            case.get("group").and_then(serde_json::Value::as_str)
                == Some(CONTROLLED_JOURNEY_SUITE_ID)
        })
        .map(|case| {
            serde_json::from_value::<ControlledJourneyCase>(case.clone())
                .expect("controlled journey follows the typed contract")
        })
        .collect::<Vec<_>>();
    assert_eq!(journeys.len(), 7, "controlled journey bank size changed");

    for journey in &journeys {
        run_controlled_journey(journey, &atomic_cases, limits.clone());
    }
}

#[test]
fn issue_118_revision_scenarios_are_frozen_in_the_unified_bank() {
    let bank: SearchQueryBank =
        serde_json::from_str(SEARCH_QUERY_BANK).expect("unified search query bank is valid");
    let suite = bank
        .suites
        .iter()
        .find(|suite| suite.id == SPATIAL_REVISION_SUITE_ID)
        .expect("Issue 118 spatial revision suite is required");
    assert_eq!(suite.runner, "rust_spatial_revision");
    assert_eq!(suite.case_groups, [SPATIAL_REVISION_SUITE_ID]);
    let cases = bank
        .cases
        .iter()
        .filter(|case| {
            case.get("group").and_then(serde_json::Value::as_str) == Some(SPATIAL_REVISION_SUITE_ID)
        })
        .map(|case| {
            serde_json::from_value::<SpatialRevisionCase>(case.clone())
                .expect("spatial revision case follows the typed contract")
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 10, "Issue 118 revision bank size changed");

    for case in cases {
        let revision = compile_search_revision(
            &case.parent_query,
            &case.utterance,
            case.parent_branch_count,
            SearchRevisionLimits {
                max_active_branches: 8,
            },
        );
        assert_eq!(
            observed_operation(revision.operation),
            case.expected_operation,
            "{} compiled the wrong operation",
            case.id
        );
        let observed_outcome = match revision.outcome {
            SearchRevisionOutcome::Candidate => SpatialRevisionExpectedOutcome::Candidate,
            SearchRevisionOutcome::RequireClarification => {
                SpatialRevisionExpectedOutcome::RequireClarification
            }
            SearchRevisionOutcome::RequireCheckpoint => {
                SpatialRevisionExpectedOutcome::RequireCheckpoint
            }
        };
        assert_eq!(
            observed_outcome, case.expected_outcome,
            "{} compiled the wrong outcome",
            case.id
        );
        assert_eq!(
            revision.candidate_query.as_deref(),
            case.expected_active_query.as_deref(),
            "{} compiled the wrong active query",
            case.id
        );
    }
}

#[test]
fn issue_118_candidate_revisions_execute_through_search_engine() {
    let bank: SearchQueryBank =
        serde_json::from_str(SEARCH_QUERY_BANK).expect("unified search query bank is valid");
    let cases = bank
        .cases
        .iter()
        .filter(|case| {
            case.get("group").and_then(serde_json::Value::as_str) == Some(SPATIAL_REVISION_SUITE_ID)
        })
        .map(|case| serde_json::from_value::<SpatialRevisionCase>(case.clone()).unwrap())
        .collect::<Vec<_>>();
    let fixture = issue_118_fixture();
    let mut executed = 0;
    for case in &cases {
        let revision = compile_search_revision(
            &case.parent_query,
            &case.utterance,
            case.parent_branch_count,
            SearchRevisionLimits {
                max_active_branches: 8,
            },
        );
        let Some(candidate_query) = revision.candidate_query.as_deref() else {
            continue;
        };
        executed += 1;
        let output = fixture.search_output(candidate_query);
        match case.id.as_str() {
            "SPATIAL-REVISION-READY" => assert!(output.results.iter().all(|result| {
                result
                    .card
                    .possession_status
                    .eq_ignore_ascii_case("Ready to Move")
            })),
            "SPATIAL-REVISION-TWO-ANCHORS" => {
                let hoodi_metro_id = fixture
                    .bundle
                    .edges
                    .iter()
                    .find(|edge| {
                        edge.edge_type == PROVIDER_BINDING_EDGE
                            && edge.to_entity_id == "place:hoodi-metro"
                    })
                    .map(|edge| edge.from_entity_id.as_str())
                    .expect("fixture retains canonical Hoodi Metro identity");
                let manipal_hospital_id = fixture
                    .bundle
                    .edges
                    .iter()
                    .find(|edge| {
                        edge.edge_type == PROVIDER_BINDING_EDGE
                            && edge.to_entity_id == "place:manipal-hospital"
                    })
                    .map(|edge| edge.from_entity_id.as_str())
                    .expect("fixture retains canonical Manipal Hospital identity");
                let ids = output
                    .results
                    .iter()
                    .map(|result| result.card.id.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
                assert!(output.results.iter().any(|result| {
                    let proof_entities = result
                        .proof_focuses
                        .iter()
                        .filter_map(|proof| proof.entity_id.as_deref())
                        .collect::<HashSet<_>>();
                    proof_entities.contains(hoodi_metro_id)
                        && proof_entities.contains(manipal_hospital_id)
                }));
            }
            "SPATIAL-REVISION-BUDGET" => {
                assert_eq!(output.intent.budget_max, Some(30_000_000));
                assert!(output
                    .results
                    .iter()
                    .any(|result| result.card.price > 25_000_000));
            }
            "SPATIAL-REVISION-KADUGODI" | "SPATIAL-REVISION-HOODI-BRANCH" => {
                assert_eq!(output.ast_branches.len(), 2);
                assert_eq!(output.intent_branches.len(), 2);
            }
            "SPATIAL-REVISION-EXCLUDE" => {
                assert!(output
                    .results
                    .iter()
                    .all(|result| result.card.area != "Varthur"));
                assert!(output.ast_branches.iter().any(contains_negated_area));
            }
            "SPATIAL-REVISION-REPLACE" => {
                assert!(output
                    .intent
                    .unsupported_inventory_types
                    .iter()
                    .any(|kind| kind.eq_ignore_ascii_case("plot")));
                assert!(output
                    .diagnostics
                    .resolved
                    .entities
                    .iter()
                    .any(|entity| entity.entity_id == "area:north-bengaluru"));
            }
            "SPATIAL-REVISION-REPHRASE" => {
                let direct = fixture.search_output(&case.parent_query);
                assert_eq!(
                    backend::search::ast::semantic_search_fingerprint(
                        &output.ast_branches,
                        &output.intent_branches,
                    ),
                    backend::search::ast::semantic_search_fingerprint(
                        &direct.ast_branches,
                        &direct.intent_branches,
                    )
                );
                assert_eq!(
                    output
                        .results
                        .iter()
                        .map(|result| result.card.id.as_str())
                        .collect::<Vec<_>>(),
                    direct
                        .results
                        .iter()
                        .map(|result| result.card.id.as_str())
                        .collect::<Vec<_>>()
                );
            }
            other => panic!("unexpected candidate revision case {other}"),
        }
    }
    assert_eq!(executed, 8, "every candidate scenario must execute");
}

fn contains_negated_area(expression: &backend::search::ConstraintExpr) -> bool {
    match expression {
        backend::search::ConstraintExpr::Not { clause } => matches!(
            clause.as_ref(),
            backend::search::ConstraintExpr::Term {
                term: backend::search::ConstraintTerm::Area { .. }
            } | backend::search::ConstraintExpr::AnyOf { .. }
        ),
        backend::search::ConstraintExpr::And { clauses }
        | backend::search::ConstraintExpr::AnyOf { clauses } => {
            clauses.iter().any(contains_negated_area)
        }
        backend::search::ConstraintExpr::Term { .. } => false,
    }
}

fn issue_118_fixture() -> MockSearchFixture {
    let mut builder = FixtureBuilder::default();
    for area in [
        "Whitefield",
        "Hoodi",
        "Kadugodi",
        "Varthur",
        "North Bengaluru",
    ] {
        builder.add_area(area);
    }
    builder.add_place("Hoodi Metro", "metro", 12.9900, 77.7150);
    builder.add_place("Manipal Hospital", "hospital", 12.9700, 77.7350);
    builder.add_edge("place:hoodi-metro", "in_market_locality", "area:hoodi");
    builder.add_edge("place:hoodi-metro", "in_area", "area:hoodi");
    builder.add_edge(
        "place:manipal-hospital",
        "in_market_locality",
        "area:whitefield",
    );
    builder.add_edge("place:manipal-hospital", "in_area", "area:whitefield");
    for spec in [
        HomeSpec::new(
            "whitefield-ready",
            "Whitefield Ready",
            "Whitefield",
            3,
            24_000_000,
            12.9800,
            77.7250,
        ),
        HomeSpec::new(
            "whitefield-stretch",
            "Whitefield Stretch",
            "Whitefield",
            3,
            28_000_000,
            12.9810,
            77.7260,
        ),
        HomeSpec::new(
            "whitefield-building",
            "Whitefield Building",
            "Whitefield",
            3,
            23_000_000,
            12.9820,
            77.7270,
        )
        .under_construction(),
        HomeSpec::new(
            "hoodi-two",
            "Hoodi Two",
            "Hoodi",
            2,
            18_000_000,
            12.9890,
            77.7160,
        ),
        HomeSpec::new(
            "kadugodi-three",
            "Kadugodi Three",
            "Kadugodi",
            3,
            26_000_000,
            12.9950,
            77.7550,
        ),
        HomeSpec::new(
            "varthur-three",
            "Varthur Three",
            "Varthur",
            3,
            20_000_000,
            12.9400,
            77.7400,
        ),
        HomeSpec::new(
            "north-three",
            "North Three",
            "North Bengaluru",
            3,
            14_000_000,
            13.0400,
            77.6200,
        ),
    ] {
        builder.add_home(spec);
    }
    builder.build(true)
}

#[test]
fn branch_quality_cohorts_preserve_disconnected_scope_and_branch_local_proof() {
    let mut builder = FixtureBuilder::default();
    let branches = (1..=16)
        .map(|index| {
            let area = format!("Cohort Area {index}");
            let society = format!("Cohort Homes {index}");
            builder.add_area(&area);
            builder.add_home(HomeSpec::new(
                &format!("cohort-home-{index}"),
                &society,
                &area,
                2,
                10_000_000 + index as u64,
                12.8 + index as f64 / 1000.0,
                77.5 + index as f64 / 1000.0,
            ));
            format!("2BHK in {area} under 2Cr")
        })
        .collect::<Vec<_>>();
    let fixture = builder.build(false);

    for cohort in [3, 8, 16] {
        let output = fixture.search_output(&branches[..cohort].join(" or "));
        assert_eq!(
            output.ast_branches.len(),
            cohort,
            "the {cohort}-branch cohort collapsed disconnected scopes"
        );
        assert_eq!(output.intent_branches.len(), cohort);
        assert_eq!(
            output.result_sets.len(),
            cohort,
            "the {cohort}-branch cohort lost executable result sets: {output:#?}"
        );
        assert!(output
            .result_sets
            .iter()
            .all(|branch| branch.results.len() == 1));
        assert_eq!(
            output
                .result_sets
                .iter()
                .flat_map(|branch| &branch.results)
                .map(|result| result.card.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            cohort,
            "the {cohort}-branch cohort duplicated or lost a branch-local result"
        );
    }
}

#[test]
fn sourced_topology_never_erases_logical_spatial_branches() {
    let mut builder = FixtureBuilder::default();
    for area in ["Eastfield", "Nextfield", "Northfield"] {
        builder.add_area(area);
        builder.add_home(HomeSpec::new(
            &format!("home-{}", slug(area)),
            &format!("{area} Homes"),
            area,
            3,
            10_000_000,
            12.9,
            77.7,
        ));
    }
    builder.add_edge(
        "area:cell:area-eastfield",
        "adjacent_area",
        "area:cell:area-nextfield",
    );
    let fixture = builder.build(false);

    let connected =
        fixture.search_output("3BHK in Eastfield under 2Cr or 3BHK in Nextfield under 2Cr");
    assert_eq!(
        connected.ast_branches.len(),
        2,
        "topology may group presentation but must preserve both logical branches"
    );
    assert_eq!(connected.compiled_plan.branches.len(), 2);
    assert!(matches!(
        connected.compiled_plan.root,
        backend::search::BoolExpr::Any(ref branches) if branches.len() == 2
    ));

    let branch_local_budget =
        fixture.search_output("3BHK in Eastfield under 2Cr or 3BHK in Nextfield under 1.5Cr");
    assert_eq!(
        branch_local_budget.ast_branches.len(),
        2,
        "different budgets must remain independently explainable"
    );

    let disconnected =
        fixture.search_output("3BHK in Eastfield under 2Cr or 3BHK in Northfield under 2Cr");
    assert_eq!(
        disconnected.ast_branches.len(),
        2,
        "disconnected spatial intent must not collapse"
    );
}

#[test]
fn geography_first_execution_prioritizes_eligible_exact_societies() {
    let mut builder = FixtureBuilder::default();
    for spec in [
        HomeSpec::new(
            "geo-air",
            "Godrej Air",
            "Alpha",
            2,
            18_000_000,
            12.90,
            77.60,
        )
        .quality(Some(0.5), Some(4.0)),
        HomeSpec::new(
            "geo-alpha-prime",
            "Alpha Prime",
            "Alpha",
            2,
            19_000_000,
            12.91,
            77.61,
        )
        .quality(Some(0.5), Some(4.8)),
        HomeSpec::new(
            "geo-waterford",
            "Prestige Waterford",
            "Beta",
            3,
            24_000_000,
            12.92,
            77.62,
        )
        .quality(Some(0.5), Some(4.4)),
        HomeSpec::new(
            "geo-beta-prime",
            "Beta Prime",
            "Beta",
            3,
            23_000_000,
            12.93,
            77.63,
        )
        .quality(Some(0.5), Some(4.9)),
        HomeSpec::new(
            "geo-song",
            "Prestige Song of the South",
            "Gamma",
            3,
            26_000_000,
            12.94,
            77.64,
        )
        .quality(Some(0.5), Some(5.0)),
    ] {
        builder.add_home(spec);
    }
    builder.add_edge(
        "area:cell:area-alpha",
        "adjacent_area",
        "area:cell:area-beta",
    );
    builder.add_edge(
        "area:cell:area-beta",
        "adjacent_area",
        "area:cell:area-gamma",
    );
    builder.add_alias("Air Homes", "Godrej Air");
    let fixture = builder.build(false);

    let bare = fixture.search_output("Godrej Air");
    assert_eq!(
        bare.result_sets[0]
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["geo-air", "geo-alpha-prime"]
    );
    assert_eq!(
        bare.compiled_plan.branches[0]
            .geo_scope
            .market_locality_ids(),
        ["area:alpha"]
    );
    assert!(matches!(
        &bare.compiled_plan.branches[0].geo_scope,
        backend::search::GeoScope::Scoped { seed_cells, supporting_evidence, .. }
            if !seed_cells.is_empty() && !supporting_evidence.is_empty()
    ));

    let alias = fixture.search_output("Air Homes");
    assert_eq!(
        alias.result_sets[0]
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["geo-air", "geo-alpha-prime"],
        "a durable alias must retain the canonical society's priority"
    );

    let scoped = fixture.search_output(
        "Godrej Air 2BHK or Prestige Waterford 3BHK or Prestige Song of the South 3BHK",
    );
    assert_eq!(scoped.compiled_plan.branches.len(), 3);
    assert_eq!(
        scoped.compiled_plan.branches[0].geo_cluster_id,
        scoped.compiled_plan.branches[1].geo_cluster_id
    );
    assert_ne!(
        scoped.compiled_plan.branches[0].geo_cluster_id,
        scoped.compiled_plan.branches[2].geo_cluster_id,
        "direct adjacency must not become transitive clustering"
    );
    assert_eq!(
        scoped
            .result_sets
            .iter()
            .map(|set| set
                .results
                .iter()
                .map(|result| result.card.id.as_str())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec!["geo-air", "geo-alpha-prime"],
            vec!["geo-waterford", "geo-beta-prime"],
            vec!["geo-song"],
        ]
    );
    assert_eq!(
        scoped
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        [
            "geo-air",
            "geo-waterford",
            "geo-song",
            "geo-alpha-prime",
            "geo-beta-prime",
        ],
        "round-robin should expose each branch's exact society before area alternatives"
    );

    let bundle_wide = fixture.search_output("2BHK or 3BHK");
    assert!(bundle_wide
        .compiled_plan
        .branches
        .iter()
        .all(|branch| branch.geo_scope.is_bundle_wide()));
    assert_eq!(
        bundle_wide
            .result_sets
            .iter()
            .map(|set| set
                .results
                .iter()
                .map(|result| result.card.id.as_str())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec!["geo-alpha-prime", "geo-air"],
            vec!["geo-song", "geo-beta-prime", "geo-waterford"],
        ]
    );
}

#[test]
fn geo_cell_cohorts_keep_paths_branch_owned_and_evidenced() {
    let fixture = MockSearchFixture::for_kind(FixtureKind::GeoCells);
    let output = fixture.search_output("Godrej Air 3BHK");
    let by_id = output
        .results
        .iter()
        .map(|result| (result.card.id.as_str(), result))
        .collect::<HashMap<_, _>>();
    let expected = [
        (
            "geo-air-3",
            backend::search::GeographyMatchKind::ExactSociety,
            0,
        ),
        (
            "geo-alpha-prime-3",
            backend::search::GeographyMatchKind::SameMarketLocality,
            0,
        ),
        (
            "geo-same-cell-3",
            backend::search::GeographyMatchKind::CellNearby,
            0,
        ),
        (
            "geo-waterford-3",
            backend::search::GeographyMatchKind::CellNearby,
            1,
        ),
        (
            "geo-two-hop-3",
            backend::search::GeographyMatchKind::CellNearby,
            2,
        ),
    ];
    for (id, kind, hops) in expected {
        let geography = by_id[id]
            .geography_match
            .as_ref()
            .unwrap_or_else(|| panic!("{id} keeps structured geography metadata"));
        assert_eq!(geography.kind, kind, "{id}");
        assert_eq!(geography.hops, hops, "{id}");
        assert!(!geography.cell_path.is_empty(), "{id}");
        assert!(!geography.evidence_refs.is_empty(), "{id}");
        assert!(geography.distance_km.is_some(), "{id}");
        assert!(geography
            .cell_path
            .iter()
            .all(|cell| !cell.to_ascii_lowercase().contains("ward")));
    }
    assert!(!by_id.contains_key("geo-far-3"));
    assert!(!by_id.contains_key("geo-song-3"));

    let chained = fixture.search_output("Godrej Air 2BHK or Prestige Waterford 3BHK");
    assert_eq!(chained.result_sets.len(), 2);
    assert_eq!(chained.result_sets[0].branch_id, "branch-1");
    assert_eq!(chained.result_sets[0].results[0].card.id, "geo-air-2");
    assert_eq!(chained.result_sets[1].branch_id, "branch-2");
    assert_eq!(chained.result_sets[1].results[0].card.id, "geo-waterford-3");
    let serialized = serde_json::to_value(&chained.results[0]).unwrap();
    assert_eq!(
        serialized["geographyMatch"]["kind"],
        serde_json::json!("exact_society")
    );
    assert!(serialized["geographyMatch"]["evidenceRefs"]
        .as_array()
        .is_some_and(|refs| !refs.is_empty()));
}

#[test]
fn exact_society_priority_runs_after_hard_eligibility_and_survives_neighborhood_fallback() {
    let mut hard_builder = FixtureBuilder::default();
    hard_builder.add_home(
        HomeSpec::new(
            "criteria-anchor",
            "Criteria Anchor",
            "Criteria Area",
            2,
            21_000_000,
            12.90,
            77.60,
        )
        .quality(Some(0.5), Some(4.0)),
    );
    hard_builder.add_home(
        HomeSpec::new(
            "criteria-alternative",
            "Criteria Alternative",
            "Criteria Area",
            2,
            19_000_000,
            12.91,
            77.61,
        )
        .quality(Some(0.5), Some(4.8)),
    );
    let hard_fixture = hard_builder.build(false);
    let hard_filtered = hard_fixture.search_output("Criteria Anchor 2BHK under 2Cr");
    assert_eq!(
        hard_filtered
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["criteria-alternative"],
        "an exact society match must not bypass budget eligibility"
    );

    let mut dangling_builder = FixtureBuilder::default();
    dangling_builder.add_home(
        HomeSpec::new(
            "dangling-anchor",
            "Dangling Anchor",
            "Dangling Area",
            2,
            19_000_000,
            12.92,
            77.62,
        )
        .quality(Some(0.5), Some(4.0)),
    );
    dangling_builder.add_home(
        HomeSpec::new(
            "bundle-star",
            "Bundle Star",
            "Other Area",
            2,
            18_000_000,
            12.93,
            77.63,
        )
        .quality(Some(0.5), Some(4.9)),
    );
    dangling_builder.edges.retain(|edge| {
        !(edge.from_entity_id == "society:dangling-anchor"
            && edge.edge_type == "in_market_locality")
    });
    let dangling_fixture = dangling_builder.build(false);
    let dangling = dangling_fixture.search_output("Dangling Anchor 2BHK under 2Cr");
    assert!(dangling.compiled_plan.branches[0]
        .geo_scope
        .is_bundle_wide());
    assert_eq!(
        dangling
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["bundle-star", "dangling-anchor"],
        "missing geo-cell evidence falls back bundle-wide without coordinate-only scoping"
    );
}

#[test]
fn connected_hard_spatial_alternatives_preserve_any_of_semantics() {
    let mut builder = FixtureBuilder::default();
    builder.add_area("Sharedfield");
    builder.add_place("East Anchor", "landmark", 12.9000, 77.7000);
    builder.add_place("West Anchor", "landmark", 12.9000, 77.7300);
    builder.add_edge(
        "place:east-anchor",
        "in_market_locality",
        "area:sharedfield",
    );
    builder.add_edge(
        "place:west-anchor",
        "in_market_locality",
        "area:sharedfield",
    );
    builder.add_home(HomeSpec::new(
        "east-home",
        "East Homes",
        "Sharedfield",
        3,
        10_000_000,
        12.9000,
        77.7010,
    ));
    builder.add_home(HomeSpec::new(
        "west-home",
        "West Homes",
        "Sharedfield",
        3,
        10_000_000,
        12.9000,
        77.7290,
    ));
    let fixture = builder.build(false);

    let output = fixture.search_output(
        "3BHK within 1 km of East Anchor under 2Cr or 3BHK within 1 km of West Anchor under 2Cr",
    );
    assert_eq!(output.compiled_plan.branches.len(), 2);
    assert!(matches!(
        output.compiled_plan.root,
        backend::search::BoolExpr::Any(ref branches) if branches.len() == 2
    ));
    assert!(output
        .compiled_plan
        .branches
        .iter()
        .all(|branch| has_required_spatial_term(&branch.predicates)));
    let ids = output
        .results
        .iter()
        .map(|result| result.card.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(ids, HashSet::from(["east-home", "west-home"]));
}

#[test]
fn named_place_resolution_uses_sourced_area_context_and_fails_closed_without_it() {
    let mut builder = FixtureBuilder::default();
    builder.add_area("Whitefield");
    builder.add_area("Hebbal");
    builder.add_place_with_id(
        "place:manipal-whitefield",
        "Manipal Hospital",
        "hospital",
        12.9700,
        77.7350,
    );
    builder.add_place_with_id(
        "place:manipal-hebbal",
        "Manipal Hospital",
        "hospital",
        13.0500,
        77.5950,
    );
    builder.add_edge(
        "place:manipal-whitefield",
        "in_market_locality",
        "area:whitefield",
    );
    builder.add_edge("place:manipal-hebbal", "in_market_locality", "area:hebbal");
    builder.add_edge("place:manipal-whitefield", "in_area", "area:whitefield");
    builder.add_edge("place:manipal-hebbal", "in_area", "area:hebbal");
    builder.add_home(HomeSpec::new(
        "whitefield-hospital-home",
        "Whitefield Hospital Homes",
        "Whitefield",
        3,
        10_000_000,
        12.9705,
        77.7355,
    ));
    builder.add_home(HomeSpec::new(
        "hebbal-hospital-home",
        "Hebbal Hospital Homes",
        "Hebbal",
        3,
        10_000_000,
        13.0505,
        77.5955,
    ));
    let fixture = builder.build(false);
    let canonical_whitefield_hospital_id = fixture
        .bundle
        .edges
        .iter()
        .find(|edge| {
            edge.edge_type == PROVIDER_BINDING_EDGE
                && edge.to_entity_id == "place:manipal-whitefield"
        })
        .map(|edge| edge.from_entity_id.as_str())
        .expect("fixture retains sourced Whitefield hospital topology");

    let scoped = fixture.search_output("3BHK near Manipal Hospital in Whitefield under 2Cr");
    assert_eq!(
        scoped
            .diagnostics
            .resolved
            .entities
            .iter()
            .filter(|entity| entity.entity_type == "place")
            .map(|entity| entity.entity_id.as_str())
            .collect::<Vec<_>>(),
        [canonical_whitefield_hospital_id]
    );
    assert_eq!(
        scoped
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["whitefield-hospital-home"]
    );

    let ambiguous = fixture.search_output("3BHK near Manipal Hospital under 2Cr");
    assert!(ambiguous.results.is_empty());
    assert!(ambiguous
        .diagnostics
        .warnings
        .iter()
        .any(|warning| warning.to_ascii_lowercase().contains("manipal hospital")));
}

#[test]
fn unknown_specific_place_never_falls_back_to_same_named_area_or_place_family() {
    let mut builder = FixtureBuilder::default();
    builder.add_area("Hoodi");
    builder.add_place("Other Metro", "metro", 12.9000, 77.6000);
    builder.add_home(HomeSpec::new(
        "other-metro-home",
        "Other Metro Homes",
        "Elsewhere",
        3,
        10_000_000,
        12.9005,
        77.6005,
    ));
    let fixture = builder.build(false);

    let output = fixture.search_output("3BHK near Hoodi Metro under 2Cr");

    assert!(output.results.is_empty());
    assert!(output
        .diagnostics
        .warnings
        .iter()
        .any(|warning| warning.to_ascii_lowercase().contains("hoodi metro")));
    assert!(output
        .diagnostics
        .resolved
        .entities
        .iter()
        .all(|entity| entity.entity_id != "area:hoodi" && entity.entity_id != "place:other-metro"));
}

#[test]
fn inside_requires_sourced_containment_and_cannot_use_nearby_coordinates() {
    let mut builder = FixtureBuilder::default();
    builder.add_area("Hoodi");
    builder.add_fact(
        "area:hoodi",
        "geo.geometry_geojson",
        FactValue::Text(
            r#"{"type":"Polygon","coordinates":[[[77.70,12.97],[77.73,12.97],[77.73,13.00],[77.70,13.00],[77.70,12.97]]]}"#
                .to_string(),
        ),
    );
    builder.add_home(HomeSpec::new(
        "sourced-inside-home",
        "Sourced Inside",
        "Hoodi",
        3,
        10_000_000,
        12.985,
        77.715,
    ));
    builder.add_fact(
        "society:sourced-inside",
        "geo.geometry_geojson",
        FactValue::Text(
            r#"{"type":"Polygon","coordinates":[[[77.714,12.984],[77.716,12.984],[77.716,12.986],[77.714,12.986],[77.714,12.984]]]}"#
                .to_string(),
        ),
    );
    builder.add_home(HomeSpec::new(
        "coordinate-only-home",
        "Coordinate Only",
        "Hoodi",
        3,
        10_000_000,
        12.986,
        77.716,
    ));
    let fixture = builder.build(false);

    let output = fixture.search_output("3BHK inside Hoodi under 2Cr");
    assert_eq!(
        output
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        ["sourced-inside-home"]
    );
    assert!(output.results[0].verified_matches.iter().any(|matched| {
        matched.relation == "inside"
            && matched.metric == "footprint_containment"
            && matched.target_entity_id.as_deref() == Some("area:hoodi")
    }));
    let spatial_match = output.results[0]
        .verified_matches
        .iter()
        .find(|matched| matched.metric == "footprint_containment")
        .expect("containment match keeps its derivation");
    assert!(spatial_match.observation_ids.is_empty());
    assert!(matches!(
        spatial_match.evidence_refs.as_slice(),
        [reference] if matches!(reference.evidence_id, EvidenceId::Derivation(_))
    ));
    let derivation = spatial_match
        .derived_evidence
        .as_ref()
        .expect("spatial proof carries its source inputs");
    assert_eq!(derivation.input_evidence.len(), 2);
    assert!(derivation.validate().is_ok());
    let inventory_matches = output.results[0]
        .verified_matches
        .iter()
        .filter(|matched| matched.algorithm_version == "inventory-option-evaluator-v2")
        .collect::<Vec<_>>();
    assert_eq!(inventory_matches.len(), 2);
    assert!(inventory_matches
        .iter()
        .any(|matched| matched.metric == "inventory_option_bhk" && matched.value == Some(3.0)));
    assert!(inventory_matches.iter().any(|matched| {
        matched.metric == "inventory_option_price_min" && matched.value == Some(10_000_000.0)
    }));
    let evidence = &inventory_matches[0].evidence_refs;
    assert_eq!(evidence.len(), 1);
    assert!(inventory_matches
        .iter()
        .all(|matched| matched.evidence_refs == *evidence));
    assert!(evidence[0]
        .validate_for("society:sourced-inside", "conversational-semantics-mock")
        .is_ok());
}

fn has_required_spatial_term(expression: &backend::search::ConstraintExpr) -> bool {
    match expression {
        backend::search::ConstraintExpr::And { clauses }
        | backend::search::ConstraintExpr::AnyOf { clauses } => {
            clauses.iter().any(has_required_spatial_term)
        }
        backend::search::ConstraintExpr::Not { clause } => has_required_spatial_term(clause),
        backend::search::ConstraintExpr::Term {
            term: backend::search::ConstraintTerm::Spatial { required, .. },
        } => *required,
        backend::search::ConstraintExpr::Term { .. } => false,
    }
}

fn run_controlled_journey(
    journey: &ControlledJourneyCase,
    atomic_cases: &HashMap<String, ControlledQueryCase>,
    limits: JourneyLimits,
) {
    assert!(
        journey.turns.len() + 1 <= limits.max_revision_depth_before_checkpoint,
        "{} exceeds the revision checkpoint limit",
        journey.id
    );
    let fixture = MockSearchFixture::for_kind(journey.fixture);
    let mut active_case = atomic_case(atomic_cases, &journey.initial_case_id, journey);
    assert_eq!(
        active_case.fixture, journey.fixture,
        "{} starts on a different fixture",
        journey.id
    );
    let mut active_output = fixture.search(&active_case.query);
    let mut active_query = active_case.query.clone();
    let mut active_branch_count = active_output.result_sets.len().max(1);
    assert_controlled_expectation(active_case, &active_output);
    assert_branch_limit(&journey.id, &active_output, limits.max_active_branches);

    for (turn_index, turn) in journey.turns.iter().enumerate() {
        assert!(
            !turn.utterance.trim().is_empty(),
            "{} has an empty turn",
            journey.id
        );
        let revision = compile_search_revision(
            &active_query,
            &turn.utterance,
            active_branch_count,
            SearchRevisionLimits {
                max_active_branches: limits.max_active_branches,
            },
        );
        assert_eq!(
            observed_operation(revision.operation),
            turn.operation,
            "{} turn {turn_index} compiled the wrong operation",
            journey.id
        );
        let candidate = turn.candidate_case_id.as_deref().map(|case_id| {
            evaluate_candidate(
                journey,
                turn_index,
                turn,
                atomic_cases,
                &fixture,
                &active_output,
                case_id,
                revision
                    .candidate_query
                    .as_deref()
                    .expect("candidate turn must compile a query"),
            )
        });
        assert_revision_outcome(journey, turn_index, turn.outcome, revision.outcome);
        let expected_active_id = expected_active_case_id(journey, turn, active_case, &candidate);
        assert_eq!(
            turn.active_case_id, expected_active_id,
            "{} turn {turn_index} activates the wrong atomic intent",
            journey.id
        );

        if let JourneyOutcome::ActivateCandidate = turn.outcome {
            let (candidate_case, candidate_output) = candidate.expect("candidate was checked");
            active_case = candidate_case;
            active_output = candidate_output;
            active_query = revision
                .candidate_query
                .expect("activated turn retains its compiled query");
            active_branch_count = revision.candidate_branch_count;
            assert_branch_limit(&journey.id, &active_output, limits.max_active_branches);
        }
    }
}

fn evaluate_candidate<'a>(
    journey: &ControlledJourneyCase,
    turn_index: usize,
    turn: &JourneyTurn,
    atomic_cases: &'a HashMap<String, ControlledQueryCase>,
    fixture: &MockSearchFixture,
    active_output: &ObservedSearch,
    case_id: &str,
    candidate_query: &str,
) -> (&'a ControlledQueryCase, ObservedSearch) {
    let candidate_case = atomic_case(atomic_cases, case_id, journey);
    assert_eq!(
        candidate_case.fixture, journey.fixture,
        "{} turn {turn_index} crosses controlled fixtures",
        journey.id
    );
    let output = fixture.search(candidate_query);
    assert_controlled_expectation(candidate_case, &output);
    let reference_output = fixture.search(&candidate_case.query);
    assert_same_search_observation(
        &journey.id,
        turn_index,
        candidate_query,
        &output,
        &reference_output,
    );
    if let Some(effect) = turn.candidate_effect {
        assert_candidate_effect(&journey.id, turn_index, effect, active_output, &output);
    }
    (candidate_case, output)
}

fn expected_active_case_id<'a>(
    journey: &ControlledJourneyCase,
    turn: &JourneyTurn,
    active_case: &'a ControlledQueryCase,
    candidate: &Option<(&'a ControlledQueryCase, ObservedSearch)>,
) -> &'a str {
    match turn.outcome {
        JourneyOutcome::ActivateCandidate => candidate
            .as_ref()
            .map(|(case, _)| case.id.as_str())
            .expect("activated journey turn requires a candidate"),
        JourneyOutcome::PreserveParent => {
            assert!(
                candidate.is_some(),
                "preserved turn must evaluate a candidate"
            );
            active_case.id.as_str()
        }
        JourneyOutcome::RequireClarification => {
            assert_eq!(turn.operation, JourneyOperation::Expand, "{}", journey.id);
            assert!(
                candidate.is_none(),
                "clarification must not execute an unbounded candidate query"
            );
            active_case.id.as_str()
        }
        JourneyOutcome::RequireCheckpoint => {
            assert_eq!(turn.operation, JourneyOperation::Expand, "{}", journey.id);
            assert!(
                candidate.is_none(),
                "checkpoint must not execute a candidate"
            );
            active_case.id.as_str()
        }
    }
}

fn observed_operation(operation: SearchRevisionOperation) -> JourneyOperation {
    match operation {
        SearchRevisionOperation::Refine => JourneyOperation::Refine,
        SearchRevisionOperation::Rephrase => JourneyOperation::Rephrase,
        SearchRevisionOperation::Expand => JourneyOperation::Expand,
        SearchRevisionOperation::Switch => JourneyOperation::Switch,
        SearchRevisionOperation::Replace => JourneyOperation::Replace,
    }
}

fn assert_revision_outcome(
    journey: &ControlledJourneyCase,
    turn_index: usize,
    expected: JourneyOutcome,
    actual: SearchRevisionOutcome,
) {
    let valid = matches!(
        (expected, actual),
        (
            JourneyOutcome::ActivateCandidate,
            SearchRevisionOutcome::Candidate
        ) | (
            JourneyOutcome::PreserveParent,
            SearchRevisionOutcome::Candidate
        ) | (
            JourneyOutcome::RequireClarification,
            SearchRevisionOutcome::RequireClarification
        ) | (
            JourneyOutcome::RequireCheckpoint,
            SearchRevisionOutcome::RequireCheckpoint
        )
    );
    assert!(
        valid,
        "{} turn {turn_index} expected {expected:?}, compiled {actual:?}",
        journey.id
    );
}

fn assert_same_search_observation(
    journey_id: &str,
    turn_index: usize,
    candidate_query: &str,
    actual: &ObservedSearch,
    expected: &ObservedSearch,
) {
    assert_eq!(
        owned_result_ids(actual),
        owned_result_ids(expected),
        "{journey_id} turn {turn_index} compiled a different ranking: {candidate_query}"
    );
    assert_eq!(
        actual.result_sets.len(),
        expected.result_sets.len(),
        "{journey_id} turn {turn_index} compiled different branches: {candidate_query}"
    );
    assert_eq!(
        actual.areas, expected.areas,
        "{journey_id} turn {turn_index}"
    );
    assert_eq!(actual.bhks, expected.bhks, "{journey_id} turn {turn_index}");
    assert_eq!(
        actual.budget_max, expected.budget_max,
        "{journey_id} turn {turn_index}"
    );
    assert_eq!(
        actual.positive_preferences, expected.positive_preferences,
        "{journey_id} turn {turn_index}"
    );
    assert_eq!(
        actual.negative_preferences, expected.negative_preferences,
        "{journey_id} turn {turn_index}"
    );
    assert_eq!(
        actual.ranking_priorities, expected.ranking_priorities,
        "{journey_id} turn {turn_index}"
    );
}

fn atomic_case<'a>(
    cases: &'a HashMap<String, ControlledQueryCase>,
    case_id: &str,
    journey: &ControlledJourneyCase,
) -> &'a ControlledQueryCase {
    cases
        .get(case_id)
        .unwrap_or_else(|| panic!("{} references missing atomic case {case_id}", journey.id))
}

fn assert_branch_limit(journey_id: &str, output: &ObservedSearch, max_branches: usize) {
    assert!(
        output.result_sets.len() <= max_branches,
        "{journey_id} activated {} branches above the limit {max_branches}",
        output.result_sets.len()
    );
}

fn owned_result_ids(output: &ObservedSearch) -> Vec<String> {
    output
        .result_sets
        .iter()
        .flatten()
        .map(|result| result.id.clone())
        .collect()
}

fn assert_candidate_effect(
    journey_id: &str,
    turn_index: usize,
    effect: CandidateEffect,
    parent: &ObservedSearch,
    candidate: &ObservedSearch,
) {
    let parent_ids = owned_result_ids(parent);
    let candidate_ids = owned_result_ids(candidate);
    let parent_set = parent_ids.iter().collect::<HashSet<_>>();
    let candidate_set = candidate_ids.iter().collect::<HashSet<_>>();
    let valid = match effect {
        CandidateEffect::Narrower => {
            !candidate_set.is_empty()
                && candidate_set.len() < parent_set.len()
                && candidate_set.is_subset(&parent_set)
        }
        CandidateEffect::SameMembership => candidate_set == parent_set,
        CandidateEffect::SameOrder => candidate_ids == parent_ids,
        CandidateEffect::AddsBranch => {
            candidate.result_sets.len() > parent.result_sets.len()
                && parent_set.is_subset(&candidate_set)
        }
        CandidateEffect::DifferentMembership => {
            !candidate_set.is_empty() && parent_set.is_disjoint(&candidate_set)
        }
        CandidateEffect::ZeroResults => candidate_ids.is_empty(),
    };
    assert!(
        valid,
        "{journey_id} turn {turn_index} expected {effect:?}; parent={parent_ids:?}, candidate={candidate_ids:?}"
    );
}

fn result_ids(output: &ObservedSearch) -> Vec<&str> {
    output
        .ordered_result_ids
        .iter()
        .map(String::as_str)
        .collect()
}

fn assert_controlled_expectation(case: &ControlledQueryCase, output: &ObservedSearch) {
    let expectation = &case.fixture_expectation;
    if let Some(expected_sets) = &expectation.result_sets {
        let actual_sets = output
            .result_sets
            .iter()
            .map(|result_set| {
                result_set
                    .iter()
                    .map(|result| result.id.as_str())
                    .collect::<Vec<_>>()
            })
            .filter(|result_set| !result_set.is_empty())
            .collect::<Vec<_>>();
        let expected_sets = expected_sets
            .iter()
            .map(|result_set| result_set.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(
            actual_sets, expected_sets,
            "{} returned unexpected branches; warnings={:?}",
            case.id, output.warnings
        );
    }

    let actual_ids = result_ids(output);
    if let Some(first_id) = &expectation.first_id {
        assert_eq!(
            actual_ids.first().copied(),
            Some(first_id.as_str()),
            "{} returned unexpected first result; all={actual_ids:?}; warnings={:?}",
            case.id,
            output.warnings
        );
    }
    for expected_id in &expectation.includes {
        assert!(
            actual_ids.contains(&expected_id.as_str()),
            "{} is missing {expected_id}; actual={actual_ids:?}; warnings={:?}",
            case.id,
            output.warnings
        );
    }
    for excluded_id in &expectation.excludes {
        assert!(
            !actual_ids.contains(&excluded_id.as_str()),
            "{} unexpectedly returned {excluded_id}; actual={actual_ids:?}",
            case.id
        );
    }
    if !expectation.ordered_prefix.is_empty() {
        let expected_prefix = expectation
            .ordered_prefix
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            actual_ids.get(..expected_prefix.len()),
            Some(expected_prefix.as_slice()),
            "{} returned unexpected ranking; actual={actual_ids:?}; warnings={:?}",
            case.id,
            output.warnings
        );
    }
    if let Some(max_results) = expectation.max_total_results {
        assert!(
            actual_ids.len() <= max_results,
            "{} returned {} homes above the global limit {max_results}",
            case.id,
            actual_ids.len()
        );
    }
    if let Some(expected_count) = expectation.non_empty_result_sets {
        assert_eq!(
            output.result_sets.len(),
            expected_count,
            "{} returned the wrong number of non-empty result sets",
            case.id
        );
    }
    if expectation.unique_result_ids == Some(true) {
        let mut unique_ids = actual_ids.clone();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        assert_eq!(
            unique_ids.len(),
            actual_ids.len(),
            "{} repeated a home across result sets: {actual_ids:?}",
            case.id
        );
    }
    if !expectation.result_set_counts.is_empty() {
        let counts = output.result_sets.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(
            counts, expectation.result_set_counts,
            "{} returned an unfair branch distribution",
            case.id
        );
    }
    if !expectation.result_set_ordered_prefixes.is_empty() {
        assert_eq!(
            output.result_sets.len(),
            expectation.result_set_ordered_prefixes.len(),
            "{} returned the wrong number of ordered branches",
            case.id
        );
        for (index, expected_prefix) in expectation.result_set_ordered_prefixes.iter().enumerate() {
            let actual = output.result_sets[index]
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>();
            let expected_prefix = expected_prefix
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            assert!(
                actual.starts_with(&expected_prefix),
                "{} branch {index} expected ordered prefix {expected_prefix:?}, got {actual:?}",
                case.id
            );
        }
    }

    let semantics = &case.expected_semantics;
    if let Some(expected) = &semantics.positive_preferences {
        for preference in expected {
            assert!(
                output.positive_preferences.contains(preference),
                "{} did not compile positive preference {preference:?}; actual={:?}",
                case.id,
                output.positive_preferences
            );
        }
    }
    if let Some(expected) = &semantics.negative_preferences {
        assert_eq!(
            &output.negative_preferences, expected,
            "{} compiled the wrong negative preferences",
            case.id
        );
    }
    if let Some(expected) = &semantics.accepted_tradeoffs {
        assert_eq!(
            &output.accepted_tradeoffs, expected,
            "{} compiled the wrong accepted tradeoffs",
            case.id
        );
    }
    if let Some(expected) = &semantics.ranking_priorities {
        assert_eq!(
            &output.ranking_priorities, expected,
            "{} compiled the wrong ranking priorities",
            case.id
        );
    }
    if let Some(expected) = &semantics.numeric_min {
        assert!(
            output.min_constraints.iter().any(|(field, value)| {
                field.eq_ignore_ascii_case(&expected.field)
                    && (*value - expected.value).abs() < f64::EPSILON
            }),
            "{} did not compile minimum constraint {} >= {}; actual={:?}",
            case.id,
            expected.field,
            expected.value,
            output.min_constraints
        );
    }

    if let Some(expected) = &semantics.area {
        assert!(
            output
                .areas
                .iter()
                .any(|area| area.eq_ignore_ascii_case(expected)),
            "{} did not resolve area {expected:?}; actual={:?}",
            case.id,
            output.areas
        );
        if semantics.branches.is_none() && semantics.allows_cell_nearby != Some(true) {
            assert!(
                output
                    .result_sets
                    .iter()
                    .flatten()
                    .all(|result| result.area.eq_ignore_ascii_case(expected)),
                "{} returned a home outside area {expected:?}; actual={actual_ids:?}",
                case.id
            );
        }
    }
    if let Some(expected) = semantics.bhk {
        assert!(
            output.bhks.contains(&expected),
            "{} did not compile {expected} BHK; actual={:?}",
            case.id,
            output.bhks
        );
        if semantics.branches.is_none() {
            assert!(
                output
                    .result_sets
                    .iter()
                    .flatten()
                    .all(|result| result.bhk == expected),
                "{} returned a home outside {expected} BHK; actual={actual_ids:?}",
                case.id
            );
        }
    }
    if let Some(expected) = &semantics.bhks {
        assert_eq!(
            &output.bhks, expected,
            "{} compiled the wrong BHK alternatives",
            case.id
        );
        if semantics.branches.is_none() {
            assert!(
                output
                    .result_sets
                    .iter()
                    .flatten()
                    .all(|result| expected.contains(&result.bhk)),
                "{} returned an unrequested BHK; actual={actual_ids:?}",
                case.id
            );
        }
    }
    if let Some(expected) = semantics.budget_max {
        assert_eq!(
            output.budget_max,
            Some(expected),
            "{} compiled the wrong maximum budget",
            case.id
        );
        if semantics.branches.is_none() {
            assert!(
                output
                    .result_sets
                    .iter()
                    .flatten()
                    .all(|result| result.price <= expected),
                "{} returned a home above budget {expected}; actual={actual_ids:?}",
                case.id
            );
        }
    }
    if let Some(expected) = &semantics.society {
        assert_resolved_entity(case, output, "society", expected);
    }
    if let Some(expected) = &semantics.near {
        assert!(
            output.resolved_entities.iter().any(|entity| {
                entity.entity_type.eq_ignore_ascii_case("place")
                    && (entity.name.eq_ignore_ascii_case(expected)
                        || entity.matched_text.eq_ignore_ascii_case(expected))
            }) || output
                .result_sets
                .iter()
                .flatten()
                .flat_map(|result| &result.proof_labels)
                .any(|label| label.eq_ignore_ascii_case(expected)),
            "{} did not resolve named place {expected:?}; observed={output:?}",
            case.id
        );
    }
    if let Some(expected) = &semantics.place_family {
        assert_resolved_entity(case, output, "place_family", expected);
    }
    if let Some(expected) = &semantics.unresolved_society {
        assert!(
            output.warnings.iter().any(|warning| warning
                .to_ascii_lowercase()
                .contains(&expected.to_ascii_lowercase())),
            "{} did not report unresolved society {expected:?}; warnings={:?}",
            case.id,
            output.warnings
        );
    }
    if let Some(expected) = &semantics.home_state {
        assert!(!actual_ids.is_empty(), "{} returned no homes", case.id);
        assert!(
            output
                .result_sets
                .iter()
                .flatten()
                .all(|result| equivalent_home_state(&result.home_state, expected)),
            "{} returned a home outside state {expected:?}",
            case.id
        );
    }
    if let Some(excluded) = &semantics.exclude_home_state {
        assert!(
            output
                .result_sets
                .iter()
                .flatten()
                .all(|result| !equivalent_home_state(&result.home_state, excluded)),
            "{} returned excluded home state {excluded:?}",
            case.id
        );
    }
    if let Some(distance_km) = semantics.distance_max_km {
        for result in output.result_sets.iter().flatten() {
            assert!(
                !result.proof_distances_m.is_empty(),
                "{} result {} has no distance receipt for the {distance_km} km constraint",
                case.id,
                result.id
            );
            assert!(
                result
                    .proof_distances_m
                    .iter()
                    .all(|distance| f64::from(*distance) <= distance_km * 1_000.0),
                "{} result {} did not enforce {distance_km} km; proof distances={:?}",
                case.id,
                result.id,
                result.proof_distances_m
            );
        }
    }
    if semantics.abstain == Some(true) {
        assert!(
            actual_ids.is_empty(),
            "{} should abstain but returned {actual_ids:?}",
            case.id
        );
    }
    if semantics.missing_optional_evidence == Some(MissingOptionalEvidence::IncludeWithoutClaim) {
        for preference in semantics
            .positive_preferences
            .as_deref()
            .unwrap_or_default()
        {
            assert!(
                output.result_sets.iter().flatten().any(|result| result
                    .preference_coverage
                    .iter()
                    .any(|(actual, status)| actual == preference && status == "no_data")),
                "{} did not keep a no-data result for optional preference {preference:?}",
                case.id
            );
        }
    }
    if let Some(branches) = &semantics.branches {
        assert_eq!(
            output.result_sets.len(),
            branches.len(),
            "{} compiled the wrong branch count",
            case.id
        );
        for (index, (branch, results)) in branches.iter().zip(&output.result_sets).enumerate() {
            assert!(!results.is_empty(), "{} branch {index} is empty", case.id);
            assert!(
                results.iter().all(|result| {
                    (branch.allows_cell_nearby == Some(true)
                        || (branch
                            .area
                            .as_ref()
                            .is_none_or(|area| result.area.eq_ignore_ascii_case(area))
                            && branch.society.as_ref().is_none_or(|society| {
                                result.society.eq_ignore_ascii_case(society)
                            })))
                        && branch.bhk.is_none_or(|bhk| result.bhk == bhk)
                        && branch
                            .budget_max
                            .is_none_or(|budget_max| result.price <= budget_max)
                }),
                "{} branch {index} violated its declared scope/BHK/budget contract",
                case.id
            );
        }
    }
    for (result_id, required_labels) in &expectation.required_proof_labels {
        let result = output
            .result_sets
            .iter()
            .flatten()
            .find(|result| &result.id == result_id)
            .unwrap_or_else(|| panic!("{} is missing proof-bearing result {result_id}", case.id));
        for required_label in required_labels {
            assert!(
                result
                    .proof_labels
                    .iter()
                    .any(|actual| actual.contains(required_label)),
                "{} result {result_id} does not prove {required_label:?}; actual={:?}",
                case.id,
                result.proof_labels
            );
        }
    }
    for forbidden_label in &expectation.forbidden_proof_labels {
        assert!(
            output
                .result_sets
                .iter()
                .flatten()
                .flat_map(|result| {
                    result
                        .proof_labels
                        .iter()
                        .chain(result.claimed_preferences.iter())
                })
                .all(|label| !label.to_ascii_lowercase().contains(forbidden_label)),
            "{} fabricated forbidden proof {forbidden_label:?}",
            case.id
        );
    }
}

fn assert_resolved_entity(
    case: &ControlledQueryCase,
    output: &ObservedSearch,
    entity_type: &str,
    expected: &str,
) {
    assert!(
        output.resolved_entities.iter().any(|entity| {
            entity.entity_type.eq_ignore_ascii_case(entity_type)
                && (entity.name.eq_ignore_ascii_case(expected)
                    || entity.matched_text.eq_ignore_ascii_case(expected))
        }),
        "{} did not resolve {entity_type} {expected:?}; actual={:?}",
        case.id,
        output.resolved_entities
    );
}

fn equivalent_home_state(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
        || (expected.eq_ignore_ascii_case("ready_to_move")
            && actual.eq_ignore_ascii_case("delivered"))
}

#[derive(Debug)]
struct ObservedSearch {
    ordered_result_ids: Vec<String>,
    result_sets: Vec<Vec<ObservedResult>>,
    warnings: Vec<String>,
    positive_preferences: Vec<String>,
    negative_preferences: Vec<String>,
    ranking_priorities: Vec<String>,
    accepted_tradeoffs: Vec<String>,
    min_constraints: Vec<(String, f64)>,
    areas: Vec<String>,
    bhks: Vec<u32>,
    budget_max: Option<u64>,
    resolved_entities: Vec<ObservedResolvedEntity>,
}

#[derive(Debug)]
struct ObservedResult {
    id: String,
    society: String,
    area: String,
    bhk: u32,
    price: u64,
    home_state: String,
    proof_labels: Vec<String>,
    proof_distances_m: Vec<u32>,
    claimed_preferences: Vec<String>,
    preference_coverage: Vec<(String, String)>,
}

#[derive(Debug)]
struct ObservedResolvedEntity {
    entity_type: String,
    name: String,
    matched_text: String,
}

struct MockSearchFixture {
    properties: Vec<Property>,
    bundle: Arc<LoadedServingBundle>,
}

#[derive(Clone, Copy, Default)]
struct FixtureProfile {
    decision_candidates: bool,
    derive_proximity: bool,
    distance_decoy: bool,
    multi_or_decoys_per_bhk: usize,
    regional_inventory: bool,
}

impl MockSearchFixture {
    fn for_kind(kind: FixtureKind) -> Self {
        let profile = match kind {
            FixtureKind::Core => FixtureProfile::default(),
            FixtureKind::BuyerLanguage => FixtureProfile {
                distance_decoy: true,
                ..FixtureProfile::default()
            },
            FixtureKind::DecisionRanking => FixtureProfile {
                decision_candidates: true,
                ..FixtureProfile::default()
            },
            FixtureKind::MultiOr => FixtureProfile {
                multi_or_decoys_per_bhk: 24,
                ..FixtureProfile::default()
            },
            FixtureKind::Proximity => FixtureProfile {
                derive_proximity: true,
                ..FixtureProfile::default()
            },
            FixtureKind::Regional => FixtureProfile {
                regional_inventory: true,
                ..FixtureProfile::default()
            },
            FixtureKind::GeoCells => return Self::geo_cells(),
        };
        Self::build(profile)
    }

    fn geo_cells() -> Self {
        let mut builder = FixtureBuilder::default();
        for market in ["Alpha", "Beta", "Gamma", "Delta"] {
            builder.add_area(market);
        }
        builder.add_geo_cell("area:cell:a", 77.00, 77.01);
        builder.add_geo_cell("area:cell:b", 77.01, 77.02);
        builder.add_geo_cell("area:cell:c", 77.02, 77.03);
        builder.add_geo_cell("area:cell:far", 77.10, 77.11);
        builder.add_geo_cell("area:cell:disconnected", 78.00, 78.01);
        builder.add_place("Anchor School", "school", 12.905, 77.005);
        for spec in [
            HomeSpec::new(
                "geo-air-2",
                "Godrej Air",
                "Alpha",
                2,
                21_000_000,
                12.905,
                77.005,
            ),
            HomeSpec::new(
                "geo-air-3",
                "Godrej Air",
                "Alpha",
                3,
                23_000_000,
                12.905,
                77.005,
            ),
            HomeSpec::new(
                "geo-alpha-value-2",
                "Alpha Value",
                "Alpha",
                2,
                19_000_000,
                12.905,
                77.006,
            ),
            HomeSpec::new(
                "geo-alpha-prime-3",
                "Alpha Prime",
                "Alpha",
                3,
                22_000_000,
                12.905,
                77.007,
            ),
            HomeSpec::new(
                "geo-same-cell-3",
                "Same Cell",
                "Beta",
                3,
                20_000_000,
                12.905,
                77.008,
            ),
            HomeSpec::new(
                "geo-waterford-3",
                "Prestige Waterford",
                "Beta",
                3,
                24_000_000,
                12.905,
                77.015,
            ),
            HomeSpec::new(
                "geo-two-hop-3",
                "Two Hop",
                "Gamma",
                3,
                20_000_000,
                12.905,
                77.025,
            ),
            HomeSpec::new(
                "geo-far-3",
                "Far Cell",
                "Gamma",
                3,
                18_000_000,
                12.905,
                77.105,
            ),
            HomeSpec::new(
                "geo-song-3",
                "Prestige Song of the South",
                "Delta",
                3,
                20_000_000,
                12.905,
                78.005,
            ),
        ] {
            builder.add_home(spec);
        }
        for (subject, cell) in [
            ("place:anchor-school", "area:cell:a"),
            ("society:godrej-air", "area:cell:a"),
            ("society:alpha-value", "area:cell:a"),
            ("society:alpha-prime", "area:cell:a"),
            ("society:same-cell", "area:cell:a"),
            ("society:prestige-waterford", "area:cell:b"),
            ("society:two-hop", "area:cell:c"),
            ("society:far-cell", "area:cell:far"),
            (
                "society:prestige-song-of-the-south",
                "area:cell:disconnected",
            ),
        ] {
            builder.add_edge(subject, "occupies_geo_cell", cell);
        }
        for (market, cell) in [
            ("area:alpha", "area:cell:a"),
            ("area:beta", "area:cell:a"),
            ("area:beta", "area:cell:b"),
            ("area:gamma", "area:cell:c"),
            ("area:gamma", "area:cell:far"),
            ("area:delta", "area:cell:disconnected"),
        ] {
            builder.add_edge(market, "covers_geo_cell", cell);
        }
        builder.add_edge("area:cell:a", "adjacent_area", "area:cell:b");
        builder.add_edge("area:cell:b", "adjacent_area", "area:cell:c");
        builder.add_edge("area:cell:b", "adjacent_area", "area:cell:far");
        builder.build(true)
    }

    fn build(profile: FixtureProfile) -> Self {
        let mut builder = FixtureBuilder::default();
        builder.add_place("Hoodi Metro", "metro", 12.9900, 77.7150);
        builder.add_place("Manipal Hospital", "hospital", 12.9700, 77.7350);
        builder.add_place("Manipal Hospital Whitefield", "hospital", 12.9690, 77.7340);
        builder.add_place("Bagmane Tech Park", "tech_park", 12.9800, 77.6600);
        builder.add_place("Gopalan National School", "school", 12.9500, 77.6400);
        builder.add_place("Cult Fitness Club", "fitness", 12.9910, 77.7160);
        builder.add_fact(
            "place:cult-fitness-club",
            "place.types",
            FactValue::Tags(vec![
                "fitness_center".to_string(),
                "gym".to_string(),
                "health".to_string(),
                "school".to_string(),
            ]),
        );
        builder.add_place("Mock Metro Station", "metro", 12.8500, 77.6000);
        if profile.regional_inventory {
            add_regional_inventory(&mut builder);
        }
        if profile.multi_or_decoys_per_bhk > 0 {
            builder.add_area("Hoodi");
        }

        builder.add_home(HomeSpec::new(
            "mock-godrej-air-3bhk",
            "Godrej Air",
            "Hoodi",
            3,
            23_000_000,
            12.9910,
            77.7160,
        ));
        if profile.multi_or_decoys_per_bhk > 0 {
            builder.add_home(HomeSpec::new(
                "mock-godrej-air-2bhk",
                "Godrej Air",
                "Hoodi",
                2,
                18_500_000,
                12.9910,
                77.7160,
            ));
            add_multi_or_decoys(&mut builder, profile.multi_or_decoys_per_bhk);
        }
        builder.add_home(HomeSpec::new(
            "mock-lakeside-orchard-4bhk",
            "Godrej Lakeside Orchard",
            "Sarjapur Road",
            4,
            30_500_000,
            12.9000,
            77.7000,
        ));
        builder.add_home(HomeSpec::new(
            "mock-bagmane-small-3bhk",
            "Bagmane Neighbourhood",
            "CV Raman Nagar",
            3,
            22_000_000,
            12.9810,
            77.6610,
        ));
        builder.add_home(HomeSpec::new(
            "mock-far-delivered-4bhk",
            "Farther Family Homes",
            "Hosa Road",
            4,
            28_000_000,
            12.8800,
            77.6500,
        ));
        builder.add_home(
            HomeSpec::new(
                "mock-bagmane-under-construction-3bhk",
                "Bagmane Future Homes",
                "CV Raman Nagar",
                3,
                20_000_000,
                12.9820,
                77.6620,
            )
            .under_construction(),
        );
        builder.add_home(HomeSpec::new(
            "mock-dual-place-3bhk",
            "Dual Place Homes",
            "Whitefield",
            3,
            26_000_000,
            12.9800,
            77.7250,
        ));
        builder.add_nearby_fact(
            "Dual Place Homes",
            "nearby_metro_stations",
            "Hoodi Metro (1.6 km)",
        );
        builder.add_nearby_fact(
            "Dual Place Homes",
            "nearby_hospitals",
            "Manipal Hospital (1.6 km)",
        );
        builder.add_home(HomeSpec::new(
            "mock-hoodi-only-3bhk",
            "Hoodi Only Homes",
            "Hoodi",
            3,
            24_000_000,
            12.9900,
            77.7155,
        ));
        builder.add_home(HomeSpec::new(
            "mock-manipal-only-3bhk",
            "Manipal Only Homes",
            "Whitefield",
            3,
            24_000_000,
            12.9700,
            77.7355,
        ));
        builder.add_home(HomeSpec::new(
            "mock-school-home-3bhk",
            "School Walk Homes",
            "Indiranagar",
            3,
            24_000_000,
            12.9510,
            77.6410,
        ));
        builder.add_home(HomeSpec::new(
            "mock-metro-home-2bhk",
            "Metro Walk Homes",
            "South Bengaluru",
            2,
            20_000_000,
            12.8510,
            77.6010,
        ));
        builder.add_nearby_fact(
            "Metro Walk Homes",
            "nearby_metro_stations",
            "Mock Metro Station (0.2 km)",
        );
        if profile.distance_decoy {
            builder.add_home(HomeSpec::new(
                "mock-far-metro-2bhk",
                "Far Metro Homes",
                "East Bengaluru",
                2,
                22_000_000,
                12.9000,
                77.7000,
            ));
            builder.add_nearby_fact(
                "Far Metro Homes",
                "nearby_metro_stations",
                "Mock Metro Station (12.0 km)",
            );
        }
        builder.add_home(
            HomeSpec::new(
                "mock-quiet-reviewed-3bhk",
                "Quiet Reviewed Homes",
                "CV Raman Nagar",
                3,
                21_000_000,
                12.9790,
                77.6590,
            )
            .quality(Some(0.05), Some(4.7)),
        );
        builder.add_nearby_fact(
            "Quiet Reviewed Homes",
            "nearby_tech_parks",
            "Bagmane Tech Park (0.3 km)",
        );
        if profile.decision_candidates {
            builder.add_home(
                HomeSpec::new(
                    "mock-quiet-priority-3bhk",
                    "Quiet Priority Homes",
                    "CV Raman Nagar",
                    3,
                    22_000_000,
                    12.9795,
                    77.6595,
                )
                .quality(Some(0.01), Some(4.0)),
            );
            builder.add_nearby_fact(
                "Quiet Priority Homes",
                "nearby_tech_parks",
                "Bagmane Tech Park (0.3 km)",
            );
            builder.add_home(
                HomeSpec::new(
                    "mock-review-priority-3bhk",
                    "Review Priority Homes",
                    "CV Raman Nagar",
                    3,
                    20_000_000,
                    12.9785,
                    77.6585,
                )
                .quality(Some(0.2), Some(4.9)),
            );
            builder.add_nearby_fact(
                "Review Priority Homes",
                "nearby_tech_parks",
                "Bagmane Tech Park (0.4 km)",
            );
            builder.add_home(
                HomeSpec::new(
                    "mock-value-priority-3bhk",
                    "Value Priority Homes",
                    "CV Raman Nagar",
                    3,
                    19_000_000,
                    12.9787,
                    77.6587,
                )
                .quality(Some(0.2), Some(4.2)),
            );
            builder.add_nearby_fact(
                "Value Priority Homes",
                "nearby_tech_parks",
                "Bagmane Tech Park (0.4 km)",
            );
        }
        builder.add_home(
            HomeSpec::new(
                "mock-missing-noise-3bhk",
                "Unmeasured Quiet Homes",
                "CV Raman Nagar",
                3,
                20_000_000,
                12.9780,
                77.6580,
            )
            .quality(None, Some(4.2)),
        );
        builder.add_nearby_fact(
            "Unmeasured Quiet Homes",
            "nearby_tech_parks",
            "Bagmane Tech Park (0.4 km)",
        );
        builder.add_home(HomeSpec::new(
            "mock-snn-etternia-3bhk",
            "SNN Raj Etternia",
            "Haralur Road",
            3,
            27_000_000,
            12.8900,
            77.6700,
        ));
        builder.add_home(HomeSpec::new(
            "mock-prestige-song-3bhk",
            "Prestige Song of the South",
            "Begur Road",
            3,
            26_000_000,
            12.8700,
            77.6200,
        ));
        builder.add_home(HomeSpec::new(
            "mock-electronic-city-3bhk",
            "Electronic City Family Homes",
            "Electronic City",
            3,
            17_000_000,
            12.8400,
            77.6700,
        ));
        builder.add_home(HomeSpec::new(
            "mock-kanakapura-road-3bhk",
            "Kanakapura Family Homes",
            "Kanakapura Road",
            3,
            23_000_000,
            12.8300,
            77.5500,
        ));
        builder.add_home(HomeSpec::new(
            "mock-balanced-commute-3bhk",
            "Balanced Commute Homes",
            "East Bengaluru",
            3,
            25_000_000,
            12.9750,
            77.7000,
        ));
        builder.add_nearby_fact(
            "Balanced Commute Homes",
            "nearby_tech_parks",
            "Bagmane Tech Park (4.4 km)",
        );
        builder.add_nearby_fact(
            "Balanced Commute Homes",
            "nearby_hospitals",
            "Manipal Hospital Whitefield (4.0 km)",
        );
        builder.add_home(HomeSpec::new(
            "mock-unbalanced-commute-3bhk",
            "Unbalanced Commute Homes",
            "East Bengaluru",
            3,
            24_000_000,
            12.9800,
            77.6620,
        ));
        builder.add_home(HomeSpec::new(
            "mock-hoodi-alternative-3bhk",
            "Hoodi Alternative",
            "Hoodi",
            3,
            23_500_000,
            12.9905,
            77.7152,
        ));
        builder.add_home(HomeSpec::new(
            "mock-hoodi-expensive-3bhk",
            "Hoodi Premium Homes",
            "Hoodi",
            3,
            26_000_000,
            12.9908,
            77.7157,
        ));
        builder.add_home(HomeSpec::new(
            "mock-larger-elsewhere-4bhk",
            "Larger Elsewhere",
            "South Bengaluru",
            4,
            29_000_000,
            12.8200,
            77.5800,
        ));

        builder.build(profile.derive_proximity)
    }

    fn search(&self, query: &str) -> ObservedSearch {
        let output = self.search_output(query);
        let negative_preferences = output
            .intent
            .negative_preferences
            .iter()
            .map(|preference| preference.raw_text.clone())
            .collect();
        let positive_preferences = output
            .intent
            .positive_preferences
            .iter()
            .map(|preference| preference.raw_text.clone())
            .collect();
        let accepted_tradeoffs = output.intent.accepted_tradeoffs.clone();
        let ranking_priorities = output.intent.ranking_priorities.clone();
        let areas = output
            .intent
            .requested_areas()
            .into_iter()
            .map(str::to_string)
            .collect();
        let bhks = output.intent.requested_bhks();
        let budget_max = output.intent.budget_max;
        let min_constraints = output
            .intent
            .hard_constraints
            .iter()
            .filter(|constraint| {
                matches!(
                    constraint.operator,
                    backend::search::intent::ConstraintOperator::Min
                )
            })
            .map(|constraint| (constraint.field.clone(), constraint.value))
            .collect();
        let resolved_entities = output
            .diagnostics
            .resolved
            .entities
            .iter()
            .map(|entity| ObservedResolvedEntity {
                entity_type: entity.entity_type.clone(),
                name: entity.name.clone(),
                matched_text: entity.matched_text.clone(),
            })
            .collect();
        let ordered_result_ids = output
            .results
            .iter()
            .map(|result| result.card.id.clone())
            .collect();
        ObservedSearch {
            ordered_result_ids,
            result_sets: output
                .result_sets
                .into_iter()
                .map(|result_set| {
                    result_set
                        .results
                        .into_iter()
                        .map(|result| {
                            let property = self
                                .properties
                                .iter()
                                .find(|property| property.id == result.card.id)
                                .expect("result property exists in controlled fixture");
                            let claimed_preferences = result
                                .match_explanation
                                .as_ref()
                                .into_iter()
                                .flat_map(|explanation| &explanation.reasons)
                                .map(|reason| reason.preference.clone())
                                .collect();
                            let preference_coverage = result
                                .match_explanation
                                .as_ref()
                                .into_iter()
                                .flat_map(|explanation| &explanation.preference_coverage)
                                .map(|coverage| {
                                    (coverage.preference.clone(), coverage.status.clone())
                                })
                                .collect();
                            ObservedResult {
                                id: result.card.id.clone(),
                                society: result.card.society_name.clone(),
                                area: property.area.clone(),
                                bhk: property.bhk,
                                price: property.price,
                                home_state: if property
                                    .possession_status
                                    .eq_ignore_ascii_case("Ready to Move")
                                {
                                    "delivered".to_string()
                                } else {
                                    "under_construction".to_string()
                                },
                                proof_labels: result
                                    .proof_focuses
                                    .iter()
                                    .filter_map(|focus| focus.matched_label.clone())
                                    .chain(
                                        result
                                            .match_explanation
                                            .as_ref()
                                            .into_iter()
                                            .flat_map(|explanation| &explanation.reasons)
                                            .map(|reason| reason.display.clone()),
                                    )
                                    .collect(),
                                proof_distances_m: result
                                    .proof_focuses
                                    .iter()
                                    .filter_map(|focus| focus.distance_m)
                                    .collect(),
                                claimed_preferences,
                                preference_coverage,
                            }
                        })
                        .collect()
                })
                .collect(),
            warnings: output.diagnostics.warnings,
            positive_preferences,
            negative_preferences,
            ranking_priorities,
            accepted_tradeoffs,
            min_constraints,
            areas,
            bhks,
            budget_max,
            resolved_entities,
        }
    }

    fn search_output(&self, query: &str) -> backend::search::engine::SearchEngineOutput {
        let index = SearchIndex::build_with_serving_graph(
            &self.properties,
            &self.bundle.entities,
            &self.bundle.edges,
        );
        let snapshot = SearchRuntimeSnapshot::new(
            self.bundle.clone(),
            self.properties.clone(),
            mock_societies(&self.properties),
            Vec::new(),
            index,
        );
        SearchEngine::new(&snapshot).search(query)
    }
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

#[derive(Default)]
struct FixtureBuilder {
    properties: Vec<Property>,
    entities: Vec<ServingEntityRecord>,
    aliases: Vec<ServingEntityAliasRecord>,
    facts: Vec<ServingFactRecord>,
    metadata: Vec<ServingSearchMetadataRecord>,
    edges: Vec<backend::serving::ServingEdgeRecord>,
}

impl FixtureBuilder {
    fn add_area(&mut self, name: &str) {
        let entity_id = format!("area:{}", slug(name));
        self.entities.push(entity(&entity_id, "area", name));
    }

    fn add_geo_cell(&mut self, entity_id: &str, min_lon: f64, max_lon: f64) {
        let mut cell = entity(entity_id, "area", "Internal search cell");
        cell.root_source = Some("openstreetmap".to_string());
        self.entities.push(cell);
        let mut fact = serving_fact(
            entity_id,
            "geo.geometry_geojson",
            FactValue::Text(format!(
                "{{\"type\":\"Polygon\",\"coordinates\":[[[{min_lon},12.9],[{max_lon},12.9],[{max_lon},12.91],[{min_lon},12.91],[{min_lon},12.9]]]}}"
            )),
        );
        fact.source_type = "OpenStreetMap".to_string();
        self.facts.push(fact);
    }

    fn add_place(&mut self, name: &str, category: &str, latitude: f64, longitude: f64) {
        let entity_id = format!("place:{}", slug(name));
        self.add_place_with_id(&entity_id, name, category, latitude, longitude);
    }

    fn add_place_with_id(
        &mut self,
        entity_id: &str,
        name: &str,
        category: &str,
        latitude: f64,
        longitude: f64,
    ) {
        self.add_place_with_source(entity_id, name, category, latitude, longitude, "Google");
    }

    fn add_place_with_source(
        &mut self,
        entity_id: &str,
        name: &str,
        category: &str,
        latitude: f64,
        longitude: f64,
        source_type: &str,
    ) {
        self.entities.push(entity(&entity_id, "place", name));
        for (fact_key, value) in [
            ("geo.latitude", FactValue::Numeric(latitude)),
            ("geo.longitude", FactValue::Numeric(longitude)),
            ("place.category", FactValue::Text(category.to_string())),
        ] {
            let mut fact = serving_fact(entity_id, fact_key, value);
            fact.source_type = source_type.to_string();
            fact.observation = Some(
                SourceObservation::new(
                    source_type,
                    format!("entity-record:{entity_id}"),
                    entity_id,
                    fact.learned_at,
                    fact.source_url.clone(),
                    vec!["asset:controlled-search-fixture/v1".to_string()],
                )
                .unwrap(),
            );
            self.facts.push(fact);
        }
    }

    fn add_edge(&mut self, from: &str, relation: &str, to: &str) {
        self.edges.push(backend::serving::ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: relation.to_string(),
            to_entity_id: to.to_string(),
            confidence: 0.9,
            source_type: "OpenStreetMap".to_string(),
            derivation: None,
        });
    }

    fn add_alias(&mut self, alias: &str, society: &str) {
        self.aliases.push(ServingEntityAliasRecord {
            alias: alias.to_string(),
            normalized_alias: alias.to_ascii_lowercase(),
            entity_id: format!("society:{}", slug(society)),
            entity_type: "society".to_string(),
            entity_name: society.to_string(),
            source: "controlled_fixture".to_string(),
        });
    }

    fn add_home(&mut self, spec: HomeSpec) {
        let society_id = slug(&spec.society);
        let entity_id = format!("society:{society_id}");
        let area_entity_id = format!("area:{}", slug(&spec.area));
        if !self
            .entities
            .iter()
            .any(|entity| entity.entity_id == area_entity_id)
        {
            self.entities
                .push(entity(&area_entity_id, "area", &spec.area));
        }
        if !self
            .entities
            .iter()
            .any(|entity| entity.entity_id == entity_id)
        {
            self.entities
                .push(entity(&entity_id, "society", &spec.society));
        }
        if !self.edges.iter().any(|edge| {
            edge.from_entity_id == entity_id
                && edge.edge_type == "in_area"
                && edge.to_entity_id == area_entity_id
        }) {
            self.add_edge(&entity_id, "in_area", &area_entity_id);
        }
        if !self.edges.iter().any(|edge| {
            edge.from_entity_id == entity_id
                && edge.edge_type == "in_market_locality"
                && edge.to_entity_id == area_entity_id
        }) {
            self.add_edge(&entity_id, "in_market_locality", &area_entity_id);
        }
        self.add_fact(
            &entity_id,
            "geo.latitude",
            FactValue::Numeric(spec.latitude),
        );
        self.add_fact(
            &entity_id,
            "geo.longitude",
            FactValue::Numeric(spec.longitude),
        );
        self.add_search_fact(
            &entity_id,
            "home_state",
            FactValue::Text(spec.state.clone()),
            &["ready to move", "under construction"],
            None,
        );
        if let Some(noise_score) = spec.noise_score {
            self.add_numeric_search_fact(
                &entity_id,
                "noise_score",
                noise_score,
                &["quiet", "quiet surroundings"],
                "LowerIsBetter",
                &[0.3, 0.5],
            );
        }
        if let Some(rating) = spec.rating {
            self.add_numeric_search_fact(
                &entity_id,
                "google_rating",
                rating,
                &["review quality", "good reviews"],
                "HigherIsBetter",
                &[4.5, 4.0],
            );
        }
        self.facts.push(inventory_receipt_fact(&entity_id, &spec));
        self.properties.push(property(&spec, &society_id));
    }

    fn add_nearby_fact(&mut self, society: &str, fact_key: &str, value: &str) {
        let entity_id = format!("society:{}", slug(society));
        self.add_search_fact(
            &entity_id,
            fact_key,
            FactValue::Text(value.to_string()),
            &["nearby"],
            Some("TextMatch"),
        );
    }

    fn add_fact(&mut self, entity_id: &str, fact_key: &str, value: FactValue) {
        self.facts.push(serving_fact(entity_id, fact_key, value));
    }

    fn add_search_fact(
        &mut self,
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        preferences: &[&str],
        direction: Option<&str>,
    ) {
        self.add_fact(entity_id, fact_key, value);
        self.metadata.push(ServingSearchMetadataRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: None,
            answers_preferences: preferences.iter().map(|value| value.to_string()).collect(),
            scoring_direction: direction.map(str::to_string),
            scoring_weight: Some(1.0),
            scoring_thresholds: Vec::new(),
        });
    }

    fn add_numeric_search_fact(
        &mut self,
        entity_id: &str,
        fact_key: &str,
        value: f64,
        preferences: &[&str],
        direction: &str,
        thresholds: &[f64],
    ) {
        self.add_fact(entity_id, fact_key, FactValue::Numeric(value));
        self.metadata.push(ServingSearchMetadataRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: None,
            answers_preferences: preferences.iter().map(|value| value.to_string()).collect(),
            scoring_direction: Some(direction.to_string()),
            scoring_weight: Some(1.0),
            scoring_thresholds: thresholds.to_vec(),
        });
    }

    fn build(mut self, derive_proximity: bool) -> MockSearchFixture {
        let mut edges = self.edges;
        qualify_fixture_edges(&mut edges, &self.facts);
        add_fixture_geo_cells(&mut self.entities, &self.facts, &mut edges);
        let identity_index =
            ServingFactIndex::from_records(self.facts.clone(), self.metadata.clone());
        let identity = materialize_canonical_spatial_identities(
            &self.entities,
            &identity_index,
            &edges,
            "conversational-semantics-mock",
        )
        .expect("controlled spatial identities materialize");
        self.entities.extend(identity.canonical_entities);
        edges.extend(identity.binding_edges);
        let topology_index =
            ServingFactIndex::from_records(self.facts.clone(), self.metadata.clone());
        let topology = backend::serving::derive_spatial_topology(
            &self.entities,
            &topology_index,
            &edges,
            &Default::default(),
            "conversational-semantics-mock",
        );
        edges.extend(topology.edges);
        if derive_proximity {
            let base_index =
                ServingFactIndex::from_records(self.facts.clone(), self.metadata.clone());
            let derived = derive_proximity_records(
                &self.entities,
                &base_index,
                &edges,
                "conversational-semantics-mock",
            )
            .expect("controlled proximity facts derive from config");
            self.facts.extend(derived.facts);
            self.metadata.extend(derived.search_metadata);
            edges.extend(derived.edges);
        }
        let mut fact_index = ServingFactIndex::from_records(self.facts.clone(), self.metadata);
        fact_index.add_canonical_spatial_bindings(&edges);
        let entity_alias_count = self.aliases.len() as u64;
        let entity_alias_index = ServingEntityAliasIndex::from_records(self.aliases)
            .expect("controlled aliases must be valid");
        let temp_dir = tempdir().expect("temporary Tantivy directory");
        let recall_facts = self
            .facts
            .iter()
            .filter(|fact| fact.source_type != "ControlledInventoryReceipt")
            .cloned()
            .collect::<Vec<_>>();
        let recall_index =
            TantivyRecallIndex::build_in_dir(temp_dir.path(), &self.entities, &recall_facts, &[])
                .expect("mock recall index");
        let geo_index =
            GeoSearchIndex::from_serving_bundle_with_edges(&self.entities, &fact_index, &edges);
        let spatial_index = SpatialServingIndex::from_serving_bundle_with_edges(
            &self.entities,
            &fact_index,
            &edges,
        );
        let search_capabilities = SearchCapabilityIndex::from_bundle(&self.entities, &fact_index);
        let graph_index = GraphIndex::from_serving_edges(&edges);
        let bundle = LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "conversational-semantics-mock".to_string(),
                format_version: 1,
                created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                entity_count: self.entities.len() as u64,
                entity_alias_count,
                fact_count: self.facts.len() as u64,
                search_metadata_count: 0,
                rera_evidence_count: 0,
                excluded_rera_evidence_society_ids: Vec::new(),
                edge_count: edges.len() as u64,
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
            entities: self.entities,
            entity_alias_index,
            edges,
            graph_index,
            recall_index,
            fact_index,
            rera_evidence_index: ReraEvidenceIndex::default(),
            geo_index,
            spatial_index,
            search_capabilities,
            cache_dir: temp_dir.keep(),
        };
        MockSearchFixture {
            properties: self.properties,
            bundle: Arc::new(bundle),
        }
    }
}

fn qualify_fixture_edges(
    edges: &mut [backend::serving::ServingEdgeRecord],
    facts: &[ServingFactRecord],
) {
    for edge in edges.iter_mut().filter(|edge| {
        edge.derivation.is_none()
            && matches!(
                edge.edge_type.as_str(),
                "in_market_locality" | "adjacent_area" | "occupies_geo_cell" | "covers_geo_cell"
            )
    }) {
        let Some(observation) = facts
            .iter()
            .filter(|fact| {
                fact.entity_id == edge.from_entity_id || fact.entity_id == edge.to_entity_id
            })
            .find_map(|fact| fact.observation.as_ref())
            .or_else(|| facts.iter().find_map(|fact| fact.observation.as_ref()))
        else {
            continue;
        };
        edge.derivation = Some(
            DerivedEvidence::new(
                "conversational-semantics-mock",
                &edge.from_entity_id,
                Some(edge.to_entity_id.clone()),
                &edge.edge_type,
                "controlled_relation",
                Some(1.0),
                Some("boolean".to_string()),
                "controlled-search-fixture-v1",
                edge.confidence,
                vec![EvidenceRef::for_observation(
                    "conversational-semantics-mock",
                    observation,
                )],
            )
            .expect("controlled edge evidence is valid"),
        );
    }
}

fn add_fixture_geo_cells(
    entities: &mut Vec<ServingEntityRecord>,
    facts: &[ServingFactRecord],
    edges: &mut Vec<backend::serving::ServingEdgeRecord>,
) {
    let memberships = edges
        .iter()
        .filter(|edge| edge.edge_type == "in_market_locality" && edge.derivation.is_some())
        .cloned()
        .collect::<Vec<_>>();
    for membership in memberships {
        let cell_id = format!("area:cell:{}", slug(&membership.to_entity_id));
        if !entities.iter().any(|entity| entity.entity_id == cell_id) {
            let mut cell = entity(&cell_id, "area", "Internal search cell");
            cell.root_source = Some("openstreetmap".to_string());
            entities.push(cell);
        }
        let Some(subject_observation) = facts
            .iter()
            .filter(|fact| fact.entity_id == membership.from_entity_id)
            .find_map(|fact| fact.observation.as_ref())
        else {
            continue;
        };
        if !edges.iter().any(|edge| {
            edge.from_entity_id == membership.from_entity_id
                && edge.edge_type == "occupies_geo_cell"
        }) {
            edges.push(fixture_derived_edge(
                &membership.from_entity_id,
                "occupies_geo_cell",
                &cell_id,
                "qualified_point_containment",
                vec![EvidenceRef::for_observation(
                    "conversational-semantics-mock",
                    subject_observation,
                )],
            ));
        }
        if !edges.iter().any(|edge| {
            edge.from_entity_id == membership.to_entity_id && edge.edge_type == "covers_geo_cell"
        }) {
            let membership_derivation = membership
                .derivation
                .as_ref()
                .expect("qualified fixture membership");
            edges.push(fixture_derived_edge(
                &membership.to_entity_id,
                "covers_geo_cell",
                &cell_id,
                "member_geo_cell_coverage",
                vec![EvidenceRef::for_derivation(membership_derivation)],
            ));
        }
    }
}

fn fixture_derived_edge(
    from: &str,
    relation: &str,
    to: &str,
    metric: &str,
    input_evidence: Vec<EvidenceRef>,
) -> backend::serving::ServingEdgeRecord {
    let derivation = DerivedEvidence::new(
        "conversational-semantics-mock",
        from,
        Some(to.to_string()),
        relation,
        metric,
        Some(1.0),
        Some("boolean".to_string()),
        "controlled-search-fixture-v1",
        0.9,
        input_evidence,
    )
    .expect("controlled geo-cell evidence is valid");
    backend::serving::ServingEdgeRecord {
        from_entity_id: from.to_string(),
        edge_type: relation.to_string(),
        to_entity_id: to.to_string(),
        confidence: 0.9,
        source_type: "ControlledGeoCell".to_string(),
        derivation: Some(derivation),
    }
}

fn add_regional_inventory(builder: &mut FixtureBuilder) {
    for area in [
        "Whitefield",
        "Sarjapur Road",
        "North Bengaluru",
        "Bellandur",
        "HSR Layout",
        "Devanahalli",
        "Yelahanka",
        "Electronic City",
    ] {
        builder.add_area(area);
    }
    builder.add_place_with_source(
        "place:google:kadugodi-tree-park-metro",
        "Kadugodi Tree Park",
        "metro",
        12.9958,
        77.7574,
        "Google",
    );
    builder.add_place_with_source(
        "place:osm:kadugodi-tree-park-metro",
        "Kadugodi Tree Park",
        "metro_station",
        12.9959,
        77.7575,
        "OpenStreetMap",
    );
    builder.add_place_with_source(
        "place:google:kadugodi-tree-park",
        "Kadugodi Tree Park",
        "park",
        12.9982,
        77.7558,
        "Google",
    );
    builder.add_place("Iblur Metro", "metro", 12.9182, 77.6713);
    builder.add_place("Nagawara Metro", "metro", 13.0448, 77.6215);

    builder.add_home(
        HomeSpec::new(
            "mock-whitefield-value-2bhk",
            "Whitefield Value Homes",
            "Whitefield",
            2,
            14_500_000,
            12.9964,
            77.7579,
        )
        .quality(Some(0.35), Some(4.2)),
    );
    builder.add_nearby_fact(
        "Whitefield Value Homes",
        "nearby_metro_stations",
        "Kadugodi Tree Park Metro (0.1 km)",
    );
    builder.add_home(
        HomeSpec::new(
            "mock-whitefield-family-3bhk",
            "Whitefield Family Homes",
            "Whitefield",
            3,
            21_000_000,
            12.9948,
            77.7552,
        )
        .quality(Some(0.25), Some(4.5)),
    );
    builder.add_nearby_fact(
        "Whitefield Family Homes",
        "nearby_metro_stations",
        "Kadugodi Tree Park Metro (0.3 km)",
    );
    builder.add_home(
        HomeSpec::new(
            "mock-whitefield-premium-3bhk",
            "Whitefield Premium Homes",
            "Whitefield",
            3,
            28_000_000,
            12.9918,
            77.7520,
        )
        .quality(Some(0.12), Some(4.7)),
    );

    builder.add_home(
        HomeSpec::new(
            "mock-sarjapur-value-2bhk",
            "Sarjapur Value Homes",
            "Sarjapur Road",
            2,
            12_800_000,
            12.9196,
            77.6730,
        )
        .quality(Some(0.4), Some(4.1)),
    );
    builder.add_nearby_fact(
        "Sarjapur Value Homes",
        "nearby_metro_stations",
        "Iblur Metro (0.3 km)",
    );
    builder.add_home(
        HomeSpec::new(
            "mock-sarjapur-family-3bhk",
            "Sarjapur Family Homes",
            "Sarjapur Road",
            3,
            18_500_000,
            12.9172,
            77.6692,
        )
        .quality(Some(0.28), Some(4.6)),
    );
    builder.add_nearby_fact(
        "Sarjapur Family Homes",
        "nearby_metro_stations",
        "Iblur Metro (0.3 km)",
    );
    builder.add_home(
        HomeSpec::new(
            "mock-sarjapur-premium-4bhk",
            "Sarjapur Premium Homes",
            "Sarjapur Road",
            4,
            31_000_000,
            12.9150,
            77.6660,
        )
        .quality(Some(0.15), Some(4.4))
        .under_construction(),
    );

    builder.add_home(
        HomeSpec::new(
            "mock-north-value-2bhk",
            "North Bengaluru Value Homes",
            "North Bengaluru",
            2,
            11_000_000,
            13.0454,
            77.6222,
        )
        .quality(Some(0.4), Some(4.0)),
    );
    builder.add_nearby_fact(
        "North Bengaluru Value Homes",
        "nearby_metro_stations",
        "Nagawara Metro (0.1 km)",
    );
    builder.add_home(
        HomeSpec::new(
            "mock-north-family-3bhk",
            "North Bengaluru Family Homes",
            "North Bengaluru",
            3,
            16_500_000,
            13.0436,
            77.6204,
        )
        .quality(Some(0.3), Some(4.6)),
    );
    builder.add_nearby_fact(
        "North Bengaluru Family Homes",
        "nearby_metro_stations",
        "Nagawara Metro (0.2 km)",
    );
    for (id, name, area, bhk, price, latitude, longitude) in [
        (
            "mock-bellandur-value-2bhk",
            "Bellandur Value Homes",
            "Bellandur",
            2,
            16_500_000,
            12.9250,
            77.6760,
        ),
        (
            "mock-hsr-family-3bhk",
            "HSR Family Homes",
            "HSR Layout",
            3,
            24_000_000,
            12.9116,
            77.6389,
        ),
        (
            "mock-devanahalli-value-2bhk",
            "Devanahalli Value Homes",
            "Devanahalli",
            2,
            11_500_000,
            13.2473,
            77.7110,
        ),
        (
            "mock-yelahanka-family-3bhk",
            "Yelahanka Family Homes",
            "Yelahanka",
            3,
            18_500_000,
            13.1007,
            77.5963,
        ),
        (
            "mock-electronic-city-value-2bhk",
            "Electronic City Value Homes",
            "Electronic City",
            2,
            10_500_000,
            12.8452,
            77.6602,
        ),
    ] {
        builder.add_home(
            HomeSpec::new(id, name, area, bhk, price, latitude, longitude)
                .quality(Some(0.3), Some(4.2)),
        );
    }
    builder.add_home(
        HomeSpec::new(
            "mock-north-premium-3bhk",
            "North Bengaluru Premium Homes",
            "North Bengaluru",
            3,
            24_000_000,
            13.0410,
            77.6170,
        )
        .quality(Some(0.1), Some(4.1)),
    );
    builder.add_nearby_fact(
        "North Bengaluru Premium Homes",
        "nearby_metro_stations",
        "Nagawara Metro (0.6 km)",
    );
}

fn add_multi_or_decoys(builder: &mut FixtureBuilder, candidates_per_bhk: usize) {
    for bhk in [2_u32, 3_u32] {
        for index in 0..candidates_per_bhk {
            builder.add_home(HomeSpec::new(
                format!("mock-or-decoy-{bhk}-{index:02}"),
                format!("OR Decoy {bhk}-{index:02}"),
                "Controlled OR District",
                bhk,
                if bhk == 2 { 18_000_000 } else { 22_000_000 },
                12.80 + (index as f64 * 0.0001),
                77.50 + (f64::from(bhk) * 0.001),
            ));
        }
    }
}

#[derive(Clone)]
struct HomeSpec {
    id: String,
    society: String,
    area: String,
    bhk: u32,
    price: u64,
    latitude: f64,
    longitude: f64,
    state: String,
    noise_score: Option<f64>,
    rating: Option<f64>,
}

impl HomeSpec {
    fn new(
        id: impl Into<String>,
        society: impl Into<String>,
        area: impl Into<String>,
        bhk: u32,
        price: u64,
        latitude: f64,
        longitude: f64,
    ) -> Self {
        Self {
            id: id.into(),
            society: society.into(),
            area: area.into(),
            bhk,
            price,
            latitude,
            longitude,
            state: "delivered".to_string(),
            noise_score: Some(0.5),
            rating: Some(4.0),
        }
    }

    fn under_construction(mut self) -> Self {
        self.state = "under_construction".to_string();
        self
    }

    fn quality(mut self, noise_score: Option<f64>, rating: Option<f64>) -> Self {
        self.noise_score = noise_score;
        self.rating = rating;
        self
    }
}

fn property(spec: &HomeSpec, society_id: &str) -> Property {
    Property {
        id: spec.id.to_string(),
        title: spec.society.to_string(),
        area: spec.area.to_string(),
        area_id: slug(&spec.area),
        city: "Bengaluru".to_string(),
        society_id: society_id.to_string(),
        builder_name: "Mock Builder".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk: spec.bhk,
        price: spec.price,
        price_min: None,
        price_max: None,
        price_per_sqft: 12_000,
        carpet_area_sqft: 1_200,
        super_builtup_sqft: 1_550,
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: if spec.state == "delivered" {
            "Ready to Move"
        } else {
            "Under Construction"
        }
        .to_string(),
        metro_distance_mins: 8,
        maintenance_cost_monthly: 6_000,
        society_quality_score: Some(0.7),
        builder_quality_score: Some(0.7),
        document_completeness_score: Some(0.8),
        litigation_risk: Some(0.1),
        noise_score: spec.noise_score,
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
        description_summary: "Controlled conversational-search fixture".to_string(),
        transparency_tags: Vec::new(),
        source_reference: "conversational-semantics-contract".to_string(),
    }
}

fn entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
    ServingEntityRecord {
        entity_id: entity_id.to_string(),
        entity_type: entity_type.to_string(),
        name: name.to_string(),
        root_source: Some("mock_contract".to_string()),
        visibility: Default::default(),
        searchable_text: name.to_string(),
    }
}

fn serving_fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
    let observed_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let source_url = Some(format!("https://example.test/entities/{entity_id}"));
    let observation = SourceObservation::new(
        "ControlledFixture",
        format!("entity-record:{entity_id}"),
        entity_id,
        observed_at,
        source_url.clone(),
        vec!["asset:controlled-search-fixture/v1".to_string()],
    )
    .unwrap();
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
        source_url,
        model: None,
        skill_id: Some("search_conversational_semantics_contract".to_string()),
        learned_at: observed_at,
        observation: Some(observation),
    }
}

fn inventory_receipt_fact(entity_id: &str, spec: &HomeSpec) -> ServingFactRecord {
    let value = serde_json::json!({
        "bhk": spec.bhk,
        "price": spec.price,
        "area_sqft": 1_600,
    })
    .to_string();
    let mut fact = serving_fact(
        entity_id,
        "controlled_inventory_option",
        FactValue::Text(value),
    );
    fact.source_type = "ControlledInventoryReceipt".to_string();
    fact.source_url = Some(format!("https://example.test/listings/{}", spec.id));
    fact.observation = Some(
        SourceObservation::new(
            "ControlledFixture",
            spec.id.clone(),
            entity_id,
            Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            fact.source_url.clone(),
            vec!["asset:controlled-search-fixture/v1".to_string()],
        )
        .unwrap(),
    );
    fact
}

fn slug(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
