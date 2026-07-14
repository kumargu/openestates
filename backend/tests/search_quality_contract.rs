use std::collections::HashMap;

use backend::assets::{
    read_skill_fact_artifact_rows, KgSocietyViewMaterializer, SkillFactAnnotationRecord,
    SkillFactMaterializer, SkillFactRecord,
};
use backend::knowledge::edge::{Edge, Relation};
use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::LakeStore;
use backend::models::{Property, Society};
use backend::search::intent::parse_intent;
use backend::search::{SearchIndex, SearchResultCard, TextSearch};
use backend::serving::{SearchServingBundleMaterializer, ServingBundleLoader, ServingFactIndex};
use chrono::Utc;
use tempfile::tempdir;

const SQM_PER_ACRE: f64 = 4046.8564224;

#[test]
fn proof_first_acreage_query_requires_rera_and_uses_greenery_as_support() {
    let mut world = SearchWorld::new(vec![
        property("large-green", "Whitefield", "large-green", 3, 19_000_000),
        property("large-plain", "Whitefield", "large-plain", 3, 18_500_000),
        property("small-green", "Whitefield", "small-green", 3, 18_000_000),
        property("unknown-area", "Whitefield", "unknown-area", 3, 17_500_000),
    ]);
    for property in &mut world.properties {
        property.greenery_score = None;
        property.open_space_score = None;
    }

    world.add_society(
        "large-green",
        RootSource::Rera,
        vec![
            rera_numeric_fact("rera_total_land_area_sqm", 12.0 * SQM_PER_ACRE),
            sourced_fact(
                "google_top_positives",
                FactValue::Tags(vec![
                    "wide open spaces with many trees".to_string(),
                    "landscaped green cover across the campus".to_string(),
                ]),
                SourceType::Google,
                0.8,
                None,
                vec![],
            ),
        ],
    );
    world.add_society(
        "large-plain",
        RootSource::Rera,
        vec![rera_numeric_fact(
            "rera_total_land_area_sqm",
            11.0 * SQM_PER_ACRE,
        )],
    );
    world.add_society(
        "small-green",
        RootSource::Rera,
        vec![
            rera_numeric_fact("rera_total_land_area_sqm", 5.0 * SQM_PER_ACRE),
            sourced_fact(
                "google_top_positives",
                FactValue::Tags(vec!["landscaped garden with trees".to_string()]),
                SourceType::Google,
                0.8,
                None,
                vec![],
            ),
        ],
    );
    world.add_society(
        "unknown-area",
        RootSource::Discovered,
        vec![sourced_fact(
            "google_top_positives",
            FactValue::Tags(vec!["green surroundings".to_string()]),
            SourceType::Google,
            0.8,
            None,
            vec![],
        )],
    );

    let results = world.run("3bhk with greenery in whitefield above 10 acres");
    let ids = result_ids(&results);

    assert_eq!(results[0].card.id, "large-green");
    assert_eq!(
        ids,
        vec!["large-green", "large-plain"],
        "below-threshold and unknown-RERA acreage projects must be excluded"
    );
    assert_reason(
        &results[0],
        "above 10 acres",
        "rera_total_land_area_sqm",
        "rera-proof",
        Some("Rera"),
    );
    assert_reason(
        &results[0],
        "greenery",
        "google_top_positives",
        "graph-text",
        Some("Google"),
    );
    assert_coverage(&results[1], "greenery", "no_data");
    assert!(
        !results[1].match_reason.contains("greenery"),
        "no-data support evidence must not be advertised as a greenery match: {}",
        results[1].match_reason
    );
}

