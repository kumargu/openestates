use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use backend::assets::{
    openestates_registry, AssetDagExecutionOptions, AssetDagExecutor, AssetDagRunManifest, AssetId,
    AssetMaterializationStore, AssetPartition, AssetRunManifestStore, AssetSourceInputs,
    CommandSourceInputProvider, LakeObjectSourceInputProvider, LocalFileSourceInputProvider,
    MaterializationId, SourceEntityResolutionScope, SourceEntitySeed, SourceInputCollectionPlan,
    SourceInputProvider, SourceInputRequest, CANONICAL_SOCIETY_NODES_ASSET_ID,
    DEFAULT_RESUME_LEASE_SECONDS, EXTERNAL_IMAGES_WEEKLY_ASSET_ID,
    EXTERNAL_LISTINGS_WEEKLY_ASSET_ID, GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID,
    GOOGLE_PLACES_WEEKLY_ASSET_ID, OSM_POWER_LINE_FACTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
    SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID, STORMWATER_DRAIN_FACTS_ASSET_ID,
};
use backend::knowledge::KnowledgeGraph;
use backend::lake::{LakeKey, LakeStore, LakeStoreLocation};
use chrono::{Duration as ChronoDuration, Utc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliOptions::parse()?;
    let partition = cli.partition();
    let project_root = cli
        .project_root
        .clone()
        .unwrap_or_else(default_project_root);
    let graph = KnowledgeGraph::new();

    let lake_location = LakeStoreLocation::from_env(&project_root)?;
    let lake = lake_location.open()?;
    let executor = AssetDagExecutor::new(openestates_registry(), lake.clone())
        .with_project_root(project_root.clone());
    let requested_at = Utc::now();
    let mut resume_manifest = if let Some(run_id) = &cli.resume_run_id {
        let manifest = AssetRunManifestStore::new(lake.clone())
            .manifest(&partition, run_id)
            .await?;
        if manifest.partition != partition {
            return Err(format!(
                "DAG run {run_id} belongs to partition {:?}, not {:?}",
                manifest.partition, partition
            )
            .into());
        }
        manifest.ensure_exact_resume()?;
        Some(manifest)
    } else {
        None
    };
    let planned_at = resume_manifest
        .as_ref()
        .map_or(requested_at, |manifest| manifest.created_at);
    let mut options = AssetDagExecutionOptions::new(partition, planned_at).dry_run(cli.dry_run);
    let scoped_run = cli.scoped_source_inputs
        || !cli.source_entity_ids.is_empty()
        || !cli.source_entity_seed_paths.is_empty();
    options = options.with_source_scope(if scoped_run {
        SourceEntityResolutionScope::Scoped
    } else {
        SourceEntityResolutionScope::Production
    });
    if let Some(manifest) = &resume_manifest {
        if !manifest.execution_version.is_empty() {
            options = options.with_version(manifest.execution_version.clone());
        }
    } else if let Some(version) = cli.version.clone() {
        options = options.with_version(version);
    }
    if !cli.force_asset_ids.is_empty() {
        options = options.with_forced_assets(cli.force_asset_ids.clone());
    }
    if cli.only_forced_assets {
        options = options.with_only_forced_assets(true);
    }
    let mut resume_lease_id = None;
    if cli.dry_run && cli.source_command.is_some() {
        eprintln!("Skipping source collector command during dry-run.");
    } else if let Some(provider) = cli.source_input_provider()? {
        let mut collection_plan = if let Some(manifest) = &resume_manifest {
            AssetSourceInputs::resume_collection_plan(manifest)
        } else {
            let plan = executor
                .plan_with_forced_assets(&options.partition, planned_at, &cli.force_asset_ids)
                .await?;
            AssetSourceInputs::collection_plan(&plan)
        };
        if resume_manifest.is_none()
            && (!cli.source_entity_ids.is_empty() || !cli.source_entity_seed_paths.is_empty())
        {
            include_scoped_rera_refresh(&mut collection_plan);
        }
        if !cli.source_collection_asset_ids.is_empty() {
            restrict_source_collection_plan(&mut collection_plan, &cli.source_collection_asset_ids);
        }
        let mut source_entities = if should_load_current_source_entities(
            resume_manifest.as_ref(),
            &cli.source_entity_ids,
            cli.scoped_source_inputs,
            &cli.source_entity_seed_paths,
        ) {
            current_source_entities(&lake, resume_manifest.as_ref(), &cli.source_entity_ids).await?
        } else {
            Vec::new()
        };
        source_entities.extend(load_source_entity_seed_files(&cli.source_entity_seed_paths).await?);
        let source_entities = dedupe_source_entities(source_entities)?;
        let runner_source_entities = source_entities.clone();
        let request = SourceInputRequest {
            project_root: project_root.clone(),
            partition: options.partition.clone(),
            planned_at,
            requested_assets: collection_plan.requested_assets,
            force_refresh_assets: collection_plan.force_refresh_assets,
            source_entities,
        };
        if let Some(manifest) = resume_manifest.as_mut() {
            let lease_id = MaterializationId::new();
            let collector_lease_seconds = cli
                .source_timeout_seconds
                .unwrap_or(30 * 60)
                .saturating_add(5 * 60) as i64;
            AssetRunManifestStore::new(lake.clone())
                .acquire_resume_lease(
                    manifest,
                    lease_id.clone(),
                    Utc::now(),
                    ChronoDuration::seconds(
                        collector_lease_seconds.max(DEFAULT_RESUME_LEASE_SECONDS),
                    ),
                )
                .await?;
            options = options.with_resume_lease(lease_id.clone());
            resume_lease_id = Some(lease_id);
        }
        let loaded = match provider.load(&request, &lake).await {
            Ok(loaded) => loaded,
            Err(err) => {
                release_cli_resume_lease(
                    &lake,
                    &options.partition,
                    cli.resume_run_id.as_ref(),
                    resume_lease_id.as_ref(),
                )
                .await;
                return Result::<(), Box<dyn std::error::Error>>::Err(Box::new(err));
            }
        };
        if let Some(mut source_inputs) = loaded {
            merge_source_input_entities(&mut source_inputs, runner_source_entities)?;
            let mut force_assets = collection_plan.force_assets;
            force_assets.extend(cli.force_asset_ids.clone());
            force_assets.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            force_assets.dedup();
            options = options
                .with_source_inputs(source_inputs)
                .with_forced_assets(force_assets);
        }
    }

    let execution_partition = options.partition.clone();
    let resume_run_id = cli.resume_run_id.clone();
    let execution = match resume_run_id.clone() {
        Some(run_id) => executor.resume(&graph, options, run_id).await,
        None => executor.execute(&graph, options).await,
    };
    let report = match execution {
        Ok(report) => report,
        Err(err) => {
            release_cli_resume_lease(
                &lake,
                &execution_partition,
                resume_run_id.as_ref(),
                resume_lease_id.as_ref(),
            )
            .await;
            let run_store = AssetRunManifestStore::new(lake.clone());
            let failed_manifest = match resume_run_id {
                Some(run_id) => run_store.manifest(&execution_partition, &run_id).await.ok(),
                None => run_store.current_manifest(&execution_partition).await.ok(),
            };
            if let Some(manifest) = failed_manifest.filter(|manifest| {
                manifest.created_at == planned_at
                    && matches!(
                        manifest.status,
                        backend::assets::DagRunStatus::Failed
                            | backend::assets::DagRunStatus::Running
                    )
            }) {
                eprintln!(
                    "Asset DAG run {} failed; resume with --resume-run {}",
                    manifest.run_id, manifest.run_id
                );
            }
            return Result::<(), Box<dyn std::error::Error>>::Err(Box::new(err));
        }
    };

    if report.dry_run {
        eprintln!("Planned asset DAG run without writing manifests.");
    } else {
        eprintln!("Asset DAG run status: {:?}", report.manifest.status);
        eprintln!("Executed assets: {}", report.executed_assets.len());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}

fn include_scoped_rera_refresh(collection_plan: &mut SourceInputCollectionPlan) {
    let rera = AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).expect("valid static RERA asset ID");
    let google_places =
        AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).expect("valid static Google asset ID");
    let google_nearby = AssetId::new(GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID)
        .expect("valid static Google nearby asset ID");
    collection_plan.requested_assets.extend([
        rera.clone(),
        google_places.clone(),
        google_nearby.clone(),
        AssetId::new(EXTERNAL_LISTINGS_WEEKLY_ASSET_ID)
            .expect("valid static external listings asset ID"),
        AssetId::new(EXTERNAL_IMAGES_WEEKLY_ASSET_ID)
            .expect("valid static external images asset ID"),
    ]);
    collection_plan
        .force_assets
        .extend([rera, google_places, google_nearby]);
    collection_plan
        .requested_assets
        .sort_by(|left, right| left.as_str().cmp(right.as_str()));
    collection_plan.requested_assets.dedup();
    collection_plan
        .force_assets
        .sort_by(|left, right| left.as_str().cmp(right.as_str()));
    collection_plan.force_assets.dedup();
}

