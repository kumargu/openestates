use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use backend::assets::{
    AssetId, AssetMaterializationStore, CatalogEnvironment, CatalogMembership, CatalogRelease,
    CatalogReleaseId, CatalogReleaseStore, CatalogTombstone, CatalogValidationStatus,
    DerivedCatalogAssets, MaterializationId, MaterializationRecord, PinnedMaterialization,
    PromoteCatalogReleaseOptions, SourceWatermark, IMAGE_MEDIA_FACTS_ASSET_ID,
    KG_SOCIETY_VIEW_ASSET_ID, RERA_CLAIMS_ASSET_ID, RERA_PROJECT_PLAN_FRAMES_ASSET_ID,
    RERA_RECEIPTS_ASSET_ID, RERA_SOURCE_RECORDS_ASSET_ID,
};
use backend::data_loader::properties_from_serving_bundle;
use backend::lake::{LakeKey, LakeStore, LakeStoreLocation, LAKE_URL_ENV};
use backend::serving::{
    read_edges_parquet, read_entities_parquet, read_facts_parquet, read_rera_evidence_parquet,
    read_search_metadata_parquet, validate_search_serving_candidate, write_frontend_media_manifest,
    SearchServingBundleMaterializer, ServingBundleLoader, ServingBundleManifest,
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
            let mut memberships = if options.membership_from_serving {
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
            if options.membership_from_serving {
                let added_societies = options
                    .added_societies
                    .iter()
                    .map(|society_id| normalized_catalog_society_id(society_id))
                    .collect::<BTreeSet<_>>();
                for membership in &mut memberships {
                    if added_societies
                        .contains(&normalized_catalog_society_id(&membership.society_id))
                    {
                        membership.membership_kind = backend::assets::CatalogMembershipKind::Added;
                    }
                }
            }
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
        Command::RebaseRera(options) => {
            let materialization = rebase_rera_serving(&lake, &store, options).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "manifest": materialization.manifest,
                    "record": materialization.record,
                }))?
            );
        }
        Command::ExtendServing(options) => {
            let materialization = extend_catalog_serving(&lake, &store, options).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "manifest": materialization.manifest,
                    "record": materialization.record,
                }))?
            );
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
    RebaseRera(RebaseReraOptions),
    ExtendServing(ExtendServingOptions),
}

struct RebaseReraOptions {
    base_release_id: CatalogReleaseId,
    evidence_serving_materialization_id: MaterializationId,
    media_facts_materialization_id: Option<MaterializationId>,
    plans_only: bool,
    version: String,
}

struct ExtendServingOptions {
    base_release_id: CatalogReleaseId,
    candidate_serving_materialization_id: MaterializationId,
    kg_materialization_id: Option<MaterializationId>,
    society_ids: Vec<String>,
    excluded_fact_keys: Vec<String>,
    refresh_existing: bool,
    version: String,
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
        "rebase-rera" => parse_rebase_rera(args).map(Command::RebaseRera),
        "extend-serving" => parse_extend_serving(args).map(Command::ExtendServing),
        "--help" | "-h" => {
            print_help();
            std::process::exit(0);
        }
        other => Err(format!("unknown command: {other}")),
    }
}

fn parse_extend_serving(args: Vec<String>) -> Result<ExtendServingOptions, String> {
    let mut cursor = 0usize;
    let mut base_release_id = None;
    let mut candidate_serving_materialization_id = None;
    let mut kg_materialization_id = None;
    let mut society_ids = Vec::new();
    let mut excluded_fact_keys = Vec::new();
    let mut refresh_existing = false;
    let mut version = None;
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
            "--candidate-serving" => {
                cursor += 1;
                candidate_serving_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--candidate-serving",
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
            "--society" => {
                cursor += 1;
                society_ids.push(take_value(&args, &mut cursor, "--society")?);
            }
            "--exclude-fact" => {
                cursor += 1;
                excluded_fact_keys.push(take_value(&args, &mut cursor, "--exclude-fact")?);
            }
            "--refresh-existing" => {
                cursor += 1;
                refresh_existing = true;
            }
            "--version" => {
                cursor += 1;
                version = Some(take_value(&args, &mut cursor, "--version")?);
            }
            other => return Err(format!("unknown extend-serving argument: {other}")),
        }
    }
    if society_ids.is_empty() {
        return Err("extend-serving requires at least one --society".to_string());
    }
    society_ids.sort();
    society_ids.dedup();
    excluded_fact_keys.sort();
    excluded_fact_keys.dedup();
    Ok(ExtendServingOptions {
        base_release_id: base_release_id
            .ok_or_else(|| "extend-serving requires --base-release".to_string())?,
        candidate_serving_materialization_id: candidate_serving_materialization_id
            .ok_or_else(|| "extend-serving requires --candidate-serving".to_string())?,
        kg_materialization_id,
        society_ids,
        excluded_fact_keys,
        refresh_existing,
        version: version.ok_or_else(|| "extend-serving requires --version".to_string())?,
    })
}

fn parse_rebase_rera(args: Vec<String>) -> Result<RebaseReraOptions, String> {
    let mut cursor = 0usize;
    let mut base_release_id = None;
    let mut evidence_serving_materialization_id = None;
    let mut media_facts_materialization_id = None;
    let mut plans_only = false;
    let mut version = None;
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
            "--evidence-serving" => {
                cursor += 1;
                evidence_serving_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--evidence-serving",
                )?)?);
            }
            "--media-facts" => {
                cursor += 1;
                media_facts_materialization_id = Some(parse_materialization_id(take_value(
                    &args,
                    &mut cursor,
                    "--media-facts",
                )?)?);
            }
            "--plans-only" => {
                cursor += 1;
                plans_only = true;
            }
            "--version" => {
                cursor += 1;
                version = Some(take_value(&args, &mut cursor, "--version")?);
            }
            other => return Err(format!("unknown rebase-rera argument: {other}")),
        }
    }
    Ok(RebaseReraOptions {
        base_release_id: base_release_id
            .ok_or_else(|| "rebase-rera requires --base-release".to_string())?,
        evidence_serving_materialization_id: evidence_serving_materialization_id
            .ok_or_else(|| "rebase-rera requires --evidence-serving".to_string())?,
        media_facts_materialization_id,
        plans_only,
        version: version.ok_or_else(|| "rebase-rera requires --version".to_string())?,
    })
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

