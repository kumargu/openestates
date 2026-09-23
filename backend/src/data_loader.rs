use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::dag_config::{
    better_source_type_for_fact, buyer_visible_fact, load_resolution_policies,
    ResolutionPoliciesFile,
};
use crate::knowledge::fact::FactValue;
use crate::models::{Property, Society};
use crate::search::SearchIndex;
use crate::security::ExecutionLanes;
use crate::serving::{
    LoadedServingBundle, ServingBundleLoader, ServingEntityFactRows, ServingEntityRecord,
    ServingFactIndex,
};
use crate::state::{
    search_log_queue_capacity_from_env, spawn_search_log_worker, AppState, SearchResponseCache,
    SearchRuntimeSnapshot,
};
use crate::{catalog::CatalogStore, lake::LakeStoreLocation, serving::ServingBundleLoadError};

pub type RuntimeServingSnapshot = SearchRuntimeSnapshot;

/// Load all data and construct the full AppState.
///
/// The active catalog serving bundle is the canonical request-path data source.
/// Legacy `data/knowledge` JSON is intentionally not loaded into runtime state.
pub async fn load_app_state(project_root: &Path) -> AppState {
    load_app_state_with_execution(project_root, ExecutionLanes::current()).await
}

pub async fn load_app_state_with_execution(
    project_root: &Path,
    execution: ExecutionLanes,
) -> AppState {
    let bundle = load_serving_bundle(project_root)
        .await
        .unwrap_or_else(|err| panic!("Serving bundle startup contract failed: {err}"));

    let search_runtime = runtime_snapshot_from_serving_bundle(bundle.clone());
    if search_runtime.properties.is_empty() {
        panic!(
            "Serving bundle {} has no property entities; refusing to fall back to legacy data",
            bundle.manifest.bundle_version
        );
    }
    println!(
        "Derived {} properties, {} societies from serving bundle {}",
        search_runtime.properties.len(),
        search_runtime.societies.len(),
        bundle.manifest.bundle_version
    );

    println!("Request-time network AI disabled: search uses only local artifacts");
    let map_overlays = crate::routes::map_overlays::load_city_map_overlays(project_root);
    let (search_event_tx, search_event_rx) = mpsc::channel(search_log_queue_capacity_from_env());
    let search_event_lake = LakeStoreLocation::from_env(project_root)
        .and_then(|location| location.open())
        .unwrap_or_else(|err| panic!("Search event lake startup contract failed: {err}"));
    spawn_search_log_worker(&execution, search_event_lake, search_event_rx);

    AppState {
        execution,
        search_runtime: ArcSwap::from_pointee(search_runtime),
        search_cache: SearchResponseCache::from_env(),
        search_revision_caches: crate::state::SearchRevisionCaches::from_config(),
        property_catalog_cache: tokio::sync::Mutex::new(None),
        search_event_tx,
        search_log_dropped_count: AtomicU64::new(0),
        recommendation_cache: RwLock::new(std::collections::HashMap::new()),

        map_overlays,
        project_root: project_root.to_path_buf(),
        process_started_at: chrono::Utc::now(),
    }
}

pub fn runtime_snapshot_from_serving_bundle(
    bundle: Arc<LoadedServingBundle>,
) -> RuntimeServingSnapshot {
    let properties = properties_from_serving_bundle(&bundle);
    let societies = societies_from_serving_bundle(&bundle);
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    SearchRuntimeSnapshot::new(bundle, properties, societies, search_index)
}

pub async fn load_serving_bundle(project_root: &Path) -> Result<Arc<LoadedServingBundle>, String> {
    let lake_location = LakeStoreLocation::from_env(project_root).map_err(|err| err.to_string())?;
    load_serving_bundle_from_location(project_root, lake_location).await
}

async fn load_serving_bundle_from_location(
    project_root: &Path,
    lake_location: LakeStoreLocation,
) -> Result<Arc<LoadedServingBundle>, String> {
    let cache_root = project_root.join("data").join("cache").join("serving");
    let lake = lake_location
        .open()
        .map_err(|err| format!("lake unavailable at {lake_location}: {err}"))?;

    let loader = ServingBundleLoader::new(lake, cache_root);
    match load_selected_search_bundle(&loader).await {
        Ok(bundle) => {
            println!(
                "Loaded serving bundle {} with {} entities and {} facts",
                bundle.manifest.bundle_version,
                bundle.manifest.entity_count,
                bundle.manifest.fact_count
            );
            Ok(Arc::new(bundle))
        }
        Err(err) => Err(format!(
            "failed to load promoted search serving bundle from {lake_location}: {err}"
        )),
    }
}

async fn load_selected_search_bundle(
    loader: &ServingBundleLoader,
) -> Result<LoadedServingBundle, ServingBundleLoadError> {
    let pointer = CatalogStore::new(loader.lake().clone())
        .pointer()
        .await
        .map_err(|error| ServingBundleLoadError::Configuration(error.to_string()))?
        .ok_or_else(|| {
            ServingBundleLoadError::Configuration(
                "the dev catalog has no serving bundle; run openestates-catalog rebuild"
                    .to_string(),
            )
        })?;
    loader
        .load_search_bundle(&pointer.current.bundle_version)
        .await
}

