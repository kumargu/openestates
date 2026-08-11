use std::path::PathBuf;

use backend::assets::{
    promote_search_serving_release, AssetId, AssetMaterializationStore, MaterializationId,
};
use backend::lake::{LakeStoreLocation, LAKE_URL_ENV};
use backend::serving::{
    validate_search_serving_candidate, write_frontend_media_manifest,
    SEARCH_SERVING_BUNDLE_ASSET_ID,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliOptions::parse()?;
    let project_root = cli.project_root.unwrap_or_else(default_project_root);
    let lake = LakeStoreLocation::from_env(&project_root)?.open()?;
    let store = AssetMaterializationStore::new(lake.clone());
    let record = match cli.materialization_id.as_ref() {
        Some(materialization_id) => store
            .record_by_id_for_asset(&cli.asset_id, materialization_id)
            .await?
            .ok_or_else(|| {
                format!(
                    "materialization {materialization_id} was not found for asset {}",
                    cli.asset_id
                )
            })?,
        None => {
            store
                .current_record(&cli.asset_id, &backend::assets::AssetPartition::global())
                .await?
        }
    };
    let validation = if record.asset_id.as_str() == SEARCH_SERVING_BUNDLE_ASSET_ID {
        Some(validate_search_serving_candidate(&lake, &record).await?)
    } else {
        None
    };
    if let Some(validation) = validation.as_ref() {
        if !validation.passed {
            println!("{}", serde_json::to_string_pretty(validation)?);
            return Err(format!(
                "serving bundle validation failed with {} issue(s); no pointers were changed",
                validation.issues.len()
            )
            .into());
        }
        if cli.sync_frontend_manifest || !cli.check_only {
            write_frontend_media_manifest(&project_root, validation)?;
        }
    }
    if cli.check_only {
        println!("{}", serde_json::to_string_pretty(&validation)?);
        return Ok(());
    }

    let (promoted, release) = if record.asset_id.as_str() == SEARCH_SERVING_BUNDLE_ASSET_ID {
        let release = promote_search_serving_release(&store, &record, cli.force).await?;
        (true, Some(release))
    } else if cli.force {
        store.force_promote_current(&record).await?;
        (true, None)
    } else {
        (store.promote_current(&record).await?, None)
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
            "validation": validation,
            "release": release.map(|release| serde_json::json!({
                "kg_society_view_materialization_id": release.kg_materialization_id,
                "current_project_facts_materialization_id": release.current_project_facts_materialization_id,
                "promoted_materialization_count": release.promoted_materializations.len(),
            })),
        })
    );
    Ok(())
}

struct CliOptions {
    project_root: Option<PathBuf>,
    asset_id: AssetId,
    materialization_id: Option<MaterializationId>,
    force: bool,
    check_only: bool,
    sync_frontend_manifest: bool,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut project_root = None;
        let mut asset_id = None;
        let mut materialization_id = None;
        let mut force = false;
        let mut current = false;
        let mut check_only = false;
        let mut sync_frontend_manifest = false;
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
                "--current" => {
                    current = true;
                }
                "--force" => {
                    force = true;
                }
                "--check-only" => {
                    check_only = true;
                }
                "--sync-frontend-manifest" => {
                    sync_frontend_manifest = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        if current == materialization_id.is_some() {
            return Err("provide exactly one of --materialization <uuid> or --current".to_string());
        }
        if current && !check_only {
            return Err("--current is only allowed with --check-only".to_string());
        }
        if sync_frontend_manifest && !check_only {
            return Err("--sync-frontend-manifest is only needed with --check-only".to_string());
        }

        Ok(Self {
            project_root,
            asset_id: asset_id.ok_or_else(|| "--asset is required".to_string())?,
            materialization_id,
            force,
            check_only,
            sync_frontend_manifest,
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
    println!("Search serving bundles promote and validate their complete pinned lineage.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin openestates-promote-materialization -- --asset <asset-id> (--materialization <uuid> | --current --check-only)"
    );
    println!();
    println!("Options:");
    println!("  --project-root <path>  Project root used for local lake resolution");
    println!("  --asset <asset-id>     Asset id, e.g. image_media_facts");
    println!("  --materialization <id> Existing materialization UUID");
    println!("  --current              Validate the current global materialization");
    println!("  --force                Replace current pointer even when materialization is older");
    println!("  --check-only           Validate without changing any current pointer");
    println!("  --sync-frontend-manifest  Write frontend/media-manifest.json after validation");
    println!(
        "  {env_name:<22} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = LAKE_URL_ENV
    );
}
