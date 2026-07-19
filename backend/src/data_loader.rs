use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use tokio::sync::RwLock;

use crate::dag_config::{
    better_source_type, buyer_visible_fact, load_resolution_policies, ResolutionPoliciesFile,
};
use crate::discovery::load_discovery_config;
use crate::knowledge;
use crate::knowledge::fact::{google_reviews_url_from_facts, FactValue};
use crate::knowledge::graph::KnowledgeGraph;
use crate::knowledge::node::NodeType;
use crate::models::area_profile::{PriceRange, RedditSignals};
use crate::models::{AreaProfile, Property, Seller, Society};
use crate::search::{HashSemanticEmbedder, SearchIndex, SemanticEmbedder, SemanticSearchIndex};
use crate::serving::{
    LoadedServingBundle, ServingBundleLoader, ServingEntityFactRows, ServingEntityRecord,
    ServingFactIndex,
};
use crate::state::AppState;
use crate::{
    lake::{LakeStoreLocation, LAKE_URL_ENV},
    serving::ServingBundleLoadError,
};

/// Load all data and construct the full AppState.
///
/// The promoted serving bundle is the canonical request-path data source.
/// Legacy `data/knowledge` JSON is intentionally not loaded into runtime state.
pub async fn load_app_state(project_root: &Path) -> AppState {
    let serving_bundle = load_serving_bundle(project_root)
        .await
        .unwrap_or_else(|err| panic!("Serving bundle startup contract failed: {err}"));
    let bundle = serving_bundle
        .as_ref()
        .unwrap_or_else(|| panic!("No promoted serving bundle found. Run the asset DAG first."));

    let graph = KnowledgeGraph::new();
    println!("Runtime knowledge graph starts empty; serving bundle is the only startup corpus");

    let properties = properties_from_serving_bundle(bundle);
    if properties.is_empty() {
        panic!(
            "Serving bundle {} has no property entities; refusing to fall back to legacy data",
            bundle.manifest.bundle_version
        );
    }
    let societies = societies_from_serving_bundle(bundle);
    let areas = areas_from_serving_properties(&properties);
    println!(
        "Derived {} properties, {} societies, {} areas from serving bundle {}",
        properties.len(),
        societies.len(),
        areas.len(),
        bundle.manifest.bundle_version
    );

    let search_index = SearchIndex::build(&properties);
    println!(
        "Built local search index for {} properties",
        properties.len()
    );

    let semantic_embedder: Arc<dyn SemanticEmbedder> = Arc::new(HashSemanticEmbedder::default());
    let semantic_index =
        SemanticSearchIndex::from_serving_entities(&bundle.entities, semantic_embedder.as_ref());
    println!(
        "Built semantic search index with {} documents using {}",
        semantic_index.len(),
        semantic_index.model_id()
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
        "Loaded {} properties, {} areas, {} societies, {} sellers",
        properties.len(),
        areas.len(),
        societies.len(),
        sellers.len()
    );

    println!("Request-time AI disabled: search uses only local serving bundle data");
    let discovery_config = load_discovery_config(project_root);

    AppState {
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
        semantic_index: RwLock::new(semantic_index),
        semantic_embedder,
        serving_bundle: RwLock::new(serving_bundle),
        areas,
        societies,
        sellers: RwLock::new(sellers),
        discovery_config,
        knowledge: Arc::new(RwLock::new(graph)),
        project_root: project_root.to_path_buf(),
        interest_counter: AtomicU64::new(0),
        interest_rate_limiter: RwLock::new((Instant::now(), 0)),
        registration_counter: AtomicU64::new(0),
        registration_rate_limiter: RwLock::new((Instant::now(), 0)),
        publish_rate_limiter: RwLock::new((Instant::now(), 0)),
    }
}

pub async fn load_serving_bundle(
    project_root: &Path,
) -> Result<Option<Arc<LoadedServingBundle>>, String> {
    let explicitly_configured = std::env::var_os(LAKE_URL_ENV).is_some();
    let lake_location = LakeStoreLocation::from_env(project_root).map_err(|err| err.to_string())?;
    load_serving_bundle_from_location(project_root, lake_location, explicitly_configured).await
}

