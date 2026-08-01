use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dag_config::{
    self, load_resolution_policies, normalize_source_type, resolve_coordinate_pair,
    CoordinateEntityScope, CoordinatePairCandidate,
};
use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeStore};

use super::skill_facts::{write_fact_annotations_parquet, write_facts_parquet};
use super::{
    read_skill_fact_artifact_rows, ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, AssetStage, MaterializationId, MaterializationRecord,
    SkillFactAnnotationRecord, SkillFactMaterializeError, SkillFactRecord,
    SourceEntityResolutionScope, SourceEntitySeed, SourceWatermark,
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
        source_entities: &[SourceEntitySeed],
        source_scope: SourceEntityResolutionScope,
        learned_at: DateTime<Utc>,
    ) -> Result<CurrentProjectFactsMaterialization, CurrentProjectFactsError> {
        let policy = load_project_claim_facts_policy()?;
        let rows = read_skill_fact_artifact_rows(&self.lake, parent_records).await?;
        let scoped_aliases = scoped_alias_map(source_entities, source_scope);
        let run_id = dag_run_id.to_string();
        let mut input_facts =
            scoped_fact_records(rows.facts, source_entities, source_scope, &run_id)?;
        input_facts.extend(source_entity_coordinate_facts(
            source_entities,
            &dag_run_id,
            learned_at,
        ));
        let input_fact_count = input_facts.len() as u64;
        let scoped_fact_keys = input_facts
            .iter()
            .map(|fact| (fact.entity_id.as_str(), fact.fact_key.as_str()))
            .collect::<HashSet<_>>();
        let input_annotations = rows
            .fact_annotations
            .into_iter()
            .map(|annotation| canonicalize_scoped_annotation(annotation, &scoped_aliases))
            .filter(|annotation| {
                scoped_fact_keys
                    .contains(&(annotation.entity_id.as_str(), annotation.fact_key.as_str()))
            })
            .collect::<Vec<_>>();
        let input_annotation_count = input_annotations.len() as u64;
        let facts = compact_fact_records(input_facts)?;
        let fact_annotations = compact_fact_annotations(input_annotations);
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

fn scoped_fact_records(
    records: Vec<SkillFactRecord>,
    source_entities: &[SourceEntitySeed],
    source_scope: SourceEntityResolutionScope,
    run_id: &str,
) -> Result<Vec<SkillFactRecord>, CurrentProjectFactsError> {
    if source_scope == SourceEntityResolutionScope::Production {
        return Ok(records);
    }
    if source_entities.is_empty() {
        return Err(CurrentProjectFactsError::InvalidScope(
            "scoped compaction requires at least one source entity".to_string(),
        ));
    }

    let scoped_aliases = scoped_alias_map(source_entities, source_scope);
    let records = records
        .into_iter()
        .map(|record| canonicalize_scoped_fact(record, &scoped_aliases))
        .collect::<Vec<_>>();
    let record_entity_ids = records
        .iter()
        .map(|record| record.entity_id.clone())
        .collect::<HashSet<_>>();
    let mut allowed_entity_ids = source_entities
        .iter()
        .map(|seed| seed.entity_id.clone())
        .collect::<HashSet<_>>();
    allowed_entity_ids.extend(
        records
            .iter()
            .filter(|record| record.run_id == run_id && !record.entity_id.starts_with("society:"))
            .map(|record| record.entity_id.clone()),
    );

    loop {
        let referenced = records
            .iter()
            .filter(|record| allowed_entity_ids.contains(record.entity_id.as_str()))
            .filter_map(|record| serde_json::from_str::<FactValue>(&record.value_json).ok())
            .filter_map(|value| match value {
                FactValue::Text(value) => Some(value),
                _ => None,
            })
            .filter_map(|value| {
                let entity_id = value.trim();
                record_entity_ids
                    .contains(entity_id)
                    .then(|| entity_id.to_string())
            })
            .collect::<Vec<_>>();
        let previous_len = allowed_entity_ids.len();
        allowed_entity_ids.extend(referenced);
        if allowed_entity_ids.len() == previous_len {
            break;
        }
    }

    Ok(records
        .into_iter()
        .filter(|record| allowed_entity_ids.contains(record.entity_id.as_str()))
        .collect())
}

fn scoped_alias_map(
    source_entities: &[SourceEntitySeed],
    source_scope: SourceEntityResolutionScope,
) -> HashMap<String, String> {
    if source_scope == SourceEntityResolutionScope::Production {
        return HashMap::new();
    }
    source_entities
        .iter()
        .filter_map(|seed| {
            let alias = seed.alias_entity_id.as_ref()?;
            (alias != &seed.entity_id).then(|| (alias.clone(), seed.entity_id.clone()))
        })
        .collect()
}

fn canonicalize_scoped_fact(
    mut record: SkillFactRecord,
    scoped_aliases: &HashMap<String, String>,
) -> SkillFactRecord {
    if let Some(entity_id) = scoped_aliases.get(&record.entity_id) {
        record.entity_id.clone_from(entity_id);
    }
    record
}

fn canonicalize_scoped_annotation(
    mut annotation: SkillFactAnnotationRecord,
    scoped_aliases: &HashMap<String, String>,
) -> SkillFactAnnotationRecord {
    if let Some(entity_id) = scoped_aliases.get(&annotation.entity_id) {
        annotation.entity_id.clone_from(entity_id);
    }
    annotation
}

fn source_entity_coordinate_facts(
    source_entities: &[SourceEntitySeed],
    run_id: &MaterializationId,
    learned_at: DateTime<Utc>,
) -> Vec<SkillFactRecord> {
    source_entities
        .iter()
        .filter_map(|seed| Some((seed, seed.latitude?, seed.longitude?)))
        .flat_map(|(seed, latitude, longitude)| {
            let input_hash = format!(
                "source-entity-seed:{}:{latitude:.7}:{longitude:.7}",
                seed.entity_id
            );
            [
                source_entity_coordinate_fact(
                    seed,
                    "geo.latitude",
                    latitude,
                    run_id,
                    learned_at,
                    &input_hash,
                ),
                source_entity_coordinate_fact(
                    seed,
                    "geo.longitude",
                    longitude,
                    run_id,
                    learned_at,
                    &input_hash,
                ),
            ]
        })
        .collect()
}

fn source_entity_coordinate_fact(
    seed: &SourceEntitySeed,
    fact_key: &str,
    value: f64,
    run_id: &MaterializationId,
    learned_at: DateTime<Utc>,
    input_hash: &str,
) -> SkillFactRecord {
    SkillFactRecord {
        entity_id: seed.entity_id.clone(),
        fact_key: fact_key.to_string(),
        value_type: "numeric".to_string(),
        value_json: serde_json::to_string(&FactValue::Numeric(value))
            .expect("numeric coordinate serializes"),
        confidence: 0.95,
        source_type: "SourceEntitySeed".to_string(),
        source_url: None,
        model: None,
        skill_id: Some("source_entity_seed".to_string()),
        triggered_by: None,
        learned_at,
        run_id: run_id.to_string(),
        input_hash: input_hash.to_string(),
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

fn compact_fact_records(
    records: Vec<SkillFactRecord>,
) -> Result<Vec<SkillFactRecord>, CurrentProjectFactsError> {
    let policies = load_resolution_policies()?;
    let mut coordinate_observations =
        HashMap::<String, HashMap<CoordinateObservationKey, PartialCoordinateObservation>>::new();
    let mut by_key: BTreeMap<(String, String, String, String), SkillFactRecord> = BTreeMap::new();
    for record in records {
        if matches!(record.fact_key.as_str(), "geo.latitude" | "geo.longitude") {
            let value = match serde_json::from_str::<FactValue>(&record.value_json)? {
                FactValue::Numeric(value) => value,
                _ => continue,
            };
            let observation = coordinate_observations
                .entry(record.entity_id.clone())
                .or_default()
                .entry(CoordinateObservationKey::from_record(&record))
                .or_default();
            if record.fact_key == "geo.latitude" {
                update_coordinate_axis(&mut observation.latitude, record, value);
            } else {
                update_coordinate_axis(&mut observation.longitude, record, value);
            }
            continue;
        }
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
    let mut compacted = by_key.into_values().collect::<Vec<_>>();
    for (entity_id, observations) in coordinate_observations {
        let scope = if entity_id.starts_with("place:") {
            CoordinateEntityScope::Place
        } else {
            CoordinateEntityScope::Society
        };
        let mut complete = observations
            .iter()
            .filter_map(|(key, observation)| {
                Some((
                    key,
                    observation.latitude.as_ref()?,
                    observation.longitude.as_ref()?,
                ))
            })
            .collect::<Vec<_>>();
        complete.sort_by(|(left, _, _), (right, _, _)| {
            right
                .learned_at
                .cmp(&left.learned_at)
                .then_with(|| left.source_type.cmp(&right.source_type))
                .then_with(|| left.source_url.cmp(&right.source_url))
                .then_with(|| left.skill_id.cmp(&right.skill_id))
                .then_with(|| left.run_id.cmp(&right.run_id))
        });
        let Some(resolved) = resolve_coordinate_pair(
            scope,
            complete
                .iter()
                .map(|(key, latitude, longitude)| CoordinatePairCandidate {
                    source_type: &key.source_type,
                    latitude: latitude.value,
                    longitude: longitude.value,
                    confidence: latitude.record.confidence.min(longitude.record.confidence),
                }),
            &policies,
        ) else {
            continue;
        };
        let Some((_, latitude, longitude)) =
            complete.into_iter().find(|(key, latitude, longitude)| {
                normalize_source_type(&key.source_type)
                    == normalize_source_type(&resolved.source_type)
                    && latitude.value == resolved.latitude
                    && longitude.value == resolved.longitude
            })
        else {
            continue;
        };
        compacted.push(latitude.record.clone());
        compacted.push(longitude.record.clone());
    }
    compacted.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then(left.fact_key.cmp(&right.fact_key))
            .then(left.source_type.cmp(&right.source_type))
            .then(left.source_url.cmp(&right.source_url))
    });
    Ok(compacted)
}

pub(super) fn resolve_coordinate_fact_records(
    records: &[SkillFactRecord],
) -> Result<Vec<SkillFactRecord>, CurrentProjectFactsError> {
    compact_fact_records(
        records
            .iter()
            .filter(|record| matches!(record.fact_key.as_str(), "geo.latitude" | "geo.longitude"))
            .cloned()
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CoordinateObservationKey {
    source_type: String,
    source_url: String,
    skill_id: String,
    run_id: String,
    learned_at: chrono::DateTime<Utc>,
}

impl CoordinateObservationKey {
    fn from_record(record: &SkillFactRecord) -> Self {
        Self {
            source_type: record.source_type.clone(),
            source_url: record.source_url.clone().unwrap_or_default(),
            skill_id: record.skill_id.clone().unwrap_or_default(),
            run_id: record.run_id.clone(),
            learned_at: record.learned_at,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct PartialCoordinateObservation {
    latitude: Option<CoordinateAxis>,
    longitude: Option<CoordinateAxis>,
}

#[derive(Debug, Clone)]
struct CoordinateAxis {
    record: SkillFactRecord,
    value: f64,
}

fn update_coordinate_axis(slot: &mut Option<CoordinateAxis>, record: SkillFactRecord, value: f64) {
    if slot
        .as_ref()
        .is_none_or(|current| record.confidence > current.record.confidence)
    {
        *slot = Some(CoordinateAxis { record, value });
    }
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
    Json(serde_json::Error),
    Lake(LakeError),
    SkillFact(SkillFactMaterializeError),
    PolicyMissing(String),
    InvalidScope(String),
}

impl fmt::Display for CurrentProjectFactsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "current project facts config error: {err}"),
            Self::Json(err) => write!(f, "current project facts JSON error: {err}"),
            Self::Lake(err) => write!(f, "current project facts lake error: {err}"),
            Self::SkillFact(err) => write!(f, "current project facts parquet error: {err}"),
            Self::PolicyMissing(policy) => write!(f, "compaction policy {policy:?} is missing"),
            Self::InvalidScope(message) => {
                write!(f, "current project facts scope error: {message}")
            }
        }
    }
}

impl std::error::Error for CurrentProjectFactsError {}

impl From<dag_config::DagConfigError> for CurrentProjectFactsError {
    fn from(err: dag_config::DagConfigError) -> Self {
        Self::Config(err)
    }
}

#[cfg(test)]
mod coordinate_tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn coordinate_compaction_keeps_one_complete_observation() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap();
        let records = vec![
            coordinate("geo.latitude", 12.9, "Google", "complete", 0.8, learned_at),
            coordinate("geo.longitude", 77.6, "Google", "complete", 0.8, learned_at),
            coordinate(
                "geo.latitude",
                13.0,
                "Google",
                "incomplete",
                1.0,
                learned_at,
            ),
            coordinate("geo.latitude", 12.1, "Rera", "rera", 1.0, learned_at),
            coordinate("geo.longitude", 77.1, "Rera", "rera", 1.0, learned_at),
        ];

        let compacted = compact_fact_records(records).unwrap();

        assert_eq!(coordinate_value(&compacted, "geo.latitude"), Some(12.9));
        assert_eq!(coordinate_value(&compacted, "geo.longitude"), Some(77.6));
        assert_eq!(compacted.len(), 2);
    }

    #[test]
    fn coordinate_compaction_pairs_axes_with_fact_specific_input_hashes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap();
        let mut latitude = coordinate("geo.latitude", 12.9, "Google", "google", 0.8, learned_at);
        latitude.input_hash = "latitude-fact-hash".to_string();
        let mut longitude = coordinate("geo.longitude", 77.6, "Google", "google", 0.8, learned_at);
        longitude.input_hash = "longitude-fact-hash".to_string();

        let compacted = compact_fact_records(vec![latitude, longitude]).unwrap();

        assert_eq!(coordinate_value(&compacted, "geo.latitude"), Some(12.9));
        assert_eq!(coordinate_value(&compacted, "geo.longitude"), Some(77.6));
    }

    #[test]
    fn coordinate_compaction_prefers_fresher_equal_confidence_observation() {
        let older_at = Utc.with_ymd_and_hms(2026, 7, 30, 0, 0, 0).unwrap();
        let fresher_at = Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap();
        let records = vec![
            coordinate("geo.latitude", 12.8, "Google", "older", 0.8, older_at),
            coordinate("geo.longitude", 77.5, "Google", "older", 0.8, older_at),
            coordinate("geo.latitude", 12.9, "Google", "fresher", 0.8, fresher_at),
            coordinate("geo.longitude", 77.6, "Google", "fresher", 0.8, fresher_at),
        ];

        let compacted = compact_fact_records(records).unwrap();

        assert_eq!(coordinate_value(&compacted, "geo.latitude"), Some(12.9));
        assert_eq!(coordinate_value(&compacted, "geo.longitude"), Some(77.6));
    }

    #[test]
    fn coordinate_compaction_prefers_seed_pair_over_google_pair() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap();
        let records = vec![
            coordinate("geo.latitude", 12.9, "Google", "google", 1.0, learned_at),
            coordinate("geo.longitude", 77.6, "Google", "google", 1.0, learned_at),
            coordinate(
                "geo.latitude",
                12.8,
                "SourceEntitySeed",
                "seed",
                0.9,
                learned_at,
            ),
            coordinate(
                "geo.longitude",
                77.5,
                "SourceEntitySeed",
                "seed",
                0.9,
                learned_at,
            ),
        ];

        let compacted = compact_fact_records(records).unwrap();

        assert_eq!(coordinate_value(&compacted, "geo.latitude"), Some(12.8));
        assert_eq!(coordinate_value(&compacted, "geo.longitude"), Some(77.5));
    }

    fn coordinate(
        fact_key: &str,
        value: f64,
        source_type: &str,
        observation: &str,
        confidence: f32,
        learned_at: DateTime<Utc>,
    ) -> SkillFactRecord {
        SkillFactRecord {
            entity_id: "society:test".to_string(),
            fact_key: fact_key.to_string(),
            value_type: "numeric".to_string(),
            value_json: serde_json::to_string(&FactValue::Numeric(value)).unwrap(),
            confidence,
            source_type: source_type.to_string(),
            source_url: Some(format!("https://example.test/{observation}")),
            model: None,
            skill_id: Some("coordinate-test".to_string()),
            triggered_by: None,
            learned_at,
            run_id: observation.to_string(),
            input_hash: observation.to_string(),
        }
    }

    fn coordinate_value(records: &[SkillFactRecord], fact_key: &str) -> Option<f64> {
        records
            .iter()
            .find(|record| record.fact_key == fact_key)
            .and_then(|record| serde_json::from_str::<FactValue>(&record.value_json).ok())
            .and_then(|value| match value {
                FactValue::Numeric(value) => Some(value),
                _ => None,
            })
    }
}

