use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::discovery::{DiscoveryConfig, DiscoveryShelfConfig};
use crate::models::{Property, PropertyCard};
use crate::routes::enrichment::enrich_property_card_with_sellers;
use crate::routes::properties::overlay_serving_google_reviews;
use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct DiscoveryResponse {
    pub product_promise: String,
    pub quotes: Vec<DiscoveryQuote>,
    pub shelves: Vec<DiscoveryShelf>,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryQuote {
    pub text: String,
    pub tone: String,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryShelf {
    pub id: String,
    pub title: String,
    pub quote: String,
    pub description: String,
    pub search_query: String,
    pub proof_label: String,
    pub cards: Vec<DiscoveryShelfCard>,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryShelfCard {
    pub property: PropertyCard,
    pub reason: String,
}

#[derive(Clone)]
struct DiscoveryCandidate {
    property: Property,
    card: PropertyCard,
}

/// GET /api/discovery — DAG-shaped discovery shelves for the landing page.
///
/// Today this derives shelves from the promoted runtime property set. The API
/// is intentionally shaped like a serving product so the builder can later read
/// a gold DAG asset without changing the frontend contract.
pub async fn discovery_home(State(state): State<Arc<AppState>>) -> Json<DiscoveryResponse> {
    let graph = state.knowledge.read().await;
    let properties = state.properties.read().await;
    let sellers = state.sellers.read().await;
    let serving_bundle = state.serving_bundle.read().await.clone();
    let serving_facts = serving_bundle.as_ref().map(|bundle| &bundle.fact_index);

    let candidates: Vec<DiscoveryCandidate> = properties
        .iter()
        .map(|property| {
            let card =
                enrich_property_card_with_sellers(property, &state.societies, &graph, &sellers);
            let card = overlay_serving_google_reviews(card, &property.society_id, serving_facts);
            DiscoveryCandidate {
                property: property.clone(),
                card,
            }
        })
        .collect();

    let config = &state.discovery_config;
    let shelves = build_shelves(config, &candidates);

    Json(DiscoveryResponse {
        product_promise: config.product_promise.clone(),
        quotes: config
            .quotes
            .iter()
            .map(|quote| DiscoveryQuote {
                text: quote.text.clone(),
                tone: quote.tone.clone(),
            })
            .collect(),
        shelves,
    })
}

fn build_shelves(
    config: &DiscoveryConfig,
    candidates: &[DiscoveryCandidate],
) -> Vec<DiscoveryShelf> {
    let mut shelves = Vec::new();

    for shelf_config in &config.shelves {
        push_shelf(&mut shelves, build_shelf(shelf_config, candidates));
    }

    shelves
}

fn build_shelf(config: &DiscoveryShelfConfig, candidates: &[DiscoveryCandidate]) -> DiscoveryShelf {
    DiscoveryShelf {
        id: config.id.clone(),
        title: config.title.clone(),
        quote: config.quote.clone(),
        description: config.description.clone(),
        search_query: config.search_query.clone(),
        proof_label: config.proof_label.clone(),
        cards: cards_for_shelf(&config.id, candidates),
    }
}

fn cards_for_shelf(shelf_id: &str, candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    match shelf_id {
        "verified_value" => value_cards(candidates),
        "low_commute_pain" => commute_cards(candidates),
        "family_ready" => family_cards(candidates),
        "premium_explainable" => premium_cards(candidates),
        "area_tracker_picks" => area_tracker_cards(candidates),
        _ => Vec::new(),
    }
}

fn push_shelf(shelves: &mut Vec<DiscoveryShelf>, shelf: DiscoveryShelf) {
    if !shelf.cards.is_empty() {
        shelves.push(shelf);
    }
}

fn value_cards(candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    let mut ranked: Vec<&DiscoveryCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.property.price_per_sqft > 0)
        .collect();
    ranked.sort_by_key(|candidate| candidate.property.price_per_sqft);
    cards_from(ranked, |candidate| {
        format!(
            "{} /sqft with {} source tag{}",
            candidate.property.price_per_sqft,
            candidate.card.transparency_tags.len(),
            if candidate.card.transparency_tags.len() == 1 {
                ""
            } else {
                "s"
            }
        )
    })
}

fn commute_cards(candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    let mut ranked: Vec<&DiscoveryCandidate> = candidates
        .iter()
        .filter(|candidate| {
            candidate.property.metro_distance_mins > 0
                && candidate.property.metro_distance_mins <= 15
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.property
            .metro_distance_mins
            .cmp(&b.property.metro_distance_mins)
            .then_with(|| {
                b.property
                    .traffic_score
                    .unwrap_or(0.0)
                    .total_cmp(&a.property.traffic_score.unwrap_or(0.0))
            })
    });
    cards_from(ranked, |candidate| {
        format!(
            "{} min metro access, traffic score {:.0}%",
            candidate.property.metro_distance_mins,
            candidate.property.traffic_score.unwrap_or(0.0) * 100.0
        )
    })
}

fn family_cards(candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    let mut ranked: Vec<&DiscoveryCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.property.bhk >= 3)
        .collect();
    ranked.sort_by(|a, b| {
        family_score(&b.property)
            .total_cmp(&family_score(&a.property))
            .then_with(|| a.property.price.cmp(&b.property.price))
    });
    cards_from(ranked, |candidate| {
        format!(
            "{} BHK, society score {:.0}%, low-risk checks visible",
            candidate.property.bhk,
            candidate.property.society_quality_score.unwrap_or(0.0) * 100.0
        )
    })
}

fn premium_cards(candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    let mut ranked: Vec<&DiscoveryCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.property.price >= 20_000_000)
        .collect();
    ranked.sort_by(|a, b| {
        premium_proof_score(&b.card)
            .cmp(&premium_proof_score(&a.card))
            .then_with(|| b.property.price.cmp(&a.property.price))
    });
    cards_from(ranked, |candidate| {
        format!(
            "{} with {} proof signal{}",
            format_price(candidate.property.price),
            premium_proof_score(&candidate.card),
            if premium_proof_score(&candidate.card) == 1 {
                ""
            } else {
                "s"
            }
        )
    })
}