fn restrict_source_collection_plan(
    collection_plan: &mut SourceInputCollectionPlan,
    allowed_assets: &[AssetId],
) {
    let mut allowed: BTreeSet<_> = allowed_assets
        .iter()
        .map(|asset_id| asset_id.as_str().to_string())
        .collect();
    let needs_google_places_companion = add_geospatial_source_companions(&mut allowed);
    collection_plan
        .requested_assets
        .retain(|asset_id| allowed.contains(asset_id.as_str()));
    collection_plan
        .force_assets
        .retain(|asset_id| allowed.contains(asset_id.as_str()));
    collection_plan
        .force_refresh_assets
        .retain(|asset_id| allowed.contains(asset_id.as_str()));
    if needs_google_places_companion {
        let google_places = AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID)
            .expect("valid static Google Places asset ID");
        collection_plan.requested_assets.push(google_places.clone());
        collection_plan.force_assets.push(google_places);
        collection_plan
            .requested_assets
            .sort_by(|left, right| left.as_str().cmp(right.as_str()));
        collection_plan.requested_assets.dedup();
        collection_plan
            .force_assets
            .sort_by(|left, right| left.as_str().cmp(right.as_str()));
        collection_plan.force_assets.dedup();
    }
}

fn add_geospatial_source_companions(allowed_assets: &mut BTreeSet<String>) -> bool {
    let needs_google_places_companion = [
        SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
        OSM_POWER_LINE_FACTS_ASSET_ID,
        STORMWATER_DRAIN_FACTS_ASSET_ID,
    ]
    .iter()
    .any(|asset_id| allowed_assets.contains(*asset_id));
    if needs_google_places_companion {
        allowed_assets.insert(GOOGLE_PLACES_WEEKLY_ASSET_ID.to_string());
    }
    needs_google_places_companion
}

async fn release_cli_resume_lease(
    lake: &LakeStore,
    partition: &AssetPartition,
    run_id: Option<&MaterializationId>,
    lease_id: Option<&MaterializationId>,
) {
    if let (Some(run_id), Some(lease_id)) = (run_id, lease_id) {
        let _ = AssetRunManifestStore::new(lake.clone())
            .release_resume_lease(partition, run_id, lease_id)
            .await;
    }
}

