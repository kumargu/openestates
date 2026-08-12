use std::fs;
use std::path::PathBuf;

use backend::assets::{
    AssetId, AssetMaterializationStore, CatalogEnvironment, CatalogMembership, CatalogRelease,
    CatalogReleaseId, CatalogReleaseStore, CatalogTombstone, CatalogValidationStatus,
    DerivedCatalogAssets, PinnedMaterialization, PromoteCatalogReleaseOptions,
};
use backend::data_loader::properties_from_serving_bundle;
use backend::lake::{LakeStoreLocation, LAKE_URL_ENV};
use backend::serving::{
    validate_search_serving_candidate, write_frontend_media_manifest, ServingBundleLoader,
    SEARCH_SERVING_BUNDLE_ASSET_ID,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = CliOptions::parse()?;
    let project_root = cli.project_root.unwrap_or_else(default_project_root);
    let lake = LakeStoreLocation::from_env(&project_root)?.open()?;
    let store = CatalogReleaseStore::new(lake.clone());

    match cli.command {
        Command::Create(options) => {
            let memberships = if options.membership_from_serving {
                let cache_root = project_root.join("data").join("cache").join("serving");
                let loader = ServingBundleLoader::new(lake.clone(), cache_root);
                let bundle = loader
                    .load_search_bundle_by_materialization(&options.serving_materialization_id)
                    .await?
                    .ok_or_else(|| {
                        format!(
                            "serving materialization {} was not found",
                            options.serving_materialization_id
                        )
                    })?;
                memberships_from_serving_bundle(&bundle)
            } else {
                options.memberships
            };
            let mut release = CatalogRelease::candidate(
                options.base_release_id,
                options.description,
                DerivedCatalogAssets {
                    serving_materialization_id: options.serving_materialization_id,
                    kg_materialization_id: options.kg_materialization_id,
                    project_facts_materialization_id: options.project_facts_materialization_id,
                    materializations: Vec::new(),
                },
            );
            release.changes.added_societies = options.added_societies;
            release.changes.refreshed_societies = options.refreshed_societies;
            release.changes.removed_societies = options.removed_societies;
            release.pinned_inputs = options.pinned_inputs;
            release.memberships = memberships;
            release.tombstones = options.tombstones;
            store.write_release(&release).await?;
            println!("{}", serde_json::to_string_pretty(&release)?);
        }
        Command::Write { release_file } => {
            let release: CatalogRelease = serde_json::from_slice(
                &fs::read(&release_file)
                    .map_err(|err| format!("failed to read {}: {err}", release_file.display()))?,
            )?;
            store.write_release(&release).await?;
            println!("{}", serde_json::to_string_pretty(&release)?);
        }
        Command::Validate { release_id } => {
            let release = store.validate_release(&release_id).await?;
            validate_release_artifacts(&store, &lake, &project_root, &release_id, false).await?;
            println!("{}", serde_json::to_string_pretty(&release)?);
            if release.validation_status == CatalogValidationStatus::Rejected {
                std::process::exit(1);
            }
        }
        Command::Promote {
            release_id,
            environment,
            expected_current_release,
            approve_production,
            force_legacy_pointer,
        } => {
            validate_release_artifacts(&store, &lake, &project_root, &release_id, true).await?;
            let pointer = store
                .promote_environment(
                    &release_id,
                    environment,
                    PromoteCatalogReleaseOptions {
                        expected_current_release,
                        approve_production,
                        force_legacy_pointer,
                    },
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&pointer)?);
        }
        Command::Rollback {
            environment,
            release_id,
            expected_current_release,
            approve_production,
            force_legacy_pointer,
        } => {
            validate_release_artifacts(&store, &lake, &project_root, &release_id, true).await?;
            let pointer = store
                .rollback_environment(
                    environment,
                    &release_id,
                    PromoteCatalogReleaseOptions {
                        expected_current_release,
                        approve_production,
                        force_legacy_pointer,
                    },
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&pointer)?);
        }
        Command::InspectRelease { release_id } => {
            let release = store.release(&release_id).await?;
            println!("{}", serde_json::to_string_pretty(&release)?);
        }
        Command::InspectEnvironment { environment } => {
            let pointer = store.current_pointer(environment).await?;
            println!("{}", serde_json::to_string_pretty(&pointer)?);
        }
    }
    Ok(())
}