pub fn properties_from_serving_bundle(bundle: &LoadedServingBundle) -> Vec<Property> {
    properties_from_serving_records_with_edges(
        &bundle.entities,
        &bundle.edges,
        &bundle.fact_index,
        bundle.manifest.proof_snapshot_identity(),
    )
}

pub fn properties_from_serving_records(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    bundle_version: &str,
) -> Vec<Property> {
    properties_from_serving_records_with_edges(entities, &[], fact_index, bundle_version)
}

pub(crate) fn properties_from_serving_records_with_edges(
    entities: &[ServingEntityRecord],
    edges: &[crate::serving::ServingEdgeRecord],
    fact_index: &ServingFactIndex,
    bundle_version: &str,
) -> Vec<Property> {
    let area_lookup = ServingAreaLookup::new(entities, edges);
    let mut properties = entities
        .iter()
        .filter(|entity| entity.entity_type == "property")
        .filter_map(|entity| {
            let societies = edges
                .iter()
                .filter(|edge| {
                    edge.from_entity_id == entity.entity_id && edge.edge_type == "in_society"
                })
                .map(|edge| edge.to_entity_id.as_str())
                .collect::<BTreeSet<_>>();
            if societies.len() != 1 {
                return None;
            }
            Some(property_from_serving_entity(
                entity,
                societies.into_iter().next().unwrap(),
                fact_index,
                &area_lookup,
                bundle_version,
            ))
        })
        .collect::<Vec<_>>();
    properties.extend(representative_properties_from_serving_societies(
        entities,
        fact_index,
        &area_lookup,
        bundle_version,
    ));
    for property in &mut properties {
        let subject = crate::routes::enrichment::society_node_id(&property.society_id);
        let canonical = fact_index
            .entity(&subject)
            .and_then(|rows| rows.facts.first())
            .map(|fact| fact.entity_id.as_str())
            .unwrap_or(&subject);
        let inventory = crate::search::InventoryOption::from_serving_observation(
            property,
            canonical,
            fact_index,
            bundle_version,
        );
        if let Some(inventory) = inventory {
            property.price = inventory.price_min.unwrap_or(0);
            property.price_min = None;
            property.price_max = None;
            property.carpet_area_sqft = 0;
            property.super_builtup_sqft = 0;
            property.price_per_sqft = inventory
                .size_sqft
                .filter(|v| *v > 0)
                .map(|size| property.price / u64::from(size))
                .unwrap_or(0);
            property.area_measurement = inventory.area_measurement;
        } else {
            property.bhk = 0;
            property.price = 0;
            property.price_min = None;
            property.price_max = None;
            property.price_per_sqft = 0;
            property.carpet_area_sqft = 0;
            property.super_builtup_sqft = 0;
            property.area_measurement = None;
            if let Some(entity) = entities.iter().find(|entity| entity.entity_id == canonical) {
                property.title = entity.name.clone();
            }
        }
    }
    properties.retain(|property| property.is_listable());
    properties.sort_by(|left, right| left.id.cmp(&right.id));
    properties.dedup_by(|left, right| left.id == right.id);
    properties
}