async fn load_serving_bundle_from_location(
    project_root: &Path,
    lake_location: LakeStoreLocation,
    explicitly_configured: bool,
) -> Result<Option<Arc<LoadedServingBundle>>, String> {
    let cache_root = project_root.join("data").join("cache").join("serving");
    let lake = match lake_location.open() {
        Ok(lake) => lake,
        Err(err) if explicitly_configured => {
            return Err(format!("lake unavailable at {lake_location}: {err}"));
        }
        Err(err) => {
            eprintln!("WARN: Serving bundle lake unavailable at {lake_location}: {err}");
            return Ok(None);
        }
    };

    match ServingBundleLoader::new(lake, cache_root)
        .load_current_search_bundle()
        .await
    {
        Ok(Some(bundle)) => {
            println!(
                "Loaded serving bundle {} with {} entities and {} facts",
                bundle.manifest.bundle_version,
                bundle.manifest.entity_count,
                bundle.manifest.fact_count
            );
            Ok(Some(Arc::new(bundle)))
        }
        Ok(None) if explicitly_configured => Err(format!(
            "no promoted search serving bundle found at explicitly configured lake {lake_location}"
        )),
        Ok(None) => {
            println!("No promoted serving bundle found; using local property recall only");
            Ok(None)
        }
        Err(err) if explicitly_configured => Err(format!(
            "failed to load promoted search serving bundle from {lake_location}: {err}"
        )),
        Err(err) => {
            log_serving_load_error(err);
            Ok(None)
        }
    }
}

fn log_serving_load_error(err: ServingBundleLoadError) {
    eprintln!("WARN: Failed to load serving bundle; using local property recall only: {err}");
}

pub fn properties_from_serving_bundle(bundle: &LoadedServingBundle) -> Vec<Property> {
    properties_from_serving_records(
        &bundle.entities,
        &bundle.fact_index,
        &bundle.manifest.bundle_version,
    )
}

pub fn properties_from_serving_records(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    bundle_version: &str,
) -> Vec<Property> {
    let mut properties = entities
        .iter()
        .filter(|entity| entity.entity_type == "property")
        .map(|entity| property_from_serving_entity(entity, fact_index, bundle_version))
        .collect::<Vec<_>>();
    if properties.is_empty() {
        properties =
            representative_properties_from_serving_societies(entities, fact_index, bundle_version);
    }
    properties.sort_by(|left, right| left.id.cmp(&right.id));
    properties
}

