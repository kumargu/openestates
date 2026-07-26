use std::path::PathBuf;

use backend::assets::{
    AssetId, AssetMaterializationStore, AssetPathBuilder, CurrentAssetPointer, MaterializationId,
};
use backend::lake::{LakeStoreLocation, LAKE_URL_ENV};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliOptions::parse()?;
    let project_root = cli.project_root.unwrap_or_else(default_project_root);
    let lake = LakeStoreLocation::from_env(&project_root)?.open()?;
    let store = AssetMaterializationStore::new(lake.clone());
    let record = store
        .record_by_id_for_asset(&cli.asset_id, &cli.materialization_id)
        .await?
        .ok_or_else(|| {
            format!(
                "materialization {} was not found for asset {}",
                cli.materialization_id, cli.asset_id
            )
        })?;
    let promoted = if cli.force {
        let pointer = CurrentAssetPointer {
            asset_id: record.asset_id.clone(),
            partition: record.partition.clone(),
            materialization_id: record.materialization_id.clone(),
            materialization_key: AssetPathBuilder::materialization_record_key(
                &record.asset_id,
                &record.partition,
                &record.materialization_id,
            )
            .to_string(),
            version: record.version.clone(),
            run_id: Some(record.run_id.clone()),
            run_created_at: Some(record.created_at),
            updated_at: Utc::now(),
        };
        lake.put_json(
            &AssetPathBuilder::current_pointer_key(&record.asset_id, &record.partition),
            &pointer,
        )
        .await?;
        true
    } else {
        store.promote_current(&record).await?
    };
    println!(
        "{}",
        serde_json::json!({
            "asset_id": record.asset_id,
            "partition": record.partition,
            "materialization_id": record.materialization_id,
            "version": record.version,
            "row_count": record.row_count,
            "promoted": promoted,
        })
    );
    Ok(())
}

struct CliOptions {
    project_root: Option<PathBuf>,
    asset_id: AssetId,
    materialization_id: MaterializationId,
    force: bool,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut project_root = None;
        let mut asset_id = None;
        let mut materialization_id = None;
        let mut force = false;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--project-root requires a value".to_string())?;
                    project_root = Some(PathBuf::from(value));
                }
                "--asset" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--asset requires an asset id".to_string())?;
                    asset_id = Some(
                        AssetId::new(value.trim())
                            .map_err(|_| format!("--asset requires a valid asset id: {value}"))?,
                    );
                }
                "--materialization" => {
                    let value = args.next().ok_or_else(|| {
                        "--materialization requires a materialization UUID".to_string()
                    })?;
                    materialization_id = Some(value.parse().map_err(|_| {
                        format!("--materialization requires a valid UUID: {value}")
                    })?);
                }
                "--force" => {
                    force = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(Self {
            project_root,
            asset_id: asset_id.ok_or_else(|| "--asset is required".to_string())?,
            materialization_id: materialization_id
                .ok_or_else(|| "--materialization is required".to_string())?,
            force,
        })
    }
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

fn print_help() {
    println!("Promote an existing OpenEstates asset materialization to current.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin openestates-promote-materialization -- --asset <asset-id> --materialization <uuid>"
    );
    println!();
    println!("Options:");
    println!("  --project-root <path>  Project root used for local lake resolution");
    println!("  --asset <asset-id>     Asset id, e.g. image_media_facts");
    println!("  --materialization <id> Existing materialization UUID");
    println!("  --force                Replace current pointer even when materialization is older");
    println!(
        "  {env_name:<22} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = LAKE_URL_ENV
    );
}
