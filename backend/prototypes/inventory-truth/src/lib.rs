pub mod model;
pub mod parquet;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use model::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn validate(data: &Fixture, policy: &Policy) -> Result<()> {
    if data.kind != "inventory_design_fixture"
        || policy.version != 1
        || policy.price_relation != Relation::Confirmed
        || policy.price_availability != Availability::Active
        || !policy.require_reliable_registrations
    {
        return Err("Unsupported preview admission policy".into());
    }
    let homes: BTreeMap<_, _> = data
        .homes
        .iter()
        .map(|home| (home.id.as_str(), home))
        .collect();
    let rows: BTreeMap<_, _> = data
        .observations
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();
    if homes.len() != data.homes.len() || rows.len() != data.observations.len() {
        return Err("Duplicate home or observation ID".into());
    }
    for home in &data.homes {
        if home.id.is_empty()
            || home.title.trim().is_empty()
            || home.bhk <= 0
            || home.area_sqft.is_some_and(|v| v <= 0)
            || home.area_sqft.is_some() != home.area_basis.is_some()
            || home.floor.is_some_and(|v| v < 0)
        {
            return Err("Invalid home measurement".into());
        }
    }
    let mut current = BTreeSet::new();
    let mut ad_bindings = BTreeMap::new();
    for row in &data.observations {
        let home = homes
            .get(row.home_id.as_str())
            .ok_or("Unbound observation")?;
        if row.id.is_empty()
            || row.provider_id.is_empty()
            || row.advertisement_id.is_empty()
            || row.source_label.trim().is_empty()
            || row.amount_inr.is_some_and(|v| v <= 0)
            || row.area_sqft.is_some_and(|v| v <= 0)
            || row.bhk.is_some_and(|v| v <= 0)
            || row.floor.is_some_and(|v| v < 0)
            || row.area_sqft.is_some() != row.area_basis.is_some()
        {
            return Err("Invalid observation measurement or identity".into());
        }
        if row
            .source_url
            .as_ref()
            .is_some_and(|url| !url.starts_with("https://"))
        {
            return Err("Unsafe source URL".into());
        }
        let ad = (&row.provider_id, &row.advertisement_id);
        if let Some(binding) = ad_bindings.insert(ad, (&row.home_id, &row.relation)) {
            if binding != (&row.home_id, &row.relation) {
                return Err("Advertisement has contradictory bindings".into());
            }
        }
        if row.current && !current.insert(ad) {
            return Err("Multiple current observations for one advertisement".into());
        }
        if row.relation == Relation::Confirmed
            && (row.bhk.is_some_and(|v| v != home.bhk)
                || row
                    .area_sqft
                    .zip(home.area_sqft)
                    .is_some_and(|(a, b)| a != b)
                || row
                    .area_basis
                    .as_ref()
                    .zip(home.area_basis.as_ref())
                    .is_some_and(|(a, b)| a != b)
                || row.floor.zip(home.floor).is_some_and(|(a, b)| a != b))
        {
            return Err("Confirmed binding has conflicting measurements".into());
        }
        let mut visited = BTreeSet::from([row.id.as_str()]);
        let mut predecessor = row.predecessor_id.as_deref();
        while let Some(id) = predecessor {
            let old = rows.get(id).ok_or("Dangling predecessor")?;
            if !visited.insert(id) {
                return Err("Cyclic observation history".into());
            }
            if old.home_id != row.home_id
                || old.provider_id != row.provider_id
                || old.advertisement_id != row.advertisement_id
                || old.relation != row.relation
                || old.current
            {
                return Err("Cross-advertisement or current predecessor".into());
            }
            predecessor = old.predecessor_id.as_deref();
        }
    }
    if ad_bindings.len() != current.len() {
        return Err("Advertisement lacks explicit current observation".into());
    }
    let mut reachable = BTreeSet::new();
    for row in data.observations.iter().filter(|row| row.current) {
        let mut next = Some(row.id.as_str());
        while let Some(id) = next {
            reachable.insert(id);
            next = rows[id].predecessor_id.as_deref();
        }
    }
    if reachable.len() != rows.len() {
        return Err("Orphan history observation".into());
    }
    let mut signals = BTreeSet::new();
    for signal in &data.signals {
        if !signals.insert(&signal.id)
            || !rows.contains_key(signal.observation_id.as_str())
            || signal.label.trim().is_empty()
            || signal.value.trim().is_empty()
        {
            return Err("Invalid identity signal".into());
        }
        if signal.disagrees && rows[signal.observation_id.as_str()].relation == Relation::Confirmed
        {
            return Err("Conflicting observation cannot be confirmed".into());
        }
    }
    Ok(())
}

