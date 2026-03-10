//! Ingestion — converts discovered properties into the system's data structures.
//!
//! Discovered properties become:
//! 1. Property structs (same shape as seed data) added to in-memory list
//! 2. Knowledge graph nodes with source_type: Google, confidence: 0.6
//! 3. Persisted to data/seed/properties.json for restart survival

use chrono::Utc;

use crate::knowledge::edge::{Edge, Relation};
use crate::knowledge::fact::{FactSource, FactValue, SourceType, SourcedFact};
use crate::knowledge::graph::KnowledgeGraph;
use crate::knowledge::node::{Node, NodeType};
use crate::models::Property;

use super::gemini::{slugify, DiscoveredProperty};

/// Convert discovered properties into Property structs and ingest into the knowledge graph.
/// Returns the new Property entries (to be added to the in-memory property list).
pub fn ingest_discoveries(
    discoveries: &[DiscoveredProperty],
    graph: &mut KnowledgeGraph,
    area_canonical: &str,
    triggered_by: &str,
) -> Vec<Property> {
    let mut new_properties = Vec::new();

    for disc in discoveries {
        let slug = slugify(&disc.project_name);
        let area = if disc.area.is_empty() {
            area_canonical.to_string()
        } else {
            disc.area.clone()
        };
        let area_slug = slugify(&area);

        // Generate property entries for each BHK config
        let configs: Vec<u32> = disc
            .configurations
            .iter()
            .filter_map(|c| {
                c.chars()
                    .find(|ch| ch.is_ascii_digit())
                    .and_then(|d| d.to_digit(10))
            })
            .collect();

        // If no configs parsed, create a single entry with bhk=0
        let bhk_list = if configs.is_empty() { vec![0] } else { configs };

        for &bhk in &bhk_list {
            let prop_id = if bhk > 0 {
                format!("discovered-{}-{}bhk", slug, bhk)
            } else {
                format!("discovered-{}", slug)
            };

            // Skip if we already have this property
            if graph.get_node(&format!("property:{}", prop_id)).is_some() {
                continue;
            }

            // Compute price from range
            let price = disc
                .price_range_max_lakhs
                .or(disc.price_range_min_lakhs)
                .map(|l| (l * 1_00_000.0) as u64)
                .unwrap_or(0);

            let price_per_sqft = disc.price_per_sqft_approx.unwrap_or(0);

            let society_id = format!("soc-{}", slug);

            let possession = disc
                .possession_status
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            let mut tags = vec![
                "Discovered via Search".to_string(),
                "Verification Pending".to_string(),
            ];
            if disc.rera_number.is_some() {
                tags.push("RERA Number Available".to_string());
            }
            match possession.as_str() {
                "ready_to_move" => tags.push("Ready to Move".to_string()),
                "under_construction" => tags.push("Under Construction".to_string()),
                "new_launch" => tags.push("New Launch".to_string()),
                _ => {}
            }

            let description = disc
                .key_highlights
                .first()
                .cloned()
                .unwrap_or_else(|| {
                    format!(
                        "{} by {} in {}",
                        disc.project_name, disc.builder_name, area
                    )
                });

            let prop = Property {
                id: prop_id.clone(),
                title: if bhk > 0 {
                    format!("{} BHK in {}", bhk, disc.project_name)
                } else {
                    disc.project_name.clone()
                },
                area: area.clone(),
                area_id: format!("area-{}", area_slug),
                city: "Bengaluru".to_string(),
                society_id: society_id.clone(),
                builder_name: disc.builder_name.clone(),
                property_type: "Apartment".to_string(),
                listing_type: "Resale".to_string(),
                bhk,
                price,
                price_per_sqft,
                carpet_area_sqft: 0,
                super_builtup_sqft: 0,
                floor: 0,
                total_floors: disc.total_floors.unwrap_or(0),
                facing: "Not specified".to_string(),
                possession_status: possession,
                metro_distance_mins: 0,
                maintenance_cost_monthly: 0,
                society_quality_score: 0.5,
                builder_quality_score: 0.5,
                document_completeness_score: 0.5,
                litigation_risk: 0.1,
                noise_score: 0.5,
                sunlight_score: 0.5,
                airport_noise_score: 0.1,
                waterlogging_risk_score: 0.2,
                traffic_score: 0.5,
                days_on_market: 0,
                greenery_score: None,
                open_space_score: None,
                resale_strength_score: None,
                interest_level: None,
                saves_last_7d: None,
                offers_last_7d: None,
                images: Vec::new(),
                hero_image: String::new(),
                description_summary: description,
                transparency_tags: tags,
                source_reference: disc
                    .source_url
                    .clone()
                    .unwrap_or_else(|| "Gemini Discovery".to_string()),
            };

            new_properties.push(prop);
        }

        // --- Knowledge graph ingestion ---
        let source = FactSource {
            source_type: SourceType::Google,
            url: disc.source_url.clone(),
            model: Some("gemini-2.5-flash".to_string()),
            skill_id: Some("live_discovery".to_string()),
            triggered_by: Some(triggered_by.to_string()),
        };

        // Create/update society node
        let society_node_id = format!("society:{}", slug);
        if graph.get_node(&society_node_id).is_none() {
            let mut node = Node::new(
                society_node_id.clone(),
                NodeType::Society,
                disc.society_name
                    .as_deref()
                    .unwrap_or(&disc.project_name),
            );

            let mut facts = Vec::new();
            facts.push(SourcedFact {
                key: "builder_name".to_string(),
                value: FactValue::Text(disc.builder_name.clone()),
                confidence: 0.7,
                source: source.clone(),
                learned_at: Utc::now(),
                version: 1,
                display_template: Some("Built by {value}".to_string()),
                answers_preferences: vec!["builder".to_string()],
                scoring_hint: None,
            });

            if let Some(ref configs) = Some(&disc.configurations) {
                if !configs.is_empty() {
                    facts.push(SourcedFact {
                        key: "configurations".to_string(),
                        value: FactValue::Tags(configs.to_vec()),
                        confidence: 0.6,
                        source: source.clone(),
                        learned_at: Utc::now(),
                        version: 1,
                        display_template: Some("Configurations: {value}".to_string()),
                        answers_preferences: Vec::new(),
                        scoring_hint: None,
                    });
                }
            }

            if let (Some(min), Some(max)) = (disc.price_range_min_lakhs, disc.price_range_max_lakhs) {
                facts.push(SourcedFact {
                    key: "price_range".to_string(),
                    value: FactValue::Text(format!("₹{:.0}L - ₹{:.0}L", min, max)),
                    confidence: 0.6,
                    source: source.clone(),
                    learned_at: Utc::now(),
                    version: 1,
                    display_template: Some("Price: {value}".to_string()),
                    answers_preferences: vec!["value for money".to_string(), "budget".to_string()],
                    scoring_hint: None,
                });
            }

            if let Some(ref possession) = disc.possession_status {
                facts.push(SourcedFact {
                    key: "possession_status".to_string(),
                    value: FactValue::Text(possession.clone()),
                    confidence: 0.7,
                    source: source.clone(),
                    learned_at: Utc::now(),
                    version: 1,
                    display_template: Some("Possession: {value}".to_string()),
                    answers_preferences: vec![
                        "ready to move".to_string(),
                        "new construction".to_string(),
                    ],
                    scoring_hint: None,
                });
            }

            node.add_facts(facts);
            graph.add_node(node);

            // Edge: society → area
            let area_node_id = format!("area:{}", area_slug);
            let mut edge = Edge::new(
                society_node_id.clone(),
                area_node_id,
                Relation::SocietyInArea,
            );
            edge.source = source.clone();
            graph.add_edge(edge);

            // Edge: society → builder
            let builder_slug = slugify(&disc.builder_name);
            let builder_node_id = format!("builder:{}", builder_slug);
            let mut edge = Edge::new(
                society_node_id.clone(),
                builder_node_id,
                Relation::BuiltBy,
            );
            edge.source = source.clone();
            graph.add_edge(edge);
        }

        // Queue enrichment for the discovered society
        graph.queue_enrichment(
            format!("society:{}", slug),
            "learn_society".to_string(),
            triggered_by.to_string(),
        );
        graph.queue_enrichment(
            format!("society:{}", slug),
            "fetch_google_reviews".to_string(),
            triggered_by.to_string(),
        );
    }

    new_properties
}

/// Persist new properties to the seed file (append, don't overwrite existing).
pub fn persist_to_seed(
    project_root: &std::path::Path,
    existing: &[Property],
    new_properties: &[Property],
) -> Result<(), String> {
    let seed_path = project_root.join("data").join("seed").join("properties.json");

    let mut all: Vec<&Property> = existing.iter().collect();
    for p in new_properties {
        if !all.iter().any(|ep| ep.id == p.id) {
            all.push(p);
        }
    }

    let json = serde_json::to_string_pretty(&all)
        .map_err(|e| format!("Failed to serialize properties: {}", e))?;

    // Atomic write: write to tmp, then rename
    let tmp_path = seed_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json)
        .map_err(|e| format!("Failed to write {}: {}", tmp_path.display(), e))?;
    std::fs::rename(&tmp_path, &seed_path)
        .map_err(|e| format!("Failed to rename to {}: {}", seed_path.display(), e))?;

    Ok(())
}
