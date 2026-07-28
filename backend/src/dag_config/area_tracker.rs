use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, Deserialize)]
pub struct AreaTrackerConfigFile {
    pub version: u32,
    pub minimum_market_listing_count: usize,
    pub sort: AreaTrackerSortConfig,
    pub fallbacks: AreaTrackerFallbackConfig,
    #[serde(default)]
    pub metrics: Vec<AreaTrackerMetricConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AreaTrackerSortConfig {
    pub metric_id: String,
    pub direction: AreaTrackerSortDirection,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AreaTrackerSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AreaTrackerFallbackConfig {
    pub primary_signal: String,
    pub top_builder: String,
    pub demand_score: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AreaTrackerMetricConfig {
    pub id: String,
    pub fact_key: String,
    pub label: String,
    pub value_type: AreaTrackerMetricValueType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_field: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AreaTrackerMetricValueType {
    Count,
    Score,
    Text,
}

pub fn area_tracker_path() -> std::path::PathBuf {
    dag_root().join("area_tracker.json")
}

pub fn load_area_tracker_config() -> Result<AreaTrackerConfigFile, DagConfigError> {
    load_area_tracker_config_from_path(&area_tracker_path())
}

pub fn load_area_tracker_config_from_path(
    path: &std::path::Path,
) -> Result<AreaTrackerConfigFile, DagConfigError> {
    let config: AreaTrackerConfigFile = load_json(path)?;
    validate_area_tracker_config(&config).map_err(DagConfigError::InvalidConfig)?;
    Ok(config)
}

pub fn area_tracker_config() -> &'static AreaTrackerConfigFile {
    static CONFIG: OnceLock<AreaTrackerConfigFile> = OnceLock::new();
    CONFIG.get_or_init(|| {
        load_area_tracker_config().expect("area_tracker.json must load and validate")
    })
}

fn validate_area_tracker_config(config: &AreaTrackerConfigFile) -> Result<(), String> {
    if config.version == 0 {
        return Err("area_tracker.version must be positive".to_string());
    }
    if config.metrics.is_empty() {
        return Err("area_tracker.metrics must not be empty".to_string());
    }
    if config.fallbacks.demand_score < 0.0 || !config.fallbacks.demand_score.is_finite() {
        return Err(
            "area_tracker.fallbacks.demand_score must be finite and non-negative".to_string(),
        );
    }
    if config.fallbacks.primary_signal.trim().is_empty() {
        return Err("area_tracker.fallbacks.primary_signal must not be empty".to_string());
    }

    let mut ids = HashSet::new();
    let mut fact_keys = HashSet::new();
    for metric in &config.metrics {
        if metric.id.trim().is_empty() {
            return Err("area_tracker metric id must not be empty".to_string());
        }
        if metric.fact_key.trim().is_empty() {
            return Err(format!(
                "area_tracker metric {} has an empty fact key",
                metric.id
            ));
        }
        if metric.label.trim().is_empty() {
            return Err(format!(
                "area_tracker metric {} has an empty label",
                metric.id
            ));
        }
        if !ids.insert(metric.id.as_str()) {
            return Err(format!(
                "area_tracker metric id {} is duplicated",
                metric.id
            ));
        }
        if !fact_keys.insert(metric.fact_key.as_str()) {
            return Err(format!(
                "area_tracker metric fact key {} is duplicated",
                metric.fact_key
            ));
        }
    }

    if !ids.contains(config.sort.metric_id.as_str()) {
        return Err(format!(
            "area_tracker.sort.metric_id {} does not match a configured metric",
            config.sort.metric_id
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_tracker_config_loads() {
        let config = load_area_tracker_config().expect("area tracker config");
        assert!(config
            .metrics
            .iter()
            .any(|metric| metric.fact_key == "area.market.listing_count"));
        assert!(config
            .metrics
            .iter()
            .any(|metric| metric.fact_key == "area.discovery.primary_signal"));
    }

    #[test]
    fn area_tracker_config_rejects_duplicate_metric_ids() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("area_tracker.json");
        std::fs::write(
            &path,
            r#"{
              "version": 1,
              "minimum_market_listing_count": 1,
              "sort": { "metric_id": "listing_count", "direction": "desc" },
              "fallbacks": {
                "primary_signal": "pending",
                "top_builder": "",
                "demand_score": 0.0
              },
              "metrics": [
                {
                  "id": "listing_count",
                  "fact_key": "area.market.listing_count",
                  "label": "Listings",
                  "value_type": "count"
                },
                {
                  "id": "listing_count",
                  "fact_key": "area.market.ready_inventory_count",
                  "label": "Ready inventory",
                  "value_type": "count"
                }
              ]
            }"#,
        )
        .expect("write fixture");

        assert!(load_area_tracker_config_from_path(&path).is_err());
    }
}