#[tokio::test]
async fn fanned_in_serving_support_facts_rank_and_explain_search() {
    let world = large_acreage_world();
    let fact_index =
        serving_index_with_resident_fact(&world.graph, r#"["greenery","trees","open space"]"#)
            .await;

    let results = world.run_with_serving_facts(
        "3bhk with greenery in whitefield above 10 acres",
        &fact_index,
    );

    assert_eq!(result_ids(&results), vec!["large-green", "large-plain"]);
    assert_reason(
        &results[0],
        "above 10 acres",
        "rera_total_land_area_sqm",
        "rera-proof",
        Some("Rera"),
    );
    assert_reason(
        &results[0],
        "greenery",
        "resident_greenery_signal",
        "serving-fact",
        Some("Reddit"),
    );
    assert_coverage(&results[1], "greenery", "no_data");
}

#[tokio::test]
async fn serving_rera_facts_can_prove_hard_constraints_without_legacy_graph_facts() {
    let mut world = large_acreage_world();
    for node in world.graph.nodes.values_mut() {
        node.facts
            .retain(|fact| fact.key != "rera_total_land_area_sqm");
    }
    let fact_index = serving_index_with_rera_land_facts(&world.graph).await;

    let results = world.run_with_serving_facts("3bhk in whitefield above 10 acres", &fact_index);

    assert_eq!(result_ids(&results), vec!["large-green", "large-plain"]);
    for result in &results {
        assert_reason(
            result,
            "above 10 acres",
            "rera_total_land_area_sqm",
            "rera-proof",
            Some("Rera"),
        );
    }
}

#[tokio::test]
async fn fanned_in_serving_support_facts_need_search_annotation_to_rank() {
    let world = large_acreage_world();
    let fact_index = serving_index_with_resident_fact(&world.graph, "[]").await;

    let results = world.run_with_serving_facts(
        "3bhk with greenery in whitefield above 10 acres",
        &fact_index,
    );
    let large_green = results
        .iter()
        .find(|result| result.card.id == "large-green")
        .expect("large-green should survive RERA acreage constraint");

    assert_coverage(large_green, "greenery", "no_data");
    assert!(
        !has_reason(large_green, "greenery", "resident_greenery_signal"),
        "serving facts must not answer new search dimensions without skill-declared answers_preferences"
    );
}

#[tokio::test]
async fn fanned_in_serving_support_facts_need_text_match_scoring_direction() {
    let world = large_acreage_world();
    let fact_index = serving_index_with_resident_fact_metadata(
        &world.graph,
        r#"["greenery","trees","open space"]"#,
        Some("HigherIsBetter"),
    )
    .await;

    let results = world.run_with_serving_facts(
        "3bhk with greenery in whitefield above 10 acres",
        &fact_index,
    );
    let large_green = results
        .iter()
        .find(|result| result.card.id == "large-green")
        .expect("large-green should survive RERA acreage constraint");

    assert_coverage(large_green, "greenery", "no_data");
    assert!(
        !has_reason(large_green, "greenery", "resident_greenery_signal"),
        "serving overlay must not score unsupported numeric directions as text evidence"
    );
}

#[test]
fn negative_risk_preference_changes_rank_and_explanation_without_kg_data() {
    let mut low_risk = property(
        "low-waterlogging",
        "Whitefield",
        "low-waterlogging",
        3,
        18_000_000,
    );
    low_risk.waterlogging_risk_score = 0.1;
    let mut high_risk = property(
        "high-waterlogging",
        "Whitefield",
        "high-waterlogging",
        3,
        17_500_000,
    );
    high_risk.waterlogging_risk_score = 0.9;

    let world = SearchWorld::new(vec![high_risk, low_risk]);

    let results = world.run("3bhk whitefield avoid waterlogging");
    let ids = result_ids(&results);

    assert_eq!(ids, vec!["low-waterlogging", "high-waterlogging"]);
    assert_reason(
        &results[0],
        "avoid waterlogging risk",
        "waterlogging_risk_score",
        "local-risk",
        Some("Seed"),
    );
    assert!(
        results[0].match_score > results[1].match_score,
        "low-risk result should outrank high-risk result"
    );
}

#[test]
fn builder_trust_can_be_proven_through_related_builder_node() {
    let mut proven = property(
        "proven-builder-home",
        "Whitefield",
        "proven-builder",
        3,
        19_000_000,
    );
    proven.builder_quality_score = 0.2;
    let mut unproven = property(
        "unproven-builder-home",
        "Whitefield",
        "unproven-builder",
        3,
        18_500_000,
    );
    unproven.builder_quality_score = 0.2;

    let mut world = SearchWorld::new(vec![unproven, proven]);
    world.add_society("proven-builder", RootSource::Rera, Vec::new());
    world.add_society("unproven-builder", RootSource::Rera, Vec::new());
    world.add_builder(
        "builder:proven-buildco",
        "Proven BuildCo",
        vec![sourced_fact(
            "delivery_track_record",
            FactValue::Text("Delivered prior RERA projects on time".to_string()),
            SourceType::Rera,
            1.0,
            Some(ScoringHint {
                direction: ScoringDirection::TextMatch,
                weight: 2.0,
                thresholds: Vec::new(),
            }),
            vec!["trusted builder", "reliable builder"],
        )],
    );
    world.add_edge(
        "society:proven-builder",
        "builder:proven-buildco",
        Relation::BuiltBy,
    );

    let results = world.run("trusted builder 3bhk whitefield");
    let ids = result_ids(&results);

    assert_eq!(ids, vec!["proven-builder-home", "unproven-builder-home"]);
    assert_reason(
        &results[0],
        "trusted builder",
        "delivery_track_record",
        "graph",
        Some("Rera"),
    );
    assert_coverage(&results[1], "trusted builder", "no_data");
}

#[test]
fn canonical_calculator_facts_do_not_affect_search_without_annotation() {
    let mut world = SearchWorld::new(vec![
        property(
            "computed-only-home",
            "Whitefield",
            "computed-only",
            3,
            19_000_000,
        ),
        property("plain-home", "Whitefield", "plain", 3, 18_500_000),
    ]);
    world.add_society(
        "computed-only",
        RootSource::Rera,
        vec![sourced_fact(
            "buy_vs_rent_irr",
            FactValue::Numeric(0.12),
            SourceType::Computed,
            0.7,
            None,
            vec![],
        )],
    );
    world.add_society("plain", RootSource::Rera, Vec::new());

    let results = world.run("premium 3bhk whitefield");

    assert!(!results.is_empty());
    for result in &results {
        let Some(explanation) = result.match_explanation.as_ref() else {
            continue;
        };
        assert!(
            explanation
                .reasons
                .iter()
                .all(|reason| reason.fact_key != "buy_vs_rent_irr"),
            "calculator facts need explicit search annotation before ranking/search explanations: {:?}",
            explanation.reasons
        );
    }
}

struct SearchWorld {
    properties: Vec<Property>,
    societies: Vec<Society>,
    society_names: HashMap<String, String>,
    graph: KnowledgeGraph,
    index: SearchIndex,
}

impl SearchWorld {
    fn new(properties: Vec<Property>) -> Self {
        let societies = properties
            .iter()
            .map(|property| society(&property.society_id, &property.area, &property.builder_name))
            .collect::<Vec<_>>();
        let society_names = societies
            .iter()
            .map(|society| (society.id.clone(), society.name.clone()))
            .collect::<HashMap<_, _>>();
        let index = SearchIndex::build(&properties);

        Self {
            properties,
            societies,
            society_names,
            graph: KnowledgeGraph::new(),
            index,
        }
    }

    fn run(&self, query: &str) -> Vec<SearchResultCard> {
        let intent = parse_intent(query);
        TextSearch::search_with_index_and_intent_and_sellers(
            &self.properties,
            Some(&self.index),
            &self.society_names,
            &self.societies,
            query,
            &intent,
            Some(&self.graph),
            &[],
        )
    }

    fn run_with_serving_facts(
        &self,
        query: &str,
        serving_facts: &ServingFactIndex,
    ) -> Vec<SearchResultCard> {
        let intent = parse_intent(query);
        TextSearch::search_with_index_extra_recall_serving_facts_and_intent_and_sellers(
            &self.properties,
            Some(&self.index),
            None,
            Some(serving_facts),
            &self.society_names,
            &self.societies,
            query,
            &intent,
            Some(&self.graph),
            &[],
        )
    }

    fn add_society(&mut self, slug: &str, root_source: RootSource, facts: Vec<SourcedFact>) {
        let mut node = Node::new(format!("society:{slug}"), NodeType::Society, slug);
        node.root_source = Some(root_source);
        node.add_facts(facts);
        self.graph.add_node(node);
    }

    fn add_builder(&mut self, node_id: &str, name: &str, facts: Vec<SourcedFact>) {
        let mut node = Node::new(node_id, NodeType::Builder, name);
        node.add_facts(facts);
        self.graph.add_node(node);
    }

    fn add_edge(&mut self, from: &str, to: &str, relation: Relation) {
        self.graph.add_edge(Edge {
            from: from.to_string(),
            to: to.to_string(),
            relation,
            weight: 1.0,
            metadata: HashMap::new(),
            source: source(SourceType::Manual),
        });
    }
}

fn assert_reason(
    result: &SearchResultCard,
    preference: &str,
    fact_key: &str,
    scoring_method: &str,
    source_type: Option<&str>,
) {
    let explanation = result
        .match_explanation
        .as_ref()
        .expect("quality case should include match explanation");
    assert!(
        explanation.reasons.iter().any(|reason| {
            reason.preference == preference
                && reason.fact_key == fact_key
                && reason.scoring_method == scoring_method
                && source_type.is_none_or(|expected| reason.source_type == expected)
        }),
        "expected reason ({preference}, {fact_key}, {scoring_method}, {source_type:?}), got {:?}",
        explanation.reasons
    );
}

fn assert_coverage(result: &SearchResultCard, preference: &str, status: &str) {
    let explanation = result
        .match_explanation
        .as_ref()
        .expect("quality case should include match explanation");
    assert!(
        explanation
            .preference_coverage
            .iter()
            .any(|coverage| coverage.preference == preference && coverage.status == status),
        "expected coverage ({preference}, {status}), got {:?}",
        explanation.preference_coverage
    );
}

fn result_ids(results: &[SearchResultCard]) -> Vec<&str> {
    results
        .iter()
        .map(|result| result.card.id.as_str())
        .collect()
}

fn has_reason(result: &SearchResultCard, preference: &str, fact_key: &str) -> bool {
    result
        .match_explanation
        .as_ref()
        .is_some_and(|explanation| {
            explanation
                .reasons
                .iter()
                .any(|reason| reason.preference == preference && reason.fact_key == fact_key)
        })
}

fn large_acreage_world() -> SearchWorld {
    let mut world = SearchWorld::new(vec![
        property("large-green", "Whitefield", "large-green", 3, 19_000_000),
        property("large-plain", "Whitefield", "large-plain", 3, 18_500_000),
    ]);
    for property in &mut world.properties {
        property.greenery_score = None;
        property.open_space_score = None;
    }
    world.add_society(
        "large-green",
        RootSource::Rera,
        vec![rera_numeric_fact(
            "rera_total_land_area_sqm",
            12.0 * SQM_PER_ACRE,
        )],
    );
    world.add_society(
        "large-plain",
        RootSource::Rera,
        vec![rera_numeric_fact(
            "rera_total_land_area_sqm",
            11.0 * SQM_PER_ACRE,
        )],
    );
    world
}

async fn serving_index_with_resident_fact(
    graph: &KnowledgeGraph,
    answers_preferences_json: &str,
) -> ServingFactIndex {
    serving_index_with_resident_fact_metadata(graph, answers_preferences_json, Some("TextMatch"))
        .await
}

async fn serving_index_with_resident_fact_metadata(
    graph: &KnowledgeGraph,
    answers_preferences_json: &str,
    scoring_direction: Option<&str>,
) -> ServingFactIndex {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();
    let support_materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "reddit_resident_facts",
            "reddit",
            "2026-07-13",
            "run-reddit-support-2026-07-13",
            &[SkillFactRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                value_type: "text".to_string(),
                value_json: serde_json::to_string(&FactValue::Text(
                    "Residents mention many trees and open internal space".to_string(),
                ))
                .unwrap(),
                confidence: 0.72,
                source_type: "Reddit".to_string(),
                source_url: Some(
                    "https://reddit.com/r/BangaloreRealEstates/comments/green".to_string(),
                ),
                model: None,
                skill_id: Some("reddit_resident_fact_extractor".to_string()),
                triggered_by: Some("3bhk whitefield greenery".to_string()),
                learned_at: Utc::now(),
                run_id: "run-reddit-support-2026-07-13".to_string(),
                input_hash: "sha256:reddit-green".to_string(),
            }],
            &[SkillFactAnnotationRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                display_template: Some("Resident signal: {value}".to_string()),
                answers_preferences_json: answers_preferences_json.to_string(),
                scoring_direction: scoring_direction.map(str::to_string),
                scoring_weight: Some(1.4),
                scoring_thresholds_json: "[]".to_string(),
            }],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    let support_rows =
        read_skill_fact_artifact_rows(&lake, std::slice::from_ref(&support_materialization.record))
            .await
            .unwrap();
    let kg_materialization = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote_with_skill_facts(
            graph,
            "2026-07-13T06:00Z",
            Vec::new(),
            vec![support_materialization.record.materialization_id.clone()],
            &support_rows.facts,
            &support_rows.fact_annotations,
        )
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_materialization, "2026-07-13T06:00Z")
        .await
        .unwrap();
    ServingBundleLoader::new(lake, cache_root.path())
        .load_current_search_bundle()
        .await
        .unwrap()
        .expect("serving bundle should load")
        .fact_index
}

