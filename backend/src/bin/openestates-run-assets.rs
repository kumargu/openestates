use std::path::PathBuf;

use backend::assets::{
    default_openestates_registry, AssetDagExecutionOptions, AssetDagExecutor, AssetPartition,
    AssetSourceInputs,
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
    if let Some(source_inputs) = cli.load_source_inputs(&lake).await? {
        options = options.with_source_inputs(source_inputs);
    }

    let executor = AssetDagExecutor::new(default_openestates_registry(), lake);
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

#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    partition_parts: Vec<(String, String)>,
    version: Option<String>,
    source_inputs_path: Option<PathBuf>,
    source_inputs_key: Option<String>,
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

        if options.source_inputs_path.is_some() && options.source_inputs_key.is_some() {
            return Err("use either --source-inputs or --source-input-key, not both".to_string());
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

    async fn load_source_inputs(
        &self,
        lake: &LakeStore,
    ) -> Result<Option<AssetSourceInputs>, Box<dyn std::error::Error>> {
        if let Some(path) = &self.source_inputs_path {
            let bytes = std::fs::read(path)?;
            let inputs = serde_json::from_slice(&bytes)?;
            return Ok(Some(inputs));
        }

        if let Some(key) = &self.source_inputs_key {
            let lake_key = LakeKey::new(key.clone())?;
            let inputs = lake.get_json(&lake_key).await?;
            return Ok(Some(inputs));
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
        "  cargo run --bin openestates-run-assets -- [--project-root <path>] [--partition key=value]... [--version <version>] [--dry-run]"
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
}
