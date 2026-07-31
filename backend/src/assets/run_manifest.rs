use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};

use super::{
    ArtifactRef, AssetDagPlan, AssetFreshness, AssetId, AssetPartition, AssetPathBuilder,
    AssetStage, CostTier, MaterializationId, MaterializationRecord, PlanDecision, PlanReason,
    RefreshCadence, SourceEntityResolutionScope, TrustTier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DagRunStatus {
    Planned,
    Running,
    Succeeded,
    SucceededWithWarnings,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRunStepStatus {
    Planned,
    Running,
    Materialized,
    Skipped,
    Succeeded,
    Failed,
    Blocked,
}

pub const DEFAULT_RESUME_LEASE_SECONDS: i64 = 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDagResumeLease {
    pub owner_id: MaterializationId,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRunAttempt {
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRunStep {
    pub asset_id: AssetId,
    pub partition: AssetPartition,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_snapshot: Vec<MaterializationId>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<AssetRunAttempt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<AssetId>,
    pub freshness: AssetFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDagRunManifest {
    #[serde(default)]
    pub format_version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_lease: Option<AssetDagResumeLease>,
    pub run_id: MaterializationId,
    pub partition: AssetPartition,
    #[serde(default)]
    pub execution_version: String,
    #[serde(default = "default_promote_current")]
    pub promote_current: bool,
    #[serde(default)]
    pub source_scope: SourceEntityResolutionScope,
    pub status: DagRunStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub total_assets: usize,
    pub planned_count: usize,
    pub skipped_count: usize,
    pub succeeded_count: usize,
    pub failed_count: usize,
    #[serde(default)]
    pub blocked_count: usize,
    pub steps: Vec<AssetRunStep>,
}

impl AssetDagRunManifest {
    pub fn from_plan(plan: &AssetDagPlan) -> Self {
        Self::from_plan_with_version(plan, "")
    }

    pub fn from_plan_with_version(
        plan: &AssetDagPlan,
        execution_version: impl Into<String>,
    ) -> Self {
        let steps = plan
            .entries
            .iter()
            .map(|entry| AssetRunStep {
                asset_id: entry.asset_id.clone(),
                partition: entry.partition.clone(),
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
                dependency_snapshot: entry.dependency_snapshot.clone(),
                row_count: None,
                artifacts: Vec::new(),
                started_at: None,
                completed_at: None,
                duration_ms: None,
                error: None,
                attempts: Vec::new(),
                blocked_by: Vec::new(),
                freshness: entry.freshness.clone(),
            })
            .collect();

        let mut manifest = Self {
            format_version: 1,
            revision: 0,
            resume_lease: None,
            run_id: plan.run_id.clone(),
            partition: plan.partition.clone(),
            execution_version: execution_version.into(),
            promote_current: true,
            source_scope: SourceEntityResolutionScope::Production,
            status: DagRunStatus::Planned,
            created_at: plan.planned_at,
            completed_at: None,
            total_assets: 0,
            planned_count: 0,
            skipped_count: 0,
            succeeded_count: 0,
            failed_count: 0,
            blocked_count: 0,
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
        step.started_at.get_or_insert(started_at);
        step.completed_at = None;
        step.duration_ms = None;
        step.error = None;
        step.blocked_by.clear();
        step.attempts.push(AssetRunAttempt {
            attempt: step.attempts.len() as u32 + 1,
            started_at,
            completed_at: None,
            error: None,
        });
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
        self.mark_step_materialized(asset_id, record, started_at, completed_at)?;
        self.mark_step_promoted(asset_id, completed_at)
    }

    pub fn mark_step_materialized(
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
        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        if record.partition != step.partition {
            return Err(RunManifestError::PartitionMismatch {
                asset_id: asset_id.clone(),
            });
        }
        step.status = AssetRunStepStatus::Materialized;
        step.materialization_id = Some(record.materialization_id.clone());
        step.parent_materializations = record.parent_materializations.clone();
        step.row_count = Some(record.row_count);
        step.artifacts = record.artifacts.clone();
        step.started_at.get_or_insert(started_at);
        step.completed_at = Some(completed_at);
        step.duration_ms = step
            .started_at
            .map(|first_started| duration_ms(first_started, completed_at));
        step.error = None;
        step.blocked_by.clear();
        close_or_add_successful_attempt(step, started_at, completed_at)?;
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_promoted(
        &mut self,
        asset_id: &AssetId,
        completed_at: DateTime<Utc>,
    ) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        if step.decision != PlanDecision::Run
            || step.status != AssetRunStepStatus::Materialized
            || step.materialization_id.is_none()
        {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        step.status = AssetRunStepStatus::Succeeded;
        step.completed_at = Some(completed_at);
        step.error = None;
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_materialized_step_failed(
        &mut self,
        asset_id: &AssetId,
        completed_at: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        if step.status != AssetRunStepStatus::Materialized {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        step.status = AssetRunStepStatus::Failed;
        step.completed_at = Some(completed_at);
        step.error = Some(error.into());
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
        if self
            .steps
            .iter()
            .find(|step| &step.asset_id == asset_id)
            .is_some_and(|step| step.status == AssetRunStepStatus::Planned)
        {
            self.mark_step_running(asset_id, started_at)?;
        }
        let error = error.into();
        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        let attempt = step
            .attempts
            .last_mut()
            .filter(|attempt| attempt.completed_at.is_none() && attempt.started_at == started_at)
            .ok_or_else(|| RunManifestError::NoRunningAttempt(asset_id.clone()))?;
        attempt.completed_at = Some(completed_at);
        attempt.error = Some(error.clone());
        step.status = AssetRunStepStatus::Failed;
        step.completed_at = Some(completed_at);
        step.duration_ms = step
            .started_at
            .map(|first_started| duration_ms(first_started, completed_at));
        step.error = Some(error);
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_attempt_failed(
        &mut self,
        asset_id: &AssetId,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Result<(), RunManifestError> {
        let error = error.into();
        let step = self.step_mut(asset_id)?;
        validate_runnable_step(step)?;
        let attempt = step
            .attempts
            .last_mut()
            .filter(|attempt| attempt.completed_at.is_none() && attempt.started_at == started_at)
            .ok_or_else(|| RunManifestError::NoRunningAttempt(asset_id.clone()))?;
        attempt.completed_at = Some(completed_at);
        attempt.error = Some(error.clone());
        step.error = Some(error);
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_blocked(
        &mut self,
        asset_id: &AssetId,
        completed_at: DateTime<Utc>,
        blocked_by: Vec<AssetId>,
    ) -> Result<(), RunManifestError> {
        if blocked_by.is_empty() {
            return Err(RunManifestError::MissingBlockedDependencies(
                asset_id.clone(),
            ));
        }
        let step = self.step_mut(asset_id)?;
        if step.decision != PlanDecision::Run || step.status != AssetRunStepStatus::Planned {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        step.status = AssetRunStepStatus::Blocked;
        step.blocked_by = blocked_by;
        step.completed_at = Some(completed_at);
        step.error = Some(format!(
            "blocked by failed dependencies: {}",
            step.blocked_by
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn mark_step_skipped(
        &mut self,
        asset_id: &AssetId,
        completed_at: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        if step.decision != PlanDecision::Run || step.status != AssetRunStepStatus::Planned {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        step.status = AssetRunStepStatus::Skipped;
        step.completed_at = Some(completed_at);
        step.error = Some(reason.into());
        self.status = DagRunStatus::Running;
        self.recount();
        Ok(())
    }

    pub fn prepare_resume(mut self, resumed_at: DateTime<Utc>) -> Result<Self, RunManifestError> {
        self.ensure_resumable()?;

        for step in &mut self.steps {
            match step.status {
                AssetRunStepStatus::Succeeded
                | AssetRunStepStatus::Skipped
                | AssetRunStepStatus::Materialized => continue,
                AssetRunStepStatus::Running => {
                    if let Some(attempt) = step
                        .attempts
                        .last_mut()
                        .filter(|attempt| attempt.completed_at.is_none())
                    {
                        attempt.completed_at = Some(resumed_at);
                        attempt.error = Some("run interrupted before completion".to_string());
                    }
                }
                AssetRunStepStatus::Failed if step.materialization_id.is_some() => {
                    step.status = AssetRunStepStatus::Materialized;
                    step.error = None;
                    step.blocked_by.clear();
                    continue;
                }
                AssetRunStepStatus::Planned
                | AssetRunStepStatus::Failed
                | AssetRunStepStatus::Blocked => {}
            }
            step.status = AssetRunStepStatus::Planned;
            step.materialization_id = None;
            step.parent_materializations.clear();
            step.row_count = None;
            step.artifacts.clear();
            step.started_at = None;
            step.completed_at = None;
            step.duration_ms = None;
            step.error = None;
            step.blocked_by.clear();
        }
        self.status = DagRunStatus::Running;
        self.completed_at = None;
        self.recount();
        Ok(self)
    }

    pub fn replay_step(&mut self, asset_id: &AssetId) -> Result<(), RunManifestError> {
        let step = self.step_mut(asset_id)?;
        if step.decision != PlanDecision::Run {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        if step.status == AssetRunStepStatus::Planned {
            return Ok(());
        }
        if step.status != AssetRunStepStatus::Succeeded
            && step.status != AssetRunStepStatus::Materialized
            && !(step.status == AssetRunStepStatus::Failed && step.materialization_id.is_some())
        {
            return Err(RunManifestError::InvalidStepTransition {
                asset_id: step.asset_id.clone(),
                status: step.status,
                decision: step.decision,
            });
        }
        reset_step_for_resume(step);
        self.recount();
        Ok(())
    }

    pub fn ensure_resumable(&self) -> Result<(), RunManifestError> {
        match self.status {
            DagRunStatus::Failed | DagRunStatus::Running | DagRunStatus::SucceededWithWarnings => {
                Ok(())
            }
            status => Err(RunManifestError::RunNotResumable(status)),
        }
    }

    pub fn ensure_exact_resume(&self) -> Result<(), RunManifestError> {
        self.ensure_resumable()?;
        if self.format_version != 1 || self.execution_version.is_empty() {
            return Err(RunManifestError::UnsupportedResumeManifest {
                format_version: self.format_version,
            });
        }
        Ok(())
    }

    fn acquire_resume_lease(
        &mut self,
        owner_id: MaterializationId,
        acquired_at: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<(), LakeError> {
        if let Some(lease) = &self.resume_lease {
            if lease.owner_id != owner_id && lease.expires_at > acquired_at {
                return Err(LakeError::ConcurrentModification(format!(
                    "DAG run {} is leased by {} until {}",
                    self.run_id, lease.owner_id, lease.expires_at
                )));
            }
        }
        self.resume_lease = Some(AssetDagResumeLease {
            owner_id,
            acquired_at,
            expires_at: acquired_at + lease_duration,
        });
        Ok(())
    }

    pub fn renew_resume_lease(&mut self, now: DateTime<Utc>) {
        if let Some(lease) = &mut self.resume_lease {
            lease.expires_at = now + Duration::seconds(DEFAULT_RESUME_LEASE_SECONDS);
        }
    }

    pub fn finish(&mut self, completed_at: DateTime<Utc>) -> Result<(), RunManifestError> {
        self.recount();
        if self.failed_count > 0 || self.blocked_count > 0 {
            self.completed_at = Some(completed_at);
            self.status = if self.terminal_steps_succeeded() {
                DagRunStatus::SucceededWithWarnings
            } else {
                DagRunStatus::Failed
            };
            return Ok(());
        }
        if self.completed_run_step_count() == self.planned_count {
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

    fn terminal_steps_succeeded(&self) -> bool {
        let terminal_steps = self.steps.iter().filter(|candidate| {
            !self.steps.iter().any(|step| {
                step.dependencies
                    .iter()
                    .any(|dependency| dependency == &candidate.asset_id)
            })
        });
        let mut terminal_count = 0;
        for step in terminal_steps {
            terminal_count += 1;
            if !matches!(
                step.status,
                AssetRunStepStatus::Succeeded | AssetRunStepStatus::Skipped
            ) {
                return false;
            }
        }
        terminal_count > 0
    }

    fn completed_run_step_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.decision == PlanDecision::Run)
            .filter(|step| {
                matches!(
                    step.status,
                    AssetRunStepStatus::Succeeded | AssetRunStepStatus::Skipped
                )
            })
            .count()
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
        self.blocked_count = self
            .steps
            .iter()
            .filter(|step| step.status == AssetRunStepStatus::Blocked)
            .count();
    }
}

fn default_promote_current() -> bool {
    true
}

fn reset_step_for_resume(step: &mut AssetRunStep) {
    step.status = AssetRunStepStatus::Planned;
    step.materialization_id = None;
    step.parent_materializations.clear();
    step.row_count = None;
    step.artifacts.clear();
    step.started_at = None;
    step.completed_at = None;
    step.duration_ms = None;
    step.error = None;
    step.blocked_by.clear();
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
    NoRunningAttempt(AssetId),
    MissingBlockedDependencies(AssetId),
    RunNotResumable(DagRunStatus),
    UnsupportedResumeManifest {
        format_version: u32,
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
            Self::NoRunningAttempt(asset_id) => {
                write!(f, "asset {asset_id} has no running attempt to complete")
            }
            Self::MissingBlockedDependencies(asset_id) => {
                write!(f, "asset {asset_id} cannot be blocked without failed dependencies")
            }
            Self::RunNotResumable(status) => {
                write!(f, "DAG run with status {status:?} cannot be resumed")
            }
            Self::UnsupportedResumeManifest { format_version } => write!(
                f,
                "DAG run manifest format {format_version} does not contain the snapshot required for exact resume"
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_created_at: Option<DateTime<Utc>>,
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

    pub async fn write_manifest_cas(
        &self,
        manifest: &mut AssetDagRunManifest,
    ) -> Result<ArtifactMetadata, LakeError> {
        let expected_revision = manifest.revision;
        let mut next = manifest.clone();
        next.revision = expected_revision + 1;
        let key = AssetPathBuilder::dag_run_manifest_key(&manifest.partition, &manifest.run_id);
        let updated = self
            .lake
            .put_json_if(&key, &next, |current: Option<&AssetDagRunManifest>| {
                current.map_or(expected_revision == 0, |current| {
                    current.run_id == manifest.run_id && current.revision == expected_revision
                })
            })
            .await?;
        if !updated {
            return Err(LakeError::ConcurrentModification(format!(
                "DAG run {} changed after revision {expected_revision}",
                manifest.run_id
            )));
        }
        *manifest = next;
        self.lake.artifact_metadata(&key).await
    }

    pub async fn acquire_resume_lease(
        &self,
        manifest: &mut AssetDagRunManifest,
        owner_id: MaterializationId,
        acquired_at: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<ArtifactMetadata, LakeError> {
        manifest.acquire_resume_lease(owner_id, acquired_at, lease_duration)?;
        self.write_manifest_cas(manifest).await
    }

    pub async fn release_resume_lease(
        &self,
        partition: &AssetPartition,
        run_id: &MaterializationId,
        owner_id: &MaterializationId,
    ) -> Result<bool, LakeError> {
        let mut manifest = self.manifest(partition, run_id).await?;
        if manifest
            .resume_lease
            .as_ref()
            .is_none_or(|lease| &lease.owner_id != owner_id)
        {
            return Ok(false);
        }
        manifest.resume_lease = None;
        self.write_manifest_cas(&mut manifest).await?;
        Ok(true)
    }

    pub async fn promote_current(&self, manifest: &AssetDagRunManifest) -> Result<bool, LakeError> {
        let pointer = CurrentDagRunPointer {
            run_id: manifest.run_id.clone(),
            run_manifest_key: AssetPathBuilder::dag_run_manifest_key(
                &manifest.partition,
                &manifest.run_id,
            )
            .to_string(),
            status: manifest.status,
            run_created_at: Some(manifest.created_at),
            updated_at: Utc::now(),
        };
        let key = AssetPathBuilder::current_dag_run_pointer_key(&manifest.partition);
        self.lake
            .put_json_if(&key, &pointer, |current: Option<&CurrentDagRunPointer>| {
                let Some(current) = current else {
                    return true;
                };
                let current_time = current.run_created_at.unwrap_or(current.updated_at);
                current.run_id == manifest.run_id
                    || manifest.created_at > current_time
                    || (manifest.created_at == current_time
                        && manifest.run_id.to_string() > current.run_id.to_string())
            })
            .await
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

    pub async fn manifest(
        &self,
        partition: &AssetPartition,
        run_id: &MaterializationId,
    ) -> Result<AssetDagRunManifest, LakeError> {
        let key = AssetPathBuilder::dag_run_manifest_key(partition, run_id);
        self.lake.get_json(&key).await
    }
}

fn close_or_add_successful_attempt(
    step: &mut AssetRunStep,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
) -> Result<(), RunManifestError> {
    if let Some(attempt) = step
        .attempts
        .last_mut()
        .filter(|attempt| attempt.completed_at.is_none())
    {
        attempt.completed_at = Some(completed_at);
        attempt.error = None;
        return Ok(());
    }
    if step.attempts.is_empty() {
        step.attempts.push(AssetRunAttempt {
            attempt: 1,
            started_at,
            completed_at: Some(completed_at),
            error: None,
        });
        return Ok(());
    }
    Err(RunManifestError::NoRunningAttempt(step.asset_id.clone()))
}

fn duration_ms(started_at: DateTime<Utc>, completed_at: DateTime<Utc>) -> u64 {
    completed_at
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0) as u64
}
