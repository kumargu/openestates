use std::collections::HashMap;
use std::sync::OnceLock;

use backend::graph::GraphIndex;
use backend::knowledge::FactValue;
use backend::models::Property;
use backend::search::geo::GeoSearchIndex;
use backend::search::{SearchCapabilityIndex, SearchEngine, SearchIndex};
use backend::serving::{
    LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest, ServingEntityAliasIndex,
    ServingEntityRecord, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
    SpatialServingIndex, TantivyRecallIndex,
};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use tempfile::tempdir;

const FROZEN_BANK: &str =
    include_str!("../../data/validation/query_bank/search_conversational_semantics_v1.json");
const BUYER_LANGUAGE_BANK: &str =
    include_str!("../../data/validation/query_bank/search_buyer_language_v1.json");
const DECISION_RANKING_BANK: &str =
    include_str!("../../data/validation/query_bank/search_decision_ranking_v1.json");
static QUERY_BANK: OnceLock<QueryBank> = OnceLock::new();

#[derive(Deserialize)]
struct QueryBank {
    cases: Vec<QueryCase>,
}

#[derive(Deserialize)]
struct QueryCase {
    id: String,
    query: String,
}

#[derive(Deserialize)]
struct ControlledQueryBank {
    cases: Vec<ControlledQueryCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlledQueryCase {
    id: String,
    query: String,
    #[serde(rename = "category")]
    _category: String,
    expected_semantics: ExpectedSemantics,
    fixture_expectation: FixtureExpectation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedSemantics {
    area: Option<String>,
    bhk: Option<u32>,
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
    area: String,
    bhk: u32,
    budget_max: u64,
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
}

#[test]
fn frozen_branch_queries_execute_against_mock_inventory() {
    let fixture = MockSearchFixture::new();

    assert_branch_ids(
        fixture.search(query("CONV-SEM-001")),
        &[
            &[
                "mock-godrej-air-3bhk",
                "mock-hoodi-only-3bhk",
                "mock-hoodi-alternative-3bhk",
            ],
            &["mock-lakeside-orchard-4bhk"],
        ],
    );
    assert_branch_ids(
        fixture.search(query("CONV-SEM-004")),
        &[&["mock-school-home-3bhk"], &["mock-metro-home-2bhk"]],
    );
    assert_branch_ids(
        fixture.search(query("CONV-SEM-005")),
        &[&["mock-godrej-air-3bhk"], &["mock-lakeside-orchard-4bhk"]],
    );
    assert_branch_ids(
        fixture.search(query("CONV-SEM-007")),
        &[&["mock-snn-etternia-3bhk"], &["mock-prestige-song-3bhk"]],
    );
    assert_branch_ids(
        fixture.search(query("CONV-SEM-008")),
        &[
            &["mock-electronic-city-3bhk"],
            &["mock-kanakapura-road-3bhk"],
        ],
    );
}

#[test]
fn frozen_proximity_and_preference_queries_execute_against_mock_facts() {
    let fixture = MockSearchFixture::new();

    let proximity_or_space = fixture.search(query("CONV-SEM-002"));
    assert_contains_ids(
        &proximity_or_space,
        &["mock-bagmane-small-3bhk", "mock-far-delivered-4bhk"],
    );
    assert_excludes_ids(
        &proximity_or_space,
        &["mock-bagmane-under-construction-3bhk"],
    );

    let dual_place = fixture.search(query("CONV-SEM-003"));
    assert_contains_ids(&dual_place, &["mock-dual-place-3bhk"]);
    assert_excludes_ids(
        &dual_place,
        &["mock-hoodi-only-3bhk", "mock-manipal-only-3bhk"],
    );
    assert_proves_places(
        &dual_place,
        "mock-dual-place-3bhk",
        &["Hoodi Metro", "Manipal Hospital"],
    );

    let soft_evidence = fixture.search(query("CONV-SEM-006"));
    assert_first_id(&soft_evidence, "mock-quiet-reviewed-3bhk");
    assert_contains_ids(&soft_evidence, &["mock-missing-noise-3bhk"]);

    let balanced_commute = fixture.search(query("CONV-SEM-009"));
    assert_first_id(&balanced_commute, "mock-balanced-commute-3bhk");
    assert_proves_places(
        &balanced_commute,
        "mock-balanced-commute-3bhk",
        &["Bagmane Tech Park", "Manipal Hospital Whitefield"],
    );
}

#[test]
fn frozen_contextual_alternatives_apply_global_exclusion() {
    let fixture = MockSearchFixture::new();
    let output = fixture.search(query("CONV-SEM-010"));

    assert_excludes_ids(&output, &["mock-godrej-air-3bhk"]);
    assert_contains_ids(
        &output,
        &["mock-hoodi-alternative-3bhk", "mock-larger-elsewhere-4bhk"],
    );
}

#[test]
fn frozen_buyer_language_queries_execute_against_product_model() {
    let fixture = MockSearchFixture::with_buyer_candidates();
    let bank: ControlledQueryBank =
        serde_json::from_str(BUYER_LANGUAGE_BANK).expect("buyer-language bank is valid");

    for case in bank.cases {
        let output = fixture.search(&case.query);
        assert_controlled_expectation(&case, &output);
    }
}

#[test]
fn frozen_decision_ranking_queries_execute_against_product_model() {
    let fixture = MockSearchFixture::with_decision_candidates();
    let bank: ControlledQueryBank =
        serde_json::from_str(DECISION_RANKING_BANK).expect("decision-ranking bank is valid");

    for case in bank.cases {
        let output = fixture.search(&case.query);
        assert_controlled_expectation(&case, &output);
    }
}

fn query(id: &str) -> &'static str {
    QUERY_BANK
        .get_or_init(|| serde_json::from_str(FROZEN_BANK).expect("frozen query bank is valid"))
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("missing frozen query {id}"))
        .query
        .as_str()
}

