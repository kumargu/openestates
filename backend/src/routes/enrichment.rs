//! Shared enrichment functions used by all routes that return property/society/area data.
//! Single source of truth: every route that returns these types calls these functions.

use serde::Serialize;

use crate::knowledge::{FactValue, KnowledgeGraph, SourcedFact};
use crate::models::{AreaProfile, Property, PropertyCard, Society};

// ---------------------------------------------------------------------------
// RERA and Area Intelligence response structs
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
pub struct ReraInfo {
    pub registered: bool,
    pub registration_number: Option<String>,
    pub status: Option<String>,
    pub completion_date: Option<String>,
    pub original_completion_date: Option<String>,
    pub delay_months: Option<i32>,
    pub total_units: Option<i32>,
    pub total_project_cost_inr: Option<f64>,
    pub land_cost_inr: Option<f64>,
    pub construction_cost_inr: Option<f64>,
    pub cost_per_unit_inr: Option<f64>,
    pub complaints_count: Option<i32>,
    pub complaints_resolved_pct: Option<f64>,
    pub builder_total_projects: Option<i32>,
    pub builder_revocations: Option<i32>,
    pub builder_states: Vec<String>,
    pub land_litigation: Option<bool>,
    pub escrow_bank: Option<String>,
    pub has_borrowing: Option<bool>,
    pub has_mortgage: Option<bool>,
    pub lat_lng: Option<String>,
    pub rera_portal_url: Option<String>,
    pub last_verified: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct AreaIntelligence {
    pub safety: Option<String>,
    pub commute_reality: Option<String>,
    pub water_supply: Option<String>,
    pub noise_level: Option<String>,
    pub green_cover: Option<String>,
    pub community_vibe: Option<String>,
    pub walkability: Option<String>,
    pub school_quality: Option<String>,
    pub grocery_shopping: Option<String>,
    pub healthcare_access: Option<String>,
    pub recurring_complaints: Vec<String>,
    pub hidden_gems: Vec<String>,
    pub deal_breakers: Vec<String>,
    pub overall_sentiment: Option<String>,
    pub source_count: Option<i32>,
    pub last_updated: Option<String>,
}

// ---------------------------------------------------------------------------
// Fact extraction helpers — work on a node's facts slice
// ---------------------------------------------------------------------------

fn get_text_fact(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts.iter().filter(|f| f.key == key).max_by_key(|f| f.version).and_then(|f| match &f.value {
        FactValue::Text(s) => Some(s.clone()),
        _ => None,
    })
}

fn get_numeric_fact(facts: &[SourcedFact], key: &str) -> Option<f64> {
    facts.iter().filter(|f| f.key == key).max_by_key(|f| f.version).and_then(|f| match &f.value {
        FactValue::Numeric(n) => Some(*n),
        _ => None,
    })
}

fn get_bool_fact(facts: &[SourcedFact], key: &str) -> Option<bool> {
    facts.iter().filter(|f| f.key == key).max_by_key(|f| f.version).and_then(|f| match &f.value {
        FactValue::Bool(b) => Some(*b),
        _ => None,
    })
}

fn get_tags_fact(facts: &[SourcedFact], key: &str) -> Vec<String> {
    facts.iter().filter(|f| f.key == key).max_by_key(|f| f.version).and_then(|f| match &f.value {
        FactValue::Tags(tags) => Some(tags.clone()),
        _ => None,
    }).unwrap_or_default()
}

/// Get the learned_at timestamp from any fact matching the key, formatted as ISO string.
fn get_fact_timestamp(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts.iter().filter(|f| f.key == key).max_by_key(|f| f.version).map(|f| f.learned_at.to_rfc3339())
}

// ---------------------------------------------------------------------------
// RERA extraction — reads rera_* facts from a society KG node
// ---------------------------------------------------------------------------

/// Extract RERA information from the knowledge graph for a given society.
/// Returns None if no "rera_registered" fact exists (meaning no RERA data has been ingested).
pub fn extract_rera_info(graph: &KnowledgeGraph, society_id: &str) -> Option<ReraInfo> {
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;
    let facts = &node.facts;

    // Only return ReraInfo if we have RERA data
    let has_rera = facts.iter().any(|f| f.key == "rera_registered");
    if !has_rera {
        return None;
    }

    let registered = get_bool_fact(facts, "rera_registered").unwrap_or(false);

    let total_units = get_numeric_fact(facts, "rera_total_units").map(|n| n as i32);

    // Get last_verified from the rera_registered fact's learned_at timestamp
    let last_verified = get_fact_timestamp(facts, "rera_registered");

    // Map fact keys: try both Python skill naming conventions
    // Skills use: rera_number, rera_total_project_cost, rera_land_cost, rera_construction_cost
    // Also try: rera_registration_number, rera_total_project_cost_inr (alt naming)
    let registration_number = get_text_fact(facts, "rera_number")
        .or_else(|| get_text_fact(facts, "rera_registration_number"));
    let total_cost_val = get_numeric_fact(facts, "rera_total_project_cost")
        .or_else(|| get_numeric_fact(facts, "rera_total_project_cost_inr"));
    let land_cost = get_numeric_fact(facts, "rera_land_cost")
        .or_else(|| get_numeric_fact(facts, "rera_land_cost_inr"));
    let construction_cost = get_numeric_fact(facts, "rera_construction_cost")
        .or_else(|| get_numeric_fact(facts, "rera_construction_cost_inr"));
    let builder_projects = get_numeric_fact(facts, "rera_builder_projects_count")
        .or_else(|| get_numeric_fact(facts, "rera_builder_total_projects"))
        .map(|n| n as i32);

    // Recompute cost_per_unit with the correct total_cost
    let cost_per_unit = match (total_cost_val, total_units) {
        (Some(cost), Some(units)) if units > 0 => Some(cost / units as f64),
        _ => get_numeric_fact(facts, "rera_cost_per_unit"),
    };

    // builder_states might be stored as Text (comma-separated) instead of Tags
    let builder_states = {
        let tags = get_tags_fact(facts, "rera_builder_states");
        if tags.is_empty() {
            // Try text version (comma-separated)
            get_text_fact(facts, "rera_builder_states")
                .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                .unwrap_or_default()
        } else {
            tags
        }
    };

    Some(ReraInfo {
        registered,
        registration_number,
        status: get_text_fact(facts, "rera_status"),
        completion_date: get_text_fact(facts, "rera_completion_date"),
        original_completion_date: get_text_fact(facts, "rera_original_completion_date"),
        delay_months: get_numeric_fact(facts, "rera_delay_months").map(|n| n as i32),
        total_units,
        total_project_cost_inr: total_cost_val,
        land_cost_inr: land_cost,
        construction_cost_inr: construction_cost,
        cost_per_unit_inr: cost_per_unit,
        complaints_count: get_numeric_fact(facts, "rera_complaints_count").map(|n| n as i32),
        complaints_resolved_pct: get_numeric_fact(facts, "rera_complaints_resolved_pct"),
        builder_total_projects: builder_projects,
        builder_revocations: get_numeric_fact(facts, "rera_builder_revocations").map(|n| n as i32),
        builder_states,
        land_litigation: get_bool_fact(facts, "rera_land_litigation"),
        escrow_bank: get_text_fact(facts, "rera_escrow_bank"),
        has_borrowing: get_bool_fact(facts, "rera_has_borrowing"),
        has_mortgage: get_bool_fact(facts, "rera_has_mortgage"),
        lat_lng: get_text_fact(facts, "rera_lat_lng"),
        rera_portal_url: get_text_fact(facts, "rera_portal_url"),
        last_verified,
    })
}

// ---------------------------------------------------------------------------
// Area Intelligence extraction — reads area intelligence facts from KG
// ---------------------------------------------------------------------------

/// Extract area intelligence from the knowledge graph for a given area.
/// Returns None if no Reddit-sourced area intelligence facts exist.
pub fn extract_area_intelligence(graph: &KnowledgeGraph, area_id: &str) -> Option<AreaIntelligence> {
    let node_id = area_node_id(area_id);
    let node = graph.get_node(&node_id)?;
    let facts = &node.facts;

    // Check if we have any area intelligence facts (Reddit-sourced or LLM-sourced)
    let intelligence_keys = [
        "safety", "commute_reality", "water_supply", "noise_level",
        "green_cover", "community_vibe", "walkability", "school_quality",
        "grocery_shopping", "healthcare_access", "recurring_complaints",
        "hidden_gems", "deal_breakers", "overall_sentiment",
    ];
    let has_intelligence = facts.iter().any(|f| intelligence_keys.contains(&f.key.as_str()));
    if !has_intelligence {
        return None;
    }

    // Count source threads (look for source_count fact or count Reddit-sourced facts)
    let source_count = get_numeric_fact(facts, "source_count").map(|n| n as i32);
    let last_updated = get_fact_timestamp(facts, "safety")
        .or_else(|| get_fact_timestamp(facts, "overall_sentiment"));

    Some(AreaIntelligence {
        safety: get_text_fact(facts, "safety"),
        commute_reality: get_text_fact(facts, "commute_reality"),
        water_supply: get_text_fact(facts, "water_supply"),
        noise_level: get_text_fact(facts, "noise_level"),
        green_cover: get_text_fact(facts, "green_cover"),
        community_vibe: get_text_fact(facts, "community_vibe"),
        walkability: get_text_fact(facts, "walkability"),
        school_quality: get_text_fact(facts, "school_quality"),
        grocery_shopping: get_text_fact(facts, "grocery_shopping"),
        healthcare_access: get_text_fact(facts, "healthcare_access"),
        recurring_complaints: get_tags_fact(facts, "recurring_complaints"),
        hidden_gems: get_tags_fact(facts, "hidden_gems"),
        deal_breakers: get_tags_fact(facts, "deal_breakers"),
        overall_sentiment: get_text_fact(facts, "overall_sentiment"),
        source_count,
        last_updated,
    })
}

// ---------------------------------------------------------------------------
// Slug normalization — single canonical implementation
// ---------------------------------------------------------------------------

/// Canonical slug: lowercase, hyphens, no "soc-" prefix.
pub fn to_slug(id: &str) -> String {
    let s = id.to_lowercase().replace(['_', ' '], "-");
    s.strip_prefix("soc-")
        .unwrap_or(&s)
        .to_string()
}

/// Build a society node ID for KG lookup.
pub fn society_node_id(society_id: &str) -> String {
    format!("society:{}", to_slug(society_id))
}

/// Build an area node ID for KG lookup.
pub fn area_node_id(area_name: &str) -> String {
    format!("area:{}", to_slug(area_name))
}

// ---------------------------------------------------------------------------
// KG fact extraction helpers
// ---------------------------------------------------------------------------

pub fn kg_numeric(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<f64> {
    let node = graph.get_node(node_id)?;
    node.facts.iter().find(|f| f.key == key).and_then(|f| match &f.value {
        FactValue::Numeric(n) => Some(*n),
        _ => None,
    })
}

pub fn kg_text(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<String> {
    let node = graph.get_node(node_id)?;
    node.facts.iter().find(|f| f.key == key).and_then(|f| match &f.value {
        FactValue::Text(s) => Some(s.clone()),
        _ => None,
    })
}

pub fn kg_tags(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<Vec<String>> {
    let node = graph.get_node(node_id)?;
    node.facts.iter().find(|f| f.key == key).and_then(|f| match &f.value {
        FactValue::Tags(tags) => Some(tags.clone()),
        _ => None,
    })
}

fn is_placeholder(s: &str) -> bool {
    s.is_empty()
        || s.starts_with("Not yet enriched")
        || s.contains("Needs enrichment")
        || s.starts_with("Area discovered")
        || s.starts_with("Society discovered")
}

// ---------------------------------------------------------------------------
// Property card enrichment — used by /properties, /search, /properties/:id
// ---------------------------------------------------------------------------

/// Enrich a Property into a PropertyCard with KG data.
/// This is THE function — every route that returns a PropertyCard must use it.
pub fn enrich_property_card(
    p: &Property,
    societies: &[Society],
    graph: &KnowledgeGraph,
) -> PropertyCard {
    let society_name = societies
        .iter()
        .find(|s| s.id == p.society_id)
        .map(|s| s.name.clone())
        .unwrap_or_default();

    let node_id = society_node_id(&p.society_id);

    let google_rating = kg_numeric(graph, &node_id, "google_rating");
    let google_review_count = kg_numeric(graph, &node_id, "google_review_count")
        .map(|n| n as u32);

    // Use photo_url from KG if property has no hero_image
    let hero_image = if p.hero_image.is_empty() {
        kg_text(graph, &node_id, "photo_url").unwrap_or_default()
    } else {
        p.hero_image.clone()
    };

    PropertyCard {
        id: p.id.clone(),
        title: p.title.clone(),
        area: p.area.clone(),
        price: p.price,
        price_per_sqft: p.price_per_sqft,
        bhk: p.bhk,
        sqft: p.carpet_area_sqft,
        society_name,
        builder_name: p.builder_name.clone(),
        hero_image,
        transparency_tags: p.transparency_tags.iter().take(3).cloned().collect(),
        description_summary: p.description_summary.clone(),
        possession_status: p.possession_status.clone(),
        metro_distance_mins: p.metro_distance_mins,
        floor: p.floor,
        total_floors: p.total_floors,
        facing: p.facing.clone(),
        google_rating,
        google_review_count,
        seller_id: p.seller_id.clone(),
    }
}

// ---------------------------------------------------------------------------
// Society enrichment — overlays KG facts onto seed data
// ---------------------------------------------------------------------------

/// Enrich a Society with knowledge graph facts. Mutates in place.
pub fn enrich_society(society: &mut Society, graph: &KnowledgeGraph) {
    let node_id = society_node_id(&society.id);

    if graph.get_node(&node_id).is_none() {
        return;
    }

    if is_placeholder(&society.review_summary) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.review_summary = val;
        }
    }

    if is_placeholder(&society.maintenance_sentiment) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.maintenance_sentiment = if val.to_lowercase().contains("maintenance") {
                val
            } else {
                "See reviews".to_string()
            };
        }
    }

