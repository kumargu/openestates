use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{KgEntityRefs, PropertyCard, SellerSummary};
use crate::recommendations::{
    build_recommendation_branches, RecommendationBranch, RecommendationBranchInputs,
};
use crate::scoring::{self, compute_transparency_score, MarketActivityResponse, TransparencyScore};
use crate::search::text::compute_confidence_for_detail;
use crate::search::ConfidenceScore;
use crate::serving::{
    GoogleReviewEvidence, LoadedServingBundle, ServingFactIndex, SocietyFactProjection,
};
use crate::state::AppState;

use crate::community::{
    community_evidence_from_fact_value, community_pulse_from_summary,
    deterministic_community_summarizer, CommunityPulse,
};
use crate::knowledge::node::NodeType;
use crate::knowledge::{google_reviews_url_from_facts, FactValue, SourcedFact};
use crate::livability_brief::{
    compose_livability_brief, filter_reddit_evidence, LivabilityBrief, LivabilityBriefInput,
    LivabilityLens, StructuredFactSignal,
};

use super::enrichment::{
    enrich_area, enrich_property_card_with_sellers, enrich_society, extract_area_intelligence,
    extract_builder_trust, extract_data_freshness, extract_rera_info, kg_entity_refs_for_property,
    society_node_id, AreaIntelligence, BuilderTrust, DataFreshness, ReraInfo,
};

/// GET /api/properties — returns UI-ready property cards.
pub async fn list_properties(State(state): State<Arc<AppState>>) -> Json<Vec<PropertyCard>> {
    let graph = state.knowledge.read().await;
    let properties = state.properties.read().await;
    let sellers = state.sellers.read().await;
    let serving_bundle = state.serving_bundle.read().await.clone();
    let serving_facts = serving_bundle.as_ref().map(|bundle| &bundle.fact_index);

    let cards: Vec<PropertyCard> = properties
        .iter()
        .filter(|property| property.is_listable())
        .map(|p| {
            let card = enrich_property_card_with_sellers(p, &state.societies, &graph, &sellers);
            overlay_serving_google_reviews(card, &p.society_id, serving_facts)
        })
        .collect();

    Json(cards)
}

