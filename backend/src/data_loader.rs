use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

use crate::knowledge;
use crate::knowledge::fact::FactValue;
use crate::knowledge::graph::KnowledgeGraph;
use crate::knowledge::node::NodeType;
use crate::models::area_profile::{PriceRange, RedditSignals};
use crate::models::{AreaProfile, Property, Seller, Society};
use crate::search::SearchIndex;
use crate::state::AppState;

/// Load all data and construct the full AppState.
///
/// All entity data (properties, societies, areas) is derived from the Knowledge Graph
/// — the single source of truth. No seed JSON files are read at startup.
pub async fn load_app_state(project_root: &Path) -> AppState {
    // --- Knowledge Graph (must load first — societies and areas derive from it) ---
    let kg_dir = knowledge::store::knowledge_dir(project_root);
    let graph = knowledge::store::load_graph(&kg_dir).unwrap_or_else(|| {
        panic!(
            "No knowledge graph found at {}. Run pipeline/seed.py first.",
            kg_dir.display()
        );
    });
    let stats = graph.stats();
    println!(
        "Knowledge graph loaded: {} nodes, {} edges, {} facts",
        stats.total_nodes, stats.total_edges, stats.total_facts
    );

    // --- Derive societies and areas from KG ---
    let societies = societies_from_graph(&graph);
    println!("Derived {} societies from knowledge graph", societies.len());

    let areas = areas_from_graph(&graph);
    println!("Derived {} areas from knowledge graph", areas.len());

    // --- Derive properties from KG ---
    let properties = properties_from_graph(&graph);
    println!(
        "Derived {} properties from knowledge graph",
        properties.len()
    );
    let search_index = SearchIndex::build(&properties);
    println!(
        "Built local search index for {} properties",
        properties.len()
    );

    // --- Sellers ---
    let sellers_path = project_root
        .join("data")
        .join("sellers")
        .join("sellers.json");
    let sellers: Vec<Seller> = if sellers_path.exists() {
        load_json_direct::<Vec<Seller>>(&sellers_path)
    } else {
        println!("WARN: No sellers.json found at {}", sellers_path.display());
        Vec::new()
    };

    println!(
        "Loaded {} properties, {} areas (KG), {} societies (KG), {} sellers",
        properties.len(),
        areas.len(),
        societies.len(),
        sellers.len()
    );

    println!("Request-time AI disabled: search uses only local knowledge graph data");

    AppState {
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
        areas,
        societies,
        sellers: RwLock::new(sellers),
        knowledge: Arc::new(RwLock::new(graph)),
        project_root: project_root.to_path_buf(),
        interest_counter: AtomicU64::new(0),
        interest_rate_limiter: RwLock::new((Instant::now(), 0)),
        registration_counter: AtomicU64::new(0),
        registration_rate_limiter: RwLock::new((Instant::now(), 0)),
        publish_rate_limiter: RwLock::new((Instant::now(), 0)),
    }
}

/// Derive Society structs from KG society nodes.
///
/// Extracts known fact keys into the flat Society struct fields.
/// Missing facts get sensible defaults — KG nodes may have sparse data
/// when offline enrichment has not filled every dimension yet.
pub fn societies_from_graph(graph: &KnowledgeGraph) -> Vec<Society> {
    graph
        .nodes_of_type(NodeType::Society)
        .into_iter()
        .map(|node| {
            // Strip "society:" prefix from node id to get the plain id
            let id = node
                .id
                .strip_prefix("society:")
                .unwrap_or(&node.id)
                .to_string();

            Society {
                id,
                name: node.name.clone(),
                area: fact_text(node, "area").into(),
                city: fact_text(node, "city").into(),
                builder_name: fact_text(node, "builder_name").into(),
                year_built: fact_numeric(node, "year_built") as u32,
                total_units: fact_numeric(node, "total_units") as u32,
                summary: fact_text(node, "summary").into(),
                maintenance_sentiment: fact_text(node, "maintenance_sentiment")
                    .or_fact_text(node, "google_sentiment")
                    .into(),
                livability_sentiment: fact_text(node, "livability_sentiment").into(),
                common_positives: fact_tags(node, "common_positives")
                    .or_fact_tags(node, "google_top_positives")
                    .into(),
                common_complaints: fact_tags(node, "common_complaints")
                    .or_fact_tags(node, "google_top_negatives")
                    .into(),
                review_summary: fact_text(node, "review_summary")
                    .or_fact_text(node, "google_common_themes")
                    .into(),
                future_google_place_name: node.name.clone(),
                future_google_place_id: None,
                future_review_enrichment_status: String::from("kg_derived"),
            }
        })
        .collect()
}

