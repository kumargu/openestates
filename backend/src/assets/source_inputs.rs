use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::environment::EnvironmentGroundwaterPotentialInput;
use super::osm_power::OsmPowerInfrastructureInput;
use super::source_provider::SourceEntitySeed;
use super::stormwater::StormwaterDrainRiskInput;
use super::transit::BengaluruMetroStationsInput;
use super::{
    AssetDagPlan, AssetDagRunManifest, AssetId, AssetRunStepStatus, ExternalImagesWeeklyInput,
    ExternalListingsWeeklyInput, GoogleNearbyPlacesWeeklyInput, GooglePlacesWeeklyInput,
    PlanReason, RedditThreadSnapshotRecord, ReraProjectPlanFramesInput, ReraReceiptsSourceInput,
    ReraRegistryMonthlyInput, ReraSourceRecordsInput, SkillFactAnnotationRecord, SkillFactRecord,
    SourceWatermark, BENGALURU_METRO_STATION_FACTS_ASSET_ID, CURRENT_PROJECT_FACTS_ASSET_ID,
    EXTERNAL_IMAGES_WEEKLY_ASSET_ID, EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
    EXTERNAL_LISTING_FACTS_ASSET_ID, GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
    GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID, GOOGLE_PLACES_WEEKLY_ASSET_ID,
    GOOGLE_REVIEW_FACTS_ASSET_ID, IMAGE_MEDIA_FACTS_ASSET_ID, OSM_POWER_LINE_FACTS_ASSET_ID,
    RERA_PROJECT_PLAN_FRAMES_ASSET_ID, RERA_RECEIPTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
    RERA_SOURCE_RECORDS_ASSET_ID, SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
    STORMWATER_DRAIN_FACTS_ASSET_ID,
};

