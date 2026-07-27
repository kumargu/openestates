use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::knowledge::search_event::SearchEvent;
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
}

/// GET /api/areas/tracker — current micro-market inventory plus live search demand.
pub async fn area_tracker(State(state): State<Arc<AppState>>) -> Json<AreaTrackerResponse> {
    let properties = state.properties.read().await;
    let areas = state.areas.read().await;
    let graph = state.knowledge.read().await;
    Json(build_area_tracker(&areas, &properties, &graph.search_log))
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

fn build_area_tracker(
    areas: &[AreaProfile],
    properties: &[Property],
    search_log: &[SearchEvent],
) -> AreaTrackerResponse {
    let markets = areas
        .iter()
        .filter_map(|area| {
            let area_key = normalize_area(&area.name);
            let area_properties = properties
                .iter()
                .filter(|property| normalize_area(&property.area) == area_key)
                .collect::<Vec<_>>();
            let listing_count = area_properties.len();
            if listing_count < 2 {
                return None;
            }
            let avg_price_per_sqft = average_price_per_sqft(&area_properties);
            let price_min = area_properties
                .iter()
                .filter(|property| property.price > 0)
                .map(|property| property.price)
                .min()
                .unwrap_or(0);
            let price_max = area_properties
                .iter()
                .filter(|property| property.price > 0)
                .map(|property| property.price)
                .max()
                .unwrap_or(0);
            let bhks = area_properties
                .iter()
                .map(|property| property.bhk)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let ready_to_move = area_properties
                .iter()
                .filter(|property| property.possession_status == "ready")
                .count();
            let near_metro = area_properties
                .iter()
                .filter(|property| property.metro_distance_mins <= 15)
                .count();
            let top_builder = top_builder(&area_properties);
            let societies = area_properties
                .iter()
                .map(|property| property.society_id.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            let matching_searches = search_log
                .iter()
                .filter(|event| {
                    event
                        .intent
                        .area
                        .as_deref()
                        .is_some_and(|intent_area| normalize_area(intent_area) == area_key)
                })
                .collect::<Vec<_>>();
            let recent_searches = matching_searches.len();
            let last_searched_at = matching_searches
                .iter()
                .map(|event| event.timestamp)
                .max()
                .map(|timestamp| timestamp.to_rfc3339());
            let evidence_gap_count = matching_searches
                .iter()
                .map(|event| event.enrichment_gaps.len())
                .sum::<usize>();
            let primary_signal = area
                .externality_tags
                .first()
                .or_else(|| area.infrastructure_tags.first())
                .cloned()
                .unwrap_or_else(|| area.livability_summary.clone());
            let demand_score = demand_score(recent_searches, evidence_gap_count, listing_count);

            Some(AreaTrackerMarket {
                id: area.id.clone(),
                name: area.name.clone(),
                city: area.city.clone(),
                listing_count,
                avg_price_per_sqft,
                price_min,
                price_max,
                bhks,
                ready_to_move,
                near_metro,
                top_builder,
                societies,
                median_price_per_sqft: area.median_price_per_sqft,
                price_range_per_sqft: area.price_range_per_sqft.clone(),
                trend_direction: area.trend_direction.clone(),
                primary_signal,
                demand_score,
                recent_searches,
                last_searched_at,
                evidence_gap_count,
                sample_size: area.sample_size,
                last_updated: area.last_updated.clone(),
            })
        })
        .collect::<Vec<_>>();
    let mut markets = markets;
    markets.sort_by_key(|market| std::cmp::Reverse(market.listing_count));

    AreaTrackerResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        total_areas: areas.len(),
        total_listings: properties.len(),
        markets,
    }
}

fn normalize_area(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn average_price_per_sqft(properties: &[&Property]) -> u64 {
    let priced = properties
        .iter()
        .filter(|property| property.price_per_sqft > 0)
        .collect::<Vec<_>>();
    if priced.is_empty() {
        return 0;
    }
    let total = priced
        .iter()
        .map(|property| property.price_per_sqft)
        .sum::<u64>();
    ((total as f64 / priced.len() as f64).round()) as u64
}

fn top_builder(properties: &[&Property]) -> String {
    let mut first_seen = HashMap::<&str, usize>::new();
    let mut counts = HashMap::<&str, usize>::new();
    for (index, property) in properties.iter().enumerate() {
        first_seen.entry(&property.builder_name).or_insert(index);
        *counts.entry(&property.builder_name).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .max_by(|(left_name, left_count), (right_name, right_count)| {
            left_count.cmp(right_count).then_with(|| {
                first_seen
                    .get(right_name)
                    .unwrap_or(&usize::MAX)
                    .cmp(first_seen.get(left_name).unwrap_or(&usize::MAX))
            })
        })
        .map(|(name, _)| name.to_string())
        .unwrap_or_default()
}

fn demand_score(recent_searches: usize, evidence_gap_count: usize, listing_count: usize) -> f32 {
    let search_pull = (recent_searches as f32 / 10.0).min(0.7);
    let gap_pull = (evidence_gap_count as f32 / 20.0).min(0.2);
    let supply_pull = (listing_count as f32 / 50.0).min(0.1);
    ((search_pull + gap_pull + supply_pull) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::knowledge::search_event::EnrichmentGap;
    use crate::models::area_profile::{PriceRange, RedditSignals};
    use crate::search::intent::SearchIntent;

    #[test]
    fn area_tracker_combines_inventory_and_search_demand() {
        let area = AreaProfile {
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
            externality_tags: vec!["metro".to_string()],
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
        };
        let property = Property {
            id: "p1".to_string(),
            title: "Test".to_string(),
            area: "Whitefield".to_string(),
            area_id: "area-whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "soc-test".to_string(),
            builder_name: String::new(),
            property_type: String::new(),
            listing_type: String::new(),
            bhk: 3,
            price: 10,
            price_per_sqft: 10,
            carpet_area_sqft: 1,
            super_builtup_sqft: 1,
            floor: 0,
            total_floors: 0,
            facing: String::new(),
            possession_status: String::new(),
            metro_distance_mins: 0,
            maintenance_cost_monthly: 0,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 0,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        };
        let mut second_property = property.clone();
        second_property.id = "p2".to_string();
        second_property.builder_name = "Builder B".to_string();
        second_property.price = 20;
        second_property.price_per_sqft = 20;
        let mut event = SearchEvent::new(
            "3BHK Whitefield".to_string(),
            SearchIntent {
                area: Some("white field".to_string()),
                excluded_areas: Vec::new(),
                bhk: Some(3),
                budget_max: None,
                hard_constraints: Vec::new(),
                preferences: Vec::new(),
                positive_preferences: Vec::new(),
                negative_preferences: Vec::new(),
                accepted_tradeoffs: Vec::new(),
                unsupported_inventory_types: Vec::new(),
                buyer_archetype: None,
            },
            1,
        );
        event.timestamp = Utc::now();
        event.enrichment_gaps.push(EnrichmentGap {
            entity_id: "society:test".to_string(),
            missing_fact: "traffic".to_string(),
            reason: "area evidence".to_string(),
        });

        let tracker = build_area_tracker(&[area], &[property, second_property], &[event]);

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
    }
}
