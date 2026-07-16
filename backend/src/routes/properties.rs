use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{KgEntityRefs, PropertyCard, SellerSummary};
use crate::scoring::{
    self, compute_transparency_score, CompareThemes, MarketActivityResponse, TradeoffsResponse,
    TransparencyScore,
};
use crate::search::text::compute_confidence_for_detail;
use crate::search::ConfidenceScore;
use crate::serving::{
    GoogleReviewEvidence, LoadedServingBundle, ServingFactIndex, SocietyFactProjection,
};
use crate::state::AppState;

use crate::knowledge::node::NodeType;
use crate::knowledge::{google_reviews_url_from_facts, FactValue, SourcedFact};

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

    let cards: Vec<PropertyCard> = properties
        .iter()
        .map(|p| enrich_property_card_with_sellers(p, &state.societies, &graph, &sellers))
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
    pub themes: CompareThemes,
    pub tradeoffs: TradeoffsResponse,
    pub market_activity: MarketActivityResponse,
    /// Similar properties from locally precomputed society embeddings.
    pub similar_properties: Vec<PropertyCard>,
    /// RERA regulatory data from the knowledge graph (None if not yet enriched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera: Option<ReraInfo>,
    /// Area intelligence from Reddit and other sources (None if not yet enriched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_intelligence: Option<AreaIntelligence>,
    /// Composite transparency score (0-100) with breakdown.
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
    /// Data confidence score — how trustworthy is this property's data?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<ConfidenceScore>,
    /// Current external review evidence projected from the Parquet serving bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_reviews: Option<ExternalReviews>,
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
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SourceItem {
    pub entity_id: String,
    pub key: String,
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<SourceAttribution>,
    pub confidence_pct: u8,
    pub learned_at: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SourceAttribution {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub source_type: String,
    pub confidence_pct: u8,
    pub learned_at: String,
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
    pub priority: u32,
    pub confidence_pct: u8,
    pub source_types: Vec<String>,
    pub entity_ids: Vec<String>,
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
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

fn collect_source_items(
    graph: &crate::knowledge::KnowledgeGraph,
    node_id: &str,
    keys: &[(&str, &str)],
) -> Vec<SourceItem> {
    keys.iter()
        .filter_map(|(key, label)| source_item(graph, node_id, key, label))
        .collect()
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
    let values = match &fact.value {
        FactValue::Tags(tags) => tags.clone(),
        _ => Vec::new(),
    };
    let raw_value = fact_value_display(&fact.value);
    let value = if fact_key == "google_review_snippets" && !values.is_empty() {
        format!("{} Google review highlights", values.len())
    } else {
        projection
            .search_metadata(fact_key)
            .and_then(|metadata| metadata.display_template.as_deref())
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
    values.truncate(5);
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

fn source_item_aliases(key: &str) -> &'static [&'static str] {
    match key {
        "market_project_status" => &["market_status"],
        _ => &[],
    }
}

fn build_source_panels(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
) -> Vec<SourcePanel> {
    let society_id = society_node_id(&property.society_id);
    let area_id = super::enrichment::area_node_id(&property.area);
    let projection =
        serving_facts.map(|facts| SocietyFactProjection::from_index(facts, &property.society_id));

    let mut panels = Vec::new();

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
            items: rera_items,
            missing: vec![],
        });
    }

    let market_items = collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("market_starting_price_inr", "Builder starting price"),
            ("market_bhk_options", "Configurations"),
            ("market_project_status", "Builder inventory status"),
            ("market_total_units", "Homes"),
            ("builder_reported_land_area_acres", "Builder project area"),
            ("official_project_url", "Official project page"),
            ("project_maps_url", "Project map"),
            ("listing_3bhk", "3BHK listing"),
            ("listing_price_3bhk", "3BHK listing price"),
            ("listing_price_range_3bhk", "3BHK listing price range"),
            ("listing_area_sqft_3bhk", "3BHK listing area"),
            ("listing_area_sqft_range_3bhk", "3BHK listing area range"),
            (
                "listing_price_per_sqft_range_3bhk",
                "3BHK listing rate range",
            ),
            ("listing_area_type_3bhk", "3BHK area basis"),
            ("listing_source_url_3bhk", "3BHK listing source"),
            ("pricing_3bhk", "3BHK pricing"),
            ("price_per_sqft", "Market rate"),
            ("price_appreciation", "Price movement"),
            ("comparable_projects", "Nearby comparables"),
        ],
    );
    panels.push(SourcePanel {
        kind: "market".to_string(),
        title: "Market trail".to_string(),
        subtitle: "Pricing, appreciation, and nearby comparable signals.".to_string(),
        items: market_items,
        missing: vec!["Registered resale transaction comps are not linked yet.".to_string()],
    });

    let mut area_items = collect_source_items(
        graph,
        &area_id,
        &[
            ("metro_details", "Metro access"),
            ("traffic_reality", "Traffic"),
            ("waterlogging_detail", "Waterlogging"),
            ("school_quality", "Schools"),
        ],
    );
    area_items.extend(collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("metro_status", "Metro access"),
            ("nearest_operational_metro_station", "Nearest station"),
            ("metro_distance_km", "Station distance"),
        ],
    ));
    panels.push(SourcePanel {
        kind: "area".to_string(),
        title: "Area trail".to_string(),
        subtitle: "Neighbourhood evidence around daily life.".to_string(),
        items: area_items,
        missing: vec![
            "Gate-level commute timings are not measured yet.".to_string(),
            "Upcoming infrastructure status needs a fresh source check.".to_string(),
        ],
    });

    let nearby_items = collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("nearby_schools", "Schools"),
            ("nearby_metro_stations", "Metro"),
            ("nearby_hospitals", "Hospitals"),
            ("nearby_fitness", "Cult / gyms"),
            ("nearby_eateries", "Eateries"),
            ("nearby_tech_parks", "Tech parks / offices"),
        ],
    );
    if !nearby_items.is_empty() {
        panels.push(SourcePanel {
            kind: "nearby".to_string(),
            title: "Nearby".to_string(),
            subtitle: "Map-backed places near the society.".to_string(),
            items: nearby_items,
            missing: vec![],
        });
    }

    let mut reddit_items = collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("community_review_summary", "What we know"),
            ("community_sentiment_score", "Signal score"),
            ("community_positive_themes", "Repeated positives"),
            ("community_concern_themes", "Repeated concerns"),
            ("community_review_highlights", "Review highlights"),
            ("community_evidence_links", "Evidence"),
        ],
    );
    if reddit_items.is_empty() {
        reddit_items.extend(collect_society_source_items(
            graph,
            &society_id,
            projection.as_ref(),
            &[
                ("resident_sentiment", "Overall take"),
                ("sentiment_summary", "What forums point to"),
                ("best_quote", "Quote"),
                ("common_positives", "Repeated positives"),
                ("common_complaints", "Repeated concerns"),
            ],
        ));
    }
    let has_theme_level_community_signal = reddit_items.iter().any(|item| {
        matches!(
            item.key.as_str(),
            "community_positive_themes" | "community_concern_themes"
        )
    });
    let mut community_missing =
        vec!["Direct Reddit comment excerpts are not stored for every society yet.".to_string()];
    if !has_theme_level_community_signal {
        community_missing
            .push("Review text is not ingested yet for every Google place.".to_string());
    }
    panels.push(SourcePanel {
        kind: "community".to_string(),
        title: "Community pulse".to_string(),
        subtitle: "Public review and resident-source signals, with gaps called out.".to_string(),
        items: reddit_items,
        missing: community_missing,
    });

    let mut review_items = collect_society_source_items(
        graph,
        &society_id,
        projection.as_ref(),
        &[
            ("google_rating", "Rating"),
            ("google_review_count", "Review count"),
            ("google_reviews_url", "Review link"),
            ("google_review_snippets", "Review highlights"),
        ],
    );
    if review_items.is_empty() {
        review_items.extend(collect_society_source_items(
            graph,
            &society_id,
            projection.as_ref(),
            &[
                ("google_sentiment", "Overall take"),
                ("google_top_positives", "Praised for"),
                ("google_top_negatives", "Recurring complaints"),
                ("google_common_themes", "Themes"),
            ],
        ));
    }
    let has_review_snippets = review_items
        .iter()
        .any(|item| item.key == "google_review_snippets" && !item.values.is_empty());
    let review_missing = if has_review_snippets {
        vec!["More verbatim review quotes still need extraction.".to_string()]
    } else {
        vec![
            "Google review snippets are not stored for this society yet.".to_string(),
            "More verbatim review quotes still need extraction.".to_string(),
        ]
    };
    panels.push(SourcePanel {
        kind: "reviews".to_string(),
        title: "Google reviews".to_string(),
        subtitle: "What public reviews consistently praise, complain about, and repeat."
            .to_string(),
        items: review_items,
        missing: review_missing,
    });

    panels
        .into_iter()
        .filter(|panel| !panel.items.is_empty() || !panel.missing.is_empty())
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

