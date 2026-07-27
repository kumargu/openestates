use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::dag_config::{
    area_tracker_config, AreaTrackerMetricConfig, AreaTrackerMetricValueType,
    AreaTrackerSortDirection,
};
use crate::models::area_profile::AreaTrackerMetrics;
use crate::models::AreaProfile;
use crate::models::Property;
use crate::routes::properties::ErrorResponse;
use crate::state::AppState;

/// Lightweight area summary for list/card views.
#[derive(Serialize)]
pub struct AreaListItem {
    pub id: String,
    pub name: String,
    pub median_price_per_sqft: u64,
    pub trend_direction: String,
    pub primary_signal: String,
}

/// GET /api/areas — returns lightweight area list for homepage cards.
pub async fn list_areas(State(state): State<Arc<AppState>>) -> Json<Vec<AreaListItem>> {
    let areas = state.areas.read().await;
    let items: Vec<AreaListItem> = areas
        .iter()
        .map(|a| {
            let primary_signal = a.externality_tags.first().cloned().unwrap_or_default();

            AreaListItem {
                id: a.id.clone(),
                name: a.name.clone(),
                median_price_per_sqft: a.median_price_per_sqft,
                trend_direction: a.trend_direction.clone(),
                primary_signal,
            }
        })
        .collect();

    Json(items)
}

/// Backend-owned Area Tracker read model.
#[derive(Serialize)]
pub struct AreaTrackerResponse {
    pub generated_at: String,
    pub total_areas: usize,
    pub total_listings: usize,
    pub metric_definitions: Vec<AreaTrackerMetricConfig>,
    pub markets: Vec<AreaTrackerMarket>,
}

#[derive(Serialize)]
pub struct AreaTrackerMarket {
    pub id: String,
    pub name: String,
    pub city: String,
    pub listing_count: usize,
    pub avg_price_per_sqft: u64,
    pub price_min: u64,
    pub price_max: u64,
    pub bhks: Vec<u32>,
    pub ready_to_move: usize,
    pub near_metro: usize,
    pub top_builder: String,
    pub societies: usize,
    pub median_price_per_sqft: u64,
    pub price_range_per_sqft: crate::models::area_profile::PriceRange,
    pub trend_direction: String,
    pub primary_signal: String,
    pub demand_score: f32,
    pub recent_searches: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_searched_at: Option<String>,
    pub evidence_gap_count: usize,
    pub sample_size: u32,
    pub last_updated: String,
    pub metrics: Vec<AreaTrackerMetricValue>,
}

#[derive(Serialize)]
pub struct AreaTrackerMetricValue {
    pub id: String,
    pub label: String,
    pub value_type: AreaTrackerMetricValueType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_field: Option<String>,
    pub value: serde_json::Value,
}

/// GET /api/areas/tracker — current micro-market inventory and configured area signals.
pub async fn area_tracker(State(state): State<Arc<AppState>>) -> Json<AreaTrackerResponse> {
    let properties = state.properties.read().await;
    let areas = state.areas.read().await;
    Json(build_area_tracker(&areas, &properties))
}

/// GET /api/areas/:id — returns full area profile.
pub async fn get_area(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AreaProfile>, (StatusCode, Json<ErrorResponse>)> {
    let areas = state.areas.read().await;
    let area = areas.iter().find(|a| a.id == id).cloned().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "area_not_found".to_string(),
            }),
        )
    })?;

    Ok(Json(area))
}

