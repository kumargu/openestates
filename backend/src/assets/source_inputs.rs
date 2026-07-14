use serde::{Deserialize, Serialize};

use super::{
    AssetDagPlan, AssetId, RedditThreadSnapshotRecord, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactRecord, SourceWatermark, GOOGLE_REVIEW_FACTS_ASSET_ID,
    REDDIT_RESIDENT_FACTS_ASSET_ID, REDDIT_THREADS_DAILY_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};

/// Control-plane input for source executors.
///
/// This file is intentionally JSON-sized metadata/input, not the durable data
/// lake shape. Executors normalize these records into Parquet-backed assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetSourceInputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_registry_monthly: Option<ReraRegistryMonthlyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_threads_daily: Option<RedditThreadsDailyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_resident_facts: Option<SkillFactsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_review_facts: Option<SkillFactsInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInputCollectionPlan {
    pub requested_assets: Vec<AssetId>,
    pub force_assets: Vec<AssetId>,
}

impl AssetSourceInputs {
    pub fn supported_asset_ids() -> Vec<AssetId> {
        [
            RERA_REGISTRY_MONTHLY_ASSET_ID,
            REDDIT_THREADS_DAILY_ASSET_ID,
            REDDIT_RESIDENT_FACTS_ASSET_ID,
            GOOGLE_REVIEW_FACTS_ASSET_ID,
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
                | GOOGLE_REVIEW_FACTS_ASSET_ID
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
        let resident_facts_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == REDDIT_RESIDENT_FACTS_ASSET_ID);
        let reddit_raw_requested = requested_assets
            .iter()
            .any(|asset_id| asset_id.as_str() == REDDIT_THREADS_DAILY_ASSET_ID);
        if resident_facts_requested && !reddit_raw_requested {
            let raw_asset = AssetId::new(REDDIT_THREADS_DAILY_ASSET_ID)
                .expect("static Reddit raw asset id is valid");
            requested_assets.push(raw_asset.clone());
            force_assets.push(raw_asset);
        }
        requested_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        SourceInputCollectionPlan {
            requested_assets,
            force_assets,
        }
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
