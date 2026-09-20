use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::discovery::{discovery_config, select_shelf_cards, BrowsePropertyCard};
use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct DiscoveryResponse {
    pub product_story: DiscoveryProductStory,
    pub shelves: Vec<DiscoveryShelf>,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryProductStory {
    pub title: String,
    pub items: Vec<DiscoveryProductStoryItem>,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryProductStoryItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action_label: String,
    pub href: String,
    pub image_src: String,
    pub image_src_narrow: String,
    pub image_alt: String,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryShelf {
    pub id: String,
    pub title: String,
    pub show_card_area: bool,
    pub cards: Vec<DiscoveryShelfCard>,
}

#[derive(Serialize, Clone)]
pub struct DiscoveryShelfCard {
    pub property: BrowsePropertyCard,
}

pub async fn discovery_home(State(state): State<Arc<AppState>>) -> Json<DiscoveryResponse> {
    let runtime = state.search_runtime.load_full();
    let config = discovery_config();
    let mut used = HashSet::new();
    let mut groups = HashMap::<usize, usize>::new();
    let mut shelves = Vec::new();
    for ranked in &runtime.discovery_shelves {
        let shelf = &config.shelves[ranked.config_index];
        let group_limit = shelf.group_by.as_ref().map_or(1, |group| group.limit);
        if groups.get(&ranked.config_index).copied().unwrap_or(0) >= group_limit {
            continue;
        }
        let cards =
            select_shelf_cards(ranked, &runtime.browse_cards, &used, None, shelf.card_limit);
        if cards.len() < shelf.minimum_cards {
            continue;
        }
        *groups.entry(ranked.config_index).or_default() += 1;
        if config.dedupe_across_shelves {
            used.extend(cards.iter().map(|c| c.society_id.clone()));
        }
        shelves.push(DiscoveryShelf {
            id: ranked.id.clone(),
            title: ranked.title.clone(),
            show_card_area: shelf.show_card_area,
            cards: cards
                .into_iter()
                .map(|property| DiscoveryShelfCard { property })
                .collect(),
        });
    }
    Json(DiscoveryResponse {
        product_story: DiscoveryProductStory {
            title: config.product_story.title.clone(),
            items: config
                .product_story
                .items
                .iter()
                .map(|item| DiscoveryProductStoryItem {
                    id: item.id.clone(),
                    title: item.title.clone(),
                    description: item.description.clone(),
                    action_label: item.action_label.clone(),
                    href: item.href.clone(),
                    image_src: item.image_src.clone(),
                    image_src_narrow: item.image_src_narrow.clone(),
                    image_alt: item.image_alt.clone(),
                })
                .collect(),
        },
        shelves,
    })
}