fn representative_properties_from_serving_societies(
    entities: &[ServingEntityRecord],
    fact_index: &ServingFactIndex,
    area_lookup: &ServingAreaLookup,
    bundle_version: &str,
) -> Vec<Property> {
    let entities_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<BTreeMap<_, _>>();

    fact_index
        .rows()
        .filter(|(entity_id, rows)| {
            entities_by_id
                .get(entity_id)
                .is_some_and(|entity| entity.entity_type == "society")
                && !rows.facts.is_empty()
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
                        area_lookup,
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
    area_lookup: &ServingAreaLookup,
    bhk: u32,
    bundle_version: &str,
) -> Property {
    let society_name = entity
        .map(|entity| entity.name.clone())
        .or_else(|| latest_text(Some(rows), "title"))
        .unwrap_or_else(|| title_case_slug(strip_entity_prefix(entity_id, "society:").as_str()));
    let society_id = entity_id.to_string();
    let society_slug = society_id.strip_prefix("society:").unwrap_or(&society_id);
    let id = if bhk > 0 {
        format!("discovered-{society_slug}-{bhk}bhk")
    } else {
        format!("discovered-{society_slug}")
    };
    let area = resolve_serving_society_area(Some(rows), area_lookup, entity_id);
    let area_id = area_lookup
        .area_id_by_society
        .get(&society_id)
        .cloned()
        .unwrap_or_default();
    let builder_name = latest_text(Some(rows), "builder_name")
        .or_else(|| latest_text(Some(rows), "rera_promoter_name"))
        .unwrap_or_default();

    Property {
        id,
        title: if bhk > 0 {
            format!("{bhk} BHK in {society_name}")
        } else {
            society_name.clone()
        },
        area: area.clone(),
        area_id,
        city: latest_text(Some(rows), "city").unwrap_or_else(|| "Bengaluru".to_string()),
        society_id,
        builder_name,
        property_type: latest_text(Some(rows), "rera_project_type")
            .unwrap_or_else(|| "Apartment".to_string()),
        listing_type: "Project".to_string(),
        bhk,
        price: 0,
        price_min: None,
        price_max: None,
        price_per_sqft: 0,
        carpet_area_sqft: 0,
        super_builtup_sqft: 0,
        area_measurement: None,
        floor: 0,
        total_floors: 0,
        facing: "Not specified".to_string(),
        possession_status: latest_text(Some(rows), "rera_status")
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
        source_reference: format!("catalog_bundle:{bundle_version}"),
    }
}

fn resolve_serving_property_area(
    rows: Option<&ServingEntityFactRows>,
    fact_index: &ServingFactIndex,
    area_lookup: &ServingAreaLookup,
    society_id: &str,
) -> String {
    let area = latest_text(rows, "area").unwrap_or_default();
    if !area.trim().is_empty() {
        return area;
    }
    let society_entity_id = society_entity_id(society_id);
    let society_rows = fact_index.entity(&society_entity_id);
    resolve_serving_society_area(society_rows, area_lookup, &society_entity_id)
}

fn property_from_serving_entity(
    entity: &ServingEntityRecord,
    society_entity_id: &str,
    fact_index: &ServingFactIndex,
    area_lookup: &ServingAreaLookup,
    bundle_version: &str,
) -> Property {
    let rows = fact_index.entity(&entity.entity_id);
    let id = strip_entity_prefix(&entity.entity_id, "property:");
    let society_id = society_entity_id.to_string();
    let title = latest_text(rows, "title")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| entity.name.clone());
    let area = resolve_serving_property_area(rows, fact_index, area_lookup, &society_id);
    let area_id = area_lookup
        .area_id_by_society
        .get(&society_id)
        .cloned()
        .unwrap_or_default();
    let bhk = latest_numeric(rows, "bhk").unwrap_or(0.0).round().max(0.0) as u32;
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

    let possession_status = latest_text(rows, "possession_status")
        .or_else(|| serving_society_text(fact_index, &society_id, "rera_status"))
        .unwrap_or_else(|| "unknown".to_string());

    Property {
        id,
        title,
        area: area.clone(),
        area_id,
        city: latest_text(rows, "city").unwrap_or_else(|| "Bengaluru".to_string()),
        society_id,
        builder_name,
        property_type: latest_text(rows, "property_type")
            .unwrap_or_else(|| "Apartment".to_string()),
        listing_type: latest_text(rows, "listing_type").unwrap_or_else(|| "Resale".to_string()),
        bhk,
        price: 0,
        price_min: None,
        price_max: None,
        price_per_sqft: 0,
        carpet_area_sqft: 0,
        super_builtup_sqft: 0,
        area_measurement: None,
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
        source_reference: format!("catalog_bundle:{bundle_version}"),
    }
}

pub fn societies_from_serving_bundle(bundle: &LoadedServingBundle) -> Vec<Society> {
    let mut societies = bundle
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| society_from_serving_entity(entity, &bundle.fact_index, &bundle.edges))
        .collect::<Vec<_>>();
    societies.sort_by(|left, right| left.id.cmp(&right.id));
    societies.dedup_by(|left, right| left.id == right.id);
    societies
}

fn society_from_serving_entity(
    entity: &ServingEntityRecord,
    fact_index: &ServingFactIndex,
    edges: &[crate::serving::ServingEdgeRecord],
) -> Society {
    let rows = fact_index.entity(&entity.entity_id);
    let id = entity.entity_id.clone();
    let google_place_id = latest_text(rows, "google_place_id");
    let area_lookup = ServingAreaLookup::new(std::slice::from_ref(entity), edges);
    Society {
        id,
        name: entity.name.clone(),
        area: resolve_serving_society_area(rows, &area_lookup, &entity.entity_id),
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
        summary: latest_text(rows, "summary").unwrap_or_default(),
        maintenance_sentiment: latest_text(rows, "maintenance_sentiment")
            .or_else(|| latest_text(rows, "google_sentiment"))
            .unwrap_or_default(),
        livability_sentiment: latest_text(rows, "livability_sentiment").unwrap_or_default(),
        common_positives: latest_tags(rows, "google_top_positives").unwrap_or_default(),
        common_complaints: latest_tags(rows, "google_top_negatives").unwrap_or_default(),
        review_summary: latest_text(rows, "google_sentiment")
            .or_else(|| latest_text(rows, "google_common_themes"))
            .unwrap_or_default(),
        google_reviews_url: latest_text(rows, "google_reviews_url"),
        future_google_place_name: entity.name.clone(),
        future_google_place_id: google_place_id,
        future_review_enrichment_status: "serving_bundle".to_string(),
    }
}