fn should_load_current_source_entities(
    resume_manifest: Option<&AssetDagRunManifest>,
    selected_entity_ids: &[String],
    scoped_source_inputs: bool,
    source_entity_seed_paths: &[PathBuf],
) -> bool {
    resume_manifest.is_some()
        || !selected_entity_ids.is_empty()
        || (!scoped_source_inputs && source_entity_seed_paths.is_empty())
}

fn merge_source_input_entities(
    source_inputs: &mut AssetSourceInputs,
    runner_source_entities: Vec<SourceEntitySeed>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut source_entities = std::mem::take(&mut source_inputs.source_entities);
    source_entities.extend(runner_source_entities);
    source_inputs.source_entities = dedupe_source_entities(source_entities)?;
    Ok(())
}

async fn current_source_entities(
    lake: &LakeStore,
    resume_manifest: Option<&AssetDagRunManifest>,
    selected_entity_ids: &[String],
) -> Result<Vec<SourceEntitySeed>, Box<dyn std::error::Error>> {
    let store = AssetMaterializationStore::new(lake.clone());
    let asset_id = AssetId::new(CANONICAL_SOCIETY_NODES_ASSET_ID)?;
    let record = match resume_manifest {
        Some(manifest) => {
            let Some(step) = manifest.steps.iter().find(|step| step.asset_id == asset_id) else {
                if selected_entity_ids.is_empty() {
                    return Ok(Vec::new());
                }
                return Err(
                    "--source-entity requires an existing canonical society snapshot"
                        .to_string()
                        .into(),
                );
            };
            let Some(materialization_id) = step
                .materialization_id
                .as_ref()
                .or(step.current_materialization_id.as_ref())
            else {
                if selected_entity_ids.is_empty() {
                    return Ok(Vec::new());
                }
                return Err(
                    "--source-entity requires an existing canonical society snapshot"
                        .to_string()
                        .into(),
                );
            };
            store
                .record(&asset_id, &step.partition, materialization_id)
                .await?
        }
        None => match store
            .current_record(&asset_id, &AssetPartition::global())
            .await
        {
            Ok(record) => record,
            Err(err) if err.is_not_found() && selected_entity_ids.is_empty() => {
                return Ok(Vec::new())
            }
            Err(err) if err.is_not_found() => {
                return Err(
                    "--source-entity requires an existing canonical society snapshot"
                        .to_string()
                        .into(),
                )
            }
            Err(err) => return Err(Box::new(err)),
        },
    };
    let rows = backend::assets::read_canonical_society_rows(lake, &record).await?;
    let names: HashMap<_, _> = rows
        .entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity.name.as_str()))
        .collect();
    let areas: HashMap<_, _> = rows
        .edges
        .iter()
        .filter(|edge| edge.relation == "SocietyInArea")
        .filter_map(|edge| {
            names
                .get(edge.to_entity_id.as_str())
                .map(|area| (edge.from_entity_id.as_str(), (*area).to_string()))
        })
        .collect();
    let mut seeds = BTreeMap::new();
    let mut unmatched: BTreeSet<_> = selected_entity_ids.iter().cloned().collect();
    for mapping in rows.mappings {
        let project_key_selected = unmatched.contains(mapping.project_key.as_str());
        let identifiers = [
            Some(mapping.canonical_entity_id.as_str()),
            mapping.alias_entity_id.as_deref(),
            Some(mapping.project_key.as_str()),
        ];
        let mut selected = selected_entity_ids.is_empty();
        if !selected {
            for identifier in identifiers.iter().flatten() {
                selected = unmatched.remove(*identifier) || selected;
            }
        }
        if !selected {
            continue;
        }
        let seed_key = source_entity_seed_key(
            &mapping.canonical_entity_id,
            &mapping.project_key,
            project_key_selected,
        );
        seeds.entry(seed_key).or_insert_with(|| SourceEntitySeed {
            entity_id: mapping.canonical_entity_id.clone(),
            alias_entity_id: mapping.alias_entity_id,
            name: mapping.project_name,
            area: areas.get(mapping.canonical_entity_id.as_str()).cloned(),
            city: Some("Bengaluru".to_string()),
            project_key: Some(mapping.project_key),
            latitude: None,
            longitude: None,
        });
    }
    if !unmatched.is_empty() {
        return Err(format!(
            "unknown --source-entity selector(s): {}",
            unmatched.into_iter().collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    Ok(seeds.into_values().collect())
}

fn source_entity_seed_key(
    canonical_entity_id: &str,
    project_key: &str,
    project_key_selected: bool,
) -> String {
    if project_key_selected {
        project_key.to_string()
    } else {
        canonical_entity_id.to_string()
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SourceEntitySeedFile {
    List(Vec<SourceEntitySeed>),
    Object {
        source_entities: Option<Vec<SourceEntitySeed>>,
    },
}

async fn load_source_entity_seed_files(
    paths: &[PathBuf],
) -> Result<Vec<SourceEntitySeed>, Box<dyn std::error::Error>> {
    let mut seeds = Vec::new();
    for path in paths {
        let bytes = tokio::fs::read(path).await?;
        match serde_json::from_slice(&bytes)? {
            SourceEntitySeedFile::List(mut rows) => seeds.append(&mut rows),
            SourceEntitySeedFile::Object { source_entities } => {
                let Some(mut rows) = source_entities else {
                    return Err(format!(
                        "source entity seed file {} must contain source_entities",
                        path.display()
                    )
                    .into());
                };
                if rows.is_empty() {
                    return Err(format!(
                        "source entity seed file {} contains no source_entities",
                        path.display()
                    )
                    .into());
                }
                seeds.append(&mut rows);
            }
        }
    }
    Ok(seeds)
}

fn dedupe_source_entities(
    seeds: Vec<SourceEntitySeed>,
) -> Result<Vec<SourceEntitySeed>, Box<dyn std::error::Error>> {
    let mut by_key = BTreeMap::<String, SourceEntitySeed>::new();
    for seed in seeds {
        validate_source_entity_seed_coordinates(&seed)?;
        let key = seed
            .project_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(seed.entity_id.as_str())
            .to_string();
        if let Some(existing) = by_key.get(&key) {
            let merged = merge_source_entity_seed(&key, existing.clone(), seed)?;
            by_key.insert(key, merged);
            continue;
        }
        by_key.insert(key, seed);
    }
    Ok(by_key.into_values().collect())
}

fn validate_source_entity_seed_coordinates(
    seed: &SourceEntitySeed,
) -> Result<(), Box<dyn std::error::Error>> {
    match (seed.latitude, seed.longitude) {
        (Some(latitude), Some(longitude))
            if backend::dag_config::valid_coordinate_pair(latitude, longitude) =>
        {
            Ok(())
        }
        (None, None) => Ok(()),
        (Some(_), Some(_)) => Err(format!(
            "source entity seed {} has invalid coordinates",
            seed.entity_id
        )
        .into()),
        _ => Err(format!(
            "source entity seed {} must provide latitude and longitude together",
            seed.entity_id
        )
        .into()),
    }
}

fn merge_source_entity_seed(
    key: &str,
    existing: SourceEntitySeed,
    incoming: SourceEntitySeed,
) -> Result<SourceEntitySeed, Box<dyn std::error::Error>> {
    if existing.entity_id != incoming.entity_id {
        return Err(format!(
            "conflicting source entity seed for selector {key}: {} vs {}",
            existing.entity_id, incoming.entity_id
        )
        .into());
    }
    let name = merge_required_seed_text(key, "name", existing.name, incoming.name)?;
    Ok(SourceEntitySeed {
        entity_id: existing.entity_id,
        alias_entity_id: merge_optional_seed_text(
            key,
            "alias_entity_id",
            existing.alias_entity_id,
            incoming.alias_entity_id,
        )?,
        name,
        area: merge_optional_seed_text(key, "area", existing.area, incoming.area)?,
        city: merge_optional_seed_text(key, "city", existing.city, incoming.city)?,
        project_key: merge_optional_seed_text(
            key,
            "project_key",
            existing.project_key,
            incoming.project_key,
        )?,
        latitude: merge_optional_seed_coordinate(
            key,
            "latitude",
            existing.latitude,
            incoming.latitude,
        )?,
        longitude: merge_optional_seed_coordinate(
            key,
            "longitude",
            existing.longitude,
            incoming.longitude,
        )?,
    })
}

fn merge_required_seed_text(
    key: &str,
    field: &str,
    existing: String,
    incoming: String,
) -> Result<String, Box<dyn std::error::Error>> {
    if existing.trim().eq_ignore_ascii_case(incoming.trim()) {
        return Ok(existing);
    }
    Err(format!(
        "conflicting source entity seed for selector {key}: {field} differs ({existing} vs {incoming})"
    )
    .into())
}

fn merge_optional_seed_text(
    key: &str,
    field: &str,
    existing: Option<String>,
    incoming: Option<String>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match (existing, incoming) {
        (Some(existing), Some(incoming))
            if existing.trim().eq_ignore_ascii_case(incoming.trim()) =>
        {
            Ok(Some(existing))
        }
        (Some(existing), None) => Ok(Some(existing)),
        (None, Some(incoming)) => Ok(Some(incoming)),
        (None, None) => Ok(None),
        (Some(existing), Some(incoming)) => Err(format!(
            "conflicting source entity seed for selector {key}: {field} differs ({existing} vs {incoming})"
        )
        .into()),
    }
}

fn merge_optional_seed_coordinate(
    key: &str,
    field: &str,
    existing: Option<f64>,
    incoming: Option<f64>,
) -> Result<Option<f64>, Box<dyn std::error::Error>> {
    match (existing, incoming) {
        (Some(existing), Some(incoming)) if (existing - incoming).abs() <= 0.0000001 => {
            Ok(Some(existing))
        }
        (Some(existing), None) => Ok(Some(existing)),
        (None, Some(incoming)) => Ok(Some(incoming)),
        (None, None) => Ok(None),
        (Some(existing), Some(incoming)) => Err(format!(
            "conflicting source entity seed for selector {key}: {field} differs ({existing} vs {incoming})"
        )
        .into()),
    }
}

#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    partition_parts: Vec<(String, String)>,
    version: Option<String>,
    source_inputs_path: Option<PathBuf>,
    source_inputs_key: Option<String>,
    source_command: Option<PathBuf>,
    source_args: Vec<OsString>,
    source_timeout_seconds: Option<u64>,
    source_max_output_mib: Option<usize>,
    source_entity_ids: Vec<String>,
    source_entity_seed_paths: Vec<PathBuf>,
    source_collection_asset_ids: Vec<AssetId>,
    scoped_source_inputs: bool,
    only_forced_assets: bool,
    force_asset_ids: Vec<AssetId>,
    resume_run_id: Option<MaterializationId>,
    dry_run: bool,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut options = CliOptions::default();
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--project-root requires a value".to_string())?;
                    options.project_root = Some(PathBuf::from(value));
                }
                "--partition" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--partition requires key=value".to_string())?;
                    options.partition_parts.push(parse_partition_part(&value)?);
                }
                "--version" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--version requires a value".to_string())?;
                    options.version = Some(value);
                }
                "--source-inputs" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-inputs requires a path".to_string())?;
                    options.source_inputs_path = Some(PathBuf::from(value));
                }
                "--source-input-key" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-input-key requires a lake key".to_string())?;
                    options.source_inputs_key = Some(value);
                }
                "--source-command" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-command requires a program".to_string())?;
                    options.source_command = Some(PathBuf::from(value));
                }
                "--source-arg" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-arg requires a value".to_string())?;
                    options.source_args.push(OsString::from(value));
                }
                "--source-timeout-seconds" => {
                    let value = args.next().ok_or_else(|| {
                        "--source-timeout-seconds requires a positive integer".to_string()
                    })?;
                    let seconds = value.parse::<u64>().map_err(|_| {
                        "--source-timeout-seconds requires a positive integer".to_string()
                    })?;
                    if seconds == 0 {
                        return Err(
                            "--source-timeout-seconds requires a positive integer".to_string()
                        );
                    }
                    options.source_timeout_seconds = Some(seconds);
                }
                "--source-max-output-mib" => {
                    let value = args.next().ok_or_else(|| {
                        "--source-max-output-mib requires an integer from 1 to 512".to_string()
                    })?;
                    let mebibytes = value.parse::<usize>().map_err(|_| {
                        "--source-max-output-mib requires an integer from 1 to 512".to_string()
                    })?;
                    if !(1..=512).contains(&mebibytes) {
                        return Err(
                            "--source-max-output-mib requires an integer from 1 to 512".to_string()
                        );
                    }
                    options.source_max_output_mib = Some(mebibytes);
                }
                "--source-entity" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-entity requires an entity id".to_string())?;
                    let value = value.trim();
                    if value.is_empty() {
                        return Err("--source-entity requires an entity id".to_string());
                    }
                    options.source_entity_ids.push(value.to_string());
                }
                "--source-entity-seeds" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--source-entity-seeds requires a path".to_string())?;
                    options.source_entity_seed_paths.push(PathBuf::from(value));
                }
                "--source-collection-asset" => {
                    let value = args.next().ok_or_else(|| {
                        "--source-collection-asset requires an asset id".to_string()
                    })?;
                    let asset_id = AssetId::new(value.trim()).map_err(|_| {
                        format!("--source-collection-asset requires a valid asset id, got: {value}")
                    })?;
                    if !AssetSourceInputs::supports_asset(&asset_id) {
                        return Err(format!(
                            "--source-collection-asset is not source-collectable: {value}"
                        ));
                    }
                    options.source_collection_asset_ids.push(asset_id);
                }
                "--scoped-source-inputs" => {
                    options.scoped_source_inputs = true;
                }
                "--only-forced-assets" => {
                    options.only_forced_assets = true;
                }
                "--force-asset" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--force-asset requires an asset id".to_string())?;
                    options
                        .force_asset_ids
                        .push(AssetId::new(value.trim()).map_err(|_| {
                            format!("--force-asset requires a valid asset id, got: {value}")
                        })?);
                }
                "--resume-run" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--resume-run requires a run UUID".to_string())?;
                    options.resume_run_id = Some(value.parse().map_err(|_| {
                        format!("--resume-run requires a valid UUID, got: {value}")
                    })?);
                }
                "--dry-run" => {
                    options.dry_run = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        let provider_count = [
            options.source_inputs_path.is_some(),
            options.source_inputs_key.is_some(),
            options.source_command.is_some(),
        ]
        .into_iter()
        .filter(|configured| *configured)
        .count();
        if provider_count > 1 {
            return Err(
                "use only one of --source-inputs, --source-input-key, or --source-command"
                    .to_string(),
            );
        }
        if !options.source_args.is_empty() && options.source_command.is_none() {
            return Err("--source-arg requires --source-command".to_string());
        }
        if options.source_timeout_seconds.is_some() && options.source_command.is_none() {
            return Err("--source-timeout-seconds requires --source-command".to_string());
        }
        if options.source_max_output_mib.is_some() && options.source_command.is_none() {
            return Err("--source-max-output-mib requires --source-command".to_string());
        }
        if options.dry_run && options.resume_run_id.is_some() {
            return Err("--resume-run cannot be combined with --dry-run".to_string());
        }

        Ok(options)
    }

    fn partition(&self) -> AssetPartition {
        if self.partition_parts.is_empty() {
            AssetPartition::global()
        } else {
            AssetPartition::new(self.partition_parts.clone())
        }
    }

    fn source_input_provider(
        &self,
    ) -> Result<Option<Box<dyn SourceInputProvider>>, Box<dyn std::error::Error>> {
        if let Some(path) = &self.source_inputs_path {
            return Ok(Some(Box::new(LocalFileSourceInputProvider::new(path))));
        }

        if let Some(key) = &self.source_inputs_key {
            let lake_key = LakeKey::new(key.clone())?;
            return Ok(Some(Box::new(LakeObjectSourceInputProvider::new(lake_key))));
        }

        if let Some(program) = &self.source_command {
            let mut provider =
                CommandSourceInputProvider::new(program).with_args(self.source_args.clone());
            if let Some(seconds) = self.source_timeout_seconds {
                provider = provider.with_timeout(Duration::from_secs(seconds));
            }
            if let Some(mebibytes) = self.source_max_output_mib {
                provider = provider.with_max_stdout_bytes(mebibytes * 1024 * 1024);
            }
            return Ok(Some(Box::new(provider)));
        }

        Ok(None)
    }
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

