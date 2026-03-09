use std::path::Path;

use crate::models::{AreaProfile, Property, Society};
use crate::state::AppState;

pub fn load_seed_data(data_dir: &Path) -> AppState {
    let properties: Vec<Property> = load_json(data_dir.join("properties.json"));
    let areas: Vec<AreaProfile> = load_json(data_dir.join("area_profiles.json"));
    let societies: Vec<Society> = load_json(data_dir.join("societies.json"));

    println!(
        "Loaded {} properties, {} areas, {} societies",
        properties.len(),
        areas.len(),
        societies.len()
    );

    AppState {
        properties,
        areas,
        societies,
    }
}

fn load_json<T: serde::de::DeserializeOwned>(path: std::path::PathBuf) -> Vec<T> {
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}
