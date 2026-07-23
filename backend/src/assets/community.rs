use std::fmt;

use crate::community::{
    community_evidence_from_fact_value, deterministic_community_summarizer, CommunityEntitySummary,
    CommunityEvidenceRecord, CommunityEvidenceSummaryEngine,
};
use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    read_skill_fact_artifact_rows, MaterializationId, MaterializationRecord,
    SkillFactAnnotationRecord, SkillFactMaterializeError, SkillFactRecord, SkillFactsInput,
};

pub const COMMUNITY_REVIEW_SUMMARY_FACTS_ASSET_ID: &str = "community_review_summary_facts";
const COMMUNITY_SUMMARIZER_SKILL_ID: &str = "community_evidence_summarizer";

pub async fn community_review_summary_facts_input(
    lake: &LakeStore,
    support_records: &[MaterializationRecord],
    run_id: &MaterializationId,
    snapshot_date: impl Into<String>,
) -> Result<SkillFactsInput, CommunitySummaryAssetError> {
    let support_rows = read_skill_fact_artifact_rows(lake, support_records).await?;
    community_review_summary_facts_from_records(&support_rows.facts, run_id, snapshot_date)
}

pub fn community_review_summary_facts_from_records(
    support_facts: &[SkillFactRecord],
    run_id: &MaterializationId,
    snapshot_date: impl Into<String>,
) -> Result<SkillFactsInput, CommunitySummaryAssetError> {
    let summarizer = deterministic_community_summarizer();
    community_review_summary_facts_from_records_with_summarizer(
        support_facts,
        run_id,
        snapshot_date,
        &summarizer,
    )
}

pub fn community_review_summary_facts_from_records_with_summarizer(
    support_facts: &[SkillFactRecord],
    run_id: &MaterializationId,
    snapshot_date: impl Into<String>,
    summarizer: &dyn CommunityEvidenceSummaryEngine,
) -> Result<SkillFactsInput, CommunitySummaryAssetError> {
    let evidence = support_facts
        .iter()
        .filter_map(skill_fact_to_community_evidence)
        .collect::<Vec<_>>();
    let summaries = summarizer.summarize(&evidence);
    let mut facts = Vec::new();
    let mut fact_annotations = Vec::new();
    for summary in summaries {
        append_summary_facts(&summary, run_id, &mut facts, &mut fact_annotations)?;
    }
    Ok(SkillFactsInput {
        source: "community".to_string(),
        snapshot_date: snapshot_date.into(),
        facts,
        fact_annotations,
        source_watermarks: Vec::new(),
    })
}

fn skill_fact_to_community_evidence(fact: &SkillFactRecord) -> Option<CommunityEvidenceRecord> {
    if fact.skill_id.as_deref() == Some(COMMUNITY_SUMMARIZER_SKILL_ID) {
        return None;
    }
    if !matches!(fact.source_type.as_str(), "Google" | "Reddit") {
        return None;
    }
    let value = serde_json::from_str::<FactValue>(&fact.value_json).ok()?;
    community_evidence_from_fact_value(
        &fact.entity_id,
        &fact.source_type,
        fact.source_url.clone(),
        &fact.fact_key,
        &value,
        fact.confidence,
        fact.learned_at,
    )
}

fn append_summary_facts(
    summary: &CommunityEntitySummary,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), CommunitySummaryAssetError> {
    let source_url = summary.source_urls.first().cloned();
    push_fact(
        summary,
        run_id,
        "community_review_summary",
        FactValue::Text(summary.summary.clone()),
        "Community signal: {value}",
        &[
            "reviews",
            "resident feedback",
            "community signal",
            "google reviews",
            "reddit",
        ],
        Some(("TextMatch", 1.2, Vec::new())),
        source_url.clone(),
        facts,
        annotations,
    )?;
    if !summary.positive_themes.is_empty() {
        push_fact(
            summary,
            run_id,
            "community_positive_themes",
            FactValue::Tags(summary.positive_themes.clone()),
            "Positive resident themes: {value}",
            &[
                "greenery",
                "maintenance",
                "amenities",
                "clubhouse",
                "metro",
                "connectivity",
                "good society",
            ],
            Some(("TextMatch", 1.4, Vec::new())),
            source_url.clone(),
            facts,
            annotations,
        )?;
    }
    if !summary.concern_themes.is_empty() {
        push_fact(
            summary,
            run_id,
            "community_concern_themes",
            FactValue::Tags(summary.concern_themes.clone()),
            "Resident watch themes: {value}",
            &["traffic", "water", "noise", "parking", "concerns"],
            None,
            source_url.clone(),
            facts,
            annotations,
        )?;
    }
    if !summary.review_highlights.is_empty() {
        push_fact(
            summary,
            run_id,
            "community_review_highlights",
            FactValue::Tags(summary.review_highlights.clone()),
            "Review highlights: {value}",
            &[
                "review highlights",
                "resident feedback",
                "google reviews",
                "community signal",
            ],
            Some(("TextMatch", 1.1, Vec::new())),
            source_url.clone(),
            facts,
            annotations,
        )?;
    }
    if !summary.source_urls.is_empty() {
        push_fact(
            summary,
            run_id,
            "community_evidence_links",
            FactValue::Tags(summary.source_urls.clone()),
            "Community evidence links: {value}",
            &["reviews", "proof", "source links"],
            Some(("TextMatch", 0.2, Vec::new())),
            source_url,
            facts,
            annotations,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    summary: &CommunityEntitySummary,
    run_id: &MaterializationId,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    answers_preferences: &[&str],
    scoring: Option<(&str, f32, Vec<f64>)>,
    source_url: Option<String>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), CommunitySummaryAssetError> {
    let value_type = match &value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: summary.entity_id.clone(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence: summary.confidence,
        source_type: "Computed".to_string(),
        source_url,
        model: summary.model.clone(),
        skill_id: Some(COMMUNITY_SUMMARIZER_SKILL_ID.to_string()),
        triggered_by: Some("asset_dag".to_string()),
        learned_at: summary.learned_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(
                format!(
                    "{}:{fact_key}:{}:{}",
                    summary.entity_id, summary.evidence_count, value_json
                )
                .as_bytes()
            )
        ),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: summary.entity_id.clone(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: scoring
            .as_ref()
            .map(|(direction, _, _)| direction.to_string()),
        scoring_weight: scoring.as_ref().map(|(_, weight, _)| *weight),
        scoring_thresholds_json: serde_json::to_string(
            &scoring.map_or_else(Vec::new, |(_, _, thresholds)| thresholds),
        )?,
    });
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
pub enum CommunitySummaryAssetError {
    SkillFacts(SkillFactMaterializeError),
    Json(serde_json::Error),
}

impl fmt::Display for CommunitySummaryAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SkillFacts(err) => write!(f, "community summary support read failed: {err}"),
            Self::Json(err) => write!(f, "community summary JSON conversion failed: {err}"),
        }
    }
}

impl std::error::Error for CommunitySummaryAssetError {}

impl From<SkillFactMaterializeError> for CommunitySummaryAssetError {
    fn from(err: SkillFactMaterializeError) -> Self {
        Self::SkillFacts(err)
    }
}

impl From<serde_json::Error> for CommunitySummaryAssetError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}
