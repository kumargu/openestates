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
struct ControlledQueryCase {
    id: String,
    query: String,
    expected_semantics: ExpectedSemantics,
    fixture_expectation: FixtureExpectation,
}

#[derive(Deserialize)]
struct ExpectedSemantics {
    #[serde(default)]
    positive_preferences: Vec<String>,
    #[serde(default)]
    negative_preferences: Vec<String>,
    #[serde(default)]
    ranking_priorities: Vec<String>,
    #[serde(default)]
    accepted_tradeoffs: Vec<String>,
    numeric_min: Option<NumericExpectation>,
}

#[derive(Deserialize)]
struct NumericExpectation {
    field: String,
    value: f64,
}

#[derive(Deserialize)]
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
    let fixture = MockSearchFixture::new();
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

    for expected_preference in &case.expected_semantics.positive_preferences {
        assert!(
            output.positive_preferences.contains(expected_preference),
            "{} did not compile positive preference {expected_preference:?}; actual={:?}",
            case.id,
            output.positive_preferences
        );
    }
    for expected_preference in &case.expected_semantics.negative_preferences {
        assert!(
            output.negative_preferences.contains(expected_preference),
            "{} did not compile negative preference {expected_preference:?}; actual={:?}",
            case.id,
            output.negative_preferences
        );
    }
    for expected_tradeoff in &case.expected_semantics.accepted_tradeoffs {
        assert!(
            output.accepted_tradeoffs.contains(expected_tradeoff),
            "{} did not compile accepted tradeoff {expected_tradeoff:?}; actual={:?}",
            case.id,
            output.accepted_tradeoffs
        );
    }
    if !case.expected_semantics.ranking_priorities.is_empty() {
        assert_eq!(
            output.ranking_priorities, case.expected_semantics.ranking_priorities,
            "{} compiled the wrong ranking priorities",
            case.id
        );
    }
    if let Some(expected) = &case.expected_semantics.numeric_min {
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
    for forbidden_label in &expectation.forbidden_proof_labels {
        assert!(
            output
                .result_sets
                .iter()
                .flatten()
                .flat_map(|result| &result.proof_labels)
                .all(|label| !label.to_ascii_lowercase().contains(forbidden_label)),
            "{} fabricated forbidden proof {forbidden_label:?}",
            case.id
        );
    }
}

struct ObservedSearch {
    result_sets: Vec<Vec<ObservedResult>>,
    warnings: Vec<String>,
    positive_preferences: Vec<String>,
    negative_preferences: Vec<String>,
    ranking_priorities: Vec<String>,
    accepted_tradeoffs: Vec<String>,
    min_constraints: Vec<(String, f64)>,
}

struct ObservedResult {
    id: String,
    proof_labels: Vec<String>,
}

struct MockSearchFixture {
    properties: Vec<Property>,
    bundle: LoadedServingBundle,
}

impl MockSearchFixture {
    fn new() -> Self {
        Self::build(false)
    }

    fn with_decision_candidates() -> Self {
        Self::build(true)
    }

    fn build(include_decision_candidates: bool) -> Self {
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
            .quality(Some(0.95), Some(4.7)),
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
                .quality(Some(0.99), Some(4.0)),
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
                .quality(Some(0.8), Some(4.9)),
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
                .quality(Some(0.8), Some(4.2)),
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
        ObservedSearch {
            result_sets: output
                .result_sets
                .into_iter()
                .map(|result_set| {
                    result_set
                        .results
                        .into_iter()
                        .map(|result| ObservedResult {
                            id: result.card.id,
                            proof_labels: result
                                .proof_focuses
                                .into_iter()
                                .filter_map(|focus| focus.matched_label)
                                .collect(),
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
        if let Some(quiet) = spec.quiet {
            self.add_search_fact(
                &entity_id,
                "noise_score",
                FactValue::Numeric(quiet),
                &["quiet", "quiet surroundings"],
                Some("HigherIsBetter"),
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
    quiet: Option<f64>,
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
            quiet: Some(0.5),
            rating: Some(4.0),
        }
    }

    fn under_construction(mut self) -> Self {
        self.state = "under_construction";
        self
    }

    fn quality(mut self, quiet: Option<f64>, rating: Option<f64>) -> Self {
        self.quiet = quiet;
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
        noise_score: spec.quiet,
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