fn representative_properties_from_serving_societies(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    bundle_version: &str,
) -> Vec<Property> {
    let entities_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<BTreeMap<_, _>>();

    fact_index
        .rows()
        .filter(|(entity_id, rows)| {
            entity_id.starts_with("society:")
                && latest_bool(Some(rows), "source_scan_selected").unwrap_or(false)
        })
        .flat_map(|(entity_id, rows)| {
            let entity = entities_by_id.get(entity_id).copied();
            let bhks = serving_society_bhks(rows);
            bhks.into_iter()
                .map(|bhk| {
                    representative_property_from_serving_society(
                        entity_id,
                        entity,
                        rows,
                        bhk,
                        bundle_version,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn representative_property_from_serving_society(
    entity_id: &str,
    entity: Option<&ServingEntityRecord>,
    rows: &ServingEntityFactRows,
    bhk: u32,
    bundle_version: &str,
) -> Property {
    let society_name = entity
        .map(|entity| entity.name.clone())
        .or_else(|| latest_text(Some(rows), "title"))
        .unwrap_or_else(|| title_case_slug(strip_entity_prefix(entity_id, "society:").as_str()));
    let society_id = society_runtime_id_from_parts(entity_id, &society_name);
    let society_slug = society_id.strip_prefix("soc-").unwrap_or(&society_id);
    let id = format!("discovered-{society_slug}-{bhk}bhk");
    let area = latest_text(Some(rows), "area").unwrap_or_default();
    let area_slug = slug(&area);
    let pricing = serving_market_pricing(rows, bhk);
    let mut price = pricing
        .map(|pricing| pricing.representative_price())
        .unwrap_or(0);
    if price == 0 {
        price = latest_numeric(Some(rows), "market_starting_price_inr")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u64;
    }
    let carpet_area_sqft = pricing
        .map(|pricing| pricing.representative_sqft())
        .unwrap_or(0);
    let price_per_sqft = if price > 0 && carpet_area_sqft > 0 {
        price / carpet_area_sqft as u64
    } else {
        0
    };
    let builder_name = latest_text(Some(rows), "builder_name")
        .or_else(|| latest_text(Some(rows), "rera_promoter_name"))
        .unwrap_or_default();
    let root_source = entity
        .and_then(|entity| entity.root_source.as_deref())
        .unwrap_or("serving_bundle");
    let mut transparency_tags = vec![
        format!("Source: {root_source}"),
        "Fresh area scan".to_string(),
    ];
    if latest_bool(Some(rows), "rera_registered").unwrap_or(false) {
        transparency_tags.push("RERA verified".to_string());
    }
    if price == 0 {
        transparency_tags.push("Price unavailable".to_string());
    }

    Property {
        id,
        title: format!("{bhk} BHK in {society_name}"),
        area: area.clone(),
        area_id: format!("area-{area_slug}"),
        city: latest_text(Some(rows), "city").unwrap_or_else(|| "Bengaluru".to_string()),
        society_id,
        builder_name,
        property_type: latest_text(Some(rows), "rera_project_type")
            .unwrap_or_else(|| "Apartment".to_string()),
        listing_type: "Project".to_string(),
        bhk,
        price,
        price_per_sqft,
        carpet_area_sqft,
        super_builtup_sqft: carpet_area_sqft,
        floor: 0,
        total_floors: 0,
        facing: "Not specified".to_string(),
        possession_status: latest_text(Some(rows), "market_project_status")
            .or_else(|| latest_text(Some(rows), "rera_status"))
            .unwrap_or_else(|| "unknown".to_string()),
        metro_distance_mins: latest_numeric(Some(rows), "metro_distance_mins")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        maintenance_cost_monthly: 0,
        society_quality_score: latest_numeric(Some(rows), "society_quality_score"),
        builder_quality_score: latest_numeric(Some(rows), "builder_quality_score"),
        document_completeness_score: latest_numeric(Some(rows), "document_completeness_score"),
        litigation_risk: latest_numeric(Some(rows), "litigation_risk"),
        noise_score: latest_numeric(Some(rows), "noise_score"),
        sunlight_score: latest_numeric(Some(rows), "sunlight_score"),
        airport_noise_score: latest_numeric(Some(rows), "airport_noise_score"),
        waterlogging_risk_score: latest_numeric(Some(rows), "waterlogging_risk_score"),
        traffic_score: latest_numeric(Some(rows), "traffic_score"),
        days_on_market: 0,
        greenery_score: latest_numeric(Some(rows), "greenery_score"),
        open_space_score: latest_numeric(Some(rows), "open_space_score"),
        resale_strength_score: latest_numeric(Some(rows), "resale_strength_score"),
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: latest_tags(Some(rows), "images").unwrap_or_default(),
        hero_image: latest_text(Some(rows), "hero_image").unwrap_or_default(),
        description_summary: latest_text(Some(rows), "summary")
            .unwrap_or_else(|| format!("{society_name} in {area}")),
        transparency_tags,
        source_reference: format!("search_serving_bundle:{bundle_version}"),
        seller_id: None,
    }
}

fn property_from_serving_entity(
    entity: &ServingEntityRecord,
    fact_index: &ServingFactIndex,
    bundle_version: &str,
) -> Property {
    let rows = fact_index.entity(&entity.entity_id);
    let id = strip_entity_prefix(&entity.entity_id, "property:");
    let society_id = derive_society_id(&id);
    let area: String = latest_text(rows, "area").unwrap_or_default();
    let area_slug = slug(&area);
    let bhk = latest_numeric(rows, "bhk").unwrap_or(0.0).round().max(0.0) as u32;
    let mut price = latest_numeric(rows, "price")
        .unwrap_or(0.0)
        .round()
        .max(0.0) as u64;
    let mut carpet_area_sqft = latest_numeric(rows, "carpet_area_sqft")
        .unwrap_or(0.0)
        .round()
        .max(0.0) as u32;

    if let Some(pricing) = market_pricing_for_serving_property(fact_index, &id, bhk) {
        let price_confidence = latest_confidence(rows, "price").unwrap_or(0.0);
        let sqft_confidence = latest_confidence(rows, "carpet_area_sqft").unwrap_or(0.0);
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
        latest_numeric(rows, "price_per_sqft")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u64
    };

    let title = latest_text(rows, "title")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| entity.name.clone());
    let builder_name = latest_text(rows, "builder_name").unwrap_or_default();
    let description_summary = latest_text(rows, "description_summary").unwrap_or_else(|| {
        let project_name = project_name_from_title_or_id(&title, &id, bhk);
        if builder_name.is_empty() && area.is_empty() {
            project_name
        } else if builder_name.is_empty() {
            format!("{project_name} in {area}")
        } else if area.is_empty() {
            format!("{project_name} by {builder_name}")
        } else {
            format!("{project_name} by {builder_name} in {area}")
        }
    });

    let root_source = entity.root_source.as_deref().unwrap_or("serving_bundle");
    let mut transparency_tags = latest_tags(rows, "transparency_tags").unwrap_or_default();
    if transparency_tags.is_empty() {
        transparency_tags.push(format!("Source: {root_source}"));
        transparency_tags.push("Lake indexed".to_string());
    }
    let possession_status = latest_text(rows, "possession_status")
        .or_else(|| serving_society_text(fact_index, &society_id, "market_project_status"))
        .unwrap_or_else(|| "unknown".to_string());

    Property {
        id,
        title,
        area: area.clone(),
        area_id: format!("area-{area_slug}"),
        city: latest_text(rows, "city").unwrap_or_else(|| "Bengaluru".to_string()),
        society_id,
        builder_name,
        property_type: latest_text(rows, "property_type")
            .unwrap_or_else(|| "Apartment".to_string()),
        listing_type: latest_text(rows, "listing_type").unwrap_or_else(|| "Resale".to_string()),
        bhk,
        price,
        price_per_sqft,
        carpet_area_sqft,
        super_builtup_sqft: latest_numeric(rows, "super_builtup_sqft")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        floor: latest_numeric(rows, "floor")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        total_floors: latest_numeric(rows, "total_floors")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        facing: latest_text(rows, "facing").unwrap_or_else(|| "Not specified".to_string()),
        possession_status,
        metro_distance_mins: latest_numeric(rows, "metro_distance_mins")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        maintenance_cost_monthly: latest_numeric(rows, "maintenance_cost_monthly")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        society_quality_score: latest_numeric(rows, "society_quality_score"),
        builder_quality_score: latest_numeric(rows, "builder_quality_score"),
        document_completeness_score: latest_numeric(rows, "document_completeness_score"),
        litigation_risk: latest_numeric(rows, "litigation_risk"),
        noise_score: latest_numeric(rows, "noise_score"),
        sunlight_score: latest_numeric(rows, "sunlight_score"),
        airport_noise_score: latest_numeric(rows, "airport_noise_score"),
        waterlogging_risk_score: latest_numeric(rows, "waterlogging_risk_score"),
        traffic_score: latest_numeric(rows, "traffic_score"),
        days_on_market: latest_numeric(rows, "days_on_market")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        greenery_score: latest_numeric(rows, "greenery_score"),
        open_space_score: latest_numeric(rows, "open_space_score"),
        resale_strength_score: latest_numeric(rows, "resale_strength_score"),
        interest_level: latest_text(rows, "interest_level"),
        saves_last_7d: latest_numeric(rows, "saves_last_7d").map(|value| value.round() as u32),
        offers_last_7d: latest_numeric(rows, "offers_last_7d").map(|value| value.round() as u32),
        images: latest_tags(rows, "images").unwrap_or_default(),
        hero_image: latest_text(rows, "hero_image").unwrap_or_default(),
        description_summary,
        transparency_tags,
        source_reference: format!("search_serving_bundle:{bundle_version}"),
        seller_id: latest_text(rows, "seller_id").filter(|value| !value.trim().is_empty()),
    }
}

pub fn societies_from_serving_bundle(bundle: &LoadedServingBundle) -> Vec<Society> {
    let mut societies = bundle
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| society_from_serving_entity(entity, &bundle.fact_index))
        .collect::<Vec<_>>();
    societies.sort_by(|left, right| left.id.cmp(&right.id));
    societies.dedup_by(|left, right| left.id == right.id);
    societies
}

fn society_from_serving_entity(
    entity: &ServingEntityRecord,
    fact_index: &ServingFactIndex,
) -> Society {
    let rows = fact_index.entity(&entity.entity_id);
    let id = society_runtime_id(entity);
    let google_place_id = latest_text(rows, "google_place_id");
    Society {
        id,
        name: entity.name.clone(),
        area: latest_text(rows, "area").unwrap_or_default(),
        city: latest_text(rows, "city").unwrap_or_else(|| "Bengaluru".to_string()),
        builder_name: latest_text(rows, "builder_name")
            .or_else(|| latest_text(rows, "rera_promoter_name"))
            .unwrap_or_default(),
        year_built: latest_numeric(rows, "year_built")
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        total_units: latest_numeric(rows, "market_total_units")
            .or_else(|| latest_numeric(rows, "rera_total_units"))
            .or_else(|| latest_numeric(rows, "total_units"))
            .unwrap_or(0.0)
            .round()
            .max(0.0) as u32,
        summary: latest_text(rows, "summary")
            .or_else(|| latest_text(rows, "community_review_summary"))
            .unwrap_or_default(),
        maintenance_sentiment: latest_text(rows, "maintenance_sentiment")
            .or_else(|| latest_text(rows, "community_review_summary"))
            .or_else(|| latest_text(rows, "google_sentiment"))
            .unwrap_or_default(),
        livability_sentiment: latest_text(rows, "livability_sentiment")
            .or_else(|| latest_text(rows, "community_review_summary"))
            .unwrap_or_default(),
        common_positives: latest_tags(rows, "community_positive_themes")
            .or_else(|| latest_tags(rows, "google_top_positives"))
            .unwrap_or_default(),
        common_complaints: latest_tags(rows, "community_concern_themes")
            .or_else(|| latest_tags(rows, "google_top_negatives"))
            .unwrap_or_default(),
        review_summary: latest_text(rows, "community_review_summary")
            .or_else(|| latest_text(rows, "google_sentiment"))
            .or_else(|| latest_text(rows, "google_common_themes"))
            .unwrap_or_default(),
        google_reviews_url: latest_text(rows, "google_reviews_url"),
        future_google_place_name: entity.name.clone(),
        future_google_place_id: google_place_id,
        future_review_enrichment_status: "serving_bundle".to_string(),
    }
}

fn areas_from_serving_properties(properties: &[Property]) -> Vec<AreaProfile> {
    let mut by_area = BTreeMap::<String, Vec<&Property>>::new();
    for property in properties {
        if !property.area.trim().is_empty() {
            by_area
                .entry(property.area.trim().to_string())
                .or_default()
                .push(property);
        }
    }

    by_area
        .into_iter()
        .map(|(area, properties)| {
            let mut prices = properties
                .iter()
                .filter_map(|property| {
                    (property.price_per_sqft > 0).then_some(property.price_per_sqft)
                })
                .collect::<Vec<_>>();
            prices.sort_unstable();
            let median_price_per_sqft = median_u64(&prices).unwrap_or(0);
            let (low, high) = match (prices.first(), prices.last()) {
                (Some(low), Some(high)) => (*low, *high),
                _ => (0, 0),
            };
            let city = properties
                .iter()
                .find_map(|property| (!property.city.is_empty()).then_some(property.city.clone()))
                .unwrap_or_else(|| "Bengaluru".to_string());
            AreaProfile {
                id: format!("area-{}", slug(&area)),
                name: area,
                city,
                median_price_per_sqft,
                price_range_per_sqft: PriceRange { low, high },
                trend_direction: String::new(),
                trend_summary: String::new(),
                metro_access_summary: String::new(),
                airport_noise_summary: String::new(),
                traffic_summary: String::new(),
                waterlogging_summary: String::new(),
                livability_summary: String::new(),
                externality_tags: Vec::new(),
                infrastructure_tags: Vec::new(),
                reddit_signals: RedditSignals {
                    decision_drivers: Vec::new(),
                    recurring_concerns: Vec::new(),
                    sentiment_label: String::new(),
                    last_updated: String::new(),
                },
                community_notes: String::new(),
                sample_size: properties.len() as u32,
                last_updated: chrono::Utc::now().to_rfc3339(),
            }
        })
        .collect()
}

fn median_u64(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    Some(values[values.len() / 2])
}

fn market_pricing_for_serving_property(
    fact_index: &ServingFactIndex,
    property_id: &str,
    bhk: u32,
) -> Option<MarketPricing> {
    if bhk == 0 {
        return None;
    }
    let society_id = derive_society_id(property_id);
    serving_society_text(fact_index, &society_id, &format!("listing_{}bhk", bhk))
        .and_then(|listing| parse_listing_pricing(&listing))
        .or_else(|| {
            let pricing =
                serving_society_text(fact_index, &society_id, &format!("pricing_{}bhk", bhk))?;
            parse_market_pricing(&pricing)
        })
}

fn serving_market_pricing(rows: &ServingEntityFactRows, bhk: u32) -> Option<MarketPricing> {
    latest_text(Some(rows), &format!("listing_{}bhk", bhk))
        .and_then(|listing| parse_listing_pricing(&listing))
        .or_else(|| {
            let pricing = latest_text(Some(rows), &format!("pricing_{}bhk", bhk))?;
            parse_market_pricing(&pricing)
        })
}

fn serving_society_bhks(rows: &ServingEntityFactRows) -> Vec<u32> {
    let mut bhks = BTreeSet::new();
    for fact in &rows.facts {
        if let Some(bhk) = bhk_from_serving_fact_key(&fact.fact_key) {
            bhks.insert(bhk);
        }
        if fact.fact_key == "market_bhk_options" {
            for bhk in bhks_from_text(&fact_to_text(&fact.value)) {
                bhks.insert(bhk);
            }
        }
    }
    if bhks.is_empty() {
        bhks.insert(3);
    }
    bhks.into_iter().take(3).collect()
}

fn bhk_from_serving_fact_key(fact_key: &str) -> Option<u32> {
    let suffix = fact_key
        .strip_prefix("listing_")
        .or_else(|| fact_key.strip_prefix("pricing_"))?;
    let digits = suffix.strip_suffix("bhk")?;
    digits
        .parse::<u32>()
        .ok()
        .filter(|value| (1..=6).contains(value))
}

fn bhks_from_text(value: &str) -> Vec<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse::<u32>().ok())
        .filter(|value| (1..=6).contains(value))
        .collect()
}

fn fact_to_text(value: &FactValue) -> String {
    match value {
        FactValue::Text(value) => value.clone(),
        FactValue::Tags(values) => values.join(" "),
        FactValue::Numeric(value) => value.to_string(),
        FactValue::Bool(value) => value.to_string(),
        FactValue::Score { explanation, .. } => explanation.clone(),
    }
}

fn serving_society_text(
    fact_index: &ServingFactIndex,
    society_id: &str,
    fact_key: &str,
) -> Option<String> {
    let entity_id = society_entity_id(society_id);
    let rows = fact_index.entity(&entity_id)?;
    latest_text(Some(rows), fact_key)
}

fn society_entity_id(society_id: &str) -> String {
    let normalized = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    if normalized.starts_with("society:") {
        normalized
    } else {
        format!(
            "society:{}",
            normalized.strip_prefix("soc-").unwrap_or(&normalized)
        )
    }
}

fn society_runtime_id(entity: &ServingEntityRecord) -> String {
    society_runtime_id_from_parts(&entity.entity_id, &entity.name)
}

fn society_runtime_id_from_parts(entity_id: &str, name: &str) -> String {
    let name_slug = slug(name);
    if !name_slug.is_empty() {
        format!("soc-{name_slug}")
    } else {
        format!("soc-{}", strip_entity_prefix(entity_id, "society:"))
    }
}

fn strip_entity_prefix(value: &str, prefix: &str) -> String {
    value.strip_prefix(prefix).unwrap_or(value).to_string()
}

fn project_name_from_title_or_id(title: &str, property_id: &str, bhk: u32) -> String {
    let prefix = format!("{bhk} BHK in ");
    title
        .strip_prefix(&prefix)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let slug = property_id
                .strip_prefix("discovered-")
                .unwrap_or(property_id)
                .trim_end_matches(&format!("-{bhk}bhk"));
            title_case_slug(slug)
        })
}

