use std::path::{Path, PathBuf};

use backend::assets::SourceEntitySeed;
use backend::catalog::{collect_society_from_dag, CatalogRoster, CatalogStore};
use backend::lake::LakeStoreLocation;
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug)]
enum Command {
    Add(PathBuf),
    Remove(String),
    Rebuild,
    Undo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSeedRoster {
    format_version: u32,
    #[serde(rename = "description")]
    _description: String,
    source_entities: Vec<SourceEntitySeed>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = parse_command()?;
    let project_root = project_root()?;
    let lake = LakeStoreLocation::from_env(&project_root)?.open()?;
    let store = CatalogStore::new(lake);
    let report = match command {
        Command::Add(path) => {
            if store.pointer().await?.is_none() {
                return Err(
                    "catalog is not initialized; run rebuild to cut over the bootstrap roster"
                        .into(),
                );
            }
            let seed: SourceEntitySeed = serde_json::from_slice(&tokio::fs::read(path).await?)?;
            let (records, warnings) =
                collect_society_from_dag(&store, &project_root, &seed).await?;
            store.upsert_snapshot(seed, &records, warnings).await?
        }
        Command::Remove(society_id) => store.remove(&society_id).await?,
        Command::Rebuild => {
            let expected = store.pointer().await?;
            let (seeds, topology) = match &expected {
                Some(pointer) => {
                    let roster = store.read_roster(&pointer.current).await?;
                    (
                        roster
                            .societies
                            .into_iter()
                            .map(|entry| entry.seed)
                            .collect::<Vec<_>>(),
                        roster.topology,
                    )
                }
                None => (
                    load_bootstrap_roster(&project_root).await?.source_entities,
                    None,
                ),
            };
            if seeds.is_empty() {
                return Err("catalog roster is empty".into());
            }
            let mut entries = Vec::with_capacity(seeds.len());
            for (index, seed) in seeds.iter().enumerate() {
                eprintln!(
                    "Collecting society {}/{}: {}",
                    index + 1,
                    seeds.len(),
                    seed.name
                );
                let (records, warnings) =
                    collect_society_from_dag(&store, &project_root, seed).await?;
                let snapshot = store
                    .write_snapshot(seed.clone(), &records, warnings)
                    .await?;
                entries.push(snapshot.roster_entry());
            }
            let roster = CatalogRoster {
                format_version: backend::catalog::CATALOG_FORMAT_VERSION,
                roster_id: backend::assets::MaterializationId::new(),
                created_at: Utc::now(),
                topology,
                societies: entries,
            };
            store
                .assemble_and_promote("rebuild", roster, expected.as_ref())
                .await?
        }
        Command::Undo => store.undo().await?,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn parse_command() -> Result<Command, String> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };
    let parsed = match command.as_str() {
        "add" => Command::Add(PathBuf::from(
            args.next()
                .ok_or_else(|| "add requires <society-seed.json>".to_string())?,
        )),
        "remove" => Command::Remove(
            args.next()
                .ok_or_else(|| "remove requires <society-id>".to_string())?,
        ),
        "rebuild" => Command::Rebuild,
        "undo" => Command::Undo,
        "help" | "--help" | "-h" => return Err(usage()),
        other => return Err(format!("unknown catalog command {other}\n{}", usage())),
    };
    if args.next().is_some() {
        return Err(format!("too many arguments\n{}", usage()));
    }
    Ok(parsed)
}

fn usage() -> String {
    [
        "Manage the OpenEstates development catalog.",
        "",
        "Usage:",
        "  openestates-catalog add <society-seed.json>",
        "  openestates-catalog remove <society-id>",
        "  openestates-catalog rebuild",
        "  openestates-catalog undo",
    ]
    .join("\n")
}

fn project_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current = std::env::current_dir()?;
    if current.join("app/config/dag/manifest.json").is_file() {
        return Ok(current);
    }
    if current
        .parent()
        .is_some_and(|parent| parent.join("app/config/dag/manifest.json").is_file())
    {
        return Ok(current.parent().expect("checked parent").to_path_buf());
    }
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend lives under project root")
        .to_path_buf())
}

async fn load_bootstrap_roster(
    project_root: &Path,
) -> Result<CatalogSeedRoster, Box<dyn std::error::Error>> {
    let path = project_root.join("data/catalog/bootstrap_roster.json");
    let roster: CatalogSeedRoster = serde_json::from_slice(&tokio::fs::read(&path).await?)?;
    if roster.format_version != 1 {
        return Err(format!(
            "unsupported catalog seed roster format {}",
            roster.format_version
        )
        .into());
    }
    Ok(roster)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_surface_is_only_the_four_catalog_operations() {
        assert!(usage().contains("add <society-seed.json>"));
        assert!(usage().contains("remove <society-id>"));
        assert!(usage().contains("rebuild"));
        assert!(usage().contains("undo"));
        assert!(!usage().contains("promote"));
        assert!(!usage().contains("materialization"));
        assert!(!usage().contains("version"));
    }
}