impl From<LakeError> for CurrentProjectFactsError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<serde_json::Error> for CurrentProjectFactsError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
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

        let compacted = compact_fact_records(vec![older, fresher, weaker]).unwrap();

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

    #[test]
    fn scoped_compaction_keeps_only_selected_and_referenced_entities() {
        let mut selected = fact("society:selected", "linked_place", 0.9, 10);
        selected.value_json =
            serde_json::to_string(&FactValue::Text("place:selected-school".to_string())).unwrap();
        let mut current_builder = fact("builder:selected", "builder_project_count", 0.9, 10);
        current_builder.run_id = "current-run".to_string();
        let records = vec![
            selected,
            fact("place:selected-school", "place.name", 0.9, 10),
            current_builder,
            fact("builder:global", "builder_project_count", 0.9, 10),
            fact("society:other", "linked_place", 0.9, 10),
            fact("place:other-school", "place.name", 0.9, 10),
        ];
        let seeds = vec![SourceEntitySeed {
            entity_id: "society:selected".to_string(),
            alias_entity_id: None,
            name: "Selected".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: None,
            latitude: None,
            longitude: None,
        }];

        let scoped = scoped_fact_records(
            records,
            &seeds,
            SourceEntityResolutionScope::Scoped,
            "current-run",
        )
        .unwrap();
        let entity_ids = scoped
            .iter()
            .map(|fact| fact.entity_id.as_str())
            .collect::<HashSet<_>>();

        assert_eq!(
            entity_ids,
            HashSet::from([
                "society:selected",
                "place:selected-school",
                "builder:selected"
            ])
        );
    }

    #[test]
    fn scoped_compaction_canonicalizes_selected_alias_facts() {
        let records = vec![
            fact("society:selected", "google_rating", 0.8, 10),
            fact("society:selected-alias", "google_rating", 0.9, 11),
            fact("society:selected-alias", "nearby_schools", 0.9, 11),
        ];
        let seeds = vec![SourceEntitySeed {
            entity_id: "society:selected".to_string(),
            alias_entity_id: Some("society:selected-alias".to_string()),
            name: "Selected".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: None,
            latitude: None,
            longitude: None,
        }];

        let scoped = scoped_fact_records(
            records,
            &seeds,
            SourceEntityResolutionScope::Scoped,
            "current-run",
        )
        .unwrap();

        assert!(scoped
            .iter()
            .all(|fact| fact.entity_id == "society:selected"));
        assert!(scoped.iter().any(|fact| fact.fact_key == "nearby_schools"));
    }

    #[test]
    fn scoped_compaction_canonicalizes_selected_alias_annotations() {
        let aliases = scoped_alias_map(
            &[SourceEntitySeed {
                entity_id: "society:selected".to_string(),
                alias_entity_id: Some("society:selected-alias".to_string()),
                name: "Selected".to_string(),
                area: None,
                city: Some("Bengaluru".to_string()),
                project_key: None,
                latitude: None,
                longitude: None,
            }],
            SourceEntityResolutionScope::Scoped,
        );
        let annotation = SkillFactAnnotationRecord {
            entity_id: "society:selected-alias".to_string(),
            fact_key: "google_rating".to_string(),
            display_template: Some("{value}".to_string()),
            answers_preferences_json: "[]".to_string(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "{}".to_string(),
        };

        let canonical = canonicalize_scoped_annotation(annotation, &aliases);

        assert_eq!(canonical.entity_id, "society:selected");
    }

    #[test]
    fn scoped_compaction_fails_closed_without_source_entities() {
        let error = scoped_fact_records(
            vec![fact("society:other", "fact", 0.9, 10)],
            &[],
            SourceEntityResolutionScope::Scoped,
            "current-run",
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("requires at least one source entity"));
    }
}
