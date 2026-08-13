use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::knowledge::KnowledgeGraph;
use crate::lake::{LakeError, LakeStore};
use crate::serving::{
    project_rera_evidence, validate_search_serving_candidate, write_frontend_media_manifest,
    ReraServingProjectionError, SearchServingBundleMaterialization,
    SearchServingBundleMaterializeError, SearchServingBundleMaterializer,
    SEARCH_SERVING_BUNDLE_ASSET_ID,
};

use super::{
    ingest_local_media_assets, read_rera_claims, read_rera_receipt_records,
    read_rera_source_records, read_skill_fact_artifact_rows, sort_materialization_records,
    ApproachRoadGraphError, ApproachRoadGraphMaterializer, AssetDagPlan, AssetDagRunManifest,
    AssetDefinition, AssetFanInError, AssetId, AssetMaterializationStore, AssetPartition,
    AssetPlanner, AssetRunManifestStore, AssetSourceInputs, AssetStage, CurrentProjectFactsError,
    CurrentProjectFactsMaterializer, DependencyFanInPolicy, EnvironmentalAssetError,
    GooglePlaceAssetError, GooglePlaceSnapshotMaterializer, KgSocietyViewMaterialization,
    KgSocietyViewMaterializeError, KgSocietyViewMaterializer, KgViewManifest, MaterializationId,
    MaterializationRecord, MediaAssetError, MediaAssetMaterializer, OsmPowerAssetError,
    PartitionResolutionError, PlannerError, ProjectEnrichmentAssetError,
    ProjectEnrichmentMaterializer, ReraAssetError, ReraClaimMaterializeError,
    ReraClaimsMaterializer, ReraEvidenceError, ReraPlanFramesAssetError, ReraReceiptsMaterializer,
    ReraRegistryMaterializer, ReraSourceRecordsError, ReraSourceRecordsMaterializer,
    RunManifestError, SkillFactMaterializeError, SkillFactMaterializer, SkillFactsInput,
    SourceEntityResolutionScope, SourceWatermark, StormwaterAssetError, TransitAssetError,
    APPROACH_ROAD_GRAPH_FACTS_ASSET_ID, BENGALURU_METRO_STATION_FACTS_ASSET_ID,
    BUILDER_RERA_AGGREGATES_ASSET_ID, CANONICAL_SOCIETY_NODES_ASSET_ID,
    CURRENT_PROJECT_FACTS_ASSET_ID, EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
    EXTERNAL_LISTINGS_WEEKLY_ASSET_ID, EXTERNAL_LISTING_FACTS_ASSET_ID,
    GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID, GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID,
    GOOGLE_PLACES_WEEKLY_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID, HOME_STATE_SIGNALS_ASSET_ID,
    IMAGE_MEDIA_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID, OSM_POWER_LINE_FACTS_ASSET_ID,
    RERA_CLAIMS_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID, RERA_PROJECT_PLAN_FRAMES_ASSET_ID,
    RERA_RECEIPTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID, RERA_SOURCE_RECORDS_ASSET_ID,
    SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID, STORMWATER_DRAIN_FACTS_ASSET_ID,
};

const DEFAULT_ASSET_EXECUTION_TIMEOUT_MS: u64 = 45 * 60 * 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDagExecutionOptions {
    pub partition: AssetPartition,
    pub planned_at: DateTime<Utc>,
    pub version: String,
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "is_default_source_inputs")]
    pub source_inputs: AssetSourceInputs,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub force_assets: Vec<AssetId>,
    #[serde(default = "default_promote_current")]
    pub promote_current: bool,
    #[serde(default)]
    pub source_scope: SourceEntityResolutionScope,
    #[serde(default)]
    pub skip_missing_source_inputs: bool,
    #[serde(default)]
    pub only_forced_assets: bool,
    #[serde(default)]
    pub retry_policy: AssetRetryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_lease_id: Option<MaterializationId>,
    #[serde(default = "default_asset_execution_timeout_ms")]
    asset_execution_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRetryPolicy {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for AssetRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 100,
            max_delay_ms: 2_000,
        }
    }
}

