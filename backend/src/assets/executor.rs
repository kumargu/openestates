use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::knowledge::KnowledgeGraph;
use crate::lake::{LakeError, LakeStore};
use crate::serving::{
    SearchServingBundleMaterialization, SearchServingBundleMaterializeError,
    SearchServingBundleMaterializer, SEARCH_SERVING_BUNDLE_ASSET_ID,
};

use super::{
    all_current_materialization_records_for_dependency, read_skill_fact_artifact_rows,
    sort_materialization_records, AssetDagPlan, AssetDagRunManifest, AssetDefinition,
    AssetFanInError, AssetId, AssetMaterializationStore, AssetPartition, AssetPlanner,
    AssetRunManifestStore, AssetSourceInputs, DependencyFanInPolicy, GooglePlaceAssetError,
    GooglePlaceSnapshotMaterializer, KgSocietyViewMaterialization, KgSocietyViewMaterializeError,
    KgSocietyViewMaterializer, MaterializationId, MaterializationRecord, PartitionResolutionError,
    PlanDecision, PlannerError, RedditThreadSnapshotMaterializeError,
    RedditThreadSnapshotMaterializer, RedditThreadsDailyInput, ReraAssetError,
    ReraRegistryMaterializer, RunManifestError, SkillFactMaterializeError, SkillFactMaterializer,
    SkillFactsInput, SourceWatermark, CANONICAL_SOCIETY_NODES_ASSET_ID,
    GOOGLE_PLACES_WEEKLY_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID,
    REDDIT_RESIDENT_FACTS_ASSET_ID, REDDIT_THREADS_DAILY_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID,
    RERA_REGISTRY_MONTHLY_ASSET_ID,
};

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
        }
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
        let mut manifest = AssetDagRunManifest::from_plan(&plan);

        if options.dry_run {
            return Ok(AssetDagExecutionReport {
                dry_run: true,
                manifest,
                run_manifest_key: None,
                current_pointer_key: None,
                executed_assets: Vec::new(),
            });
        }

        self.persist_manifest(&manifest, false).await?;
        let mut executed_assets = Vec::new();
        let mut records_by_asset = HashMap::new();
        let mut kg_view: Option<KgSocietyViewMaterialization> = None;

        for entry in plan.entries.iter() {
            if entry.decision == PlanDecision::Skip {
                continue;
            }

            let asset_id = entry.asset_id.clone();
            let asset_partition = entry.partition.clone();
            let started_at = Utc::now();
            manifest.mark_step_running(&asset_id, started_at)?;
            self.persist_manifest(&manifest, false).await?;

            match self
                .execute_asset(AssetExecutionContext {
                    dag: self,
                    graph,
                    options: &options,
                    run_id: &manifest.run_id,
                    asset_id: &asset_id,
                    asset_partition: &asset_partition,
                    records_by_asset: &records_by_asset,
                    kg_view: kg_view.as_ref(),
                })
                .await
            {
                Ok(executed) => {
                    let completed_at = Utc::now();
                    let record = executed.record().clone();
                    let success_result = self
                        .prepare_successful_step(
                            &manifest,
                            asset_id.clone(),
                            &record,
                            &records_by_asset,
                            &options.partition,
                            StepTiming {
                                started_at,
                                completed_at,
                            },
                        )
                        .await;
                    let next_manifest = match success_result {
                        Ok(next_manifest) => next_manifest,
                        Err(err) => {
                            manifest.mark_step_failed(
                                &asset_id,
                                started_at,
                                completed_at,
                                err.to_string(),
                            )?;
                            manifest.finish(completed_at)?;
                            self.persist_manifest(&manifest, true).await?;
                            return Err(err);
                        }
                    };

                    manifest = next_manifest;
                    self.persist_manifest(&manifest, false).await?;
                    records_by_asset.insert(asset_id.clone(), record);
                    if let ExecutedAsset::KgSocietyView(materialization) = executed {
                        kg_view = Some(materialization);
                    }
                    executed_assets.push(asset_id);
                }
                Err(err) => {
                    let completed_at = Utc::now();
                    manifest.mark_step_failed(
                        &asset_id,
                        started_at,
                        completed_at,
                        err.to_string(),
                    )?;
                    manifest.finish(completed_at)?;
                    self.persist_manifest(&manifest, true).await?;
                    return Err(err);
                }
            }
        }

        let completed_at = Utc::now();
        manifest.finish(completed_at)?;
        let persisted = self.persist_manifest(&manifest, true).await?;

        Ok(AssetDagExecutionReport {
            dry_run: false,
            manifest,
            run_manifest_key: Some(persisted.run_manifest_key),
            current_pointer_key: Some(persisted.current_pointer_key),
            executed_assets,
        })
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
    ) -> Result<Vec<MaterializationId>, AssetDagExecutorError> {
        Ok(self
            .dependency_materialization_records(asset_id, run_partition, records_by_asset)
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
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let definition =
            self.registry
                .get(asset_id)
                .ok_or_else(|| AssetDagExecutorError::UnknownAsset {
                    asset_id: asset_id.clone(),
                })?;
        let mut parents = Vec::new();

        for dependency in &definition.dependencies {
            let dependency_records = self
                .dependency_records(
                    definition,
                    asset_id,
                    dependency,
                    run_partition,
                    records_by_asset,
                )
                .await?;
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
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let records = match definition.dependency_fan_in_policy(dependency) {
            DependencyFanInPolicy::ResolvedPartition => {
                vec![
                    self.resolved_dependency_record(
                        asset_id,
                        dependency,
                        run_partition,
                        records_by_asset,
                    )
                    .await?,
                ]
            }
            DependencyFanInPolicy::AllCurrentPartitions => {
                self.all_current_dependency_records(dependency, records_by_asset)
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
    ) -> Result<MaterializationRecord, AssetDagExecutorError> {
        if let Some(record) = records_by_asset.get(dependency) {
            return Ok(record.clone());
        }

        let dependency_partition = self.asset_partition(dependency, run_partition)?;
        self.materializations
            .current_record(dependency, &dependency_partition)
            .await
            .map_err(|err| {
                if err.is_not_found() {
                    AssetDagExecutorError::MissingDependency {
                        asset_id: asset_id.clone(),
                        dependency: dependency.clone(),
                    }
                } else {
                    AssetDagExecutorError::Lake(err)
                }
            })
    }

    async fn all_current_dependency_records(
        &self,
        dependency: &AssetId,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
    ) -> Result<Vec<MaterializationRecord>, AssetDagExecutorError> {
        let mut records = all_current_materialization_records_for_dependency(
            &self.registry,
            &self.materializations,
            dependency,
        )
        .await?;

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

    async fn prepare_successful_step(
        &self,
        manifest: &AssetDagRunManifest,
        asset_id: AssetId,
        record: &MaterializationRecord,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
        run_partition: &AssetPartition,
        timing: StepTiming,
    ) -> Result<AssetDagRunManifest, AssetDagExecutorError> {
        self.validate_record(asset_id.clone(), record, records_by_asset, run_partition)
            .await?;

        let mut next_manifest = manifest.clone();
        next_manifest.mark_step_succeeded(
            &asset_id,
            record,
            timing.started_at,
            timing.completed_at,
        )?;
        self.materializations.promote_current(record).await?;
        Ok(next_manifest)
    }

    async fn validate_record(
        &self,
        asset_id: AssetId,
        record: &MaterializationRecord,
        records_by_asset: &HashMap<AssetId, MaterializationRecord>,
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
            .dependency_materializations(&asset_id, run_partition, records_by_asset)
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
        manifest: &AssetDagRunManifest,
        promote_current: bool,
    ) -> Result<PersistedManifest, AssetDagExecutorError> {
        let meta = self.run_manifests.write_manifest(manifest).await?;
        if promote_current {
            self.run_manifests.promote_current(manifest).await?;
        }
        Ok(PersistedManifest {
            run_manifest_key: meta.key.to_string(),
            current_pointer_key: super::AssetPathBuilder::current_dag_run_pointer_key(
                &manifest.partition,
            )
            .to_string(),
        })
    }
}

#[derive(Debug)]
struct PersistedManifest {
    run_manifest_key: String,
    current_pointer_key: String,
}

#[derive(Debug, Clone, Copy)]
struct StepTiming {
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

#[derive(Clone)]
struct BuiltInAssetExecutorRegistry {
    executors: HashMap<AssetId, BuiltInAssetExecutor>,
}

impl BuiltInAssetExecutorRegistry {
    fn default_openestates() -> Self {
        let mut executors = HashMap::new();
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
            static_asset_id(REDDIT_THREADS_DAILY_ASSET_ID),
            BuiltInAssetExecutor::RedditThreadsDaily,
        );
        executors.insert(
            static_asset_id(REDDIT_RESIDENT_FACTS_ASSET_ID),
            BuiltInAssetExecutor::RedditResidentFacts,
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

#[derive(Clone, Copy)]
enum BuiltInAssetExecutor {
    ReraRegistryMonthly,
    CanonicalSocietyNodes,
    ReraLegalFacts,
    RedditThreadsDaily,
    RedditResidentFacts,
    GooglePlacesWeekly,
    GoogleReviewFacts,
    KgSocietyView,
    SearchServingBundle,
}

impl BuiltInAssetExecutor {
    async fn execute(
        &self,
        context: AssetExecutionContext<'_>,
    ) -> Result<ExecutedAsset, AssetDagExecutorError> {
        match self {
            Self::ReraRegistryMonthly => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let input = context
                    .options
                    .source_inputs
                    .rera_registry_monthly
                    .as_ref()
                    .ok_or(AssetDagExecutorError::SourceInputMissing {
                        asset_id: context.asset_id.clone(),
                    })?;
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
            Self::RedditThreadsDaily => {
                let input = context
                    .options
                    .source_inputs
                    .reddit_threads_daily
                    .as_ref()
                    .ok_or(AssetDagExecutorError::SourceInputMissing {
                        asset_id: context.asset_id.clone(),
                    })?;
                let parent_materializations = context
                    .dag
                    .dependency_materializations(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
                    )
                    .await?;
                let materialization =
                    RedditThreadSnapshotMaterializer::new(context.dag.lake.clone())
                        .materialize_for_run(
                            input.snapshot_date.clone(),
                            input.subreddit.clone(),
                            context.run_id.to_string(),
                            &input.records,
                            parent_materializations,
                            reddit_thread_watermarks(input),
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?;
                Ok(ExecutedAsset::RedditThreadsDaily(materialization.record))
            }
            Self::RedditResidentFacts => {
                let input = context
                    .options
                    .source_inputs
                    .reddit_resident_facts
                    .as_ref()
                    .ok_or(AssetDagExecutorError::SourceInputMissing {
                        asset_id: context.asset_id.clone(),
                    })?;
                let materialization = execute_skill_fact_asset(context, input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::GooglePlacesWeekly => {
                let input = context
                    .options
                    .source_inputs
                    .google_places_weekly
                    .as_ref()
                    .ok_or(AssetDagExecutorError::SourceInputMissing {
                        asset_id: context.asset_id.clone(),
                    })?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
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
                    )
                    .await?;
                let google_record = dependency_record(
                    context.asset_id,
                    &parent_records,
                    GOOGLE_PLACES_WEEKLY_ASSET_ID,
                )?;
                let input = super::google_review_facts_input(
                    &context.dag.lake,
                    google_record,
                    context.run_id,
                )
                .await?;
                let materialization = execute_skill_fact_asset(context, &input).await?;
                Ok(ExecutedAsset::SkillFacts(materialization))
            }
            Self::KgSocietyView => {
                ensure_global_partition(context.asset_id, context.asset_partition)?;
                let parent_records = context
                    .dag
                    .dependency_materialization_records(
                        context.asset_id,
                        &context.options.partition,
                        context.records_by_asset,
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
                let canonical_rows =
                    super::read_canonical_society_rows(&context.dag.lake, canonical_record).await?;
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
                let materialization =
                    SearchServingBundleMaterializer::new(context.dag.lake.clone())
                        .materialize_from_kg_view_for_run(
                            kg_view,
                            context.options.version.clone(),
                            context.run_id.clone(),
                            context.asset_partition.clone(),
                        )
                        .await?;
                Ok(ExecutedAsset::SearchServingBundle(materialization))
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
        )
        .await?;
    let materialization = SkillFactMaterializer::new(context.dag.lake.clone())
        .materialize_for_run(
            context.asset_id.as_str(),
            input.source.clone(),
            input.snapshot_date.clone(),
            context.run_id.to_string(),
            &input.facts,
            &input.fact_annotations,
            parent_materializations,
            skill_fact_watermarks(input),
            context.run_id.clone(),
            context.asset_partition.clone(),
        )
        .await?;
    Ok(materialization.record)
}

fn reddit_thread_watermarks(input: &RedditThreadsDailyInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }

    let high_watermark = input
        .records
        .iter()
        .map(|record| record.fetched_at)
        .max()
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| input.snapshot_date.clone());
    vec![SourceWatermark {
        source: format!("reddit:{}", input.subreddit),
        high_watermark,
    }]
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
            record.asset_id.as_str() == RERA_LEGAL_FACTS_ASSET_ID
                || definition.dependency_fan_in_policy(&record.asset_id)
                    == DependencyFanInPolicy::AllCurrentPartitions
        })
        .cloned()
        .collect()
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
    RedditThreadsDaily(MaterializationRecord),
    SkillFacts(MaterializationRecord),
    KgSocietyView(KgSocietyViewMaterialization),
    SearchServingBundle(SearchServingBundleMaterialization),
}

impl ExecutedAsset {
    fn record(&self) -> &MaterializationRecord {
        match self {
            Self::Record(record) | Self::RedditThreadsDaily(record) | Self::SkillFacts(record) => {
                record
            }
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
    RedditThreadSnapshot(RedditThreadSnapshotMaterializeError),
    GooglePlace(GooglePlaceAssetError),
    SearchServingBundle(SearchServingBundleMaterializeError),
    SkillFact(SkillFactMaterializeError),
    Rera(ReraAssetError),
    NoExecutor {
        asset_id: AssetId,
    },
    SourceInputMissing {
        asset_id: AssetId,
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
            Self::RedditThreadSnapshot(err) => {
                write!(f, "reddit thread source execution failed: {err}")
            }
            Self::GooglePlace(err) => write!(f, "Google place source execution failed: {err}"),
            Self::SearchServingBundle(err) => {
                write!(f, "search serving bundle execution failed: {err}")
            }
            Self::SkillFact(err) => write!(f, "skill fact source execution failed: {err}"),
            Self::Rera(err) => write!(f, "RERA asset execution failed: {err}"),
            Self::NoExecutor { asset_id } => {
                write!(f, "no executor registered for planned asset {asset_id}")
            }
            Self::SourceInputMissing { asset_id } => {
                write!(f, "source input payload is required to execute asset {asset_id}")
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
        }
    }
}

impl std::error::Error for AssetDagExecutorError {}

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

impl From<RedditThreadSnapshotMaterializeError> for AssetDagExecutorError {
    fn from(err: RedditThreadSnapshotMaterializeError) -> Self {
        Self::RedditThreadSnapshot(err)
    }
}

impl From<GooglePlaceAssetError> for AssetDagExecutorError {
    fn from(err: GooglePlaceAssetError) -> Self {
        Self::GooglePlace(err)
    }
}

impl From<SearchServingBundleMaterializeError> for AssetDagExecutorError {
    fn from(err: SearchServingBundleMaterializeError) -> Self {
        Self::SearchServingBundle(err)
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

fn is_default_source_inputs(source_inputs: &AssetSourceInputs) -> bool {
    source_inputs.rera_registry_monthly.is_none()
        && source_inputs.reddit_threads_daily.is_none()
        && source_inputs.reddit_resident_facts.is_none()
        && source_inputs.google_places_weekly.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetStage, CostTier, RefreshCadence, TrustTier};

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

        let support_records =
            support_fact_records(&definition, &[custom_record.clone(), resolved_record]);

        assert_eq!(support_records, vec![custom_record]);
    }
}