/// Control-plane input for source executors.
///
/// This file is intentionally JSON-sized metadata/input, not the durable data
/// lake shape. Executors normalize these records into Parquet-backed assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetSourceInputs {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_entities: Vec<SourceEntitySeed>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_failures: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_registry_monthly: Option<ReraRegistryMonthlyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_receipts: Option<ReraReceiptsSourceInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_source_records: Option<ReraSourceRecordsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_project_plan_frames: Option<ReraProjectPlanFramesInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_threads_daily: Option<RedditThreadsDailyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_resident_facts: Option<SkillFactsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_places_weekly: Option<GooglePlacesWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_nearby_places_weekly: Option<GoogleNearbyPlacesWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_listings_weekly: Option<ExternalListingsWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_images_weekly: Option<ExternalImagesWeeklyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_groundwater_potential: Option<EnvironmentGroundwaterPotentialInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bengaluru_metro_stations: Option<BengaluruMetroStationsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub osm_power_infrastructure: Option<OsmPowerInfrastructureInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stormwater_drains: Option<StormwaterDrainRiskInput>,
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
            RERA_RECEIPTS_ASSET_ID,
            RERA_SOURCE_RECORDS_ASSET_ID,
            RERA_REGISTRY_MONTHLY_ASSET_ID,
            RERA_PROJECT_PLAN_FRAMES_ASSET_ID,
            GOOGLE_PLACES_WEEKLY_ASSET_ID,
            GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
            SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
            BENGALURU_METRO_STATION_FACTS_ASSET_ID,
            OSM_POWER_LINE_FACTS_ASSET_ID,
            STORMWATER_DRAIN_FACTS_ASSET_ID,
        ]
        .into_iter()
        .map(|id| AssetId::new(id).expect("static source input asset id is valid"))
        .collect()
    }

    pub fn supports_asset(asset_id: &AssetId) -> bool {
        matches!(
            asset_id.as_str(),
            RERA_RECEIPTS_ASSET_ID
                | RERA_SOURCE_RECORDS_ASSET_ID
                | RERA_REGISTRY_MONTHLY_ASSET_ID
                | RERA_PROJECT_PLAN_FRAMES_ASSET_ID
                | GOOGLE_PLACES_WEEKLY_ASSET_ID
                | GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID
                | EXTERNAL_LISTINGS_WEEKLY_ASSET_ID
                | EXTERNAL_IMAGES_WEEKLY_ASSET_ID
                | SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID
                | BENGALURU_METRO_STATION_FACTS_ASSET_ID
                | OSM_POWER_LINE_FACTS_ASSET_ID
                | STORMWATER_DRAIN_FACTS_ASSET_ID
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
            EXTERNAL_LISTING_FACTS_ASSET_ID,
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            false,
        );
        add_derived_companion(
            plan,
            &mut requested_assets,
            &mut force_assets,
            IMAGE_MEDIA_FACTS_ASSET_ID,
            EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
            false,
        );
        for raw_asset_id in [
            SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
            OSM_POWER_LINE_FACTS_ASSET_ID,
            STORMWATER_DRAIN_FACTS_ASSET_ID,
        ] {
            add_raw_companion(
                &mut requested_assets,
                &mut force_assets,
                plan.run_entries()
                    .any(|entry| entry.asset_id.as_str() == CURRENT_PROJECT_FACTS_ASSET_ID),
                raw_asset_id,
                false,
            );
        }
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
                step.asset_id.as_str() == EXTERNAL_LISTING_FACTS_ASSET_ID && step_needs_replay(step)
            }),
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            true,
        );
        add_raw_companion(
            &mut requested_assets,
            &mut force_assets,
            manifest.steps.iter().any(|step| {
                step.asset_id.as_str() == IMAGE_MEDIA_FACTS_ASSET_ID && step_needs_replay(step)
            }),
            EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
            true,
        );
        for raw_asset_id in [
            SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
            OSM_POWER_LINE_FACTS_ASSET_ID,
            STORMWATER_DRAIN_FACTS_ASSET_ID,
        ] {
            add_raw_companion(
                &mut requested_assets,
                &mut force_assets,
                manifest.steps.iter().any(|step| {
                    step.asset_id.as_str() == CURRENT_PROJECT_FACTS_ASSET_ID
                        && step_needs_replay(step)
                }),
                raw_asset_id,
                true,
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    use super::super::{
        AssetFreshness, AssetPartition, AssetPlanEntry, AssetStage, CostTier,
        FreshnessReferenceKind, MaterializationId, PlanDecision, RefreshCadence, TrustTier,
    };

    #[test]
    fn forced_google_nearby_facts_request_raw_nearby_source_input() {
        let plan = test_plan([test_run_entry(GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID)]);

        let collection_plan = AssetSourceInputs::collection_plan(&plan);

        assert!(collection_plan
            .requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID));
        assert!(collection_plan
            .force_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID));
    }

    #[test]
    fn forced_raw_source_asset_is_requested_by_collection_plan() {
        let plan = test_plan([test_run_entry(GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID)]);

        let collection_plan = AssetSourceInputs::collection_plan(&plan);

        assert_eq!(
            collection_plan.requested_assets,
            vec![AssetId::new(GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID).unwrap()]
        );
    }

    #[test]
    fn resumed_current_project_facts_request_required_red_flag_source_inputs() {
        let plan = test_plan([test_run_entry(CURRENT_PROJECT_FACTS_ASSET_ID)]);
        let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "resume-required");
        manifest.steps[0].status = AssetRunStepStatus::Blocked;

        let collection_plan = AssetSourceInputs::resume_collection_plan(&manifest);

        for asset_id in [
            SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
            OSM_POWER_LINE_FACTS_ASSET_ID,
            STORMWATER_DRAIN_FACTS_ASSET_ID,
        ] {
            let asset_id = AssetId::new(asset_id).unwrap();
            assert!(collection_plan.requested_assets.contains(&asset_id));
            assert!(collection_plan.force_assets.contains(&asset_id));
        }
    }

    fn test_plan(entries: impl IntoIterator<Item = AssetPlanEntry>) -> AssetDagPlan {
        AssetDagPlan {
            run_id: MaterializationId::new(),
            partition: AssetPartition::global(),
            planned_at: chrono::Utc.with_ymd_and_hms(2026, 7, 22, 12, 0, 0).unwrap(),
            entries: entries.into_iter().collect(),
        }
    }

    fn test_run_entry(asset_id: &str) -> AssetPlanEntry {
        AssetPlanEntry {
            asset_id: AssetId::new(asset_id).unwrap(),
            partition: AssetPartition::global(),
            stage: AssetStage::Raw,
            dependencies: Vec::new(),
            refresh: RefreshCadence::Weekly,
            cost_tier: CostTier::Free,
            trust_tier: TrustTier::Support,
            decision: PlanDecision::Run,
            reason: Some(PlanReason::Forced),
            current_materialization_id: None,
            current_version: None,
            current_created_at: None,
            current_parent_materializations: Vec::new(),
            dependency_snapshot: Vec::new(),
            freshness: AssetFreshness {
                cadence: RefreshCadence::Weekly,
                reference_kind: FreshnessReferenceKind::Missing,
                reference_value: None,
                reference_time: None,
                current_age_seconds: None,
                max_age_seconds: Some(7 * 24 * 60 * 60),
                is_stale: true,
            },
        }
    }
}
