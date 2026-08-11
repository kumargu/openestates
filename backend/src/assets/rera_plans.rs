use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;
use crate::lake::keys::KeyError;
use crate::lake::{LakeError, LakeKey, LakePrefix, LakeStore};

use super::{SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark};

pub const RERA_PROJECT_PLAN_FRAMES_ASSET_ID: &str = "rera_project_plan_frames";
const PROJECT_PLAN_FRAMES_FACT_KEY: &str = "media.project_plan_frames";
const PROJECT_PLAN_PAYLOAD_PREFIX: &str = "media/rera_plans";
const PROJECT_PLAN_PAYLOAD_NAME: &str = "project_plan_frames.json";

#[derive(Debug, Deserialize)]
struct ProjectPlanFramesPayload {
    provider: String,
    society_entity_id: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    registration_number: Option<String>,
    #[serde(default)]
    site_overview: Option<serde_json::Value>,
    #[serde(default)]
    floor_plans: Vec<serde_json::Value>,
    #[serde(default)]
    filed_plan_previews: Vec<serde_json::Value>,
}

impl ProjectPlanFramesPayload {
    fn has_renderable_preview(&self) -> bool {
        self.site_overview.is_some()
            || !self.floor_plans.is_empty()
            || !self.filed_plan_previews.is_empty()
    }
}

pub async fn rera_project_plan_frames_input(
    lake: &LakeStore,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, ReraPlanFramesAssetError> {
    let payload_keys = discover_plan_payloads(lake).await?;
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut seen_facts = BTreeSet::<(String, String)>::new();
    let mut watermarks = Vec::new();

    for key in payload_keys {
        let payload_text = lake.get_text(&key).await?;
        let payload: ProjectPlanFramesPayload =
            serde_json::from_str(&payload_text).map_err(ReraPlanFramesAssetError::Json)?;
        if !payload.provider.eq_ignore_ascii_case("rera") {
            return Err(ReraPlanFramesAssetError::NonReraProvider {
                key,
                provider: payload.provider,
            });
        }
        if payload.society_entity_id.trim().is_empty() {
            return Err(ReraPlanFramesAssetError::MissingSocietyEntityId { key });
        }
        if !payload.has_renderable_preview() {
            continue;
        }

        let fact_identity = (
            payload.society_entity_id.clone(),
            PROJECT_PLAN_FRAMES_FACT_KEY.to_string(),
        );
        if !seen_facts.insert(fact_identity) {
            return Err(ReraPlanFramesAssetError::DuplicateTarget {
                society_entity_id: payload.society_entity_id,
            });
        }

        let source_url = payload.source_url.clone();
        let fact_key = PROJECT_PLAN_FRAMES_FACT_KEY.to_string();

        facts.push(SkillFactRecord {
            entity_id: payload.society_entity_id.clone(),
            fact_key: fact_key.clone(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text(payload_text.clone()))
                .map_err(ReraPlanFramesAssetError::Json)?,
            confidence: if payload.floor_plans.is_empty() {
                0.75
            } else {
                0.9
            },
            source_type: "Rera".to_string(),
            source_url: source_url.clone(),
            model: None,
            skill_id: Some("promote_rera_project_plans".to_string()),
            triggered_by: payload.registration_number.clone(),
            learned_at,
            run_id: run_id.to_string(),
            input_hash: sha256_hex(payload_text.as_bytes()),
        });
        annotations.push(SkillFactAnnotationRecord {
            entity_id: payload.society_entity_id,
            fact_key,
            display_template: Some("project plans available".to_string()),
            answers_preferences_json: serde_json::to_string(&[
                "floor plan",
                "site overview",
                "layout",
            ])
            .map_err(ReraPlanFramesAssetError::Json)?,
            scoring_direction: Some("text_match".to_string()),
            scoring_weight: Some(0.4),
            scoring_thresholds_json: "[]".to_string(),
        });
        watermarks.push(SourceWatermark {
            source: "rera_project_plan_frames".to_string(),
            high_watermark: format!("sha256:{}", sha256_hex(payload_text.as_bytes())),
        });
    }

    if watermarks.is_empty() {
        watermarks.push(SourceWatermark {
            source: "rera_project_plan_frames_empty".to_string(),
            high_watermark: "no_renderable_previews".to_string(),
        });
    }

    Ok(SkillFactsInput {
        source: "rera_project_plan_frames".to_string(),
        snapshot_date: learned_at.date_naive().to_string(),
        facts,
        fact_annotations: annotations,
        source_watermarks: watermarks,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn discover_plan_payloads(
    lake: &LakeStore,
) -> Result<Vec<LakeKey>, ReraPlanFramesAssetError> {
    let prefix = LakePrefix::new(PROJECT_PLAN_PAYLOAD_PREFIX)?;
    let keys = match lake.list_keys(&prefix).await {
        Ok(keys) => keys,
        Err(err) if err.is_not_found() => Vec::new(),
        Err(err) => return Err(err.into()),
    };
    Ok(keys
        .into_iter()
        .filter(|key| {
            key.as_str()
                .ends_with(&format!("/{PROJECT_PLAN_PAYLOAD_NAME}"))
        })
        .collect())
}

#[derive(Debug)]
pub enum ReraPlanFramesAssetError {
    DuplicateTarget { society_entity_id: String },
    MissingSocietyEntityId { key: LakeKey },
    Key(KeyError),
    Lake(LakeError),
    Json(serde_json::Error),
    NonReraProvider { key: LakeKey, provider: String },
}

impl fmt::Display for ReraPlanFramesAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTarget { society_entity_id } => {
                write!(f, "duplicate RERA plan payload for {society_entity_id}")
            }
            Self::MissingSocietyEntityId { key } => {
                write!(f, "RERA plan payload {} has no society entity ID", key)
            }
            Self::Key(err) => write!(f, "invalid RERA project plan lake key: {err}"),
            Self::Lake(err) => write!(f, "RERA project plan lake error: {err}"),
            Self::Json(err) => write!(f, "RERA project plan JSON error: {err}"),
            Self::NonReraProvider { key, provider } => write!(
                f,
                "RERA project plan payload {} has non-RERA provider {provider}",
                key
            ),
        }
    }
}

impl std::error::Error for ReraPlanFramesAssetError {}

impl From<KeyError> for ReraPlanFramesAssetError {
    fn from(err: KeyError) -> Self {
        Self::Key(err)
    }
}

impl From<LakeError> for ReraPlanFramesAssetError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_payload_prefix_is_relative_to_the_configured_lake() {
        assert_eq!(PROJECT_PLAN_PAYLOAD_PREFIX, "media/rera_plans");
    }

    #[tokio::test]
    async fn missing_plan_payloads_emit_skipped_watermark() {
        let root = tempfile::tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let input = rera_project_plan_frames_input(&lake, "test-run", Utc::now())
            .await
            .unwrap();
        assert!(input.facts.is_empty());
        assert_eq!(
            input.source_watermarks[0].high_watermark,
            "no_renderable_previews"
        );
    }
}
