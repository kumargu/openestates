use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use crate::models::{PropertyCard, SellerSummary};
use crate::scoring::{
    self, CompareThemes, MarketActivityResponse, TradeoffsResponse, TransparencyScore,
    compute_transparency_score,
};
use crate::search::ConfidenceScore;
use crate::search::text::compute_confidence_for_detail;
use crate::state::AppState;

use crate::knowledge::node::NodeType;

use super::enrichment::{
    enrich_area, enrich_property_card_with_sellers, enrich_society, extract_area_intelligence,
    extract_builder_trust, extract_data_freshness, extract_rera_info, society_node_id,
    AreaIntelligence, BuilderTrust, DataFreshness, ReraInfo,
};

/// GET /api/properties — returns UI-ready property cards.
pub async fn list_properties(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PropertyCard>> {
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
    /// Similar properties from semantically related societies (embedding-based).
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
    /// Data freshness — how recently and richly the society data was updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_freshness: Option<DataFreshness>,
    /// Data confidence score — how trustworthy is this property's data?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<ConfidenceScore>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/properties/:id — returns joined property + society + area,
/// enriched from the knowledge graph.
pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    let properties = state.properties.read().await;
    let property = properties
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "property_not_found".to_string(),
                }),
            )
        })?;

    let graph = state.knowledge.read().await;

    // Enrich society from KG
    let mut society = state
        .societies
        .iter()
        .find(|s| s.id == property.society_id)
        .cloned();
    if let Some(ref mut soc) = society {
        enrich_society(soc, &graph);
    }

    // Enrich area from KG
    let mut area = state
        .areas
        .iter()
        .find(|a| a.id == property.area_id)
        .cloned();
    if let Some(ref mut ap) = area {
        enrich_area(ap, &graph);
    }

    // Compute themes, tradeoffs, market activity (KG-first scoring)
    let themes = scoring::compute_themes(
        &property,
        area.as_ref(),
        society.as_ref(),
        &graph,
    );
    let tradeoffs = scoring::compute_tradeoffs(
        &property,
        area.as_ref(),
        society.as_ref(),
        &graph,
    );
    let market_activity = scoring::compute_market_activity(&property, area.as_ref());

    // Hold a read lock on sellers — no clone needed, just borrow for the
    // duration of this request.
    let sellers_guard = state.sellers.read().await;

    // Find similar properties via embedding similarity on the society node
    let similar_properties = {
        let soc_node_id = society_node_id(&property.society_id);
        let similar_societies = graph.similar_to(&soc_node_id, 5, Some(NodeType::Society));

        let mut similar = Vec::new();
        for sim_soc in &similar_societies {
            if sim_soc.similarity < 0.3 {
                continue;
            }
            // Find one property from this society
            if let Some(prop) = properties.iter().find(|p| {
                society_node_id(&p.society_id) == sim_soc.node_id && p.id != property.id
            }) {
                similar.push(enrich_property_card_with_sellers(prop, &state.societies, &graph, &sellers_guard));
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
    let area_intelligence = extract_area_intelligence(&graph, &property.area_id);

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
            let ps = node.facts.iter()
                .filter(|f| f.key == "project_status")
                .max_by_key(|f| f.version)
                .and_then(|f| match &f.value {
                    crate::knowledge::FactValue::Text(s) => Some(s.clone()),
                    _ => None,
                });
            // Get display_template for project_status fact
            let ps_display = node.facts.iter()
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
        data_freshness,
        confidence_score,
    }))
}

/// Count non-empty lines in a JSONL file (interest events).
async fn count_interest_lines(path: &std::path::Path) -> usize {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents.lines().filter(|l: &&str| !l.trim().is_empty()).count(),
        Err(_) => 0,
    }
}
