use serde::{Deserialize, Serialize};

use super::{
    RedditThreadSnapshotRecord, SkillFactAnnotationRecord, SkillFactRecord, SourceWatermark,
};

/// Control-plane input for source executors.
///
/// This file is intentionally JSON-sized metadata/input, not the durable data
/// lake shape. Executors normalize these records into Parquet-backed assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetSourceInputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_threads_daily: Option<RedditThreadsDailyInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reddit_resident_facts: Option<SkillFactsInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_review_facts: Option<SkillFactsInput>,
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
