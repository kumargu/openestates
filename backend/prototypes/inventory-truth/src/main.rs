use inventory_truth_preview::{
    model::{Fixture, InventoryCatalog, InventoryDetail, Policy},
    parquet, router, Inventory, Result,
};
use std::{fs, path::Path};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("materialize") if args.len() == 4 => {
            let fixture: Fixture = serde_json::from_slice(&fs::read(&args[1])?)?;
            let policy: Policy = serde_json::from_slice(&fs::read(&args[2])?)?;
            parquet::materialize(&fixture, &policy, Path::new(&args[3]))?;
            println!("Materialized preview snapshot at {}", args[3]);
        }
        Some("schema") if args.len() == 2 => {
            let directory = Path::new(&args[1]); fs::create_dir_all(directory)?;
            let settings = schemars::generate::SchemaSettings::draft07().for_serialize();
            fs::write(directory.join("InventoryCatalog.json"), serde_json::to_vec_pretty(&settings.clone().into_generator().into_root_schema_for::<InventoryCatalog>())?)?;
            fs::write(directory.join("InventoryDetail.json"), serde_json::to_vec_pretty(&settings.into_generator().into_root_schema_for::<InventoryDetail>())?)?;
        }
        Some("serve") if args.len() == 2 || args.len() == 3 => {
            let state = Inventory::from_snapshot(Path::new(&args[1]))?;
            let port: u16 = args.get(2).map(String::as_str).unwrap_or("4019").parse()?;
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            println!("Inventory design fixture API: http://127.0.0.1:{port}");
            axum::serve(listener, router(state)).await?;
        }
        _ => return Err("Usage: inventory-truth-preview materialize INPUT.json POLICY.json NEW_DIRECTORY | schema DIRECTORY | serve SNAPSHOT_DIRECTORY [PORT]".into()),
    }
    Ok(())
}