pub struct Inventory {
    catalog: InventoryCatalog,
    details: BTreeMap<String, InventoryDetail>,
}
impl Inventory {
    pub fn from_snapshot(root: &std::path::Path) -> Result<Self> {
        let (data, policy, snapshot) = parquet::load(root)?;
        Self::project(data, policy, snapshot)
    }
    pub fn project(data: Fixture, policy: Policy, snapshot_id: String) -> Result<Self> {
        validate(&data, &policy)?;
        let all: BTreeMap<_, _> = data
            .observations
            .iter()
            .map(|row| (row.id.as_str(), row))
            .collect();
        let mut details = BTreeMap::new();
        for home in &data.homes {
            let mut advertisements = Vec::new();
            let mut candidates = Vec::new();
            let mut comparables = Vec::new();
            let mut registrations = Vec::new();
            for row in data
                .observations
                .iter()
                .filter(|row| row.home_id == home.id && row.current)
            {
                let mut history = Vec::new();
                let mut next = row.predecessor_id.as_deref();
                while let Some(id) = next {
                    history.push(all[id].clone());
                    next = all[id].predecessor_id.as_deref();
                }
                let mut identity: Vec<_> = data
                    .signals
                    .iter()
                    .filter(|s| s.observation_id == row.id)
                    .cloned()
                    .collect();
                identity.sort_by(|a, b| a.id.cmp(&b.id));
                let ad = Advertisement {
                    observation: row.clone(),
                    history,
                    identity,
                };
                match row.relation {
                    Relation::Confirmed => advertisements.push(ad),
                    Relation::Likely => candidates.push(ad),
                    Relation::Comparable if row.availability == policy.price_availability => {
                        comparables.push(ad)
                    }
                    Relation::Registration if row.reliable => registrations.push(ad),
                    _ => {}
                }
            }
            // Stable identity order, never source preference, date or implied price trend.
            for group in [
                &mut advertisements,
                &mut candidates,
                &mut comparables,
                &mut registrations,
            ] {
                group.sort_by(|a, b| {
                    (&a.observation.provider_id, &a.observation.advertisement_id)
                        .cmp(&(&b.observation.provider_id, &b.observation.advertisement_id))
                });
            }
            let eligible: Vec<_> = advertisements
                .iter()
                .map(|ad| &ad.observation)
                .filter(|row| {
                    row.availability == policy.price_availability && row.amount_inr.is_some()
                })
                .collect();
            let amounts: Vec<_> = eligible.iter().filter_map(|row| row.amount_inr).collect();
            let ask = amounts
                .iter()
                .min()
                .zip(amounts.iter().max())
                .map(|(min, max)| AskRange {
                    min_inr: *min,
                    max_inr: *max,
                    observation_ids: eligible.iter().map(|row| row.id.clone()).collect(),
                });
            let summary = HomeSummary {
                home: home.clone(),
                ask,
                advertisement_count: advertisements.len(),
                active_count: advertisements
                    .iter()
                    .filter(|ad| ad.observation.availability == policy.price_availability)
                    .count(),
                uncertain_count: candidates.len(),
                conflicts: candidates
                    .iter()
                    .flat_map(|ad| ad.identity.iter().filter(|s| s.disagrees).cloned())
                    .collect(),
                // One shared observation date may be displayed. It never selects a price.
                observed_on: eligible
                    .first()
                    .filter(|first| {
                        eligible
                            .iter()
                            .all(|row| row.observed_on == first.observed_on)
                    })
                    .map(|row| row.observed_on.clone()),
                price_change: if eligible.len() == 1 {
                    let row = eligible[0];
                    row.predecessor_id.as_deref().and_then(|id| {
                        let previous = all[id];
                        row.amount_inr
                            .zip(previous.amount_inr)
                            .filter(|(now, before)| now != before)
                            .map(|(now, before)| PriceChange {
                                observation_id: row.id.clone(),
                                previous_observation_id: previous.id.clone(),
                                difference_inr: now - before,
                            })
                    })
                } else {
                    None
                },
            };
            details.insert(
                home.id.clone(),
                InventoryDetail {
                    contract_version: 1,
                    snapshot_id: snapshot_id.clone(),
                    fixture: true,
                    summary,
                    advertisements,
                    candidates,
                    comparables,
                    registrations,
                },
            );
        }
        let catalog = InventoryCatalog {
            contract_version: 1,
            snapshot_id,
            fixture: true,
            homes: details.values().map(|d| d.summary.clone()).collect(),
        };
        Ok(Self { catalog, details })
    }
}

#[derive(serde::Deserialize)]
struct SnapshotQuery {
    snapshot: String,
}
async fn catalog(State(state): State<Arc<Inventory>>) -> Json<InventoryCatalog> {
    Json(state.catalog.clone())
}
async fn detail(
    State(state): State<Arc<Inventory>>,
    Path(id): Path<String>,
    Query(query): Query<SnapshotQuery>,
) -> std::result::Result<Json<InventoryDetail>, (StatusCode, Json<serde_json::Value>)> {
    if query.snapshot != state.catalog.snapshot_id {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"code":"snapshot_mismatch"})),
        ));
    }
    state.details.get(&id).cloned().map(Json).ok_or((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"code":"home_not_found"})),
    ))
}
pub fn router(state: Inventory) -> Router {
    Router::new()
        .route("/api/inventory/homes", get(catalog))
        .route("/api/inventory/homes/{id}", get(detail))
        .with_state(Arc::new(state))
}
