use std::path::PathBuf;

use backend::assets::{
    default_openestates_registry, AssetDagRunManifest, AssetMaterializationStore, AssetPartition,
    AssetPathBuilder, AssetPlanner, AssetRunManifestStore,
};
use backend::lake::LakeStoreLocation;
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let project_root = options
        .project_root
        .clone()
        .unwrap_or_else(default_project_root);
    let lake_location = LakeStoreLocation::from_env(&project_root)?;
    let lake = lake_location.open()?;
    let materializations = AssetMaterializationStore::new(lake.clone());
    let planner = AssetPlanner::new(default_openestates_registry(), materializations);
    let partition = options.partition();
    let plan = planner
        .plan_partition_details(&partition, Utc::now())
        .await?;
    let manifest = AssetDagRunManifest::from_plan(&plan);

    if options.write_manifest {
        let run_store = AssetRunManifestStore::new(lake);
        let meta = run_store.write_manifest(&manifest).await?;
        eprintln!("Wrote DAG run manifest: {}", meta.key);
        eprintln!(
            "Did not promote current pointer for a planned-only manifest: {}",
            AssetPathBuilder::current_dag_run_pointer_key(&manifest.partition)
        );
    }

    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    partition_parts: Vec<(String, String)>,
    write_manifest: bool,
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
                "--write-manifest" => {
                    options.write_manifest = true;
                }
                "--partition" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--partition requires key=value".to_string())?;
                    options.partition_parts.push(parse_partition_part(&value)?);
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
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
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

fn print_help() {
    println!("Plan the OpenEstates asset DAG and print a run manifest.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin openestates-plan-assets -- [--project-root <path>] [--partition key=value]... [--write-manifest]"
    );
    println!();
    println!("Options:");
    println!(
        "  --partition       Add a partition coordinate, e.g. --partition source=reddit --partition dt=2026-07-13"
    );
    println!(
        "  --write-manifest   Write the planned run manifest to the configured lake without promoting current.json"
    );
    println!(
        "  {env_name:<18} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = backend::lake::LAKE_URL_ENV
    );
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
