use std::collections::HashMap;

use backend::knowledge::FactValue;
use backend::models::Property;
use backend::search::intent::{parse_intent, Polarity};
use backend::search::{HashSemanticEmbedder, SearchIndex, SemanticSearchIndex, TextSearch};
use backend::serving::{ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord};
use chrono::Utc;

const MIN_TARGETS_AT_RANK_ONE: usize = 20;
const MIN_MEAN_RECIPROCAL_RANK: f64 = 0.86;

#[test]
fn config_backed_semantic_quality_suite_ranks_evidence_targets() {
    let cases = semantic_quality_cases();
    let mut rank_one_count = 0;
    let mut reciprocal_rank_sum = 0.0;

    for case in &cases {
        assert_case_is_config_backed(case);
        let world = MockSearchWorld::for_case(case);
        let results = world.search(case.query);
        let target_rank = results
            .iter()
            .position(|result| result.card.id == case.target_id)
            .map(|rank| rank + 1)
            .unwrap_or(usize::MAX);

        if target_rank == 1 {
            rank_one_count += 1;
        }
        reciprocal_rank_sum += 1.0 / target_rank as f64;

        assert!(
            target_rank <= 3,
            "{} target ranked {target_rank}; top results were {:?}",
            case.name,
            results
                .iter()
                .take(5)
                .map(|result| (&result.card.id, result.semantic_score))
                .collect::<Vec<_>>()
        );

        let target = results
            .iter()
            .find(|result| result.card.id == case.target_id)
            .expect("target should be returned");
        let explanation = target
            .match_explanation
            .as_ref()
            .expect("preference query should produce evidence coverage");
        assert!(
            explanation
                .reasons
                .iter()
                .any(|reason| reason.fact_key == case.target_fact_key),
            "{} should be explained by {}, got {:?}",
            case.name,
            case.target_fact_key,
            explanation.reasons
        );
        assert!(
            target.semantic_score.unwrap_or(0.0) > 0.0,
            "{} should have semantic recall metadata",
            case.name
        );
    }

    let mean_reciprocal_rank = reciprocal_rank_sum / cases.len() as f64;
    assert!(
        rank_one_count >= MIN_TARGETS_AT_RANK_ONE,
        "rank@1 {rank_one_count}/{} below target",
        cases.len()
    );
    assert!(
        mean_reciprocal_rank >= MIN_MEAN_RECIPROCAL_RANK,
        "MRR {mean_reciprocal_rank:.3} below {MIN_MEAN_RECIPROCAL_RANK:.3}"
    );
}

#[test]
fn generic_home_language_does_not_create_false_lexical_winners() {
    let case = SemanticQualityCase {
        name: "generic home language trap",
        query: "peaceful home for parents near hospital",
        target_id: "target-hospital",
        target_fact_key: "nearby_hospitals",
        target_fact: fact(
            "target-hospital",
            "nearby_hospitals",
            FactValueSpec::Text("Carewell Hospital (0.4 km, 4.6 rating, 900 reviews)"),
            "social infrastructure",
        ),
        target_document: "senior friendly quiet community with healthcare access",
        expected_signals: vec![expected(
            "social infrastructure",
            Polarity::Positive,
            "nearby_hospitals",
        )],
        distractor_documents: &[
            (
                "generic-homes-1",
                "Nikoo Homes brand name without healthcare evidence",
            ),
            (
                "generic-homes-2",
                "Dream Homes project title without hospital access",
            ),
        ],
        extra_facts: vec![],
    };
    let world = MockSearchWorld::for_case(&case);
    let results = world.search(case.query);

    assert_eq!(results[0].card.id, case.target_id);
    assert!(
        !results[0].match_reason.contains("matched 'home'"),
        "generic buyer words should not explain ranking: {}",
        results[0].match_reason
    );
}