async fn serving_index_with_rera_land_facts(graph: &KnowledgeGraph) -> ServingFactIndex {
    let lake_root = tempdir().unwrap();
    let cache_root = tempdir().unwrap();
    let lake = LakeStore::local(lake_root.path()).unwrap();
    let now = Utc::now();
    let facts = [
        ("society:large-green", 12.0 * SQM_PER_ACRE),
        ("society:large-plain", 11.0 * SQM_PER_ACRE),
    ]
    .into_iter()
    .map(|(entity_id, value)| SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: "rera_total_land_area_sqm".to_string(),
        value_type: "numeric".to_string(),
        value_json: serde_json::to_string(&FactValue::Numeric(value)).unwrap(),
        confidence: 1.0,
        source_type: "Rera".to_string(),
        source_url: Some("https://rera.karnataka.gov.in/projectViewDetails".to_string()),
        model: None,
        skill_id: Some("fetch_rera".to_string()),
        triggered_by: None,
        learned_at: now,
        run_id: "run-rera-proof-2026-07-13".to_string(),
        input_hash: format!("sha256:{entity_id}"),
    })
    .collect::<Vec<_>>();
    let support_materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "rera_legal_facts",
            "rera",
            "2026-07-13",
            "run-rera-proof-2026-07-13",
            &facts,
            &[],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    let support_rows =
        read_skill_fact_artifact_rows(&lake, std::slice::from_ref(&support_materialization.record))
            .await
            .unwrap();
    let kg_materialization = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote_with_skill_facts(
            graph,
            "2026-07-13T06:00Z",
            Vec::new(),
            vec![support_materialization.record.materialization_id.clone()],
            &support_rows.facts,
            &support_rows.fact_annotations,
        )
        .await
        .unwrap();
    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_and_promote_from_kg_view(&kg_materialization, "2026-07-13T06:00Z")
        .await
        .unwrap();
    ServingBundleLoader::new(lake, cache_root.path())
        .load_current_search_bundle()
        .await
        .unwrap()
        .expect("serving bundle should load")
        .fact_index
}