fn build_area_tracker(areas: &[AreaProfile], properties: &[Property]) -> AreaTrackerResponse {
    let config = area_tracker_config();
    let markets = areas
        .iter()
        .filter_map(|area| {
            let metrics = area.tracker_metrics.as_ref()?;
            let listing_count = metrics.listing_count.unwrap_or_default();
            if listing_count < config.minimum_market_listing_count {
                return None;
            }
            let primary_signal = non_empty(metrics.primary_signal.as_deref())
                .unwrap_or(config.fallbacks.primary_signal.as_str())
                .to_string();
            let demand_score = metrics
                .demand_score
                .unwrap_or(config.fallbacks.demand_score);
            let market_metrics = metric_values(metrics, &config.metrics);

            Some(AreaTrackerMarket {
                id: area.id.clone(),
                name: area.name.clone(),
                city: area.city.clone(),
                listing_count,
                avg_price_per_sqft: metrics
                    .avg_price_per_sqft
                    .unwrap_or(area.median_price_per_sqft),
                price_min: metrics.price_min.unwrap_or(area.price_range_per_sqft.low),
                price_max: metrics.price_max.unwrap_or(area.price_range_per_sqft.high),
                bhks: metrics.bhks.clone(),
                ready_to_move: metrics.ready_inventory_count.unwrap_or_default(),
                near_metro: metrics.metro_supported_count.unwrap_or_default(),
                top_builder: non_empty(metrics.top_builder.as_deref())
                    .unwrap_or(config.fallbacks.top_builder.as_str())
                    .to_string(),
                societies: metrics.societies.unwrap_or_default(),
                median_price_per_sqft: area.median_price_per_sqft,
                price_range_per_sqft: area.price_range_per_sqft.clone(),
                trend_direction: area.trend_direction.clone(),
                primary_signal,
                demand_score,
                recent_searches: metrics.recent_searches.unwrap_or_default(),
                last_searched_at: metrics.last_searched_at.clone(),
                evidence_gap_count: metrics.evidence_gap_count.unwrap_or_default(),
                sample_size: area.sample_size,
                last_updated: area.last_updated.clone(),
                metrics: market_metrics,
            })
        })
        .collect::<Vec<_>>();
    let mut markets = markets;
    markets.sort_by(|left, right| compare_markets(left, right, config));

    AreaTrackerResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        total_areas: areas.len(),
        total_listings: total_listings(areas, properties),
        metric_definitions: config.metrics.clone(),
        markets,
    }
}

fn metric_values(
    metrics: &AreaTrackerMetrics,
    definitions: &[AreaTrackerMetricConfig],
) -> Vec<AreaTrackerMetricValue> {
    definitions
        .iter()
        .filter_map(|definition| {
            let value = match definition.api_field.as_deref() {
                Some("listing_count") => metrics.listing_count.map(serde_json::Value::from),
                Some("ready_to_move") => metrics.ready_inventory_count.map(serde_json::Value::from),
                Some("near_metro") => metrics.metro_supported_count.map(serde_json::Value::from),
                Some("demand_score") => metrics.demand_score.map(serde_json::Value::from),
                Some("primary_signal") => {
                    metrics.primary_signal.clone().map(serde_json::Value::from)
                }
                Some("societies") => metrics.societies.map(serde_json::Value::from),
                Some(_) => None,
                None => metrics.extra_metrics.get(&definition.id).cloned(),
            }?;
            Some(AreaTrackerMetricValue {
                id: definition.id.clone(),
                label: definition.label.clone(),
                value_type: definition.value_type.clone(),
                api_field: definition.api_field.clone(),
                value,
            })
        })
        .collect()
}

fn compare_markets(
    left: &AreaTrackerMarket,
    right: &AreaTrackerMarket,
    config: &crate::dag_config::AreaTrackerConfigFile,
) -> std::cmp::Ordering {
    let left_value = sortable_metric_value(left, &config.sort.metric_id).unwrap_or_default();
    let right_value = sortable_metric_value(right, &config.sort.metric_id).unwrap_or_default();
    let metric_ordering = left_value
        .partial_cmp(&right_value)
        .unwrap_or(std::cmp::Ordering::Equal);
    let directed_metric_ordering = match config.sort.direction {
        AreaTrackerSortDirection::Asc => metric_ordering,
        AreaTrackerSortDirection::Desc => metric_ordering.reverse(),
    };
    directed_metric_ordering.then_with(|| left.name.cmp(&right.name))
}

fn sortable_metric_value(market: &AreaTrackerMarket, metric_id: &str) -> Option<f64> {
    market
        .metrics
        .iter()
        .find(|metric| metric.id == metric_id)
        .and_then(|metric| metric.value.as_f64())
}

