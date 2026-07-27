use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfacesFile {
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    pub surfaces: Vec<UiSurfaceConfig>,
    #[serde(default)]
    pub surface_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfaceConfig {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub kicker: Option<String>,
    #[serde(default)]
    pub leaf_keys: Vec<String>,
    #[serde(default)]
    pub traversal: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub primary_entity: Option<String>,
    #[serde(default)]
    pub scene: Option<UiSurfaceSceneConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfaceSceneConfig {
    pub anchor: UiSurfaceAnchorConfig,
    #[serde(default)]
    pub layers: Vec<UiSurfaceLayerRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSurfaceAnchorConfig {
    #[serde(rename = "entityRef")]
    pub entity_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfaceLayerRule {
    pub id: String,
    pub label: String,
    #[serde(default, rename = "factKeys")]
    pub fact_keys: Vec<String>,
    #[serde(default, rename = "edgeTypes")]
    pub edge_types: Vec<String>,
    #[serde(default, rename = "linkedEntityFactKeys")]
    pub linked_entity_fact_keys: Vec<String>,
    #[serde(default, rename = "sortPriorityFactKeys")]
    pub sort_priority_fact_keys: Vec<String>,
    pub family: String,
    #[serde(rename = "relationClass")]
    pub relation_class: String,
    #[serde(rename = "renderKind")]
    pub render_kind: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default, rename = "maxItems")]
    pub max_items: Option<usize>,
    #[serde(default, rename = "expandedMaxItems")]
    pub expanded_max_items: Option<usize>,
    #[serde(default, rename = "spreadMinDistanceKm")]
    pub spread_min_distance_km: Option<f64>,
    #[serde(default, rename = "showReviewMetrics")]
    pub show_review_metrics: Option<bool>,
    #[serde(default, rename = "includeNameMarkers")]
    pub include_name_markers: Vec<String>,
    #[serde(default, rename = "includeRelatedSocietyFacts")]
    pub include_related_society_facts: bool,
    #[serde(default)]
    pub chainable: bool,
    #[serde(default, rename = "directOnly")]
    pub direct_only: bool,
    #[serde(default = "default_enabled", rename = "enabledByDefault")]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub rank: Option<u32>,
}

fn default_enabled() -> bool {
    true
}

pub fn ui_surfaces_path() -> PathBuf {
    dag_root().join("ui_surfaces.json")
}

pub fn load_ui_surfaces() -> Result<UiSurfacesFile, DagConfigError> {
    load_ui_surfaces_from_path(&ui_surfaces_path())
}

pub fn load_ui_surfaces_from_path(path: &Path) -> Result<UiSurfacesFile, DagConfigError> {
    let config: UiSurfacesFile = load_json(path)?;
    validate_ui_surfaces(&config)?;
    Ok(config)
}

static UI_SURFACES_CONFIG: OnceLock<Result<UiSurfacesFile, String>> = OnceLock::new();

pub fn ui_surfaces_config() -> Result<&'static UiSurfacesFile, DagConfigError> {
    match UI_SURFACES_CONFIG.get_or_init(|| load_ui_surfaces().map_err(|err| err.to_string())) {
        Ok(config) => Ok(config),
        Err(err) => Err(DagConfigError::InvalidConfig(err.clone())),
    }
}

fn validate_ui_surfaces(config: &UiSurfacesFile) -> Result<(), DagConfigError> {
    if let Some(surface_count) = config.surface_count {
        if surface_count != config.surfaces.len() {
            return Err(DagConfigError::InvalidConfig(format!(
                "ui_surfaces surface_count {surface_count} does not match {} surfaces",
                config.surfaces.len()
            )));
        }
    }

    for surface in &config.surfaces {
        if surface.id.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(
                "ui_surfaces contains a blank surface id".to_string(),
            ));
        }
        let Some(scene) = surface.scene.as_ref() else {
            continue;
        };
        if scene.anchor.entity_ref.trim().is_empty() {
            return Err(DagConfigError::InvalidConfig(format!(
                "surface {} has a blank scene anchor entityRef",
                surface.id
            )));
        }
        for layer in &scene.layers {
            if layer.id.trim().is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} has a blank layer id",
                    surface.id
                )));
            }
            if layer.fact_keys.is_empty() && layer.edge_types.is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} layer {} has no factKeys or edgeTypes",
                    surface.id, layer.id
                )));
            }
            if layer.direct_only && layer.chainable {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} layer {} cannot be both directOnly and chainable",
                    surface.id, layer.id
                )));
            }
            if let (Some(max_items), Some(expanded_max_items)) =
                (layer.max_items, layer.expanded_max_items)
            {
                if expanded_max_items < max_items {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} expandedMaxItems must be >= maxItems",
                        surface.id, layer.id
                    )));
                }
            }
            if layer
                .spread_min_distance_km
                .is_some_and(|distance| !distance.is_finite() || distance < 0.0)
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} layer {} spreadMinDistanceKm must be a non-negative number",
                    surface.id, layer.id
                )));
            }
            for fact_key in &layer.sort_priority_fact_keys {
                if !layer.fact_keys.iter().any(|key| key == fact_key)
                    && !layer
                        .linked_entity_fact_keys
                        .iter()
                        .any(|key| key == fact_key)
                {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} sortPriorityFactKeys contains unknown fact key {}",
                        surface.id, layer.id, fact_key
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_surfaces_loads_scene_rules() {
        let config = load_ui_surfaces().expect("ui_surfaces.json should load");
        let around = config
            .surfaces
            .iter()
            .find(|surface| surface.id == "around_this_home")
            .expect("around_this_home surface exists");
        let scene = around.scene.as_ref().expect("scene rules exist");
        assert_eq!(scene.anchor.entity_ref, "society");
        assert!(scene.layers.iter().any(|layer| layer.id == "metro"));
    }

    #[test]
    fn risk_scene_layers_are_direct_only() {
        let config = load_ui_surfaces().expect("ui_surfaces.json should load");
        for surface_id in ["approach_road", "flooding"] {
            let surface = config
                .surfaces
                .iter()
                .find(|surface| surface.id == surface_id)
                .expect("risk surface exists");
            let scene = surface.scene.as_ref().expect("risk scene rules exist");
            assert!(scene.layers.iter().all(|layer| layer.direct_only));
            assert!(scene.layers.iter().all(|layer| !layer.chainable));
        }
    }
}
