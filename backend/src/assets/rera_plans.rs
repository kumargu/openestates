use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::dag_config;
use crate::knowledge::FactValue;

use super::{SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark};

pub const RERA_PROJECT_PLAN_FRAMES_ASSET_ID: &str = "rera_project_plan_frames";
const PROJECT_PLAN_FRAMES_FACT_KEY: &str = "media.project_plan_frames";

#[derive(Debug, Deserialize)]
struct ReraProjectPlanTargets {
    #[serde(default)]
    projects: Vec<ReraProjectPlanTarget>,
}

#[derive(Debug, Deserialize)]
struct ReraProjectPlanTarget {
    society_slug: String,
    society_entity_id: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    registration_number: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectPlanFramesPayload {
    provider: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    floor_plans: Vec<serde_json::Value>,
}

pub fn rera_project_plan_frames_input(
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, ReraPlanFramesAssetError> {
    let targets = load_targets()?;
    let repo_root = repo_root();
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut seen_facts = BTreeSet::<(String, String)>::new();
    let mut watermarks = Vec::new();

    for target in targets.projects {
        let fact_identity = (
            target.society_entity_id.clone(),
            PROJECT_PLAN_FRAMES_FACT_KEY.to_string(),
        );
        if !seen_facts.insert(fact_identity) {
            return Err(ReraPlanFramesAssetError::DuplicateTarget {
                society_entity_id: target.society_entity_id,
            });
        }

        let path = plan_payload_path(&repo_root, &target.society_slug);
        if !path.exists() {
            watermarks.push(SourceWatermark {
                source: format!("rera_project_plan_frames:{}", target.society_slug),
                high_watermark: "missing".to_string(),
            });
            continue;
        }

        let payload_text = std::fs::read_to_string(&path).map_err(ReraPlanFramesAssetError::Io)?;
        let payload: ProjectPlanFramesPayload =
            serde_json::from_str(&payload_text).map_err(ReraPlanFramesAssetError::Json)?;
        if !payload.provider.eq_ignore_ascii_case("rera") {
            return Err(ReraPlanFramesAssetError::NonReraProvider {
                society_slug: target.society_slug,
                provider: payload.provider,
            });
        }

        let source_url = payload
            .source_url
            .clone()
            .or_else(|| target.source_url.clone());
        let fact_key = PROJECT_PLAN_FRAMES_FACT_KEY.to_string();

        facts.push(SkillFactRecord {
            entity_id: target.society_entity_id.clone(),
            fact_key: fact_key.clone(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text(payload_text.clone()))
                .map_err(ReraPlanFramesAssetError::Json)?,
            confidence: if payload.floor_plans.is_empty() {
                0.55
            } else {
                0.9
            },
            source_type: "Rera".to_string(),
            source_url: source_url.clone(),
            model: None,
            skill_id: Some("promote_rera_project_plans".to_string()),
            triggered_by: target.registration_number.clone(),
            learned_at,
            run_id: run_id.to_string(),
            input_hash: sha256_hex(payload_text.as_bytes()),
        });
        annotations.push(SkillFactAnnotationRecord {
            entity_id: target.society_entity_id,
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
            source: format!("rera_project_plan_frames:{}", target.society_slug),
            high_watermark: path.display().to_string(),
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

fn load_targets() -> Result<ReraProjectPlanTargets, ReraPlanFramesAssetError> {
    let path = dag_config::dag_root().join("rera_project_plan_targets.json");
    dag_config::load_json(&path).map_err(ReraPlanFramesAssetError::Config)
}

fn plan_payload_path(repo_root: &Path, society_slug: &str) -> PathBuf {
    repo_root
        .join("data")
        .join("lake")
        .join("media")
        .join("rera_plans")
        .join(society_slug)
        .join("project_plan_frames.json")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate lives in the OpenEstates repo")
        .to_path_buf()
}

#[derive(Debug)]
pub enum ReraPlanFramesAssetError {
    Config(dag_config::DagConfigError),
    DuplicateTarget {
        society_entity_id: String,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    NonReraProvider {
        society_slug: String,
        provider: String,
    },
}

impl fmt::Display for ReraPlanFramesAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "RERA project plan target config error: {err}"),
            Self::DuplicateTarget { society_entity_id } => {
                write!(f, "duplicate RERA plan target for {society_entity_id}")
            }
            Self::Io(err) => write!(f, "RERA project plan IO error: {err}"),
            Self::Json(err) => write!(f, "RERA project plan JSON error: {err}"),
            Self::NonReraProvider {
                society_slug,
                provider,
            } => write!(
                f,
                "RERA project plan target {society_slug} has non-RERA provider {provider}"
            ),
        }
    }
}

impl std::error::Error for ReraPlanFramesAssetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_payload_path_is_lake_media_path() {
        let path = plan_payload_path(Path::new("/repo"), "prestige-waterford");
        assert_eq!(
            path,
            Path::new("/repo")
                .join("data/lake/media/rera_plans/prestige-waterford/project_plan_frames.json")
        );
    }
}