async fn validate_release_artifacts(
    releases: &CatalogReleaseStore,
    lake: &backend::lake::LakeStore,
    project_root: &std::path::Path,
    release_id: &CatalogReleaseId,
    sync_frontend_manifest: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let release = releases.release(release_id).await?;
    let asset_id = AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
    let materializations = AssetMaterializationStore::new(lake.clone());
    let record = materializations
        .record_by_id_for_asset(
            &asset_id,
            &release.derived_assets.serving_materialization_id,
        )
        .await?
        .ok_or_else(|| {
            format!(
                "serving materialization {} was not found",
                release.derived_assets.serving_materialization_id
            )
        })?;
    let report = validate_search_serving_candidate(lake, &record).await?;
    if !report.passed {
        return Err(format!(
            "catalog release {release_id} failed {} serving artifact gate(s): {}",
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
        )
        .into());
    }
    if sync_frontend_manifest {
        write_frontend_media_manifest(project_root, &report)?;
    }
    Ok(())
}

struct CliOptions {
    project_root: Option<PathBuf>,
    command: Command,
}

#[allow(clippy::large_enum_variant)]
enum Command {
    Create(CreateOptions),
    Write {
        release_file: PathBuf,
    },
    Validate {
        release_id: CatalogReleaseId,
    },
    Promote {
        release_id: CatalogReleaseId,
        environment: CatalogEnvironment,
        expected_current_release: Option<CatalogReleaseId>,
        approve_production: bool,
        force_legacy_pointer: bool,
    },
    Rollback {
        environment: CatalogEnvironment,
        release_id: CatalogReleaseId,
        expected_current_release: Option<CatalogReleaseId>,
        approve_production: bool,
        force_legacy_pointer: bool,
    },
    InspectRelease {
        release_id: CatalogReleaseId,
    },
    InspectEnvironment {
        environment: CatalogEnvironment,
    },
}

struct CreateOptions {
    base_release_id: Option<CatalogReleaseId>,
    description: Option<String>,
    serving_materialization_id: backend::assets::MaterializationId,
    kg_materialization_id: backend::assets::MaterializationId,
    project_facts_materialization_id: backend::assets::MaterializationId,
    pinned_inputs: Vec<PinnedMaterialization>,
    memberships: Vec<CatalogMembership>,
    membership_from_serving: bool,
    tombstones: Vec<CatalogTombstone>,
    added_societies: Vec<String>,
    refreshed_societies: Vec<String>,
    removed_societies: Vec<String>,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut project_root = None;
        let mut args = std::env::args().skip(1).collect::<Vec<_>>();
        let mut cursor = 0usize;
        while cursor < args.len() {
            if args[cursor] != "--project-root" {
                break;
            }
            cursor += 1;
            project_root = Some(PathBuf::from(take_value(
                &args,
                &mut cursor,
                "--project-root",
            )?));
        }
        if cursor >= args.len() {
            print_help();
            return Err("missing command".to_string());
        }
        let command_name = args[cursor].clone();
        cursor += 1;
        let command_args = args.split_off(cursor);
        let command = parse_command(&command_name, command_args)?;
        Ok(Self {
            project_root,
            command,
        })
    }
}

fn parse_command(command: &str, args: Vec<String>) -> Result<Command, String> {
    match command {
        "create" => parse_create(args).map(Command::Create),
        "write" => {
            let mut cursor = 0usize;
            let mut release_file = None;
            while cursor < args.len() {
                match args[cursor].as_str() {
                    "--release-file" => {
                        cursor += 1;
                        release_file = Some(PathBuf::from(take_value(
                            &args,
                            &mut cursor,
                            "--release-file",
                        )?));
                    }
                    other => return Err(format!("unknown write argument: {other}")),
                }
            }
            Ok(Command::Write {
                release_file: release_file
                    .ok_or_else(|| "write requires --release-file".to_string())?,
            })
        }
        "validate" => {
            let mut cursor = 0usize;
            let release_id = parse_required_release_arg(&args, &mut cursor)?;
            ensure_consumed(&args, cursor)?;
            Ok(Command::Validate { release_id })
        }
        "promote" => parse_promote(args, false),
        "rollback" => parse_promote(args, true),
        "inspect" => parse_inspect(args),
        "--help" | "-h" => {
            print_help();
            std::process::exit(0);
        }
        other => Err(format!("unknown command: {other}")),
    }
}