#[test]
fn semantic_similarity_without_evidence_does_not_outrank_serving_proof() {
    let case = SemanticQualityCase {
        name: "proof beats semantic-only hospital copy",
        query: "peaceful home for parents near hospital",
        target_id: "target-hospital-proof",
        target_fact_key: "nearby_hospitals",
        target_fact: fact(
            "target-hospital-proof",
            "nearby_hospitals",
            FactValueSpec::Text("Carewell Hospital (0.8 km, 4.5 rating, 700 reviews)"),
            "social infrastructure",
        ),
        target_document: "quiet senior friendly residence with healthcare access nearby",
        expected_signals: vec![expected(
            "social infrastructure",
            Polarity::Positive,
            "nearby_hospitals",
        )],
        distractor_documents: &[
            (
                "semantic-only-hospital-copy",
                "hospital hospital clinic parents senior peaceful healthcare home",
            ),
            (
                "wrong-category-nearby-proof",
                "family school infrastructure near campus and shops",
            ),
        ],
        extra_facts: vec![fact(
            "wrong-category-nearby-proof",
            "nearby_schools",
            FactValueSpec::Text("Northstar School (0.2 km, 4.7 rating, 500 reviews)"),
            "social infrastructure",
        )],
    };
    let world = MockSearchWorld::for_case(&case);
    let results = world.search(case.query);

    assert_eq!(results[0].card.id, case.target_id);
    let top_explanation = results[0].match_explanation.as_ref().unwrap();
    assert!(
        top_explanation
            .reasons
            .iter()
            .any(|reason| reason.fact_key == "nearby_hospitals"),
        "hospital intent should be proved by hospital evidence: {:?}",
        top_explanation.reasons
    );
}

#[derive(Clone)]
struct ExpectedSignal {
    label: &'static str,
    polarity: Polarity,
    fact_key: &'static str,
}

#[derive(Clone, Copy)]
struct MockFactSpec {
    property_id: &'static str,
    fact_key: &'static str,
    value: FactValueSpec,
    preference: &'static str,
}

#[derive(Clone, Copy)]
enum FactValueSpec {
    Text(&'static str),
    Numeric(f64),
    Tags(&'static [&'static str]),
}

struct SemanticQualityCase {
    name: &'static str,
    query: &'static str,
    target_id: &'static str,
    target_fact_key: &'static str,
    target_fact: MockFactSpec,
    target_document: &'static str,
    expected_signals: Vec<ExpectedSignal>,
    distractor_documents: &'static [(&'static str, &'static str)],
    extra_facts: Vec<MockFactSpec>,
}

fn expected(label: &'static str, polarity: Polarity, fact_key: &'static str) -> ExpectedSignal {
    ExpectedSignal {
        label,
        polarity,
        fact_key,
    }
}

const fn fact(
    property_id: &'static str,
    fact_key: &'static str,
    value: FactValueSpec,
    preference: &'static str,
) -> MockFactSpec {
    MockFactSpec {
        property_id,
        fact_key,
        value,
        preference,
    }
}