    if is_placeholder(&society.livability_sentiment) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.livability_sentiment = val.chars().take(120).collect();
        }
    }

    if society.common_positives.is_empty() {
        if let Some(tags) = kg_tags(graph, &node_id, "google_top_positives") {
            society.common_positives = tags;
        }
    }

    if society.common_complaints.is_empty() {
        if let Some(tags) = kg_tags(graph, &node_id, "google_top_negatives") {
            society.common_complaints = tags;
        }
    }
}

// ---------------------------------------------------------------------------
// Area enrichment — overlays KG facts onto seed data
// ---------------------------------------------------------------------------

/// Enrich an AreaProfile with knowledge graph facts. Mutates in place.
pub fn enrich_area(area: &mut AreaProfile, graph: &KnowledgeGraph) {
    let node_id = area_node_id(&area.name);

    if graph.get_node(&node_id).is_none() {
        return;
    }

    let set_if_placeholder = |field: &mut String, key: &str| {
        if is_placeholder(field) {
            if let Some(val) = kg_text(graph, &node_id, key) {
                *field = val;
            }
        }
    };

    set_if_placeholder(&mut area.metro_access_summary, "metro_details");
    set_if_placeholder(&mut area.traffic_summary, "traffic_reality");
    set_if_placeholder(&mut area.waterlogging_summary, "waterlogging_detail");
    set_if_placeholder(&mut area.livability_summary, "livability_summary");
    set_if_placeholder(&mut area.community_notes, "area_vibe");

    // Trend
    if is_placeholder(&area.trend_summary) {
        if let Some(trend) = kg_text(graph, &node_id, "price_trend") {
            area.trend_summary = format!("Price trend: {}", trend);
            area.trend_direction = trend;
        }
    }

    // Tags
    if area.externality_tags.is_empty() {
        let mut tags = Vec::new();
        if let Some(wl) = kg_text(graph, &node_id, "waterlogging_risk") {
            tags.push(format!("Waterlogging: {}", wl));
        }
        if let Some(m) = kg_text(graph, &node_id, "metro_status") {
            tags.push(format!("Metro: {}", m));
        }
        if let Some(p) = kg_text(graph, &node_id, "price_trend") {
            tags.push(format!("Price: {}", p));
        }
        if !tags.is_empty() {
            area.externality_tags = tags;
        }
    }

    if area.infrastructure_tags.is_empty() {
        let mut tags = Vec::new();
        if let Some(infra) = kg_text(graph, &node_id, "upcoming_infra") {
            let truncated: String = infra.chars().take(80).collect();
            tags.push(truncated);
        }
        if let Some(schools) = kg_tags(graph, &node_id, "school_quality") {
            for s in schools.into_iter().take(3) {
                tags.push(format!("School: {}", s));
            }
        }
        if !tags.is_empty() {
            area.infrastructure_tags = tags;
        }
    }
}