fn parse_partition_part(value: &str) -> Result<(String, String), String> {
    let (key, value) = value
        .split_once('=')
        .ok_or_else(|| format!("partition must be key=value, got: {value}"))?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return Err(format!("partition must be key=value, got: {key}={value}"));
    }
    Ok((key.to_string(), value.to_string()))
}

fn print_help() {
    println!("Run the OpenEstates asset DAG and write a durable run manifest.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin openestates-run-assets -- [--project-root <path>] [--partition key=value]... [--version <version>] [--source-command <program> [--source-arg <arg>]... [--source-timeout-seconds <seconds>]] [--source-entity <entity-id>]... [--resume-run <uuid>] [--dry-run]"
    );
    println!();
    println!("Options:");
    println!("  --dry-run       Plan the DAG without writing run manifests or artifacts");
    println!("  --partition     Add a partition coordinate, e.g. --partition dt=2026-07-13");
    println!(
        "  --version       Artifact version for runnable assets; defaults to current UTC time"
    );
    println!("  --source-inputs Read source executor inputs from a local JSON file");
    println!("  --source-input-key Read source executor inputs from a lake object key");
    println!("  --source-command Run a collector that reads SourceInputRequest JSON on stdin");
    println!("  --source-arg     Pass one literal argument to the source collector program");
    println!("  --source-timeout-seconds Override the collector timeout (default: 1800)");
    println!("  --source-max-output-mib Override the collector output cap (default: 16)");
    println!("  --source-entity Limit source collection to one entity, alias, or project key");
    println!("  --source-entity-seeds Add source entities from a JSON file");
    println!("  --source-collection-asset Restrict source collection to one collectable asset id");
    println!("  --scoped-source-inputs Treat --source-inputs as already scoped to a partial run");
    println!(
        "  --only-forced-assets Skip non-forced planned assets and use their current snapshots"
    );
    println!("  --force-asset   Force one asset id to run even when freshness says skip");
    println!("  --resume-run Resume one failed or interrupted run by UUID");
    println!(
        "  {env_name:<18} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = backend::lake::LAKE_URL_ENV
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend::assets::{
        CanonicalSocietyMaterializer, ReraProjectSnapshotRecord, ReraRegistryMaterializer,
        ReraRegistryMonthlyInput,
    };
    use chrono::TimeZone;
    use tempfile::tempdir;

    #[test]
    fn scoped_collection_refreshes_rera_listing_with_selected_details() {
        let mut plan = SourceInputCollectionPlan {
            requested_assets: Vec::new(),
            force_assets: Vec::new(),
            force_refresh_assets: Vec::new(),
        };

        include_scoped_rera_refresh(&mut plan);
        include_scoped_rera_refresh(&mut plan);

        assert_eq!(
            plan.requested_assets,
            vec![
                AssetId::new("external_images_weekly").unwrap(),
                AssetId::new("external_listings_weekly").unwrap(),
                AssetId::new("google_nearby_places_weekly").unwrap(),
                AssetId::new("google_places_weekly").unwrap(),
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
            ]
        );
        assert_eq!(
            plan.force_assets,
            vec![
                AssetId::new("google_nearby_places_weekly").unwrap(),
                AssetId::new("google_places_weekly").unwrap(),
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap()
            ]
        );
    }

    #[test]
    fn source_collection_filter_keeps_only_allowed_source_assets() {
        let mut plan = SourceInputCollectionPlan {
            requested_assets: vec![
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
            ],
            force_assets: vec![
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
            ],
            force_refresh_assets: vec![
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
            ],
        };

        restrict_source_collection_plan(
            &mut plan,
            &[AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap()],
        );

        assert_eq!(
            plan.requested_assets,
            vec![AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap()]
        );
        assert_eq!(
            plan.force_assets,
            vec![AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap()]
        );
        assert_eq!(
            plan.force_refresh_assets,
            vec![AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap()]
        );
    }

    #[test]
    fn source_collection_filter_keeps_google_places_for_geospatial_assets() {
        let mut plan = SourceInputCollectionPlan {
            requested_assets: vec![
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
            ],
            force_assets: vec![
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(RERA_REGISTRY_MONTHLY_ASSET_ID).unwrap(),
            ],
            force_refresh_assets: Vec::new(),
        };

        restrict_source_collection_plan(
            &mut plan,
            &[AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap()],
        );

        assert_eq!(
            plan.requested_assets,
            vec![
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
            ]
        );
        assert_eq!(
            plan.force_assets,
            vec![
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
            ]
        );
    }

    #[test]
    fn source_collection_filter_adds_google_places_for_geospatial_assets() {
        let mut plan = SourceInputCollectionPlan {
            requested_assets: vec![AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap()],
            force_assets: vec![AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap()],
            force_refresh_assets: Vec::new(),
        };

        restrict_source_collection_plan(
            &mut plan,
            &[AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap()],
        );

        assert_eq!(
            plan.requested_assets,
            vec![
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
            ]
        );
        assert_eq!(
            plan.force_assets,
            vec![
                AssetId::new(GOOGLE_PLACES_WEEKLY_ASSET_ID).unwrap(),
                AssetId::new(OSM_POWER_LINE_FACTS_ASSET_ID).unwrap(),
            ]
        );
    }

    #[test]
    fn scoped_seed_runs_do_not_load_current_source_entities() {
        assert!(!should_load_current_source_entities(None, &[], true, &[]));
        assert!(should_load_current_source_entities(
            None,
            &["society:selected".to_string()],
            true,
            &[]
        ));
        assert!(!should_load_current_source_entities(
            None,
            &[],
            false,
            &[PathBuf::from("seeds.json")]
        ));
        assert!(should_load_current_source_entities(None, &[], false, &[]));
    }

    #[tokio::test]
    async fn source_entity_seed_files_load_object_shape() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("seeds.json");
        std::fs::write(
            &path,
            r#"{
              "source_entities": [
                {
                  "entity_id": "society:rera-6f1049070060d911",
                  "alias_entity_id": "society:folium-by-sumadhura-phase-i",
                  "name": "FOLIUM BY SUMADHURA PHASE-I",
                  "area": "Whitefield",
                  "city": "Bengaluru",
                  "project_key": "PRM/KA/RERA/1251/446/PR/280222/004738",
                  "latitude": 12.9698,
                  "longitude": 77.75
                }
              ]
            }"#,
        )
        .unwrap();

        let seeds = load_source_entity_seed_files(&[path]).await.unwrap();

        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].name, "FOLIUM BY SUMADHURA PHASE-I");
        assert_eq!(
            seeds[0].project_key.as_deref(),
            Some("PRM/KA/RERA/1251/446/PR/280222/004738")
        );
        assert_eq!(seeds[0].latitude, Some(12.9698));
        assert_eq!(seeds[0].longitude, Some(77.75));
    }

    #[tokio::test]
    async fn source_entity_seed_files_fail_closed_on_missing_source_entities() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("seeds.json");
        std::fs::write(&path, r#"{"source_entity": []}"#).unwrap();

        let error = load_source_entity_seed_files(&[path]).await.unwrap_err();

        assert!(error.to_string().contains("must contain source_entities"));
    }

    #[test]
    fn source_entity_dedupe_fails_on_conflicting_project_seed() {
        let left = SourceEntitySeed {
            entity_id: "society:rera-left".to_string(),
            alias_entity_id: None,
            name: "Left".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: Some("PRM-SAME".to_string()),
            latitude: None,
            longitude: None,
        };
        let mut right = left.clone();
        right.entity_id = "society:rera-right".to_string();

        let error = dedupe_source_entities(vec![left, right]).unwrap_err();

        assert!(error.to_string().contains("conflicting source entity seed"));
    }

    #[test]
    fn source_entity_dedupe_merges_matching_project_seed_metadata() {
        let left = SourceEntitySeed {
            entity_id: "society:rera-folium".to_string(),
            alias_entity_id: None,
            name: "FOLIUM BY SUMADHURA PHASE-I".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: Some("PRM-FOLIUM".to_string()),
            latitude: None,
            longitude: None,
        };
        let right = SourceEntitySeed {
            entity_id: "society:rera-folium".to_string(),
            alias_entity_id: Some("society:folium-by-sumadhura-phase-i".to_string()),
            name: "Folium By Sumadhura Phase-I".to_string(),
            area: Some("Whitefield".to_string()),
            city: Some("bengaluru".to_string()),
            project_key: Some("PRM-FOLIUM".to_string()),
            latitude: Some(12.971234567),
            longitude: Some(77.751234567),
        };

        let merged = dedupe_source_entities(vec![left, right]).unwrap();

        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].alias_entity_id.as_deref(),
            Some("society:folium-by-sumadhura-phase-i")
        );
        assert_eq!(merged[0].area.as_deref(), Some("Whitefield"));
        assert_eq!(merged[0].latitude, Some(12.971234567));
    }

    #[test]
    fn source_entity_dedupe_rejects_partial_coordinate_pairs() {
        let seed = SourceEntitySeed {
            entity_id: "society:rera-partial".to_string(),
            alias_entity_id: None,
            name: "Partial Coordinates".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: Some("PRM-PARTIAL".to_string()),
            latitude: Some(12.9),
            longitude: None,
        };

        let error = dedupe_source_entities(vec![seed]).unwrap_err();

        assert!(error
            .to_string()
            .contains("must provide latitude and longitude together"));
    }

    #[test]
    fn explicit_registration_selectors_preserve_each_project_mapping() {
        assert_eq!(
            source_entity_seed_key("society:shared", "PRM-PHASE-1", true),
            "PRM-PHASE-1"
        );
        assert_eq!(
            source_entity_seed_key("society:shared", "PRM-PHASE-2", true),
            "PRM-PHASE-2"
        );
        assert_eq!(
            source_entity_seed_key("society:shared", "PRM-PHASE-2", false),
            "society:shared"
        );
    }

    #[test]
    fn scoped_source_input_keeps_embedded_identity_scope() {
        let embedded = SourceEntitySeed {
            entity_id: "society:rera-embedded".to_string(),
            alias_entity_id: None,
            name: "Embedded Society".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: Some("PRM-EMBEDDED".to_string()),
            latitude: None,
            longitude: None,
        };
        let mut source_inputs = AssetSourceInputs {
            source_entities: vec![embedded.clone()],
            ..AssetSourceInputs::default()
        };

        merge_source_input_entities(&mut source_inputs, Vec::new()).unwrap();

        assert_eq!(source_inputs.source_entities, vec![embedded]);
    }

    #[tokio::test]
    async fn source_entity_selection_fails_closed_without_a_canonical_snapshot() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();

        let error =
            current_source_entities(&lake, None, &["society:prestige-raintree-park".to_string()])
                .await
                .unwrap_err();

        assert!(error
            .to_string()
            .contains("requires an existing canonical society snapshot"));
    }

    #[tokio::test]
    async fn resumed_source_entity_selection_fails_closed_without_a_canonical_step() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let executor = AssetDagExecutor::new(openestates_registry(), lake.clone());
        let run_partition =
            AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
        let plan = executor.plan(&run_partition, now).await.unwrap();
        let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "resume-v1");
        manifest
            .steps
            .retain(|step| step.asset_id.as_str() != CANONICAL_SOCIETY_NODES_ASSET_ID);

        let error = current_source_entities(
            &lake,
            Some(&manifest),
            &["society:prestige-raintree-park".to_string()],
        )
        .await
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("requires an existing canonical society snapshot"));
    }

    #[tokio::test]
    async fn resumed_source_entity_selection_fails_closed_without_a_snapshot_id() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let executor = AssetDagExecutor::new(openestates_registry(), lake.clone());
        let run_partition =
            AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
        let plan = executor.plan(&run_partition, now).await.unwrap();
        let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "resume-v1");
        let canonical = manifest
            .steps
            .iter_mut()
            .find(|step| step.asset_id.as_str() == CANONICAL_SOCIETY_NODES_ASSET_ID)
            .unwrap();
        canonical.materialization_id = None;
        canonical.current_materialization_id = None;

        let error = current_source_entities(
            &lake,
            Some(&manifest),
            &["society:prestige-raintree-park".to_string()],
        )
        .await
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("requires an existing canonical society snapshot"));
    }

    #[tokio::test]
    async fn resumed_collection_seeds_entities_from_the_run_snapshot() {
        let temp = tempdir().unwrap();
        let lake = LakeStore::local(temp.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let old_canonical = canonical_fixture(&lake, "Old Snapshot Society", "old", now).await;
        let current_canonical =
            canonical_fixture(&lake, "Current Pointer Society", "current", now).await;
        AssetMaterializationStore::new(lake.clone())
            .promote_current(&current_canonical)
            .await
            .unwrap();

        let executor = AssetDagExecutor::new(openestates_registry(), lake.clone());
        let run_partition =
            AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);
        let plan = executor.plan(&run_partition, now).await.unwrap();
        let mut manifest = AssetDagRunManifest::from_plan_with_version(&plan, "resume-v1");
        manifest
            .steps
            .iter_mut()
            .find(|step| step.asset_id.as_str() == CANONICAL_SOCIETY_NODES_ASSET_ID)
            .unwrap()
            .current_materialization_id = Some(old_canonical.materialization_id.clone());

        let resumed = current_source_entities(&lake, Some(&manifest), &[])
            .await
            .unwrap();
        let live = current_source_entities(&lake, None, &[]).await.unwrap();

        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].name, "Old Snapshot Society");
        assert!(resumed[0].entity_id.starts_with("society:rera-"));
        assert_eq!(
            resumed[0].alias_entity_id.as_deref(),
            Some("society:old-snapshot-society")
        );
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].name, "Current Pointer Society");
        assert!(live[0].entity_id.starts_with("society:rera-"));
        assert_eq!(
            live[0].alias_entity_id.as_deref(),
            Some("society:current-pointer-society")
        );

        let selected = current_source_entities(
            &lake,
            Some(&manifest),
            &["society:old-snapshot-society".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert!(selected[0].entity_id.starts_with("society:rera-"));
        assert_eq!(
            selected[0].alias_entity_id.as_deref(),
            Some("society:old-snapshot-society")
        );
        assert_eq!(selected[0].project_key.as_deref(), Some("PRM-old"));

        let excluded = current_source_entities(
            &lake,
            Some(&manifest),
            &["society:not-in-this-run".to_string()],
        )
        .await
        .unwrap_err();
        assert!(excluded
            .to_string()
            .contains("unknown --source-entity selector"));

        manifest
            .steps
            .iter_mut()
            .find(|step| step.asset_id.as_str() == CANONICAL_SOCIETY_NODES_ASSET_ID)
            .unwrap()
            .current_materialization_id = Some(MaterializationId::new());
        assert!(current_source_entities(&lake, Some(&manifest), &[])
            .await
            .is_err());
    }

    async fn canonical_fixture(
        lake: &LakeStore,
        project_name: &str,
        suffix: &str,
        now: chrono::DateTime<Utc>,
    ) -> backend::assets::MaterializationRecord {
        let rera = ReraRegistryMaterializer::new(lake.clone())
            .materialize_for_run(
                &ReraRegistryMonthlyInput {
                    snapshot_date: "2026-07".to_string(),
                    projects: vec![ReraProjectSnapshotRecord {
                        ack_number: Some(format!("ACK-{suffix}")),
                        registration_number: Some(format!("PRM-{suffix}")),
                        project_name: project_name.to_string(),
                        promoter_name: None,
                        status: Some("Approved".to_string()),
                        project_type: None,
                        project_address: None,
                        area_name: Some("Whitefield".to_string()),
                        district: None,
                        taluk: None,
                        total_land_area_sqm: None,
                        land_litigation: None,
                        source_url: format!("https://rera.karnataka.gov.in/{suffix}"),
                        fetched_at: now,
                    }],
                    detail_facts: Vec::new(),
                    detail_fact_annotations: Vec::new(),
                    source_watermarks: Vec::new(),
                },
                MaterializationId::new(),
                AssetPartition::global(),
            )
            .await
            .unwrap();
        CanonicalSocietyMaterializer::new(lake.clone())
            .materialize_from_rera_for_run(
                &rera,
                suffix,
                MaterializationId::new(),
                AssetPartition::global(),
            )
            .await
            .unwrap()
    }
}
