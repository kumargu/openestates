use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use backend::assets::{
    default_openestates_registry, AssetDagExecutionOptions, AssetDagExecutor, AssetId,
    AssetMaterializationStore, AssetPartition, AssetSourceInputs, CommandSourceInputProvider,
    LakeObjectSourceInputProvider, LocalFileSourceInputProvider, SourceEntitySeed,
    SourceInputProvider, SourceInputRequest, CANONICAL_SOCIETY_NODES_ASSET_ID,
};
use backend::knowledge::{store as kg_store, KnowledgeGraph};
use backend::lake::{LakeKey, LakeStore};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliOptions::parse()?;
    let partition = cli.partition();
    let project_root = cli
        .project_root
        .clone()
        .unwrap_or_else(default_project_root);
    let graph = if cli.dry_run {
        KnowledgeGraph::new()
    } else {
        let kg_dir = kg_store::knowledge_dir(&project_root);
        kg_store::load_graph(&kg_dir).ok_or_else(|| {
            format!(
                "No knowledge graph found at {}. Seed or load KG before running assets.",
                kg_dir.display()
            )
        })?
    };

    let lake_root = project_root.join("data").join("lake");
    let lake = LakeStore::local(&lake_root)?;
    let planned_at = Utc::now();
    let mut options = AssetDagExecutionOptions::new(partition, planned_at).dry_run(cli.dry_run);
    if let Some(version) = cli.version.clone() {
        options = options.with_version(version);
    }
    let executor = AssetDagExecutor::new(default_openestates_registry(), lake.clone());
    if cli.dry_run && cli.source_command.is_some() {
        eprintln!("Skipping source collector command during dry-run.");
    } else if let Some(provider) = cli.source_input_provider()? {
        let plan = executor.plan(&options.partition, planned_at).await?;
        let collection_plan = AssetSourceInputs::collection_plan(&plan);
        let request = SourceInputRequest {
            project_root: project_root.clone(),
            partition: options.partition.clone(),
            planned_at,
            requested_assets: collection_plan.requested_assets,
            force_refresh_assets: collection_plan.force_refresh_assets,
            source_entities: current_source_entities(&lake).await?,
        };
        if let Some(source_inputs) = provider.load(&request, &lake).await? {
            options = options
                .with_source_inputs(source_inputs)
                .with_forced_assets(collection_plan.force_assets);
        }
    }

    let report = executor.execute(&graph, options).await?;

    if report.dry_run {
        eprintln!("Planned asset DAG run without writing manifests.");
    } else {
        eprintln!("Asset DAG run status: {:?}", report.manifest.status);
        eprintln!("Executed assets: {}", report.executed_assets.len());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}

async fn current_source_entities(
    lake: &LakeStore,
) -> Result<Vec<SourceEntitySeed>, Box<dyn std::error::Error>> {
    let store = AssetMaterializationStore::new(lake.clone());
    let asset_id = AssetId::new(CANONICAL_SOCIETY_NODES_ASSET_ID)?;
    let record = match store
        .current_record(&asset_id, &AssetPartition::global())
        .await
    {
        Ok(record) => record,
        Err(err) if err.is_not_found() => return Ok(Vec::new()),
        Err(err) => return Err(Box::new(err)),
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
    for mapping in rows.mappings {
        seeds
            .entry(mapping.canonical_entity_id.clone())
            .or_insert_with(|| SourceEntitySeed {
                entity_id: mapping.canonical_entity_id.clone(),
                name: mapping.project_name,
                area: areas.get(mapping.canonical_entity_id.as_str()).cloned(),
                city: Some("Bengaluru".to_string()),
                project_key: Some(mapping.project_key),
            });
    }
    Ok(seeds.into_values().collect())
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
        "  cargo run --bin openestates-run-assets -- [--project-root <path>] [--partition key=value]... [--version <version>] [--source-command <program> [--source-arg <arg>]... [--source-timeout-seconds <seconds>]] [--dry-run]"
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
}
