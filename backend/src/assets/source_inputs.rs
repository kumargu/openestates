use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    AssetDagPlan, AssetDagRunManifest, AssetId, AssetRunStepStatus, ExternalListingsWeeklyInput,
    GoogleNearbyPlacesWeeklyInput, GooglePlacesWeeklyInput, MetroStationsMonthlyInput, PlanReason,
    PrestigeInventoryWeeklyInput, RedditThreadSnapshotRecord, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactRecord, SourceWatermark, EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
    EXTERNAL_LISTING_FACTS_ASSET_ID, GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
    GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID, GOOGLE_PLACES_WEEKLY_ASSET_ID,
    GOOGLE_REVIEW_FACTS_ASSET_ID, MARKET_PROJECT_FACTS_ASSET_ID, METRO_PROXIMITY_FACTS_ASSET_ID,
    METRO_STATIONS_MONTHLY_ASSET_ID, PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
    REDDIT_RESIDENT_FACTS_ASSET_ID, REDDIT_THREADS_DAILY_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};

/// Control-plane input for source executors.
///
/// This file is intentionally JSON-sized metadata/input, not the durable data
/// lake shape. Executors normalize these records into Parquet-backed assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetSourceInputs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_failures: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_registry_monthly: Option<ReraRegistryMonthlyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_threads_daily: Option<RedditThreadsDailyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_resident_facts: Option<SkillFactsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_places_weekly: Option<GooglePlacesWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_nearby_places_weekly: Option<GoogleNearbyPlacesWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prestige_inventory_weekly: Option<PrestigeInventoryWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_listings_weekly: Option<ExternalListingsWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metro_stations_monthly: Option<MetroStationsMonthlyInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInputCollectionPlan {
    pub requested_assets: Vec<AssetId>,
    pub force_assets: Vec<AssetId>,
    pub force_refresh_assets: Vec<AssetId>,
}

impl AssetSourceInputs {
    pub fn supported_asset_ids() -> Vec<AssetId> {
        [
            RERA_REGISTRY_MONTHLY_ASSET_ID,
            REDDIT_THREADS_DAILY_ASSET_ID,
            REDDIT_RESIDENT_FACTS_ASSET_ID,
            GOOGLE_PLACES_WEEKLY_ASSET_ID,
            GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
            PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            METRO_STATIONS_MONTHLY_ASSET_ID,
        ]
        .into_iter()
        .map(|id| AssetId::new(id).expect("static source input asset id is valid"))
        .collect()
    }

    pub fn supports_asset(asset_id: &AssetId) -> bool {
        matches!(
            asset_id.as_str(),
            RERA_REGISTRY_MONTHLY_ASSET_ID
                | REDDIT_THREADS_DAILY_ASSET_ID
                | REDDIT_RESIDENT_FACTS_ASSET_ID
                | GOOGLE_PLACES_WEEKLY_ASSET_ID
                | GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID
                | PRESTIGE_INVENTORY_WEEKLY_ASSET_ID
                | EXTERNAL_LISTINGS_WEEKLY_ASSET_ID
                | METRO_STATIONS_MONTHLY_ASSET_ID
        )
    }

    pub fn requested_asset_ids(plan: &AssetDagPlan) -> Vec<AssetId> {
        plan.run_entries()
            .filter(|entry| Self::supports_asset(&entry.asset_id))
            .map(|entry| entry.asset_id.clone())
            .collect()
    }