fn normalized_catalog_society_id(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    if let Some(slug) = normalized.strip_prefix("soc-") {
        format!("society:{slug}")
    } else if normalized.starts_with("society:") {
        normalized
    } else {
        format!("society:{normalized}")
    }
}

async fn extend_catalog_serving(
    lake: &LakeStore,
    catalog_store: &CatalogReleaseStore,
    options: ExtendServingOptions,
) -> Result<backend::serving::SearchServingBundleMaterialization, Box<dyn std::error::Error>> {
    let base_release = catalog_store.release(&options.base_release_id).await?;
    if base_release.validation_status != CatalogValidationStatus::Validated {
        return Err(format!(
            "base catalog release {} is {:?}; validate it before extending the catalog",
            base_release.release_id, base_release.validation_status
        )
        .into());
    }

    let materializations = AssetMaterializationStore::new(lake.clone());
    let (base_record, base_manifest) = serving_record_and_manifest(
        lake,
        &materializations,
        &base_release.derived_assets.serving_materialization_id,
    )
    .await?;
    let (candidate_record, candidate_manifest) = serving_record_and_manifest(
        lake,
        &materializations,
        &options.candidate_serving_materialization_id,
    )
    .await?;
    let lineage_kg_materialization_id = match options.kg_materialization_id {
        Some(materialization_id) => {
            let kg_asset = AssetId::new(KG_SOCIETY_VIEW_ASSET_ID)
                .expect("static KG society-view asset id is valid");
            materializations
                .record_by_id_for_asset(&kg_asset, &materialization_id)
                .await?
                .ok_or_else(|| format!("KG materialization {materialization_id} was not found"))?;
            materialization_id
        }
        None => base_release.derived_assets.kg_materialization_id.clone(),
    };

    let mut entities = read_entities_parquet(
        &lake
            .get_bytes(&LakeKey::new(base_manifest.entity_parquet_key.clone())?)
            .await?,
    )?;
    let candidate_entities = read_entities_parquet(
        &lake
            .get_bytes(&LakeKey::new(
                candidate_manifest.entity_parquet_key.clone(),
            )?)
            .await?,
    )?;
    let requested_societies = options.society_ids.into_iter().collect::<BTreeSet<_>>();
    let candidate_societies = candidate_entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    if candidate_societies != requested_societies {
        return Err(format!(
            "candidate serving societies do not match requested additions; candidate={}, requested={}",
            candidate_societies.into_iter().collect::<Vec<_>>().join(", "),
            requested_societies.iter().cloned().collect::<Vec<_>>().join(", ")
        )
        .into());
    }

    let base_entity_ids = entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let candidate_entity_ids = candidate_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    if !options.refresh_existing {
        if let Some(existing) = requested_societies
            .iter()
            .find(|society_id| base_entity_ids.contains(*society_id))
        {
            return Err(
                format!("society {existing} already exists in the base serving bundle").into(),
            );
        }
    } else if let Some(missing) = requested_societies
        .iter()
        .find(|society_id| !base_entity_ids.contains(*society_id))
    {
        return Err(format!("society {missing} does not exist in the base serving bundle").into());
    }
    let added_entity_ids = candidate_entities
        .iter()
        .filter(|entity| !base_entity_ids.contains(&entity.entity_id))
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    if options.refresh_existing {
        entities.retain(|entity| !candidate_entity_ids.contains(&entity.entity_id));
        entities.extend(candidate_entities);
    } else {
        entities.extend(
            candidate_entities
                .into_iter()
                .filter(|entity| added_entity_ids.contains(&entity.entity_id)),
        );
    }
    let mut facts = read_facts_parquet(
        &lake
            .get_bytes(&LakeKey::new(base_manifest.fact_parquet_key.clone())?)
            .await?,
    )?;
    let candidate_facts = read_facts_parquet(
        &lake
            .get_bytes(&LakeKey::new(candidate_manifest.fact_parquet_key.clone())?)
            .await?,
    )?;
    let retired_entity_ids = if options.refresh_existing {
        replaced_linked_entity_ids(
            &facts,
            &candidate_facts,
            &base_entity_ids,
            &candidate_entity_ids,
        )
    } else {
        BTreeSet::new()
    };
    entities.retain(|entity| !retired_entity_ids.contains(&entity.entity_id));
    if options.refresh_existing {
        refresh_candidate_facts(&mut facts, candidate_facts, &candidate_entity_ids);
    } else {
        facts.extend(
            candidate_facts
                .into_iter()
                .filter(|fact| added_entity_ids.contains(&fact.entity_id)),
        );
    }
    facts.retain(|fact| !retired_entity_ids.contains(&fact.entity_id));
    let excluded_fact_keys = options
        .excluded_fact_keys
        .into_iter()
        .collect::<BTreeSet<_>>();
    facts.retain(|fact| !excluded_fact_keys.contains(&fact.fact_key));

    let mut search_metadata = read_search_metadata_parquet(
        &lake
            .get_bytes(&LakeKey::new(
                base_manifest.search_metadata_parquet_key.clone(),
            )?)
            .await?,
    )?;
    let candidate_search_metadata = read_search_metadata_parquet(
        &lake
            .get_bytes(&LakeKey::new(
                candidate_manifest.search_metadata_parquet_key.clone(),
            )?)
            .await?,
    )?;
    if options.refresh_existing {
        refresh_candidate_metadata(
            &mut search_metadata,
            candidate_search_metadata,
            &candidate_entity_ids,
        );
    } else {
        search_metadata.extend(
            candidate_search_metadata
                .into_iter()
                .filter(|metadata| added_entity_ids.contains(&metadata.entity_id)),
        );
    }
    search_metadata.retain(|row| !retired_entity_ids.contains(&row.entity_id));
    search_metadata.retain(|metadata| !excluded_fact_keys.contains(&metadata.fact_key));

    let merged_entity_ids = entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();

    let mut edges = match base_manifest.edge_parquet_key.as_ref() {
        Some(key) => read_edges_parquet(&lake.get_bytes(&LakeKey::new(key.clone())?).await?)?,
        None => Vec::new(),
    };
    let candidate_edges = match candidate_manifest.edge_parquet_key.as_ref() {
        Some(key) => read_edges_parquet(&lake.get_bytes(&LakeKey::new(key.clone())?).await?)?,
        None => Vec::new(),
    };
    edges.retain(|edge| {
        merged_entity_ids.contains(&edge.from_entity_id)
            && merged_entity_ids.contains(&edge.to_entity_id)
            && (!options.refresh_existing
                || !candidate_entity_ids.contains(&edge.from_entity_id)
                || !candidate_entity_ids.contains(&edge.to_entity_id))
    });
    let mut edge_keys = edges
        .iter()
        .map(|edge| {
            (
                edge.from_entity_id.clone(),
                edge.edge_type.clone(),
                edge.to_entity_id.clone(),
                edge.source_type.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    for edge in candidate_edges {
        let touches_added_entity = added_entity_ids.contains(&edge.from_entity_id)
            || added_entity_ids.contains(&edge.to_entity_id);
        let refreshes_candidate_edge = options.refresh_existing
            && candidate_entity_ids.contains(&edge.from_entity_id)
            && candidate_entity_ids.contains(&edge.to_entity_id);
        let endpoints_exist = merged_entity_ids.contains(&edge.from_entity_id)
            && merged_entity_ids.contains(&edge.to_entity_id);
        let edge_key = (
            edge.from_entity_id.clone(),
            edge.edge_type.clone(),
            edge.to_entity_id.clone(),
            edge.source_type.clone(),
        );
        if (touches_added_entity || refreshes_candidate_edge)
            && endpoints_exist
            && edge_keys.insert(edge_key)
        {
            edges.push(edge);
        }
    }

    let mut evidence = match base_manifest.rera_evidence_parquet_key.as_ref() {
        Some(key) => {
            read_rera_evidence_parquet(&lake.get_bytes(&LakeKey::new(key.clone())?).await?)?
        }
        None => Vec::new(),
    };
    let candidate_evidence_key = candidate_manifest
        .rera_evidence_parquet_key
        .as_ref()
        .ok_or("candidate serving bundle has no RERA evidence table")?;
    let candidate_evidence = read_rera_evidence_parquet(
        &lake
            .get_bytes(&LakeKey::new(candidate_evidence_key.clone())?)
            .await?,
    )?;
    let candidate_evidence_societies = candidate_evidence
        .iter()
        .map(|record| record.society_id.clone())
        .collect::<BTreeSet<_>>();
    if !requested_societies.is_subset(&candidate_evidence_societies) {
        return Err(
            "candidate serving bundle is missing RERA evidence for an added society".into(),
        );
    }
    if options.refresh_existing {
        evidence.retain(|record| !requested_societies.contains(&record.society_id));
    }
    evidence.extend(
        candidate_evidence
            .into_iter()
            .filter(|record| requested_societies.contains(&record.society_id)),
    );

    let included_societies = entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let mut excluded_rera_evidence_society_ids = base_manifest
        .excluded_rera_evidence_society_ids
        .into_iter()
        .chain(candidate_manifest.excluded_rera_evidence_society_ids)
        .filter(|society_id| !included_societies.contains(society_id))
        .collect::<Vec<_>>();
    excluded_rera_evidence_society_ids.sort();
    excluded_rera_evidence_society_ids.dedup();

    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_child_from_serving_records_with_rera_for_run(
            entities,
            facts,
            search_metadata,
            edges,
            evidence,
            excluded_rera_evidence_society_ids,
            options.version,
            vec![
                SourceWatermark {
                    source: "catalog_base_serving".to_string(),
                    high_watermark: base_record.materialization_id.to_string(),
                },
                SourceWatermark {
                    source: "catalog_extension_serving".to_string(),
                    high_watermark: candidate_record.materialization_id.to_string(),
                },
            ],
            // The merged bundle is the new serving commit point. Its two
            // input bundles remain immutable source watermarks, but cannot be
            // direct parents: doing so creates multiple global
            // search_serving_bundle materializations in one promotion
            // lineage. The base KG remains the default. A refresh may instead
            // pin an explicitly rebuilt KG so new project-fact lineage can be
            // promoted; the catalog gates validate that complete chain.
            vec![lineage_kg_materialization_id],
            MaterializationId::new(),
        )
        .await
        .map_err(Into::into)
}

fn refresh_candidate_facts(
    base: &mut Vec<backend::serving::ServingFactRecord>,
    candidate: Vec<backend::serving::ServingFactRecord>,
    entity_ids: &BTreeSet<String>,
) {
    let keys = candidate
        .iter()
        .filter(|fact| entity_ids.contains(&fact.entity_id))
        .map(|fact| (fact.entity_id.clone(), fact.fact_key.clone()))
        .collect::<BTreeSet<_>>();
    base.retain(|fact| !keys.contains(&(fact.entity_id.clone(), fact.fact_key.clone())));
    base.extend(
        candidate
            .into_iter()
            .filter(|fact| entity_ids.contains(&fact.entity_id)),
    );
}

fn replaced_linked_entity_ids(
    base_facts: &[backend::serving::ServingFactRecord],
    candidate_facts: &[backend::serving::ServingFactRecord],
    base_entity_ids: &BTreeSet<String>,
    candidate_entity_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut candidate_links = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for fact in candidate_facts {
        let backend::knowledge::FactValue::Text(target) = &fact.value else {
            continue;
        };
        if !candidate_entity_ids.contains(&fact.entity_id) || !candidate_entity_ids.contains(target)
        {
            continue;
        }
        candidate_links
            .entry((fact.entity_id.clone(), fact.fact_key.clone()))
            .or_default()
            .insert(target.clone());
    }
    let externally_linked = base_facts
        .iter()
        .filter(|fact| !candidate_entity_ids.contains(&fact.entity_id))
        .filter_map(|fact| match &fact.value {
            backend::knowledge::FactValue::Text(target) if base_entity_ids.contains(target) => {
                Some(target.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    base_facts
        .iter()
        .filter_map(|fact| {
            let candidate_targets =
                candidate_links.get(&(fact.entity_id.clone(), fact.fact_key.clone()))?;
            let backend::knowledge::FactValue::Text(target) = &fact.value else {
                return None;
            };
            if !base_entity_ids.contains(target)
                || candidate_entity_ids.contains(target)
                || candidate_targets.contains(target)
            {
                return None;
            }
            (!externally_linked.contains(target)).then(|| target.clone())
        })
        .collect()
}

fn refresh_candidate_metadata(
    base: &mut Vec<backend::serving::ServingSearchMetadataRecord>,
    candidate: Vec<backend::serving::ServingSearchMetadataRecord>,
    entity_ids: &BTreeSet<String>,
) {
    let keys = candidate
        .iter()
        .filter(|row| entity_ids.contains(&row.entity_id))
        .map(|row| (row.entity_id.clone(), row.fact_key.clone()))
        .collect::<BTreeSet<_>>();
    base.retain(|row| !keys.contains(&(row.entity_id.clone(), row.fact_key.clone())));
    base.extend(
        candidate
            .into_iter()
            .filter(|row| entity_ids.contains(&row.entity_id)),
    );
}

async fn rebase_rera_serving(
    lake: &LakeStore,
    catalog_store: &CatalogReleaseStore,
    options: RebaseReraOptions,
) -> Result<backend::serving::SearchServingBundleMaterialization, Box<dyn std::error::Error>> {
    let base_release = catalog_store.release(&options.base_release_id).await?;
    if base_release.validation_status != CatalogValidationStatus::Validated {
        return Err(format!(
            "base catalog release {} is {:?}; validate it before rebasing RERA evidence",
            base_release.release_id, base_release.validation_status
        )
        .into());
    }

    let materializations = AssetMaterializationStore::new(lake.clone());
    let (base_record, base_manifest) = serving_record_and_manifest(
        lake,
        &materializations,
        &base_release.derived_assets.serving_materialization_id,
    )
    .await?;
    let (evidence_record, evidence_manifest) = serving_record_and_manifest(
        lake,
        &materializations,
        &options.evidence_serving_materialization_id,
    )
    .await?;

    let entities = read_entities_parquet(
        &lake
            .get_bytes(&LakeKey::new(base_manifest.entity_parquet_key.clone())?)
            .await?,
    )?;
    let mut facts = read_facts_parquet(
        &lake
            .get_bytes(&LakeKey::new(base_manifest.fact_parquet_key.clone())?)
            .await?,
    )?;
    let mut search_metadata = read_search_metadata_parquet(
        &lake
            .get_bytes(&LakeKey::new(
                base_manifest.search_metadata_parquet_key.clone(),
            )?)
            .await?,
    )?;
    let edges = match base_manifest.edge_parquet_key.as_ref() {
        Some(key) => read_edges_parquet(&lake.get_bytes(&LakeKey::new(key.clone())?).await?)?,
        None => Vec::new(),
    };
    let evidence_key = if options.plans_only {
        base_manifest
            .rera_evidence_parquet_key
            .as_ref()
            .ok_or("base serving bundle has no RERA evidence table")?
    } else {
        evidence_manifest
            .rera_evidence_parquet_key
            .as_ref()
            .ok_or("evidence serving bundle has no RERA evidence table")?
    };
    let evidence =
        read_rera_evidence_parquet(&lake.get_bytes(&LakeKey::new(evidence_key.clone())?).await?)?;
    if evidence.is_empty() {
        return Err("evidence serving bundle has no RERA evidence rows".into());
    }
    let excluded_rera_evidence_society_ids = if options.plans_only {
        base_manifest.excluded_rera_evidence_society_ids.clone()
    } else {
        evidence_manifest.excluded_rera_evidence_society_ids.clone()
    };

    let catalog_subject_ids =
        catalog_rera_subject_ids(&base_release.memberships, &entities, &edges);
    let catalog_society_ids = catalog_society_ids(&entities);
    let evidence_facts = read_facts_parquet(
        &lake
            .get_bytes(&LakeKey::new(evidence_manifest.fact_parquet_key.clone())?)
            .await?,
    )?;
    let evidence_search_metadata = read_search_metadata_parquet(
        &lake
            .get_bytes(&LakeKey::new(
                evidence_manifest.search_metadata_parquet_key.clone(),
            )?)
            .await?,
    )?;

    let plan_record = ancestor_materialization_for_asset(
        &materializations,
        &evidence_record,
        RERA_PROJECT_PLAN_FRAMES_ASSET_ID,
    )
    .await?
    .ok_or("evidence serving bundle has no RERA project-plan ancestor")?;
    let plan_fact_key = plan_record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("/facts/part-00000.parquet"))
        .map(|artifact| artifact.key.clone())
        .ok_or("RERA project-plan materialization has no fact artifact")?;
    let plan_facts = read_facts_parquet(&lake.get_bytes(&LakeKey::new(plan_fact_key)?).await?)?;
    let media_record = if options.plans_only {
        rebase_plan_facts(&mut facts, plan_facts, &catalog_society_ids);
        None
    } else {
        rebase_source_facts(
            &mut facts,
            &mut search_metadata,
            evidence_facts.clone(),
            evidence_search_metadata.clone(),
            &catalog_subject_ids,
            "rera",
        );
        let (media_facts, media_record) = match options.media_facts_materialization_id.as_ref() {
            Some(materialization_id) => {
                let media_asset = AssetId::new(IMAGE_MEDIA_FACTS_ASSET_ID)
                    .expect("static image media asset id is valid");
                let record = materializations
                    .record_by_id_for_asset(&media_asset, materialization_id)
                    .await?
                    .ok_or_else(|| {
                        format!("image media materialization {materialization_id} was not found")
                    })?;
                let fact_key = record
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.key.ends_with("/facts/part-00000.parquet"))
                    .map(|artifact| artifact.key.clone())
                    .ok_or("image media materialization has no fact artifact")?;
                let rows = read_facts_parquet(&lake.get_bytes(&LakeKey::new(fact_key)?).await?)?;
                (rows, Some(record))
            }
            None => (evidence_facts, None),
        };
        rebase_source_facts(
            &mut facts,
            &mut search_metadata,
            media_facts,
            evidence_search_metadata,
            &catalog_society_ids,
            "externalimage",
        );
        rebase_source_facts(
            &mut facts,
            &mut search_metadata,
            plan_facts,
            Vec::new(),
            &catalog_society_ids,
            "rera",
        );
        media_record
    };

    let required_rera_assets = [
        RERA_RECEIPTS_ASSET_ID,
        RERA_SOURCE_RECORDS_ASSET_ID,
        RERA_CLAIMS_ASSET_ID,
    ];
    let mut rera_parents = BTreeMap::<String, MaterializationId>::new();
    for parent_id in &evidence_record.parent_materializations {
        let Some(parent) = materializations.record_by_id(parent_id).await? else {
            return Err(format!("evidence serving parent {parent_id} is missing").into());
        };
        if required_rera_assets.contains(&parent.asset_id.as_str())
            && rera_parents
                .insert(parent.asset_id.to_string(), parent.materialization_id)
                .is_some()
        {
            return Err(format!(
                "evidence serving bundle has multiple {} parents",
                parent.asset_id
            )
            .into());
        }
    }
    let mut parents = vec![
        base_release.derived_assets.kg_materialization_id.clone(),
        plan_record.materialization_id.clone(),
    ];
    if let Some(media_record) = media_record {
        parents.push(media_record.materialization_id);
    }
    for asset_id in required_rera_assets {
        parents.push(rera_parents.remove(asset_id).ok_or_else(|| {
            format!("evidence serving bundle is missing required {asset_id} parent")
        })?);
    }

    // The evidence bundle remains an immutable source watermark, but cannot
    // be a direct parent: a release lineage may contain only one global search
    // serving materialization. Its KG, plan, and RERA inputs above preserve the
    // evidence lineage needed for validation and promotion.

    SearchServingBundleMaterializer::new(lake.clone())
        .materialize_child_from_serving_records_with_rera_for_run(
            entities,
            facts,
            search_metadata,
            edges,
            evidence,
            excluded_rera_evidence_society_ids,
            options.version,
            vec![
                SourceWatermark {
                    source: "catalog_base_serving".to_string(),
                    high_watermark: base_record.materialization_id.to_string(),
                },
                SourceWatermark {
                    source: "rera_evidence_serving".to_string(),
                    high_watermark: evidence_record.materialization_id.to_string(),
                },
            ],
            parents,
            MaterializationId::new(),
        )
        .await
        .map_err(Into::into)
}

fn rebase_source_facts(
    base_facts: &mut Vec<backend::serving::ServingFactRecord>,
    base_search_metadata: &mut Vec<backend::serving::ServingSearchMetadataRecord>,
    candidate_facts: Vec<backend::serving::ServingFactRecord>,
    candidate_search_metadata: Vec<backend::serving::ServingSearchMetadataRecord>,
    catalog_subject_ids: &BTreeSet<String>,
    source_type: &str,
) {
    let refreshed_facts = candidate_facts
        .into_iter()
        .filter(|fact| {
            catalog_subject_ids.contains(&fact.entity_id)
                && fact.source_type.eq_ignore_ascii_case(source_type)
        })
        .fold(
            BTreeMap::<(String, String), backend::serving::ServingFactRecord>::new(),
            |mut current, fact| {
                let key = (fact.entity_id.clone(), fact.fact_key.clone());
                match current.get(&key) {
                    Some(existing)
                        if existing.learned_at > fact.learned_at
                            || (existing.learned_at == fact.learned_at
                                && existing.confidence >= fact.confidence) => {}
                    _ => {
                        current.insert(key, fact);
                    }
                }
                current
            },
        )
        .into_values()
        .collect::<Vec<_>>();
    let refreshed_keys = refreshed_facts
        .iter()
        .map(|fact| (fact.entity_id.clone(), fact.fact_key.clone()))
        .collect::<BTreeSet<_>>();

    base_facts
        .retain(|fact| !refreshed_keys.contains(&(fact.entity_id.clone(), fact.fact_key.clone())));
    base_facts.extend(refreshed_facts);

    base_search_metadata.retain(|metadata| {
        !refreshed_keys.contains(&(metadata.entity_id.clone(), metadata.fact_key.clone()))
    });
    base_search_metadata.extend(candidate_search_metadata.into_iter().filter(|metadata| {
        refreshed_keys.contains(&(metadata.entity_id.clone(), metadata.fact_key.clone()))
    }));
}

fn rebase_plan_facts(
    base_facts: &mut Vec<backend::serving::ServingFactRecord>,
    candidate_facts: Vec<backend::serving::ServingFactRecord>,
    catalog_society_ids: &BTreeSet<String>,
) {
    const PROJECT_PLAN_FACT_KEY: &str = "media.project_plan_frames";

    let refreshed_facts = candidate_facts
        .into_iter()
        .filter(|fact| {
            catalog_society_ids.contains(&fact.entity_id)
                && fact.fact_key == PROJECT_PLAN_FACT_KEY
                && fact.source_type.eq_ignore_ascii_case("rera")
        })
        .fold(
            BTreeMap::<String, backend::serving::ServingFactRecord>::new(),
            |mut current, fact| {
                match current.get(&fact.entity_id) {
                    Some(existing)
                        if existing.learned_at > fact.learned_at
                            || (existing.learned_at == fact.learned_at
                                && existing.confidence >= fact.confidence) => {}
                    _ => {
                        current.insert(fact.entity_id.clone(), fact);
                    }
                }
                current
            },
        )
        .into_values()
        .collect::<Vec<_>>();
    let refreshed_entities = refreshed_facts
        .iter()
        .map(|fact| fact.entity_id.clone())
        .collect::<BTreeSet<_>>();

    base_facts.retain(|fact| {
        fact.fact_key != PROJECT_PLAN_FACT_KEY || !refreshed_entities.contains(&fact.entity_id)
    });
    base_facts.extend(refreshed_facts);
}

fn catalog_rera_subject_ids(
    memberships: &[CatalogMembership],
    entities: &[backend::serving::ServingEntityRecord],
    edges: &[backend::serving::ServingEdgeRecord],
) -> BTreeSet<String> {
    let property_entity_ids = memberships
        .iter()
        .filter_map(|membership| membership.property_id.as_deref())
        .map(|property_id| {
            if property_id.starts_with("property:") {
                property_id.to_string()
            } else {
                format!("property:{property_id}")
            }
        })
        .collect::<BTreeSet<_>>();
    let society_entity_ids = catalog_society_ids(entities);
    let mut subjects = memberships
        .iter()
        .filter(|membership| membership.property_id.is_some())
        .map(|membership| normalized_catalog_society_id(&membership.society_id))
        .collect::<BTreeSet<_>>();

    for edge in edges {
        if property_entity_ids.contains(&edge.from_entity_id)
            && society_entity_ids.contains(&edge.to_entity_id)
        {
            subjects.insert(edge.to_entity_id.clone());
        }
        if property_entity_ids.contains(&edge.to_entity_id)
            && society_entity_ids.contains(&edge.from_entity_id)
        {
            subjects.insert(edge.from_entity_id.clone());
        }
    }
    subjects
}

fn catalog_society_ids(entities: &[backend::serving::ServingEntityRecord]) -> BTreeSet<String> {
    entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.clone())
        .collect()
}

async fn ancestor_materialization_for_asset(
    materializations: &AssetMaterializationStore,
    root: &MaterializationRecord,
    asset_id: &str,
) -> Result<Option<MaterializationRecord>, Box<dyn std::error::Error>> {
    let mut pending = root.parent_materializations.clone();
    let mut visited = BTreeSet::new();
    let mut match_record = None;

    while let Some(materialization_id) = pending.pop() {
        if !visited.insert(materialization_id.to_string()) {
            continue;
        }
        let record = materializations
            .record_by_id(&materialization_id)
            .await?
            .ok_or_else(|| format!("materialization ancestor {materialization_id} is missing"))?;
        if record.asset_id.as_str() == asset_id {
            if match_record.is_some() {
                return Err(format!(
                    "serving ancestry contains multiple {asset_id} materializations"
                )
                .into());
            }
            match_record = Some(record);
        } else {
            pending.extend(record.parent_materializations.iter().cloned());
        }
    }

    Ok(match_record)
}

async fn serving_record_and_manifest(
    lake: &LakeStore,
    materializations: &AssetMaterializationStore,
    materialization_id: &MaterializationId,
) -> Result<(MaterializationRecord, ServingBundleManifest), Box<dyn std::error::Error>> {
    let serving_asset =
        AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static serving asset id is valid");
    let record = materializations
        .record_by_id_for_asset(&serving_asset, materialization_id)
        .await?
        .ok_or_else(|| format!("serving materialization {materialization_id} was not found"))?;
    let manifest_key = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("manifest.json"))
        .map(|artifact| artifact.key.clone())
        .ok_or_else(|| format!("serving materialization {materialization_id} has no manifest"))?;
    let manifest = lake.get_json(&LakeKey::new(manifest_key)?).await?;
    Ok((record, manifest))
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
    println!("  cargo run --bin openestates-catalog-release -- rebase-rera --base-release <uuid> --evidence-serving <uuid> --version <version>");
    println!("  cargo run --bin openestates-catalog-release -- extend-serving --base-release <uuid> --candidate-serving <uuid> --society <canonical-id> --version <version>");
    println!();
    println!("Options:");
    println!("  --project-root <path>       Project root used for local lake resolution");
    println!("  --base-release <uuid>       Base catalog release for an incremental candidate");
    println!("  --input <asset:uuid>        Pin an immutable input materialization; repeatable");
    println!("  --membership-file <path>    JSON array of CatalogMembership records");
    println!("  --membership-from-serving   Derive memberships from the pinned serving bundle");
    println!("  --evidence-serving <uuid>   Unpromoted serving candidate containing refreshed RERA evidence");
    println!(
        "  --media-facts <uuid>        Optional scoped image-media facts used during the rebase"
    );
    println!("  --plans-only                Replace only RERA plan-frame facts during the rebase");
    println!("  --candidate-serving <uuid>  Scoped serving candidate used to extend a catalog");
    println!("  --kg <uuid>                 Optional rebuilt KG lineage for a scoped refresh");
    println!("  --society <canonical-id>    Society expected in the scoped candidate; repeatable");
    println!(
        "  --refresh-existing          Refresh matching fact keys for an existing catalog society"
    );
    println!(
        "  --exclude-fact <fact-key>   Remove a superseded fact and search metadata; repeatable"
    );
    println!("  --tombstone-file <path>     JSON array of CatalogTombstone records");
    println!("  --expected-current <uuid>   Compare-and-swap expected environment release");
    println!("  --approve-production        Required for production promotion");
    println!("  --force-legacy-pointer      Force legacy global current pointers during production promotion");
    println!(
        "  {env_name:<27} Lake URL: file:///absolute/path or s3://bucket/optional/prefix",
        env_name = LAKE_URL_ENV
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend::assets::CatalogMembershipKind;
    use backend::knowledge::FactValue;
    use backend::serving::{
        ServingEdgeRecord, ServingEntityRecord, ServingFactRecord, ServingSearchMetadataRecord,
    };
    use chrono::Utc;

    fn fact(entity_id: &str, fact_key: &str, source_type: &str, value: &str) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "text".to_string(),
            value_text: Some(value.to_string()),
            value: FactValue::Text(value.to_string()),
            confidence: 0.9,
            source_type: source_type.to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc::now(),
        }
    }

    fn metadata(entity_id: &str, fact_key: &str, template: &str) -> ServingSearchMetadataRecord {
        ServingSearchMetadataRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: Some(template.to_string()),
            answers_preferences: Vec::new(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds: Vec::new(),
        }
    }

    #[test]
    fn rera_rebase_refreshes_catalog_facts_without_importing_other_entities() {
        let society = "society:rera-catalog";
        let mut facts = vec![
            fact(society, "media.project_plan_frames", "Rera", "old"),
            fact(society, "google_rating", "Google", "4.2"),
        ];
        let mut metadata_rows = vec![metadata(society, "media.project_plan_frames", "old plans")];

        rebase_source_facts(
            &mut facts,
            &mut metadata_rows,
            vec![
                fact(society, "media.project_plan_frames", "Rera", "new"),
                fact("society:rera-outside", "rera_status", "Rera", "approved"),
                fact(society, "google_rating", "Google", "4.8"),
            ],
            vec![
                metadata(society, "media.project_plan_frames", "new plans"),
                metadata("society:rera-outside", "rera_status", "status"),
            ],
            &BTreeSet::from([society.to_string()]),
            "rera",
        );

        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|row| {
            row.fact_key == "media.project_plan_frames"
                && row.value == FactValue::Text("new".to_string())
        }));
        assert!(facts.iter().any(|row| {
            row.fact_key == "google_rating" && row.value == FactValue::Text("4.2".to_string())
        }));
        assert_eq!(metadata_rows.len(), 1);
        assert_eq!(
            metadata_rows[0].display_template.as_deref(),
            Some("new plans")
        );
    }

    #[test]
    fn plan_only_rebase_preserves_every_unrelated_fact() {
        let society = "society:rera-catalog";
        let outside = "society:rera-outside";
        let mut facts = vec![
            fact(society, "media.project_plan_frames", "Rera", "old"),
            fact(society, "rera_status", "Rera", "approved"),
            fact(society, "google_rating", "Google", "4.2"),
        ];

        rebase_plan_facts(
            &mut facts,
            vec![
                fact(society, "media.project_plan_frames", "Rera", "new"),
                fact(society, "rera_status", "Rera", "changed"),
                fact(outside, "media.project_plan_frames", "Rera", "outside"),
            ],
            &BTreeSet::from([society.to_string()]),
        );

        assert_eq!(facts.len(), 3);
        assert!(facts.iter().any(|row| {
            row.fact_key == "media.project_plan_frames"
                && row.value == FactValue::Text("new".to_string())
        }));
        assert!(facts.iter().any(|row| {
            row.fact_key == "rera_status" && row.value == FactValue::Text("approved".to_string())
        }));
        assert!(!facts.iter().any(|row| row.entity_id == outside));
    }

    #[test]
    fn source_rebase_refreshes_catalog_media_without_importing_other_societies() {
        let society = "society:rera-catalog";
        let mut facts = vec![fact(
            society,
            "hero_image",
            "ExternalImage",
            "/societies/rera-catalog/1.jpg",
        )];
        let mut metadata_rows = Vec::new();

        rebase_source_facts(
            &mut facts,
            &mut metadata_rows,
            vec![
                fact(
                    society,
                    "hero_image",
                    "ExternalImage",
                    "/media/images/sha256/aa/current.jpg",
                ),
                fact(
                    "society:rera-outside",
                    "hero_image",
                    "ExternalImage",
                    "/media/images/sha256/bb/outside.jpg",
                ),
            ],
            Vec::new(),
            &BTreeSet::from([society.to_string()]),
            "externalimage",
        );

        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0].value,
            FactValue::Text("/media/images/sha256/aa/current.jpg".to_string())
        );
    }

    #[test]
    fn scoped_refresh_replaces_only_candidate_fact_keys() {
        let society = "society:rera-catalog";
        let route = "place:approach-road:catalog-route";
        let outside = "society:rera-outside";
        let mut facts = vec![
            fact(society, "google_rating", "Google", "4.2"),
            fact(society, "approach_road", "OpenStreetMap", "old route"),
            fact(outside, "approach_road", "OpenStreetMap", "outside route"),
        ];
        let entity_ids = BTreeSet::from([society.to_string(), route.to_string()]);

        refresh_candidate_facts(
            &mut facts,
            vec![
                fact(society, "approach_road", "OpenStreetMap", "ECC Road"),
                fact(route, "geo.geometry_geojson", "OpenStreetMap", "line"),
            ],
            &entity_ids,
        );

        assert_eq!(facts.len(), 4);
        assert!(facts.iter().any(|row| {
            row.entity_id == society
                && row.fact_key == "google_rating"
                && row.value == FactValue::Text("4.2".to_string())
        }));
        assert!(facts.iter().any(|row| {
            row.entity_id == society
                && row.fact_key == "approach_road"
                && row.value == FactValue::Text("ECC Road".to_string())
        }));
        assert!(facts.iter().any(|row| row.entity_id == route));
        assert!(facts.iter().any(|row| row.entity_id == outside));
    }

    #[test]
    fn scoped_refresh_retires_replaced_unshared_link_targets() {
        let society = "society:waterford";
        let old_route = "place:approach-road:waterford-old";
        let new_route = "place:approach-road:waterford-new";
        let base_facts = vec![
            fact(society, "approach_road_entity", "OpenStreetMap", old_route),
            fact(
                old_route,
                "geo.geometry_geojson",
                "OpenStreetMap",
                "old line",
            ),
        ];
        let candidate_facts = vec![
            fact(society, "approach_road_entity", "OpenStreetMap", new_route),
            fact(
                new_route,
                "geo.geometry_geojson",
                "OpenStreetMap",
                "new line",
            ),
        ];

        let retired = replaced_linked_entity_ids(
            &base_facts,
            &candidate_facts,
            &BTreeSet::from([society.to_string(), old_route.to_string()]),
            &BTreeSet::from([society.to_string(), new_route.to_string()]),
        );

        assert_eq!(retired, BTreeSet::from([old_route.to_string()]));
    }

    #[test]
    fn scoped_refresh_keeps_link_targets_shared_outside_the_candidate() {
        let society = "society:waterford";
        let other_society = "society:other";
        let old_route = "place:approach-road:shared";
        let new_route = "place:approach-road:waterford-new";
        let base_facts = vec![
            fact(society, "approach_road_entity", "OpenStreetMap", old_route),
            fact(
                other_society,
                "approach_road_entity",
                "OpenStreetMap",
                old_route,
            ),
        ];
        let candidate_facts = vec![
            fact(society, "approach_road_entity", "OpenStreetMap", new_route),
            fact(
                new_route,
                "geo.geometry_geojson",
                "OpenStreetMap",
                "new line",
            ),
        ];

        let retired = replaced_linked_entity_ids(
            &base_facts,
            &candidate_facts,
            &BTreeSet::from([
                society.to_string(),
                other_society.to_string(),
                old_route.to_string(),
            ]),
            &BTreeSet::from([society.to_string(), new_route.to_string()]),
        );

        assert!(retired.is_empty());
    }

    #[test]
    fn rera_rebase_scope_follows_property_membership_edges() {
        let membership = CatalogMembership {
            society_id: "soc-godrej-lakeside-orchard".to_string(),
            property_id: Some("discovered-godrej-lakeside-orchard-3bhk".to_string()),
            property_config_id: None,
            rera_id: None,
            aliases: Vec::new(),
            coordinates: None,
            source_materialization_id: None,
            membership_kind: CatalogMembershipKind::Reused,
        };
        let entities = vec![ServingEntityRecord {
            entity_id: "society:rera-688242e8e3711955".to_string(),
            entity_type: "society".to_string(),
            name: "GODREJ LAKESIDE ORCHARD".to_string(),
            root_source: Some("rera".to_string()),
            searchable_text: String::new(),
        }];
        let edges = vec![ServingEdgeRecord {
            from_entity_id: "property:discovered-godrej-lakeside-orchard-3bhk".to_string(),
            edge_type: "in_society".to_string(),
            to_entity_id: "society:rera-688242e8e3711955".to_string(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
        }];

        let subjects = catalog_rera_subject_ids(&[membership], &entities, &edges);

        assert!(subjects.contains("society:godrej-lakeside-orchard"));
        assert!(subjects.contains("society:rera-688242e8e3711955"));
        assert!(!subjects.contains("society:rera-outside"));
    }

    #[test]
    fn catalog_society_scope_excludes_non_society_entities() {
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:rera-688242e8e3711955".to_string(),
                entity_type: "society".to_string(),
                name: "GODREJ LAKESIDE ORCHARD".to_string(),
                root_source: Some("rera".to_string()),
                searchable_text: String::new(),
            },
            ServingEntityRecord {
                entity_id: "place:outside".to_string(),
                entity_type: "place".to_string(),
                name: "Outside".to_string(),
                root_source: None,
                searchable_text: String::new(),
            },
        ];

        let subjects = catalog_society_ids(&entities);

        assert!(subjects.contains("society:rera-688242e8e3711955"));
        assert!(!subjects.contains("place:outside"));
    }
}
