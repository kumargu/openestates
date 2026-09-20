//! Persisted catalog transactions. Modules select assets; DAG records own completion.
use super::*;
use crate::assets::{ArtifactRef, MaterializationRecord};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogApplyRequest {
    pub operation_id: String,
    #[serde(default)]
    pub upserts: Vec<CatalogUpsert>,
    #[serde(default)]
    pub removals: Vec<String>,
    #[serde(default)]
    pub shared_refresh_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogUpsert {
    pub seed: SourceEntitySeed,
    #[serde(default)]
    pub refresh_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogApplyReport {
    #[serde(flatten)]
    pub publication: CatalogOperationReport,
    pub updated: Vec<String>,
    pub retained: Vec<String>,
    pub omitted: Vec<String>,
    pub removed: Vec<String>,
    pub collected_assets: Vec<String>,
    pub reused_assets: Vec<String>,
    pub failed_assets: BTreeMap<String, String>,
    pub collection_ms: u64,
    pub assembly_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrichmentConfig {
    version: u32,
    initial_modules: Vec<String>,
    required_collectors: Vec<String>,
    collector_options: serde_json::Value,
    origin_collector: String,
    origin_dependents: Vec<String>,
    shared_fact_producers: BTreeMap<String, Vec<String>>,
    pub cleanup_enabled: bool,
    modules: BTreeMap<String, Module>,
    collectors: BTreeMap<String, Collector>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Module {
    scope: String,
    refresh: Vec<String>,
    ensure: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collector {
    input_field: String,
    scope: String,
    requires: Vec<String>,
    context: Vec<String>,
}

pub(super) fn enrichment_config() -> Result<EnrichmentConfig, CatalogError> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/config/dag/catalog_enrichment.json");
    let config: EnrichmentConfig =
        serde_json::from_slice(&std::fs::read(path).map_err(LakeError::Io)?)?;
    let supported = AssetSourceInputs::supported_asset_ids();
    if config.version != 1 {
        return Err(invalid("unsupported catalog enrichment version"));
    }
    for (asset, collector) in &config.collectors {
        if !supported.iter().any(|id| id.as_str() == asset)
            || !["society", "shared"].contains(&collector.scope.as_str())
        {
            return Err(invalid(format!("invalid catalog collector {asset}")));
        }
        for dependency in collector.requires.iter().chain(&collector.context) {
            if !config.collectors.contains_key(dependency) {
                return Err(invalid(format!(
                    "unknown collector dependency {dependency}"
                )));
            }
        }
    }
    for module in config.modules.values() {
        if !["society", "shared"].contains(&module.scope.as_str()) {
            return Err(invalid(format!(
                "invalid catalog module scope {}",
                module.scope
            )));
        }
        for asset in module.refresh.iter().chain(&module.ensure) {
            if !config.collectors.contains_key(asset) {
                return Err(invalid(format!("unknown collector {asset}")));
            }
        }
        if module
            .refresh
            .iter()
            .any(|asset| config.collectors[asset].scope != module.scope)
        {
            return Err(invalid(format!(
                "module refresh scope does not match {}",
                module.scope
            )));
        }
    }
    Ok(config)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogInputLineage {
    pub owner: String,
    pub producer_hash: String,
    pub dependencies: BTreeMap<String, ArtifactRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Collection {
    input: Option<ArtifactRef>,
    error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SocietyWork {
    seed: SourceEntitySeed,
    inputs: BTreeMap<String, ArtifactRef>,
    #[serde(default)]
    input_lineage: BTreeMap<String, CatalogInputLineage>,
    pins: Vec<MaterializationRecord>,
    run_id: MaterializationId,
    snapshot: Option<CatalogRosterEntry>,
    error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Operation {
    revision: u64,
    lease_owner: Option<MaterializationId>,
    lease_expires_at: Option<DateTime<Utc>>,
    request: CatalogApplyRequest,
    base: Option<CatalogPointer>,
    roster: CatalogRoster,
    producer_hash: String,
    collections: BTreeMap<String, Collection>,
    societies: BTreeMap<String, SocietyWork>,
    shared_inputs: BTreeMap<String, ArtifactRef>,
    #[serde(default)]
    shared_materializations: BTreeMap<String, MaterializationRecord>,
    #[serde(default)]
    shared_runs: BTreeMap<String, MaterializationId>,
    candidate: Option<CatalogRoster>,
    generation: Option<CatalogGeneration>,
    report: Option<CatalogApplyReport>,
    collection_ms: u64,
    completed: bool,
}

impl CatalogStore {
    pub async fn apply(
        &self,
        request: CatalogApplyRequest,
    ) -> Result<CatalogApplyReport, CatalogError> {
        let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("project root");
        let provider = CommandSourceInputProvider::new(catalog_source_python(
            std::env::var("OPENESTATES_SOURCE_PYTHON").ok(),
        ))
        .with_args([
            OsString::from("-m"),
            OsString::from("pipeline.collect_asset_sources"),
        ])
        .with_max_stdout_bytes(CATALOG_MAX_SOURCE_STDOUT_BYTES);
        self.apply_with_provider(request, project_root, &provider)
            .await
    }

    pub async fn apply_with_provider(
        &self,
        request: CatalogApplyRequest,
        project_root: &Path,
        provider: &dyn SourceInputProvider,
    ) -> Result<CatalogApplyReport, CatalogError> {
        let owner = MaterializationId::new();
        let key = operation_key(&request.operation_id)?;
        let result = self
            .apply_operation(request, project_root, provider, &owner)
            .await;
        if let Ok(mut operation) = self.lake.get_json::<Operation>(&key).await {
            if operation.lease_owner.as_ref() == Some(&owner) {
                let revision = operation.revision;
                operation.lease_owner = None;
                operation.lease_expires_at = None;
                operation.revision += 1;
                self.lake
                    .put_json_if(&key, &operation, |current: Option<&Operation>| {
                        current.is_some_and(|current| {
                            current.revision == revision
                                && current.lease_owner.as_ref() == Some(&owner)
                        })
                    })
                    .await?;
            }
        }
        result
    }

    async fn apply_operation(
        &self,
        request: CatalogApplyRequest,
        project_root: &Path,
        provider: &dyn SourceInputProvider,
        owner: &MaterializationId,
    ) -> Result<CatalogApplyReport, CatalogError> {
        let config = enrichment_config()?;
        let key = operation_key(&request.operation_id)?;
        let producer_hash = producer_hash(project_root)?;
        let mut operation: Operation = match self.lake.get_json(&key).await {
            Ok(operation) => operation,
            Err(error) if error.is_not_found() => {
                let base = self.pointer().await?;
                let roster = match &base {
                    Some(base) => self.read_roster(&base.current).await?,
                    None => CatalogRoster::empty(),
                };
                validate_request(&request, &roster, &config)?;
                let operation = Operation {
                    revision: 0,
                    lease_owner: None,
                    lease_expires_at: None,
                    request: request.clone(),
                    base,
                    roster,
                    producer_hash: producer_hash.clone(),
                    collections: BTreeMap::new(),
                    societies: BTreeMap::new(),
                    shared_inputs: BTreeMap::new(),
                    shared_materializations: BTreeMap::new(),
                    shared_runs: BTreeMap::new(),
                    candidate: None,
                    generation: None,
                    report: None,
                    collection_ms: 0,
                    completed: false,
                };
                self.lake
                    .put_json_if(&key, &operation, |current: Option<&Operation>| {
                        current.is_none()
                    })
                    .await?;
                self.lake.get_json(&key).await?
            }
            Err(error) => return Err(error.into()),
        };
        if operation.request != request {
            return Err(invalid("operation ID already names a different request"));
        }
        if operation.completed {
            return operation
                .report
                .ok_or_else(|| invalid("completed operation has no report"));
        }
        let now = Utc::now();
        if operation.lease_owner.is_some()
            && operation
                .lease_expires_at
                .is_some_and(|expires| expires > now)
        {
            return Err(invalid("operation is already running"));
        }
        let revision = operation.revision;
        operation.lease_owner = Some(owner.clone());
        operation.lease_expires_at = Some(now + chrono::Duration::hours(1));
        operation.revision += 1;
        if !self
            .lake
            .put_json_if(&key, &operation, |current: Option<&Operation>| {
                current.is_some_and(|current| current.revision == revision)
            })
            .await?
        {
            return Err(invalid("operation was claimed concurrently"));
        }
        // Recover a crash between pointer CAS and marking the operation complete.
        let pointer = self.pointer().await?;
        if let Some(generation) = &operation.generation {
            if pointer.as_ref().is_some_and(|p| {
                &p.current == generation || p.previous.as_ref() == Some(generation)
            }) {
                operation.completed = true;
                self.save_operation(&key, &mut operation).await?;
                return operation
                    .report
                    .ok_or_else(|| invalid("published operation has no report"));
            }
        }
        if pointer != operation.base {
            return Err(CatalogError::CasConflict);
        }
        if operation.producer_hash != producer_hash {
            return Err(invalid(
                "unfinished operation producer/config changed; use its original code to resume",
            ));
        }

        if operation.candidate.is_none() {
            let started = Instant::now();
            // Shared collection is independent of society collection. Collect
            // explicit shared refreshes and missing shared prerequisites once.
            let mut wanted_shared = BTreeSet::new();
            let mut refresh_shared = BTreeSet::new();
            for module in &request.shared_refresh_modules {
                wanted_shared.extend(config.modules[module].refresh.iter().cloned());
                refresh_shared.extend(config.modules[module].refresh.iter().cloned());
            }
            let active_shared = operation
                .roster
                .shared_assets
                .iter()
                .map(|record| record.asset_id.to_string())
                .collect::<BTreeSet<_>>();
            for change in &request.upserts {
                let prior = operation.roster.societies.iter().find(|entry| {
                    !seed_identities(&entry.seed).is_disjoint(&seed_identities(&change.seed))
                });
                let modules = if prior.is_none() && change.refresh_modules.is_empty() {
                    &config.initial_modules
                } else {
                    &change.refresh_modules
                };
                for module in modules {
                    for asset in config.modules[module]
                        .refresh
                        .iter()
                        .chain(&config.modules[module].ensure)
                    {
                        if config.collectors[asset].scope == "shared"
                            && !active_shared.contains(asset)
                        {
                            wanted_shared.insert(asset.clone());
                        }
                    }
                }
            }
            for asset in wanted_shared {
                if operation.shared_materializations.contains_key(&asset) {
                    continue;
                }
                let collector = &config.collectors[&asset];
                let collection_id = format!("shared/{asset}");
                if !operation.collections.contains_key(&collection_id) {
                    let source_request = SourceInputRequest {
                        project_root: project_root.to_path_buf(),
                        partition: AssetPartition::global(),
                        planned_at: Utc::now(),
                        requested_assets: vec![asset_id(&asset)?],
                        force_refresh_assets: if refresh_shared.contains(&asset) {
                            vec![asset_id(&asset)?]
                        } else {
                            Vec::new()
                        },
                        source_entities: Vec::new(),
                        dependency_inputs: serde_json::json!({"catalog_options": config.collector_options}),
                    };
                    let outcome = self
                        .collect_input(&source_request, &collector.input_field, provider)
                        .await?;
                    operation.collections.insert(collection_id.clone(), outcome);
                    self.save_operation(&key, &mut operation).await?;
                }
                let Some(reference) = operation.collections[&collection_id].input.clone() else {
                    continue;
                };
                operation
                    .shared_inputs
                    .insert(asset.clone(), reference.clone());
                let run_id = operation
                    .shared_runs
                    .entry(asset.clone())
                    .or_default()
                    .clone();
                self.save_operation(&key, &mut operation).await?;
                let mut value = serde_json::Map::new();
                value.insert(
                    collector.input_field.clone(),
                    self.read_collection(&reference).await?,
                );
                // The registry also contains society-partitioned descendants,
                // so planning needs a run coordinate in only-forced mode. The
                // shared asset still resolves to its configured global partition.
                let shared_run_partition = AssetPartition::new([("society", "shared")]);
                let mut options = AssetDagExecutionOptions::new(shared_run_partition, Utc::now())
                    .with_source_scope(SourceEntityResolutionScope::Scoped)
                    .with_source_inputs(serde_json::from_value(serde_json::Value::Object(value))?)
                    .with_only_forced_assets(true)
                    .with_forced_assets(vec![asset_id(&asset)?])
                    .with_required_assets(vec![asset_id(&asset)?]);
                options.pinned_materializations = Some(Vec::new());
                options.initial_run_id = Some(run_id.clone());
                let executor = AssetDagExecutor::new(openestates_registry(), self.lake.clone());
                let run_key = crate::assets::AssetPathBuilder::dag_run_manifest_key(
                    &options.partition,
                    &run_id,
                );
                let report = match self
                    .lake
                    .get_json::<crate::assets::AssetDagRunManifest>(&run_key)
                    .await
                {
                    Ok(_) => {
                        options.force_assets.clear();
                        options.only_forced_assets = false;
                        executor
                            .resume(&KnowledgeGraph::new(), options, run_id)
                            .await
                    }
                    Err(error) if error.is_not_found() => {
                        executor.execute(&KnowledgeGraph::new(), options).await
                    }
                    Err(error) => return Err(error.into()),
                }
                .map_err(|error| invalid(error.to_string()))?;
                let step = report
                    .manifest
                    .steps
                    .iter()
                    .find(|step| {
                        step.asset_id.as_str() == asset
                            && step.status == AssetRunStepStatus::Succeeded
                    })
                    .ok_or_else(|| invalid("shared asset did not materialize"))?;
                let record = AssetMaterializationStore::new(self.lake.clone())
                    .record(
                        &step.asset_id,
                        &step.partition,
                        step.materialization_id.as_ref().expect("succeeded step"),
                    )
                    .await?;
                operation
                    .shared_materializations
                    .insert(asset.clone(), record);
                self.save_operation(&key, &mut operation).await?;
            }
            let mut changes = request.upserts.clone();
            if !operation.shared_materializations.is_empty() {
                for entry in &operation.roster.societies {
                    let identities = seed_identities(&entry.seed);
                    let removed = request
                        .removals
                        .iter()
                        .any(|id| identities.contains(&normalize_identity(id)));
                    let already_changed = changes
                        .iter()
                        .any(|change| !identities.is_disjoint(&seed_identities(&change.seed)));
                    if !removed && !already_changed {
                        changes.push(CatalogUpsert {
                            seed: entry.seed.clone(),
                            refresh_modules: Vec::new(),
                        });
                    }
                }
            }
            for change in changes {
                let id = canonical_seed_id(&change.seed).to_string();
                if operation
                    .societies
                    .get(&id)
                    .is_some_and(|work| work.snapshot.is_some() || work.error.is_some())
                {
                    continue;
                }
                let prior = operation
                    .roster
                    .societies
                    .iter()
                    .find(|entry| {
                        !seed_identities(&entry.seed).is_disjoint(&seed_identities(&change.seed))
                    })
                    .cloned();
                if !operation.societies.contains_key(&id) {
                    let (inputs, pins, input_lineage) = if let Some(entry) = &prior {
                        let (manifest, records) = self.read_snapshot(entry).await?;
                        let mut pins = if manifest.materializations.is_empty()
                            && (!change.refresh_modules.is_empty()
                                || !request.shared_refresh_modules.is_empty())
                        {
                            self.recover_snapshot_lineage(entry, &records).await?
                        } else {
                            manifest.materializations
                        };
                        let mut inputs = manifest.inputs;
                        self.recover_collector_inputs(&pins, &mut inputs).await?;
                        let registry = openestates_registry();
                        pins.retain(|record| {
                            registry.get(&record.asset_id).is_some()
                                && !config
                                    .collectors
                                    .get(record.asset_id.as_str())
                                    .is_some_and(|collector| collector.scope == "shared")
                        });
                        (inputs, pins, manifest.input_lineage)
                    } else {
                        (BTreeMap::new(), Vec::new(), BTreeMap::new())
                    };
                    operation.societies.insert(
                        id.clone(),
                        SocietyWork {
                            seed: change.seed.clone(),
                            inputs,
                            pins,
                            input_lineage,
                            run_id: MaterializationId::new(),
                            snapshot: None,
                            error: None,
                        },
                    );
                    self.save_operation(&key, &mut operation).await?;
                }
                let mut work = operation.societies[&id].clone();
                let registry = openestates_registry();
                work.pins.retain(|record| {
                    registry.get(&record.asset_id).is_some()
                        && !config
                            .collectors
                            .get(record.asset_id.as_str())
                            .is_some_and(|collector| collector.scope == "shared")
                });
                if prior
                    .as_ref()
                    .is_some_and(|entry| entry.seed == change.seed)
                    && change.refresh_modules.is_empty()
                    && request.shared_refresh_modules.is_empty()
                {
                    work.snapshot = prior;
                    operation.societies.insert(id, work);
                    self.save_operation(&key, &mut operation).await?;
                    continue;
                }
                let modules = if prior.is_none() && change.refresh_modules.is_empty() {
                    &config.initial_modules
                } else {
                    &change.refresh_modules
                };
                let mut refresh = BTreeSet::new();
                let mut wanted = BTreeSet::new();
                for name in modules.iter() {
                    let module = &config.modules[name];
                    refresh.extend(module.refresh.iter().cloned());
                    wanted.extend(module.refresh.iter().chain(&module.ensure).cloned());
                }
                // Bootstrap required evidence for a new society. An existing
                // legacy snapshot without recoverable lineage remains intact;
                // unrelated refreshes must not trigger regulatory collection.
                if prior.is_none() && work.pins.is_empty() {
                    wanted.extend(config.required_collectors.iter().cloned());
                }
                let origin_changed = prior.as_ref().is_some_and(|entry| {
                    entry.seed.latitude != change.seed.latitude
                        || entry.seed.longitude != change.seed.longitude
                });
                if origin_changed {
                    refresh.insert(config.origin_collector.clone());
                    wanted.insert(config.origin_collector.clone());
                    for asset in &config.origin_dependents {
                        if work.inputs.contains_key(asset)
                            || work.pins.iter().any(|pin| pin.asset_id.as_str() == asset)
                        {
                            // Legacy observations do not declare spatial coverage;
                            // recollect instead of extending it by assumption.
                            wanted.insert(asset.clone());
                            refresh.insert(asset.clone());
                        }
                    }
                }
                let order = collection_order(&wanted, &config)?;
                let mut changed = BTreeSet::new();
                for asset in order {
                    let collector = &config.collectors[&asset];
                    let scope = if collector.scope == "shared" {
                        "shared"
                    } else {
                        &id
                    };
                    let collection_id = format!("{scope}/{asset}");
                    if collector.scope == "shared" {
                        if let Some(input) = operation.shared_inputs.get(&asset) {
                            work.inputs.insert(asset.clone(), input.clone());
                        }
                    }
                    let prior_input = work.inputs.get(&asset).cloned();
                    if collector.scope == "shared"
                        && operation.shared_materializations.contains_key(&asset)
                    {
                        continue;
                    }
                    let pinned = work
                        .pins
                        .iter()
                        .any(|record| record.asset_id.as_str() == asset);
                    let dependency_changed = collector
                        .requires
                        .iter()
                        .any(|dependency| changed.contains(dependency));
                    if !refresh.contains(&asset)
                        && !dependency_changed
                        && (prior_input.is_some() || pinned)
                    {
                        continue;
                    }
                    if !operation.collections.contains_key(&collection_id) {
                        let mut dependencies = serde_json::Map::new();
                        dependencies
                            .insert("catalog_options".into(), config.collector_options.clone());
                        for dependency in collector.requires.iter().chain(&collector.context) {
                            if let Some(reference) = work.inputs.get(dependency) {
                                let value = self.read_collection(reference).await?;
                                dependencies.insert(
                                    config.collectors[dependency].input_field.clone(),
                                    value,
                                );
                            }
                        }
                        let missing = collector.requires.iter().find(|dependency| {
                            !dependencies.contains_key(&config.collectors[*dependency].input_field)
                        });
                        let outcome = if let Some(dependency) = missing {
                            Collection {
                                input: None,
                                error: Some(format!(
                                    "required collector input {dependency} unavailable"
                                )),
                            }
                        } else {
                            let source_request = SourceInputRequest {
                                project_root: project_root.to_path_buf(),
                                partition: AssetPartition::new([("society", safe_segment(&id))]),
                                planned_at: Utc::now(),
                                requested_assets: vec![asset_id(&asset)?],
                                force_refresh_assets: if refresh.contains(&asset) {
                                    vec![asset_id(&asset)?]
                                } else {
                                    Vec::new()
                                },
                                source_entities: if collector.scope == "shared" {
                                    Vec::new()
                                } else {
                                    vec![change.seed.clone()]
                                },
                                dependency_inputs: serde_json::Value::Object(dependencies),
                            };
                            self.collect_input(&source_request, &collector.input_field, provider)
                                .await?
                        };
                        operation.collections.insert(collection_id.clone(), outcome);
                        self.save_operation(&key, &mut operation).await?;
                    }
                    let outcome = &operation.collections[&collection_id];
                    if outcome.error.is_some()
                        && (dependency_changed
                            || (origin_changed
                                && (asset == config.origin_collector
                                    || config.origin_dependents.contains(&asset))))
                    {
                        work.inputs.remove(&asset);
                        work.input_lineage.remove(&asset);
                        let mut invalidated = BTreeSet::from([asset.clone()]);
                        let registry = openestates_registry();
                        for id in registry
                            .topological_order()
                            .map_err(|error| invalid(error.to_string()))?
                        {
                            if registry
                                .get(&id)
                                .expect("registered")
                                .dependencies
                                .iter()
                                .any(|dependency| invalidated.contains(dependency.as_str()))
                            {
                                invalidated.insert(id.to_string());
                            }
                        }
                        work.pins
                            .retain(|pin| !invalidated.contains(pin.asset_id.as_str()));
                        changed.extend(invalidated);
                    }
                    if let Some(reference) = &outcome.input {
                        if work.inputs.get(&asset) != Some(reference) || refresh.contains(&asset) {
                            changed.insert(asset.clone());
                        }
                        work.input_lineage.insert(
                            asset.clone(),
                            CatalogInputLineage {
                                owner: scope.to_string(),
                                producer_hash: producer_hash.clone(),
                                dependencies: collector
                                    .requires
                                    .iter()
                                    .chain(&collector.context)
                                    .filter_map(|id| {
                                        work.inputs
                                            .get(id)
                                            .map(|reference| (id.clone(), reference.clone()))
                                    })
                                    .collect(),
                            },
                        );
                        work.inputs.insert(asset.clone(), reference.clone());
                        if collector.scope == "shared" {
                            operation
                                .shared_inputs
                                .insert(asset.clone(), reference.clone());
                        }
                    }
                    operation.societies.insert(id.clone(), work.clone());
                    self.save_operation(&key, &mut operation).await?;
                }
                if changed.is_empty()
                    && operation.shared_materializations.is_empty()
                    && prior
                        .as_ref()
                        .is_some_and(|entry| entry.seed == change.seed)
                {
                    work.snapshot = prior;
                    operation.societies.insert(id, work);
                    self.save_operation(&key, &mut operation).await?;
                    continue;
                }
                let mut inputs = serde_json::Map::new();
                inputs.insert(
                    "source_entities".into(),
                    serde_json::to_value(vec![change.seed.clone()])?,
                );
                for (asset, reference) in &work.inputs {
                    inputs.insert(
                        config.collectors[asset].input_field.clone(),
                        self.read_collection(reference).await?,
                    );
                }
                let mut execution_pins = work.pins.clone();
                let mut shared = operation
                    .roster
                    .shared_assets
                    .iter()
                    .map(|record| (record.asset_id.to_string(), record.clone()))
                    .collect::<BTreeMap<_, _>>();
                shared.extend(operation.shared_materializations.clone());
                execution_pins.extend(shared.into_values());
                let mut forced = changed;
                let shared_changed = operation
                    .shared_materializations
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let mut changed_for_cascade = forced.clone();
                changed_for_cascade.extend(shared_changed.iter().cloned());
                for definition in registry.definitions() {
                    if !execution_pins
                        .iter()
                        .any(|record| record.asset_id == definition.id)
                        && (!config.collectors.contains_key(definition.id.as_str())
                            || work.inputs.contains_key(definition.id.as_str()))
                    {
                        forced.insert(definition.id.to_string());
                    }
                }
                for asset in registry
                    .topological_order()
                    .map_err(|error| invalid(error.to_string()))?
                {
                    let definition = registry.get(&asset).expect("registered");
                    if definition
                        .dependencies
                        .iter()
                        .any(|dependency| changed_for_cascade.contains(dependency.as_str()))
                    {
                        forced.insert(asset.to_string());
                        changed_for_cascade.insert(asset.to_string());
                    }
                }
                for asset in shared_changed {
                    forced.remove(&asset);
                }
                forced.insert(SOCIETY_GOLD_SNAPSHOT_ASSET_ID.to_string());
                // Inputs without recoverable provenance cannot be silently overwritten by empty contributions.
                if work.pins.is_empty() && prior.is_some() {
                    work.error = Some("no recoverable collection lineage".into());
                } else if config.required_collectors.iter().any(|asset| {
                    !work.inputs.contains_key(asset)
                        && !work.pins.iter().any(|pin| pin.asset_id.as_str() == asset)
                }) {
                    work.error = Some("required source evidence unavailable".into());
                } else {
                    match materialize_society(
                        self,
                        &change.seed,
                        serde_json::from_value(serde_json::Value::Object(inputs))?,
                        execution_pins,
                        forced
                            .iter()
                            .map(|id| asset_id(id))
                            .collect::<Result<_, _>>()?,
                        work.run_id.clone(),
                    )
                    .await
                    {
                        Ok((records, warnings, pins)) => {
                            let mut snapshot = self
                                .write_snapshot(change.seed.clone(), &records, warnings)
                                .await?;
                            snapshot.inputs = work.inputs.clone();
                            snapshot.input_lineage = work.input_lineage.clone();
                            snapshot.materializations = pins
                                .into_iter()
                                .filter(|record| {
                                    !config
                                        .collectors
                                        .get(record.asset_id.as_str())
                                        .is_some_and(|collector| collector.scope == "shared")
                                })
                                .collect();
                            snapshot.producer_hash = producer_hash.clone();
                            work.pins = snapshot.materializations.clone();
                            self.lake
                                .put_json(
                                    &snapshot_manifest_key(&snapshot.seed, &snapshot.snapshot_id),
                                    &snapshot,
                                )
                                .await?;
                            work.snapshot = Some(snapshot.roster_entry());
                        }
                        Err(error) => {
                            // Integrity and identity failures are fatal. Source failures are recorded above.
                            return Err(error);
                        }
                    }
                }
                operation.societies.insert(id, work);
                self.save_operation(&key, &mut operation).await?;
            }
            operation.collection_ms += started.elapsed().as_millis() as u64;
            let mut candidate = operation.roster.clone();
            for work in operation.societies.values() {
                if let Some(snapshot) = &work.snapshot {
                    candidate.societies.retain(|entry| {
                        seed_identities(&entry.seed).is_disjoint(&seed_identities(&work.seed))
                    });
                    candidate.societies.push(snapshot.clone());
                }
            }
            candidate.societies.retain(|entry| {
                !request
                    .removals
                    .iter()
                    .any(|id| seed_identities(&entry.seed).contains(&normalize_identity(id)))
            });
            for (asset, record) in &operation.shared_materializations {
                candidate
                    .shared_assets
                    .retain(|pin| pin.asset_id.as_str() != asset);
                candidate.shared_assets.push(record.clone());
            }
            operation.candidate = Some(candidate.normalized()?);
            self.save_operation(&key, &mut operation).await?;
        }
        if operation.generation.is_none() {
            let started = Instant::now();
            let (generation, publication) = self
                .prepare_generation(
                    request.operation_id.clone(),
                    operation.candidate.clone().expect("candidate persisted"),
                    operation.base.as_ref(),
                )
                .await?;
            let mut report = CatalogApplyReport {
                publication,
                updated: Vec::new(),
                retained: Vec::new(),
                omitted: Vec::new(),
                removed: request.removals.clone(),
                collected_assets: Vec::new(),
                reused_assets: Vec::new(),
                failed_assets: BTreeMap::new(),
                collection_ms: operation.collection_ms,
                assembly_ms: started.elapsed().as_millis() as u64,
            };
            for (id, work) in &operation.societies {
                if let Some(error) = &work.error {
                    report.publication.warnings.push(format!("{id}: {error}"));
                }
                let previous = operation.roster.societies.iter().find(|entry| {
                    !seed_identities(&entry.seed).is_disjoint(&seed_identities(&work.seed))
                });
                if work
                    .snapshot
                    .as_ref()
                    .is_some_and(|entry| Some(entry) != previous)
                {
                    report.updated.push(id.clone());
                } else if previous.is_none() {
                    report.omitted.push(id.clone());
                }
            }
            for entry in &operation.candidate.as_ref().expect("candidate").societies {
                let id = canonical_seed_id(&entry.seed).to_string();
                if !report.updated.contains(&id) {
                    report.retained.push(id);
                }
            }
            for (id, collection) in &operation.collections {
                if collection.input.is_some() {
                    report.collected_assets.push(id.clone());
                }
                if let Some(error) = &collection.error {
                    report.failed_assets.insert(id.clone(), error.clone());
                }
            }
            for (id, work) in &operation.societies {
                for pin in &work.pins {
                    if pin.run_id != work.run_id {
                        let scope = if config
                            .collectors
                            .get(pin.asset_id.as_str())
                            .is_some_and(|collector| collector.scope == "shared")
                        {
                            "shared"
                        } else {
                            id
                        };
                        report
                            .reused_assets
                            .push(format!("{scope}/{}", pin.asset_id));
                    }
                }
                for asset in work.inputs.keys() {
                    let scope = if config.collectors[asset].scope == "shared" {
                        "shared"
                    } else {
                        id
                    };
                    let label = format!("{scope}/{asset}");
                    if !report.collected_assets.contains(&label) {
                        report.reused_assets.push(label);
                    }
                }
            }
            for pin in &operation
                .candidate
                .as_ref()
                .expect("candidate")
                .shared_assets
            {
                if !operation
                    .shared_materializations
                    .contains_key(pin.asset_id.as_str())
                {
                    report
                        .reused_assets
                        .push(format!("shared/{}", pin.asset_id));
                }
            }
            report.reused_assets.sort();
            report.reused_assets.dedup();
            operation.report = Some(report);
            operation.generation = Some(generation);
            self.save_operation(&key, &mut operation).await?;
        }
        self.publish_generation(
            operation.generation.clone().expect("prepared generation"),
            operation.base.as_ref(),
        )
        .await?;
        operation.completed = true;
        self.save_operation(&key, &mut operation).await?;
        Ok(operation.report.expect("prepared report"))
    }

    async fn collect_input(
        &self,
        request: &SourceInputRequest,
        field: &str,
        provider: &dyn SourceInputProvider,
    ) -> Result<Collection, CatalogError> {
        let asset = request.requested_assets[0].as_str();
        eprintln!(
            "Collecting {}: {asset}",
            request
                .source_entities
                .first()
                .map_or("shared", |seed| canonical_seed_id(seed))
        );
        let result = match provider.load(request, &self.lake).await {
            Ok(Some(inputs)) => {
                if let Some(error) = inputs.source_failures.get(asset) {
                    Collection {
                        input: None,
                        error: Some(error.clone()),
                    }
                } else {
                    let value = serde_json::to_value(inputs)?;
                    if let Some(value) = value.get(field).filter(|value| !value.is_null()) {
                        let bytes = serde_json::to_vec(value)?;
                        let hash = digest(&bytes);
                        let meta = self
                            .lake
                            .put_bytes(
                                &LakeKey::new(format!("raw/catalog_inputs/sha256={hash}.json"))?,
                                bytes,
                            )
                            .await?;
                        Collection {
                            input: Some(ArtifactRef::json(meta)),
                            error: None,
                        }
                    } else {
                        Collection {
                            input: None,
                            error: Some("collector returned no asset input".into()),
                        }
                    }
                }
            }
            Ok(None) => Collection {
                input: None,
                error: Some("collector returned no inputs".into()),
            },
            Err(error) => Collection {
                input: None,
                error: Some(error.to_string()),
            },
        };
        Ok(result)
    }

    async fn save_operation(
        &self,
        key: &LakeKey,
        operation: &mut Operation,
    ) -> Result<(), CatalogError> {
        let revision = operation.revision;
        operation.revision += 1;
        operation.lease_expires_at = Some(Utc::now() + chrono::Duration::hours(1));
        if !self
            .lake
            .put_json_if(key, operation, |current: Option<&Operation>| {
                current.is_some_and(|current| {
                    current.revision == revision && current.lease_owner == operation.lease_owner
                })
            })
            .await?
        {
            return Err(invalid("operation lease/revision changed"));
        }
        Ok(())
    }

    async fn recover_snapshot_lineage(
        &self,
        entry: &CatalogRosterEntry,
        expected: &CatalogRecords,
    ) -> Result<Vec<MaterializationRecord>, CatalogError> {
        let prefix = LakePrefix::new(format!(
            "manifests/runs/society={}",
            safe_segment(canonical_seed_id(&entry.seed))
        ))?;
        let materializations = AssetMaterializationStore::new(self.lake.clone());
        for key in self.lake.list_keys(&prefix).await? {
            if key.as_str().ends_with("/current.json") {
                continue;
            }
            let run: crate::assets::AssetDagRunManifest = self.lake.get_json(&key).await?;
            if !run.steps.iter().any(|step| {
                step.asset_id.as_str() == SOCIETY_GOLD_SNAPSHOT_ASSET_ID
                    && step.status == AssetRunStepStatus::Succeeded
            }) {
                continue;
            }
            let mut pins = BTreeMap::new();
            for step in &run.steps {
                if step.status != AssetRunStepStatus::Succeeded {
                    continue;
                }
                if let Some(id) = &step.materialization_id {
                    let record = materializations
                        .record(&step.asset_id, &step.partition, id)
                        .await?;
                    pins.insert(step.asset_id.to_string(), record);
                }
            }
            // Select by exact content/identity, never by capture time or newest run.
            let records = records_from_materializations(self, &entry.seed, &pins).await?;
            if &records == expected {
                eprintln!("Recovered lineage for {} from {}", entry.seed.name, key);
                return Ok(pins.into_values().collect());
            }
        }
        Ok(Vec::new())
    }

    async fn recover_collector_inputs(
        &self,
        pins: &[MaterializationRecord],
        inputs: &mut BTreeMap<String, ArtifactRef>,
    ) -> Result<(), CatalogError> {
        // Reconstruct typed collector context from the exact raw Parquet pin. This
        // is migration, not an independently selected cache or a source lookup.
        for record in pins {
            if inputs.contains_key(record.asset_id.as_str()) {
                continue;
            }
            let value = match record.asset_id.as_str() {
                crate::assets::GOOGLE_PLACES_WEEKLY_ASSET_ID => {
                    let rows = crate::assets::google::read_google_place_rows(&self.lake, record)
                        .await
                        .map_err(|e| invalid(e.to_string()))?;
                    Some(
                        serde_json::json!({"snapshot_date": "imported", "records": rows, "source_watermarks": record.source_watermarks}),
                    )
                }
                crate::assets::GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID => {
                    let rows =
                        crate::assets::google::read_google_nearby_place_rows(&self.lake, record)
                            .await
                            .map_err(|e| invalid(e.to_string()))?;
                    Some(
                        serde_json::json!({"snapshot_date": "imported", "records": rows, "source_watermarks": record.source_watermarks}),
                    )
                }
                _ => None,
            };
            if let Some(value) = value {
                let bytes = serde_json::to_vec(&value)?;
                let hash = digest(&bytes);
                let meta = self
                    .lake
                    .put_bytes(
                        &LakeKey::new(format!("raw/catalog_inputs/sha256={hash}.json"))?,
                        bytes,
                    )
                    .await?;
                inputs.insert(record.asset_id.to_string(), ArtifactRef::json(meta));
            }
        }
        Ok(())
    }

    async fn read_collection(
        &self,
        reference: &ArtifactRef,
    ) -> Result<serde_json::Value, CatalogError> {
        let key = LakeKey::new(reference.key.clone())?;
        self.lake
            .verify_artifact(&key, reference.size_bytes, &reference.content_hash)
            .await?;
        Ok(self.lake.get_json(&key).await?)
    }
}

fn validate_request(
    request: &CatalogApplyRequest,
    roster: &CatalogRoster,
    config: &EnrichmentConfig,
) -> Result<(), CatalogError> {
    let mut identities = HashSet::new();
    for change in &request.upserts {
        validate_seed(&change.seed)?;
        let mut ids = seed_identities(&change.seed);
        for entry in &roster.societies {
            if !ids.is_disjoint(&seed_identities(&entry.seed)) {
                ids.extend(seed_identities(&entry.seed));
            }
        }
        for id in ids {
            if !identities.insert(id) {
                return Err(invalid("duplicate/conflicting society changes"));
            }
        }
        for module in &change.refresh_modules {
            let definition = config
                .modules
                .get(module)
                .ok_or_else(|| invalid(format!("unknown module {module}")))?;
            if definition.scope != "society" {
                return Err(invalid(format!("module {module} is not society-scoped")));
            }
        }
    }
    for id in &request.removals {
        let entry = roster
            .societies
            .iter()
            .find(|entry| seed_identities(&entry.seed).contains(&normalize_identity(id)))
            .ok_or_else(|| invalid(format!("society {id} is not in the active roster")))?;
        for identity in seed_identities(&entry.seed) {
            if !identities.insert(identity) {
                return Err(invalid("duplicate/conflicting society changes"));
            }
        }
    }
    for module in &request.shared_refresh_modules {
        let definition = config
            .modules
            .get(module)
            .ok_or_else(|| invalid(format!("unknown shared module {module}")))?;
        if definition.scope != "shared" {
            return Err(invalid(format!("module {module} is not shared")));
        }
    }
    Ok(())
}
fn collection_order(
    wanted: &BTreeSet<String>,
    config: &EnrichmentConfig,
) -> Result<Vec<String>, CatalogError> {
    fn visit(
        asset: &str,
        config: &EnrichmentConfig,
        wanted: &BTreeSet<String>,
        visiting: &mut BTreeSet<String>,
        done: &mut Vec<String>,
    ) -> Result<(), CatalogError> {
        if done.iter().any(|id| id == asset) {
            return Ok(());
        }
        if !visiting.insert(asset.to_string()) {
            return Err(invalid("collector dependency cycle"));
        }
        for dependency in config.collectors[asset].requires.iter().chain(
            config.collectors[asset]
                .context
                .iter()
                .filter(|id| wanted.contains(*id)),
        ) {
            visit(dependency, config, wanted, visiting, done)?;
        }
        visiting.remove(asset);
        done.push(asset.to_string());
        Ok(())
    }
    let mut order = Vec::new();
    for asset in wanted {
        visit(asset, config, wanted, &mut BTreeSet::new(), &mut order)?;
    }
    Ok(order)
}
fn operation_key(id: &str) -> Result<LakeKey, CatalogError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(invalid(
            "operation_id must be 1–128 ASCII letters, digits, '_' or '-'",
        ));
    }
    Ok(LakeKey::new(format!(
        "manifests/catalog/operations/{id}.json"
    ))?)
}
fn asset_id(id: &str) -> Result<AssetId, CatalogError> {
    AssetId::new(id).map_err(|error| invalid(error.to_string()))
}
fn invalid(message: impl Into<String>) -> CatalogError {
    CatalogError::Invalid(message.into())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn producer_hash(root: &Path) -> Result<String, CatalogError> {
    let mut digest = Sha256::new();
    for path in [
        "app/config/dag/catalog_enrichment.json",
        "app/config/dag/asset_registry.json",
        "app/config/dag/nearby_place_categories.json",
        "app/config/dag/fact_registry.json",
        "pipeline/collect_asset_sources.py",
    ] {
        digest.update(path.as_bytes());
        digest.update(std::fs::read(root.join(path)).map_err(LakeError::Io)?);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Strip replaced shared contributions from every older snapshot before adding
/// the explicit shared generation. An old topology cannot resurrect a vanished
/// station or boundary. Ownership is configured, not inferred from timestamps.
pub(super) async fn replace_shared_contributions(
    lake: &LakeStore,
    shared: &[MaterializationRecord],
    snapshots: &mut Vec<CatalogRecords>,
) -> Result<(), CatalogError> {
    let config = enrichment_config()?;
    for record in shared {
        let producers = config
            .shared_fact_producers
            .get(record.asset_id.as_str())
            .ok_or_else(|| invalid("shared asset has no ownership policy"))?;
        for snapshot in snapshots.iter_mut() {
            let removed = snapshot
                .facts
                .iter()
                .filter(|fact| {
                    fact.skill_id
                        .as_ref()
                        .is_some_and(|skill| producers.contains(skill))
                })
                .map(|fact| (fact.entity_id.clone(), fact.fact_key.clone()))
                .collect::<BTreeSet<_>>();
            snapshot.facts.retain(|fact| {
                !fact
                    .skill_id
                    .as_ref()
                    .is_some_and(|skill| producers.contains(skill))
            });
            let remaining = snapshot
                .facts
                .iter()
                .map(|fact| fact.entity_id.as_str())
                .collect::<HashSet<_>>();
            let removed_entities = removed
                .iter()
                .filter(|(id, _)| !remaining.contains(id.as_str()))
                .map(|(id, _)| id.clone())
                .collect::<HashSet<_>>();
            snapshot
                .entities
                .retain(|entity| !removed_entities.contains(&entity.entity_id));
            snapshot
                .search_metadata
                .retain(|row| !removed.contains(&(row.entity_id.clone(), row.fact_key.clone())));
            snapshot.edges.retain(|edge| {
                !removed_entities.contains(&edge.from_entity_id)
                    && !removed_entities.contains(&edge.to_entity_id)
            });
        }
        for artifact in &record.artifacts {
            lake.verify_artifact(
                &LakeKey::new(artifact.key.clone())?,
                artifact.size_bytes,
                &artifact.content_hash,
            )
            .await?;
        }
        let rows = crate::assets::read_skill_fact_artifact_rows(lake, std::slice::from_ref(record))
            .await
            .map_err(|error| invalid(error.to_string()))?;
        let gold = SocietyGoldRecords::from_graph_with_skill_facts(
            &KnowledgeGraph::new(),
            &rows.facts,
            &rows.fact_annotations,
        )
        .map_err(|error| invalid(error.to_string()))?;
        snapshots.push(CatalogRecords::from_society_gold(&gold, Vec::new())?);
    }
    Ok(())
}