impl AssetDagExecutionOptions {
    pub fn new(partition: AssetPartition, planned_at: DateTime<Utc>) -> Self {
        Self {
            partition,
            planned_at,
            version: default_asset_version(planned_at),
            dry_run: false,
            source_inputs: AssetSourceInputs::default(),
            force_assets: Vec::new(),
            promote_current: true,
            source_scope: SourceEntityResolutionScope::Production,
            skip_missing_source_inputs: false,
            only_forced_assets: false,
            retry_policy: AssetRetryPolicy::default(),
            resume_lease_id: None,
            asset_execution_timeout_ms: DEFAULT_ASSET_EXECUTION_TIMEOUT_MS,
        }
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_source_inputs(mut self, source_inputs: AssetSourceInputs) -> Self {
        self.source_inputs = source_inputs;
        self
    }

    pub fn with_forced_assets(mut self, force_assets: Vec<AssetId>) -> Self {
        self.force_assets = force_assets;
        self
    }

    pub fn with_promote_current(mut self, promote_current: bool) -> Self {
        self.promote_current = promote_current;
        self
    }

    pub fn with_source_scope(mut self, source_scope: SourceEntityResolutionScope) -> Self {
        self.source_scope = source_scope;
        if source_scope == SourceEntityResolutionScope::Scoped {
            self.promote_current = false;
        }
        self
    }

    pub fn with_skip_missing_source_inputs(mut self, skip_missing_source_inputs: bool) -> Self {
        self.skip_missing_source_inputs = skip_missing_source_inputs;
        self
    }

    pub fn with_only_forced_assets(mut self, only_forced_assets: bool) -> Self {
        self.only_forced_assets = only_forced_assets;
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: AssetRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn with_resume_lease(mut self, lease_id: MaterializationId) -> Self {
        self.resume_lease_id = Some(lease_id);
        self
    }

    pub fn with_asset_execution_timeout(mut self, timeout: Duration) -> Self {
        self.asset_execution_timeout_ms = (timeout.as_millis().min(u64::MAX as u128) as u64)
            .min(DEFAULT_ASSET_EXECUTION_TIMEOUT_MS);
        self
    }

    fn asset_execution_timeout_ms(&self) -> u64 {
        self.asset_execution_timeout_ms
            .min(DEFAULT_ASSET_EXECUTION_TIMEOUT_MS)
    }
}

fn default_promote_current() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDagExecutionReport {
    pub dry_run: bool,
    pub manifest: AssetDagRunManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_manifest_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_pointer_key: Option<String>,
    pub executed_assets: Vec<AssetId>,
}

#[derive(Clone)]
pub struct AssetDagExecutor {
    lake: LakeStore,
    registry: super::AssetRegistry,
    executors: BuiltInAssetExecutorRegistry,
    materializations: AssetMaterializationStore,
    run_manifests: AssetRunManifestStore,
    project_root: PathBuf,
    sync_frontend_manifest: bool,
}

impl AssetDagExecutor {
    pub fn new(registry: super::AssetRegistry, lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        let run_manifests = AssetRunManifestStore::new(lake.clone());
        Self {
            lake,
            registry,
            executors: BuiltInAssetExecutorRegistry::default_openestates(),
            materializations,
            run_manifests,
            project_root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("backend crate should live under project root")
                .to_path_buf(),
            sync_frontend_manifest: false,
        }
    }

    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = project_root.into();
        self.sync_frontend_manifest = true;
        self
    }

    pub async fn plan(
        &self,
        partition: &AssetPartition,
        planned_at: DateTime<Utc>,
    ) -> Result<AssetDagPlan, AssetDagExecutorError> {
        let planner = AssetPlanner::new(self.registry.clone(), self.materializations.clone());
        Ok(planner
            .plan_partition_details(partition, planned_at)
            .await?)
    }

    pub async fn plan_with_forced_assets(
        &self,
        partition: &AssetPartition,
        planned_at: DateTime<Utc>,
        force_assets: &[AssetId],
    ) -> Result<AssetDagPlan, AssetDagExecutorError> {
        let planner = AssetPlanner::new(self.registry.clone(), self.materializations.clone());
        let forced_assets = force_assets.iter().cloned().collect();
        Ok(planner
            .plan_partition_details_with_forced(partition, planned_at, &forced_assets)
            .await?)
    }

    pub async fn execute(
        &self,
        graph: &KnowledgeGraph,
        options: AssetDagExecutionOptions,
    ) -> Result<AssetDagExecutionReport, AssetDagExecutorError> {
        let plan = self
            .plan_with_forced_assets(
                &options.partition,
                options.planned_at,
                &options.force_assets,
            )
            .await?;
        let mut manifest =
            AssetDagRunManifest::from_plan_with_version(&plan, options.version.clone());
        manifest.promote_current = options.promote_current;
        manifest.source_scope = options.source_scope;
        let mut options = options;
        if !options.promote_current {
            options.version = format!("{}-run-{}", options.version, manifest.run_id);
            manifest.execution_version.clone_from(&options.version);
        }

        if options.dry_run {
            return Ok(AssetDagExecutionReport {
                dry_run: true,
                manifest,
                run_manifest_key: None,
                current_pointer_key: None,
                executed_assets: Vec::new(),
            });
        }

        let dependency_snapshot = self.load_dependency_snapshot(&manifest).await?;
        self.execute_manifest(
            graph,
            options,
            manifest,
            HashMap::new(),
            dependency_snapshot,
        )
        .await
    }

    pub async fn resume(
        &self,
        graph: &KnowledgeGraph,
        options: AssetDagExecutionOptions,
        run_id: MaterializationId,
    ) -> Result<AssetDagExecutionReport, AssetDagExecutorError> {
        if options.dry_run {
            return Err(AssetDagExecutorError::ResumeDryRunUnsupported);
        }
        let mut manifest = self
            .run_manifests
            .manifest(&options.partition, &run_id)
            .await?;
        if manifest.partition != options.partition {
            return Err(AssetDagExecutorError::ResumePartitionMismatch {
                run_id,
                expected: manifest.partition,
                actual: options.partition,
            });
        }
        manifest.ensure_exact_resume()?;
        let mut options = options;
        options.promote_current = manifest.promote_current;
        options.source_scope = manifest.source_scope;
        let lease_id = options.resume_lease_id.clone().unwrap_or_default();
        self.run_manifests
            .acquire_resume_lease(
                &mut manifest,
                lease_id.clone(),
                Utc::now(),
                chrono::Duration::seconds(super::DEFAULT_RESUME_LEASE_SECONDS),
            )
            .await?;
        options.resume_lease_id = Some(lease_id.clone());
        options.planned_at = manifest.created_at;
        if !manifest.execution_version.is_empty() {
            options.version.clone_from(&manifest.execution_version);
        }
        let lease_partition = manifest.partition.clone();
        let lease_run_id = manifest.run_id.clone();
        let result = async {
            let dependency_snapshot = self.load_dependency_snapshot(&manifest).await?;
            let records_by_asset = self.restore_succeeded_records(&manifest).await?;
            if self
                .recover_interrupted_materialization(
                    &mut manifest,
                    &records_by_asset,
                    &dependency_snapshot,
                    &options.partition,
                    &options.retry_policy,
                )
                .await?
            {
                self.persist_manifest(&mut manifest, false).await?;
            }
            let mut manifest = manifest.prepare_resume(Utc::now())?;
            for asset_id in &options.force_assets {
                manifest.replay_step(asset_id)?;
            }
            let records_by_asset = self.restore_succeeded_records(&manifest).await?;
            self.execute_manifest(
                graph,
                options,
                manifest,
                records_by_asset,
                dependency_snapshot,
            )
            .await
        }
        .await;
        if result.is_err() {
            let _ = self
                .run_manifests
                .release_resume_lease(&lease_partition, &lease_run_id, &lease_id)
                .await;
        }
        result
    }

    async fn recover_interrupted_materialization(
        &self,
        manifest: &mut AssetDagRunManifest,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
        run_partition: &AssetPartition,
        retry_policy: &AssetRetryPolicy,
    ) -> Result<bool, AssetDagExecutorError> {
        let Some(step) = manifest
            .steps
            .iter()
            .find(|step| step.status == super::AssetRunStepStatus::Running)
            .cloned()
        else {
            return Ok(false);
        };
        let started_at = step
            .attempts
            .last()
            .map_or(manifest.created_at, |attempt| attempt.started_at);
        let Some(record) = self
            .materializations
            .record_for_run_attempt(
                &step.asset_id,
                &step.partition,
                &manifest.run_id,
                started_at,
            )
            .await?
        else {
            return Ok(false);
        };
        self.validate_record_with_retry(
            step.asset_id.clone(),
            &record,
            records_by_asset,
            dependency_snapshot,
            run_partition,
            retry_policy,
        )
        .await?;
        manifest.mark_step_materialized(&step.asset_id, &record, started_at, Utc::now())?;
        Ok(true)
    }

    async fn execute_manifest(
        &self,
        graph: &KnowledgeGraph,
        options: AssetDagExecutionOptions,
        mut manifest: AssetDagRunManifest,
        mut records_by_asset: HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<AssetDagExecutionReport, AssetDagExecutorError> {
        self.persist_manifest(&mut manifest, false).await?;
        let mut executed_assets = Vec::new();
        let mut kg_view = if manifest.steps.iter().any(|step| {
            step.status == super::AssetRunStepStatus::Planned
                && step
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.as_str() == KG_SOCIETY_VIEW_ASSET_ID)
        }) {
            self.restore_kg_view_runtime(
                graph,
                &records_by_asset,
                &dependency_snapshot,
                &options.partition,
            )
            .await?
        } else {
            None
        };
        let mut first_error = None;

        for step in manifest.steps.clone() {
            if step.status == super::AssetRunStepStatus::Materialized {
                let record = self.record_for_manifest_step(&step).await?;
                if !manifest.promote_current {
                    manifest.mark_step_promoted(&step.asset_id, Utc::now())?;
                    self.persist_manifest(&mut manifest, false).await?;
                    records_by_asset.insert(step.asset_id.clone(), record);
                    if step.asset_id.as_str() == KG_SOCIETY_VIEW_ASSET_ID {
                        kg_view = self
                            .restore_kg_view_runtime(
                                graph,
                                &records_by_asset,
                                &dependency_snapshot,
                                &options.partition,
                            )
                            .await?;
                    }
                    continue;
                }
                match self
                    .promote_materialization_with_retry(
                        &record,
                        manifest.created_at,
                        step.current_materialization_id.as_ref(),
                        &options.retry_policy,
                    )
                    .await
                {
                    Ok(()) => {
                        manifest.mark_step_promoted(&step.asset_id, Utc::now())?;
                        self.persist_manifest(&mut manifest, false).await?;
                        records_by_asset.insert(step.asset_id.clone(), record);
                        if step.asset_id.as_str() == KG_SOCIETY_VIEW_ASSET_ID {
                            kg_view = self
                                .restore_kg_view_runtime(
                                    graph,
                                    &records_by_asset,
                                    &dependency_snapshot,
                                    &options.partition,
                                )
                                .await?;
                        }
                    }
                    Err(err) => {
                        manifest.mark_materialized_step_failed(
                            &step.asset_id,
                            Utc::now(),
                            err.to_string(),
                        )?;
                        self.persist_manifest(&mut manifest, false).await?;
                        first_error.get_or_insert(err);
                    }
                }
                continue;
            }
            if step.status != super::AssetRunStepStatus::Planned {
                continue;
            }

            let definition = self.registry.get(&step.asset_id).ok_or_else(|| {
                AssetDagExecutorError::UnknownAsset {
                    asset_id: step.asset_id.clone(),
                }
            })?;
            let blocked_by = blocked_dependencies(&manifest, definition);
            if !blocked_by.is_empty() {
                manifest.mark_step_blocked(&step.asset_id, Utc::now(), blocked_by)?;
                self.persist_manifest(&mut manifest, false).await?;
                continue;
            }

            let asset_id = step.asset_id;
            let asset_partition = step.partition;
            if options.only_forced_assets && !options.force_assets.contains(&asset_id) {
                manifest.mark_step_skipped(
                    &asset_id,
                    Utc::now(),
                    "only-forced-assets mode; using current dependency snapshot",
                )?;
                self.persist_manifest(&mut manifest, false).await?;
                continue;
            }
            if should_skip_missing_optional_source_input(&asset_id, &options.source_inputs) {
                manifest.mark_step_skipped(
                    &asset_id,
                    Utc::now(),
                    "optional source input missing; enrichment gap recorded",
                )?;
                self.persist_manifest(&mut manifest, false).await?;
                continue;
            }
            if options.skip_missing_source_inputs
                && should_skip_missing_source_input(&asset_id, &options.source_inputs)
            {
                manifest.mark_step_skipped(
                    &asset_id,
                    Utc::now(),
                    "scoped source input missing; skipped",
                )?;
                self.persist_manifest(&mut manifest, false).await?;
                continue;
            }
            let mut attempt = 0;
            loop {
                attempt += 1;
                let started_at = Utc::now();
                manifest.mark_step_running(&asset_id, started_at)?;
                self.persist_manifest(&mut manifest, false).await?;

                let asset_execution_timeout_ms = options.asset_execution_timeout_ms();
                let execution = tokio::time::timeout(
                    Duration::from_millis(asset_execution_timeout_ms),
                    self.execute_asset(AssetExecutionContext {
                        dag: self,
                        graph,
                        options: &options,
                        run_id: &manifest.run_id,
                        asset_id: &asset_id,
                        asset_partition: &asset_partition,
                        records_by_asset: &records_by_asset,
                        dependency_snapshot: &dependency_snapshot,
                        kg_view: kg_view.as_ref(),
                    }),
                )
                .await
                .map_err(|_| AssetDagExecutorError::AssetExecutionTimedOut {
                    asset_id: asset_id.clone(),
                    timeout_ms: asset_execution_timeout_ms,
                })
                .and_then(|result| result);

                match execution {
                    Ok(executed) => {
                        let completed_at = Utc::now();
                        let record = executed.record().clone();
                        if let Err(err) = self
                            .validate_record_with_retry(
                                asset_id.clone(),
                                &record,
                                &records_by_asset,
                                &dependency_snapshot,
                                &options.partition,
                                &options.retry_policy,
                            )
                            .await
                        {
                            manifest.mark_step_failed(
                                &asset_id,
                                started_at,
                                completed_at,
                                err.to_string(),
                            )?;
                            self.persist_manifest(&mut manifest, false).await?;
                            first_error.get_or_insert(err);
                            break;
                        }
                        manifest.mark_step_materialized(
                            &asset_id,
                            &record,
                            started_at,
                            completed_at,
                        )?;
                        self.persist_manifest(&mut manifest, false).await?;
                        if !manifest.promote_current {
                            manifest.mark_step_promoted(&asset_id, Utc::now())?;
                            self.persist_manifest(&mut manifest, false).await?;
                            records_by_asset.insert(asset_id.clone(), record);
                            if asset_id.as_str() == KG_SOCIETY_VIEW_ASSET_ID {
                                kg_view = self
                                    .restore_kg_view_runtime(
                                        graph,
                                        &records_by_asset,
                                        &dependency_snapshot,
                                        &options.partition,
                                    )
                                    .await?;
                            }
                            executed_assets.push(asset_id.clone());
                            break;
                        }
                        match self
                            .promote_materialization_with_retry(
                                &record,
                                manifest.created_at,
                                step.current_materialization_id.as_ref(),
                                &options.retry_policy,
                            )
                            .await
                        {
                            Ok(()) => {
                                manifest.mark_step_promoted(&asset_id, Utc::now())?;
                                self.persist_manifest(&mut manifest, false).await?;
                                records_by_asset.insert(asset_id.clone(), record);
                                if asset_id.as_str() == KG_SOCIETY_VIEW_ASSET_ID {
                                    kg_view = self
                                        .restore_kg_view_runtime(
                                            graph,
                                            &records_by_asset,
                                            &dependency_snapshot,
                                            &options.partition,
                                        )
                                        .await?;
                                }
                                executed_assets.push(asset_id.clone());
                            }
                            Err(err) => {
                                manifest.mark_materialized_step_failed(
                                    &asset_id,
                                    Utc::now(),
                                    err.to_string(),
                                )?;
                                self.persist_manifest(&mut manifest, false).await?;
                                first_error.get_or_insert(err);
                            }
                        }
                        break;
                    }
                    Err(err) => {
                        let completed_at = Utc::now();
                        let should_retry = err.is_retryable()
                            && attempt < options.retry_policy.max_attempts.max(1);
                        if should_retry {
                            manifest.mark_step_attempt_failed(
                                &asset_id,
                                started_at,
                                completed_at,
                                err.to_string(),
                            )?;
                            self.persist_manifest(&mut manifest, false).await?;
                            tokio::time::sleep(retry_delay(&options.retry_policy, attempt)).await;
                            continue;
                        }
                        manifest.mark_step_failed(
                            &asset_id,
                            started_at,
                            completed_at,
                            err.to_string(),
                        )?;
                        self.persist_manifest(&mut manifest, false).await?;
                        first_error.get_or_insert(err);
                        break;
                    }
                }
            }
        }

        let completed_at = Utc::now();
        manifest.finish(completed_at)?;
        manifest.resume_lease = None;
        let promote_current = manifest.promote_current;
        let persisted = self
            .persist_manifest(&mut manifest, promote_current)
            .await?;

        if manifest.status == super::DagRunStatus::Failed {
            if let Some(err) = first_error {
                return Err(err);
            }
        }

        Ok(AssetDagExecutionReport {
            dry_run: false,
            manifest,
            run_manifest_key: Some(persisted.run_manifest_key),
            current_pointer_key: persisted.current_pointer_key,
            executed_assets,
        })
    }

    async fn record_for_manifest_step(
        &self,
        step: &super::AssetRunStep,
    ) -> Result<MaterializationRecord, AssetDagExecutorError> {
        let materialization_id = step.materialization_id.as_ref().ok_or_else(|| {
            AssetDagExecutorError::ResumeMissingMaterialization {
                asset_id: step.asset_id.clone(),
            }
        })?;
        Ok(self
            .materializations
            .record(&step.asset_id, &step.partition, materialization_id)
            .await?)
    }

    async fn promote_materialization_with_retry(
        &self,
        record: &MaterializationRecord,
        run_created_at: DateTime<Utc>,
        expected_current: Option<&MaterializationId>,
        policy: &AssetRetryPolicy,
    ) -> Result<(), AssetDagExecutorError> {
        if record.asset_id.as_str() == SEARCH_SERVING_BUNDLE_ASSET_ID {
            let report = validate_search_serving_candidate(&self.lake, record)
                .await
                .map_err(|error| {
                    AssetDagExecutorError::ServingReleaseValidation(error.to_string())
                })?;
            if !report.passed {
                return Err(AssetDagExecutorError::ServingReleaseValidation(format!(
                    "bundle {} failed {} release gate(s): {}",
                    report.bundle_version,
                    report.issues.len(),
                    report
                        .issues
                        .iter()
                        .take(5)
                        .map(|issue| match issue.reference.as_deref() {
                            Some(reference) => format!("{} ({reference})", issue.message),
                            None => issue.message.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            if self.sync_frontend_manifest {
                write_frontend_media_manifest(&self.project_root, &report).map_err(|error| {
                    AssetDagExecutorError::ServingReleaseValidation(format!(
                        "could not write frontend media manifest: {error}"
                    ))
                })?;
            }
        }
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self
                .materializations
                .promote_current_for_run_if_current(record, run_created_at, expected_current)
                .await
            {
                Ok(_) => return Ok(()),
                Err(err) if err.is_retryable() && attempt < policy.max_attempts.max(1) => {
                    tokio::time::sleep(retry_delay(policy, attempt)).await;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    async fn restore_succeeded_records(
        &self,
        manifest: &AssetDagRunManifest,
    ) -> Result<HashMap<AssetId, MaterializationRecord>, AssetDagExecutorError> {
        let mut records = HashMap::new();
        for step in manifest
            .steps
            .iter()
            .filter(|step| step.status == super::AssetRunStepStatus::Succeeded)
        {
            let materialization_id = step.materialization_id.as_ref().ok_or_else(|| {
                AssetDagExecutorError::ResumeMissingMaterialization {
                    asset_id: step.asset_id.clone(),
                }
            })?;
            let record = self
                .materializations
                .record(&step.asset_id, &step.partition, materialization_id)
                .await?;
            if record.run_id != manifest.run_id {
                return Err(AssetDagExecutorError::ResumeMaterializationRunMismatch {
                    asset_id: step.asset_id.clone(),
                    expected: manifest.run_id.clone(),
                    actual: record.run_id,
                });
            }
            self.validate_restored_artifacts(&step.asset_id, &record)
                .await?;
            records.insert(step.asset_id.clone(), record);
        }
        Ok(records)
    }

    async fn validate_restored_artifacts(
        &self,
        asset_id: &AssetId,
        record: &MaterializationRecord,
    ) -> Result<(), AssetDagExecutorError> {
        for artifact in &record.artifacts {
            let key = crate::lake::LakeKey::new(artifact.key.clone()).map_err(LakeError::Key)?;
            let actual = match self.lake.artifact_metadata(&key).await {
                Ok(actual) => actual,
                Err(err) if err.is_not_found() => {
                    return Err(AssetDagExecutorError::ResumeArtifactIntegrity {
                        asset_id: asset_id.clone(),
                        key: artifact.key.clone(),
                        reason: "artifact is missing".to_string(),
                    });
                }
                Err(err) => return Err(err.into()),
            };
            if artifact.hash_algorithm != actual.hash_algorithm
                || artifact.content_hash != actual.content_hash
                || artifact.size_bytes != actual.size_bytes
            {
                return Err(AssetDagExecutorError::ResumeArtifactIntegrity {
                    asset_id: asset_id.clone(),
                    key: artifact.key.clone(),
                    reason: format!(
                        "expected {}:{} ({} bytes), got {}:{} ({} bytes)",
                        artifact.hash_algorithm,
                        artifact.content_hash,
                        artifact.size_bytes,
                        actual.hash_algorithm,
                        actual.content_hash,
                        actual.size_bytes
                    ),
                });
            }
        }
        Ok(())
    }

    async fn load_dependency_snapshot(
        &self,
        manifest: &AssetDagRunManifest,
    ) -> Result<HashMap<AssetId, Vec<MaterializationRecord>>, AssetDagExecutorError> {
        let mut snapshot: HashMap<AssetId, Vec<MaterializationRecord>> = HashMap::new();
        for step in &manifest.steps {
            if let Some(materialization_id) = &step.current_materialization_id {
                let record = self
                    .materializations
                    .record(&step.asset_id, &step.partition, materialization_id)
                    .await?;
                insert_snapshot_record(&mut snapshot, record);
            }
            for dependency in &step.dependencies {
                for materialization_id in &step.dependency_snapshot {
                    if let Some(record) = self
                        .materializations
                        .record_by_id_for_asset(dependency, materialization_id)
                        .await?
                    {
                        insert_snapshot_record(&mut snapshot, record);
                    }
                }
            }
        }
        for records in snapshot.values_mut() {
            sort_materialization_records(records);
        }
        Ok(snapshot)
    }

    async fn restore_kg_view_runtime(
        &self,
        _graph: &KnowledgeGraph,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
        run_partition: &AssetPartition,
    ) -> Result<Option<KgSocietyViewMaterialization>, AssetDagExecutorError> {
        let asset_id = static_asset_id(KG_SOCIETY_VIEW_ASSET_ID);
        let expected_partition = self.asset_partition(&asset_id, run_partition)?;
        let record = records_by_asset.get(&asset_id).or_else(|| {
            dependency_snapshot.get(&asset_id).and_then(|records| {
                records
                    .iter()
                    .find(|record| record.partition == expected_partition)
            })
        });
        let Some(record) = record else {
            return Ok(None);
        };
        self.validate_restored_artifacts(&asset_id, record).await?;
        let manifest_artifact = record
            .artifacts
            .iter()
            .find(|artifact| artifact.key.ends_with("manifest.json"))
            .ok_or_else(|| AssetDagExecutorError::ResumeMissingKgViewManifest {
                materialization_id: record.materialization_id.clone(),
            })?;
        let manifest_key =
            crate::lake::LakeKey::new(manifest_artifact.key.clone()).map_err(LakeError::Key)?;
        let manifest: KgViewManifest = self.lake.get_json(&manifest_key).await?;
        let records = super::load_kg_view_records(&self.lake, &manifest).await?;
        if manifest.graph_content_hash != records.content_hash {
            return Err(AssetDagExecutorError::ResumeKgViewContentMismatch {
                materialization_id: record.materialization_id.clone(),
                expected: manifest.graph_content_hash,
                actual: records.content_hash,
            });
        }
        Ok(Some(KgSocietyViewMaterialization {
            manifest,
            record: record.clone(),
            records,
        }))
    }

    async fn execute_asset(
        &self,
        context: AssetExecutionContext<'_>,
    ) -> Result<ExecutedAsset, AssetDagExecutorError> {
        let executor = self.executors.get(context.asset_id).ok_or_else(|| {
            AssetDagExecutorError::NoExecutor {
                asset_id: context.asset_id.clone(),
            }
        })?;
        executor.execute(context).await
    }

    async fn dependency_materializations(
        &self,
        asset_id: &AssetId,
        run_partition: &AssetPartition,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<Vec<MaterializationId>, AssetDagExecutorError> {
        Ok(self
            .dependency_materialization_records(
                asset_id,
                run_partition,
                records_by_asset,
                dependency_snapshot,
            )
            .await?
            .into_iter()
            .map(|record| record.materialization_id)
            .collect())
    }

    async fn dependency_materialization_records(
        &self,
        asset_id: &AssetId,
        run_partition: &AssetPartition,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let definition =
            self.registry
                .get(asset_id)
                .ok_or_else(|| AssetDagExecutorError::UnknownAsset {
                    asset_id: asset_id.clone(),
                })?;
        let mut parents = Vec::new();

        for dependency in &definition.dependencies {
            let dependency_records = match self
                .dependency_records(
                    definition,
                    asset_id,
                    dependency,
                    run_partition,
                    records_by_asset,
                    dependency_snapshot,
                )
                .await
            {
                Ok(records) => records,
                Err(AssetDagExecutorError::MissingDependency { .. })
                    if definition.is_optional_dependency(dependency) =>
                {
                    Vec::new()
                }
                Err(error) => return Err(error),
            };
            parents.extend(dependency_records);
        }

        Ok(parents)
    }

    async fn dependency_records(
        &self,
        definition: &super::AssetDefinition,
        asset_id: &AssetId,
        dependency: &AssetId,
        run_partition: &AssetPartition,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let records = match definition.dependency_fan_in_policy(dependency) {
            DependencyFanInPolicy::ResolvedPartition => {
                vec![
                    self.resolved_dependency_record(
                        asset_id,
                        dependency,
                        run_partition,
                        records_by_asset,
                        dependency_snapshot,
                    )
                    .await?,
                ]
            }
            DependencyFanInPolicy::AllCurrentPartitions => {
                self.all_current_dependency_records(
                    dependency,
                    records_by_asset,
                    dependency_snapshot,
                )
                .await?
            }
        };

        if records.is_empty() {
            return Err(AssetDagExecutorError::MissingDependency {
                asset_id: asset_id.clone(),
                dependency: dependency.clone(),
            });
        }
        Ok(records)
    }

    async fn resolved_dependency_record(
        &self,
        asset_id: &AssetId,
        dependency: &AssetId,
        run_partition: &AssetPartition,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<MaterializationRecord, AssetDagExecutorError> {
        if let Some(record) = records_by_asset.get(dependency) {
            return Ok(record.clone());
        }

        let dependency_partition = self.asset_partition(dependency, run_partition)?;
        dependency_snapshot
            .get(dependency)
            .and_then(|records| {
                records
                    .iter()
                    .find(|record| record.partition == dependency_partition)
            })
            .cloned()
            .ok_or_else(|| AssetDagExecutorError::MissingDependency {
                asset_id: asset_id.clone(),
                dependency: dependency.clone(),
            })
    }

    async fn all_current_dependency_records(
        &self,
        dependency: &AssetId,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let mut records = dependency_snapshot
            .get(dependency)
            .cloned()
            .unwrap_or_default();

        if let Some(run_record) = records_by_asset.get(dependency) {
            records.retain(|record| record.partition != run_record.partition);
            records.push(run_record.clone());
        }

        sort_materialization_records(&mut records);
        Ok(records)
    }

    fn asset_partition(
        &self,
        asset_id: &AssetId,
        run_partition: &AssetPartition,
    ) -> Result<AssetPartition, AssetDagExecutorError> {
        self.registry
            .partition_for(asset_id, run_partition)
            .map_err(AssetDagExecutorError::Partition)
    }

    async fn validate_record_with_retry(
        &self,
        asset_id: AssetId,
        record: &MaterializationRecord,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
        run_partition: &AssetPartition,
        policy: &AssetRetryPolicy,
    ) -> Result<(), AssetDagExecutorError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self
                .validate_record(
                    asset_id.clone(),
                    record,
                    records_by_asset,
                    dependency_snapshot,
                    run_partition,
                )
                .await
            {
                Ok(()) => return Ok(()),
                Err(err) if err.is_retryable() && attempt < policy.max_attempts.max(1) => {
                    tokio::time::sleep(retry_delay(policy, attempt)).await;
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn validate_record(
        &self,
        asset_id: AssetId,
        record: &MaterializationRecord,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        dependency_snapshot: &HashMap<AssetId, Vec<MaterializationRecord>>,
        run_partition: &AssetPartition,
    ) -> Result<(), AssetDagExecutorError> {
        let expected_partition = self.asset_partition(&asset_id, run_partition)?;
        if record.partition != expected_partition {
            return Err(AssetDagExecutorError::AssetPartitionMismatch {
                asset_id,
                expected: expected_partition,
                actual: record.partition.clone(),
            });
        }
        let expected = self
            .dependency_materializations(
                &asset_id,
                run_partition,
                records_by_asset,
                dependency_snapshot,
            )
            .await?;
        if record.parent_materializations != expected {
            return Err(AssetDagExecutorError::ParentLineageMismatch {
                asset_id,
                expected,
                actual: record.parent_materializations.clone(),
            });
        }
        Ok(())
    }

    async fn persist_manifest(
        &self,
        manifest: &mut AssetDagRunManifest,
        promote_current: bool,
    ) -> Result<PersistedManifest, AssetDagExecutorError> {
        manifest.renew_resume_lease(Utc::now());
        let meta = self.run_manifests.write_manifest_cas(manifest).await?;
        if promote_current {
            self.run_manifests.promote_current(manifest).await?;
        }
        Ok(PersistedManifest {
            run_manifest_key: meta.key.to_string(),
            current_pointer_key: promote_current.then(|| {
                super::AssetPathBuilder::current_dag_run_pointer_key(&manifest.partition)
                    .to_string()
            }),
        })
    }
}

#[derive(Debug)]
struct PersistedManifest {
    run_manifest_key: String,
    current_pointer_key: Option<String>,
}

#[derive(Clone)]
struct BuiltInAssetExecutorRegistry {
    executors: HashMap<AssetId, BuiltInAssetExecutor>,
}

impl BuiltInAssetExecutorRegistry {
    fn default_openestates() -> Self {
        let mut executors = HashMap::new();
        executors.insert(
            static_asset_id(RERA_RECEIPTS_ASSET_ID),
            BuiltInAssetExecutor::ReraReceipts,
        );
        executors.insert(
            static_asset_id(RERA_SOURCE_RECORDS_ASSET_ID),
            BuiltInAssetExecutor::ReraSourceRecords,
        );
        executors.insert(
            static_asset_id(RERA_CLAIMS_ASSET_ID),
            BuiltInAssetExecutor::ReraClaims,
        );
        executors.insert(
            static_asset_id(RERA_REGISTRY_MONTHLY_ASSET_ID),
            BuiltInAssetExecutor::ReraRegistryMonthly,
        );
        executors.insert(
            static_asset_id(CANONICAL_SOCIETY_NODES_ASSET_ID),
            BuiltInAssetExecutor::CanonicalSocietyNodes,
        );
        executors.insert(
            static_asset_id(RERA_LEGAL_FACTS_ASSET_ID),
            BuiltInAssetExecutor::ReraLegalFacts,
        );
        executors.insert(
            static_asset_id(RERA_PROJECT_PLAN_FRAMES_ASSET_ID),
            BuiltInAssetExecutor::ReraProjectPlanFrames,
        );
        executors.insert(
            static_asset_id(GOOGLE_PLACES_WEEKLY_ASSET_ID),
            BuiltInAssetExecutor::GooglePlacesWeekly,
        );
        executors.insert(
            static_asset_id(GOOGLE_REVIEW_FACTS_ASSET_ID),
            BuiltInAssetExecutor::GoogleReviewFacts,
        );
        executors.insert(
            static_asset_id(GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID),
            BuiltInAssetExecutor::GoogleNearbyPlacesWeekly,
        );
        executors.insert(
            static_asset_id(GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID),
            BuiltInAssetExecutor::GoogleNearbyPlaceFacts,
        );
        executors.insert(
            static_asset_id(EXTERNAL_LISTINGS_WEEKLY_ASSET_ID),
            BuiltInAssetExecutor::ExternalListingsWeekly,
        );
        executors.insert(
            static_asset_id(EXTERNAL_LISTING_FACTS_ASSET_ID),
            BuiltInAssetExecutor::ExternalListingFacts,
        );
        executors.insert(
            static_asset_id(EXTERNAL_IMAGES_WEEKLY_ASSET_ID),
            BuiltInAssetExecutor::ExternalImagesWeekly,
        );
        executors.insert(
            static_asset_id(IMAGE_MEDIA_FACTS_ASSET_ID),
            BuiltInAssetExecutor::ImageMediaFacts,
        );
        executors.insert(
            static_asset_id(BUILDER_RERA_AGGREGATES_ASSET_ID),
            BuiltInAssetExecutor::BuilderReraAggregates,
        );
        executors.insert(
            static_asset_id(HOME_STATE_SIGNALS_ASSET_ID),
            BuiltInAssetExecutor::HomeStateSignals,
        );
        executors.insert(
            static_asset_id(APPROACH_ROAD_GRAPH_FACTS_ASSET_ID),
            BuiltInAssetExecutor::ApproachRoadGraphFacts,
        );
        executors.insert(
            static_asset_id(SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID),
            BuiltInAssetExecutor::SocietyGroundwaterPotentialFacts,
        );
        executors.insert(
            static_asset_id(BENGALURU_METRO_STATION_FACTS_ASSET_ID),
            BuiltInAssetExecutor::BengaluruMetroStationFacts,
        );
        executors.insert(
            static_asset_id(OSM_POWER_LINE_FACTS_ASSET_ID),
            BuiltInAssetExecutor::OsmPowerLineFacts,
        );
        executors.insert(
            static_asset_id(STORMWATER_DRAIN_FACTS_ASSET_ID),
            BuiltInAssetExecutor::StormwaterDrainFacts,
        );
        executors.insert(
            static_asset_id(CURRENT_PROJECT_FACTS_ASSET_ID),
            BuiltInAssetExecutor::CurrentProjectFacts,
        );
        executors.insert(
            static_asset_id(KG_SOCIETY_VIEW_ASSET_ID),
            BuiltInAssetExecutor::KgSocietyView,
        );
        executors.insert(
            static_asset_id(SEARCH_SERVING_BUNDLE_ASSET_ID),
            BuiltInAssetExecutor::SearchServingBundle,
        );
        Self { executors }
    }

    fn get(&self, asset_id: &AssetId) -> Option<&BuiltInAssetExecutor> {
        self.executors.get(asset_id)
    }
}

#[derive(Clone)]
enum BuiltInAssetExecutor {
    ReraReceipts,
    ReraSourceRecords,
    ReraClaims,
    ReraRegistryMonthly,
    CanonicalSocietyNodes,
    ReraLegalFacts,
    ReraProjectPlanFrames,
    GooglePlacesWeekly,
    GoogleReviewFacts,
    GoogleNearbyPlacesWeekly,
    GoogleNearbyPlaceFacts,
    ExternalListingsWeekly,
    ExternalListingFacts,
    ExternalImagesWeekly,
    ImageMediaFacts,
    BuilderReraAggregates,
    HomeStateSignals,
    ApproachRoadGraphFacts,
    SocietyGroundwaterPotentialFacts,
    BengaluruMetroStationFacts,
    OsmPowerLineFacts,
    StormwaterDrainFacts,
    CurrentProjectFacts,
    KgSocietyView,
    SearchServingBundle,
    #[cfg(test)]
    TestFailOnce(std::sync::Arc<std::sync::atomic::AtomicUsize>),
    #[cfg(test)]
    TestSleep(Duration),
}

impl BuiltInAssetExecutor {
    async fn execute(
        &self,
        context: AssetExecutionContext<'_>,
    ) -> Result<ExecutedAsset, AssetDagExecutorError> {
        match self {
            Self::ReraReceipts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .rera_receipts
                    .clone()
                    .ok_or_else(|| source_input_error(&context))?
                    .into_receipts_input()?;
                let record = ReraReceiptsMaterializer::new(context.dag.lake.clone())
                    .materialize_for_run(
                        &input,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ReraSourceRecords => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .rera_source_records
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let receipts_record =
                    dependency_record(context.asset_id, &parent_records, RERA_RECEIPTS_ASSET_ID)?;
                let record = ReraSourceRecordsMaterializer::new(context.dag.lake.clone())
                    .materialize_for_run(
                        input,
                        receipts_record,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ReraClaims => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let source_records = dependency_record(
                    context.asset_id,
                    &parent_records,
                    RERA_SOURCE_RECORDS_ASSET_ID,
                )?;
                let record = ReraClaimsMaterializer::new(context.dag.lake.clone())
                    .materialize_from_source_records_for_run(
                        source_records,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ReraRegistryMonthly => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .rera_registry_monthly
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let record = ReraRegistryMaterializer::new(context.dag.lake.clone())
                    .materialize_for_run(
                        input,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::CanonicalSocietyNodes => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let rera_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    RERA_REGISTRY_MONTHLY_ASSET_ID,
                )?;
                let record = super::CanonicalSocietyMaterializer::new(context.dag.lake.clone())
                    .materialize_from_rera_for_run(
                        rera_record,
                        &context.options.version,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ReraLegalFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let rera_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    RERA_REGISTRY_MONTHLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::rera_legal_facts_input(
                    &context.dag.lake,
                    rera_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let record = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(record))
            }
            Self::ReraProjectPlanFrames => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let source_input = context
                    .options
                    .source_inputs
                    .rera_project_plan_frames
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let input = super::rera_project_plan_frames_input(
                    &context.dag.lake,
                    source_input,
                    &context.run_id.to_string(),
                    context.options.planned_at,
                )
                .await?;
                let record = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(record))
            }
            Self::GooglePlacesWeekly => {
                let input = context
                    .options
                    .source_inputs
                    .google_places_weekly
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::canonicalize_google_places_input(
                    &context.dag.lake,
                    input,
                    canonical_record,
                    &context.options.source_inputs.source_entities,
                    context.options.source_scope,
                )
                .await?;
                let parent_materializations = parent_records
                    .iter()
                    .map(|record| record.materialization_id.clone())
                    .collect();
                let materialization =
                    GooglePlaceSnapshotMaterializer::new(context.dag.lake.clone())
                        .materialize_for_run(
                            &input,
                            context.run_id.to_string(),
                            parent_materializations,
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?;
                Ok(ExecutedAsset::Record(materialization.record))
            }
            Self::GoogleReviewFacts => {
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let google_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    GOOGLE_PLACES_WEEKLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::google_review_facts_input_with_aliases(
                    &context.dag.lake,
                    google_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::GoogleNearbyPlacesWeekly => {
                let input = context
                    .options
                    .source_inputs
                    .google_nearby_places_weekly
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::canonicalize_google_nearby_places_input(
                    &context.dag.lake,
                    input,
                    canonical_record,
                    &context.options.source_inputs.source_entities,
                    context.options.source_scope,
                )
                .await?;
                let parent_materializations = parent_records
                    .iter()
                    .map(|record| record.materialization_id.clone())
                    .collect();
                let materialization =
                    GooglePlaceSnapshotMaterializer::new(context.dag.lake.clone())
                        .materialize_nearby_for_run(
                            &input,
                            context.run_id.to_string(),
                            parent_materializations,
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?;
                Ok(ExecutedAsset::Record(materialization.record))
            }
            Self::GoogleNearbyPlaceFacts => {
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let nearby_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::google_nearby_place_facts_input_with_aliases(
                    &context.dag.lake,
                    nearby_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::ExternalListingsWeekly => {
                let input = context
                    .options
                    .source_inputs
                    .external_listings_weekly
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let parent_materializations = parent_records
                    .iter()
                    .map(|record| record.materialization_id.clone())
                    .collect();
                let record = ProjectEnrichmentMaterializer::new(context.dag.lake.clone())
                    .materialize_external_listings(
                        input,
                        parent_materializations,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ExternalListingFacts => {
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let listing_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::external_listing_facts_input_with_aliases(
                    &context.dag.lake,
                    listing_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::ExternalImagesWeekly => {
                let input = context
                    .options
                    .source_inputs
                    .external_images_weekly
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let input =
                    ingest_local_media_assets(&context.dag.lake, &context.dag.project_root, input)
                        .await?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let parent_materializations = parent_records
                    .iter()
                    .map(|record| record.materialization_id.clone())
                    .collect();
                let record = MediaAssetMaterializer::new(context.dag.lake.clone())
                    .materialize_external_images(
                        &input,
                        parent_materializations,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            Self::ImageMediaFacts => {
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let image_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::image_media_facts_input_with_aliases(
                    &context.dag.lake,
                    image_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::BuilderReraAggregates => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let rera_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    RERA_REGISTRY_MONTHLY_ASSET_ID,
                )?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::builder_rera_aggregate_facts_input(
                    &context.dag.lake,
                    rera_record,
                    canonical_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::HomeStateSignals => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let rera_facts_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    RERA_LEGAL_FACTS_ASSET_ID,
                )?;
                let input = super::home_state_signals_input(
                    &context.dag.lake,
                    std::slice::from_ref(rera_facts_record),
                    context.run_id,
                    context.options.planned_at,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::ApproachRoadGraphFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let materialization = ApproachRoadGraphMaterializer::new(context.dag.lake.clone())
                    .materialize_for_run(
                        context.options.planned_at,
                        &parent_records,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                    )
                    .await?;
                Ok(ExecutedAsset::Record(materialization.record))
            }
            Self::SocietyGroundwaterPotentialFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .environment_groundwater_potential
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let input = super::society_groundwater_potential_facts_input(
                    &context.dag.lake,
                    input,
                    &parent_records,
                    &context.options.source_inputs.source_entities,
                    context.run_id,
                    context.options.planned_at,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::BengaluruMetroStationFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .bengaluru_metro_stations
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let run_id = context.run_id.to_string();
                let input = super::bengaluru_metro_station_facts_input(
                    input,
                    &run_id,
                    context.options.planned_at,
                )?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::OsmPowerLineFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .osm_power_infrastructure
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::canonicalize_osm_power_infrastructure_input(
                    &context.dag.lake,
                    input,
                    canonical_record,
                    &context.options.source_inputs.source_entities,
                    context.options.source_scope,
                )
                .await?;
                let input = super::osm_power_line_facts_input(&input, &context.run_id.to_string())?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::StormwaterDrainFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .stormwater_drains
                    .as_ref()
                    .ok_or_else(|| source_input_error(&context))?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let input = super::canonicalize_stormwater_drain_input(
                    &context.dag.lake,
                    input,
                    canonical_record,
                    &context.options.source_inputs.source_entities,
                    context.options.source_scope,
                )
                .await?;
                let input =
                    super::stormwater_drain_facts_input(&input, &context.run_id.to_string())?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::CurrentProjectFacts => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let materialization =
                    CurrentProjectFactsMaterializer::new(context.dag.lake.clone())
                        .materialize_for_run(
                            &parent_records,
                            &context.options.version,
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                            &context.options.source_inputs.source_entities,
                            context.options.source_scope,
                            context.options.planned_at,
                        )
                        .await?;
                Ok(ExecutedAsset::Record(materialization.record))
            }
            Self::KgSocietyView => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let definition = context.dag.registry.get(context.asset_id).ok_or_else(|| {
                    AssetDagExecutorError::UnknownAsset {
                        asset_id: context.asset_id.clone(),
                    }
                })?;
                let support_records = support_fact_records(definition, &parent_records);
                let support_rows =
                    read_skill_fact_artifact_rows(&context.dag.lake, &support_records).await?;
                let canonical_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    CANONICAL_SOCIETY_NODES_ASSET_ID,
                )?;
                let mut canonical_rows =
                    super::read_canonical_society_rows(&context.dag.lake, canonical_record).await?;
                if let Some(approach_record) = parent_records
                    .iter()
                    .find(|record| record.asset_id.as_str() == APPROACH_ROAD_GRAPH_FACTS_ASSET_ID)
                {
                    let approach_rows =
                        super::read_approach_road_graph_rows(&context.dag.lake, approach_record)
                            .await?;
                    canonical_rows
                        .entities
                        .extend(approach_rows.canonical.entities);
                    canonical_rows.edges.extend(approach_rows.canonical.edges);
                }
                let parent_materializations = parent_records
                    .iter()
                    .map(|record| record.materialization_id.clone())
                    .collect();
                let materialization = KgSocietyViewMaterializer::new(context.dag.lake.clone())
                    .materialize_for_run_with_asset_rows(
                        context.graph,
                        context.options.version.clone(),
                        vec![knowledge_graph_watermark(context.graph)],
                        parent_materializations,
                        context.run_id.clone(),
                        context.asset_partition.clone(),
                        &canonical_rows.entities,
                        &canonical_rows.edges,
                        &support_rows.facts,
                        &support_rows.fact_annotations,
                    )
                    .await?;
                Ok(ExecutedAsset::KgSocietyView(materialization))
            }
            Self::SearchServingBundle => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let kg_view = context
                    .kg_view
                    .ok_or(AssetDagExecutorError::MissingRuntimeKgView)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                        context.dependency_snapshot,
                    )
                    .await?;
                let rera_parents = parent_records
                    .iter()
                    .filter(|record| {
                        matches!(
                            record.asset_id.as_str(),
                            RERA_RECEIPTS_ASSET_ID
                                | RERA_SOURCE_RECORDS_ASSET_ID
                                | RERA_CLAIMS_ASSET_ID
                        )
                    })
                    .collect::<Vec<_>>();
                let materializer = SearchServingBundleMaterializer::new(context.dag.lake.clone());
                let materialization = if rera_parents.is_empty() {
                    materializer
                        .materialize_from_kg_view_for_run(
                            kg_view,
                            context.options.version.clone(),
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?
                } else {
                    let receipt_record = dependency_record(
                        context.asset_id,
                        &parent_records,
                        RERA_RECEIPTS_ASSET_ID,
                    )?;
                    let source_record = dependency_record(
                        context.asset_id,
                        &parent_records,
                        RERA_SOURCE_RECORDS_ASSET_ID,
                    )?;
                    let claims_record =
                        dependency_record(context.asset_id, &parent_records, RERA_CLAIMS_ASSET_ID)?;
                    let receipts =
                        read_rera_receipt_records(&context.dag.lake, receipt_record).await?;
                    let source_records =
                        read_rera_source_records(&context.dag.lake, source_record).await?;
                    let claims = read_rera_claims(&context.dag.lake, claims_record).await?;
                    let rera_evidence = project_rera_evidence(&source_records, &claims, &receipts)?;
                    materializer
                        .materialize_from_kg_view_and_rera_for_run(
                            kg_view,
                            rera_evidence,
                            rera_parents
                                .into_iter()
                                .map(|record| record.materialization_id.clone())
                                .collect(),
                            context.options.version.clone(),
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?
                };
                Ok(ExecutedAsset::SearchServingBundle(materialization))
            }
            #[cfg(test)]
            Self::TestFailOnce(attempts) => {
                use std::sync::atomic::Ordering;

                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(AssetDagExecutorError::Lake(LakeError::Io(
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "injected transient timeout",
                        ),
                    )));
                }
                let record = MaterializationRecord::succeeded(
                    context.asset_id.clone(),
                    super::AssetStage::Raw,
                    context.asset_partition.clone(),
                    context.options.version.clone(),
                    Vec::new(),
                )
                .with_run_id(context.run_id.clone());
                context
                    .dag
                    .materializations
                    .write_materialization(&record)
                    .await?;
                Ok(ExecutedAsset::Record(record))
            }
            #[cfg(test)]
            Self::TestSleep(duration) => {
                tokio::time::sleep(*duration).await;
                unreachable!("test sleep executor should be cancelled by its timeout")
            }
        }
    }
}

struct AssetExecutionContext<'a> {
    dag: &'a AssetDagExecutor,
    graph: &'a KnowledgeGraph,
    options: &'a AssetDagExecutionOptions,
    run_id: &'a MaterializationId,
    asset_id: &'a AssetId,
    asset_partition: &'a AssetPartition,
    records_by_asset: &'a HashMap<AssetId, MaterializationRecord>,
    dependency_snapshot: &'a HashMap<AssetId, Vec<MaterializationRecord>>,
    kg_view: Option<&'a KgSocietyViewMaterialization>,
}

async fn execute_skill_fact_asset(
    context: AssetExecutionContext<'_>,
    input: &SkillFactsInput,
) -> Result<MaterializationRecord, AssetDagExecutorError> {
    let parent_materializations = context
        .dag
        .dependency_materializations(
            context.asset_id,
            &context.options.partition,
            context.records_by_asset,
            context.dependency_snapshot,
        )
        .await?;
    let materializer = SkillFactMaterializer::new(context.dag.lake.clone());
    let watermarks = skill_fact_watermarks(input);
    let materialization = if input.facts.is_empty() {
        materializer
            .materialize_skipped_for_run(
                context.asset_id.as_str(),
                input.source.clone(),
                input.snapshot_date.clone(),
                context.run_id.to_string(),
                parent_materializations,
                watermarks,
                context.run_id.clone(),
                context.asset_partition.clone(),
            )
            .await?
    } else {
        materializer
            .materialize_for_run(
                context.asset_id.as_str(),
                input.source.clone(),
                input.snapshot_date.clone(),
                context.run_id.to_string(),
                &input.facts,
                &input.fact_annotations,
                parent_materializations,
                watermarks,
                context.run_id.clone(),
                context.asset_partition.clone(),
            )
            .await?
    };
    Ok(materialization.record)
}

fn skill_fact_watermarks(input: &SkillFactsInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }

    let high_watermark = input
        .facts
        .iter()
        .map(|fact| fact.learned_at)
        .max()
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| input.snapshot_date.clone());
    vec![SourceWatermark {
        source: input.source.clone(),
        high_watermark,
    }]
}

fn support_fact_records(
    definition: &AssetDefinition,
    parent_records: &[MaterializationRecord],
) -> Vec<MaterializationRecord> {
    parent_records
        .iter()
        .filter(|record| {
            record.asset_id.as_str() == CURRENT_PROJECT_FACTS_ASSET_ID
                || (record.stage == AssetStage::Silver
                    && (definition.dependencies.contains(&record.asset_id)
                        || definition.dependency_fan_in_policy(&record.asset_id)
                            == DependencyFanInPolicy::AllCurrentPartitions))
        })
        .cloned()
        .collect()
}

fn insert_snapshot_record(
    snapshot: &mut HashMap<AssetId, Vec<MaterializationRecord>>,
    record: MaterializationRecord,
) {
    let records = snapshot.entry(record.asset_id.clone()).or_default();
    if records
        .iter()
        .all(|existing| existing.materialization_id != record.materialization_id)
    {
        records.push(record);
    }
}

fn dependency_record<'a>(
    asset_id: &AssetId,
    records: &'a [MaterializationRecord],
    dependency: &str,
) -> Result<&'a MaterializationRecord, AssetDagExecutorError> {
    records
        .iter()
        .find(|record| record.asset_id.as_str() == dependency)
        .ok_or_else(|| AssetDagExecutorError::MissingDependency {
            asset_id: asset_id.clone(),
            dependency: static_asset_id(dependency),
        })
}

enum ExecutedAsset {
    Record(MaterializationRecord),
    SkillFacts(MaterializationRecord),
    KgSocietyView(KgSocietyViewMaterialization),
    SearchServingBundle(SearchServingBundleMaterialization),
}

impl ExecutedAsset {
    fn record(&self) -> &MaterializationRecord {
        match self {
            Self::Record(record) | Self::SkillFacts(record) => record,
            Self::KgSocietyView(materialization) => &materialization.record,
            Self::SearchServingBundle(materialization) => &materialization.record,
        }
    }
}

#[derive(Debug)]
pub enum AssetDagExecutorError {
    Planner(PlannerError),
    Manifest(RunManifestError),
    Lake(LakeError),
    FanIn(AssetFanInError),
    Partition(PartitionResolutionError),
    KgSocietyView(KgSocietyViewMaterializeError),
    GooglePlace(GooglePlaceAssetError),
    ProjectEnrichment(ProjectEnrichmentAssetError),
    Media(MediaAssetError),
    SearchServingBundle(SearchServingBundleMaterializeError),
    ServingReleaseValidation(String),
    SkillFact(SkillFactMaterializeError),
    ApproachRoadGraph(ApproachRoadGraphError),
    Environmental(EnvironmentalAssetError),
    Transit(TransitAssetError),
    OsmPower(OsmPowerAssetError),
    Stormwater(StormwaterAssetError),
    CurrentProjectFacts(CurrentProjectFactsError),
    ReraEvidence(ReraEvidenceError),
    ReraSourceRecords(ReraSourceRecordsError),
    ReraClaims(ReraClaimMaterializeError),
    ReraServingProjection(ReraServingProjectionError),
    Rera(ReraAssetError),
    ReraPlanFrames(ReraPlanFramesAssetError),
    CanonicalNodes(super::CanonicalNodesError),
    NoExecutor {
        asset_id: AssetId,
    },
    SourceInputMissing {
        asset_id: AssetId,
    },
    SourceCollectionFailed {
        asset_id: AssetId,
        reason: String,
    },
    MissingDependency {
        asset_id: AssetId,
        dependency: AssetId,
    },
    MissingRuntimeKgView,
    ParentLineageMismatch {
        asset_id: AssetId,
        expected: Vec<MaterializationId>,
        actual: Vec<MaterializationId>,
    },
    AssetPartitionMismatch {
        asset_id: AssetId,
        expected: AssetPartition,
        actual: AssetPartition,
    },
    UnsupportedPartition {
        asset_id: AssetId,
        partition: AssetPartition,
    },
    UnknownAsset {
        asset_id: AssetId,
    },
    ResumeDryRunUnsupported,
    ResumePartitionMismatch {
        run_id: MaterializationId,
        expected: AssetPartition,
        actual: AssetPartition,
    },
    ResumeMissingMaterialization {
        asset_id: AssetId,
    },
    ResumeMaterializationRunMismatch {
        asset_id: AssetId,
        expected: MaterializationId,
        actual: MaterializationId,
    },
    ResumeMissingParentMaterialization {
        asset_id: AssetId,
        parent_id: MaterializationId,
    },
    ResumeMissingKgViewManifest {
        materialization_id: MaterializationId,
    },
    ResumeKgViewContentMismatch {
        materialization_id: MaterializationId,
        expected: String,
        actual: String,
    },
    ResumeArtifactIntegrity {
        asset_id: AssetId,
        key: String,
        reason: String,
    },
    AssetExecutionTimedOut {
        asset_id: AssetId,
        timeout_ms: u64,
    },
}

impl fmt::Display for AssetDagExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planner(err) => write!(f, "asset DAG planning failed: {err}"),
            Self::Manifest(err) => write!(f, "asset DAG manifest update failed: {err}"),
            Self::Lake(err) => write!(f, "asset DAG lake operation failed: {err}"),
            Self::FanIn(err) => write!(f, "asset DAG fan-in failed: {err}"),
            Self::Partition(err) => write!(f, "asset partition resolution failed: {err}"),
            Self::KgSocietyView(err) => write!(f, "KG society view execution failed: {err}"),
            Self::ApproachRoadGraph(err) => {
                write!(f, "approach-road graph asset execution failed: {err}")
            }
            Self::Environmental(err) => write!(f, "environmental asset execution failed: {err}"),
            Self::Transit(err) => write!(f, "transit asset execution failed: {err}"),
            Self::OsmPower(err) => write!(f, "OSM power asset execution failed: {err}"),
            Self::Stormwater(err) => write!(f, "stormwater asset execution failed: {err}"),
            Self::CurrentProjectFacts(err) => {
                write!(f, "current project facts compaction failed: {err}")
            }
            Self::ReraEvidence(err) => write!(f, "RERA evidence asset execution failed: {err}"),
            Self::ReraSourceRecords(err) => {
                write!(f, "RERA source record asset execution failed: {err}")
            }
            Self::ReraClaims(err) => write!(f, "RERA claim asset execution failed: {err}"),
            Self::ReraServingProjection(err) => {
                write!(f, "RERA serving projection failed: {err}")
            }
            Self::GooglePlace(err) => write!(f, "Google place source execution failed: {err}"),
            Self::ProjectEnrichment(err) => {
                write!(f, "project enrichment execution failed: {err}")
            }
            Self::Media(err) => write!(f, "media asset execution failed: {err}"),
            Self::SearchServingBundle(err) => {
                write!(f, "search serving bundle execution failed: {err}")
            }
            Self::ServingReleaseValidation(message) => {
                write!(f, "serving release validation failed: {message}")
            }
            Self::SkillFact(err) => write!(f, "skill fact source execution failed: {err}"),
            Self::Rera(err) => write!(f, "RERA asset execution failed: {err}"),
            Self::ReraPlanFrames(err) => {
                write!(f, "RERA project plan asset execution failed: {err}")
            }
            Self::CanonicalNodes(err) => write!(f, "canonical nodes asset execution failed: {err}"),
            Self::NoExecutor { asset_id } => {
                write!(f, "no executor registered for planned asset {asset_id}")
            }
            Self::SourceInputMissing { asset_id } => {
                write!(f, "source input payload is required to execute asset {asset_id}")
            }
            Self::SourceCollectionFailed { asset_id, reason } => {
                write!(f, "source collection failed for asset {asset_id}: {reason}")
            }
            Self::MissingDependency {
                asset_id,
                dependency,
            } => write!(
                f,
                "asset {asset_id} cannot run because dependency {dependency} has no current materialization"
            ),
            Self::MissingRuntimeKgView => write!(
                f,
                "search serving bundle requires kg_society_view to run in the same DAG run"
            ),
            Self::ParentLineageMismatch {
                asset_id,
                expected,
                actual,
            } => write!(
                f,
                "asset {asset_id} returned parent lineage {actual:?}, expected {expected:?}"
            ),
            Self::AssetPartitionMismatch {
                asset_id,
                expected,
                actual,
            } => write!(
                f,
                "asset {asset_id} materialized partition {actual:?}, expected {expected:?}"
            ),
            Self::UnsupportedPartition {
                asset_id,
                partition,
            } => write!(
                f,
                "asset {asset_id} cannot execute for non-global partition {partition:?}"
            ),
            Self::UnknownAsset { asset_id } => {
                write!(f, "asset {asset_id} is not registered in the DAG")
            }
            Self::ResumeDryRunUnsupported => {
                f.write_str("resuming a DAG run is not supported in dry-run mode")
            }
            Self::ResumePartitionMismatch {
                run_id,
                expected,
                actual,
            } => write!(
                f,
                "DAG run {run_id} belongs to partition {expected:?}, not {actual:?}"
            ),
            Self::ResumeMissingMaterialization { asset_id } => write!(
                f,
                "succeeded asset {asset_id} has no materialization to restore during resume"
            ),
            Self::ResumeMaterializationRunMismatch {
                asset_id,
                expected,
                actual,
            } => write!(
                f,
                "asset {asset_id} materialization belongs to run {actual}, expected {expected}"
            ),
            Self::ResumeMissingParentMaterialization {
                asset_id,
                parent_id,
            } => write!(
                f,
                "asset {asset_id} cannot restore parent materialization {parent_id}"
            ),
            Self::ResumeMissingKgViewManifest { materialization_id } => write!(
                f,
                "KG view materialization {materialization_id} has no manifest artifact"
            ),
            Self::ResumeKgViewContentMismatch {
                materialization_id,
                expected,
                actual,
            } => write!(
                f,
                "KG view materialization {materialization_id} content hash changed during resume: expected {expected}, got {actual}"
            ),
            Self::ResumeArtifactIntegrity {
                asset_id,
                key,
                reason,
            } => write!(
                f,
                "asset {asset_id} cannot resume because artifact {key} failed integrity validation: {reason}"
            ),
            Self::AssetExecutionTimedOut {
                asset_id,
                timeout_ms,
            } => write!(
                f,
                "asset {asset_id} exceeded its execution timeout of {timeout_ms}ms"
            ),
        }
    }
}

impl std::error::Error for AssetDagExecutorError {}

impl AssetDagExecutorError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::AssetExecutionTimedOut { .. } => true,
            Self::Lake(err) => err.is_retryable(),
            Self::GooglePlace(GooglePlaceAssetError::Lake(err))
            | Self::ProjectEnrichment(ProjectEnrichmentAssetError::Lake(err))
            | Self::Media(MediaAssetError::Lake(err))
            | Self::SkillFact(SkillFactMaterializeError::Lake(err))
            | Self::ApproachRoadGraph(ApproachRoadGraphError::Lake(err))
            | Self::CurrentProjectFacts(CurrentProjectFactsError::Lake(err))
            | Self::Rera(ReraAssetError::Lake(err))
            | Self::CanonicalNodes(super::CanonicalNodesError::Lake(err))
            | Self::KgSocietyView(KgSocietyViewMaterializeError::Lake(err))
            | Self::SearchServingBundle(SearchServingBundleMaterializeError::Lake(err)) => {
                err.is_retryable()
            }
            Self::FanIn(AssetFanInError::Lake(err)) => err.is_retryable(),
            _ => false,
        }
    }
}

impl From<PlannerError> for AssetDagExecutorError {
    fn from(err: PlannerError) -> Self {
        Self::Planner(err)
    }
}

impl From<RunManifestError> for AssetDagExecutorError {
    fn from(err: RunManifestError) -> Self {
        Self::Manifest(err)
    }
}

impl From<LakeError> for AssetDagExecutorError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<AssetFanInError> for AssetDagExecutorError {
    fn from(err: AssetFanInError) -> Self {
        Self::FanIn(err)
    }
}

impl From<KgSocietyViewMaterializeError> for AssetDagExecutorError {
    fn from(err: KgSocietyViewMaterializeError) -> Self {
        Self::KgSocietyView(err)
    }
}

impl From<ApproachRoadGraphError> for AssetDagExecutorError {
    fn from(err: ApproachRoadGraphError) -> Self {
        Self::ApproachRoadGraph(err)
    }
}

impl From<EnvironmentalAssetError> for AssetDagExecutorError {
    fn from(err: EnvironmentalAssetError) -> Self {
        Self::Environmental(err)
    }
}

impl From<TransitAssetError> for AssetDagExecutorError {
    fn from(err: TransitAssetError) -> Self {
        Self::Transit(err)
    }
}

impl From<OsmPowerAssetError> for AssetDagExecutorError {
    fn from(err: OsmPowerAssetError) -> Self {
        Self::OsmPower(err)
    }
}

impl From<StormwaterAssetError> for AssetDagExecutorError {
    fn from(err: StormwaterAssetError) -> Self {
        Self::Stormwater(err)
    }
}

impl From<CurrentProjectFactsError> for AssetDagExecutorError {
    fn from(err: CurrentProjectFactsError) -> Self {
        Self::CurrentProjectFacts(err)
    }
}

impl From<GooglePlaceAssetError> for AssetDagExecutorError {
    fn from(err: GooglePlaceAssetError) -> Self {
        Self::GooglePlace(err)
    }
}

impl From<ProjectEnrichmentAssetError> for AssetDagExecutorError {
    fn from(err: ProjectEnrichmentAssetError) -> Self {
        Self::ProjectEnrichment(err)
    }
}

impl From<MediaAssetError> for AssetDagExecutorError {
    fn from(err: MediaAssetError) -> Self {
        Self::Media(err)
    }
}

impl From<SearchServingBundleMaterializeError> for AssetDagExecutorError {
    fn from(err: SearchServingBundleMaterializeError) -> Self {
        Self::SearchServingBundle(err)
    }
}

impl From<ReraServingProjectionError> for AssetDagExecutorError {
    fn from(err: ReraServingProjectionError) -> Self {
        Self::ReraServingProjection(err)
    }
}

impl From<SkillFactMaterializeError> for AssetDagExecutorError {
    fn from(err: SkillFactMaterializeError) -> Self {
        Self::SkillFact(err)
    }
}

impl From<ReraAssetError> for AssetDagExecutorError {
    fn from(err: ReraAssetError) -> Self {
        Self::Rera(err)
    }
}

impl From<ReraEvidenceError> for AssetDagExecutorError {
    fn from(err: ReraEvidenceError) -> Self {
        Self::ReraEvidence(err)
    }
}

impl From<ReraSourceRecordsError> for AssetDagExecutorError {
    fn from(err: ReraSourceRecordsError) -> Self {
        Self::ReraSourceRecords(err)
    }
}

impl From<ReraClaimMaterializeError> for AssetDagExecutorError {
    fn from(err: ReraClaimMaterializeError) -> Self {
        Self::ReraClaims(err)
    }
}

impl From<ReraPlanFramesAssetError> for AssetDagExecutorError {
    fn from(err: ReraPlanFramesAssetError) -> Self {
        Self::ReraPlanFrames(err)
    }
}

impl From<super::CanonicalNodesError> for AssetDagExecutorError {
    fn from(err: super::CanonicalNodesError) -> Self {
        Self::CanonicalNodes(err)
    }
}

fn ensure_global_partition(
    asset_id: &AssetId,
    partition: &AssetPartition,
) -> Result<(), AssetDagExecutorError> {
    if partition.is_global() {
        Ok(())
    } else {
        Err(AssetDagExecutorError::UnsupportedPartition {
            asset_id: asset_id.clone(),
            partition: partition.clone(),
        })
    }
}

fn source_input_error(context: &AssetExecutionContext<'_>) -> AssetDagExecutorError {
    match context
        .options
        .source_inputs
        .source_failures
        .get(context.asset_id.as_str())
    {
        Some(reason) => AssetDagExecutorError::SourceCollectionFailed {
            asset_id: context.asset_id.clone(),
            reason: reason.clone(),
        },
        None => AssetDagExecutorError::SourceInputMissing {
            asset_id: context.asset_id.clone(),
        },
    }
}

fn should_skip_missing_optional_source_input(
    asset_id: &AssetId,
    source_inputs: &AssetSourceInputs,
) -> bool {
    if source_inputs
        .source_failures
        .contains_key(asset_id.as_str())
    {
        return false;
    }
    match asset_id.as_str() {
        BENGALURU_METRO_STATION_FACTS_ASSET_ID => source_inputs.bengaluru_metro_stations.is_none(),
        _ => false,
    }
}

fn should_skip_missing_source_input(asset_id: &AssetId, source_inputs: &AssetSourceInputs) -> bool {
    if source_inputs
        .source_failures
        .contains_key(asset_id.as_str())
    {
        return false;
    }
    match asset_id.as_str() {
        RERA_RECEIPTS_ASSET_ID => source_inputs.rera_receipts.is_none(),
        RERA_SOURCE_RECORDS_ASSET_ID => source_inputs.rera_source_records.is_none(),
        RERA_REGISTRY_MONTHLY_ASSET_ID => source_inputs.rera_registry_monthly.is_none(),
        GOOGLE_PLACES_WEEKLY_ASSET_ID => source_inputs.google_places_weekly.is_none(),
        GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID => source_inputs.google_nearby_places_weekly.is_none(),
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID => source_inputs.external_listings_weekly.is_none(),
        EXTERNAL_IMAGES_WEEKLY_ASSET_ID => source_inputs.external_images_weekly.is_none(),
        BENGALURU_METRO_STATION_FACTS_ASSET_ID => source_inputs.bengaluru_metro_stations.is_none(),
        _ => false,
    }
}

fn blocked_dependencies(
    manifest: &AssetDagRunManifest,
    definition: &super::AssetDefinition,
) -> Vec<AssetId> {
    definition
        .dependencies
        .iter()
        .filter(|dependency| !definition.is_optional_dependency(dependency))
        .filter(|dependency| {
            manifest.steps.iter().any(|step| {
                &step.asset_id == *dependency
                    && matches!(
                        step.status,
                        super::AssetRunStepStatus::Failed | super::AssetRunStepStatus::Blocked
                    )
            })
        })
        .cloned()
        .collect()
}

fn retry_delay(policy: &AssetRetryPolicy, failed_attempt: u32) -> Duration {
    let exponent = failed_attempt.saturating_sub(1).min(31);
    let multiplier = 1_u64 << exponent;
    Duration::from_millis(
        policy
            .initial_delay_ms
            .saturating_mul(multiplier)
            .min(policy.max_delay_ms),
    )
}

fn knowledge_graph_watermark(graph: &KnowledgeGraph) -> SourceWatermark {
    let stats = graph.stats();
    SourceWatermark {
        source: "knowledge_graph".to_string(),
        high_watermark: format!(
            "nodes={} edges={} facts={}",
            stats.total_nodes, stats.total_edges, stats.total_facts
        ),
    }
}

fn static_asset_id(id: &str) -> AssetId {
    AssetId::new(id).expect("static asset id is valid")
}

fn default_asset_version(planned_at: DateTime<Utc>) -> String {
    planned_at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn default_asset_execution_timeout_ms() -> u64 {
    DEFAULT_ASSET_EXECUTION_TIMEOUT_MS
}

fn is_default_source_inputs(source_inputs: &AssetSourceInputs) -> bool {
    source_inputs.source_entities.is_empty()
        && source_inputs.source_failures.is_empty()
        && source_inputs.rera_registry_monthly.is_none()
        && source_inputs.reddit_threads_daily.is_none()
        && source_inputs.reddit_resident_facts.is_none()
        && source_inputs.google_places_weekly.is_none()
        && source_inputs.google_nearby_places_weekly.is_none()
        && source_inputs.external_listings_weekly.is_none()
        && source_inputs.external_images_weekly.is_none()
        && source_inputs.environment_groundwater_potential.is_none()
        && source_inputs.bengaluru_metro_stations.is_none()
        && source_inputs.osm_power_infrastructure.is_none()
        && source_inputs.stormwater_drains.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetRegistry, AssetStage, CostTier, RefreshCadence, TrustTier};
    use chrono::TimeZone;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn transient_asset_failure_retries_and_records_both_attempts() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let asset_id = AssetId::new("transient_root").unwrap();
        let registry = AssetRegistry::new(vec![AssetDefinition::new(
            asset_id.clone(),
            AssetStage::Raw,
            "transient test root",
            Vec::new(),
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Root,
        )])
        .unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let mut executors = HashMap::new();
        executors.insert(
            asset_id.clone(),
            BuiltInAssetExecutor::TestFailOnce(attempts.clone()),
        );
        let executor = AssetDagExecutor {
            materializations: AssetMaterializationStore::new(lake.clone()),
            run_manifests: AssetRunManifestStore::new(lake.clone()),
            lake,
            registry,
            executors: BuiltInAssetExecutorRegistry { executors },
            project_root: root.path().to_path_buf(),
            sync_frontend_manifest: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 6, 0, 0).unwrap();

        let report = executor
            .execute(
                &KnowledgeGraph::new(),
                AssetDagExecutionOptions::new(AssetPartition::global(), now).with_retry_policy(
                    AssetRetryPolicy {
                        max_attempts: 2,
                        initial_delay_ms: 0,
                        max_delay_ms: 0,
                    },
                ),
            )
            .await
            .unwrap();

        let step = report
            .manifest
            .steps
            .iter()
            .find(|step| step.asset_id == asset_id)
            .unwrap();
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(step.status, super::super::AssetRunStepStatus::Succeeded);
        assert_eq!(step.attempts.len(), 2);
        assert!(step.attempts[0].error.is_some());
        assert!(step.attempts[1].error.is_none());
    }

    #[tokio::test]
    async fn skipped_current_promotion_still_completes_the_run_step() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let asset_id = AssetId::new("scoped_root").unwrap();
        let registry = AssetRegistry::new(vec![AssetDefinition::new(
            asset_id.clone(),
            AssetStage::Raw,
            "scoped test root",
            Vec::new(),
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Root,
        )])
        .unwrap();
        let attempts = Arc::new(AtomicUsize::new(1));
        let mut executors = HashMap::new();
        executors.insert(
            asset_id.clone(),
            BuiltInAssetExecutor::TestFailOnce(attempts.clone()),
        );
        let executor = AssetDagExecutor {
            materializations: AssetMaterializationStore::new(lake.clone()),
            run_manifests: AssetRunManifestStore::new(lake.clone()),
            lake: lake.clone(),
            registry,
            executors: BuiltInAssetExecutorRegistry { executors },
            project_root: root.path().to_path_buf(),
            sync_frontend_manifest: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 7, 26, 8, 0, 0).unwrap();

        let report = executor
            .execute(
                &KnowledgeGraph::new(),
                AssetDagExecutionOptions::new(AssetPartition::global(), now)
                    .with_promote_current(false),
            )
            .await
            .unwrap();

        let step = report
            .manifest
            .steps
            .iter()
            .find(|step| step.asset_id == asset_id)
            .unwrap();
        assert_eq!(step.status, super::super::AssetRunStepStatus::Succeeded);
        assert!(!report.manifest.promote_current);
        assert!(report
            .manifest
            .execution_version
            .ends_with(&report.manifest.run_id.to_string()));
        assert!(report.current_pointer_key.is_none());
        assert!(AssetMaterializationStore::new(lake)
            .current_record(&asset_id, &AssetPartition::global())
            .await
            .is_err());
    }

    #[test]
    fn required_red_flag_source_inputs_do_not_skip_when_missing() {
        for asset_id in [
            OSM_POWER_LINE_FACTS_ASSET_ID,
            STORMWATER_DRAIN_FACTS_ASSET_ID,
        ] {
            let asset_id = AssetId::new(asset_id).unwrap();

            assert!(!should_skip_missing_optional_source_input(
                &asset_id,
                &AssetSourceInputs::default()
            ));

            let mut source_inputs = AssetSourceInputs::default();
            source_inputs
                .source_failures
                .insert(asset_id.to_string(), "collector failed".to_string());
            assert!(!should_skip_missing_optional_source_input(
                &asset_id,
                &source_inputs
            ));
        }
    }

    #[tokio::test]
    async fn asset_timeout_stays_inside_the_resume_lease_window() {
        assert!(
            DEFAULT_ASSET_EXECUTION_TIMEOUT_MS
                < super::super::DEFAULT_RESUME_LEASE_SECONDS as u64 * 1_000
        );
        let capped = AssetDagExecutionOptions::new(AssetPartition::global(), Utc::now())
            .with_asset_execution_timeout(Duration::from_secs(2 * 60 * 60));
        assert_eq!(
            capped.asset_execution_timeout_ms(),
            DEFAULT_ASSET_EXECUTION_TIMEOUT_MS
        );

        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let asset_id = AssetId::new("slow_root").unwrap();
        let registry = AssetRegistry::new(vec![AssetDefinition::new(
            asset_id.clone(),
            AssetStage::Raw,
            "slow test root",
            Vec::new(),
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Root,
        )])
        .unwrap();
        let mut executors = HashMap::new();
        executors.insert(
            asset_id.clone(),
            BuiltInAssetExecutor::TestSleep(Duration::from_millis(20)),
        );
        let executor = AssetDagExecutor {
            materializations: AssetMaterializationStore::new(lake.clone()),
            run_manifests: AssetRunManifestStore::new(lake.clone()),
            lake,
            registry,
            executors: BuiltInAssetExecutorRegistry { executors },
            project_root: root.path().to_path_buf(),
            sync_frontend_manifest: false,
        };
        let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();

        let error = executor
            .execute(
                &KnowledgeGraph::new(),
                AssetDagExecutionOptions::new(AssetPartition::global(), now)
                    .with_asset_execution_timeout(Duration::from_millis(1))
                    .with_retry_policy(AssetRetryPolicy {
                        max_attempts: 1,
                        initial_delay_ms: 0,
                        max_delay_ms: 0,
                    }),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AssetDagExecutorError::AssetExecutionTimedOut {
                asset_id: timed_out,
                timeout_ms: 1
            } if timed_out == asset_id
        ));
    }

    #[test]
    fn support_fact_records_follow_registry_fan_in_policy() {
        let custom_support_asset = AssetId::new("custom_support_facts").unwrap();
        let resolved_asset = AssetId::new("resolved_dependency").unwrap();
        let definition = AssetDefinition::new(
            AssetId::new("kg_society_view").unwrap(),
            AssetStage::Gold,
            "test KG",
            vec![custom_support_asset.clone(), resolved_asset.clone()],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        )
        .with_dependency_fan_in_policy(
            "custom_support_facts",
            DependencyFanInPolicy::AllCurrentPartitions,
        );
        let custom_record = MaterializationRecord::succeeded(
            custom_support_asset.clone(),
            AssetStage::Silver,
            AssetPartition::new([("dt", "2026-07-13"), ("source", "custom")]),
            "2026-07-13",
            Vec::new(),
        );
        let resolved_record = MaterializationRecord::succeeded(
            resolved_asset,
            AssetStage::Silver,
            AssetPartition::global(),
            "2026-07-13",
            Vec::new(),
        );

        let support_records = support_fact_records(
            &definition,
            &[custom_record.clone(), resolved_record.clone()],
        );

        assert_eq!(support_records, vec![custom_record, resolved_record]);
    }
}