#[derive(Serialize)]
pub struct PropertyDetail {
    pub property: crate::models::Property,
    /// Stable graph IDs the UI can dereference to render dynamic KG-backed sections.
    pub entity_refs: KgEntityRefs,
    /// Canonical UI read model for dynamic proof-backed cards on the property page.
    ///
    /// New UI should render optional property-page sections from this field
    /// instead of hardcoding cards or calling legacy KG endpoints directly.
    /// `source_panels` below is retained as a compatibility field for the
    /// current frontend while it migrates.
    pub evidence: PropertyEvidenceResponse,
    pub society: Option<crate::models::Society>,
    pub area: Option<crate::models::AreaProfile>,
    pub market_activity: MarketActivityResponse,
    /// Similar properties from locally precomputed society embeddings.
    pub similar_properties: Vec<PropertyCard>,
    /// Counterfactual branches — why you might consider an alternative instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendation_branches: Vec<RecommendationBranch>,
    /// RERA regulatory data from the knowledge graph (None if not yet enriched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera: Option<ReraInfo>,
    /// Area intelligence from Reddit and other sources (None if not yet enriched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_intelligence: Option<AreaIntelligence>,
    /// Composite transparency score (0-100) with breakdown — internal only, not buyer-facing.
    #[serde(skip_serializing)]
    pub transparency_score: TransparencyScore,
    /// Lowest price_per_sqft among properties in the same area.
    pub area_price_range_low: Option<u64>,
    /// Highest price_per_sqft among properties in the same area.
    pub area_price_range_high: Option<u64>,
    /// Seller info for buyer-facing display (no email/phone).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seller: Option<SellerSummary>,
    /// Number of buyers who have expressed interest.
    pub interest_count: usize,
    /// Where the society data originally came from: "rera", "seller", "discovered", "legacy"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_source: Option<String>,
    /// Human-readable project status from skill's display_template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status_display: Option<String>,
    /// Compact buyer-facing state signal for first-scan UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_state_display: Option<String>,
    /// Machine-readable project status: "ready_to_move", "under_construction", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status: Option<String>,
    /// Builder delivery track record from knowledge graph
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_trust: Option<BuilderTrust>,
    /// Other locally tracked projects tied to the same builder/promoter name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_portfolio: Option<BuilderPortfolio>,
    /// Source-backed facts grouped for the detail page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_panels: Vec<SourcePanel>,
    /// Data freshness — how recently and richly the society data was updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_freshness: Option<DataFreshness>,
    /// Data confidence score — how trustworthy is this property's data? Internal only.
    #[serde(skip_serializing)]
    pub confidence_score: Option<ConfidenceScore>,
    /// Current external review evidence projected from the Parquet serving bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_reviews: Option<ExternalReviews>,
    /// Receipt-backed livability diligence brief composed from DAG facts and mined themes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livability_brief: Option<LivabilityBrief>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ExternalReviews {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_reviews_url: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BuilderPortfolio {
    pub builder_name: String,
    pub tracked_projects: usize,
    pub rera_registered_projects: usize,
    pub delayed_projects: usize,
    pub complaint_projects: usize,
    pub revocations: Option<i32>,
    pub projects: Vec<BuilderProjectRecord>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BuilderProjectRecord {
    pub property_id: String,
    pub project_name: String,
    pub area: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera_portal_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_months: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complaints_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status_display: Option<String>,
    pub current: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct SourcePanel {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<EvidenceMediaStrip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_pulse: Option<CommunityPulse>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SourceItem {
    pub entity_id: String,
    pub key: String,
    pub label: String,
    pub value: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<SourceAttribution>,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
    pub learned_at: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SourceAttribution {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub source_type: String,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
    pub learned_at: String,
}

// Property evidence panel layout — schema in app/config/product/evidence_sections.json.
const BUYER_CONTEXT_SECTIONS_JSON: &str =
    include_str!("../../../app/config/product/evidence_sections.json");
static BUYER_CONTEXT_DEFINITIONS: OnceLock<Vec<BuyerContextDefinition>> = OnceLock::new();

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EvidenceMediaStrip {
    pub kind: String,
    pub provider: String,
    pub title: String,
    pub caption: String,
    pub capture_date_label: String,
    pub coverage_quality: String,
    pub frames: Vec<EvidenceMediaFrame>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EvidenceMediaFrame {
    pub label: String,
    pub distance_from_gate_m: u32,
    pub image_url: String,
    pub heading: f64,
    pub pitch: f64,
    pub fov: f64,
    pub capture_date: String,
    pub source_url: String,
}

#[derive(Deserialize)]
struct ApproachRoadVisualRecord {
    provider: String,
    coverage_quality: String,
    frames: Vec<ApproachRoadVisualFrameRecord>,
}

#[derive(Deserialize)]
struct ApproachRoadVisualFrameRecord {
    label: String,
    distance_from_gate_m: u32,
    #[serde(default)]
    pano_id: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    location_query: Option<String>,
    #[serde(default)]
    radius_m: Option<u32>,
    heading: f64,
    pitch: f64,
    fov: f64,
    capture_date: String,
    #[serde(default)]
    image_url: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct PropertyEvidenceResponse {
    pub property_id: String,
    pub entity_refs: KgEntityRefs,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_bundle_version: Option<String>,
    pub sections: Vec<EvidenceSection>,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvidenceSection {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub subtitle: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    pub priority: u32,
    pub constellation: String,
    pub header_meta: String,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
    pub source_types: Vec<String>,
    pub entity_ids: Vec<String>,
    pub presentation: EvidencePresentation,
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<EvidenceMediaStrip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_pulse: Option<CommunityPulse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EvidencePresentation {
    pub variant: String,
    pub density: String,
    pub max_preview_items: usize,
}

#[derive(Deserialize)]
pub struct PropertyEvidenceBatchRequest {
    pub property_ids: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct PropertyEvidenceBatchResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serving_bundle_version: Option<String>,
    pub results: Vec<PropertyEvidenceResponse>,
    pub missing_property_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

fn canonical_property_id(id: &str) -> &str {
    match id {
        "discovered" => "discovered-sumadhura-eden-garden-3bhk",
        "fixture-prestige-lakeside-3bhk" => "discovered-prestige-lakeside-habitat-3bhk",
        "fixture-samadhura-capitol-3bhk" => "discovered-sumadhura-capitol-residences-3bhk",
        "fixture-vaswani-starlight-3bhk" => "discovered-vaswani-starlight-3bhk",
        "fixture-prestige-city-3bhk" => "discovered-the-prestige-city-3bhk",
        _ => id,
    }
}

fn find_property_by_request_id<'a>(
    properties: &'a [crate::models::Property],
    id: &str,
) -> Option<&'a crate::models::Property> {
    let canonical_id = canonical_property_id(id);
    properties.iter().find(|p| p.id == canonical_id)
}

fn normalized_builder_key(name: &str) -> String {
    const CORPORATE_SUFFIXES: &[&str] = &[
        "private",
        "pvt",
        "limited",
        "ltd",
        "llp",
        "inc",
        "corp",
        "corporation",
        "company",
        "co",
    ];

    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty() && !CORPORATE_SUFFIXES.contains(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn project_name_for(property: &crate::models::Property) -> String {
    let prefix = format!("{} BHK in ", property.bhk);
    property
        .title
        .strip_prefix(&prefix)
        .unwrap_or(&property.title)
        .to_string()
}

fn project_status_display_for(
    graph: &crate::knowledge::KnowledgeGraph,
    society_id: &str,
) -> Option<String> {
    let soc_node_id = society_node_id(society_id);
    let node = graph.get_node(&soc_node_id)?;
    node.facts
        .iter()
        .filter(|f| f.key == "project_status")
        .max_by_key(|f| f.version)
        .and_then(|f| {
            f.display_template.clone().map(|tmpl| {
                if tmpl.contains("{value}") {
                    if let crate::knowledge::FactValue::Text(ref val) = f.value {
                        tmpl.replace("{value}", val)
                    } else {
                        tmpl
                    }
                } else {
                    tmpl
                }
            })
        })
}

fn area_lookup_key(id_or_name: &str) -> String {
    let normalized = id_or_name.to_lowercase().replace(['_', ' '], "-");
    normalized
        .strip_prefix("area-")
        .unwrap_or(&normalized)
        .to_string()
}

fn fact_value_display(value: &FactValue) -> String {
    match value {
        FactValue::Numeric(n) => {
            if n.fract().abs() < f64::EPSILON {
                format!("{}", *n as i64)
            } else {
                format!("{n:.1}")
            }
        }
        FactValue::Text(text) => text.clone(),
        FactValue::Bool(value) => {
            if *value {
                "Yes".to_string()
            } else {
                "No".to_string()
            }
        }
        FactValue::Tags(tags) => tags.join(", "),
        FactValue::Score { value, explanation } => format!("{value:.1}: {explanation}"),
    }
}

fn fact_display(fact: &SourcedFact) -> String {
    let value = fact_value_display(&fact.value);
    fact.display_template
        .as_ref()
        .map(|template| template.replace("{value}", &value))
        .unwrap_or(value)
}

fn latest_fact<'a>(
    graph: &'a crate::knowledge::KnowledgeGraph,
    node_id: &str,
    key: &str,
) -> Option<&'a SourcedFact> {
    graph.get_node(node_id)?.get_fact(key)
}

fn source_item(
    graph: &crate::knowledge::KnowledgeGraph,
    node_id: &str,
    key: &str,
    label: &str,
) -> Option<SourceItem> {
    if key == "google_reviews_url" {
        if let Some(item) = google_reviews_url_source_item(graph, node_id, key, label) {
            return Some(item);
        }
    }
    let fact = latest_fact(graph, node_id, key)?;
    source_item_from_fact(node_id, fact, key, key, label)
}

fn google_reviews_url_source_item(
    graph: &crate::knowledge::KnowledgeGraph,
    node_id: &str,
    key: &str,
    label: &str,
) -> Option<SourceItem> {
    let node = graph.get_node(node_id)?;
    let url = google_reviews_url_from_facts(&node.facts, &node.name)?;
    Some(SourceItem {
        entity_id: node_id.to_string(),
        key: key.to_string(),
        label: label.to_string(),
        value: url.clone(),
        scope: entity_scope(node_id).to_string(),
        relationship: None,
        values: Vec::new(),
        source_type: "Google".to_string(),
        source_url: Some(url),
        attributions: Vec::new(),
        confidence_pct: 60,
        learned_at: node
            .facts
            .iter()
            .filter(|fact| fact.source.source_type == crate::knowledge::fact::SourceType::Google)
            .map(|fact| fact.learned_at)
            .max()
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339(),
    })
}

fn source_item_with_display_key(
    graph: &crate::knowledge::KnowledgeGraph,
    node_id: &str,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let fact = latest_fact(graph, node_id, fact_key)?;
    source_item_from_fact(node_id, fact, fact_key, display_key, label)
}

fn source_item_from_fact(
    entity_id: &str,
    fact: &SourcedFact,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let values = match &fact.value {
        FactValue::Tags(tags) => tags.clone(),
        _ => Vec::new(),
    };
    let value = match (&fact.value, fact_key) {
        (FactValue::Numeric(n), "rera_complaints_count") if (*n - 1.0).abs() < f64::EPSILON => {
            "1 complaint filed".to_string()
        }
        (FactValue::Numeric(n), "rera_complaints_count") => {
            format!("{} complaints filed", *n as i64)
        }
        (FactValue::Numeric(n), "rera_delay_months") if (*n - 1.0).abs() < f64::EPSILON => {
            "1 month delay".to_string()
        }
        (FactValue::Numeric(n), "rera_delay_months") => {
            format!("{} month delay", *n as i64)
        }
        (FactValue::Numeric(n), "rera_builder_revocations") if (*n - 1.0).abs() < f64::EPSILON => {
            "1 revocation".to_string()
        }
        (FactValue::Numeric(n), "rera_builder_revocations") => {
            format!("{} revocations", *n as i64)
        }
        (FactValue::Numeric(n), "reddit_thread_count") if (*n - 1.0).abs() < f64::EPSILON => {
            "1 thread".to_string()
        }
        (FactValue::Numeric(n), "reddit_thread_count") => {
            format!("{} threads", *n as i64)
        }
        (FactValue::Numeric(n), "reddit_total_comments") if (*n - 1.0).abs() < f64::EPSILON => {
            "1 comment counted".to_string()
        }
        (FactValue::Numeric(n), "reddit_total_comments") => {
            format!("{} comments counted", *n as i64)
        }
        (FactValue::Numeric(n), "reddit_total_score") => {
            format!("{} community score", *n as i64)
        }
        (FactValue::Tags(tags), "google_review_snippets") => {
            format!("{} Google review highlights", tags.len())
        }
        (FactValue::Tags(tags), "reddit_threads") => tags.join("\n"),
        _ => fact_display(fact),
    };
    if is_low_signal_source_value(display_key, &value) {
        return None;
    }
    Some(SourceItem {
        entity_id: entity_id.to_string(),
        key: display_key.to_string(),
        label: label.to_string(),
        value,
        scope: entity_scope(entity_id).to_string(),
        relationship: None,
        values,
        source_type: format!("{:?}", fact.source.source_type),
        source_url: fact.source.url.clone(),
        attributions: Vec::new(),
        confidence_pct: (fact.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
        learned_at: fact.learned_at.to_rfc3339(),
    })
}

fn is_low_signal_source_value(key: &str, value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    match key {
        "best_quote" | "sentiment_summary" | "resident_sentiment" | "google_sentiment" => {
            normalized.contains("not available")
                || normalized.contains("not captured")
                || normalized.contains("not provided")
                || normalized.contains("no specific resident sentiment")
                || normalized.contains("no reddit discussion content")
                || normalized.contains("no verbatim quotes")
                || normalized.contains("direct resident sentiment")
                || normalized.contains("actual content of the reddit discussions was not provided")
                || normalized.contains("quote can be extracted")
        }
        _ => false,
    }
}

fn serving_source_item(
    projection: &SocietyFactProjection<'_>,
    key: &str,
    label: &str,
) -> Option<SourceItem> {
    serving_source_item_with_display_key(projection, key, key, label)
}

fn serving_source_item_with_display_key(
    projection: &SocietyFactProjection<'_>,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let fact = projection.latest_record(fact_key)?;
    let display_template = projection
        .search_metadata(fact_key)
        .and_then(|metadata| metadata.display_template.as_deref());
    source_item_from_serving_fact(fact, fact_key, display_key, label, display_template)
}

fn serving_entity_source_item(
    facts: &ServingFactIndex,
    entity_id: &str,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let rows = facts.entity(entity_id)?;
    let fact = rows
        .facts
        .iter()
        .filter(|fact| fact.fact_key == fact_key)
        .max_by_key(|fact| fact.learned_at)?;
    let display_template = rows
        .search_metadata_for_fact_key(fact_key)
        .next()
        .and_then(|metadata| metadata.display_template.as_deref());
    source_item_from_serving_fact(fact, fact_key, display_key, label, display_template)
}

fn source_item_from_serving_fact(
    fact: &crate::serving::ServingFactRecord,
    fact_key: &str,
    display_key: &str,
    label: &str,
    display_template: Option<&str>,
) -> Option<SourceItem> {
    let values = match &fact.value {
        FactValue::Tags(tags) => tags.clone(),
        _ => Vec::new(),
    };
    let raw_value = fact_value_display(&fact.value);
    let value = if fact_key == "google_review_snippets" && !values.is_empty() {
        format!("{} Google review highlights", values.len())
    } else {
        display_template
            .map(|template| template.replace("{value}", &raw_value))
            .unwrap_or(raw_value)
    };
    if is_low_signal_source_value(display_key, &value) {
        return None;
    }
    Some(SourceItem {
        entity_id: fact.entity_id.clone(),
        key: display_key.to_string(),
        label: label.to_string(),
        value,
        scope: entity_scope(&fact.entity_id).to_string(),
        relationship: None,
        values,
        source_type: fact.source_type.clone(),
        source_url: fact.source_url.clone(),
        attributions: Vec::new(),
        confidence_pct: (fact.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
        learned_at: fact.learned_at.to_rfc3339(),
    })
}

#[derive(Clone)]
struct SourceValue {
    value: String,
    source_url: Option<String>,
    source_type: String,
    confidence_pct: u8,
    learned_at: chrono::DateTime<chrono::Utc>,
}

fn serving_multi_source_item(
    projection: &SocietyFactProjection<'_>,
    fact_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let mut values = Vec::<SourceValue>::new();
    for fact in projection.records(fact_key) {
        match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => values.push(SourceValue {
                value: value.trim().to_string(),
                source_url: fact.source_url.clone(),
                source_type: fact.source_type.clone(),
                confidence_pct: confidence_pct(fact.confidence),
                learned_at: fact.learned_at,
            }),
            FactValue::Tags(tags) => {
                for value in tags
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    values.push(SourceValue {
                        value: value.to_string(),
                        source_url: fact.source_url.clone(),
                        source_type: fact.source_type.clone(),
                        confidence_pct: confidence_pct(fact.confidence),
                        learned_at: fact.learned_at,
                    });
                }
            }
            _ => {}
        }
    }
    values.sort_by(|left, right| {
        nearby_distance_key(&left.value)
            .partial_cmp(&nearby_distance_key(&right.value))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.confidence_pct.cmp(&left.confidence_pct))
            .then_with(|| left.value.cmp(&right.value))
    });
    values.dedup_by(|left, right| left.value == right.value && left.source_url == right.source_url);
    values.truncate(source_value_limit(fact_key));
    if values.is_empty() {
        return None;
    }

    let first = values[0].clone();
    let learned_at = values
        .iter()
        .map(|value| value.learned_at)
        .max()
        .unwrap_or(first.learned_at);
    let confidence_pct = values
        .iter()
        .map(|value| value.confidence_pct)
        .max()
        .unwrap_or(0);
    let attributions = values
        .iter()
        .map(|value| SourceAttribution {
            value: value.value.clone(),
            source_url: value.source_url.clone(),
            source_type: value.source_type.clone(),
            confidence_pct: value.confidence_pct,
            learned_at: value.learned_at.to_rfc3339(),
        })
        .collect::<Vec<_>>();
    let item_values = values
        .iter()
        .map(|value| value.value.clone())
        .collect::<Vec<_>>();
    let value = if item_values.len() == 1 {
        item_values[0].clone()
    } else {
        format!("{} map-backed places", item_values.len())
    };

    Some(SourceItem {
        entity_id: projection
            .records(fact_key)
            .first()
            .map(|fact| fact.entity_id.clone())
            .unwrap_or_default(),
        key: fact_key.to_string(),
        label: label.to_string(),
        value,
        scope: "society".to_string(),
        relationship: None,
        values: item_values,
        source_type: first.source_type,
        source_url: first.source_url,
        attributions,
        confidence_pct,
        learned_at: learned_at.to_rfc3339(),
    })
}

fn confidence_pct(confidence: f32) -> u8 {
    (confidence * 100.0).round().clamp(0.0, 100.0) as u8
}

fn nearby_distance_key(value: &str) -> f64 {
    let Some(open_index) = value.find('(') else {
        return f64::INFINITY;
    };
    let tail = &value[open_index + 1..];
    let Some(km_index) = tail.find(" km") else {
        return f64::INFINITY;
    };
    tail[..km_index]
        .trim()
        .parse::<f64>()
        .unwrap_or(f64::INFINITY)
}

fn collect_society_source_items(
    graph: &crate::knowledge::KnowledgeGraph,
    node_id: &str,
    projection: Option<&SocietyFactProjection<'_>>,
    keys: &[(&str, &str)],
) -> Vec<SourceItem> {
    keys.iter()
        .filter_map(|(key, label)| {
            if key.starts_with("nearby_") {
                if let Some(item) = projection
                    .and_then(|projection| serving_multi_source_item(projection, key, label))
                {
                    return Some(item);
                }
            }
            projection
                .and_then(|projection| serving_source_item(projection, key, label))
                .or_else(|| source_item(graph, node_id, key, label))
                .or_else(|| {
                    source_item_aliases(key).iter().find_map(|alias| {
                        projection
                            .and_then(|projection| {
                                serving_source_item_with_display_key(projection, alias, key, label)
                            })
                            .or_else(|| {
                                source_item_with_display_key(graph, node_id, alias, key, label)
                            })
                    })
                })
        })
        .collect()
}

fn section_fact_key_labels(section_kind: &str) -> Vec<(&'static str, &'static str)> {
    evidence_section_definition(section_kind)
        .filter(|definition| !definition.facts.is_empty())
        .map(|definition| {
            definition
                .facts
                .iter()
                .map(|fact| (fact.key.as_str(), fact.label.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

fn source_value_limit(fact_key: &str) -> usize {
    buyer_context_definitions()
        .iter()
        .flat_map(|definition| definition.facts.iter())
        .find(|fact| fact.key == fact_key)
        .and_then(|fact| fact.max_values)
        .unwrap_or(5)
}

fn collect_community_evidence_records(
    graph: &crate::knowledge::KnowledgeGraph,
    society_id: &str,
    area_name: Option<&str>,
    projection: Option<&SocietyFactProjection<'_>>,
) -> Vec<crate::community::CommunityEvidenceRecord> {
    // Source-backed community keys. UI summaries are composed at request time
    // from these facts instead of reading precomputed lake text.
    const PRIMARY_FACT_KEYS: &[&str] = &[
        "google_rating",
        "google_review_count",
        "google_review_snippets",
        "google_reviews_url",
        "resident_discussion",
    ];
    // Legacy pre-DAG keys, only consulted when no primary facts exist, so stale
    // generated quotes never leak alongside current evidence.
    const LEGACY_FALLBACK_KEYS: &[&str] = &[
        "resident_sentiment",
        "sentiment_summary",
        "best_quote",
        "common_positives",
        "common_complaints",
        "google_sentiment",
        "google_top_positives",
        "google_top_negatives",
        "google_common_themes",
    ];

    let mut records =
        collect_community_records_for_keys(graph, society_id, projection, PRIMARY_FACT_KEYS);
    if records.is_empty() {
        records =
            collect_community_records_for_keys(graph, society_id, projection, LEGACY_FALLBACK_KEYS);
    }
    records.extend(collect_area_community_records(
        graph,
        area_name.map(super::enrichment::area_node_id),
    ));
    filter_reddit_evidence(records)
}

fn collect_area_community_records(
    graph: &crate::knowledge::KnowledgeGraph,
    area_id: Option<String>,
) -> Vec<crate::community::CommunityEvidenceRecord> {
    let Some(area_id) = area_id else {
        return Vec::new();
    };
    const AREA_FACT_KEYS: &[&str] = &[
        "traffic_reality",
        "waterlogging_detail",
        "waterlogging_risk",
        "lake_waterlogging_context",
        "resident_discussion",
        "google_review_snippets",
    ];
    collect_community_records_for_keys(graph, &area_id, None, AREA_FACT_KEYS)
}

fn collect_structured_livability_facts(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<StructuredFactSignal> {
    let society_id = society_node_id(&property.society_id);
    let area_id = super::enrichment::area_node_id(&property.area);
    let definitions: &[(&str, &str, LivabilityLens, &str)] = &[
        (
            "home_state",
            "delivery state",
            LivabilityLens::Lifecycle,
            "society",
        ),
        (
            "home_age_bucket",
            "project age",
            LivabilityLens::Lifecycle,
            "society",
        ),
        (
            "home_timeline_state",
            "project timeline",
            LivabilityLens::Lifecycle,
            "society",
        ),
        (
            "approach_road_condition",
            "approach road access",
            LivabilityLens::Risk,
            "society",
        ),
        (
            "access_road_quality",
            "road quality",
            LivabilityLens::Risk,
            "road_segment",
        ),
        (
            "road_width",
            "road width",
            LivabilityLens::Risk,
            "road_segment",
        ),
        (
            "waterlogging_detail",
            "area waterlogging",
            LivabilityLens::Risk,
            "area",
        ),
        (
            "waterlogging_risk",
            "waterlogging risk",
            LivabilityLens::Risk,
            "area",
        ),
        (
            "stp_concern",
            "STP concern",
            LivabilityLens::Risk,
            "society",
        ),
        (
            "high_tension_wire_concern",
            "high-tension wires",
            LivabilityLens::Risk,
            "society",
        ),
        (
            "nearby_schools",
            "school access",
            LivabilityLens::Positive,
            "society",
        ),
        (
            "nearby_metro_stations",
            "metro access",
            LivabilityLens::Positive,
            "society",
        ),
    ];

    let mut signals = Vec::new();
    for (fact_key, label, lens, scope) in definitions {
        let entity_id = match *scope {
            "area" => area_id.as_str(),
            _ => society_id.as_str(),
        };
        let has_fact = if *scope == "road_segment" {
            serving_facts.is_some_and(|facts| {
                road_segment_entity_ids(property, facts, graph_index)
                    .iter()
                    .any(|entity_id| {
                        facts.entity(entity_id).is_some_and(|rows| {
                            rows.facts.iter().any(|fact| fact.fact_key == *fact_key)
                        })
                    })
            })
        } else {
            projection
                .and_then(|projection| projection.latest_record(fact_key))
                .is_some()
                || graph
                    .get_node(entity_id)
                    .is_some_and(|node| node.facts.iter().any(|fact| fact.key == *fact_key))
        };
        if has_fact {
            signals.push(StructuredFactSignal {
                fact_key: (*fact_key).to_string(),
                label: (*label).to_string(),
                lens: *lens,
            });
        }
    }
    signals
}

fn build_livability_brief(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    society_name: &str,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    community_records: &[crate::community::CommunityEvidenceRecord],
    community_pulse: Option<&CommunityPulse>,
) -> Option<LivabilityBrief> {
    let home_state_evidence = projection.map(|projection| projection.project_home_state());
    let home_state = home_state_evidence
        .as_ref()
        .and_then(|evidence| evidence.state.as_deref());
    let home_age_bucket = home_state_evidence
        .as_ref()
        .and_then(|evidence| evidence.age_bucket.as_deref());
    let home_timeline_state = projection
        .and_then(|projection| projection.latest_text("home_timeline_state"))
        .map(|fact| fact.value);
    let home_timeline_ref = home_timeline_state.as_deref();
    let structured_facts = collect_structured_livability_facts(
        graph,
        property,
        projection,
        serving_facts,
        graph_index,
    );
    let (community_positives, community_concerns, source_urls) =
        if let Some(pulse) = community_pulse {
            // Brief owns synthesized themes; pulse keeps review receipts only.
            (&[][..], &[][..], pulse.source_urls.as_slice())
        } else {
            (&[][..], &[][..], &[][..])
        };

    compose_livability_brief(&LivabilityBriefInput {
        society_name,
        area_name: &property.area,
        home_state,
        home_age_bucket,
        home_timeline_state: home_timeline_ref,
        evidence_records: community_records,
        structured_facts: &structured_facts,
        community_positives,
        community_concerns,
        source_urls,
    })
}

fn enrich_community_pulse_source_urls(
    graph: &crate::knowledge::KnowledgeGraph,
    society_id: &str,
    pulse: &mut CommunityPulse,
) {
    if let Some(node) = graph.get_node(society_id) {
        if let Some(url) = google_reviews_url_from_facts(&node.facts, &node.name) {
            if !pulse.source_urls.iter().any(|existing| existing == &url) {
                pulse.source_urls.push(url);
            }
        }
    }
    pulse.source_urls.sort();
    pulse.source_urls.dedup();
    pulse.source_urls.truncate(5);
}

fn collect_community_records_for_keys(
    graph: &crate::knowledge::KnowledgeGraph,
    society_id: &str,
    projection: Option<&SocietyFactProjection<'_>>,
    keys: &[&str],
) -> Vec<crate::community::CommunityEvidenceRecord> {
    let mut records = Vec::new();
    for key in keys {
        if let Some(record) = projection
            .and_then(|projection| projection.latest_record(key))
            .and_then(|fact| {
                community_evidence_from_fact_value(
                    &fact.entity_id,
                    &fact.source_type,
                    fact.source_url.clone(),
                    key,
                    &fact.value,
                    fact.confidence,
                    fact.learned_at,
                )
            })
        {
            records.push(record);
            continue;
        }

        if let Some(node) = graph.get_node(society_id) {
            if let Some(fact) = node
                .facts
                .iter()
                .filter(|fact| fact.key == *key)
                .max_by_key(|fact| fact.version)
            {
                if let Some(record) = community_evidence_from_fact_value(
                    society_id,
                    &format!("{:?}", fact.source.source_type),
                    fact.source.url.clone(),
                    key,
                    &fact.value,
                    fact.confidence,
                    fact.learned_at,
                ) {
                    records.push(record);
                }
            }
        }
    }
    records
}

fn source_item_aliases(_key: &str) -> &'static [&'static str] {
    &[]
}

fn entity_scope(entity_id: &str) -> &'static str {
    if entity_id.starts_with("property:") {
        "property"
    } else if entity_id.starts_with("society:") || entity_id.starts_with("soc-") {
        "society"
    } else if entity_id.starts_with("area:") || entity_id.starts_with("area-") {
        "area"
    } else if entity_id.starts_with("builder:") {
        "builder"
    } else if entity_id.starts_with("road_segment:") {
        "road_segment"
    } else if entity_id.starts_with("road:") {
        "road"
    } else if entity_id.starts_with("poi:") {
        "poi"
    } else if entity_id.starts_with("waterbody:") {
        "waterbody"
    } else {
        "entity"
    }
}

fn approach_road_media_for(
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<EvidenceMediaStrip> {
    let api_key = crate::street_view::google_maps_api_key()?;
    let facts = serving_facts?;
    let road_entity_id = resolve_road_segment_entity_id(property, facts, graph_index)?;
    let fact = facts
        .entity(&road_entity_id)?
        .facts
        .iter()
        .find(|fact| fact.fact_key == "media.approach_road_frames")?;
    let payload = match &fact.value {
        FactValue::Text(text) => text.as_str(),
        _ => return None,
    };
    let record: ApproachRoadVisualRecord = serde_json::from_str(payload).ok()?;
    if record.coverage_quality == "missing" || record.frames.is_empty() {
        return None;
    }

    let frames = record
        .frames
        .into_iter()
        .take(5)
        .filter_map(|frame| approach_road_media_frame(frame, &api_key))
        .collect::<Vec<_>>();
    if frames.is_empty() {
        return None;
    }

    let capture_date_label = capture_date_label(&frames);
    let caption = format!("Last-lane context · {capture_date_label}");
    Some(EvidenceMediaStrip {
        kind: "street_view_strip".to_string(),
        provider: record.provider,
        title: "Approach road".to_string(),
        caption,
        capture_date_label,
        coverage_quality: record.coverage_quality,
        frames,
    })
}

fn resolve_road_segment_entity_id(
    property: &crate::models::Property,
    serving_facts: &ServingFactIndex,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<String> {
    road_segment_entity_ids(property, serving_facts, graph_index)
        .into_iter()
        .find(|entity_id| has_approach_road_frames(serving_facts, entity_id))
}

fn road_segment_entity_ids(
    property: &crate::models::Property,
    serving_facts: &ServingFactIndex,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<String> {
    let society_id = crate::routes::enrichment::society_node_id(&property.society_id);
    let mut entity_ids = Vec::new();
    let mut seen = HashSet::new();

    if let Some(index) = graph_index {
        let steps = index.walk_out(&society_id, &["served_by_road"], 1);
        for step in steps {
            if !step.to_entity_id.starts_with("road_segment:") {
                continue;
            }
            if serving_facts.entity(&step.to_entity_id).is_none() {
                continue;
            }
            if seen.insert(step.to_entity_id.clone()) {
                entity_ids.push(step.to_entity_id);
            }
        }
    }

    let slug = society_id
        .strip_prefix("society:")
        .unwrap_or(society_id.as_str());
    let fallback = format!("road_segment:{slug}-approach");
    if serving_facts.entity(&fallback).is_some() && seen.insert(fallback.clone()) {
        entity_ids.push(fallback);
    }

    entity_ids
}

fn has_approach_road_frames(serving_facts: &ServingFactIndex, entity_id: &str) -> bool {
    serving_facts.entity(entity_id).is_some_and(|rows| {
        rows.facts
            .iter()
            .any(|fact| fact.fact_key == "media.approach_road_frames")
    })
}

fn approach_road_media_frame(
    frame: ApproachRoadVisualFrameRecord,
    api_key: &str,
) -> Option<EvidenceMediaFrame> {
    let street_input = crate::street_view::StreetViewFrameInput {
        pano_id: frame.pano_id.clone(),
        location: match (frame.latitude, frame.longitude) {
            (Some(latitude), Some(longitude)) => Some(crate::street_view::StreetViewLocation {
                latitude,
                longitude,
            }),
            _ => None,
        },
        location_query: frame.location_query.clone(),
        radius_m: frame.radius_m,
        heading: frame.heading,
        pitch: frame.pitch,
        fov: frame.fov,
    };
    let image_url = frame
        .image_url
        .filter(|url| !url.trim().is_empty())
        .or_else(|| crate::street_view::street_view_static_url(&street_input, api_key))?;
    let source_url = crate::street_view::street_view_pano_url(&street_input)?;

    Some(EvidenceMediaFrame {
        label: frame.label,
        distance_from_gate_m: frame.distance_from_gate_m,
        image_url,
        heading: frame.heading,
        pitch: frame.pitch,
        fov: frame.fov,
        capture_date: frame.capture_date,
        source_url,
    })
}

fn capture_date_label(frames: &[EvidenceMediaFrame]) -> String {
    frames
        .iter()
        .map(|frame| frame.capture_date.as_str())
        .max()
        .map(|date| format!("Street View {date}"))
        .unwrap_or_else(|| "Street View".to_string())
}

fn evidence_media_source_item(media: &EvidenceMediaStrip) -> SourceItem {
    let learned_at = media
        .frames
        .iter()
        .filter_map(|frame| {
            chrono::NaiveDate::parse_from_str(&format!("{}-01", frame.capture_date), "%Y-%m-%d")
                .ok()
        })
        .max()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|date| date.and_utc().to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let source_url = media.frames.first().map(|frame| frame.source_url.clone());
    let value = match media.kind.as_str() {
        "street_view_strip" => format!("{} outside road views", media.frames.len()),
        _ => format!("{} visual receipts", media.frames.len()),
    };
    let values = media
        .frames
        .iter()
        .map(|frame| frame.label.clone())
        .collect::<Vec<_>>();

    SourceItem {
        entity_id: "road_segment:approach-road".to_string(),
        key: format!("{}_available", media.kind),
        label: media.title.clone(),
        value,
        scope: "road_segment".to_string(),
        relationship: Some("gate approach".to_string()),
        values,
        source_type: "Google".to_string(),
        source_url,
        attributions: Vec::new(),
        confidence_pct: if media.coverage_quality == "strong" {
            85
        } else {
            72
        },
        learned_at,
    }
}

#[derive(Clone, Deserialize)]
struct ContextFactDefinition {
    key: String,
    label: String,
    scope: String,
    relationship: String,
    #[serde(default)]
    max_values: Option<usize>,
}

#[derive(Clone, Deserialize)]
struct BuyerContextDefinition {
    kind: String,
    #[serde(default)]
    priority: u32,
    #[serde(default)]
    constellation: String,
    #[serde(default)]
    surfaces: Vec<String>,
    title: String,
    subtitle: String,
    scope: String,
    relationship: String,
    #[serde(default)]
    presentation: Option<EvidencePresentation>,
    #[serde(default)]
    media: Vec<String>,
    facts: Vec<ContextFactDefinition>,
}

fn evidence_section_definition(kind: &str) -> Option<&'static BuyerContextDefinition> {
    buyer_context_definitions()
        .iter()
        .find(|definition| definition.kind == kind)
}

fn default_evidence_presentation() -> EvidencePresentation {
    EvidencePresentation {
        variant: "fact_list".to_string(),
        density: "standard".to_string(),
        max_preview_items: 4,
    }
}

fn buyer_context_definitions() -> &'static [BuyerContextDefinition] {
    BUYER_CONTEXT_DEFINITIONS
        .get_or_init(|| {
            serde_json::from_str(BUYER_CONTEXT_SECTIONS_JSON)
                .expect("app/config/product/evidence_sections.json should be valid")
        })
        .as_slice()
}

fn build_buyer_context_panels(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<SourcePanel> {
    buyer_context_definitions()
        .iter()
        .filter(|definition| {
            definition
                .surfaces
                .iter()
                .any(|surface| surface == "buyer_context")
        })
        .filter_map(|definition| {
            let mut items = collect_buyer_context_items(
                graph,
                property,
                projection,
                serving_facts,
                graph_index,
                &definition.facts,
            );
            let media = definition
                .media
                .iter()
                .filter_map(|media_kind| {
                    context_media_for(media_kind, property, serving_facts, graph_index)
                })
                .inspect(|media| {
                    items.push(evidence_media_source_item(media));
                })
                .collect::<Vec<_>>();
            (!items.is_empty() || !media.is_empty()).then(|| SourcePanel {
                kind: definition.kind.clone(),
                title: definition.title.clone(),
                subtitle: definition.subtitle.clone(),
                scope: definition.scope.clone(),
                relationship: Some(definition.relationship.clone()),
                items,
                missing: Vec::new(),
                media,
                community_pulse: None,
            })
        })
        .collect()
}

fn collect_buyer_context_items(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    facts: &[ContextFactDefinition],
) -> Vec<SourceItem> {
    let society_id = society_node_id(&property.society_id);
    let area_id = super::enrichment::area_node_id(&property.area);
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for fact in facts {
        let scoped = match fact.scope.as_str() {
            "area" | "waterbody" | "poi" => source_item(graph, &area_id, &fact.key, &fact.label)
                .or_else(|| {
                    projection.and_then(|projection| {
                        serving_source_item(projection, &fact.key, &fact.label)
                    })
                }),
            "road_segment" => serving_road_segment_source_item(
                property,
                serving_facts,
                graph_index,
                &fact.key,
                &fact.label,
            )
            .or_else(|| source_item(graph, &society_id, &fact.key, &fact.label)),
            _ => projection
                .and_then(|projection| serving_source_item(projection, &fact.key, &fact.label))
                .or_else(|| source_item(graph, &society_id, &fact.key, &fact.label))
                .or_else(|| {
                    serving_related_society_source_item(
                        property,
                        serving_facts,
                        &fact.key,
                        &fact.label,
                    )
                }),
        };
        if let Some(item) = scoped.map(|item| with_context_scope(item, fact)) {
            let key = format!("{}:{}:{}", item.entity_id, item.key, item.value);
            if seen.insert(key) {
                items.push(item);
            }
        }
    }

    items
}

fn serving_related_society_source_item(
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    fact_key: &str,
    label: &str,
) -> Option<SourceItem> {
    if !fact_key.starts_with("environment.") {
        return None;
    }
    let facts = serving_facts?;
    let target_names = related_society_match_names(property);
    if target_names.is_empty() {
        return None;
    }

    facts.rows().find_map(|(entity_id, rows)| {
        if !entity_id.starts_with("society:") {
            return None;
        }
        if !rows.facts.iter().any(|fact| fact.fact_key == fact_key) {
            return None;
        }
        if !serving_society_rows_match_names(rows, &target_names) {
            return None;
        }
        serving_entity_source_item(facts, entity_id, fact_key, fact_key, label)
    })
}

fn related_society_match_names(property: &crate::models::Property) -> Vec<String> {
    let mut names = vec![project_name_for(property), property.society_id.clone()];
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| normalized_project_name(&name))
        .collect()
}

fn serving_society_rows_match_names(
    rows: &crate::serving::ServingEntityFactRows,
    target_names: &[String],
) -> bool {
    const NAME_FACT_KEYS: &[&str] = &["listing_society", "title", "rera_project_name"];
    rows.facts.iter().any(|fact| {
        NAME_FACT_KEYS.contains(&fact.fact_key.as_str())
            && match &fact.value {
                FactValue::Text(value) => normalized_project_name(value)
                    .is_some_and(|name| target_names.iter().any(|target| target == &name)),
                _ => false,
            }
    })
}

fn normalized_project_name(value: &str) -> Option<String> {
    let normalized = value
        .to_lowercase()
        .replace('&', " and ")
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| {
            !token.is_empty()
                && !matches!(
                    *token,
                    "soc" | "society" | "rera" | "project" | "phase" | "the"
                )
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn serving_road_segment_source_item(
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    fact_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let facts = serving_facts?;
    road_segment_entity_ids(property, facts, graph_index)
        .iter()
        .find_map(|entity_id| {
            serving_entity_source_item(facts, entity_id, fact_key, fact_key, label)
        })
}

fn with_context_scope(mut item: SourceItem, fact: &ContextFactDefinition) -> SourceItem {
    item.scope = fact.scope.clone();
    item.relationship = Some(fact.relationship.clone());
    item
}

fn context_media_for(
    media_kind: &str,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<EvidenceMediaStrip> {
    match media_kind {
        "approach_road_visuals" => approach_road_media_for(property, serving_facts, graph_index),
        _ => None,
    }
}

pub(crate) fn build_source_panels(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<SourcePanel> {
    let society_id = society_node_id(&property.society_id);
    let projection =
        serving_facts.map(|facts| SocietyFactProjection::from_index(facts, &property.society_id));

    let mut panels = Vec::new();
    panels.extend(build_buyer_context_panels(
        graph,
        property,
        projection.as_ref(),
        serving_facts,
        graph_index,
    ));

    let rera_items = collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("rera_status", "Status"),
            ("rera_number", "Registration"),
            ("rera_completion_date", "Completion"),
            ("rera_total_land_area_sqm", "Land area"),
            ("rera_delay_months", "Delay"),
            ("rera_complaints_count", "Complaints"),
            ("rera_builder_revocations", "Builder revocations"),
        ],
    );
    if !rera_items.is_empty() {
        panels.push(SourcePanel {
            kind: "rera".to_string(),
            title: "RERA file".to_string(),
            subtitle: "Official project registration and delivery record.".to_string(),
            scope: "society".to_string(),
            relationship: Some("project registration".to_string()),
            items: rera_items,
            missing: vec![],
            media: vec![],
            community_pulse: None,
        });
    }

    let market_fact_keys = section_fact_key_labels("market");
    let market_items =
        collect_society_source_items(graph, &society_id, projection.as_ref(), &market_fact_keys);
    panels.push(SourcePanel {
        kind: "market".to_string(),
        title: "Market trail".to_string(),
        subtitle: "Pricing, appreciation, and nearby comparable signals.".to_string(),
        scope: "society".to_string(),
        relationship: Some("project market trail".to_string()),
        items: market_items,
        missing: vec!["Registered resale transaction comps are not linked yet.".to_string()],
        media: vec![],
        community_pulse: None,
    });

    let nearby_fact_keys = section_fact_key_labels("nearby");
    let nearby_items =
        collect_society_source_items(graph, &society_id, projection.as_ref(), &nearby_fact_keys);
    if !nearby_items.is_empty() {
        panels.push(SourcePanel {
            kind: "nearby".to_string(),
            title: "Nearby".to_string(),
            subtitle: "Map-backed places near the society.".to_string(),
            scope: "society".to_string(),
            relationship: Some("nearby map context".to_string()),
            items: nearby_items,
            missing: vec![],
            media: vec![],
            community_pulse: None,
        });
    }

    let community_records = collect_community_evidence_records(
        graph,
        &society_id,
        Some(property.area.as_str()),
        projection.as_ref(),
    );
    let mut community_pulse = if community_records.is_empty() {
        None
    } else {
        deterministic_community_summarizer()
            .summarize(&community_records)
            .into_iter()
            .next()
            .map(|summary| community_pulse_from_summary(&summary))
    };
    if let Some(pulse) = community_pulse.as_mut() {
        enrich_community_pulse_source_urls(graph, &society_id, pulse);
    }

    if let Some(pulse) = community_pulse.clone() {
        let mut community_missing = Vec::new();
        if pulse.positives.is_empty() && pulse.concerns.is_empty() {
            community_missing
                .push("Review themes are still being expanded across sources.".to_string());
        }
        if pulse.quotes.is_empty() {
            community_missing.push("Review snippets are still being extracted.".to_string());
        }

        panels.push(SourcePanel {
            kind: "community".to_string(),
            title: "Community pulse".to_string(),
            subtitle: format!("{} · {}", pulse.source_label, pulse.sentiment_band),
            scope: "society".to_string(),
            relationship: Some("resident and review context".to_string()),
            items: Vec::new(),
            missing: community_missing,
            media: vec![],
            community_pulse: Some(pulse),
        });
    }

    panels
        .into_iter()
        .filter(|panel| {
            !panel.items.is_empty() || !panel.media.is_empty() || panel.community_pulse.is_some()
        })
        .collect()
}

fn build_property_evidence_response(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    serving_bundle: Option<&LoadedServingBundle>,
) -> PropertyEvidenceResponse {
    let entity_refs = kg_entity_refs_for_property(property, graph);
    let source_panels = build_source_panels(
        graph,
        property,
        serving_bundle.map(|bundle| &bundle.fact_index),
        serving_bundle.map(|bundle| &bundle.graph_index),
    );
    build_property_evidence_response_from_panels(
        property.id.clone(),
        entity_refs,
        serving_bundle,
        source_panels,
    )
}

fn build_property_evidence_response_from_panels(
    property_id: String,
    entity_refs: KgEntityRefs,
    serving_bundle: Option<&LoadedServingBundle>,
    source_panels: Vec<SourcePanel>,
) -> PropertyEvidenceResponse {
    let mut sections = source_panels
        .into_iter()
        .map(|panel| evidence_section_from_panel(panel, &entity_refs))
        .collect::<Vec<_>>();
    sections.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.title.cmp(&right.title))
    });

    PropertyEvidenceResponse {
        property_id,
        entity_refs,
        serving_bundle_version: serving_bundle.map(|bundle| bundle.manifest.bundle_version.clone()),
        sections,
    }
}

pub(crate) fn evidence_section_from_panel(
    panel: SourcePanel,
    entity_refs: &KgEntityRefs,
) -> EvidenceSection {
    let source_types = unique_sorted(
        panel
            .items
            .iter()
            .map(|item| item.source_type.clone())
            .collect(),
    );
    let mut entity_ids = unique_sorted(
        panel
            .items
            .iter()
            .map(|item| item.entity_id.clone())
            .collect(),
    );
    if entity_ids.is_empty() {
        entity_ids = fallback_section_entity_ids(&panel.kind, entity_refs);
    }
    let confidence_pct = panel
        .community_pulse
        .as_ref()
        .map(|pulse| pulse.confidence_pct)
        .filter(|_| panel.kind == "community")
        .unwrap_or_else(|| section_confidence_pct(&panel.items));
    let summary = panel
        .community_pulse
        .as_ref()
        .map(|pulse| pulse.paragraph.clone())
        .unwrap_or_else(|| section_summary(&panel));
    let presentation = evidence_section_presentation(&panel.kind);
    let constellation = evidence_section_constellation(&panel.kind);
    let header_meta = section_header_meta(&panel, &source_types);

    EvidenceSection {
        priority: evidence_section_priority(&panel.kind),
        kind: panel.kind,
        title: panel.title,
        summary,
        subtitle: panel.subtitle,
        scope: panel.scope,
        relationship: panel.relationship,
        constellation,
        header_meta,
        confidence_pct,
        source_types,
        entity_ids,
        items: panel.items,
        presentation,
        missing: panel.missing,
        media: panel.media,
        community_pulse: panel.community_pulse,
    }
}

fn evidence_section_constellation(kind: &str) -> String {
    evidence_section_definition(kind)
        .map(|definition| definition.constellation.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "trust".to_string())
}

fn section_header_meta(panel: &SourcePanel, source_types: &[String]) -> String {
    let fact_count = if let Some(pulse) = panel.community_pulse.as_ref() {
        pulse.quotes.len()
    } else {
        panel
            .items
            .iter()
            .filter(|item| source_item_has_display_value(item))
            .count()
    };
    if source_types.is_empty() {
        format!("{fact_count} facts")
    } else {
        format!("{fact_count} facts · {}", source_types.join(", "))
    }
}

fn source_item_has_display_value(item: &SourceItem) -> bool {
    item.values.iter().any(|value| !value.trim().is_empty()) || !item.value.trim().is_empty()
}

fn dedup_community_pulse_against_brief(panels: &mut [SourcePanel], brief: &LivabilityBrief) {
    let mut brief_themes = std::collections::BTreeSet::new();
    for block in &brief.blocks {
        for theme in &block.themes {
            brief_themes.insert(theme.to_ascii_lowercase());
        }
    }
    if brief_themes.is_empty() {
        return;
    }

    for panel in panels.iter_mut() {
        if panel.kind != "community" {
            continue;
        }
        let Some(pulse) = panel.community_pulse.as_mut() else {
            continue;
        };
        pulse
            .positives
            .retain(|theme| !brief_themes.contains(&theme.to_ascii_lowercase()));
        pulse
            .concerns
            .retain(|theme| !brief_themes.contains(&theme.to_ascii_lowercase()));
    }
}

fn evidence_section_presentation(kind: &str) -> EvidencePresentation {
    evidence_section_definition(kind)
        .and_then(|definition| definition.presentation.clone())
        .unwrap_or_else(default_evidence_presentation)
}

fn evidence_section_priority(kind: &str) -> u32 {
    evidence_section_definition(kind)
        .map(|definition| definition.priority)
        .unwrap_or(100)
}

fn section_confidence_pct(items: &[SourceItem]) -> u8 {
    if items.is_empty() {
        return 0;
    }
    let total = items
        .iter()
        .map(|item| u32::from(item.confidence_pct))
        .sum::<u32>();
    ((total / items.len() as u32).min(100)) as u8
}

fn section_summary(panel: &SourcePanel) -> String {
    if let Some(item) = primary_section_item(panel) {
        if panel.kind == "water_context" && item.key == "environment.groundwater_potential_class" {
            return groundwater_potential_summary(item);
        }
        return source_item_summary(item);
    }
    if let Some(media) = panel.media.first() {
        return media.caption.clone();
    }
    panel
        .subtitle
        .trim()
        .is_empty()
        .then(|| "Evidence will appear here once source-backed facts are promoted.".to_string())
        .unwrap_or_else(|| truncate_summary(&panel.subtitle, 96))
}

fn groundwater_potential_summary(item: &SourceItem) -> String {
    let class = item
        .value
        .rsplit_once(':')
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| item.value.trim());
    if class.is_empty() {
        return "Groundwater potential context is available for this society.".to_string();
    }
    format!("{class} groundwater potential zone near the society.")
}

fn primary_section_item(panel: &SourcePanel) -> Option<&SourceItem> {
    if let Some(definition) = evidence_section_definition(&panel.kind) {
        for fact in &definition.facts {
            if let Some(item) = panel.items.iter().find(|item| item.key == fact.key) {
                return Some(item);
            }
        }
    }
    panel.items.first()
}

fn fallback_section_entity_ids(kind: &str, entity_refs: &KgEntityRefs) -> Vec<String> {
    let scope = evidence_section_definition(kind)
        .map(|definition| definition.scope.as_str())
        .unwrap_or("property");
    entity_ids_for_section_scope(scope, entity_refs)
}

fn entity_ids_for_section_scope(scope: &str, entity_refs: &KgEntityRefs) -> Vec<String> {
    match scope {
        "area" | "waterbody" | "poi" => vec![entity_refs.area_entity_id.clone()],
        "society" => vec![entity_refs.society_entity_id.clone()],
        "road_segment" => vec![
            entity_refs.society_entity_id.clone(),
            "road_segment:approach-road".to_string(),
        ],
        _ => vec![entity_refs.property_entity_id.clone()],
    }
}

fn source_item_summary(item: &SourceItem) -> String {
    if let Some(first_value) = item.values.first() {
        let suffix = if item.values.len() > 1 {
            format!(" +{} more", item.values.len() - 1)
        } else {
            String::new()
        };
        return format!(
            "{}: {}{}",
            item.label,
            truncate_summary(first_value, 80),
            suffix
        );
    }
    let value = truncate_summary(&item.value, 96);
    if source_value_is_self_labeled(&item.value, &item.label) {
        value
    } else {
        format!("{}: {value}", item.label)
    }
}

fn source_value_is_self_labeled(value: &str, label: &str) -> bool {
    let value = value.trim();
    let label = label.trim();
    if value.is_empty() || label.is_empty() {
        return false;
    }

    let normalized_value = value.to_lowercase();
    let normalized_label = label.to_lowercase();
    if normalized_value.starts_with(&format!("{normalized_label}:")) {
        return true;
    }

    let Some(colon_index) = value.find(':') else {
        return false;
    };
    if colon_index > 48 || value.starts_with("http://") || value.starts_with("https://") {
        return false;
    }

    let prefix = value[..colon_index].trim();
    prefix
        .chars()
        .any(|character| character.is_ascii_alphabetic())
}

fn truncate_summary(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out = trimmed.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn unique_sorted(mut values: Vec<String>) -> Vec<String> {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
    values
}

fn build_builder_portfolio(
    graph: &crate::knowledge::KnowledgeGraph,
    properties: &[crate::models::Property],
    current: &crate::models::Property,
) -> Option<BuilderPortfolio> {
    let builder_key = normalized_builder_key(&current.builder_name);
    if builder_key.is_empty() {
        return None;
    }

    let mut seen_societies = HashSet::new();
    let mut projects = Vec::new();
    let mut rera_registered_projects = 0;
    let mut delayed_projects = 0;
    let mut complaint_projects = 0;
    let mut revocations: Option<i32> = None;

    for property in properties {
        if normalized_builder_key(&property.builder_name) != builder_key {
            continue;
        }
        if !seen_societies.insert(property.society_id.clone()) {
            continue;
        }

        let rera = extract_rera_info(graph, &property.society_id);
        if rera.as_ref().is_some_and(|r| r.registered) {
            rera_registered_projects += 1;
        }
        if rera
            .as_ref()
            .and_then(|r| r.delay_months)
            .is_some_and(|months| months > 0)
        {
            delayed_projects += 1;
        }
        if rera
            .as_ref()
            .and_then(|r| r.complaints_count)
            .is_some_and(|count| count > 0)
        {
            complaint_projects += 1;
        }
        if let Some(builder_revocations) = rera.as_ref().and_then(|r| r.builder_revocations) {
            revocations = Some(revocations.unwrap_or(0).max(builder_revocations));
        }

        projects.push(BuilderProjectRecord {
            property_id: property.id.clone(),
            project_name: project_name_for(property),
            area: property.area.clone(),
            rera_number: rera.as_ref().and_then(|r| r.registration_number.clone()),
            rera_portal_url: rera.as_ref().and_then(|r| r.rera_portal_url.clone()),
            rera_status: rera.as_ref().and_then(|r| r.status.clone()),
            completion_date: rera.as_ref().and_then(|r| r.completion_date.clone()),
            delay_months: rera.as_ref().and_then(|r| r.delay_months),
            complaints_count: rera.as_ref().and_then(|r| r.complaints_count),
            project_status_display: project_status_display_for(graph, &property.society_id),
            current: property.society_id == current.society_id,
        });
    }

    if projects.len() <= 1 && rera_registered_projects == 0 {
        return None;
    }

    projects.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| a.project_name.cmp(&b.project_name))
    });
    let tracked_projects = projects.len();
    projects.truncate(8);

    Some(BuilderPortfolio {
        builder_name: current.builder_name.clone(),
        tracked_projects,
        rera_registered_projects,
        delayed_projects,
        complaint_projects,
        revocations,
        projects,
    })
}

/// GET /api/properties/:id/evidence — returns backend-shaped dynamic evidence
/// sections for the UI. React should render these sections, not interpret raw KG
/// facts itself.
pub async fn get_property_evidence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyEvidenceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let properties = state.properties.read().await;
    let property = find_property_by_request_id(&properties, &id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        )
    })?;
    let serving_bundle = state.serving_bundle.read().await.clone();
    let graph = state.knowledge.read().await;

    Ok(Json(build_property_evidence_response(
        &graph,
        property,
        serving_bundle.as_deref(),
    )))
}

const MAX_EVIDENCE_BATCH_SIZE: usize = 20;

/// POST /api/properties/evidence/batch — returns evidence sections for a bounded
/// set of property IDs so search/list/compare views can prefetch dynamic cards.
pub async fn get_property_evidence_batch(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PropertyEvidenceBatchRequest>,
) -> Json<PropertyEvidenceBatchResponse> {
    let limit = request
        .limit
        .unwrap_or(MAX_EVIDENCE_BATCH_SIZE)
        .clamp(1, MAX_EVIDENCE_BATCH_SIZE);
    let mut requested = Vec::new();
    for id in request.property_ids {
        if requested.len() >= limit {
            break;
        }
        let canonical = canonical_property_id(&id).to_string();
        if !requested.iter().any(|existing| existing == &canonical) {
            requested.push(canonical);
        }
    }

    let properties = state.properties.read().await;
    let serving_bundle = state.serving_bundle.read().await.clone();
    let graph = state.knowledge.read().await;
    let mut results = Vec::new();
    let mut missing_property_ids = Vec::new();

    for property_id in requested {
        if let Some(property) = properties
            .iter()
            .find(|property| property.id == property_id)
        {
            results.push(build_property_evidence_response(
                &graph,
                property,
                serving_bundle.as_deref(),
            ));
        } else {
            missing_property_ids.push(property_id);
        }
    }

    Json(PropertyEvidenceBatchResponse {
        serving_bundle_version: serving_bundle
            .as_ref()
            .map(|bundle| bundle.manifest.bundle_version.clone()),
        results,
        missing_property_ids,
    })
}

/// GET /api/properties/:id — returns joined property + society + area,
/// enriched from the knowledge graph.
pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    let properties = state.properties.read().await;
    let property = find_property_by_request_id(&properties, &id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "property_not_found".to_string(),
                }),
            )
        })?;
    if !property.is_listable() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        ));
    }

    let serving_bundle = state.serving_bundle.read().await.clone();

    let graph = state.knowledge.read().await;

    // Enrich society from KG
    let society_key = super::enrichment::to_slug(&property.society_id);
    let mut society = state
        .societies
        .iter()
        .find(|s| super::enrichment::to_slug(&s.id) == society_key)
        .cloned();
    if let Some(ref mut soc) = society {
        enrich_society(soc, &graph);
    }

    let external_reviews = external_reviews_for(
        &property.society_id,
        society.as_ref(),
        &graph,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );

    // Enrich area from KG
    let area_key = area_lookup_key(&property.area_id);
    let mut area = state
        .areas
        .iter()
        .find(|a| area_lookup_key(&a.id) == area_key || area_lookup_key(&a.name) == area_key)
        .cloned();
    if let Some(ref mut ap) = area {
        enrich_area(ap, &graph);
    }

    // Compute market activity context from listing facts.
    let market_activity = scoring::compute_market_activity(&property, area.as_ref());

    // Hold a read lock on sellers — no clone needed, just borrow for the
    // duration of this request.
    let sellers_guard = state.sellers.read().await;

    // Find similar properties via local embedding similarity on the society node.
    let similar_properties = {
        let soc_node_id = society_node_id(&property.society_id);
        let similar_societies = graph.similar_to(&soc_node_id, 5, Some(NodeType::Society));
        let serving_facts = serving_bundle.as_ref().map(|bundle| &bundle.fact_index);

        let mut similar = Vec::new();
        for sim_soc in &similar_societies {
            if sim_soc.similarity < 0.3 {
                continue;
            }
            // Find one property from this society
            if let Some(prop) = properties
                .iter()
                .find(|p| society_node_id(&p.society_id) == sim_soc.node_id && p.id != property.id)
            {
                let card = enrich_property_card_with_sellers(
                    prop,
                    &state.societies,
                    &graph,
                    &sellers_guard,
                );
                similar.push(overlay_serving_google_reviews(
                    card,
                    &prop.society_id,
                    serving_facts,
                ));
                if similar.len() >= 4 {
                    break;
                }
            }
        }
        similar
    };

    // Extract RERA info from the society's KG node
    let rera = rera_info_for(
        &property.society_id,
        &graph,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );

    // Extract area intelligence from the area's KG node
    let area_intelligence = extract_area_intelligence(&graph, &property.area);

    // Compute transparency score
    let transparency_score = compute_transparency_score(&property, rera.as_ref());

    // Compute area price range from all properties in the same area
    let (area_price_range_low, area_price_range_high) = {
        let area_props: Vec<u64> = properties
            .iter()
            .filter(|p| p.area_id == property.area_id && p.price_per_sqft > 0)
            .map(|p| p.price_per_sqft)
            .collect();

        if area_props.len() >= 2 {
            let low = area_props.iter().copied().min();
            let high = area_props.iter().copied().max();
            (low, high)
        } else {
            (None, None)
        }
    };

    // Look up seller for this property.
    // Pick first verified seller, then highest completeness.
    let seller = {
        let mut matching: Vec<_> = sellers_guard
            .iter()
            .filter(|s| s.property_ids.contains(&property.id))
            .collect();
        matching.sort_by(|a, b| {
            b.verified
                .cmp(&a.verified)
                .then_with(|| b.completeness_pct().cmp(&a.completeness_pct()))
        });
        matching.first().map(|s| s.to_summary())
    };

    // Read interest count from JSONL file.
    let interest_count = {
        let file_path = state
            .project_root
            .join("data")
            .join("interests")
            .join(format!("{}.jsonl", property.id));
        count_interest_lines(&file_path).await
    };

    // Extract root_source, project_status, and project_status_display from society KG node
    let (root_source, project_status, project_status_display) = {
        let soc_node_id = society_node_id(&property.society_id);
        if let Some(node) = graph.get_node(&soc_node_id) {
            let rs = node.root_source.map(|r| r.as_str().to_string());
            // Get machine-readable project_status
            let ps = node
                .facts
                .iter()
                .filter(|f| f.key == "project_status")
                .max_by_key(|f| f.version)
                .and_then(|f| match &f.value {
                    crate::knowledge::FactValue::Text(s) => Some(s.clone()),
                    _ => None,
                });
            // Get display_template for project_status fact
            let ps_display = node
                .facts
                .iter()
                .filter(|f| f.key == "project_status")
                .max_by_key(|f| f.version)
                .and_then(|f| {
                    f.display_template.clone().map(|tmpl| {
                        if tmpl.contains("{value}") {
                            if let crate::knowledge::FactValue::Text(ref val) = f.value {
                                tmpl.replace("{value}", val)
                            } else {
                                tmpl
                            }
                        } else {
                            tmpl
                        }
                    })
                });
            (rs, ps, ps_display)
        } else {
            (None, None, None)
        }
    };
    let (project_status, project_status_display) =
        if let Some(serving_bundle) = serving_bundle.as_ref() {
            let projection =
                SocietyFactProjection::from_index(&serving_bundle.fact_index, &property.society_id)
                    .project_status(project_status, project_status_display);
            (projection.status, projection.display)
        } else {
            (project_status, project_status_display)
        };
    let home_state_display = serving_bundle.as_ref().and_then(|bundle| {
        SocietyFactProjection::from_index(&bundle.fact_index, &property.society_id)
            .project_home_state()
            .display
    });

    // Extract builder trust from KG
    let builder_trust = extract_builder_trust(&graph, &property.society_id);
    let builder_portfolio = build_builder_portfolio(&graph, &properties, &property);
    let entity_refs = kg_entity_refs_for_property(&property, &graph);
    let mut source_panels = build_source_panels(
        &graph,
        &property,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
        serving_bundle.as_ref().map(|bundle| &bundle.graph_index),
    );
    let community_pulse = source_panels
        .iter()
        .find(|panel| panel.kind == "community")
        .and_then(|panel| panel.community_pulse.as_ref());
    let society_projection = serving_bundle
        .as_ref()
        .map(|bundle| SocietyFactProjection::from_index(&bundle.fact_index, &property.society_id));
    let community_records = collect_community_evidence_records(
        &graph,
        &society_node_id(&property.society_id),
        Some(property.area.as_str()),
        society_projection.as_ref(),
    );
    let society_display_name = society
        .as_ref()
        .map(|society| society.name.as_str())
        .unwrap_or(property.society_id.as_str());
    let livability_brief = build_livability_brief(
        &graph,
        &property,
        society_display_name,
        society_projection.as_ref(),
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
        serving_bundle.as_ref().map(|bundle| &bundle.graph_index),
        &community_records,
        community_pulse,
    );
    if let Some(brief) = livability_brief.as_ref() {
        dedup_community_pulse_against_brief(&mut source_panels, brief);
    }
    let evidence = build_property_evidence_response_from_panels(
        property.id.clone(),
        entity_refs.clone(),
        serving_bundle.as_deref(),
        source_panels.clone(),
    );

    let area_median_ppsf = area
        .as_ref()
        .map(|profile| profile.median_price_per_sqft)
        .or_else(|| match (area_price_range_low, area_price_range_high) {
            (Some(low), Some(high)) => Some((low + high) / 2),
            (Some(low), None) => Some(low),
            (None, Some(high)) => Some(high),
            (None, None) => None,
        });
    let recommendation_branches = build_recommendation_branches(RecommendationBranchInputs {
        current: &property,
        current_evidence: &evidence,
        graph: &graph,
        properties: &properties,
        societies: &state.societies,
        sellers: &sellers_guard,
        serving_bundle: serving_bundle.as_deref(),
        area_median_ppsf,
    });

    // Extract data freshness from KG
    let data_freshness = extract_data_freshness(&graph, &property.society_id);

    // Compute confidence score for detail page (uses fact-quality instead of match_quality)
    let confidence_score = compute_confidence_for_detail(Some(&graph), &property.society_id);

    Ok(Json(PropertyDetail {
        entity_refs,
        evidence,
        property,
        society,
        area,
        market_activity,
        similar_properties,
        recommendation_branches,
        rera,
        area_intelligence,
        transparency_score,
        area_price_range_low,
        area_price_range_high,
        seller,
        interest_count,
        root_source,
        project_status_display,
        home_state_display,
        project_status,
        builder_trust,
        builder_portfolio,
        source_panels,
        data_freshness,
        confidence_score,
        external_reviews,
        livability_brief,
    }))
}

