use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};

use super::{
    ArtifactRef, AssetDagPlan, AssetFreshness, AssetId, AssetPartition, AssetPathBuilder,
    AssetStage, CostTier, MaterializationId, MaterializationRecord, PlanDecision, PlanReason,
    RefreshCadence, TrustTier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DagRunStatus {
    Planned,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRunStepStatus {
    Planned,
    Running,
    Skipped,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRunStep {
    pub asset_id: AssetId,
    pub stage: AssetStage,
    pub dependencies: Vec<AssetId>,
    pub refresh: RefreshCadence,
    pub cost_tier: CostTier,
    pub trust_tier: TrustTier,
    pub decision: PlanDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<PlanReason>,
    pub status: AssetRunStepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_materialization_id: Option<MaterializationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialization_id: Option<MaterializationId>,
    pub parent_materializations: Vec<MaterializationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub freshness: AssetFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDagRunManifest {
    pub run_id: MaterializationId,
    pub partition: AssetPartition,
    pub status: DagRunStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub total_assets: usize,
    pub planned_count: usize,
    pub skipped_count: usize,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub steps: Vec<AssetRunStep>,
}

impl AssetDagRunManifest {
    pub fn from_plan(plan: &AssetDagPlan) -> Self {
        let steps = plan
            .entries
            .iter()
            .map(|entry| AssetRunStep {
                asset_id: entry.asset_id.clone(),
                stage: entry.stage,
                dependencies: entry.dependencies.clone(),
                refresh: entry.refresh,
                cost_tier: entry.cost_tier,
                trust_tier: entry.trust_tier,
                decision: entry.decision,
                reason: entry.reason.clone(),
                status: match entry.decision {
                    PlanDecision::Run => AssetRunStepStatus::Planned,
                    PlanDecision::Skip => AssetRunStepStatus::Skipped,
                },
                current_materialization_id: entry.current_materialization_id.clone(),
                materialization_id: None,
                parent_materializations: entry.current_parent_materializations.clone(),
                row_count: None,
                artifacts: Vec::new(),
                started_at: None,
                completed_at: None,
                duration_ms: None,
                error: None,
                freshness: entry.freshness.clone(),
            })
            .collect();

        let mut manifest = Self {
            run_id: plan.run_id.clone(),
            partition: plan.partition.clone(),
            status: DagRunStatus::Planned,
            created_at: plan.planned_at,
            completed_at: None,
            total_assets: 0,
            planned_count: 0,
            skipped_count: 0,
            succeeded_count: 0,
            failed_count: 0,
            steps,
        };
        manifest.recount();
        manifest
    }

    pub fn mark_step_running(
        &mut self,
        asset_id: &AssetId,
        started_at: DateTime<Utc>,
    ) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        step.status = AssetRunStepStatus::Running;
        step.started_at = Some(started_at);
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_succeeded(
        &mut self,
        asset_id: &AssetId,
        record: &MaterializationRecord,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Result<(), RunManifestError> {
        if &record.asset_id != asset_id {
            return Err(RunManifestError::AssetMismatch {
                expected: asset_id.clone(),
                actual: record.asset_id.clone(),
            });
        }
        if record.run_id != self.run_id {
            return Err(RunManifestError::RunIdMismatch {
                asset_id: asset_id.clone(),
                expected: self.run_id.clone(),
                actual: record.run_id.clone(),
            });
        }
        if record.partition != self.partition {
            return Err(RunManifestError::PartitionMismatch {
                asset_id: asset_id.clone(),
            });
        }

        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        step.status = AssetRunStepStatus::Succeeded;
        step.materialization_id = Some(record.materialization_id.clone());
        step.parent_materializations = record.parent_materializations.clone();
        step.row_count = Some(record.row_count);
        step.artifacts = record.artifacts.clone();
        step.started_at = Some(started_at);
        step.completed_at = Some(completed_at);
        step.duration_ms = Some(duration_ms(started_at, completed_at));
        step.error = None;
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_failed(
        &mut self,
        asset_id: &AssetId,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        step.status = AssetRunStepStatus::Failed;
        step.started_at = Some(started_at);
        step.completed_at = Some(completed_at);
        step.duration_ms = Some(duration_ms(started_at, completed_at));
        step.error = Some(error.into());
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn finish(&mut self, completed_at: DateTime<Utc>) -> Result<(), RunManifestError> {
        self.recount();
        if self.failed_count > 0 {
            self.completed_at = Some(completed_at);
            self.status = DagRunStatus::Failed;
            return Ok(());
        }
        if self.succeeded_count == self.planned_count {
            self.completed_at = Some(completed_at);
            self.status = DagRunStatus::Succeeded;
            return Ok(());
        }

        Err(RunManifestError::IncompleteRun {
            planned: self.planned_count,
            succeeded: self.succeeded_count,
            failed: self.failed_count,
        })
    }

    fn step_mut(&mut self, asset_id: &AssetId) -> Result<&mut AssetRunStep, RunManifestError> {
        self.steps
            .iter_mut()
            .find(|step| &step.asset_id == asset_id)
            .ok_or_else(|| RunManifestError::UnknownAsset(asset_id.clone()))
    }

    fn recount(&mut self) {
        self.total_assets = self.steps.len();
        self.planned_count = self
            .steps
            .iter()
            .filter(|step| step.decision == PlanDecision::Run)
            .count();
        self.skipped_count = self
            .steps
            .iter()
            .filter(|step| step.status == AssetRunStepStatus::Skipped)
            .count();
        self.succeeded_count = self
            .steps
            .iter()
            .filter(|step| step.status == AssetRunStepStatus::Succeeded)
            .count();
        self.failed_count = self
            .steps
            .iter()
            .filter(|step| step.status == AssetRunStepStatus::Failed)
            .count();
    }
}

fn validate_runnable_step(step: &AssetRunStep) -> Result<(), RunManifestError> {
    if step.decision != PlanDecision::Run {
        return Err(RunManifestError::InvalidStepTransition {
            asset_id: step.asset_id.clone(),
            status: step.status,
            decision: step.decision,
        });
    }

    match step.status {
        AssetRunStepStatus::Planned | AssetRunStepStatus::Running => Ok(()),
        status => Err(RunManifestError::InvalidStepTransition {
            asset_id: step.asset_id.clone(),
            status,
            decision: step.decision,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunManifestError {
    UnknownAsset(AssetId),
    AssetMismatch {
        expected: AssetId,
        actual: AssetId,
    },
    RunIdMismatch {
        asset_id: AssetId,
        expected: MaterializationId,
        actual: MaterializationId,
    },
    PartitionMismatch {
        asset_id: AssetId,
    },
    InvalidStepTransition {
        asset_id: AssetId,
        status: AssetRunStepStatus,
        decision: PlanDecision,
    },
    IncompleteRun {
        planned: usize,
        succeeded: usize,
        failed: usize,
    },
}

impl std::fmt::Display for RunManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAsset(asset_id) => write!(f, "asset {asset_id} is not in DAG run"),
            Self::AssetMismatch { expected, actual } => {
                write!(f, "record for asset {actual} cannot complete DAG step {expected}")
            }
            Self::RunIdMismatch {
                asset_id,
                expected,
                actual,
            } => write!(
                f,
                "asset {asset_id} record belongs to run {actual}, expected {expected}"
            ),
            Self::PartitionMismatch { asset_id } => {
                write!(f, "asset {asset_id} record partition does not match DAG run")
            }
            Self::InvalidStepTransition {
                asset_id,
                status,
                decision,
            } => write!(
                f,
                "asset {asset_id} cannot transition from status {status:?} with decision {decision:?}"
            ),
            Self::IncompleteRun {
                planned,
                succeeded,
                failed,
            } => write!(
                f,
                "DAG run is incomplete: planned={planned}, succeeded={succeeded}, failed={failed}"
            ),
        }
    }
}

impl std::error::Error for RunManifestError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentDagRunPointer {
    pub run_id: MaterializationId,
    pub run_manifest_key: String,
    pub status: DagRunStatus,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct AssetRunManifestStore {
    lake: LakeStore,
}

impl AssetRunManifestStore {
    pub fn new(lake: LakeStore) -> Self {
        Self { lake }
    }

    pub async fn write_manifest(
        &self,
        manifest: &AssetDagRunManifest,
    ) -> Result<ArtifactMetadata, LakeError> {
        let key = AssetPathBuilder::dag_run_manifest_key(&manifest.partition, &manifest.run_id);
        self.lake.put_json(&key, manifest).await
    }

    pub async fn promote_current(
        &self,
        manifest: &AssetDagRunManifest,
    ) -> Result<ArtifactMetadata, LakeError> {
        let pointer = CurrentDagRunPointer {
            run_id: manifest.run_id.clone(),
            run_manifest_key: AssetPathBuilder::dag_run_manifest_key(
                &manifest.partition,
                &manifest.run_id,
            )
            .to_string(),
            status: manifest.status,
            updated_at: Utc::now(),
        };
        let key = AssetPathBuilder::current_dag_run_pointer_key(&manifest.partition);
        self.lake.put_json(&key, &pointer).await
    }

    pub async fn current_pointer(
        &self,
        partition: &AssetPartition,
    ) -> Result<CurrentDagRunPointer, LakeError> {
        let key = AssetPathBuilder::current_dag_run_pointer_key(partition);
        self.lake.get_json(&key).await
    }

    pub async fn current_manifest(
        &self,
        partition: &AssetPartition,
    ) -> Result<AssetDagRunManifest, LakeError> {
        let pointer = self.current_pointer(partition).await?;
        let key = LakeKey::new(pointer.run_manifest_key).expect("stored DAG run key");
        self.lake.get_json(&key).await
    }
}

fn duration_ms(started_at: DateTime<Utc>, completed_at: DateTime<Utc>) -> u64 {
    completed_at
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0) as u64
}