fn semantic_quality_cases() -> Vec<SemanticQualityCase> {
    vec![
        SemanticQualityCase {
            name: "hospital access for parents",
            query: "peaceful home for parents near hospital",
            target_id: "target-hospital",
            target_fact_key: "nearby_hospitals",
            target_fact: fact(
                "target-hospital",
                "nearby_hospitals",
                FactValueSpec::Text("Carewell Hospital (0.4 km, 4.6 rating, 900 reviews)"),
                "social infrastructure",
            ),
            target_document: "senior friendly quiet community with healthcare access",
            expected_signals: vec![expected(
                "social infrastructure",
                Polarity::Positive,
                "nearby_hospitals",
            )],
            distractor_documents: &[
                ("distractor-school", "kids school campus with playground"),
                ("distractor-cafe", "restaurants cafes and weekend retail"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "school access for family",
            query: "family home near schools for kids",
            target_id: "target-school",
            target_fact_key: "nearby_schools",
            target_fact: fact(
                "target-school",
                "nearby_schools",
                FactValueSpec::Text("Northstar School (0.3 km, 4.7 rating, 500 reviews)"),
                "social infrastructure",
            ),
            target_document: "children friendly family campus with primary school access",
            expected_signals: vec![expected(
                "social infrastructure",
                Polarity::Positive,
                "nearby_schools",
            )],
            distractor_documents: &[
                ("distractor-hospital", "senior healthcare hospital access"),
                ("distractor-office", "office commute tech park corridor"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "metro commute",
            query: "easy office commute by metro",
            target_id: "target-metro",
            target_fact_key: "nearest_operational_metro_station",
            target_fact: fact(
                "target-metro",
                "nearest_operational_metro_station",
                FactValueSpec::Text("Kadugodi Tree Park metro (0.6 km, operational)"),
                "metro access",
            ),
            target_document: "transit commute connectivity for office workers",
            expected_signals: vec![expected(
                "metro access",
                Polarity::Positive,
                "nearest_operational_metro_station",
            )],
            distractor_documents: &[
                (
                    "distractor-road",
                    "wide road car commute but no train station",
                ),
                ("distractor-remote", "quiet remote residential enclave"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "greenery",
            query: "green calm apartment with trees",
            target_id: "target-greenery",
            target_fact_key: "green_cover",
            target_fact: fact(
                "target-greenery",
                "green_cover",
                FactValueSpec::Text("mature trees and landscaped open space"),
                "greenery",
            ),
            target_document: "landscaped garden tree cover nature open space",
            expected_signals: vec![expected("greenery", Polarity::Positive, "green_cover")],
            distractor_documents: &[
                ("distractor-amenities", "clubhouse pool indoor amenities"),
                ("distractor-market", "retail high street and restaurants"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "review quality",
            query: "good reviews from actual residents",
            target_id: "target-reviews",
            target_fact_key: "google_top_positives",
            target_fact: fact(
                "target-reviews",
                "google_top_positives",
                FactValueSpec::Text("residents praise maintenance, security, and community"),
                "review quality",
            ),
            target_document: "resident community review lived experience positive feedback",
            expected_signals: vec![expected(
                "review quality",
                Polarity::Positive,
                "google_top_positives",
            )],
            distractor_documents: &[
                ("distractor-builder", "builder marketing launch brochure"),
                ("distractor-empty", "thin listing with no lived feedback"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "approach road",
            query: "good approach road and access",
            target_id: "target-approach-road",
            target_fact_key: "approach_road_condition",
            target_fact: fact(
                "target-approach-road",
                "approach_road_condition",
                FactValueSpec::Text("wide road and smooth approach with usable access proof"),
                "approach road",
            ),
            target_document: "access road approach connectivity wide paved road",
            expected_signals: vec![expected(
                "approach road",
                Polarity::Positive,
                "approach_road_condition",
            )],
            distractor_documents: &[
                (
                    "distractor-internal-road",
                    "internal driveway landscaping only",
                ),
                ("distractor-amenity-road", "clubhouse next to road"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "amenity quality",
            query: "society with excellent clubhouse pool and amenities",
            target_id: "target-amenities",
            target_fact_key: "amenity_quality",
            target_fact: fact(
                "target-amenities",
                "amenity_quality",
                FactValueSpec::Text("clubhouse, pool, gym, and sports amenities are active"),
                "amenity quality",
            ),
            target_document: "active clubhouse swimming pool sports gym community amenities",
            expected_signals: vec![expected(
                "amenity quality",
                Polarity::Positive,
                "amenity_quality",
            )],
            distractor_documents: &[
                ("distractor-empty-club", "clubhouse promised in brochure"),
                ("distractor-maintenance", "well maintained tower lobby"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "maintenance",
            query: "well maintained society with good maintenance",
            target_id: "target-maintenance",
            target_fact_key: "maintenance_quality",
            target_fact: fact(
                "target-maintenance",
                "maintenance_quality",
                FactValueSpec::Text("well maintained common areas and responsive association"),
                "maintenance",
            ),
            target_document: "maintained society association cleaning security repairs",
            expected_signals: vec![expected(
                "maintenance",
                Polarity::Positive,
                "maintenance_quality",
            )],
            distractor_documents: &[
                (
                    "distractor-cheap",
                    "low maintenance charges but unclear upkeep",
                ),
                (
                    "distractor-new",
                    "new handover with no resident operating history",
                ),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "liveability",
            query: "liveable society with families already living",
            target_id: "target-liveability",
            target_fact_key: "livability_sentiment",
            target_fact: fact(
                "target-liveability",
                "livability_sentiment",
                FactValueSpec::Text("families living, occupied towers, and active community"),
                "liveability",
            ),
            target_document:
                "liveable society families living occupied end use community active residents",
            expected_signals: vec![expected(
                "liveability",
                Polarity::Positive,
                "livability_sentiment",
            )],
            distractor_documents: &[
                ("distractor-investor", "launch project with future upside"),
                ("distractor-vacant", "empty layout with few residents"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "area development",
            query: "developed around with future area development",
            target_id: "target-area-development",
            target_fact_key: "area_development_signal",
            target_fact: fact(
                "target-area-development",
                "area_development_signal",
                FactValueSpec::Text("developing corridor with upcoming metro and retail"),
                "area development",
            ),
            target_document: "surrounding development infrastructure retail metro corridor",
            expected_signals: vec![expected(
                "area development",
                Polarity::Positive,
                "area_development_signal",
            )],
            distractor_documents: &[
                (
                    "distractor-isolated",
                    "quiet isolated layout nothing around",
                ),
                ("distractor-luxury", "premium tower finishes"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "water supply",
            query: "good water supply with cauvery and no tanker issue",
            target_id: "target-water",
            target_fact_key: "water_supply",
            target_fact: fact(
                "target-water",
                "water_supply",
                FactValueSpec::Text("cauvery connection and no water issue mentioned"),
                "water supply",
            ),
            target_document: "cauvery water supply borewell backup no tanker dependence",
            expected_signals: vec![expected("water supply", Polarity::Positive, "water_supply")],
            distractor_documents: &[
                ("distractor-pool", "swimming pool and water features"),
                ("distractor-tanker", "tanker water dependency concerns"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "rental and resale demand",
            query: "good rental yield and resale demand for investment",
            target_id: "target-rental-resale",
            target_fact_key: "rental_yield_signal",
            target_fact: fact(
                "target-rental-resale",
                "rental_yield_signal",
                FactValueSpec::Text("strong tenant interest and healthy rental demand"),
                "rental and resale demand",
            ),
            target_document: "investment rental tenant yield resale liquidity demand",
            expected_signals: vec![expected(
                "rental and resale demand",
                Polarity::Positive,
                "rental_yield_signal",
            )],
            distractor_documents: &[
                ("distractor-self-use", "peaceful end use home"),
                ("distractor-pricey", "expensive premium tower low yield"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "reddit discussions",
            query: "reddit resident feedback and reddit reviews",
            target_id: "target-reddit",
            target_fact_key: "reddit_threads",
            target_fact: fact(
                "target-reddit",
                "reddit_threads",
                FactValueSpec::Tags(&["resident feedback", "maintenance discussion"]),
                "reddit discussions",
            ),
            target_document: "reddit discussions resident feedback forum comments",
            expected_signals: vec![expected(
                "reddit discussions",
                Polarity::Positive,
                "reddit_threads",
            )],
            distractor_documents: &[
                ("distractor-google", "google reviews only"),
                ("distractor-social", "social media marketing posts"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "delivered society",
            query: "ready to move delivered project",
            target_id: "target-delivered",
            target_fact_key: "possession_status",
            target_fact: fact(
                "target-delivered",
                "possession_status",
                FactValueSpec::Text("ready to move delivered completed"),
                "delivered society",
            ),
            target_document: "completed delivered ready resale occupied",
            expected_signals: vec![expected(
                "delivered society",
                Polarity::Positive,
                "possession_status",
            )],
            distractor_documents: &[
                (
                    "distractor-under-construction",
                    "under construction future project",
                ),
                ("distractor-new-launch", "new launch inventory"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "new property",
            query: "new property recently delivered society",
            target_id: "target-new-property",
            target_fact_key: "home_age_bucket",
            target_fact: fact(
                "target-new-property",
                "home_age_bucket",
                FactValueSpec::Text("newly delivered 1-5 yrs old"),
                "new property",
            ),
            target_document: "newly delivered recent society fresh handover",
            expected_signals: vec![expected(
                "new property",
                Polarity::Positive,
                "home_age_bucket",
            )],
            distractor_documents: &[
                ("distractor-old", "mature old established society"),
                ("distractor-launch", "upcoming launch not delivered"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "established society",
            query: "mature established society with older property",
            target_id: "target-established",
            target_fact_key: "home_age_bucket",
            target_fact: fact(
                "target-established",
                "home_age_bucket",
                FactValueSpec::Text("5-10 yrs old mature established society"),
                "established society",
            ),
            target_document: "mature established community older property operating history",
            expected_signals: vec![expected(
                "established society",
                Polarity::Positive,
                "home_age_bucket",
            )],
            distractor_documents: &[
                ("distractor-newer", "newly delivered recent project"),
                ("distractor-under-construction", "ongoing future project"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "under construction",
            query: "under construction upcoming project",
            target_id: "target-under-construction",
            target_fact_key: "home_state",
            target_fact: fact(
                "target-under-construction",
                "home_state",
                FactValueSpec::Text("under construction ongoing upcoming project"),
                "under construction",
            ),
            target_document: "under construction ongoing upcoming launch project",
            expected_signals: vec![expected(
                "under construction",
                Polarity::Positive,
                "home_state",
            )],
            distractor_documents: &[
                ("distractor-ready", "ready to move completed project"),
                ("distractor-resale", "mature resale society"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "airport access",
            query: "near airport with airport access",
            target_id: "target-airport",
            target_fact_key: "airport_distance_km",
            target_fact: fact(
                "target-airport",
                "airport_distance_km",
                FactValueSpec::Numeric(18.0),
                "airport access",
            ),
            target_document: "airport road access close to airport corridor",
            expected_signals: vec![expected(
                "airport access",
                Polarity::Positive,
                "airport_distance_km",
            )],
            distractor_documents: &[
                ("distractor-noise", "airport noise and aircraft sound"),
                ("distractor-metro", "metro access but far from airport"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "lake proximity",
            query: "lake proximity and lake nearby",
            target_id: "target-lake",
            target_fact_key: "lake_proximity",
            target_fact: fact(
                "target-lake",
                "lake_proximity",
                FactValueSpec::Text("lake view with no lake flooding reported"),
                "lake proximity",
            ),
            target_document: "lake nearby lake view open space water body",
            expected_signals: vec![expected(
                "lake proximity",
                Polarity::Positive,
                "lake_proximity",
            )],
            distractor_documents: &[
                (
                    "distractor-waterlogging",
                    "lake flooding and waterlogging near lake",
                ),
                ("distractor-garden", "garden open space but no lake"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "environment sensitivity",
            query: "quiet surroundings with low pollution",
            target_id: "target-environment",
            target_fact_key: "environment_sensitivity",
            target_fact: fact(
                "target-environment",
                "environment_sensitivity",
                FactValueSpec::Text("quiet peaceful clean surroundings with low pollution"),
                "environment sensitivity",
            ),
            target_document: "quiet peaceful clean surroundings low pollution calm",
            expected_signals: vec![expected(
                "environment sensitivity",
                Polarity::Positive,
                "environment_sensitivity",
            )],
            distractor_documents: &[
                (
                    "distractor-airport-noise",
                    "airport noise and traffic corridor",
                ),
                ("distractor-commercial", "busy commercial high street"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "low traffic",
            query: "low traffic and smooth commute",
            target_id: "target-low-traffic",
            target_fact_key: "traffic_score",
            target_fact: fact(
                "target-low-traffic",
                "traffic_score",
                FactValueSpec::Numeric(0.12),
                "traffic",
            ),
            target_document: "smooth commute low traffic predictable access",
            expected_signals: vec![expected("traffic", Polarity::Negative, "traffic_score")],
            distractor_documents: &[
                (
                    "distractor-metro-traffic",
                    "metro commute but traffic congestion",
                ),
                ("distractor-road-only", "wide road but severe traffic"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "low waterlogging",
            query: "avoid waterlogging and no flooding",
            target_id: "target-low-waterlogging",
            target_fact_key: "waterlogging_risk_score",
            target_fact: fact(
                "target-low-waterlogging",
                "waterlogging_risk_score",
                FactValueSpec::Numeric(0.08),
                "waterlogging risk",
            ),
            target_document: "not flood prone no waterlogging dry approach",
            expected_signals: vec![expected(
                "waterlogging risk",
                Polarity::Negative,
                "waterlogging_risk_score",
            )],
            distractor_documents: &[
                (
                    "distractor-lake-risk",
                    "lake overflow flooding waterlogging",
                ),
                (
                    "distractor-road-risk",
                    "approach road waterlogging mentioned",
                ),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "low noise",
            query: "avoid noise and low airport noise",
            target_id: "target-low-noise",
            target_fact_key: "noise_score",
            target_fact: fact(
                "target-low-noise",
                "noise_score",
                FactValueSpec::Numeric(0.15),
                "noise",
            ),
            target_document: "quiet low noise peaceful away from airport noise",
            expected_signals: vec![expected("noise", Polarity::Negative, "noise_score")],
            distractor_documents: &[
                ("distractor-aircraft", "aircraft noise and busy road"),
                (
                    "distractor-commercial-noise",
                    "commercial noise and construction noise",
                ),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "on-time delivery",
            query: "no possession delay and not delayed",
            target_id: "target-no-delay",
            target_fact_key: "home_timeline_state",
            target_fact: fact(
                "target-no-delay",
                "home_timeline_state",
                FactValueSpec::Text("on track not delayed"),
                "delay risk",
            ),
            target_document: "on track delivery no possession delay",
            expected_signals: vec![expected(
                "delay risk",
                Polarity::Negative,
                "home_timeline_state",
            )],
            distractor_documents: &[
                ("distractor-delayed", "delayed possession handover delay"),
                (
                    "distractor-ready",
                    "ready to move but delay history unknown",
                ),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "avoid bad approach road",
            query: "avoid bad approach road and narrow road",
            target_id: "target-safe-road",
            target_fact_key: "approach_road_condition",
            target_fact: fact(
                "target-safe-road",
                "approach_road_condition",
                FactValueSpec::Text("wide road and good access road"),
                "approach road",
            ),
            target_document: "wide road good access smooth approach",
            expected_signals: vec![expected(
                "approach road",
                Polarity::Negative,
                "approach_road_condition",
            )],
            distractor_documents: &[
                ("distractor-narrow-road", "narrow approach road single lane"),
                (
                    "distractor-road-digging",
                    "road digging and poor road access",
                ),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "avoid airport noise",
            query: "far from airport noise with quiet surroundings",
            target_id: "target-low-airport-noise",
            target_fact_key: "airport_noise_score",
            target_fact: fact(
                "target-low-airport-noise",
                "airport_noise_score",
                FactValueSpec::Numeric(0.1),
                "environment sensitivity",
            ),
            target_document: "quiet surroundings low airport noise calm environment",
            expected_signals: vec![expected(
                "environment sensitivity",
                Polarity::Negative,
                "airport_noise_score",
            )],
            distractor_documents: &[
                ("distractor-airport-close", "near airport aircraft noise"),
                ("distractor-pollution", "pollution and dust on main road"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "avoid lake flooding",
            query: "avoid lake flooding and away from lake",
            target_id: "target-away-lake",
            target_fact_key: "lake_waterlogging_context",
            target_fact: fact(
                "target-away-lake",
                "lake_waterlogging_context",
                FactValueSpec::Text("not near lake and no lake flooding"),
                "lake proximity",
            ),
            target_document: "away from lake no lake flooding dry surroundings",
            expected_signals: vec![expected(
                "lake proximity",
                Polarity::Negative,
                "lake_waterlogging_context",
            )],
            distractor_documents: &[
                ("distractor-lake-view-risk", "near lake with lake overflow"),
                ("distractor-waterfront", "waterfront lake nearby"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "avoid stp smell",
            query: "avoid stp smell and sewage treatment plant issue",
            target_id: "target-stp-ok",
            target_fact_key: "stp_concern",
            target_fact: fact(
                "target-stp-ok",
                "stp_concern",
                FactValueSpec::Text("stp maintained and no sewage smell"),
                "stp concern",
            ),
            target_document: "stp maintained no sewage smell treatment works",
            expected_signals: vec![expected("stp concern", Polarity::Negative, "stp_concern")],
            distractor_documents: &[
                ("distractor-stp-smell", "sewage smell from stp"),
                ("distractor-plant-nearby", "sewage treatment plant issue"),
            ],
            extra_facts: vec![],
        },
        SemanticQualityCase {
            name: "avoid high tension wires",
            query: "avoid high tension wires and power lines",
            target_id: "target-no-ht",
            target_fact_key: "high_tension_wire_concern",
            target_fact: fact(
                "target-no-ht",
                "high_tension_wire_concern",
                FactValueSpec::Text("away from power lines and no high tension wires"),
                "high tension wires",
            ),
            target_document: "away from power lines no high tension wires",
            expected_signals: vec![expected(
                "high tension wires",
                Polarity::Negative,
                "high_tension_wire_concern",
            )],
            distractor_documents: &[
                ("distractor-power-lines", "high tension line beside tower"),
                (
                    "distractor-transmission",
                    "transmission line and power lines nearby",
                ),
            ],
            extra_facts: vec![],
        },
    ]
}

fn assert_case_is_config_backed(case: &SemanticQualityCase) {
    let intent = parse_intent(case.query);
    for expected in &case.expected_signals {
        let signals = match expected.polarity {
            Polarity::Positive => &intent.positive_preferences,
            Polarity::Negative => &intent.negative_preferences,
        };
        let signal = signals
            .iter()
            .find(|signal| signal.raw_text == expected.label)
            .unwrap_or_else(|| {
                panic!(
                    "{} should parse {} signal {:?} from {:?}; positive={:?} negative={:?}",
                    case.name,
                    polarity_name(&expected.polarity),
                    expected.label,
                    case.query,
                    intent.positive_preferences,
                    intent.negative_preferences
                )
            });
        assert!(
            signal
                .expanded_keys
                .iter()
                .any(|key| key == expected.fact_key),
            "{} {} keys should include {}, got {:?}",
            case.name,
            expected.label,
            expected.fact_key,
            signal.expanded_keys
        );
    }
}

fn polarity_name(polarity: &Polarity) -> &'static str {
    match polarity {
        Polarity::Positive => "positive",
        Polarity::Negative => "negative",
    }
}

struct MockSearchWorld {
    properties: Vec<Property>,
    search_index: SearchIndex,
    semantic_index: SemanticSearchIndex,
    serving_facts: ServingFactIndex,
    society_names: HashMap<String, String>,
    embedder: HashSemanticEmbedder,
}

impl MockSearchWorld {
    fn for_case(case: &SemanticQualityCase) -> Self {
        let mut properties = vec![property(case.target_id, case.target_document)];
        for (id, document) in case.distractor_documents {
            properties.push(property(id, document));
        }
        for i in 0..32 {
            properties.push(property(
                &format!("background-{i}"),
                "ordinary apartment listing with clubhouse and resale details",
            ));
        }

        let search_index = SearchIndex::build(&properties);
        let embedder = HashSemanticEmbedder::default();
        let semantic_index = SemanticSearchIndex::from_properties(&properties, &embedder);

        let mut fact_specs = vec![case.target_fact];
        fact_specs.extend(case.extra_facts.iter().copied());
        let facts = fact_specs
            .iter()
            .map(|spec| {
                serving_fact(
                    &society_node_id(spec.property_id),
                    spec.fact_key,
                    spec.value,
                )
            })
            .collect();
        let metadata = fact_specs
            .iter()
            .map(|spec| {
                search_metadata(
                    &society_node_id(spec.property_id),
                    spec.fact_key,
                    spec.preference,
                )
            })
            .collect();
        let serving_facts = ServingFactIndex::from_records(facts, metadata);
        let society_names = properties
            .iter()
            .map(|property| (property.society_id.clone(), property.title.clone()))
            .collect();

        Self {
            properties,
            search_index,
            semantic_index,
            serving_facts,
            society_names,
            embedder,
        }
    }

    fn search(&self, query: &str) -> Vec<backend::search::SearchResultCard> {
        let intent = parse_intent(query);
        let semantic_hits = self.semantic_index.search(query, &self.embedder, 64);
        let semantic_scores = self
            .search_index
            .property_scores_for_semantic_hits(&semantic_hits);
        let candidate_ids = semantic_scores.keys().cloned().collect::<Vec<_>>();
        TextSearch::search_with_index_extra_recall_semantic_scores_serving_facts_and_intent_and_sellers(
            &self.properties,
            Some(&self.search_index),
            Some(&candidate_ids),
            Some(&semantic_scores),
            None,
            Some(&self.serving_facts),
            &self.society_names,
            &[],
            query,
            &intent,
            None,
            &[],
        )
    }
}

fn property(id: &str, description_summary: &str) -> Property {
    Property {
        id: id.to_string(),
        title: format!("Mock residence {id}"),
        area: "Quality Test Area".to_string(),
        area_id: "quality-test-area".to_string(),
        city: "Bengaluru".to_string(),
        society_id: id.to_string(),
        builder_name: "Quality Builder".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk: 3,
        price: 18_000_000,
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
        noise_score: Some(0.3),
        sunlight_score: Some(0.7),
        airport_noise_score: Some(0.1),
        waterlogging_risk_score: Some(0.2),
        traffic_score: Some(0.3),
        days_on_market: 20,
        greenery_score: Some(0.5),
        open_space_score: Some(0.5),
        resale_strength_score: Some(0.7),
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: description_summary.to_string(),
        transparency_tags: Vec::new(),
        source_reference: "semantic-quality-contract".to_string(),
        seller_id: None,
    }
}

fn serving_fact(entity_id: &str, fact_key: &str, value: FactValueSpec) -> ServingFactRecord {
    let value = fact_value(value);
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type(&value).to_string(),
        value_text: fact_value_text(&value),
        value,
        confidence: 0.9,
        source_type: "Google".to_string(),
        source_url: None,
        model: None,
        skill_id: Some("semantic_quality_mock".to_string()),
        learned_at: Utc::now(),
    }
}

fn fact_value(value: FactValueSpec) -> FactValue {
    match value {
        FactValueSpec::Text(value) => FactValue::Text(value.to_string()),
        FactValueSpec::Numeric(value) => FactValue::Numeric(value),
        FactValueSpec::Tags(values) => {
            FactValue::Tags(values.iter().map(|value| (*value).to_string()).collect())
        }
    }
}

fn value_type(value: &FactValue) -> &'static str {
    match value {
        FactValue::Text(_) => "text",
        FactValue::Numeric(_) => "number",
        FactValue::Tags(_) => "tags",
        FactValue::Bool(_) => "bool",
        FactValue::Score { .. } => "score",
    }
}

fn fact_value_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(value) => Some(value.clone()),
        FactValue::Numeric(value) => Some(value.to_string()),
        FactValue::Tags(values) => Some(values.join(", ")),
        FactValue::Bool(value) => Some(value.to_string()),
        FactValue::Score { explanation, .. } => Some(explanation.clone()),
    }
}

fn search_metadata(
    entity_id: &str,
    fact_key: &str,
    preference: &str,
) -> ServingSearchMetadataRecord {
    ServingSearchMetadataRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(format!("{preference}: {{value}}")),
        answers_preferences: vec![preference.to_string()],
        scoring_direction: Some("TextMatch".to_string()),
        scoring_weight: Some(1.2),
        scoring_thresholds: Vec::new(),
    }
}

fn society_node_id(society_id: &str) -> String {
    format!("society:{society_id}")
}