fn evidence_section_from_panel(panel: SourcePanel, entity_refs: &KgEntityRefs) -> EvidenceSection {
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
    let confidence_pct = section_confidence_pct(&panel.items);
    let summary = section_summary(&panel);

    EvidenceSection {
        priority: evidence_section_priority(&panel.kind),
        kind: panel.kind,
        title: panel.title,
        summary,
        subtitle: panel.subtitle,
        confidence_pct,
        source_types,
        entity_ids,
        items: panel.items,
        missing: panel.missing,
    }
}

fn evidence_section_priority(kind: &str) -> u32 {
    match kind {
        "rera" => 10,
        "market" => 20,
        "nearby" => 30,
        "reviews" => 40,
        "community" => 50,
        "area" => 60,
        _ => 100,
    }
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
        return source_item_summary(item);
    }
    panel
        .missing
        .first()
        .map(|gap| format!("Gap: {}", truncate_summary(gap, 96)))
        .unwrap_or_else(|| "No source-backed signals yet.".to_string())
}

fn primary_section_item<'a>(panel: &'a SourcePanel) -> Option<&'a SourceItem> {
    for key in primary_section_keys(&panel.kind) {
        if let Some(item) = panel.items.iter().find(|item| item.key == *key) {
            return Some(item);
        }
    }
    panel.items.first()
}