fn resolution_policies() -> &'static ResolutionPoliciesFile {
    static POLICIES: OnceLock<ResolutionPoliciesFile> = OnceLock::new();
    POLICIES.get_or_init(|| {
        load_resolution_policies().unwrap_or_else(|_| ResolutionPoliciesFile {
            version: 1,
            default_strategy: None,
            source_tiers: Vec::new(),
            never_default_fact_prefixes: Vec::new(),
            source_caps: HashMap::new(),
        })
    })
}

fn latest_fact<'a>(
    rows: Option<&'a ServingEntityFactRows>,
    fact_key: &str,
) -> Option<&'a crate::serving::ServingFactRecord> {
    let policies = resolution_policies();
    rows?.facts
        .iter()
        .filter(|fact| {
            fact.fact_key == fact_key
                && buyer_visible_fact(&fact.fact_key, &fact.source_type, policies)
        })
        .max_by(|left, right| {
            if better_source_type(
                &left.source_type,
                &right.source_type,
                left.confidence,
                right.confidence,
                policies,
            ) {
                std::cmp::Ordering::Greater
            } else if better_source_type(
                &right.source_type,
                &left.source_type,
                right.confidence,
                left.confidence,
                policies,
            ) {
                std::cmp::Ordering::Less
            } else {
                left.learned_at.cmp(&right.learned_at)
            }
        })
}

