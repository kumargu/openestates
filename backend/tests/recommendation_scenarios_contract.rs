use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use backend::graph::GraphIndex;
use backend::knowledge::{FactValue, KnowledgeGraph};
use backend::models::{KgEntityRefs, Property, Society};
use backend::recommendations::{
    build_recommendation_branches, RecommendationBranch, RecommendationBranchInputs,
};
use backend::routes::properties::PropertyEvidenceResponse;
use backend::search::{geo::GeoSearchIndex, SearchCapabilityIndex};
use backend::serving::{
    LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest, ServingEdgeRecord,
    ServingEntityAliasIndex, ServingEntityRecord, ServingFactIndex, ServingFactRecord,
    SpatialServingIndex, TantivyRecallIndex,
};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use tempfile::tempdir;

const SCENARIO_BANK: &str = include_str!("../../data/validation/recommendation_scenario_bank.json");
const EXPECTED_SCENARIO_COUNT: usize = 10;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioBank {
    version: u32,
    generated_at: String,
    description: String,
    notes: Vec<String>,
    cases: Vec<ScenarioCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCase {
    id: String,
    category: String,
    admission: Admission,
    purpose: String,
    area_median_ppsf: u64,
    anchor: PropertySpec,
    candidates: Vec<PropertySpec>,
    #[serde(default)]
    facts: Vec<FactSpec>,
    #[serde(default)]
    edges: Vec<EdgeSpec>,
    expectation: Expectation,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Admission {
    CurrentContract,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertySpec {
    id: String,
    society: String,
    area: String,
    bhk: u32,
    price: u64,
    ppsf: u64,
    builder: String,
    #[serde(default = "default_property_type")]
    property_type: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FactSpec {
    property_id: String,
    key: String,
    value: serde_json::Value,
    source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeSpec {
    from_property_id: String,
    edge_type: String,
    to_entity_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expectation {
    minimum_items: Option<usize>,
    exact_item_count: Option<usize>,
    unique_property_ids: Option<bool>,
    exclude_anchor: Option<bool>,
    #[serde(default)]
    required_properties: Vec<String>,
    #[serde(default)]
    forbidden_properties: Vec<String>,
    #[serde(default)]
    exact_branch_properties: BTreeMap<String, String>,
    #[serde(default)]
    branch_must_not_use: BTreeMap<String, String>,
    #[serde(default)]
    required_channels: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    tradeoff_contains: BTreeMap<String, String>,
    max_properties_per_society: Option<usize>,
    repeat_with_reversed_inventory: Option<bool>,
}

#[test]
fn recommendation_scenario_bank_is_well_formed() {
    let bank = parse_bank();
    assert_eq!(bank.version, 1);
    assert!(!bank.generated_at.trim().is_empty());
    assert!(!bank.description.trim().is_empty());
    assert!(bank.notes.len() >= 3);
    assert_eq!(bank.cases.len(), EXPECTED_SCENARIO_COUNT);

    let ids = bank
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<HashSet<_>>();
    let categories = bank
        .cases
        .iter()
        .map(|case| case.category.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), EXPECTED_SCENARIO_COUNT, "duplicate scenario IDs");
    assert_eq!(
        categories.len(),
        EXPECTED_SCENARIO_COUNT,
        "each scenario must cover a distinct recommendation risk"
    );

    assert!(bank
        .cases
        .iter()
        .all(|case| case.admission == Admission::CurrentContract));

    for case in &bank.cases {
        assert!(
            !case.purpose.trim().is_empty(),
            "{} has no purpose",
            case.id
        );
        assert!(case.area_median_ppsf > 0, "{} has no area median", case.id);
        let mut property_ids = case
            .candidates
            .iter()
            .map(|property| property.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(
            property_ids.len(),
            case.candidates.len(),
            "{} repeats a candidate ID",
            case.id
        );
        assert!(
            property_ids.insert(&case.anchor.id),
            "{} repeats its anchor as a candidate",
            case.id
        );
        assert!(
            case.facts
                .iter()
                .all(|fact| property_ids.contains(fact.property_id.as_str())),
            "{} has a fact for an unknown property",
            case.id
        );
        assert!(
            case.edges
                .iter()
                .all(|edge| property_ids.contains(edge.from_property_id.as_str())),
            "{} has an edge for an unknown property",
            case.id
        );
    }
}

#[test]
fn current_recommendation_scenarios_execute_against_controlled_inventory() {
    let bank = parse_bank();
    let failures = evaluate_cases(&bank.cases);
    assert!(
        failures.is_empty(),
        "current recommendation contract misses:\n{}",
        failures.join("\n")
    );
}

fn parse_bank() -> ScenarioBank {
    serde_json::from_str(SCENARIO_BANK).expect("recommendation scenario bank follows its schema")
}

fn evaluate_cases<'a>(cases: impl IntoIterator<Item = &'a ScenarioCase>) -> Vec<String> {
    let mut failures = Vec::new();
    for case in cases {
        let observed = execute(case, false);
        let mut case_failures = expectation_failures(case, &observed);
        if case.expectation.repeat_with_reversed_inventory == Some(true) {
            let reversed = execute(case, true);
            let left = branch_signature(&observed);
            let right = branch_signature(&reversed);
            if left != right {
                case_failures.push(format!(
                    "inventory reversal changed output: normal={left:?}, reversed={right:?}"
                ));
            }
        }
        if !case_failures.is_empty() {
            failures.push(format!(
                "{} [contract_gap]: {}",
                case.id,
                case_failures.join("; ")
            ));
        }
    }
    failures
}

fn execute(case: &ScenarioCase, reverse_candidates: bool) -> Vec<RecommendationBranch> {
    let mut specs = Vec::with_capacity(case.candidates.len() + 1);
    specs.push(case.anchor.clone());
    let mut candidates = case.candidates.clone();
    if reverse_candidates {
        candidates.reverse();
    }
    specs.extend(candidates);

    let properties = specs.iter().map(property).collect::<Vec<_>>();
    let societies = specs.iter().map(society).collect::<Vec<_>>();
    let bundle = build_bundle(case, &specs);
    let graph = KnowledgeGraph::new();
    let anchor = properties
        .iter()
        .find(|property| property.id == case.anchor.id)
        .expect("anchor property exists");
    let evidence = empty_evidence(anchor);

    build_recommendation_branches(RecommendationBranchInputs {
        current: anchor,
        current_evidence: &evidence,
        graph: &graph,
        properties: &properties,
        societies: &societies,
        serving_bundle: Some(&bundle),
        area_median_ppsf: Some(case.area_median_ppsf),
    })
}

fn expectation_failures(case: &ScenarioCase, branches: &[RecommendationBranch]) -> Vec<String> {
    let expected = &case.expectation;
    let mut failures = Vec::new();
    let ids = branches
        .iter()
        .map(|branch| branch.property.id.as_str())
        .collect::<Vec<_>>();

    if let Some(minimum) = expected.minimum_items {
        if branches.len() < minimum {
            failures.push(format!(
                "expected at least {minimum} items, got {}",
                branches.len()
            ));
        }
    }
    if let Some(count) = expected.exact_item_count {
        if branches.len() != count {
            failures.push(format!("expected {count} items, got {}", branches.len()));
        }
    }
    if expected.unique_property_ids == Some(true) {
        let unique = ids.iter().copied().collect::<HashSet<_>>();
        if unique.len() != ids.len() {
            failures.push(format!("duplicate property IDs: {ids:?}"));
        }
    }
    if expected.exclude_anchor == Some(true) && ids.contains(&case.anchor.id.as_str()) {
        failures.push("anchor was recommended".to_string());
    }
    for id in &expected.required_properties {
        if !ids.contains(&id.as_str()) {
            failures.push(format!("missing required property {id}; actual={ids:?}"));
        }
    }
    for id in &expected.forbidden_properties {
        if ids.contains(&id.as_str()) {
            failures.push(format!("returned forbidden property {id}"));
        }
    }
    for (branch_id, property_id) in &expected.exact_branch_properties {
        match branches
            .iter()
            .find(|branch| &branch.branch_id == branch_id)
        {
            Some(branch) if branch.property.id == *property_id => {}
            Some(branch) => failures.push(format!(
                "branch {branch_id} expected {property_id}, got {}",
                branch.property.id
            )),
            None => failures.push(format!("missing branch {branch_id}")),
        }
    }
    for (branch_id, forbidden_id) in &expected.branch_must_not_use {
        if branches
            .iter()
            .any(|branch| &branch.branch_id == branch_id && &branch.property.id == forbidden_id)
        {
            failures.push(format!(
                "branch {branch_id} used evidence-missing {forbidden_id}"
            ));
        }
    }
    for (property_id, channels) in &expected.required_channels {
        let Some(branch) = branches
            .iter()
            .find(|branch| &branch.property.id == property_id)
        else {
            continue;
        };
        for channel in channels {
            if !branch.channels.iter().any(|hit| &hit.channel == channel) {
                failures.push(format!(
                    "property {property_id} lacks channel {channel}; actual={:?}",
                    branch
                        .channels
                        .iter()
                        .map(|hit| hit.channel.as_str())
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    for (branch_id, needle) in &expected.tradeoff_contains {
        let actual = branches
            .iter()
            .find(|branch| &branch.branch_id == branch_id)
            .and_then(|branch| branch.tradeoff.as_deref());
        if !actual.is_some_and(|text| text.contains(needle)) {
            failures.push(format!(
                "branch {branch_id} tradeoff does not contain {needle:?}; actual={actual:?}"
            ));
        }
    }
    if let Some(maximum) = expected.max_properties_per_society {
        let mut counts = HashMap::<&str, usize>::new();
        for branch in branches {
            *counts
                .entry(branch.property.society_name.as_str())
                .or_default() += 1;
        }
        let crowded = counts
            .into_iter()
            .filter(|(_, count)| *count > maximum)
            .collect::<Vec<_>>();
        if !crowded.is_empty() {
            failures.push(format!("society diversity exceeded {maximum}: {crowded:?}"));
        }
    }
    failures
}

fn branch_signature(branches: &[RecommendationBranch]) -> Vec<(&str, &str)> {
    branches
        .iter()
        .map(|branch| (branch.branch_id.as_str(), branch.property.id.as_str()))
        .collect()
}

fn property(spec: &PropertySpec) -> Property {
    let listable = spec.price > 0 || spec.bhk > 0;
    Property {
        id: spec.id.clone(),
        title: spec.society.clone(),
        area: spec.area.clone(),
        area_id: slug(&spec.area),
        city: "Bengaluru".to_string(),
        society_id: slug(&spec.society),
        builder_name: spec.builder.clone(),
        property_type: spec.property_type.clone(),
        listing_type: "Resale".to_string(),
        bhk: spec.bhk,
        price: spec.price,
        price_min: None,
        price_max: None,
        price_per_sqft: spec.ppsf,
        carpet_area_sqft: if listable { 1_350 } else { 0 },
        super_builtup_sqft: if listable { 1_650 } else { 0 },
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: "Ready to Move".to_string(),
        metro_distance_mins: 12,
        maintenance_cost_monthly: 6_000,
        society_quality_score: None,
        builder_quality_score: None,
        document_completeness_score: None,
        litigation_risk: None,
        noise_score: None,
        sunlight_score: None,
        airport_noise_score: None,
        waterlogging_risk_score: None,
        traffic_score: None,
        days_on_market: 20,
        greenery_score: None,
        open_space_score: None,
        resale_strength_score: None,
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: "Controlled recommendation scenario".to_string(),
        transparency_tags: Vec::new(),
        source_reference: "recommendation-scenarios-contract".to_string(),
    }
}

fn default_property_type() -> String {
    "Apartment".to_string()
}

fn society(spec: &PropertySpec) -> Society {
    Society {
        id: slug(&spec.society),
        name: spec.society.clone(),
        area: spec.area.clone(),
        city: "Bengaluru".to_string(),
        builder_name: spec.builder.clone(),
        year_built: 2021,
        total_units: 240,
        summary: String::new(),
        maintenance_sentiment: String::new(),
        livability_sentiment: String::new(),
        common_positives: Vec::new(),
        common_complaints: Vec::new(),
        review_summary: String::new(),
        google_reviews_url: None,
        future_google_place_name: spec.society.clone(),
        future_google_place_id: None,
        future_review_enrichment_status: "not_requested".to_string(),
    }
}

fn empty_evidence(property: &Property) -> PropertyEvidenceResponse {
    PropertyEvidenceResponse {
        property_id: property.id.clone(),
        entity_refs: KgEntityRefs {
            property_entity_id: format!("property:{}", property.id),
            society_entity_id: society_entity_id(property),
            area_entity_id: format!("area:{}", property.area_id),
            builder_entity_id: None,
            source_entity_ids: Vec::new(),
        },
        serving_bundle_version: Some("recommendation-scenarios-v1".to_string()),
        sections: Vec::new(),
    }
}

fn build_bundle(case: &ScenarioCase, specs: &[PropertySpec]) -> LoadedServingBundle {
    let mut entities = specs
        .iter()
        .map(|spec| ServingEntityRecord {
            entity_id: format!("society:{}", slug(&spec.society)),
            entity_type: "society".to_string(),
            name: spec.society.clone(),
            root_source: Some("mock_contract".to_string()),
            searchable_text: spec.society.clone(),
        })
        .collect::<Vec<_>>();
    let property_by_id = specs
        .iter()
        .map(|spec| (spec.id.as_str(), spec))
        .collect::<HashMap<_, _>>();
    let mut facts = case
        .facts
        .iter()
        .map(|fact| {
            let spec = property_by_id[fact.property_id.as_str()];
            serving_fact(&society_entity_id_from_spec(spec), fact)
        })
        .collect::<Vec<_>>();
    for spec in specs {
        if let (Some(latitude), Some(longitude)) = (spec.latitude, spec.longitude) {
            facts.push(numeric_fact(
                &society_entity_id_from_spec(spec),
                "geo.latitude",
                latitude,
                "Google",
            ));
            facts.push(numeric_fact(
                &society_entity_id_from_spec(spec),
                "geo.longitude",
                longitude,
                "Google",
            ));
        }
    }
    let edges = case
        .edges
        .iter()
        .map(|edge| ServingEdgeRecord {
            from_entity_id: society_entity_id_from_spec(
                property_by_id[edge.from_property_id.as_str()],
            ),
            edge_type: edge.edge_type.clone(),
            to_entity_id: edge.to_entity_id.clone(),
            confidence: 1.0,
            source_type: "MockGraph".to_string(),
        })
        .collect::<Vec<_>>();
    for target in edges
        .iter()
        .map(|edge| edge.to_entity_id.as_str())
        .collect::<BTreeSet<_>>()
    {
        entities.push(ServingEntityRecord {
            entity_id: target.to_string(),
            entity_type: "place".to_string(),
            name: target.trim_start_matches("place:").replace('-', " "),
            root_source: Some("mock_contract".to_string()),
            searchable_text: String::new(),
        });
    }

    let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    let temp_dir = tempdir().expect("temporary recommendation Tantivy directory");
    let recall_index = TantivyRecallIndex::build_in_dir(temp_dir.path(), &entities, &facts, &[])
        .expect("controlled recommendation recall index");
    let geo_index = GeoSearchIndex::from_serving_bundle(&entities, &fact_index);
    let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
    for spec in specs
        .iter()
        .filter(|spec| spec.latitude.is_some() && spec.longitude.is_some())
    {
        let entity_id = society_entity_id_from_spec(spec);
        assert!(
            spatial_index.point_for_entity(&entity_id).is_some(),
            "controlled coordinates for {} must satisfy serving resolution policy",
            spec.id
        );
    }
    let search_capabilities = SearchCapabilityIndex::from_bundle(&entities, &fact_index);
    let graph_index = GraphIndex::from_serving_edges(&edges);

    LoadedServingBundle {
        manifest: ServingBundleManifest {
            bundle_version: "recommendation-scenarios-v1".to_string(),
            format_version: 1,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            entity_count: entities.len() as u64,
            entity_alias_count: 0,
            fact_count: facts.len() as u64,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: edges.len() as u64,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: BTreeMap::new(),
            entity_parquet_key: "entities.parquet".to_string(),
            entity_alias_parquet_key: None,
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: None,
            edge_parquet_key: Some("edges.parquet".to_string()),
            quarantine_report_key: None,
            schema_key: "schema.json".to_string(),
            trust_policy_key: "trust.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entities,
        entity_alias_index: ServingEntityAliasIndex::default(),
        edges,
        graph_index,
        recall_index,
        fact_index,
        rera_evidence_index: ReraEvidenceIndex::default(),
        geo_index,
        spatial_index,
        search_capabilities,
        cache_dir: temp_dir.keep(),
    }
}

fn serving_fact(entity_id: &str, fact: &FactSpec) -> ServingFactRecord {
    let value = match &fact.value {
        serde_json::Value::Bool(value) => FactValue::Bool(*value),
        serde_json::Value::Number(value) => {
            FactValue::Numeric(value.as_f64().expect("numeric fixture fact"))
        }
        serde_json::Value::String(value) => FactValue::Text(value.clone()),
        other => panic!("unsupported fixture fact value: {other}"),
    };
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact.key.clone(),
        value_type: match value {
            FactValue::Bool(_) => "bool",
            FactValue::Numeric(_) | FactValue::Score { .. } => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Tags(_) => "tags",
        }
        .to_string(),
        value_text: None,
        value,
        confidence: 1.0,
        source_type: fact.source.clone(),
        source_url: None,
        model: None,
        skill_id: Some("recommendation_scenarios_contract".to_string()),
        learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
    }
}

fn numeric_fact(entity_id: &str, key: &str, value: f64, source: &str) -> ServingFactRecord {
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: key.to_string(),
        value_type: "numeric".to_string(),
        value_text: None,
        value: FactValue::Numeric(value),
        confidence: 1.0,
        source_type: source.to_string(),
        source_url: None,
        model: None,
        skill_id: Some("recommendation_scenarios_contract".to_string()),
        learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
    }
}

fn society_entity_id(property: &Property) -> String {
    format!("society:{}", property.society_id)
}

fn society_entity_id_from_spec(spec: &PropertySpec) -> String {
    format!("society:{}", slug(&spec.society))
}

fn slug(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