    pub fn collection_plan(plan: &AssetDagPlan) -> SourceInputCollectionPlan {
        let mut requested_assets = Self::requested_asset_ids(plan);
        let mut force_assets = Vec::new();
        let mut force_refresh_assets: Vec<AssetId> = plan
            .run_entries()
            .filter(|entry| matches!(entry.reason, Some(PlanReason::Stale { .. })))
            .filter(|entry| Self::supports_asset(&entry.asset_id))
            .map(|entry| entry.asset_id.clone())
            .collect();
        let resident_facts_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == REDDIT_RESIDENT_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            resident_facts_requested,
            REDDIT_THREADS_DAILY_ASSET_ID,
            false,
        );
        let google_facts_requested = plan
            .run_entries()
            .any(|entry| entry.asset_id.as_str() == GOOGLE_REVIEW_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            google_facts_requested,
            GOOGLE_PLACES_WEEKLY_ASSET_ID,
            false,
        );
        let google_nearby_facts_requested = plan
            .run_entries()
            .any(|entry| entry.asset_id.as_str() == GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            google_nearby_facts_requested,
            GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
            false,
        );
        add_derived_companion(
            plan,
            &mut requested_assets,
            &mut force_assets,
            MARKET_PROJECT_FACTS_ASSET_ID,
            PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
            false,
        );
        add_derived_companion(
            plan,
            &mut requested_assets,
            &mut force_assets,
            EXTERNAL_LISTING_FACTS_ASSET_ID,
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            false,
        );
        add_derived_companion(
            plan,
            &mut requested_assets,
            &mut force_assets,
            METRO_PROXIMITY_FACTS_ASSET_ID,
            METRO_STATIONS_MONTHLY_ASSET_ID,
            false,
        );
        requested_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        requested_assets.dedup();
        force_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        force_assets.dedup();
        force_refresh_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        SourceInputCollectionPlan {
            requested_assets,
            force_assets,
            force_refresh_assets,
        }
    }

    pub fn resume_collection_plan(manifest: &AssetDagRunManifest) -> SourceInputCollectionPlan {
        let mut requested_assets: Vec<_> = manifest
            .steps
            .iter()
            .filter(|step| step_needs_replay(step) && Self::supports_asset(&step.asset_id))
            .map(|step| step.asset_id.clone())
            .collect();
        let mut force_assets = Vec::new();
        let reddit_facts_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == REDDIT_RESIDENT_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            reddit_facts_requested,
            REDDIT_THREADS_DAILY_ASSET_ID,
            true,
        );
        let google_facts_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == GOOGLE_REVIEW_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            google_facts_requested,
            GOOGLE_PLACES_WEEKLY_ASSET_ID,
            true,
        );
        let google_nearby_facts_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID);
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            google_nearby_facts_requested,
            GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
            true,
        );
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            manifest.steps.iter().any(|step| {
                step.asset_id.as_str() == MARKET_PROJECT_FACTS_ASSET_ID && step_needs_replay(step)
            }),
            PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
            true,
        );
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            manifest.steps.iter().any(|step| {
                step.asset_id.as_str() == EXTERNAL_LISTING_FACTS_ASSET_ID && step_needs_replay(step)
            }),
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            true,
        );
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            manifest.steps.iter().any(|step| {
                step.asset_id.as_str() == METRO_PROXIMITY_FACTS_ASSET_ID && step_needs_replay(step)
            }),
            METRO_STATIONS_MONTHLY_ASSET_ID,
            true,
        );
        requested_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        requested_assets.dedup();
        force_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        force_assets.dedup();
        SourceInputCollectionPlan {
            requested_assets,
            force_assets,
            force_refresh_assets: Vec::new(),
        }
    }
}

fn step_needs_replay(step: &super::AssetRunStep) -> bool {
    match step.status {
        AssetRunStepStatus::Succeeded
        | AssetRunStepStatus::Skipped
        | AssetRunStepStatus::Materialized => false,
        AssetRunStepStatus::Failed => step.materialization_id.is_none(),
        AssetRunStepStatus::Planned | AssetRunStepStatus::Running | AssetRunStepStatus::Blocked => {
            true
        }
    }
}

fn add_derived_companion(
    plan: &AssetDagPlan,
    requested_assets: &mut Vec<AssetId>,
    force_assets: &mut Vec<AssetId>,
    derived_asset_id: &str,
    raw_asset_id: &str,
    force_when_requested: bool,
) {
    add_raw_companion(
        requested_assets,
        force_assets,
        plan.run_entries()
            .any(|entry| entry.asset_id.as_str() == derived_asset_id),
        raw_asset_id,
        force_when_requested,
    );
}

fn add_raw_companion(
    requested_assets: &mut Vec<AssetId>,
    force_assets: &mut Vec<AssetId>,
    derived_requested: bool,
    raw_asset_id: &str,
    force_when_requested: bool,
) {
    if !derived_requested {
        return;
    }
    let raw_asset = AssetId::new(raw_asset_id).expect("static raw asset id is valid");
    let already_requested = requested_assets
        .iter()
        .any(|asset_id| asset_id == &raw_asset);
    if !already_requested {
        requested_assets.push(raw_asset.clone());
    }
    if force_when_requested || !already_requested {
        force_assets.push(raw_asset);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedditThreadsDailyInput {
    pub snapshot_date: String,
    pub subreddit: String,
    #[serde(default)]
    pub records: Vec<RedditThreadSnapshotRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFactsInput {
    pub source: String,
    pub snapshot_date: String,
    #[serde(default)]
    pub facts: Vec<SkillFactRecord>,
    #[serde(default)]
    pub fact_annotations: Vec<SkillFactAnnotationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}