fn latest_text(rows: Option<&ServingEntityFactRows>, fact_key: &str) -> Option<String> {
    latest_fact(rows, fact_key).and_then(|fact| match &fact.value {
        FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        FactValue::Numeric(value) if value.is_finite() => Some(value.to_string()),
        FactValue::Bool(value) => Some(value.to_string()),
        FactValue::Score { value, .. } if value.is_finite() => Some(value.to_string()),
        FactValue::Tags(values) if !values.is_empty() => Some(values.join(", ")),
        _ => None,
    })
}

fn latest_numeric(rows: Option<&ServingEntityFactRows>, fact_key: &str) -> Option<f64> {
    latest_fact(rows, fact_key).and_then(|fact| match &fact.value {
        FactValue::Numeric(value) if value.is_finite() => Some(*value),
        FactValue::Score { value, .. } if value.is_finite() => Some(*value),
        _ => None,
    })
}

fn latest_bool(rows: Option<&ServingEntityFactRows>, fact_key: &str) -> Option<bool> {
    latest_fact(rows, fact_key).and_then(|fact| match &fact.value {
        FactValue::Bool(value) => Some(*value),
        _ => None,
    })
}

fn latest_tags(rows: Option<&ServingEntityFactRows>, fact_key: &str) -> Option<Vec<String>> {
    latest_fact(rows, fact_key).and_then(|fact| match &fact.value {
        FactValue::Tags(values) => Some(
            values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
        .filter(|values| !values.is_empty()),
        FactValue::Text(value) if !value.trim().is_empty() => Some(vec![value.trim().to_string()]),
        _ => None,
    })
}

fn latest_confidence(rows: Option<&ServingEntityFactRows>, fact_key: &str) -> Option<f32> {
    latest_fact(rows, fact_key).map(|fact| fact.confidence)
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut pending_dash = false;
    for character in value.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !output.is_empty() {
                output.push('-');
            }
            output.push(character);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    output
}

fn title_case_slug(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

            let google_place_id: Option<String> = fact_text(node, "google_place_id").into_option();
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
                google_reviews_url: google_reviews_url_from_facts(&node.facts, &node.name),
                future_google_place_name: node.name.clone(),
                future_google_place_id: google_place_id,
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
#[cfg(test)]
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
                society_quality_score: optional_fact_numeric(node, "society_quality_score"),
                builder_quality_score: optional_fact_numeric(node, "builder_quality_score"),
                document_completeness_score: optional_fact_numeric(node, "document_completeness_score"),
                litigation_risk: optional_fact_numeric(node, "litigation_risk"),
                noise_score: optional_fact_numeric(node, "noise_score"),
                sunlight_score: optional_fact_numeric(node, "sunlight_score"),
                airport_noise_score: optional_fact_numeric(node, "airport_noise_score"),
                waterlogging_risk_score: optional_fact_numeric(node, "waterlogging_risk_score"),
                traffic_score: optional_fact_numeric(node, "traffic_score"),
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
    price: u64,
    price_low: u64,
    price_high: u64,
    sqft: u32,
    sqft_low: u32,
    sqft_high: u32,
}

impl MarketPricing {
    fn representative_price(&self) -> u64 {
        self.price
    }

    fn representative_sqft(&self) -> u32 {
        self.sqft
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
    let listing_text: String = fact_text(society_node, &format!("listing_{}bhk", bhk)).into();
    parse_listing_pricing(&listing_text).or_else(|| {
        let pricing_text: String = fact_text(society_node, &format!("pricing_{}bhk", bhk)).into();
        parse_market_pricing(&pricing_text)
    })
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
        price: (((price_low_lakh + price_high_lakh) / 2.0) * 100_000.0).round() as u64,
        price_low: (price_low_lakh * 100_000.0).round() as u64,
        price_high: (price_high_lakh * 100_000.0).round() as u64,
        sqft: ((sqft_low + sqft_high) / 2.0).round() as u32,
        sqft_low: sqft_low.round() as u32,
        sqft_high: sqft_high.round() as u32,
    })
}