#[derive(Debug, Clone, Default)]
struct ServingAreaLookup {
    area_name_by_id: BTreeMap<String, String>,
    area_id_by_society: BTreeMap<String, String>,
}

impl ServingAreaLookup {
    fn new(entities: &[ServingEntityRecord], edges: &[crate::serving::ServingEdgeRecord]) -> Self {
        let area_name_by_id = entities
            .iter()
            .filter(|entity| entity.entity_type == "area")
            .map(|entity| (entity.entity_id.clone(), entity.name.clone()))
            .collect::<BTreeMap<_, _>>();
        let area_id_by_society = edges
            .iter()
            .filter(|edge| edge.edge_type == "in_area")
            .filter(|edge| {
                edge.from_entity_id.starts_with("society:")
                    && edge.to_entity_id.starts_with("area:")
            })
            .map(|edge| (edge.from_entity_id.clone(), edge.to_entity_id.clone()))
            .collect::<BTreeMap<_, _>>();
        Self {
            area_name_by_id,
            area_id_by_society,
        }
    }

    fn society_area(&self, society_entity_id: &str) -> Option<String> {
        let area_id = self.area_id_by_society.get(society_entity_id)?;
        self.area_name_by_id.get(area_id).cloned().or_else(|| {
            area_id
                .strip_prefix("area:")
                .map(title_case_slug)
                .filter(|value| !value.trim().is_empty())
        })
    }
}

fn resolve_serving_society_area(
    rows: Option<&ServingEntityFactRows>,
    area_lookup: &ServingAreaLookup,
    society_entity_id: &str,
) -> String {
    latest_text(rows, "area")
        .or_else(|| latest_text(rows, "listing_locality"))
        .or_else(|| area_lookup.society_area(society_entity_id))
        .unwrap_or_default()
}

fn serving_society_bhks(rows: &ServingEntityFactRows) -> Vec<u32> {
    let mut bhks = rows
        .facts
        .iter()
        .filter(|fact| {
            fact.observation
                .as_ref()
                .is_some_and(|observation| observation.validate().is_ok())
        })
        .filter_map(priced_bhk_from_fact)
        .collect::<BTreeSet<_>>();
    if bhks.is_empty() {
        bhks.insert(0);
    }
    bhks.into_iter().collect()
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

fn priced_bhk_from_fact(fact: &crate::serving::ServingFactRecord) -> Option<u32> {
    bhk_from_serving_fact_key(&fact.fact_key)
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
    if society_id.starts_with("society:") {
        return society_id.to_string();
    }
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
        load_resolution_policies().expect("resolution policies must validate before serving")
    })
}