fn parse_create(args: Vec<String>) -> Result<CreateOptions, String> {
    let mut cursor = 0usize;
    let mut base_release_id = None;
    let mut description = None;
    let mut serving_materialization_id = None;
    let mut kg_materialization_id = None;
    let mut project_facts_materialization_id = None;
    let mut pinned_inputs = Vec::new();
    let mut memberships = Vec::new();
    let mut membership_from_serving = false;
    let mut tombstones = Vec::new();
    let mut added_societies = Vec::new();
    let mut refreshed_societies = Vec::new();
    let mut removed_societies = Vec::new();

    while cursor < args.len() {
        match args[cursor].as_str() {
            "--base-release" => {
                cursor += 1;
                base_release_id = Some(parse_catalog_release_id(take_value(
                    &args,
                    &mut cursor,
                    "--base-release",
                )?)?);
            }
            "--description" => {
                cursor += 1;
                description = Some(take_value(&args, &mut cursor, "--description")?);
            }
            "--serving" => {
                cursor += 1;
                serving_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--serving",
                )?)?);
            }
            "--kg" => {
                cursor += 1;
                kg_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--kg",
                )?)?);
            }
            "--project-facts" => {
                cursor += 1;
                project_facts_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--project-facts",
                )?)?);
            }
            "--input" => {
                cursor += 1;
                pinned_inputs.push(parse_pinned_input(take_value(
                    &args,
                    &mut cursor,
                    "--input",
                )?)?);
            }
            "--membership-file" => {
                cursor += 1;
                memberships = read_json_file(take_value(&args, &mut cursor, "--membership-file")?)?;
            }
            "--membership-from-serving" => {
                cursor += 1;
                membership_from_serving = true;
            }
            "--tombstone-file" => {
                cursor += 1;
                tombstones = read_json_file(take_value(&args, &mut cursor, "--tombstone-file")?)?;
            }
            "--added-society" => {
                cursor += 1;
                added_societies.push(take_value(&args, &mut cursor, "--added-society")?);
            }
            "--refreshed-society" => {
                cursor += 1;
                refreshed_societies.push(take_value(&args, &mut cursor, "--refreshed-society")?);
            }
            "--removed-society" => {
                cursor += 1;
                removed_societies.push(take_value(&args, &mut cursor, "--removed-society")?);
            }
            other => return Err(format!("unknown create argument: {other}")),
        }
    }

    Ok(CreateOptions {
        base_release_id,
        description,
        serving_materialization_id: serving_materialization_id
            .ok_or_else(|| "create requires --serving".to_string())?,
        kg_materialization_id: kg_materialization_id
            .ok_or_else(|| "create requires --kg".to_string())?,
        project_facts_materialization_id: project_facts_materialization_id
            .ok_or_else(|| "create requires --project-facts".to_string())?,
        pinned_inputs,
        memberships,
        membership_from_serving,
        tombstones,
        added_societies,
        refreshed_societies,
        removed_societies,
    })
}

fn parse_promote(args: Vec<String>, rollback: bool) -> Result<Command, String> {
    let mut cursor = 0usize;
    let mut release_id = None;
    let mut environment = None;
    let mut expected_current_release = None;
    let mut approve_production = false;
    let mut force_legacy_pointer = false;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--release" => {
                cursor += 1;
                release_id = Some(parse_catalog_release_id(take_value(
                    &args,
                    &mut cursor,
                    "--release",
                )?)?);
            }
            "--env" => {
                cursor += 1;
                environment = Some(parse_environment(take_value(&args, &mut cursor, "--env")?)?);
            }
            "--expected-current" => {
                cursor += 1;
                expected_current_release = Some(parse_catalog_release_id(take_value(
                    &args,
                    &mut cursor,
                    "--expected-current",
                )?)?);
            }
            "--approve-production" => {
                cursor += 1;
                approve_production = true;
            }
            "--force-legacy-pointer" => {
                cursor += 1;
                force_legacy_pointer = true;
            }
            other => return Err(format!("unknown promotion argument: {other}")),
        }
    }
    let release_id = release_id.ok_or_else(|| "promotion requires --release".to_string())?;
    let environment = environment.ok_or_else(|| "promotion requires --env".to_string())?;
    if rollback {
        Ok(Command::Rollback {
            environment,
            release_id,
            expected_current_release,
            approve_production,
            force_legacy_pointer,
        })
    } else {
        Ok(Command::Promote {
            release_id,
            environment,
            expected_current_release,
            approve_production,
            force_legacy_pointer,
        })
    }
}

fn parse_inspect(args: Vec<String>) -> Result<Command, String> {
    let mut cursor = 0usize;
    let mut release_id = None;
    let mut environment = None;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--release" => {
                cursor += 1;
                release_id = Some(parse_catalog_release_id(take_value(
                    &args,
                    &mut cursor,
                    "--release",
                )?)?);
            }
            "--env" => {
                cursor += 1;
                environment = Some(parse_environment(take_value(&args, &mut cursor, "--env")?)?);
            }
            other => return Err(format!("unknown inspect argument: {other}")),
        }
    }
    match (release_id, environment) {
        (Some(release_id), None) => Ok(Command::InspectRelease { release_id }),
        (None, Some(environment)) => Ok(Command::InspectEnvironment { environment }),
        _ => Err("inspect requires exactly one of --release or --env".to_string()),
    }
}