fn parse_listing_pricing(raw: &str) -> Option<MarketPricing> {
    if raw.trim().is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let price = value.get("price")?.as_f64()?;
    let sqft = value.get("area_sqft")?.as_f64()?;
    if !price.is_finite() || !sqft.is_finite() || price <= 0.0 || sqft <= 0.0 {
        return None;
    }
    let price_low = value
        .get("price_min")
        .and_then(|value| value.as_f64())
        .unwrap_or(price);
    let price_high = value
        .get("price_max")
        .and_then(|value| value.as_f64())
        .unwrap_or(price);
    let sqft_low = value
        .get("area_sqft_min")
        .and_then(|value| value.as_f64())
        .unwrap_or(sqft);
    let sqft_high = value
        .get("area_sqft_max")
        .and_then(|value| value.as_f64())
        .unwrap_or(sqft);
    if !price_low.is_finite()
        || !price_high.is_finite()
        || !sqft_low.is_finite()
        || !sqft_high.is_finite()
        || price_low <= 0.0
        || price_high <= 0.0
        || sqft_low <= 0.0
        || sqft_high <= 0.0
    {
        return None;
    }

    Some(MarketPricing {
        price: price.round() as u64,
        price_low: price_low.round() as u64,
        price_high: price_high.round() as u64,
        sqft: sqft.round() as u32,
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

    fn into_option(self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(self.0)
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
    optional_fact_numeric(node, key).unwrap_or(0.0)
}

fn optional_fact_numeric(node: &knowledge::node::Node, key: &str) -> Option<f64> {
    node.get_fact(key).and_then(|fact| match &fact.value {
        FactValue::Numeric(value) if value.is_finite() => Some(*value),
        FactValue::Score { value, .. } if value.is_finite() => Some(*value),
        _ => None,
    })
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
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[tokio::test]
    async fn explicit_lake_requires_a_promoted_serving_bundle() {
        let root = tempdir().unwrap();
        let lake_location = LakeStoreLocation::Local(root.path().join("lake"));

        let err = match load_serving_bundle_from_location(root.path(), lake_location.clone(), true)
            .await
        {
            Ok(_) => panic!("explicit lake without a promoted bundle should fail"),
            Err(err) => err,
        };
        assert!(err.contains("no promoted search serving bundle found"));

        assert!(
            load_serving_bundle_from_location(root.path(), lake_location, false)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn serving_records_project_runtime_property_with_alias_society_facts() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "property:discovered-prestige-lavender-fields-3bhk".to_string(),
                entity_type: "property".to_string(),
                name: "3 BHK in Prestige Lavender Fields".to_string(),
                root_source: Some("discovered".to_string()),
                searchable_text: String::new(),
            },
            ServingEntityRecord {
                entity_id: "society:rera-a19f2cf2456fc549".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Lavender Fields".to_string(),
                root_source: Some("rera".to_string()),
                searchable_text: String::new(),
            },
        ];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "title",
                    FactValue::Text("3 BHK in Prestige Lavender Fields".to_string()),
                    0.8,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "area",
                    FactValue::Text("Varthur".to_string()),
                    0.8,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "city",
                    FactValue::Text("Bengaluru".to_string()),
                    0.8,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "builder_name",
                    FactValue::Text("Prestige Group".to_string()),
                    0.8,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "bhk",
                    FactValue::Numeric(3.0),
                    0.8,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "price",
                    FactValue::Numeric(1.0),
                    0.6,
                ),
                serving_fact(
                    "property:discovered-prestige-lavender-fields-3bhk",
                    "carpet_area_sqft",
                    FactValue::Numeric(1.0),
                    0.6,
                ),
                serving_fact(
                    "society:prestige-lavender-fields",
                    "pricing_3bhk",
                    FactValue::Text(
                        r#"{"price_range_lakh":"240-360","sqft_range":"1800-2200"}"#.to_string(),
                    ),
                    0.95,
                ),
                serving_fact(
                    "society:prestige-lavender-fields",
                    "market_project_status",
                    FactValue::Text("Sold Out".to_string()),
                    0.95,
                ),
                serving_fact(
                    "society:rera-a19f2cf2456fc549",
                    "community_review_summary",
                    FactValue::Text("Google signal is mixed-positive.".to_string()),
                    0.85,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        assert_eq!(properties.len(), 1);
        let property = &properties[0];
        assert_eq!(property.id, "discovered-prestige-lavender-fields-3bhk");
        assert_eq!(property.society_id, "soc-prestige-lavender-fields");
        assert_eq!(property.area, "Varthur");
        assert_eq!(property.price, 30_000_000);
        assert_eq!(property.carpet_area_sqft, 2_000);
        assert_eq!(property.possession_status, "Sold Out");
        assert_eq!(property.source_reference, "search_serving_bundle:bundle-v1");

        let society = society_from_serving_entity(&entities[1], &fact_index);
        assert_eq!(society.id, "soc-prestige-lavender-fields");
        assert_eq!(society.review_summary, "Google signal is mixed-positive.");
    }

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
        // Missing quality/risk scores stay absent — no bootstrap defaults.
        assert!(p.society_quality_score.is_none());
        assert!(p.litigation_risk.is_none());
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
    fn test_low_confidence_property_prefers_external_listing_pricing() {
        let mut graph = KnowledgeGraph::new();
        let mut society = make_society_node(
            "prestige-raintree-park",
            "Prestige Raintree Park",
            "Whitefield",
            "Prestige Group",
        );
        society.add_facts(vec![
            SourcedFact::manual(
                "listing_3bhk",
                FactValue::Text(r#"{"price":31000000,"area_sqft":1900}"#.into()),
            ),
            SourcedFact::manual(
                "pricing_3bhk",
                FactValue::Text(
                    r#"{"bhk":"3BHK","price_range_lakh":"259-353","sqft_range":"2004-2482"}"#
                        .into(),
                ),
            ),
        ]);
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
        assert_eq!(p.price, 31_000_000);
        assert_eq!(p.carpet_area_sqft, 1900);
        assert_eq!(p.price_per_sqft, 16_315);
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

    fn serving_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        confidence: f32,
    ) -> crate::serving::ServingFactRecord {
        crate::serving::ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: match &value {
                FactValue::Text(_) => "text",
                FactValue::Numeric(_) => "numeric",
                FactValue::Bool(_) => "bool",
                FactValue::Tags(_) => "tags",
                FactValue::Score { .. } => "score",
            }
            .to_string(),
            value_text: None,
            value,
            confidence,
            source_type: "Computed".to_string(),
            source_url: None,
            model: None,
            skill_id: Some("test".to_string()),
            learned_at: Utc.timestamp_opt(1, 0).unwrap(),
        }
    }
}
