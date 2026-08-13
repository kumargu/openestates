use std::fmt;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark};

pub const RERA_PROJECT_PLAN_FRAMES_ASSET_ID: &str = "rera_project_plan_frames";
const PROJECT_PLAN_FRAMES_FACT_KEY: &str = "media.project_plan_frames";
const MAX_PREVIEWS_PER_SOCIETY: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReraProjectPlanFramesInput {
    pub source: String,
    pub snapshot_date: String,
    #[serde(default)]
    pub catalog_entity_count: usize,
    #[serde(default)]
    pub exact_registration_count: usize,
    #[serde(default)]
    pub projects: Vec<ReraProjectPlanProjectInput>,
    #[serde(default)]
    pub failures: Vec<ReraProjectPlanFailureInput>,
    #[serde(default)]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraProjectPlanProjectInput {
    pub society_entity_id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub registration_number: String,
    #[serde(default)]
    pub previews: Vec<ReraProjectPlanPreviewInput>,
    #[serde(default)]
    pub payload_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReraProjectPlanPreviewInput {
    pub artifact_id: String,
    pub kind: String,
    pub role: String,
    pub buyer_label: String,
    pub source_url: String,
    pub source_hash: String,
    pub source_cache_relative_path: String,
    pub preview_hash: String,
    pub cache_relative_path: String,
    pub page: u32,
    pub confidence: f64,
    pub status: String,
    #[serde(default)]
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReraProjectPlanFailureInput {
    pub society_entity_id: String,
    #[serde(default)]
    pub registration_number: Option<String>,
    pub reason: String,
}

#[derive(Debug, Serialize)]
struct ProjectPlanFramesPayload<'a> {
    provider: &'static str,
    coverage_quality: &'static str,
    registration_number: &'a str,
    society_entity_id: &'a str,
    floor_plans: [serde_json::Value; 0],
    filed_plan_previews: Vec<PromotedFiledPlanPreview<'a>>,
}

#[derive(Debug, Serialize)]
struct PromotedFiledPlanPreview<'a> {
    artifact_id: &'a str,
    kind: &'a str,
    label: &'a str,
    preview_url: String,
    thumbnail_url: String,
    page: u32,
    confidence: f64,
    source_lineage: PreviewSourceLineage<'a>,
}

#[derive(Debug, Serialize)]
struct PreviewSourceLineage<'a> {
    source_type: &'static str,
    registration_number: &'a str,
    source_url: &'a str,
    source_hash: &'a str,
    preview_hash: &'a str,
}

