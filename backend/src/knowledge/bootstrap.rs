//! Bootstrap the knowledge graph from existing seed data.
//!
//! Reads properties.json, societies.json, area_profiles.json and creates
//! typed graph nodes with edges. All facts are sourced as `Manual`.

use super::edge::{Edge, Relation};
use super::fact::{FactValue, SourcedFact};
use super::graph::KnowledgeGraph;
use super::node::{node_id, Node, NodeType};
use crate::models::{AreaProfile, Property, Society};

/// Bootstrap a knowledge graph from seed data.
pub fn bootstrap_from_seed(
    properties: &[Property],
    societies: &[Society],
    areas: &[AreaProfile],
) -> KnowledgeGraph {
    let mut graph = KnowledgeGraph::new();

    // --- Area nodes ---
    for area in areas {
        let id = node_id(NodeType::Area, &slugify(&area.id));
        let mut node = Node::new(&id, NodeType::Area, &area.name);

        node.add_facts(vec![
            SourcedFact::manual("city", FactValue::Text(area.city.clone())),
            SourcedFact::manual(
                "median_price_per_sqft",
                FactValue::Numeric(area.median_price_per_sqft as f64),
            ),
            SourcedFact::manual("trend_direction", FactValue::Text(area.trend_direction.clone())),
            SourcedFact::manual("trend_summary", FactValue::Text(area.trend_summary.clone())),
            SourcedFact::manual(
                "metro_access",
                FactValue::Text(area.metro_access_summary.clone()),
            ),
            SourcedFact::manual("traffic", FactValue::Text(area.traffic_summary.clone())),
            SourcedFact::manual(
                "waterlogging",
                FactValue::Text(area.waterlogging_summary.clone()),
            ),
            SourcedFact::manual(
                "livability",
                FactValue::Text(area.livability_summary.clone()),
            ),
            SourcedFact::manual(
                "externality_tags",
                FactValue::Tags(area.externality_tags.clone()),
            ),
            SourcedFact::manual(
                "infrastructure_tags",
                FactValue::Tags(area.infrastructure_tags.clone()),
            ),
            SourcedFact::manual(
                "reddit_sentiment",
                FactValue::Text(area.reddit_signals.sentiment_label.clone()),
            ),
            SourcedFact::manual(
                "reddit_decision_drivers",
                FactValue::Tags(area.reddit_signals.decision_drivers.clone()),
            ),
            SourcedFact::manual(
                "reddit_concerns",
                FactValue::Tags(area.reddit_signals.recurring_concerns.clone()),
            ),
        ]);

        graph.add_node(node);
    }

    // --- Builder nodes (deduplicated from societies + properties) ---
    let mut seen_builders = std::collections::HashSet::new();
    for society in societies {
        let slug = slugify(&society.builder_name);
        if seen_builders.insert(slug.clone()) {
            let id = node_id(NodeType::Builder, &slug);
            graph.add_node(Node::new(&id, NodeType::Builder, &society.builder_name));
        }
    }

    // --- Society nodes ---
    for society in societies {
        let id = node_id(NodeType::Society, &slugify(&society.id));
        let mut node = Node::new(&id, NodeType::Society, &society.name);

        node.add_facts(vec![
            SourcedFact::manual("area", FactValue::Text(society.area.clone())),
            SourcedFact::manual("city", FactValue::Text(society.city.clone())),
            SourcedFact::manual("builder_name", FactValue::Text(society.builder_name.clone())),
            SourcedFact::manual("year_built", FactValue::Numeric(society.year_built as f64)),
            SourcedFact::manual("total_units", FactValue::Numeric(society.total_units as f64)),
            SourcedFact::manual("summary", FactValue::Text(society.summary.clone())),
            SourcedFact::manual(
                "maintenance_sentiment",
                FactValue::Text(society.maintenance_sentiment.clone()),
            ),
            SourcedFact::manual(
                "livability_sentiment",
                FactValue::Text(society.livability_sentiment.clone()),
            ),
            SourcedFact::manual(
                "common_positives",
                FactValue::Tags(society.common_positives.clone()),
            ),
            SourcedFact::manual(
                "common_complaints",
                FactValue::Tags(society.common_complaints.clone()),
            ),
            SourcedFact::manual("review_summary", FactValue::Text(society.review_summary.clone())),
        ]);

        graph.add_node(node);

        // Edge: Society → Area
        let area_id = find_area_id(areas, &society.area);
        if let Some(area_id) = area_id {
            graph.add_edge(Edge::new(
                id.clone(),
                node_id(NodeType::Area, &slugify(&area_id)),
                Relation::SocietyInArea,
            ));
        }

        // Edge: Society → Builder
        graph.add_edge(Edge::new(
            id,
            node_id(NodeType::Builder, &slugify(&society.builder_name)),
            Relation::BuiltBy,
        ));
    }

    // --- Property nodes ---
    for prop in properties {
        let id = node_id(NodeType::Property, &slugify(&prop.id));
        let mut node = Node::new(&id, NodeType::Property, &prop.title);

        node.add_facts(vec![
            SourcedFact::manual("area", FactValue::Text(prop.area.clone())),
            SourcedFact::manual("city", FactValue::Text(prop.city.clone())),
            SourcedFact::manual("bhk", FactValue::Numeric(prop.bhk as f64)),
            SourcedFact::manual("price", FactValue::Numeric(prop.price as f64)),
            SourcedFact::manual("price_per_sqft", FactValue::Numeric(prop.price_per_sqft as f64)),
            SourcedFact::manual(
                "carpet_area_sqft",
                FactValue::Numeric(prop.carpet_area_sqft as f64),
            ),
            SourcedFact::manual("floor", FactValue::Numeric(prop.floor as f64)),
            SourcedFact::manual("total_floors", FactValue::Numeric(prop.total_floors as f64)),
            SourcedFact::manual("facing", FactValue::Text(prop.facing.clone())),
            SourcedFact::manual(
                "possession_status",
                FactValue::Text(prop.possession_status.clone()),
            ),
            SourcedFact::manual(
                "metro_distance_mins",
                FactValue::Numeric(prop.metro_distance_mins as f64),
            ),
            SourcedFact::manual(
                "society_quality_score",
                FactValue::Score {
                    value: prop.society_quality_score,
                    explanation: "From seed data".into(),
                },
            ),
            SourcedFact::manual(
                "builder_quality_score",
                FactValue::Score {
                    value: prop.builder_quality_score,
                    explanation: "From seed data".into(),
                },
            ),
            SourcedFact::manual(
                "waterlogging_risk",
                FactValue::Score {
                    value: prop.waterlogging_risk_score,
                    explanation: "From seed data".into(),
                },
            ),
            SourcedFact::manual(
                "transparency_tags",
                FactValue::Tags(prop.transparency_tags.clone()),
            ),
            SourcedFact::manual(
                "description_summary",
                FactValue::Text(prop.description_summary.clone()),
            ),
        ]);

        graph.add_node(node);

        // Edge: Property → Society
        let society_graph_id = node_id(NodeType::Society, &slugify(&prop.society_id));
        if graph.get_node(&society_graph_id).is_some() {
            graph.add_edge(Edge::new(id.clone(), society_graph_id, Relation::PropertyInSociety));
        }

        // Edge: Property → Builder
        graph.add_edge(Edge::new(
            id,
            node_id(NodeType::Builder, &slugify(&prop.builder_name)),
            Relation::BuiltBy,
        ));
    }

    graph.rebuild_indexes();
    graph
}

/// Find the area_id for a given area name from the area profiles.
fn find_area_id(areas: &[AreaProfile], area_name: &str) -> Option<String> {
    areas
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(area_name))
        .map(|a| a.id.clone())
}

/// Simple slug: lowercase, replace spaces/underscores with hyphens, strip non-alphanum.
fn slugify(s: &str) -> String {
    s.to_lowercase()
        .replace(['_', ' '], "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}