/// Derive AreaProfile structs from KG area nodes.
///
/// KG area nodes have a different fact schema than legacy seed JSON, so we
/// map available facts and default the rest. The old fields like
/// `airport_noise_summary` and `reddit_signals` may not exist in KG yet.
fn areas_from_graph(graph: &KnowledgeGraph) -> Vec<AreaProfile> {
    graph
        .nodes_of_type(NodeType::Area)
        .into_iter()
        .map(|node| {
            let id = node
                .id
                .strip_prefix("area:")
                .unwrap_or(&node.id)
                .to_string();

            AreaProfile {
                id,
                name: node.name.clone(),
                city: fact_text(node, "city").into(),
                median_price_per_sqft: fact_numeric(node, "median_price_per_sqft") as u64,
                price_range_per_sqft: PriceRange { low: 0, high: 0 },
                trend_direction: fact_text(node, "trend_direction")
                    .or_fact_text(node, "price_trend")
                    .into(),
                trend_summary: fact_text(node, "trend_summary").into(),
                metro_access_summary: fact_text(node, "metro_details")
                    .or_fact_text(node, "metro_access")
                    .or_fact_text(node, "metro_status")
                    .into(),
                airport_noise_summary: fact_text(node, "airport_noise_summary").into(),
                traffic_summary: fact_text(node, "traffic")
                    .or_fact_text(node, "traffic_reality")
                    .into(),
                waterlogging_summary: fact_text(node, "waterlogging")
                    .or_fact_text(node, "waterlogging_risk")
                    .or_fact_text(node, "waterlogging_detail")
                    .into(),
                livability_summary: fact_text(node, "livability")
                    .or_fact_text(node, "livability_summary")
                    .or_fact_text(node, "area_vibe")
                    .into(),
                externality_tags: fact_tags(node, "externality_tags").into(),
                infrastructure_tags: fact_tags(node, "infrastructure_tags")
                    .or_fact_tags(node, "upcoming_infra")
                    .into(),
                reddit_signals: RedditSignals {
                    decision_drivers: fact_tags(node, "reddit_decision_drivers").into(),
                    recurring_concerns: fact_tags(node, "reddit_concerns").into(),
                    sentiment_label: fact_text(node, "reddit_sentiment").into(),
                    last_updated: String::new(),
                },
                community_notes: fact_text(node, "community_notes").into(),
                sample_size: 0,
                last_updated: node.updated_at.to_rfc3339(),
            }
        })
        .collect()
}

