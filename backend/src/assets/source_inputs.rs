use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    AssetDagPlan, AssetDagRunManifest, AssetId, AssetRunStepStatus, GooglePlacesWeeklyInput,
    PlanReason, RedditThreadSnapshotRecord, ReraRegistryMonthlyInput, SkillFactAnnotationRecord,
    SkillFactRecord, SourceWatermark, GOOGLE_PLACES_WEEKLY_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID,
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
            .filter(|step| {
                let needs_collection = match step.status {
                    AssetRunStepStatus::Succeeded
                    | AssetRunStepStatus::Skipped
                    | AssetRunStepStatus::Materialized => false,
                    AssetRunStepStatus::Failed => step.materialization_id.is_none(),
                    AssetRunStepStatus::Planned
                    | AssetRunStepStatus::Running
                    | AssetRunStepStatus::Blocked => true,
                };
                needs_collection && Self::supports_asset(&step.asset_id)
            })
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