fn assert_branch_ids(output: ObservedSearch, expected: &[&[&str]]) {
    let actual = output
        .result_sets
        .iter()
        .map(|result_set| {
            result_set
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual, expected,
        "warnings: {:?}; negative_preferences={:?}",
        output.warnings, output.negative_preferences
    );
}

fn result_ids(output: &ObservedSearch) -> Vec<&str> {
    output
        .result_sets
        .iter()
        .flatten()
        .map(|result| result.id.as_str())
        .collect()
}

fn assert_contains_ids(output: &ObservedSearch, expected: &[&str]) {
    let actual = result_ids(output);
    for id in expected {
        assert!(
            actual.contains(id),
            "missing {id}; actual={actual:?}; warnings={:?}",
            output.warnings
        );
    }
}

fn assert_excludes_ids(output: &ObservedSearch, forbidden: &[&str]) {
    let actual = result_ids(output);
    for id in forbidden {
        assert!(
            !actual.contains(id),
            "unexpected {id}; actual={actual:?}; negative_preferences={:?}",
            output.negative_preferences
        );
    }
}

fn assert_first_id(output: &ObservedSearch, expected: &str) {
    let actual = result_ids(output);
    assert_eq!(
        actual.first().copied(),
        Some(expected),
        "actual={actual:?}; warnings={:?}",
        output.warnings
    );
}

fn assert_proves_places(output: &ObservedSearch, result_id: &str, labels: &[&str]) {
    let result = output
        .result_sets
        .iter()
        .flatten()
        .find(|result| result.id == result_id)
        .unwrap_or_else(|| panic!("missing result {result_id}"));
    for label in labels {
        assert!(
            result
                .proof_labels
                .iter()
                .any(|matched| matched.contains(label)),
            "{result_id} does not prove {label}: {:?}",
            result.proof_labels
        );
    }
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
    }
    if let Some(expected) = semantics.bhk {
        assert!(
            output.bhks.contains(&expected),
            "{} did not compile {expected} BHK; actual={:?}",
            case.id,
            output.bhks
        );
    }
    if let Some(expected) = semantics.budget_max {
        assert_eq!(
            output.budget_max,
            Some(expected),
            "{} compiled the wrong maximum budget",
            case.id
        );
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
        let distances = output
            .result_sets
            .iter()
            .flatten()
            .flat_map(|result| result.proof_distances_m.iter().copied())
            .collect::<Vec<_>>();
        if !distances.is_empty() {
            assert!(
                distances
                    .iter()
                    .all(|distance| f64::from(*distance) <= distance_km * 1_000.0),
                "{} did not enforce {distance_km} km; proof distances={distances:?}",
                case.id
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
                    result.area.eq_ignore_ascii_case(&branch.area)
                        && result.bhk == branch.bhk
                        && result.price <= branch.budget_max
                }),
                "{} branch {index} violated its area/BHK/budget contract",
                case.id
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
    bundle: LoadedServingBundle,
}

impl MockSearchFixture {
    fn new() -> Self {
        Self::build(false, false)
    }

    fn with_buyer_candidates() -> Self {
        Self::build(false, true)
    }

    fn with_decision_candidates() -> Self {
        Self::build(true, false)
    }

    fn build(include_decision_candidates: bool, include_distance_decoy: bool) -> Self {
        let mut builder = FixtureBuilder::default();
        builder.add_place("Hoodi Metro", "metro", 12.9900, 77.7150);
        builder.add_place("Manipal Hospital", "hospital", 12.9700, 77.7350);
        builder.add_place("Manipal Hospital Whitefield", "hospital", 12.9690, 77.7340);
        builder.add_place("Bagmane Tech Park", "tech_park", 12.9800, 77.6600);
        builder.add_place("Gopalan National School", "school", 12.9500, 77.6400);
        builder.add_place("Mock Metro Station", "metro", 12.8500, 77.6000);

        builder.add_home(HomeSpec::new(
            "mock-godrej-air-3bhk",
            "Godrej Air",
            "Hoodi",
            3,
            23_000_000,
            12.9910,
            77.7160,
        ));
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
        if include_distance_decoy {
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
        if include_decision_candidates {
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

        builder.build()
    }

    fn search(&self, query: &str) -> ObservedSearch {
        let index =
            SearchIndex::build_with_serving_entities(&self.properties, &self.bundle.entities);
        let society_names = self
            .properties
            .iter()
            .map(|property| (property.society_id.clone(), property.title.clone()))
            .collect::<HashMap<_, _>>();
        let property_by_id = self
            .properties
            .iter()
            .enumerate()
            .map(|(index, property)| (property.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let output = SearchEngine {
            properties: &self.properties,
            search_index: &index,
            serving_bundle: Some(&self.bundle),
            society_names: &society_names,
            property_by_id: Some(&property_by_id),
            societies: &[],
            graph: None,
        }
        .search(query);
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
        ObservedSearch {
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
}

#[derive(Default)]
struct FixtureBuilder {
    properties: Vec<Property>,
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    metadata: Vec<ServingSearchMetadataRecord>,
}

impl FixtureBuilder {
    fn add_place(&mut self, name: &str, category: &str, latitude: f64, longitude: f64) {
        let entity_id = format!("place:{}", slug(name));
        self.entities.push(entity(&entity_id, "place", name));
        self.add_fact(&entity_id, "geo.latitude", FactValue::Numeric(latitude));
        self.add_fact(&entity_id, "geo.longitude", FactValue::Numeric(longitude));
        self.add_fact(
            &entity_id,
            "place.category",
            FactValue::Text(category.to_string()),
        );
    }

    fn add_home(&mut self, spec: HomeSpec) {
        let society_id = slug(spec.society);
        let entity_id = format!("society:{society_id}");
        if !self
            .entities
            .iter()
            .any(|entity| entity.entity_id == entity_id)
        {
            self.entities
                .push(entity(&entity_id, "society", spec.society));
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
            FactValue::Text(spec.state.to_string()),
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

    fn build(self) -> MockSearchFixture {
        let fact_index = ServingFactIndex::from_records(self.facts.clone(), self.metadata);
        let entity_alias_index = ServingEntityAliasIndex::default();
        let temp_dir = tempdir().expect("temporary Tantivy directory");
        let recall_index =
            TantivyRecallIndex::build_in_dir(temp_dir.path(), &self.entities, &self.facts, &[])
                .expect("mock recall index");
        let geo_index = GeoSearchIndex::from_serving_bundle(&self.entities, &fact_index);
        let spatial_index = SpatialServingIndex::from_serving_bundle(&self.entities, &fact_index);
        let search_capabilities = SearchCapabilityIndex::from_bundle(&self.entities, &fact_index);
        let bundle = LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "conversational-semantics-mock".to_string(),
                format_version: 1,
                created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                entity_count: self.entities.len() as u64,
                entity_alias_count: 0,
                fact_count: self.facts.len() as u64,
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
            entities: self.entities,
            entity_alias_index,
            edges: Vec::new(),
            graph_index: GraphIndex::default(),
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
            bundle,
        }
    }
}

#[derive(Clone, Copy)]
struct HomeSpec {
    id: &'static str,
    society: &'static str,
    area: &'static str,
    bhk: u32,
    price: u64,
    latitude: f64,
    longitude: f64,
    state: &'static str,
    noise_score: Option<f64>,
    rating: Option<f64>,
}

impl HomeSpec {
    fn new(
        id: &'static str,
        society: &'static str,
        area: &'static str,
        bhk: u32,
        price: u64,
        latitude: f64,
        longitude: f64,
    ) -> Self {
        Self {
            id,
            society,
            area,
            bhk,
            price,
            latitude,
            longitude,
            state: "delivered",
            noise_score: Some(0.5),
            rating: Some(4.0),
        }
    }

    fn under_construction(mut self) -> Self {
        self.state = "under_construction";
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
        area_id: slug(spec.area),
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
        searchable_text: name.to_string(),
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
        skill_id: Some("search_conversational_semantics_contract".to_string()),
        learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
    }
}

fn slug(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