fn area_tracker_cards(candidates: &[DiscoveryCandidate]) -> Vec<DiscoveryShelfCard> {
    let mut active_areas: Vec<(String, usize)> = Vec::new();
    for candidate in candidates {
        if let Some((_, count)) = active_areas
            .iter_mut()
            .find(|(area, _)| area == &candidate.property.area)
        {
            *count += 1;
        } else {
            active_areas.push((candidate.property.area.clone(), 1));
        }
    }
    active_areas.sort_by(|a, b| b.1.cmp(&a.1));
    let top_areas: HashSet<String> = active_areas
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .take(6)
        .map(|(area, _)| area)
        .collect();

    let mut ranked: Vec<&DiscoveryCandidate> = candidates
        .iter()
        .filter(|candidate| top_areas.contains(&candidate.property.area))
        .collect();
    ranked.sort_by(|a, b| {
        area_strength_score(&b.property)
            .total_cmp(&area_strength_score(&a.property))
            .then_with(|| a.property.price.cmp(&b.property.price))
    });
    cards_from(ranked, |candidate| {
        format!(
            "{} has active supply and {} BHK coverage",
            candidate.property.area, candidate.property.bhk
        )
    })
}

fn cards_from<F>(ranked: Vec<&DiscoveryCandidate>, reason_for: F) -> Vec<DiscoveryShelfCard>
where
    F: Fn(&DiscoveryCandidate) -> String,
{
    let mut seen_societies = HashSet::new();
    let mut cards = Vec::new();
    for candidate in ranked {
        if !seen_societies.insert(candidate.property.society_id.clone()) {
            continue;
        }
        cards.push(DiscoveryShelfCard {
            property: candidate.card.clone(),
            reason: reason_for(candidate),
        });
        if cards.len() == 3 {
            break;
        }
    }
    cards
}

fn family_score(property: &Property) -> f64 {
    property.society_quality_score.unwrap_or(0.0)
        + property.sunlight_score.unwrap_or(0.0) * 0.3
        + (1.0 - property.litigation_risk.unwrap_or(1.0)) * 0.25
        + (1.0 - property.waterlogging_risk_score.unwrap_or(1.0)) * 0.2
}

fn premium_proof_score(card: &PropertyCard) -> usize {
    let mut score = card.transparency_tags.len();
    if card.root_source.is_some() {
        score += 1;
    }
    if card.project_status_display.is_some() {
        score += 1;
    }
    if card.google_rating.is_some() {
        score += 1;
    }
    score
}

fn area_strength_score(property: &Property) -> f64 {
    property.society_quality_score.unwrap_or(0.0)
        + property.traffic_score.unwrap_or(0.0) * 0.25
        + property.resale_strength_score.unwrap_or(0.0) * 0.2
}

fn format_price(price: u64) -> String {
    if price >= 10_000_000 {
        format!("{:.1} Cr", price as f64 / 10_000_000.0)
    } else if price >= 100_000 {
        format!("{:.0} L", price as f64 / 100_000.0)
    } else {
        price.to_string()
    }
}