fn property(id: &str, area: &str, society_id: &str, bhk: u32, price: u64) -> Property {
    Property {
        id: id.to_string(),
        title: format!("{bhk} BHK test home"),
        area: area.to_string(),
        area_id: area.to_lowercase().replace(' ', "-"),
        city: "Bengaluru".to_string(),
        society_id: society_id.to_string(),
        builder_name: "Test Builder".to_string(),
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
        society_quality_score: 0.7,
        builder_quality_score: 0.7,
        document_completeness_score: 0.8,
        litigation_risk: 0.1,
        noise_score: 0.2,
        sunlight_score: 0.7,
        airport_noise_score: 0.1,
        waterlogging_risk_score: 0.2,
        traffic_score: 0.4,
        days_on_market: 20,
        greenery_score: Some(0.6),
        open_space_score: Some(0.6),
        resale_strength_score: Some(0.7),
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: "Local quality harness listing".to_string(),
        transparency_tags: Vec::new(),
        source_reference: "search-quality-contract".to_string(),
        seller_id: None,
    }
}

fn society(id: &str, area: &str, builder_name: &str) -> Society {
    Society {
        id: id.to_string(),
        name: format!("{id} Society"),
        area: area.to_string(),
        city: "Bengaluru".to_string(),
        builder_name: builder_name.to_string(),
        year_built: 2020,
        total_units: 400,
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
    }
}

fn rera_numeric_fact(key: &str, value: f64) -> SourcedFact {
    sourced_fact(
        key,
        FactValue::Numeric(value),
        SourceType::Rera,
        1.0,
        None,
        vec![],
    )
}

fn sourced_fact(
    key: &str,
    value: FactValue,
    source_type: SourceType,
    confidence: f32,
    scoring_hint: Option<ScoringHint>,
    answers_preferences: Vec<&str>,
) -> SourcedFact {
    SourcedFact {
        key: key.to_string(),
        value,
        confidence,
        source: source(source_type),
        learned_at: Utc::now(),
        version: 1,
        display_template: Some(format!("{key}: {{value}}")),
        answers_preferences: answers_preferences
            .into_iter()
            .map(str::to_string)
            .collect(),
        scoring_hint,
    }
}

fn source(source_type: SourceType) -> FactSource {
    FactSource {
        source_type,
        url: None,
        model: None,
        skill_id: None,
        triggered_by: None,
    }
}
