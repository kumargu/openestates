use std::collections::BTreeMap;
use std::fmt;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::dag_config;
use crate::lake::{LakeError, LakeStore};

use super::skill_facts::{write_fact_annotations_parquet, write_facts_parquet};
use super::{
    read_skill_fact_artifact_rows, ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, AssetStage, MaterializationId, MaterializationRecord,
    SkillFactAnnotationRecord, SkillFactMaterializeError, SkillFactRecord, SourceWatermark,
};

pub const CURRENT_PROJECT_FACTS_ASSET_ID: &str = "current_project_facts";
const CURRENT_PROJECT_FACTS_FORMAT_VERSION: u32 = 1;
const PROJECT_CLAIM_FACTS_POLICY_ID: &str = "project_claim_facts";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompactionPolicyFile {
    policies: Vec<CompactionPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompactionPolicy {
    id: String,
    enabled: bool,
    target_file_size_mb: u64,
    small_file_threshold: u64,
    delta_row_threshold: u64,
    delta_byte_threshold_mb: u64,
    max_delta_age_hours: u64,
    #[serde(default)]
    output: CompactionPolicyOutput,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CompactionPolicyOutput {
    file_name_template: Option<String>,
    compression: Option<String>,
    rows_per_row_group: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentProjectFactsManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub policy_id: String,
    pub policy_enabled: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub input_fact_count: u64,
    pub output_fact_count: u64,
    pub input_annotation_count: u64,
    pub output_annotation_count: u64,
    pub parent_materializations: Vec<MaterializationId>,
    pub fact_parquet_key: String,
    pub fact_annotation_parquet_key: String,
    pub manifest_key: String,
}

#[derive(Debug, Clone)]
pub struct CurrentProjectFactsMaterialization {
    pub manifest: CurrentProjectFactsManifest,
    pub record: MaterializationRecord,
}

#[derive(Clone)]
pub struct CurrentProjectFactsMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl CurrentProjectFactsMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_for_run(
        &self,
        parent_records: &[MaterializationRecord],
        version: &str,
        dag_run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<CurrentProjectFactsMaterialization, CurrentProjectFactsError> {
        let policy = load_project_claim_facts_policy()?;
        let rows = read_skill_fact_artifact_rows(&self.lake, parent_records).await?;
        let input_fact_count = rows.facts.len() as u64;
        let input_annotation_count = rows.fact_annotations.len() as u64;
        let facts = compact_fact_records(rows.facts);
        let fact_annotations = compact_fact_annotations(rows.fact_annotations);
        let part_file_name = compaction_part_file_name(&policy, 0);

        let fact_key = AssetPathBuilder::gold_asset_key(
            CURRENT_PROJECT_FACTS_ASSET_ID,
            version,
            &format!("facts/{part_file_name}"),
        );
        let fact_meta = self
            .lake
            .put_bytes(&fact_key, write_facts_parquet(&facts)?)
            .await?;

        let fact_annotation_key = AssetPathBuilder::gold_asset_key(
            CURRENT_PROJECT_FACTS_ASSET_ID,
            version,
            &format!("fact_annotations/{part_file_name}"),
        );
        let fact_annotation_meta = self
            .lake
            .put_bytes(
                &fact_annotation_key,
                write_fact_annotations_parquet(&fact_annotations)?,
            )
            .await?;

        let manifest_key = AssetPathBuilder::gold_asset_key(
            CURRENT_PROJECT_FACTS_ASSET_ID,
            version,
            "manifest.json",
        );
        let parent_materializations = parent_records
            .iter()
            .map(|record| record.materialization_id.clone())
            .collect::<Vec<_>>();
        let mut artifacts = vec![
            ArtifactRef::parquet(fact_meta),
            ArtifactRef::parquet(fact_annotation_meta),
        ];
        let manifest = CurrentProjectFactsManifest {
            asset_id: CURRENT_PROJECT_FACTS_ASSET_ID.to_string(),
            format_version: CURRENT_PROJECT_FACTS_FORMAT_VERSION,
            policy_id: policy.id.clone(),
            policy_enabled: policy.enabled,
            created_at: Utc::now(),
            input_fact_count,
            output_fact_count: facts.len() as u64,
            input_annotation_count,
            output_annotation_count: fact_annotations.len() as u64,
            parent_materializations: parent_materializations.clone(),
            fact_parquet_key: fact_key.to_string(),
            fact_annotation_parquet_key: fact_annotation_key.to_string(),
            manifest_key: manifest_key.to_string(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let asset = AssetId::new(CURRENT_PROJECT_FACTS_ASSET_ID)
            .expect("static current project facts asset id is valid");
        let record = MaterializationRecord::succeeded(
            asset,
            AssetStage::Gold,
            partition,
            version,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(compaction_watermarks(&policy, parent_records))
        .with_row_count(facts.len() as u64);
        self.materializations.write_materialization(&record).await?;

        Ok(CurrentProjectFactsMaterialization { manifest, record })
    }
}

fn load_project_claim_facts_policy() -> Result<CompactionPolicy, CurrentProjectFactsError> {
    let path = dag_config::dag_root().join("compaction_policies.json");
    let file: CompactionPolicyFile = dag_config::load_json(&path)?;
    file.policies
        .into_iter()
        .find(|policy| policy.id == PROJECT_CLAIM_FACTS_POLICY_ID)
        .ok_or_else(|| {
            CurrentProjectFactsError::PolicyMissing(PROJECT_CLAIM_FACTS_POLICY_ID.to_string())
        })
}

fn compaction_part_file_name(policy: &CompactionPolicy, part_number: u32) -> String {
    let part_number = format!("{part_number:05}");
    policy
        .output
        .file_name_template
        .as_deref()
        .unwrap_or("part-{part_number}.parquet")
        .replace("{part_number}", &part_number)
}

fn compact_fact_records(records: Vec<SkillFactRecord>) -> Vec<SkillFactRecord> {
    let mut by_key: BTreeMap<(String, String, String, String), SkillFactRecord> = BTreeMap::new();
    for record in records {
        let key = (
            record.entity_id.clone(),
            record.fact_key.clone(),
            record.source_type.clone(),
            record.source_url.clone().unwrap_or_default(),
        );
        match by_key.get(&key) {
            Some(existing) if fact_precedes(&record, existing) => {}
            _ => {
                by_key.insert(key, record);
            }
        }
    }
    by_key.into_values().collect()
}

fn fact_precedes(candidate: &SkillFactRecord, existing: &SkillFactRecord) -> bool {
    candidate.confidence < existing.confidence
        || (candidate.confidence == existing.confidence
            && candidate.learned_at <= existing.learned_at)
}

fn compact_fact_annotations(
    records: Vec<SkillFactAnnotationRecord>,
) -> Vec<SkillFactAnnotationRecord> {
    let mut by_key: BTreeMap<(String, String), SkillFactAnnotationRecord> = BTreeMap::new();
    for record in records {
        by_key.insert((record.entity_id.clone(), record.fact_key.clone()), record);
    }
    by_key.into_values().collect()
}

fn compaction_watermarks(
    policy: &CompactionPolicy,
    parent_records: &[MaterializationRecord],
) -> Vec<SourceWatermark> {
    let mut watermarks = vec![SourceWatermark {
        source: "compaction_policy".to_string(),
        high_watermark: format!(
            "{}:enabled={},small_files={},delta_rows={},delta_mb={},max_age_h={},target_mb={},compression={},row_group_rows={}",
            policy.id,
            policy.enabled,
            policy.small_file_threshold,
            policy.delta_row_threshold,
            policy.delta_byte_threshold_mb,
            policy.max_delta_age_hours,
            policy.target_file_size_mb,
            policy.output.compression.as_deref().unwrap_or("zstd"),
            policy
                .output
                .rows_per_row_group
                .map(|rows| rows.to_string())
                .unwrap_or_else(|| "default".to_string())
        ),
    }];
    watermarks.extend(
        parent_records
            .iter()
            .flat_map(|record| record.source_watermarks.clone()),
    );
    watermarks
}

#[derive(Debug)]
pub enum CurrentProjectFactsError {
    Config(dag_config::DagConfigError),
    Lake(LakeError),
    SkillFact(SkillFactMaterializeError),
    PolicyMissing(String),
}

impl fmt::Display for CurrentProjectFactsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "current project facts config error: {err}"),
            Self::Lake(err) => write!(f, "current project facts lake error: {err}"),
            Self::SkillFact(err) => write!(f, "current project facts parquet error: {err}"),
            Self::PolicyMissing(policy) => write!(f, "compaction policy {policy:?} is missing"),
        }
    }
}

impl std::error::Error for CurrentProjectFactsError {}

impl From<dag_config::DagConfigError> for CurrentProjectFactsError {
    fn from(err: dag_config::DagConfigError) -> Self {
        Self::Config(err)
    }
}

impl From<LakeError> for CurrentProjectFactsError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<SkillFactMaterializeError> for CurrentProjectFactsError {
    fn from(err: SkillFactMaterializeError) -> Self {
        Self::SkillFact(err)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn fact(entity_id: &str, fact_key: &str, confidence: f32, learned_at: i64) -> SkillFactRecord {
        SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "text".to_string(),
            value_json: "\"value\"".to_string(),
            confidence,
            source_type: "Rera".to_string(),
            source_url: Some("https://example.test".to_string()),
            model: None,
            skill_id: Some("test".to_string()),
            triggered_by: None,
            learned_at: Utc.timestamp_opt(learned_at, 0).unwrap(),
            run_id: "run".to_string(),
            input_hash: "hash".to_string(),
        }
    }

    #[test]
    fn compact_fact_records_keeps_highest_confidence_then_freshest() {
        let mut older = fact("society:one", "project_unit_count", 0.8, 10);
        older.value_json = "\"older\"".to_string();
        let mut fresher = fact("society:one", "project_unit_count", 0.8, 20);
        fresher.value_json = "\"fresher\"".to_string();
        let weaker = fact("society:one", "project_unit_count", 0.6, 30);

        let compacted = compact_fact_records(vec![older, fresher, weaker]);

        assert_eq!(compacted.len(), 1);
        assert_eq!(compacted[0].value_json, "\"fresher\"");
    }

    #[test]
    fn compaction_part_file_name_uses_policy_template_with_padded_number() {
        let policy = CompactionPolicy {
            id: "project_claim_facts".to_string(),
            enabled: true,
            target_file_size_mb: 128,
            small_file_threshold: 24,
            delta_row_threshold: 5000,
            delta_byte_threshold_mb: 64,
            max_delta_age_hours: 24,
            output: CompactionPolicyOutput {
                file_name_template: Some("chunk-{part_number}.parquet".to_string()),
                compression: Some("zstd".to_string()),
                rows_per_row_group: Some(20_000),
            },
        };

        assert_eq!(compaction_part_file_name(&policy, 7), "chunk-00007.parquet");
    }
}
