use std::collections::{HashMap, HashSet};
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
    #[serde(default, rename = "proofHandoff")]
    pub proof_handoff: Option<UiSurfaceProofHandoffConfig>,
    #[serde(default, rename = "comparisonDimensions")]
    pub comparison_dimensions: Vec<UiComparisonDimensionConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSurfaceProofHandoffConfig {
    pub kind: String,
    #[serde(rename = "targetId")]
    pub target_id: String,
    #[serde(default, rename = "factKeys")]
    pub fact_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiComparisonDimensionConfig {
    pub key: String,
    pub label: String,
    #[serde(rename = "valueKey")]
    pub value_key: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfaceSceneConfig {
    pub anchor: UiSurfaceAnchorConfig,
    #[serde(default)]
    pub experience: Option<UiSurfaceSceneExperienceConfig>,
    #[serde(default)]
    pub layers: Vec<UiSurfaceLayerRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSurfaceSceneExperienceConfig {
    pub reveal_duration_ms: u32,
    pub start_range_m: f64,
    pub final_range_m: f64,
    pub final_tilt: f64,
    pub final_heading: f64,
    pub rotation_arc_degrees: f64,
    pub boundary_padding: f64,
    pub mobile_boundary_padding: f64,
    #[serde(default)]
    pub missing_boundary_state: Option<String>,
    #[serde(default)]
    pub google_unavailable_state: Option<String>,
    #[serde(default)]
    pub society_play_label: Option<String>,
    #[serde(default)]
    pub society_pause_label: Option<String>,
    #[serde(default)]
    pub society_resume_label: Option<String>,
    #[serde(default)]
    pub search_context_label: Option<String>,
    #[serde(default)]
    pub search_context_view_home_label: Option<String>,
    #[serde(default)]
    pub back_to_society_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSurfaceAnchorConfig {
    #[serde(rename = "entityRef")]
    pub entity_ref: String,
    #[serde(default, rename = "boundaryFactKey")]
    pub boundary_fact_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSurfaceLayerRule {
    pub id: String,
    pub label: String,
    #[serde(default, rename = "factKeys")]
    pub fact_keys: Vec<String>,
    #[serde(default, rename = "featureLabels")]
    pub feature_labels: HashMap<String, String>,
    #[serde(default, rename = "featureProperties")]
    pub feature_properties: HashMap<String, String>,
    #[serde(default, rename = "featureValueLabels")]
    pub feature_value_labels: HashMap<String, HashMap<String, String>>,
    #[serde(default, rename = "emptyState")]
    pub empty_state: Option<String>,
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
    #[serde(default, rename = "mapPresentation")]
    pub map_presentation: Option<String>,
    #[serde(default)]
    pub experience: Option<UiSurfaceLayerExperienceConfig>,
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
    #[serde(default = "default_enabled", rename = "enabledByDefault")]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSurfaceLayerExperienceConfig {
    pub kind: String,
    pub waypoint_spacing_m: u32,
    #[serde(default)]
    pub overview_dwell_ms: Option<u32>,
    pub dwell_ms: u32,
    #[serde(default)]
    pub anchor_look_ahead_m: Option<u32>,
    #[serde(default)]
    pub anchor_pitch: Option<f64>,
    #[serde(default)]
    pub anchor_interior_pitch: Option<f64>,
    #[serde(default)]
    pub anchor_interior_dwell_ms: Option<u32>,
    #[serde(default)]
    pub anchor_pose_transition_ms: Option<u32>,
    pub camera_altitude_m: f64,
    pub camera_range_m: f64,
    pub camera_tilt: f64,
    pub camera_fov: f64,
    pub street_view_zoom: f64,
    pub transition_ms: u32,
    #[serde(default)]
    pub target_duration_ms: Option<u32>,
    #[serde(default)]
    pub minimum_duration_ms: Option<u32>,
    #[serde(default)]
    pub maximum_duration_ms: Option<u32>,
    #[serde(default)]
    pub minimum_frame_dwell_ms: Option<u32>,
    #[serde(default)]
    pub panorama_crossfade_ms: Option<u32>,
    #[serde(default)]
    pub entrance_dwell_ms: Option<u32>,
    #[serde(default)]
    pub maximum_panorama_gap_m: Option<u32>,
    #[serde(default)]
    pub short_gap_state: Option<String>,
    #[serde(default)]
    pub ends_here_state: Option<String>,
    #[serde(default)]
    pub unavailable_state: Option<String>,
    #[serde(default)]
    pub pause_label: Option<String>,
    #[serde(default)]
    pub resume_label: Option<String>,
    #[serde(default)]
    pub replay_label: Option<String>,
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
        let mut comparison_dimension_keys = HashSet::new();
        for dimension in &surface.comparison_dimensions {
            if dimension.key.trim().is_empty()
                || dimension.label.trim().is_empty()
                || dimension.value_key.trim().is_empty()
                || dimension.format.trim().is_empty()
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} contains an incomplete comparison dimension",
                    surface.id
                )));
            }
            if !comparison_dimension_keys.insert(dimension.key.as_str()) {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} repeats comparison dimension {}",
                    surface.id, dimension.key
                )));
            }
        }
        if let Some(handoff) = surface.proof_handoff.as_ref() {
            if !matches!(handoff.kind.as_str(), "scene" | "section") {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} has unsupported proof handoff kind {}",
                    surface.id, handoff.kind
                )));
            }
            if handoff.target_id.trim().is_empty() {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} has a blank proof handoff targetId",
                    surface.id
                )));
            }
            if handoff.kind == "section"
                && handoff.fact_keys.is_empty()
                && surface.leaf_keys.is_empty()
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} section proof handoff has no fact keys",
                    surface.id
                )));
            }
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
        if scene
            .anchor
            .boundary_fact_key
            .as_deref()
            .is_some_and(|fact_key| fact_key.trim().is_empty())
        {
            return Err(DagConfigError::InvalidConfig(format!(
                "surface {} has a blank scene anchor boundaryFactKey",
                surface.id
            )));
        }
        if let Some(experience) = scene.experience.as_ref() {
            let positive = |value: f64| value.is_finite() && value > 0.0;
            let has_blank_copy = [
                experience.missing_boundary_state.as_deref(),
                experience.google_unavailable_state.as_deref(),
                experience.society_play_label.as_deref(),
                experience.society_pause_label.as_deref(),
                experience.society_resume_label.as_deref(),
                experience.search_context_label.as_deref(),
                experience.search_context_view_home_label.as_deref(),
                experience.back_to_society_label.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|value| value.trim().is_empty());
            let missing_society_control = experience
                .society_play_label
                .as_deref()
                .is_none_or(str::is_empty)
                || experience
                    .society_pause_label
                    .as_deref()
                    .is_none_or(str::is_empty)
                || experience
                    .society_resume_label
                    .as_deref()
                    .is_none_or(str::is_empty)
                || experience
                    .search_context_view_home_label
                    .as_deref()
                    .is_none_or(str::is_empty);
            if experience.reveal_duration_ms == 0
                || !positive(experience.start_range_m)
                || !positive(experience.final_range_m)
                || !positive(experience.final_tilt)
                || !experience.final_heading.is_finite()
                || !positive(experience.rotation_arc_degrees)
                || !positive(experience.boundary_padding)
                || !positive(experience.mobile_boundary_padding)
                || has_blank_copy
                || missing_society_control
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} contains an invalid scene experience",
                    surface.id
                )));
            }
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
            if let Some(experience) = layer.experience.as_ref() {
                let finite_positive = |value: f64| value.is_finite() && value > 0.0;
                let missing_playback_copy = [
                    experience.pause_label.as_deref(),
                    experience.resume_label.as_deref(),
                    experience.replay_label.as_deref(),
                ]
                .into_iter()
                .any(|value| value.is_none_or(|label| label.trim().is_empty()));
                let interior_pose_values = [
                    experience.anchor_interior_pitch.is_some(),
                    experience.anchor_interior_dwell_ms.is_some(),
                    experience.anchor_pose_transition_ms.is_some(),
                ];
                let incomplete_interior_pose = interior_pose_values.iter().any(|value| *value)
                    && interior_pose_values.iter().any(|value| !*value);
                let interior_pose_exceeds_dwell = matches!(
                    (
                        experience.entrance_dwell_ms,
                        experience.anchor_interior_dwell_ms,
                        experience.anchor_pose_transition_ms,
                    ),
                    (Some(entrance), Some(interior), Some(transition))
                        if interior.saturating_add(transition) >= entrance
                );
                if experience.kind.trim().is_empty()
                    || experience.waypoint_spacing_m == 0
                    || experience.overview_dwell_ms == Some(0)
                    || experience.dwell_ms == 0
                    || experience
                        .anchor_pitch
                        .is_some_and(|pitch| !pitch.is_finite() || !(-90.0..=90.0).contains(&pitch))
                    || experience
                        .anchor_interior_pitch
                        .is_some_and(|pitch| !pitch.is_finite() || !(-90.0..=90.0).contains(&pitch))
                    || experience.anchor_interior_dwell_ms == Some(0)
                    || experience.anchor_pose_transition_ms == Some(0)
                    || incomplete_interior_pose
                    || interior_pose_exceeds_dwell
                    || experience.transition_ms == 0
                    || experience.target_duration_ms == Some(0)
                    || experience.minimum_duration_ms == Some(0)
                    || experience.maximum_duration_ms == Some(0)
                    || experience.minimum_frame_dwell_ms == Some(0)
                    || experience.panorama_crossfade_ms == Some(0)
                    || experience.entrance_dwell_ms == Some(0)
                    || experience.maximum_panorama_gap_m == Some(0)
                    || missing_playback_copy
                    || matches!(
                        (
                            experience.minimum_duration_ms,
                            experience.target_duration_ms,
                            experience.maximum_duration_ms,
                        ),
                        (Some(minimum), Some(target), Some(maximum))
                            if minimum > target || target > maximum
                    )
                    || !finite_positive(experience.camera_altitude_m)
                    || !finite_positive(experience.camera_range_m)
                    || !finite_positive(experience.camera_tilt)
                    || !finite_positive(experience.camera_fov)
                    || !experience.street_view_zoom.is_finite()
                    || experience.street_view_zoom < 0.0
                {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} contains an invalid experience",
                        surface.id, layer.id
                    )));
                }
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
            for (fact_key, label) in &layer.feature_labels {
                if !layer.fact_keys.iter().any(|key| key == fact_key) {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} featureLabels contains unknown fact key {}",
                        surface.id, layer.id, fact_key
                    )));
                }
                if label.trim().is_empty() {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} featureLabels contains a blank label for {}",
                        surface.id, layer.id, fact_key
                    )));
                }
            }
            for (property, fact_key) in &layer.feature_properties {
                if property.trim().is_empty() || fact_key.trim().is_empty() {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} contains an incomplete feature property",
                        surface.id, layer.id
                    )));
                }
            }
            if layer
                .empty_state
                .as_deref()
                .is_some_and(|state| state.trim().is_empty())
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "surface {} layer {} has a blank emptyState",
                    surface.id, layer.id
                )));
            }
            for (property, labels) in &layer.feature_value_labels {
                if !layer.feature_properties.contains_key(property)
                    || labels.is_empty()
                    || labels
                        .iter()
                        .any(|(value, label)| value.trim().is_empty() || label.trim().is_empty())
                {
                    return Err(DagConfigError::InvalidConfig(format!(
                        "surface {} layer {} has invalid featureValueLabels for {}",
                        surface.id, layer.id, property
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
        assert_eq!(scene.anchor.boundary_fact_key, None);
        let metro = scene
            .layers
            .iter()
            .find(|layer| layer.id == "metro")
            .expect("metro layer exists");
        assert_eq!(metro.fact_keys, ["nearby_metro_stations"]);
        assert_eq!(metro.map_presentation, None);
        assert!(metro.linked_entity_fact_keys.is_empty());
        assert!(scene.layers.iter().all(|layer| layer.id != "approach_road"));
        assert!(around
            .leaf_keys
            .iter()
            .all(|key| key != "transit_access_route" && key != "transit_access_route_entity"));

        let arrival = config
            .surfaces
            .iter()
            .find(|surface| surface.id == "arrival_story")
            .expect("arrival_story surface exists");
        let arrival_scene = arrival.scene.as_ref().expect("arrival scene rules exist");
        let scene_experience = arrival_scene
            .experience
            .as_ref()
            .expect("arrival experience");
        assert_eq!(scene_experience.reveal_duration_ms, 7_000);
        assert_eq!(scene_experience.final_range_m, 700.0);
        assert_eq!(scene_experience.final_tilt, 48.0);
        assert_eq!(scene_experience.final_heading, 210.0);
        assert_eq!(
            scene_experience.society_pause_label.as_deref(),
            Some("Pause society reveal")
        );
        assert_eq!(
            scene_experience.society_resume_label.as_deref(),
            Some("Resume society reveal")
        );
        assert_eq!(
            scene_experience.google_unavailable_state.as_deref(),
            Some("Map unavailable")
        );
        assert_eq!(
            scene_experience.search_context_view_home_label.as_deref(),
            Some("View home")
        );
        assert_eq!(
            arrival_scene.anchor.boundary_fact_key.as_deref(),
            Some("society.boundary_geojson")
        );
        let arrival_metro = arrival_scene
            .layers
            .iter()
            .find(|layer| layer.id == "metro")
            .expect("arrival metro layer exists");
        assert_eq!(
            arrival_metro.map_presentation.as_deref(),
            Some("immersive_3d")
        );
        let entrance = arrival_scene
            .layers
            .iter()
            .find(|layer| layer.id == "entrance")
            .expect("entrance layer exists");
        assert_eq!(entrance.render_kind, "arrival_marker");
        assert_eq!(entrance.empty_state.as_deref(), Some("Entrance not mapped"));
        assert_eq!(
            entrance
                .feature_properties
                .get("status")
                .map(String::as_str),
            Some("entrance.status")
        );
        assert_eq!(
            entrance
                .feature_value_labels
                .get("status")
                .and_then(|labels| labels.get("inferred"))
                .map(String::as_str),
            Some("Likely entrance")
        );
        let approach_road = arrival_scene
            .layers
            .iter()
            .find(|layer| layer.id == "approach_road")
            .expect("approach road layer exists");
        assert_eq!(approach_road.render_kind, "terrain_corridor");
        assert_eq!(
            arrival_scene
                .layers
                .iter()
                .find(|layer| layer.id == "approach_road")
                .and_then(|layer| layer.map_presentation.as_deref()),
            Some("immersive_3d")
        );
        let experience = approach_road.experience.as_ref().expect("road experience");
        assert_eq!(experience.kind, "street_view_tour");
        assert_eq!(experience.waypoint_spacing_m, 65);
        assert_eq!(experience.overview_dwell_ms, Some(1800));
        assert_eq!(experience.dwell_ms, 2500);
        assert_eq!(experience.anchor_look_ahead_m, Some(35));
        assert_eq!(experience.anchor_pitch, Some(6.0));
        assert_eq!(experience.anchor_interior_pitch, Some(10.0));
        assert_eq!(experience.anchor_interior_dwell_ms, Some(2000));
        assert_eq!(experience.anchor_pose_transition_ms, Some(700));
        assert_eq!(experience.transition_ms, 4200);
        assert_eq!(experience.entrance_dwell_ms, Some(5000));
        assert_eq!(experience.minimum_frame_dwell_ms, Some(1500));
        assert_eq!(experience.panorama_crossfade_ms, Some(280));
    }

    #[test]
    fn ui_surfaces_load_non_scene_proof_handoffs() {
        let config = load_ui_surfaces().expect("ui_surfaces.json should load");
        let legal = config
            .surfaces
            .iter()
            .find(|surface| surface.id == "legal_rera")
            .expect("legal surface");
        let handoff = legal.proof_handoff.as_ref().expect("legal proof handoff");
        assert_eq!(handoff.kind, "section");
        assert_eq!(handoff.target_id, "official-record");
        assert!(handoff.fact_keys.contains(&"rera_status".to_string()));
    }

    #[test]
    fn risk_scene_layers_use_risk_relation_class() {
        let config = load_ui_surfaces().expect("ui_surfaces.json should load");
        for surface_id in ["approach_road", "flooding"] {
            let surface = config
                .surfaces
                .iter()
                .find(|surface| surface.id == surface_id)
                .expect("risk surface exists");
            let scene = surface.scene.as_ref().expect("risk scene rules exist");
            assert!(scene
                .layers
                .iter()
                .all(|layer| layer.relation_class == "risk_externality"));
        }
    }

    #[test]
    fn short_compare_dimensions_come_from_ui_config() {
        let config = load_ui_surfaces().expect("ui_surfaces.json should load");
        let compare = config
            .surfaces
            .iter()
            .find(|surface| surface.id == "property_short_compare")
            .expect("property_short_compare surface exists");

        assert_eq!(compare.comparison_dimensions.len(), 5);
        assert_eq!(compare.comparison_dimensions[0].label, "Price");
        assert_eq!(compare.comparison_dimensions[4].label, "Rating");
    }
}