fn external_reviews_for(
    society_id: &str,
    society: Option<&crate::models::Society>,
    graph: &crate::knowledge::KnowledgeGraph,
    serving_facts: Option<&ServingFactIndex>,
) -> Option<ExternalReviews> {
    let node_id = society_node_id(society_id);
    let fallback = GoogleReviewEvidence {
        rating: super::enrichment::kg_numeric(graph, &node_id, "google_rating")
            .filter(|rating| (0.0..=5.0).contains(rating)),
        review_count: super::enrichment::kg_numeric(graph, &node_id, "google_review_count")
            .filter(|count| count.is_finite() && *count >= 0.0 && *count <= u32::MAX as f64)
            .map(|count| count.round() as u32),
        reviews_url: society.and_then(|society| society.google_reviews_url.clone()),
    };
    let evidence = serving_facts
        .map(|facts| {
            SocietyFactProjection::from_index(facts, society_id)
                .project_google_reviews(fallback.clone())
        })
        .unwrap_or(fallback);

    (!evidence.is_empty()).then_some(ExternalReviews {
        google_rating: evidence.rating,
        google_review_count: evidence.review_count,
        google_reviews_url: evidence.reviews_url,
    })
}

pub(crate) fn overlay_serving_google_reviews(
    mut card: PropertyCard,
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> PropertyCard {
    let Some(serving_facts) = serving_facts else {
        return card;
    };
    let fallback = GoogleReviewEvidence {
        rating: card.google_rating,
        review_count: card.google_review_count,
        reviews_url: card.google_reviews_url.clone(),
    };
    let evidence = SocietyFactProjection::from_index(serving_facts, society_id)
        .project_google_reviews(fallback);
    card.google_rating = evidence.rating;
    card.google_review_count = evidence.review_count;
    card.google_reviews_url = evidence.reviews_url;
    card.home_state_display = SocietyFactProjection::from_index(serving_facts, society_id)
        .project_home_state()
        .display;
    card
}

fn rera_info_for(
    society_id: &str,
    graph: &crate::knowledge::KnowledgeGraph,
    serving_facts: Option<&ServingFactIndex>,
) -> Option<ReraInfo> {
    let fallback = extract_rera_info(graph, society_id);
    let Some(serving_facts) = serving_facts else {
        return fallback;
    };
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    let has_serving_rera = projection.latest_bool("rera_registered").is_some()
        || projection.latest_text("rera_number").is_some();
    if !has_serving_rera {
        return fallback;
    }

    let mut info = fallback.unwrap_or_default();
    if let Some(fact) = projection.latest_bool("rera_registered") {
        info.registered = fact.value;
    }
    if let Some(fact) = projection.latest_text("rera_number") {
        info.registration_number = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_status") {
        info.status = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_completion_date") {
        info.completion_date = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_original_completion_date") {
        info.original_completion_date = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_delay_months") {
        info.delay_months = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_total_units") {
        info.total_units = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_total_land_area_sqm") {
        info.total_land_area_sqm = Some(fact.value);
        info.total_land_area_acres = Some(fact.value / 4_046.856_422_4);
    }
    if let Some(fact) = projection.latest_numeric("rera_total_project_cost") {
        info.total_project_cost_inr = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_land_cost") {
        info.land_cost_inr = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_construction_cost") {
        info.construction_cost_inr = Some(fact.value);
    }
    info.cost_per_unit_inr = match (info.total_project_cost_inr, info.total_units) {
        (Some(cost), Some(units)) if units > 0 => Some(cost / units as f64),
        _ => projection
            .latest_numeric("rera_cost_per_unit")
            .map(|fact| fact.value)
            .or(info.cost_per_unit_inr),
    };
    if let Some(fact) = projection.latest_numeric("rera_complaints_count") {
        info.complaints_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_complaints_resolved_pct") {
        info.complaints_resolved_pct = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_builder_projects_count") {
        info.builder_total_projects = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_builder_revocations") {
        info.builder_revocations = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_builder_states") {
        info.builder_states = fact
            .value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
    }
    if let Some(fact) = projection.latest_bool("rera_land_litigation") {
        info.land_litigation = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_escrow_bank") {
        info.escrow_bank = Some(fact.value);
    }
    if let Some(fact) = projection.latest_bool("rera_has_borrowing") {
        info.has_borrowing = Some(fact.value);
    }
    if let Some(fact) = projection.latest_bool("rera_has_mortgage") {
        info.has_mortgage = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_lat_lng") {
        info.lat_lng = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_portal_url") {
        info.rera_portal_url = Some(fact.value);
    }
    info.last_verified = projection
        .latest_learned_at_with_prefix("rera_")
        .map(|timestamp| timestamp.to_rfc3339())
        .or(info.last_verified);
    Some(info)
}

fn projected_i32(value: f64) -> Option<i32> {
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then(|| value.round() as i32)
}

/// Count non-empty lines in a JSONL file (interest events).
async fn count_interest_lines(path: &std::path::Path) -> usize {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents
            .lines()
            .filter(|l: &&str| !l.trim().is_empty())
            .count(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod serving_state_tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::knowledge::fact::{FactSource, SourceType};
    use crate::knowledge::node::{Node, NodeType};
    use crate::models::{Property, Society};
    use crate::routes::enrichment::enrich_property_card;
    use crate::serving::{ServingFactRecord, ServingSearchMetadataRecord};

    #[test]
    fn search_card_and_property_detail_project_the_same_current_review_facts() {
        let graph = legacy_graph();
        let property = property();
        let society = society();
        let serving = serving_index();

        let mut card = enrich_property_card(&property, std::slice::from_ref(&society), &graph);
        crate::search::text::enrich_card_from_serving_facts(
            &mut card,
            &serving,
            &property.society_id,
        );
        let detail =
            external_reviews_for(&property.society_id, Some(&society), &graph, Some(&serving))
                .expect("review evidence should be present");

        assert_eq!(detail.google_rating, card.google_rating);
        assert_eq!(detail.google_review_count, card.google_review_count);
        assert_eq!(detail.google_reviews_url, card.google_reviews_url);
        assert_eq!(detail.google_rating, Some(4.6));
        assert_eq!(detail.google_review_count, Some(431));
        assert_eq!(
            detail.google_reviews_url.as_deref(),
            Some("https://example.com/current")
        );
    }

    #[test]
    fn property_detail_keeps_legacy_review_fallback_without_serving_facts() {
        let graph = legacy_graph();
        let detail = external_reviews_for("sample", Some(&society()), &graph, None)
            .expect("legacy evidence should remain available");

        assert_eq!(detail.google_rating, Some(3.8));
        assert_eq!(detail.google_review_count, Some(87));
        assert_eq!(
            detail.google_reviews_url.as_deref(),
            Some("https://example.com/legacy")
        );
    }

    #[test]
    fn property_detail_projects_current_rera_facts_and_exposes_acreage() {
        let graph = crate::knowledge::KnowledgeGraph::new();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact("rera_registered", FactValue::Bool(true), 10),
                serving_fact(
                    "rera_number",
                    FactValue::Text("PRM-CURRENT".to_string()),
                    10,
                ),
                serving_fact("rera_total_units", FactValue::Numeric(1_520.0), 10),
                serving_fact(
                    "rera_total_land_area_sqm",
                    FactValue::Numeric(112_652.0),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let detail = rera_info_for("sample", &graph, Some(&serving))
            .expect("serving RERA facts should create detail evidence");

        assert!(detail.registered);
        assert_eq!(detail.registration_number.as_deref(), Some("PRM-CURRENT"));
        assert_eq!(detail.total_units, Some(1_520));
        assert_eq!(detail.total_land_area_sqm, Some(112_652.0));
        assert!((detail.total_land_area_acres.unwrap() - 27.8369).abs() < 0.001);
        assert_eq!(
            detail.last_verified.as_deref(),
            Some("1970-01-01T00:00:10+00:00")
        );
    }

    #[test]
    fn source_panels_project_current_serving_land_area_and_review_link() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "rera_total_land_area_sqm",
                    FactValue::Numeric(112_652.0),
                    10,
                ),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("https://example.com/current".to_string()),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels(&graph, &property, Some(&serving), None);
        let keys = panels
            .iter()
            .flat_map(|panel| panel.items.iter().map(|item| item.key.as_str()))
            .collect::<Vec<_>>();

        assert!(
            keys.contains(&"rera_total_land_area_sqm"),
            "RERA land area should be visible in detail source panels: {keys:?}"
        );
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("community pulse should exist when Google review facts are present");
        assert!(
            community.community_pulse.as_ref().is_some_and(|pulse| pulse
                .source_urls
                .iter()
                .any(|url| url == "https://example.com/current")),
            "Google review link should be visible in community pulse sources"
        );
    }

    #[test]
    fn source_panels_include_dynamic_nearby_items_when_backed_by_facts() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "nearby_schools",
                    FactValue::Text("Far School (4.0 km, 4.5 rating)".to_string()),
                    10,
                ),
                serving_fact_with_url(
                    "nearby_schools",
                    FactValue::Text("Greenwood High (1.2 km, 4.3 rating)".to_string()),
                    "https://maps.google.com/greenwood",
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels(&graph, &property, Some(&serving), None);
        let nearby = panels
            .iter()
            .find(|panel| panel.kind == "nearby")
            .expect("nearby panel should appear when map-backed nearby facts exist");

        assert_eq!(nearby.items[0].key, "nearby_schools");
        assert_eq!(
            nearby.items[0].values,
            vec![
                "Greenwood High (1.2 km, 4.3 rating)".to_string(),
                "Far School (4.0 km, 4.5 rating)".to_string(),
            ]
        );
        assert_eq!(
            nearby.items[0].attributions[0].source_url.as_deref(),
            Some("https://maps.google.com/greenwood")
        );
        assert!(nearby.missing.is_empty());
    }

    #[test]
    fn source_panels_merge_google_reviews_into_community_pulse() {
        let mut graph = legacy_graph();
        let node = graph
            .nodes
            .get_mut("society:sample")
            .expect("fixture society exists");
        node.add_fact(legacy_fact(
            "best_quote",
            FactValue::Text("Resident says: stale generated quote".to_string()),
        ));
        node.add_fact(legacy_fact(
            "google_top_positives",
            FactValue::Tags(vec!["stale generated review theme".to_string()]),
        ));
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact("google_rating", FactValue::Numeric(3.9), 10),
                serving_fact("google_review_count", FactValue::Numeric(392.0), 10),
                serving_fact(
                    "google_review_snippets",
                    FactValue::Tags(vec![
                        "Amenities and greenery are repeatedly praised.".to_string(),
                        "Traffic is still called out as a concern.".to_string(),
                    ]),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels(&graph, &property, Some(&serving), None);
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("community pulse should be backed by current summary and Google facts");
        let pulse = community
            .community_pulse
            .as_ref()
            .expect("community pulse should expose structured read model");

        assert_eq!(pulse.source_label, "Google review");
        assert_eq!(pulse.sentiment_band, "Mixed-positive");
        assert!(pulse.paragraph.contains("greenery"));
        assert!(pulse.paragraph.contains("amenities"));
        assert!(pulse.paragraph.contains("traffic"));
        assert!(!pulse.paragraph.contains("3.9"));
        assert!(!pulse.paragraph.contains("/5"));
        assert_eq!(pulse.positives, vec!["amenities", "greenery"]);
        assert_eq!(pulse.concerns, vec!["traffic"]);
        assert_eq!(pulse.quotes.len(), 2);
        assert!(community.items.is_empty());
        assert!(
            panels.iter().all(|panel| panel.kind != "reviews"),
            "Google reviews should be a source inside Community pulse, not a separate panel"
        );
        assert!(!community
            .missing
            .iter()
            .any(|item| item.contains("Review text is not ingested")));
    }

    #[test]
    fn source_panels_do_not_derive_approach_road_signal_from_review_snippets() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact(
                "google_review_snippets",
                FactValue::Tags(vec![
                    "Approach road is wide and clean near the main gate.".to_string(),
                    "Clubhouse and greenery are repeatedly praised.".to_string(),
                    "Access road has some road digging during peak hours.".to_string(),
                ]),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels(&graph, &property, Some(&serving), None);
        assert!(panels
            .iter()
            .filter(|panel| panel.kind == "approach_road")
            .flat_map(|panel| panel.items.iter())
            .all(|item| item.key != "approach_road_condition"));
    }

    #[test]
    fn source_panels_include_approach_road_fact_from_served_road_segment() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact_for_entity(
                "road_segment:sample-approach",
                "access_road_quality",
                FactValue::Text(
                    "Gate-side approach road identified; visual road-width verification is pending."
                        .to_string(),
                ),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );
        let graph_index =
            crate::graph::GraphIndex::from_serving_edges(&[crate::serving::ServingEdgeRecord {
                from_entity_id: "society:sample".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road_segment:sample-approach".to_string(),
                confidence: 0.82,
                source_type: "approach_road".to_string(),
            }]);

        let panels = build_source_panels(&graph, &property, Some(&serving), Some(&graph_index));
        let approach = panels
            .iter()
            .find(|panel| panel.kind == "approach_road")
            .expect("road-segment facts should produce an approach-road panel");
        let item = approach
            .items
            .iter()
            .find(|item| item.key == "access_road_quality")
            .expect("served road segment fact should be surfaced");

        assert_eq!(item.entity_id, "road_segment:sample-approach");
        assert_eq!(item.scope, "road_segment");
        assert_eq!(item.relationship.as_deref(), Some("approach road"));
        assert!(item.value.contains("Gate-side approach road"));
        assert!(
            !approach
                .items
                .iter()
                .any(|item| item.key == "approach_road_condition"),
            "road-segment facts should not require review-snippet phrase matching"
        );
    }

    #[test]
    fn source_panels_build_review_link_from_google_review_facts_without_explicit_url() {
        let graph = legacy_graph_without_review_url();
        let property = property();

        let panels = build_source_panels(&graph, &property, None, None);
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("Google review facts should produce a community pulse");
        let pulse = community
            .community_pulse
            .as_ref()
            .expect("community pulse should expose review source links");
        assert!(
            pulse.source_urls.iter().any(|url| {
                url == "https://www.google.com/maps/search/?api=1&query=Sample%20Society"
            }),
            "Google review facts should expose a navigable Maps search link: {:?}",
            pulse.source_urls
        );
    }

    #[test]
    fn property_evidence_sections_are_backend_shaped_for_dynamic_ui() {
        let mut graph = legacy_graph();
        let node = graph
            .nodes
            .get_mut("society:sample")
            .expect("fixture society exists");
        node.add_fact(legacy_fact(
            "rera_number",
            FactValue::Text("PRM-UI-CONTRACT".to_string()),
        ));
        node.add_fact(legacy_fact(
            "nearby_schools",
            FactValue::Tags(vec![
                "Greenwood High (1.2 km, 4.3 rating)".to_string(),
                "Inventure Academy (2.1 km, 4.1 rating)".to_string(),
            ]),
        ));

        let response = build_property_evidence_response(&graph, &property(), None);

        assert_eq!(response.property_id, "sample-3bhk");
        assert_eq!(response.entity_refs.society_entity_id, "society:sample");
        assert!(response.sections.len() >= 2);
        assert!(
            response
                .sections
                .windows(2)
                .all(|pair| pair[0].priority <= pair[1].priority),
            "evidence sections should be sorted by backend priority: {:?}",
            response
                .sections
                .iter()
                .map(|section| (&section.kind, section.priority))
                .collect::<Vec<_>>()
        );

        let rera = response
            .sections
            .iter()
            .find(|section| section.kind == "rera")
            .expect("RERA section should be produced when RERA facts exist");
        assert_eq!(rera.priority, 10);
        assert_eq!(rera.constellation, "trust");
        assert_eq!(rera.header_meta, "1 facts · Google");
        assert_eq!(rera.summary, "Registration: PRM-UI-CONTRACT");
        assert_eq!(rera.source_types, vec!["Google".to_string()]);
        assert_eq!(rera.entity_ids, vec!["society:sample".to_string()]);
        assert!(
            rera.items
                .iter()
                .all(|item| item.entity_id == "society:sample"),
            "items should carry graph drilldown IDs: {:?}",
            rera.items
        );

        let nearby = response
            .sections
            .iter()
            .find(|section| section.kind == "nearby")
            .expect("nearby section should be produced when map-backed facts exist");
        assert_eq!(
            nearby.summary,
            "Schools: Greenwood High (1.2 km, 4.3 rating) +1 more"
        );
        assert!(nearby.missing.is_empty());
    }

    #[test]
    fn property_evidence_includes_groundwater_potential_water_context() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![typed_serving_fact(
                "environment.groundwater_potential_class",
                FactValue::Text("Moderate".to_string()),
                "OpenCity",
                Some("https://data.opencity.in/example-groundwater.kml"),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            kg_entity_refs_for_property(&property, &graph),
            None,
            build_source_panels(&graph, &property, Some(&serving), None),
        );
        let section = response
            .sections
            .iter()
            .find(|section| section.kind == "water_context")
            .expect("groundwater fact should produce the water context section");

        assert_eq!(section.title, "Water context");
        assert_eq!(section.priority, 34);
        assert_eq!(
            section.summary,
            "Moderate groundwater potential zone near the society."
        );
        assert_eq!(section.source_types, vec!["OpenCity".to_string()]);
        assert_eq!(
            section.items[0].key,
            "environment.groundwater_potential_class"
        );
        assert_eq!(
            section.items[0].relationship.as_deref(),
            Some("surrounding water quality")
        );
    }

    #[test]
    fn property_evidence_finds_groundwater_on_matching_rera_society_entity() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact_for_entity(
                    "society:rera-sample",
                    "environment.groundwater_potential_class",
                    FactValue::Text("Moderate".to_string()),
                    10,
                ),
                serving_fact_for_entity(
                    "society:rera-sample",
                    "listing_society",
                    FactValue::Text("Sample Society".to_string()),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            kg_entity_refs_for_property(&property, &graph),
            None,
            build_source_panels(&graph, &property, Some(&serving), None),
        );
        let section = response
            .sections
            .iter()
            .find(|section| section.kind == "water_context")
            .expect("matching RERA society groundwater fact should produce water context");

        assert_eq!(
            section.summary,
            "Moderate groundwater potential zone near the society."
        );
        assert_eq!(section.items[0].entity_id, "society:rera-sample");
    }

    #[test]
    fn property_evidence_omits_missing_only_sections() {
        let graph = legacy_graph();
        let response = build_property_evidence_response(&graph, &property(), None);

        assert!(
            response.sections.iter().all(|section| {
                !section.items.is_empty()
                    || !section.media.is_empty()
                    || section.community_pulse.is_some()
            }),
            "missing-only sections should stay out of the user-facing evidence response"
        );
    }

    #[test]
    fn evidence_summaries_do_not_repeat_self_labeled_values() {
        let item = SourceItem {
            entity_id: "society:sample".to_string(),
            key: "listing_source_name_3bhk".to_string(),
            label: "3BHK source name".to_string(),
            value: "3BHK source name: MagicBricks".to_string(),
            scope: "society".to_string(),
            relationship: None,
            values: Vec::new(),
            source_type: "ExternalListing".to_string(),
            source_url: None,
            attributions: Vec::new(),
            confidence_pct: 90,
            learned_at: "2026-07-15T00:00:00Z".to_string(),
        };

        assert_eq!(source_item_summary(&item), "3BHK source name: MagicBricks");
    }

    #[test]
    fn market_trail_uses_configured_external_listing_facts() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                typed_serving_fact(
                    "listing_source_name_3bhk",
                    FactValue::Text("MagicBricks".to_string()),
                    "ExternalListing",
                    Some("https://www.magicbricks.com/example-green"),
                    11,
                ),
                typed_serving_fact(
                    "listing_price_range_3bhk",
                    FactValue::Text("INR 2.5 Cr - 3.5 Cr".to_string()),
                    "ExternalListing",
                    Some("https://www.magicbricks.com/example-green"),
                    11,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let market_panel = build_source_panels(&graph, &property, Some(&serving), None)
            .into_iter()
            .find(|panel| panel.kind == "market")
            .expect("Market trail should render when builder or listing facts exist");
        let market = evidence_section_from_panel(
            market_panel,
            &kg_entity_refs_for_property(&property, &graph),
        );

        assert_eq!(market.source_types, vec!["ExternalListing".to_string()]);
        assert!(market.items.iter().any(|item| {
            item.key == "listing_source_name_3bhk"
                && item.label == "3BHK source name"
                && item.value == "MagicBricks"
                && item.source_type == "ExternalListing"
        }));
    }

    fn legacy_graph() -> crate::knowledge::KnowledgeGraph {
        let mut graph = crate::knowledge::KnowledgeGraph::new();
        let mut node = Node::new("society:sample", NodeType::Society, "Sample Society");
        node.add_fact(legacy_fact("google_rating", FactValue::Numeric(3.8)));
        node.add_fact(legacy_fact("google_review_count", FactValue::Numeric(87.0)));
        node.add_fact(legacy_fact(
            "google_reviews_url",
            FactValue::Text("https://example.com/legacy".to_string()),
        ));
        graph.add_node(node);
        graph
    }

    fn legacy_graph_without_review_url() -> crate::knowledge::KnowledgeGraph {
        let mut graph = crate::knowledge::KnowledgeGraph::new();
        let mut node = Node::new("society:sample", NodeType::Society, "Sample Society");
        node.add_fact(legacy_fact(
            "google_sentiment",
            FactValue::Text("good".to_string()),
        ));
        graph.add_node(node);
        graph
    }

    fn legacy_fact(key: &str, value: FactValue) -> SourcedFact {
        SourcedFact {
            key: key.to_string(),
            value,
            confidence: 0.8,
            source: FactSource {
                source_type: SourceType::Google,
                url: None,
                model: None,
                skill_id: None,
                triggered_by: None,
            },
            learned_at: Utc.timestamp_opt(1, 0).unwrap(),
            version: 1,
            display_template: None,
            answers_preferences: Vec::new(),
            scoring_hint: None,
        }
    }

    fn serving_index() -> ServingFactIndex {
        ServingFactIndex::from_records(
            vec![
                serving_fact("google_rating", FactValue::Numeric(4.6), 2),
                serving_fact("google_review_count", FactValue::Numeric(431.0), 2),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("https://example.com/current".to_string()),
                    2,
                ),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("invalid".to_string()),
                    3,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        )
    }

    fn serving_fact(key: &str, value: FactValue, learned_at: i64) -> ServingFactRecord {
        serving_fact_with_optional_url(key, value, None, learned_at)
    }

    fn serving_fact_for_entity(
        entity_id: &str,
        key: &str,
        value: FactValue,
        learned_at: i64,
    ) -> ServingFactRecord {
        let mut fact = serving_fact(key, value, learned_at);
        fact.entity_id = entity_id.to_string();
        fact
    }

    fn serving_fact_with_url(
        key: &str,
        value: FactValue,
        source_url: &str,
        learned_at: i64,
    ) -> ServingFactRecord {
        serving_fact_with_optional_url(key, value, Some(source_url.to_string()), learned_at)
    }

    fn serving_fact_with_optional_url(
        key: &str,
        value: FactValue,
        source_url: Option<String>,
        learned_at: i64,
    ) -> ServingFactRecord {
        typed_serving_fact(
            key,
            value,
            if key.starts_with("rera_") {
                "Rera"
            } else {
                "Google"
            },
            source_url.as_deref(),
            learned_at,
        )
    }

    fn typed_serving_fact(
        key: &str,
        value: FactValue,
        source_type: &str,
        source_url: Option<&str>,
        learned_at: i64,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: "society:sample".to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: source_type.to_string(),
            source_url: source_url.map(str::to_string),
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(learned_at, 0).unwrap(),
        }
    }

    fn society() -> Society {
        Society {
            id: "sample".to_string(),
            name: "Sample Society".to_string(),
            area: "Whitefield".to_string(),
            city: "Bengaluru".to_string(),
            builder_name: "Sample Builder".to_string(),
            year_built: 2020,
            total_units: 100,
            summary: String::new(),
            maintenance_sentiment: String::new(),
            livability_sentiment: String::new(),
            common_positives: Vec::new(),
            common_complaints: Vec::new(),
            review_summary: String::new(),
            google_reviews_url: Some("https://example.com/legacy".to_string()),
            future_google_place_name: String::new(),
            future_google_place_id: None,
            future_review_enrichment_status: String::new(),
        }
    }

    fn property() -> Property {
        Property {
            id: "sample-3bhk".to_string(),
            title: "3 BHK in Sample Society".to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "sample".to_string(),
            builder_name: "Sample Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 20_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_500,
            super_builtup_sqft: 2_000,
            floor: 5,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready to move".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 8_000,
            society_quality_score: Some(0.8),
            builder_quality_score: Some(0.8),
            document_completeness_score: Some(0.8),
            litigation_risk: Some(0.1),
            noise_score: Some(0.2),
            sunlight_score: Some(0.8),
            airport_noise_score: Some(0.1),
            waterlogging_risk_score: Some(0.1),
            traffic_score: Some(0.4),
            days_on_market: 10,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
            seller_id: None,
        }
    }
}