fn parse_required_release_arg(
    args: &[String],
    cursor: &mut usize,
) -> Result<CatalogReleaseId, String> {
    let mut release_id = None;
    while *cursor < args.len() {
        match args[*cursor].as_str() {
            "--release" => {
                *cursor += 1;
                release_id = Some(parse_catalog_release_id(take_value(
                    args,
                    cursor,
                    "--release",
                )?)?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    release_id.ok_or_else(|| "--release is required".to_string())
}

fn take_value(args: &[String], cursor: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*cursor)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *cursor += 1;
    Ok(value)
}

fn ensure_consumed(args: &[String], cursor: usize) -> Result<(), String> {
    if cursor == args.len() {
        Ok(())
    } else {
        Err(format!("unexpected argument {}", args[cursor]))
    }
}

fn parse_catalog_release_id(value: String) -> Result<CatalogReleaseId, String> {
    value
        .parse()
        .map_err(|err| format!("invalid catalog release id {value}: {err}"))
}

fn parse_materialization_id(value: String) -> Result<backend::assets::MaterializationId, String> {
    value
        .parse()
        .map_err(|err| format!("invalid materialization id {value}: {err}"))
}

fn parse_environment(value: String) -> Result<CatalogEnvironment, String> {
    value.parse()
}

fn parse_pinned_input(value: String) -> Result<PinnedMaterialization, String> {
    let (asset, materialization) = value.split_once(':').ok_or_else(|| {
        "--input must be formatted as <asset-id>:<materialization-uuid>".to_string()
    })?;
    Ok(PinnedMaterialization {
        asset_id: AssetId::new(asset)
            .map_err(|err| format!("invalid pinned input asset id {asset}: {err}"))?,
        materialization_id: parse_materialization_id(materialization.to_string())?,
    })
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: String) -> Result<T, String> {
    let path = PathBuf::from(path);
    let bytes =
        fs::read(&path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("failed to parse {} as JSON: {err}", path.display()))
}

fn memberships_from_serving_bundle(
    bundle: &backend::serving::LoadedServingBundle,
) -> Vec<CatalogMembership> {
    let mut memberships = properties_from_serving_bundle(bundle)
        .into_iter()
        .map(|property| CatalogMembership {
            society_id: property.society_id.clone(),
            property_id: Some(property.id.clone()),
            property_config_id: Some(property.id),
            rera_id: None,
            aliases: vec![property.society_id.replace('-', " ")],
            coordinates: None,
            source_materialization_id: None,
            membership_kind: backend::assets::CatalogMembershipKind::Reused,
        })
        .collect::<Vec<_>>();
    memberships.sort_by(|left, right| {
        left.society_id
            .cmp(&right.society_id)
            .then(left.property_id.cmp(&right.property_id))
    });
    memberships
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

fn print_help() {
    println!("Manage immutable OpenEstates catalog releases and environment pointers.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-catalog-release -- create --serving <uuid> --kg <uuid> --project-facts <uuid>");
    println!("  cargo run --bin openestates-catalog-release -- validate --release <uuid>");
    println!("  cargo run --bin openestates-catalog-release -- promote --release <uuid> --env <dev|staging|production>");
    println!("  cargo run --bin openestates-catalog-release -- inspect --release <uuid>");
    println!(
        "  cargo run --bin openestates-catalog-release -- inspect --env <dev|staging|production>"
    );
    println!("  cargo run --bin openestates-catalog-release -- rollback --release <uuid> --env <dev|staging|production>");
    println!();
    println!("Options:");
    println!("  --project-root <path>       Project root used for local lake resolution");
    println!("  --base-release <uuid>       Base catalog release for an incremental candidate");
    println!("  --input <asset:uuid>        Pin an immutable input materialization; repeatable");
    println!("  --membership-file <path>    JSON array of CatalogMembership records");
    println!("  --membership-from-serving   Derive memberships from the pinned serving bundle");
    println!("  --tombstone-file <path>     JSON array of CatalogTombstone records");
    println!("  --expected-current <uuid>   Compare-and-swap expected environment release");
    println!("  --approve-production        Required for production promotion");
    println!("  --force-legacy-pointer      Force legacy global current pointers during production promotion");
    println!(
        "  {env_name:<27} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = LAKE_URL_ENV
    );
}