pub async fn rera_project_plan_frames_input(
    lake: &LakeStore,
    input: &ReraProjectPlanFramesInput,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, ReraPlanFramesAssetError> {
    let cache_root = repo_root().join("data/cache");
    rera_project_plan_frames_input_with_cache_root(lake, input, &cache_root, run_id, learned_at)
        .await
}

async fn rera_project_plan_frames_input_with_cache_root(
    lake: &LakeStore,
    input: &ReraProjectPlanFramesInput,
    cache_root: &Path,
    run_id: &str,
    learned_at: DateTime<Utc>,
) -> Result<SkillFactsInput, ReraPlanFramesAssetError> {
    let mut projects = input.projects.clone();
    projects.sort_by(|left, right| left.society_entity_id.cmp(&right.society_entity_id));
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut watermarks = input.source_watermarks.clone();

    for failure in &input.failures {
        watermarks.push(SourceWatermark {
            source: format!(
                "rera_project_plan_frames_skipped:{}",
                failure.society_entity_id
            ),
            high_watermark: failure.reason.clone(),
        });
    }

    for project in &projects {
        let mut promoted = Vec::new();
        for preview in project.previews.iter().take(MAX_PREVIEWS_PER_SOCIETY) {
            match promote_preview(lake, cache_root, &project.registration_number, preview).await {
                Ok(preview) => promoted.push(preview),
                Err(reason) => watermarks.push(SourceWatermark {
                    source: format!(
                        "rera_project_plan_frames_skipped:{}:{}",
                        project.society_entity_id, preview.artifact_id
                    ),
                    high_watermark: reason,
                }),
            }
        }
        if promoted.is_empty() {
            continue;
        }

        let source_url = promoted
            .first()
            .map(|preview| preview.source_lineage.source_url.to_string());
        let payload = ProjectPlanFramesPayload {
            provider: "RERA",
            coverage_quality: "usable",
            registration_number: &project.registration_number,
            society_entity_id: &project.society_entity_id,
            floor_plans: [],
            filed_plan_previews: promoted,
        };
        let payload_text = serde_json::to_string(&payload)?;
        facts.push(SkillFactRecord {
            entity_id: project.society_entity_id.clone(),
            fact_key: PROJECT_PLAN_FRAMES_FACT_KEY.to_string(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text(payload_text.clone()))?,
            confidence: project
                .previews
                .iter()
                .take(MAX_PREVIEWS_PER_SOCIETY)
                .map(|preview| preview.confidence as f32)
                .fold(0.0, f32::max)
                .min(0.9),
            source_type: "Rera".to_string(),
            source_url,
            model: None,
            skill_id: Some("fetch_rera".to_string()),
            triggered_by: Some(project.registration_number.clone()),
            learned_at,
            run_id: run_id.to_string(),
            input_hash: sha256_hex(payload_text.as_bytes()),
        });
        annotations.push(SkillFactAnnotationRecord {
            entity_id: project.society_entity_id.clone(),
            fact_key: PROJECT_PLAN_FRAMES_FACT_KEY.to_string(),
            display_template: Some("Filed plans available".to_string()),
            answers_preferences_json: serde_json::to_string(&[
                "site plan",
                "floor plan",
                "layout",
            ])?,
            scoring_direction: Some("TextMatch".to_string()),
            scoring_weight: Some(0.4),
            scoring_thresholds_json: "[]".to_string(),
        });
    }

    Ok(SkillFactsInput {
        source: "rera_project_plan_frames".to_string(),
        snapshot_date: input.snapshot_date.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: watermarks,
    })
}

async fn promote_preview<'a>(
    lake: &LakeStore,
    cache_root: &Path,
    registration_number: &'a str,
    preview: &'a ReraProjectPlanPreviewInput,
) -> Result<PromotedFiledPlanPreview<'a>, String> {
    if preview.status != "accepted" {
        return Err(preview
            .rejection_reason
            .clone()
            .unwrap_or_else(|| "preview was not accepted".to_string()));
    }
    let source_path = cache_path(cache_root, &preview.source_cache_relative_path)?;
    let source_bytes = std::fs::read(&source_path)
        .map_err(|err| format!("could not read cached source PDF: {err}"))?;
    if !source_bytes.starts_with(b"%PDF") || sha256_hex(&source_bytes) != preview.source_hash {
        return Err("cached source PDF hash does not match metadata".to_string());
    }

    let preview_path = cache_path(cache_root, &preview.cache_relative_path)?;
    let preview_bytes = std::fs::read(&preview_path)
        .map_err(|err| format!("could not read cached preview: {err}"))?;
    if !preview_bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || sha256_hex(&preview_bytes) != preview.preview_hash
    {
        return Err("cached preview hash does not match metadata".to_string());
    }

    let society_slug = preview_slug(registration_number);
    let relative_key = format!(
        "media/previews/rera_plans/{society_slug}/{}.png",
        preview.preview_hash
    );
    let key = LakeKey::new(relative_key.clone()).map_err(|err| err.to_string())?;
    lake.put_bytes(&key, preview_bytes)
        .await
        .map_err(|err| err.to_string())?;
    let preview_url = format!("/{relative_key}");
    Ok(PromotedFiledPlanPreview {
        artifact_id: &preview.artifact_id,
        kind: &preview.kind,
        label: &preview.buyer_label,
        preview_url: preview_url.clone(),
        thumbnail_url: preview_url,
        page: preview.page,
        confidence: preview.confidence,
        source_lineage: PreviewSourceLineage {
            source_type: "Rera",
            registration_number,
            source_url: &preview.source_url,
            source_hash: &preview.source_hash,
            preview_hash: &preview.preview_hash,
        },
    })
}

fn cache_path(cache_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("preview cache path must be a normalized relative path".to_string());
    }
    Ok(cache_root.join(path))
}

fn preview_slug(registration_number: &str) -> String {
    format!("rera-{}", &sha256_hex(registration_number.as_bytes())[..16])
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate lives in the OpenEstates repo")
        .to_path_buf()
}