fn total_listings(areas: &[AreaProfile], properties: &[Property]) -> usize {
    let metric_total = areas
        .iter()
        .filter_map(|area| area.tracker_metrics.as_ref()?.listing_count)
        .sum::<usize>();
    if metric_total > 0 {
        metric_total
    } else {
        properties.len()
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::area_profile::{AreaTrackerMetrics, PriceRange, RedditSignals};

    #[test]
    fn area_tracker_uses_seeded_metrics_without_runtime_derivation() {
        let mut metrics = AreaTrackerMetrics {
            listing_count: Some(2),
            avg_price_per_sqft: Some(15),
            price_min: Some(10),
            price_max: Some(20),
            bhks: vec![3],
            ready_inventory_count: Some(1),
            metro_supported_count: Some(2),
            top_builder: Some("Builder B".to_string()),
            societies: Some(1),
            primary_signal: Some("metro".to_string()),
            demand_score: Some(0.24),
            recent_searches: Some(1),
            evidence_gap_count: Some(1),
            ..AreaTrackerMetrics::default()
        };
        metrics.extra_metrics.insert(
            "test_only_absorption".to_string(),
            serde_json::Value::from(7),
        );

        let tracker = build_area_tracker(&[area_with_metrics(Some(metrics))], &[]);

        assert_eq!(tracker.total_areas, 1);
        assert_eq!(tracker.total_listings, 2);
        assert_eq!(tracker.markets[0].listing_count, 2);
        assert_eq!(tracker.markets[0].avg_price_per_sqft, 15);
        assert_eq!(tracker.markets[0].price_min, 10);
        assert_eq!(tracker.markets[0].price_max, 20);
        assert_eq!(tracker.markets[0].bhks, vec![3]);
        assert_eq!(tracker.markets[0].near_metro, 2);
        assert_eq!(tracker.markets[0].recent_searches, 1);
        assert_eq!(tracker.markets[0].evidence_gap_count, 1);
        assert_eq!(tracker.markets[0].primary_signal, "metro");
        assert!(tracker
            .metric_definitions
            .iter()
            .any(|metric| metric.fact_key == "area.market.listing_count"));
        assert!(tracker.markets[0]
            .metrics
            .iter()
            .any(|metric| metric.id == "listing_count" && metric.value == 2));
    }

    #[test]
    fn area_tracker_missing_facts_use_configured_fallbacks() {
        let tracker = build_area_tracker(
            &[area_with_metrics(Some(AreaTrackerMetrics {
                listing_count: Some(2),
                ..AreaTrackerMetrics::default()
            }))],
            &[],
        );

        assert_eq!(tracker.markets[0].primary_signal, "Market facts pending");
        assert_eq!(tracker.markets[0].demand_score, 0.0);
        assert_eq!(tracker.markets[0].top_builder, "");
    }

    #[test]
    fn area_tracker_generic_metric_values_do_not_need_api_field_branches() {
        let mut metrics = AreaTrackerMetrics::default();
        metrics
            .extra_metrics
            .insert("walkability".to_string(), serde_json::Value::from(0.82));
        let values = metric_values(
            &metrics,
            &[AreaTrackerMetricConfig {
                id: "walkability".to_string(),
                fact_key: "area.discovery.walkability".to_string(),
                label: "Walkability".to_string(),
                value_type: AreaTrackerMetricValueType::Score,
                api_field: None,
            }],
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].id, "walkability");
        assert_eq!(values[0].value, serde_json::Value::from(0.82));
    }

    fn area_with_metrics(tracker_metrics: Option<AreaTrackerMetrics>) -> AreaProfile {
        AreaProfile {
            id: "area-whitefield".to_string(),
            name: "Whitefield".to_string(),
            city: "Bengaluru".to_string(),
            median_price_per_sqft: 14_000,
            price_range_per_sqft: PriceRange {
                low: 10_000,
                high: 18_000,
            },
            trend_direction: "up".to_string(),
            trend_summary: String::new(),
            metro_access_summary: String::new(),
            airport_noise_summary: String::new(),
            traffic_summary: String::new(),
            waterlogging_summary: String::new(),
            livability_summary: "IT corridor".to_string(),
            externality_tags: Vec::new(),
            infrastructure_tags: Vec::new(),
            reddit_signals: RedditSignals {
                decision_drivers: Vec::new(),
                recurring_concerns: Vec::new(),
                sentiment_label: String::new(),
                last_updated: String::new(),
            },
            community_notes: String::new(),
            sample_size: 12,
            last_updated: "2026-07-17T00:00:00Z".to_string(),
            tracker_metrics,
        }
    }
}