fn latest_fact<'a>(
    rows: Option<&'a ServingEntityFactRows>,
    fact_key: &str,
) -> Option<&'a crate::serving::ServingFactRecord> {
    let policies = resolution_policies();
    rows?
        .facts
        .iter()
        .filter(|fact| {
            fact.fact_key == fact_key
                && buyer_visible_fact(&fact.fact_key, &fact.source_type, policies)
        })
        .max_by(|left, right| {
            if better_source_type_for_fact(
                Some(fact_key),
                &left.source_type,
                &right.source_type,
                left.confidence,
                right.confidence,
                policies,
            ) {
                std::cmp::Ordering::Greater
            } else if better_source_type_for_fact(
                Some(fact_key),
                &right.source_type,
                &left.source_type,
                right.confidence,
                left.confidence,
                policies,
            ) {
                std::cmp::Ordering::Less
            } else {
                right
                    .stable_selection_key()
                    .cmp(&left.stable_selection_key())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[tokio::test]
    async fn explicit_lake_requires_a_promoted_serving_bundle() {
        let root = tempdir().unwrap();
        let lake_location = LakeStoreLocation::Local(root.path().join("lake"));

        let err = match load_serving_bundle_from_location(root.path(), lake_location.clone()).await
        {
            Ok(_) => panic!("explicit lake without a promoted bundle should fail"),
            Err(err) => err,
        };
        assert!(err.contains("the dev catalog has no serving bundle"));

        assert!(
            load_serving_bundle_from_location(root.path(), lake_location)
                .await
                .is_err()
        );
    }

    #[test]
    fn serving_records_project_runtime_property_with_explicit_society_membership() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "property:discovered-prestige-lavender-fields-3bhk".to_string(),
                entity_type: "property".to_string(),
                name: "3 BHK in Prestige Lavender Fields".to_string(),
                root_source: Some("discovered".to_string()),
                visibility: Default::default(),
                searchable_text: String::new(),
            },
            ServingEntityRecord {
                entity_id: "society:rera-a19f2cf2456fc549".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Lavender Fields".to_string(),
                root_source: Some("rera".to_string()),
                visibility: Default::default(),
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
                    "society:rera-a19f2cf2456fc549",
                    "pricing_3bhk",
                    FactValue::Text(
                        r#"{"price_range_lakh":"240-360","sqft_range":"1800-2200"}"#.to_string(),
                    ),
                    0.95,
                ),
                serving_fact(
                    "society:rera-a19f2cf2456fc549",
                    "rera_status",
                    FactValue::Text("Completed".to_string()),
                    0.95,
                ),
                serving_fact(
                    "society:rera-a19f2cf2456fc549",
                    "google_sentiment",
                    FactValue::Text("Google signal is mixed-positive.".to_string()),
                    0.85,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records_with_edges(
            &entities,
            &[membership(
                "property:discovered-prestige-lavender-fields-3bhk",
                "society:rera-a19f2cf2456fc549",
            )],
            &fact_index,
            "bundle-v1",
        );
        let property = properties
            .iter()
            .find(|property| property.id == "discovered-prestige-lavender-fields-3bhk")
            .unwrap();
        assert_eq!(property.id, "discovered-prestige-lavender-fields-3bhk");
        assert_eq!(property.society_id, "society:rera-a19f2cf2456fc549");
        assert_eq!(property.area, "Varthur");
        assert_eq!(property.price, 0);
        assert_eq!(property.price_min, None);
        assert_eq!(property.price_max, None);
        assert_eq!(property.carpet_area_sqft, 0);
        assert_eq!(property.possession_status, "Completed");
        assert_eq!(property.source_reference, "catalog_bundle:bundle-v1");

        let society = society_from_serving_entity(&entities[1], &fact_index, &[]);
        assert_eq!(society.id, "society:rera-a19f2cf2456fc549");
        assert_eq!(society.review_summary, "Google signal is mixed-positive.");
    }

    #[test]
    fn one_selected_listing_owns_display_and_search_measurements() {
        let subject = "society:record-selection";
        let entity = ServingEntityRecord {
            entity_id: subject.into(),
            entity_type: "society".into(),
            name: "Record Selection".into(),
            root_source: None,
            visibility: Default::default(),
            searchable_text: String::new(),
        };
        let strong = serving_fact_with_source(
            subject,
            "listing_3bhk",
            FactValue::Text(listing_payload(10_000_000.0, 900.0)),
            0.95,
            "ExternalListing",
        );
        let mut weak = serving_fact_with_source(
            subject,
            "listing_3bhk",
            FactValue::Text(listing_payload(10_000_000.0, 1800.0)),
            0.65,
            "ExternalListing",
        );
        weak.observation = Some(
            crate::serving::SourceObservation::new(
                "ExternalListing",
                "different-listing",
                subject,
                Utc.timestamp_opt(1, 0).unwrap(),
                None,
                vec!["fixture/source-v1".into()],
            )
            .unwrap(),
        );
        for records in [vec![strong.clone(), weak.clone()], vec![weak, strong]] {
            let facts = ServingFactIndex::from_records(records, Vec::new());
            let properties =
                properties_from_serving_records(std::slice::from_ref(&entity), &facts, "snapshot");
            let property = &properties[0];
            let selected = crate::search::InventoryOption::from_serving_observation(
                property, subject, &facts, "snapshot",
            )
            .unwrap();
            assert_eq!(property.area_measurement, selected.area_measurement);
            assert_eq!(property.area_measurement.as_ref().unwrap().value, 900.0);
            assert_eq!(property.price, selected.price_min.unwrap());
            assert_eq!(property.carpet_area_sqft, 0);
            let wire =
                serde_json::to_value(crate::public_contract::PropertyAttributes::from(property))
                    .unwrap();
            assert!(wire.get("carpet_area_sqft").is_none());
        }
    }

    #[test]
    fn project_summary_keeps_society_browseable_without_inventory_claims() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:brigade-lakefront-crimson".to_string(),
            entity_type: "society".to_string(),
            name: "Brigade Lakefront Crimson".to_string(),
            root_source: Some("discovered".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![serving_fact(
                "society:brigade-lakefront-crimson",
                "listing_3bhk",
                FactValue::Text(listing_payload_range(
                    32_250_000.0,
                    30_000_000.0,
                    48_000_000.0,
                    1_800.0,
                )),
                0.8,
            )],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        assert_eq!(properties.len(), 1);
        let property = &properties[0];
        assert_eq!(property.price, 0);
        assert_eq!(property.price_min, None);
        assert_eq!(property.price_max, None);
        assert!(property.area_measurement.is_none());
    }

    #[test]
    fn representative_property_uses_serving_area_edge_when_area_fact_is_missing() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:godrej-splendour".to_string(),
                entity_type: "society".to_string(),
                name: "Godrej Splendour".to_string(),
                root_source: Some("rera".to_string()),
                visibility: Default::default(),
                searchable_text: String::new(),
            },
            ServingEntityRecord {
                entity_id: "area:whitefield".to_string(),
                entity_type: "area".to_string(),
                name: "Whitefield".to_string(),
                root_source: Some("rera".to_string()),
                visibility: Default::default(),
                searchable_text: String::new(),
            },
        ];
        let edges = vec![crate::serving::ServingEdgeRecord {
            from_entity_id: "society:godrej-splendour".to_string(),
            edge_type: "in_area".to_string(),
            to_entity_id: "area:whitefield".to_string(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
            derivation: None,
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:godrej-splendour",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(16_200_000.0, 1_261.0)),
                    0.8,
                ),
                serving_fact(
                    "society:godrej-splendour",
                    "rera_registered",
                    FactValue::Bool(true),
                    1.0,
                ),
            ],
            Vec::new(),
        );

        let properties =
            properties_from_serving_records_with_edges(&entities, &edges, &fact_index, "bundle-v1");

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].area, "Whitefield");
        assert_eq!(properties[0].area_id, "area:whitefield");
    }

    #[test]
    fn display_names_and_slugs_do_not_establish_inventory() {
        let entities = vec![ServingEntityRecord {
            entity_id: "property:discovered-svamitva-soul-spring-3bhk".to_string(),
            entity_type: "property".to_string(),
            name: "3 BHK in Svamitva Soul Spring".to_string(),
            root_source: Some("discovered".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "property:discovered-svamitva-soul-spring-3bhk",
                    "title",
                    FactValue::Text("3 BHK in Svamitva Soul Spring".to_string()),
                    0.8,
                ),
                serving_fact(
                    "society:svamitva-soul-spring",
                    "area",
                    FactValue::Text("Whitefield".to_string()),
                    0.9,
                ),
                serving_fact(
                    "property:discovered-svamitva-soul-spring-3bhk",
                    "price",
                    FactValue::Numeric(18_000_000.0),
                    0.9,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records_with_edges(
            &entities,
            &[membership(
                "property:discovered-svamitva-soul-spring-3bhk",
                "society:svamitva-soul-spring",
            )],
            &fact_index,
            "bundle-v1",
        );
        let property = properties
            .iter()
            .find(|property| property.id == "discovered-svamitva-soul-spring-3bhk")
            .unwrap();
        assert_eq!(property.bhk, 0);
        assert_eq!(property.price, 0);
        assert_eq!(property.area, "Whitefield");
    }

    #[test]
    fn representative_property_uses_listing_locality_when_area_edge_is_missing() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:godrej-splendour".to_string(),
            entity_type: "society".to_string(),
            name: "Godrej Splendour".to_string(),
            root_source: Some("rera".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:godrej-splendour",
                    "listing_3bhk",
                    FactValue::Text("3 BHK listing: INR 1.62 Cr for 1261 sq ft".to_string()),
                    0.8,
                ),
                serving_fact(
                    "society:godrej-splendour",
                    "listing_locality",
                    FactValue::Text("Whitefield".to_string()),
                    0.8,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-locality");

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].area, "Whitefield");
    }

    #[test]
    fn serving_society_listing_creates_representative_property_without_source_scan() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:prestige-elm-park".to_string(),
            entity_type: "society".to_string(),
            name: "Prestige Elm Park".to_string(),
            root_source: Some("rera".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:prestige-elm-park",
                    "rera_registered",
                    FactValue::Bool(true),
                    1.0,
                ),
                serving_fact(
                    "society:prestige-elm-park",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(12_500_000.0, 1_250.0)),
                    0.95,
                ),
                serving_fact(
                    "society:prestige-elm-park",
                    "rera_status",
                    FactValue::Text("New Launch".to_string()),
                    0.95,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        assert_eq!(properties.len(), 1);
        let property = &properties[0];
        assert_eq!(property.id, "discovered-prestige-elm-park-3bhk");
        assert_eq!(property.price, 12_500_000);
        assert_eq!(property.possession_status, "New Launch");
    }

    #[test]
    fn serving_direct_properties_do_not_hide_market_backed_societies() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "property:discovered-prestige-waterford-3bhk".to_string(),
                entity_type: "property".to_string(),
                name: "3 BHK in Prestige Waterford".to_string(),
                root_source: Some("external_listing".to_string()),
                visibility: Default::default(),
                searchable_text: "3 BHK in Prestige Waterford".to_string(),
            },
            ServingEntityRecord {
                entity_id: "society:prestige-elm-park".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Elm Park".to_string(),
                root_source: Some("builder_official".to_string()),
                visibility: Default::default(),
                searchable_text: "Prestige Elm Park".to_string(),
            },
        ];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "property:discovered-prestige-waterford-3bhk",
                    "price",
                    FactValue::Numeric(24_000_000.0),
                    0.9,
                ),
                serving_fact(
                    "society:prestige-elm-park",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(12_500_000.0, 1_250.0)),
                    0.95,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records_with_edges(
            &entities,
            &[membership(
                "property:discovered-prestige-waterford-3bhk",
                "society:prestige-waterford",
            )],
            &fact_index,
            "bundle-v1",
        );
        let ids = properties
            .iter()
            .map(|property| property.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(properties.len(), 2);
        assert!(ids.contains(&"discovered-prestige-waterford-3bhk"));
        assert!(ids.contains(&"discovered-prestige-elm-park-3bhk"));
    }

    #[test]
    fn same_name_societies_keep_distinct_canonical_identities() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:prestige-elm-park".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Elm Park".to_string(),
                root_source: Some("rera".to_string()),
                visibility: Default::default(),
                searchable_text: String::new(),
            },
            ServingEntityRecord {
                entity_id: "society:rera-elm-park-alias".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Elm Park".to_string(),
                root_source: Some("rera".to_string()),
                visibility: Default::default(),
                searchable_text: String::new(),
            },
        ];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:prestige-elm-park",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(12_500_000.0, 1_250.0)),
                    0.95,
                ),
                serving_fact(
                    "society:rera-elm-park-alias",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(12_500_000.0, 1_250.0)),
                    0.95,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        assert_eq!(properties.len(), 2);
        let mut renamed = entities.clone();
        renamed.reverse();
        for entity in &mut renamed {
            entity.name = format!("Renamed {}", entity.entity_id);
        }
        let renamed_properties =
            properties_from_serving_records(&renamed, &fact_index, "bundle-v1");
        assert_eq!(
            properties
                .iter()
                .map(|property| (&property.id, &property.society_id))
                .collect::<Vec<_>>(),
            renamed_properties
                .iter()
                .map(|property| (&property.id, &property.society_id))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            properties
                .iter()
                .filter(|property| property.id == "discovered-prestige-elm-park-3bhk")
                .count(),
            1
        );
    }

    #[test]
    fn serving_society_without_inventory_remains_browseable() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:rera-only-project".to_string(),
            entity_type: "society".to_string(),
            name: "RERA Only Project".to_string(),
            root_source: Some("rera".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![serving_fact(
                "society:rera-only-project",
                "rera_registered",
                FactValue::Bool(true),
                1.0,
            )],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        assert_eq!(properties.len(), 1);
        let public = serde_json::to_value(&properties[0]).unwrap();
        assert!(public.get("bhk").is_none());
        assert!(public.get("price").is_none());
        assert!(public.get("carpet_area_sqft").is_none());
    }

    #[test]
    fn society_configurations_cannot_establish_inventory_without_a_binding() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:pursuit-of-a-radical-rhapsody-phase-2".to_string(),
            entity_type: "society".to_string(),
            name: "Pursuit of a Radical Rhapsody Phase 2".to_string(),
            root_source: Some("rera".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:pursuit-of-a-radical-rhapsody-phase-2",
                    "available_configurations",
                    FactValue::Text("2 BHK, 3BHK, 4 BHK, 5BHK".to_string()),
                    0.95,
                ),
                serving_fact(
                    "society:pursuit-of-a-radical-rhapsody-phase-2",
                    "rera_registered",
                    FactValue::Bool(true),
                    1.0,
                ),
                serving_fact(
                    "society:pursuit-of-a-radical-rhapsody-phase-2",
                    "area",
                    FactValue::Text("Whitefield".to_string()),
                    0.9,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");
        let bhks = properties
            .iter()
            .map(|property| property.bhk)
            .collect::<Vec<_>>();

        assert_eq!(bhks, vec![0]);
        assert!(properties.iter().all(|property| property.price == 0));
        assert!(properties
            .iter()
            .all(|property| property.price_per_sqft == 0));
    }

    #[test]
    fn serving_society_priced_bhks_do_not_expand_from_rera_configurations() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:priced-project".to_string(),
            entity_type: "society".to_string(),
            name: "Priced Project".to_string(),
            root_source: Some("rera".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "society:priced-project",
                    "listing_3bhk",
                    FactValue::Text(listing_payload(12_500_000.0, 1_250.0)),
                    0.95,
                ),
                serving_fact(
                    "society:priced-project",
                    "available_configurations",
                    FactValue::Text("2 BHK, 3 BHK, 4 BHK".to_string()),
                    0.95,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v1");

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].id, "discovered-priced-project-3bhk");
        assert_eq!(properties[0].price, 12_500_000);
    }

    fn membership(property_id: &str, society_id: &str) -> crate::serving::ServingEdgeRecord {
        crate::serving::ServingEdgeRecord {
            from_entity_id: property_id.into(),
            edge_type: "in_society".into(),
            to_entity_id: society_id.into(),
            confidence: 1.0,
            source_type: "fixture".into(),
            derivation: None,
        }
    }

    fn listing_payload(price: f64, sqft: f64) -> String {
        listing_payload_range(price, price, price, sqft)
    }

    fn listing_payload_range(price: f64, price_min: f64, price_max: f64, sqft: f64) -> String {
        serde_json::json!({
            "listing_type": "sale",
            "bhk": 3,
            "area_type": "carpet",
            "price": price as u64,
            "price_min": price_min as u64,
            "price_max": price_max as u64,
            "area_sqft": sqft as u32,
            "area_sqft_min": sqft,
            "area_sqft_max": sqft
        })
        .to_string()
    }

    fn serving_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        confidence: f32,
    ) -> crate::serving::ServingFactRecord {
        let source = if fact_key.starts_with("listing_") {
            "ExternalListing"
        } else {
            "Computed"
        };
        serving_fact_with_source(entity_id, fact_key, value, confidence, source)
    }

    fn serving_fact_with_source(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        confidence: f32,
        source_type: &str,
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
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: Some("test".to_string()),
            learned_at: Utc.timestamp_opt(1, 0).unwrap(),
            observation: Some(
                crate::serving::SourceObservation::new(
                    source_type,
                    format!("fixture:{fact_key}"),
                    entity_id.to_string(),
                    Utc.timestamp_opt(1, 0).unwrap(),
                    None,
                    vec!["fixture/source-v1".to_string()],
                )
                .unwrap(),
            ),
        }
    }

    #[test]
    fn reddit_theme_fact_loses_to_google_on_same_fact_key() {
        let entity_id = "society:prestige-waterford";
        let index = crate::serving::ServingFactIndex::from_records(
            vec![
                serving_fact_with_source(
                    entity_id,
                    "operating.tanker_dependence",
                    FactValue::Text("mentioned".to_string()),
                    0.4,
                    "RedditTheme",
                ),
                serving_fact_with_source(
                    entity_id,
                    "operating.tanker_dependence",
                    FactValue::Text("high".to_string()),
                    0.9,
                    "Google",
                ),
            ],
            vec![],
        );
        let rows = index.entity(entity_id).expect("entity rows");

        assert_eq!(
            latest_text(Some(rows), "operating.tanker_dependence"),
            Some("high".to_string())
        );
    }

    #[test]
    fn timestamps_do_not_select_runtime_facts() {
        let entity_id = "society:stable-selection";
        let mut stable = serving_fact(
            entity_id,
            "summary",
            FactValue::Text("alpha".to_string()),
            0.9,
        );
        stable.learned_at = Utc.timestamp_opt(1, 0).unwrap();
        let mut later = serving_fact(
            entity_id,
            "summary",
            FactValue::Text("zeta".to_string()),
            0.9,
        );
        later.learned_at = Utc.timestamp_opt(2, 0).unwrap();
        let index = ServingFactIndex::from_records(vec![later, stable], Vec::new());

        assert_eq!(
            latest_text(index.entity(entity_id), "summary"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn serving_properties_without_price_remain_in_runtime_catalog_when_configured() {
        let entities = vec![ServingEntityRecord {
            entity_id: "property:discovered-prestige-lakeside-habitat-3bhk".to_string(),
            entity_type: "property".to_string(),
            name: "3 BHK in Prestige Lakeside Habitat".to_string(),
            root_source: Some("discovered".to_string()),
            visibility: Default::default(),
            searchable_text: "3 BHK in Prestige Lakeside Habitat".to_string(),
        }];
        let property_id = "property:discovered-prestige-lakeside-habitat-3bhk";
        let fact_index = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    property_id,
                    "title",
                    FactValue::Text("3 BHK in Prestige Lakeside Habitat".to_string()),
                    0.8,
                ),
                serving_fact(
                    property_id,
                    "hero_image",
                    FactValue::Text("/media/lakeside.webp".to_string()),
                    0.8,
                ),
            ],
            Vec::new(),
        );

        let properties = properties_from_serving_records_with_edges(
            &entities,
            &[membership(
                "property:discovered-prestige-lakeside-habitat-3bhk",
                "society:prestige-lakeside-habitat",
            )],
            &fact_index,
            "bundle-v1",
        );
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].bhk, 0);
        assert!(serde_json::to_value(&properties[0])
            .unwrap()
            .get("bhk")
            .is_none());
        assert_eq!(properties[0].price, 0);
    }

    #[test]
    fn image_backed_society_without_configuration_stays_unknown_instead_of_becoming_3bhk() {
        let entities = vec![ServingEntityRecord {
            entity_id: "society:promising-unknown-config".to_string(),
            entity_type: "society".to_string(),
            name: "Promising Unknown Config".to_string(),
            root_source: Some("discovered".to_string()),
            visibility: Default::default(),
            searchable_text: String::new(),
        }];
        let fact_index = ServingFactIndex::from_records(
            vec![serving_fact(
                "society:promising-unknown-config",
                "hero_image",
                FactValue::Text("/media/promising.webp".to_string()),
                0.9,
            )],
            Vec::new(),
        );

        let properties = properties_from_serving_records(&entities, &fact_index, "bundle-v3");

        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].id, "discovered-promising-unknown-config");
        assert_eq!(properties[0].title, "Promising Unknown Config");
        assert_eq!(properties[0].bhk, 0);
        assert_eq!(properties[0].price, 0);
        assert_eq!(properties[0].hero_image, "/media/promising.webp");
    }
}