/// Derive Property structs from KG property nodes.
///
/// Maps KG fact keys (area, city, bhk, price, etc.) to Property struct fields.
/// Missing facts get conservative defaults so sparse local nodes can still render.
pub fn properties_from_graph(graph: &KnowledgeGraph) -> Vec<Property> {
    graph
        .nodes_of_type(NodeType::Property)
        .into_iter()
        .map(|node| {
            // Strip "property:" prefix from node id
            let id = node
                .id
                .strip_prefix("property:")
                .unwrap_or(&node.id)
                .to_string();

            // Derive society_id from property slug:
            // "discovered-prestige-park-grove-3bhk" → "soc-prestige-park-grove"
            let society_id = derive_society_id(&id);

            let area: String = fact_text(node, "area").into();
            let area_slug = area.to_lowercase().replace(' ', "-");
            let bhk = fact_numeric(node, "bhk") as u32;
            let mut price = fact_numeric(node, "price") as u64;
            let mut carpet_area_sqft = fact_numeric(node, "carpet_area_sqft") as u32;
            if let Some(pricing) = market_pricing_for_property(graph, &id, bhk) {
                let price_confidence = fact_confidence(node, "price");
                let sqft_confidence = fact_confidence(node, "carpet_area_sqft");
                if should_use_market_pricing(
                    price,
                    carpet_area_sqft,
                    price_confidence,
                    sqft_confidence,
                    pricing,
                ) {
                    price = pricing.representative_price();
                    carpet_area_sqft = pricing.representative_sqft();
                }
            }
            let price_per_sqft = if carpet_area_sqft > 0 && price > 0 {
                price / carpet_area_sqft as u64
            } else {
                0
            };

            let title: String = fact_text(node, "title").into();
            let title = if title.is_empty() {
                if bhk > 0 {
                    format!("{} BHK in {}", bhk, node.name)
                } else {
                    node.name.clone()
                }
            } else {
                title
            };

            let description: String = fact_text(node, "description_summary").into();
            let description = if description.is_empty() {
                let builder: String = fact_text(node, "builder_name").into();
                format!("{} by {} in {}", node.name, builder, area)
            } else {
                description
            };

            let mut tags: Vec<String> = fact_tags(node, "transparency_tags").into();
            if tags.is_empty() {
                tags.push("Discovered via Search".to_string());
                tags.push("Verification Pending".to_string());
            }

            Property {
                id,
                title,
                area: area.clone(),
                area_id: format!("area-{}", area_slug),
                city: fact_text(node, "city").into(),
                society_id,
                builder_name: fact_text(node, "builder_name").into(),
                property_type: {
                    let t: String = fact_text(node, "property_type").into();
                    if t.is_empty() {
                        "Apartment".to_string()
                    } else {
                        t
                    }
                },
                listing_type: {
                    let t: String = fact_text(node, "listing_type").into();
                    if t.is_empty() {
                        "Resale".to_string()
                    } else {
                        t
                    }
                },
                bhk,
                price,
                price_per_sqft,
                carpet_area_sqft,
                super_builtup_sqft: fact_numeric(node, "super_builtup_sqft") as u32,
                floor: fact_numeric(node, "floor") as u32,
                total_floors: fact_numeric(node, "total_floors") as u32,
                facing: {
                    let f: String = fact_text(node, "facing").into();
                    if f.is_empty() {
                        "Not specified".to_string()
                    } else {
                        f
                    }
                },
                possession_status: {
                    let p: String = fact_text(node, "possession_status").into();
                    if p.is_empty() {
                        "unknown".to_string()
                    } else {
                        p
                    }
                },
                metro_distance_mins: fact_numeric(node, "metro_distance_mins") as u32,
                maintenance_cost_monthly: fact_numeric(node, "maintenance_cost_monthly") as u32,
                society_quality_score: fact_numeric(node, "society_quality_score").max(0.5),
                builder_quality_score: fact_numeric(node, "builder_quality_score").max(0.5),
                document_completeness_score: fact_numeric(node, "document_completeness_score")
                    .max(0.5),
                litigation_risk: {
                    let v = fact_numeric(node, "litigation_risk");
                    if v == 0.0 {
                        0.1
                    } else {
                        v
                    }
                },
                noise_score: fact_numeric(node, "noise_score").max(0.5),
                sunlight_score: fact_numeric(node, "sunlight_score").max(0.5),
                airport_noise_score: {
                    let v = fact_numeric(node, "airport_noise_score");
                    if v == 0.0 {
                        0.1
                    } else {
                        v
                    }
                },
                waterlogging_risk_score: {
                    let v = fact_numeric(node, "waterlogging_risk_score");
                    if v == 0.0 {
                        0.2
                    } else {
                        v
                    }
                },
                traffic_score: fact_numeric(node, "traffic_score").max(0.5),
                days_on_market: fact_numeric(node, "days_on_market") as u32,
                greenery_score: None,
                open_space_score: None,
                resale_strength_score: None,
                interest_level: None,
                saves_last_7d: None,
                offers_last_7d: None,
                images: {
                    let imgs: Vec<String> = fact_tags(node, "images").into();
                    imgs
                },
                hero_image: fact_text(node, "hero_image").into(),
                description_summary: description,
                transparency_tags: tags,
                source_reference: {
                    let s: String = fact_text(node, "source_reference").into();
                    if s.is_empty() {
                        "Knowledge Graph".to_string()
                    } else {
                        s
                    }
                },
                seller_id: {
                    let s: String = fact_text(node, "seller_id").into();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
            }
        })
        .collect()
}

/// Derive society_id from a property slug.
///
/// Strips BHK suffix (e.g. "-3bhk") and "discovered-" prefix, then prepends "soc-".
/// Examples:
///   "discovered-prestige-park-grove-3bhk" → "soc-prestige-park-grove"
///   "prop-w-001" → "soc-prop-w-001" (no BHK suffix or discovered- prefix)
fn derive_society_id(property_id: &str) -> String {
    let mut slug = property_id.to_string();

    // Strip BHK suffix like "-3bhk", "-2bhk"
    if let Some(pos) = slug.rfind("-") {
        let suffix = &slug[pos + 1..];
        if suffix.ends_with("bhk") && suffix[..suffix.len() - 3].parse::<u32>().is_ok() {
            slug.truncate(pos);
        }
    }

    // Strip "discovered-" prefix
    if let Some(rest) = slug.strip_prefix("discovered-") {
        slug = rest.to_string();
    }

    format!("soc-{}", slug)
}

#[derive(Clone, Copy, Debug)]
struct MarketPricing {
    price_low: u64,
    price_high: u64,
    sqft_low: u32,
    sqft_high: u32,
}

impl MarketPricing {
    fn representative_price(&self) -> u64 {
        (self.price_low + self.price_high) / 2
    }

    fn representative_sqft(&self) -> u32 {
        (self.sqft_low + self.sqft_high) / 2
    }
}

fn market_pricing_for_property(
    graph: &KnowledgeGraph,
    property_id: &str,
    bhk: u32,
) -> Option<MarketPricing> {
    if bhk == 0 {
        return None;
    }

    let society_id = derive_society_id(property_id);
    let slug = society_id.strip_prefix("soc-")?;
    let society_node = graph.get_node(&format!("society:{}", slug))?;
    let pricing_text: String = fact_text(society_node, &format!("pricing_{}bhk", bhk)).into();
    parse_market_pricing(&pricing_text)
}

fn parse_market_pricing(raw: &str) -> Option<MarketPricing> {
    if raw.trim().is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let price_range = value.get("price_range_lakh")?.as_str()?;
    let sqft_range = value.get("sqft_range")?.as_str()?;
    let (price_low_lakh, price_high_lakh) = parse_number_range(price_range)?;
    let (sqft_low, sqft_high) = parse_number_range(sqft_range)?;

    Some(MarketPricing {
        price_low: (price_low_lakh * 100_000.0).round() as u64,
        price_high: (price_high_lakh * 100_000.0).round() as u64,
        sqft_low: sqft_low.round() as u32,
        sqft_high: sqft_high.round() as u32,
    })
}

fn parse_number_range(raw: &str) -> Option<(f64, f64)> {
    let numbers: Vec<f64> = raw
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();

    match numbers.as_slice() {
        [] => None,
        [single] => Some((*single, *single)),
        many => Some((many[0], *many.last().unwrap_or(&many[0]))),
    }
}

fn should_use_market_pricing(
    price: u64,
    sqft: u32,
    price_confidence: f32,
    sqft_confidence: f32,
    pricing: MarketPricing,
) -> bool {
    let low_confidence = price_confidence <= 0.65 || sqft_confidence <= 0.65;
    if !low_confidence {
        return false;
    }

    let price_low_floor = pricing.price_low.saturating_mul(3) / 4;
    let price_high_ceiling = pricing.price_high.saturating_mul(5) / 4;
    let sqft_low_floor = pricing.sqft_low.saturating_mul(1) / 2;
    let sqft_high_ceiling = pricing.sqft_high.saturating_mul(3) / 2;

    price == 0
        || sqft == 0
        || price < price_low_floor
        || price > price_high_ceiling
        || sqft < sqft_low_floor
        || sqft > sqft_high_ceiling
}

// --- Fact extraction helpers ---

/// A string wrapper that supports fallback chaining via `.or_fact_text()`.
struct FactStr(String);

impl FactStr {
    /// If this string is empty, try another fact key from the node.
    fn or_fact_text(self, node: &knowledge::node::Node, key: &str) -> Self {
        if self.0.is_empty() {
            fact_text(node, key)
        } else {
            self
        }
    }
}

/// Allow implicit conversion to String for struct field assignment.
impl From<FactStr> for String {
    fn from(f: FactStr) -> String {
        f.0
    }
}

/// A tags wrapper that supports fallback chaining via `.or_fact_tags()`.
struct FactTags(Vec<String>);

impl FactTags {
    fn or_fact_tags(self, node: &knowledge::node::Node, key: &str) -> Self {
        if self.0.is_empty() {
            fact_tags(node, key)
        } else {
            self
        }
    }
}

impl From<FactTags> for Vec<String> {
    fn from(f: FactTags) -> Vec<String> {
        f.0
    }
}

/// Extract a text fact value, returning empty string if missing.
fn fact_text(node: &knowledge::node::Node, key: &str) -> FactStr {
    let s = node
        .get_fact(key)
        .map(|f| match &f.value {
            FactValue::Text(t) => t.clone(),
            FactValue::Numeric(n) => n.to_string(),
            FactValue::Bool(b) => b.to_string(),
            FactValue::Score { value, .. } => value.to_string(),
            FactValue::Tags(tags) => tags.join(", "),
        })
        .unwrap_or_default();
    FactStr(s)
}

/// Extract a numeric fact value, returning 0.0 if missing.
fn fact_numeric(node: &knowledge::node::Node, key: &str) -> f64 {
    node.get_fact(key)
        .map(|f| match &f.value {
            FactValue::Numeric(n) => *n,
            FactValue::Score { value, .. } => *value,
            _ => 0.0,
        })
        .unwrap_or(0.0)
}

fn fact_confidence(node: &knowledge::node::Node, key: &str) -> f32 {
    node.get_fact(key).map(|f| f.confidence).unwrap_or(0.0)
}

/// Extract a tags fact value, returning empty vec if missing.
fn fact_tags(node: &knowledge::node::Node, key: &str) -> FactTags {
    let tags = node
        .get_fact(key)
        .map(|f| match &f.value {
            FactValue::Tags(t) => t.clone(),
            FactValue::Text(t) if !t.is_empty() => vec![t.clone()],
            _ => Vec::new(),
        })
        .unwrap_or_default();
    FactTags(tags)
}

/// Direct file read fallback (sync).
fn load_json_direct<T: serde::de::DeserializeOwned>(path: &PathBuf) -> T {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::fact::SourcedFact;
    use crate::knowledge::node::Node;

    fn make_society_node(slug: &str, name: &str, area: &str, builder: &str) -> Node {
        let id = format!("society:{}", slug);
        let mut node = Node::new(&id, NodeType::Society, name);
        node.add_facts(vec![
            SourcedFact::manual("area", FactValue::Text(area.into())),
            SourcedFact::manual("city", FactValue::Text("Bengaluru".into())),
            SourcedFact::manual("builder_name", FactValue::Text(builder.into())),
            SourcedFact::manual("year_built", FactValue::Numeric(2020.0)),
            SourcedFact::manual("total_units", FactValue::Numeric(500.0)),
            SourcedFact::manual("summary", FactValue::Text("A great society".into())),
        ]);
        node
    }

    fn make_area_node(slug: &str, name: &str) -> Node {
        let id = format!("area:{}", slug);
        let mut node = Node::new(&id, NodeType::Area, name);
        node.add_facts(vec![
            SourcedFact::manual("city", FactValue::Text("Bengaluru".into())),
            SourcedFact::manual("metro_status", FactValue::Text("operational".into())),
            SourcedFact::manual("area_vibe", FactValue::Text("Tech hub".into())),
        ]);
        node
    }

    #[test]
    fn test_societies_from_graph() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_society_node(
            "test-society",
            "Test Society",
            "Whitefield",
            "Test Builder",
        ));
        graph.rebuild_indexes();

        let societies = societies_from_graph(&graph);
        assert_eq!(societies.len(), 1);
        let s = &societies[0];
        assert_eq!(s.id, "test-society");
        assert_eq!(s.name, "Test Society");
        assert_eq!(s.area, "Whitefield");
        assert_eq!(s.builder_name, "Test Builder");
        assert_eq!(s.year_built, 2020);
        assert_eq!(s.total_units, 500);
    }

    #[test]
    fn test_areas_from_graph() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_area_node("whitefield", "Whitefield"));
        graph.rebuild_indexes();

        let areas = areas_from_graph(&graph);
        assert_eq!(areas.len(), 1);
        let a = &areas[0];
        assert_eq!(a.id, "whitefield");
        assert_eq!(a.name, "Whitefield");
        assert_eq!(a.city, "Bengaluru");
        // metro_access_summary falls back to metro_status
        assert_eq!(a.metro_access_summary, "operational");
        // livability_summary falls back to area_vibe
        assert_eq!(a.livability_summary, "Tech hub");
    }

    #[test]
    fn test_society_sparse_data_defaults() {
        let mut graph = KnowledgeGraph::new();
        // Minimal node — only name, no facts
        let node = Node::new("society:sparse", NodeType::Society, "Sparse Society");
        graph.add_node(node);
        graph.rebuild_indexes();

        let societies = societies_from_graph(&graph);
        assert_eq!(societies.len(), 1);
        let s = &societies[0];
        assert_eq!(s.id, "sparse");
        assert_eq!(s.name, "Sparse Society");
        assert_eq!(s.area, "");
        assert_eq!(s.year_built, 0);
        assert_eq!(s.total_units, 0);
    }

    fn make_property_node(
        slug: &str,
        name: &str,
        area: &str,
        builder: &str,
        bhk: u32,
        price: f64,
    ) -> Node {
        let id = format!("property:{}", slug);
        let mut node = Node::new(&id, NodeType::Property, name);
        node.add_facts(vec![
            SourcedFact::manual("area", FactValue::Text(area.into())),
            SourcedFact::manual("city", FactValue::Text("Bengaluru".into())),
            SourcedFact::manual("builder_name", FactValue::Text(builder.into())),
            SourcedFact::manual("bhk", FactValue::Numeric(bhk as f64)),
            SourcedFact::manual("price", FactValue::Numeric(price)),
            SourcedFact::manual("carpet_area_sqft", FactValue::Numeric(1200.0)),
            SourcedFact::manual("title", FactValue::Text(format!("{} BHK in {}", bhk, name))),
        ]);
        node
    }

    fn low_conf_numeric_fact(key: &str, value: f64) -> SourcedFact {
        let mut fact = SourcedFact::manual(key, FactValue::Numeric(value));
        fact.confidence = 0.6;
        fact
    }

    #[test]
    fn test_properties_from_graph() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_property_node(
            "discovered-prestige-lakeside-3bhk",
            "Prestige Lakeside Habitat",
            "Whitefield",
            "Prestige Group",
            3,
            15000000.0,
        ));
        graph.rebuild_indexes();

        let properties = properties_from_graph(&graph);
        assert_eq!(properties.len(), 1);
        let p = &properties[0];
        assert_eq!(p.id, "discovered-prestige-lakeside-3bhk");
        assert_eq!(p.area, "Whitefield");
        assert_eq!(p.city, "Bengaluru");
        assert_eq!(p.builder_name, "Prestige Group");
        assert_eq!(p.bhk, 3);
        assert_eq!(p.price, 15000000);
        assert_eq!(p.carpet_area_sqft, 1200);
        assert_eq!(p.price_per_sqft, 12500); // 15000000 / 1200
        assert_eq!(p.title, "3 BHK in Prestige Lakeside Habitat");
        assert_eq!(p.society_id, "soc-prestige-lakeside");
        assert_eq!(p.property_type, "Apartment");
        assert_eq!(p.listing_type, "Resale");
    }

    #[test]
    fn test_property_sparse_defaults() {
        let mut graph = KnowledgeGraph::new();
        // Minimal node — only name, no facts
        let node = Node::new(
            "property:minimal-prop",
            NodeType::Property,
            "Minimal Property",
        );
        graph.add_node(node);
        graph.rebuild_indexes();

        let properties = properties_from_graph(&graph);
        assert_eq!(properties.len(), 1);
        let p = &properties[0];
        assert_eq!(p.id, "minimal-prop");
        assert_eq!(p.area, "");
        assert_eq!(p.bhk, 0);
        assert_eq!(p.price, 0);
        assert_eq!(p.price_per_sqft, 0);
        assert_eq!(p.carpet_area_sqft, 0);
        assert_eq!(p.property_type, "Apartment");
        assert_eq!(p.facing, "Not specified");
        assert_eq!(p.possession_status, "unknown");
        // Default scores
        assert_eq!(p.society_quality_score, 0.5);
        assert_eq!(p.litigation_risk, 0.1);
        assert!(p
            .transparency_tags
            .contains(&"Discovered via Search".to_string()));
        assert!(p.greenery_score.is_none());
        assert!(p.seller_id.is_none());
    }

    #[test]
    fn test_low_confidence_property_uses_society_market_pricing() {
        let mut graph = KnowledgeGraph::new();
        let mut society = make_society_node(
            "prestige-raintree-park",
            "Prestige Raintree Park",
            "Whitefield",
            "Prestige Group",
        );
        society.add_fact(SourcedFact::manual(
            "pricing_3bhk",
            FactValue::Text(
                r#"{"bhk":"3BHK","price_range_lakh":"259-353","sqft_range":"2004-2482"}"#.into(),
            ),
        ));
        graph.add_node(society);

        let id = "property:discovered-prestige-raintree-park-3bhk";
        let mut property = Node::new(id, NodeType::Property, "Prestige Raintree Park");
        property.add_facts(vec![
            SourcedFact::manual("area", FactValue::Text("Whitefield".into())),
            SourcedFact::manual("city", FactValue::Text("Bengaluru".into())),
            SourcedFact::manual("builder_name", FactValue::Text("Prestige Group".into())),
            SourcedFact::manual("bhk", FactValue::Numeric(3.0)),
            low_conf_numeric_fact("price", 11_500_000.0),
            low_conf_numeric_fact("carpet_area_sqft", 521.0),
            SourcedFact::manual(
                "title",
                FactValue::Text("3 BHK in Prestige Raintree Park".into()),
            ),
        ]);
        graph.add_node(property);
        graph.rebuild_indexes();

        let properties = properties_from_graph(&graph);
        let p = properties
            .iter()
            .find(|p| p.id == "discovered-prestige-raintree-park-3bhk")
            .expect("property should be derived");
        assert_eq!(p.price, 30_600_000);
        assert_eq!(p.carpet_area_sqft, 2243);
        assert_eq!(p.price_per_sqft, 13_642);
    }

    #[test]
    fn test_parse_number_range() {
        assert_eq!(parse_number_range("259-353"), Some((259.0, 353.0)));
        assert_eq!(parse_number_range("2004-2482"), Some((2004.0, 2482.0)));
        assert_eq!(parse_number_range("200"), Some((200.0, 200.0)));
    }

    #[test]
    fn test_derive_society_id() {
        assert_eq!(
            derive_society_id("discovered-prestige-park-grove-3bhk"),
            "soc-prestige-park-grove"
        );
        assert_eq!(
            derive_society_id("discovered-sobha-windsor-2bhk"),
            "soc-sobha-windsor"
        );
        assert_eq!(derive_society_id("prop-w-001"), "soc-prop-w-001");
        assert_eq!(
            derive_society_id("discovered-some-project"),
            "soc-some-project"
        );
    }

    #[test]
    fn test_fact_text_fallback_chain() {
        let mut node = Node::new("society:test", NodeType::Society, "Test");
        // Only add google_sentiment, not maintenance_sentiment
        node.add_fact(SourcedFact::manual(
            "google_sentiment",
            FactValue::Text("positive".into()),
        ));

        let result: String = fact_text(&node, "maintenance_sentiment")
            .or_fact_text(&node, "google_sentiment")
            .into();
        assert_eq!(result, "positive");
    }
}