fn primary_section_keys(kind: &str) -> &'static [&'static str] {
    match kind {
        "rera" => &["rera_number", "rera_status", "rera_completion_date"],
        "market" => &[
            "listing_3bhk",
            "listing_price_3bhk",
            "market_project_status",
            "market_starting_price_inr",
            "official_project_url",
            "project_maps_url",
        ],
        "nearby" => &[
            "nearby_schools",
            "nearby_metro_stations",
            "nearby_hospitals",
            "nearby_fitness",
            "nearby_eateries",
            "nearby_tech_parks",
        ],
        "reviews" => &[
            "google_rating",
            "google_review_count",
            "google_review_snippets",
            "google_reviews_url",
        ],
        "community" => &[
            "community_review_summary",
            "community_review_highlights",
            "community_positive_themes",
            "community_concern_themes",
            "resident_sentiment",
        ],
        "area" => &[
            "metro_details",
            "traffic_reality",
            "waterlogging_detail",
            "metro_status",
        ],
        _ => &[],
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

fn fallback_section_entity_ids(kind: &str, entity_refs: &KgEntityRefs) -> Vec<String> {
    match kind {
        "area" => vec![entity_refs.area_entity_id.clone()],
        "rera" | "market" | "nearby" | "reviews" | "community" => {
            vec![entity_refs.society_entity_id.clone()]
        }
        _ => vec![entity_refs.property_entity_id.clone()],
    }
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

    // Compute themes, tradeoffs, market activity (KG-first scoring)
    let themes = scoring::compute_themes(&property, area.as_ref(), society.as_ref(), &graph);
    let tradeoffs = scoring::compute_tradeoffs(&property, area.as_ref(), society.as_ref(), &graph);
    let market_activity = scoring::compute_market_activity(&property, area.as_ref());

    // Hold a read lock on sellers — no clone needed, just borrow for the
    // duration of this request.
    let sellers_guard = state.sellers.read().await;

    // Find similar properties via local embedding similarity on the society node.
    let similar_properties = {
        let soc_node_id = society_node_id(&property.society_id);
        let similar_societies = graph.similar_to(&soc_node_id, 5, Some(NodeType::Society));

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
                similar.push(enrich_property_card_with_sellers(
                    prop,
                    &state.societies,
                    &graph,
                    &sellers_guard,
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

    // Extract builder trust from KG
    let builder_trust = extract_builder_trust(&graph, &property.society_id);
    let builder_portfolio = build_builder_portfolio(&graph, &properties, &property);
    let entity_refs = kg_entity_refs_for_property(&property, &graph);
    let source_panels = build_source_panels(
        &graph,
        &property,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );
    let evidence = build_property_evidence_response_from_panels(
        property.id.clone(),
        entity_refs.clone(),
        serving_bundle.as_deref(),
        source_panels.clone(),
    );

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
        themes,
        tradeoffs,
        market_activity,
        similar_properties,
        rera,
        area_intelligence,
        transparency_score,
        area_price_range_low,
        area_price_range_high,
        seller,
        interest_count,
        root_source,
        project_status_display,
        project_status,
        builder_trust,
        builder_portfolio,
        source_panels,
        data_freshness,
        confidence_score,
        external_reviews,
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

        let panels = build_source_panels(&graph, &property, Some(&serving));
        let keys = panels
            .iter()
            .flat_map(|panel| panel.items.iter().map(|item| item.key.as_str()))
            .collect::<Vec<_>>();

        assert!(
            keys.contains(&"rera_total_land_area_sqm"),
            "RERA land area should be visible in detail source panels: {keys:?}"
        );
        assert!(
            keys.contains(&"google_reviews_url"),
            "Google review link should be visible in detail source panels: {keys:?}"
        );
    }

    #[test]
    fn source_panels_project_legacy_market_status_as_canonical_inventory_status() {
        let graph = legacy_graph();
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact(
                "market_status",
                FactValue::Text("under_construction".to_string()),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels(&graph, &property, Some(&serving));
        let item = panels
            .iter()
            .flat_map(|panel| panel.items.iter())
            .find(|item| item.key == "market_project_status")
            .expect("legacy market_status should fill canonical market_project_status detail");

        assert_eq!(item.value, "under_construction");
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

        let panels = build_source_panels(&graph, &property, Some(&serving));
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
    fn source_panels_project_current_community_and_google_review_card_facts() {
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
                serving_fact(
                    "community_review_summary",
                    FactValue::Text("Google signal is mixed-positive: 3.9/5 from 392 reviews. Review text is not ingested yet.".to_string()),
                    10,
                ),
                serving_fact("community_sentiment_score", FactValue::Numeric(78.0), 10),
                serving_fact(
                    "community_positive_themes",
                    FactValue::Tags(vec!["greenery".to_string(), "amenities".to_string()]),
                    10,
                ),
                serving_fact(
                    "community_review_highlights",
                    FactValue::Tags(vec![
                        "Amenities and greenery are repeatedly praised.".to_string(),
                        "Traffic is still called out as a concern.".to_string(),
                    ]),
                    10,
                ),
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

        let panels = build_source_panels(&graph, &property, Some(&serving));
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("community card should be backed by current summary facts");
        let reviews = panels
            .iter()
            .find(|panel| panel.kind == "reviews")
            .expect("review card should be backed by current Google facts");
        let community_keys = community
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();
        let review_keys = reviews
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();

        assert!(community_keys.contains(&"community_review_summary"));
        assert!(community_keys.contains(&"community_sentiment_score"));
        assert!(community_keys.contains(&"community_positive_themes"));
        assert!(community_keys.contains(&"community_review_highlights"));
        assert!(review_keys.contains(&"google_rating"));
        assert!(review_keys.contains(&"google_review_count"));
        assert!(review_keys.contains(&"google_review_snippets"));
        let snippet_item = reviews
            .items
            .iter()
            .find(|item| item.key == "google_review_snippets")
            .expect("review snippets should be exposed as bullet-ready values");
        assert_eq!(snippet_item.value, "2 Google review highlights");
        assert_eq!(snippet_item.values.len(), 2);
        assert!(!community
            .missing
            .iter()
            .any(|item| item.contains("Review text is not ingested")));
        assert!(!reviews
            .missing
            .iter()
            .any(|item| item.contains("snippets are not stored")));
        assert!(!community_keys.contains(&"best_quote"));
        assert!(!review_keys.contains(&"google_top_positives"));
    }

    #[test]
    fn source_panels_build_review_link_from_google_review_facts_without_explicit_url() {
        let graph = legacy_graph_without_review_url();
        let property = property();

        let panels = build_source_panels(&graph, &property, None);
        let item = panels
            .iter()
            .flat_map(|panel| panel.items.iter())
            .find(|item| item.key == "google_reviews_url")
            .expect("Google review facts should expose a navigable Maps search link");

        assert_eq!(
            item.value,
            "https://www.google.com/maps/search/?api=1&query=Sample%20Society"
        );
        assert_eq!(item.source_type, "Google");
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
        assert_eq!(rera.summary, "Registration: PRM-UI-CONTRACT");
        assert_eq!(rera.confidence_pct, 80);
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
    fn property_evidence_keeps_missing_only_sections_with_entity_fallbacks() {
        let graph = legacy_graph();
        let response = build_property_evidence_response(&graph, &property(), None);
        let area = response
            .sections
            .iter()
            .find(|section| section.kind == "area")
            .expect("area gaps should be explicit even when facts are sparse");

        assert!(area.items.is_empty());
        assert_eq!(area.entity_ids, vec!["area:whitefield".to_string()]);
        assert!(area.summary.starts_with("Gap: "));
    }

    #[test]
    fn evidence_summaries_do_not_repeat_self_labeled_values() {
        let item = SourceItem {
            entity_id: "society:sample".to_string(),
            key: "market_project_status".to_string(),
            label: "Builder inventory status".to_string(),
            value: "Builder inventory status: Sold Out".to_string(),
            values: Vec::new(),
            source_type: "BuilderOfficial".to_string(),
            source_url: None,
            attributions: Vec::new(),
            confidence_pct: 90,
            learned_at: "2026-07-15T00:00:00Z".to_string(),
        };

        assert_eq!(
            source_item_summary(&item),
            "Builder inventory status: Sold Out"
        );
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
        ServingFactRecord {
            entity_id: "society:sample".to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: if key.starts_with("rera_") {
                "Rera".to_string()
            } else {
                "Google".to_string()
            },
            source_url,
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
            society_quality_score: 0.8,
            builder_quality_score: 0.8,
            document_completeness_score: 0.8,
            litigation_risk: 0.1,
            noise_score: 0.2,
            sunlight_score: 0.8,
            airport_noise_score: 0.1,
            waterlogging_risk_score: 0.1,
            traffic_score: 0.4,
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
