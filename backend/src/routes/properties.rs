use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::models::{PropertyCard, SellerSummary};
use crate::scoring::{
    self, compute_transparency_score, CompareThemes, MarketActivityResponse, TradeoffsResponse,
    TransparencyScore,
};
use crate::search::text::compute_confidence_for_detail;
use crate::search::ConfidenceScore;
use crate::serving::{GoogleReviewEvidence, ServingFactIndex, SocietyFactProjection};
use crate::state::AppState;

use crate::knowledge::node::NodeType;
use crate::knowledge::{FactValue, SourcedFact};

use super::enrichment::{
    enrich_area, enrich_property_card_with_sellers, enrich_society, extract_area_intelligence,
    extract_builder_trust, extract_data_freshness, extract_rera_info, society_node_id,
    AreaIntelligence, BuilderTrust, DataFreshness, ReraInfo,
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
    pub key: String,
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub confidence_pct: u8,
    pub learned_at: String,
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
    let fact = latest_fact(graph, node_id, key)?;
    let values = match &fact.value {
        FactValue::Tags(tags) => tags.clone(),
        _ => Vec::new(),
    };
    let value = match (&fact.value, key) {
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
        (FactValue::Tags(tags), "reddit_threads") => tags.join("\n"),
        _ => fact_display(fact),
    };
    if is_low_signal_source_value(key, &value) {
        return None;
    }
    Some(SourceItem {
        key: key.to_string(),
        label: label.to_string(),
        value,
        values,
        source_type: format!("{:?}", fact.source.source_type),
        source_url: fact.source.url.clone(),
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

fn build_source_panels(
    graph: &crate::knowledge::KnowledgeGraph,
    property: &crate::models::Property,
) -> Vec<SourcePanel> {
    let society_id = society_node_id(&property.society_id);
    let area_id = super::enrichment::area_node_id(&property.area);

    let mut panels = Vec::new();

    let rera_items = collect_source_items(
        graph,
        &society_id,
        &[
            ("rera_status", "Status"),
            ("rera_number", "Registration"),
            ("rera_completion_date", "Completion"),
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

    let market_items = collect_source_items(
        graph,
        &society_id,
        &[
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

    let area_items = collect_source_items(
        graph,
        &area_id,
        &[
            ("metro_details", "Metro access"),
            ("traffic_reality", "Traffic"),
            ("waterlogging_detail", "Waterlogging"),
            ("school_quality", "Schools"),
        ],
    );
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

    let reddit_items = collect_source_items(
        graph,
        &society_id,
        &[
            ("resident_sentiment", "Overall take"),
            ("sentiment_summary", "What forums point to"),
            ("best_quote", "Quote"),
            ("common_positives", "Repeated positives"),
            ("common_complaints", "Repeated concerns"),
        ],
    );
    panels.push(SourcePanel {
        kind: "community".to_string(),
        title: "Community pulse".to_string(),
        subtitle: "Forum chatter distilled into takeaways, quotes, and recurring issues."
            .to_string(),
        items: reddit_items,
        missing: vec![
            "Direct Reddit comment excerpts are not stored for every society yet.".to_string(),
            "Thread-level coverage still needs improving for low-mention projects.".to_string(),
        ],
    });

    let review_items = collect_source_items(
        graph,
        &society_id,
        &[
            ("google_sentiment", "Overall take"),
            ("google_top_positives", "Praised for"),
            ("google_top_negatives", "Recurring complaints"),
            ("google_common_themes", "Themes"),
        ],
    );
    panels.push(SourcePanel {
        kind: "reviews".to_string(),
        title: "Google reviews".to_string(),
        subtitle: "What public reviews consistently praise, complain about, and repeat."
            .to_string(),
        items: review_items,
        missing: vec![
            "Google review snippets are not stored for this society yet.".to_string(),
            "More verbatim review quotes still need extraction.".to_string(),
        ],
    });

    panels
        .into_iter()
        .filter(|panel| !panel.items.is_empty() || !panel.missing.is_empty())
        .collect()
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

/// GET /api/properties/:id — returns joined property + society + area,
/// enriched from the knowledge graph.
pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    let properties = state.properties.read().await;
    let canonical_id = canonical_property_id(&id);
    let property = properties
        .iter()
        .find(|p| p.id == canonical_id)
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
    let rera = extract_rera_info(&graph, &property.society_id);

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

    // Extract builder trust from KG
    let builder_trust = extract_builder_trust(&graph, &property.society_id);
    let builder_portfolio = build_builder_portfolio(&graph, &properties, &property);
    let source_panels = build_source_panels(&graph, &property);

    // Extract data freshness from KG
    let data_freshness = extract_data_freshness(&graph, &property.society_id);

    // Compute confidence score for detail page (uses fact-quality instead of match_quality)
    let confidence_score = compute_confidence_for_detail(Some(&graph), &property.society_id);

    Ok(Json(PropertyDetail {
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
        ServingFactRecord {
            entity_id: "society:sample".to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: "Google".to_string(),
            source_url: None,
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
