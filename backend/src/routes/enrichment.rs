//! Shared enrichment functions used by all routes that return property/society/area data.
//! Single source of truth: every route that returns these types calls these functions.

use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::models::{AreaProfile, Property, PropertyCard, Society};

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
