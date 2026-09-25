use backend::catalog::{CatalogApplyRequest, CatalogStore};
use backend::lake::LakeStoreLocation;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = project_root()?;
    let store = CatalogStore::new(LakeStoreLocation::from_env(&root)?.open()?);
    let report = match args.as_slice() {
        [command, path] if command == "apply" => {
            let request: CatalogApplyRequest =
                serde_json::from_slice(&tokio::fs::read(path).await?)?;
            serde_json::to_value(store.apply(request).await?)?
        }
        [command] if command == "undo" => serde_json::to_value(store.undo().await?)?,
        _ => return Err("Usage: openestates-catalog apply <request.json> | undo".into()),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
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