#[derive(Debug)]
pub enum ReraPlanFramesAssetError {
    Json(serde_json::Error),
    Lake(LakeError),
}

impl fmt::Display for ReraPlanFramesAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "RERA project plan JSON error: {error}"),
            Self::Lake(error) => write!(formatter, "RERA project plan lake error: {error}"),
        }
    }
}

impl std::error::Error for ReraPlanFramesAssetError {}

impl From<serde_json::Error> for ReraPlanFramesAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<LakeError> for ReraPlanFramesAssetError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn verifies_hashes_and_materializes_canonical_plan_fact() {
        let root = tempdir().unwrap();
        let cache = root.path().join("cache");
        let lake = LakeStore::local(root.path().join("lake")).unwrap();
        let pdf = b"%PDF-test";
        let png = b"\x89PNG\r\n\x1a\nplan";
        let source_hash = sha256_hex(pdf);
        let preview_hash = sha256_hex(png);
        let source_relative =
            format!("rera_project_plan_frames/objects/documents/{source_hash}.pdf");
        let preview_relative =
            format!("rera_project_plan_frames/objects/previews/{preview_hash}.png");
        let source_path = cache.join(&source_relative);
        let preview_path = cache.join(&preview_relative);
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(preview_path.parent().unwrap()).unwrap();
        std::fs::write(source_path, pdf).unwrap();
        std::fs::write(preview_path, png).unwrap();
        let input = ReraProjectPlanFramesInput {
            source: "rera".to_string(),
            snapshot_date: "2026-08-12".to_string(),
            projects: vec![ReraProjectPlanProjectInput {
                society_entity_id: "society:rera-canonical".to_string(),
                aliases: vec!["society:brigade-laguna".to_string()],
                registration_number: "PRM/TEST/1".to_string(),
                previews: vec![ReraProjectPlanPreviewInput {
                    artifact_id: "plan-1".to_string(),
                    kind: "site_plan".to_string(),
                    role: "site_plan".to_string(),
                    buyer_label: "Site plan".to_string(),
                    source_url: "https://rera.test/plan".to_string(),
                    source_hash,
                    source_cache_relative_path: source_relative,
                    preview_hash,
                    cache_relative_path: preview_relative,
                    page: 1,
                    confidence: 0.85,
                    status: "accepted".to_string(),
                    rejection_reason: None,
                }],
                payload_hash: None,
            }],
            ..ReraProjectPlanFramesInput::default()
        };

        let result = rera_project_plan_frames_input_with_cache_root(
            &lake,
            &input,
            &cache,
            "run-1",
            Utc::now(),
        )
        .await
        .unwrap();

        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].entity_id, "society:rera-canonical");
        assert!(result.facts[0].value_json.contains("Site plan"));
        assert!(!result.facts[0].value_json.contains("status"));
    }

    #[tokio::test]
    async fn skips_one_bad_preview_without_aborting_other_projects() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path().join("lake")).unwrap();
        let input = ReraProjectPlanFramesInput {
            source: "rera".to_string(),
            snapshot_date: "2026-08-12".to_string(),
            projects: vec![ReraProjectPlanProjectInput {
                society_entity_id: "society:rera-canonical".to_string(),
                aliases: Vec::new(),
                registration_number: "PRM/TEST/1".to_string(),
                previews: vec![ReraProjectPlanPreviewInput {
                    artifact_id: "missing".to_string(),
                    kind: "site_plan".to_string(),
                    role: "site_plan".to_string(),
                    buyer_label: "Site plan".to_string(),
                    source_url: "https://rera.test/plan".to_string(),
                    source_hash: "bad".to_string(),
                    source_cache_relative_path: "missing.pdf".to_string(),
                    preview_hash: "bad".to_string(),
                    cache_relative_path: "missing.png".to_string(),
                    page: 1,
                    confidence: 0.85,
                    status: "accepted".to_string(),
                    rejection_reason: None,
                }],
                payload_hash: None,
            }],
            ..ReraProjectPlanFramesInput::default()
        };

        let result = rera_project_plan_frames_input_with_cache_root(
            &lake,
            &input,
            root.path(),
            "run-1",
            Utc::now(),
        )
        .await
        .unwrap();

        assert!(result.facts.is_empty());
        assert!(result
            .source_watermarks
            .iter()
            .any(|watermark| watermark.source.contains("missing")));
    }
}
