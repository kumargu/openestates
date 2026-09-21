use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::models::Property;
use crate::serving::{GoogleReviewEvidence, ServingFactIndex, SocietyFactProjection};

const DISCOVERY_CONFIG_JSON: &str = include_str!("../../app/config/product/discovery_home.json");
const MAX_DISCOVERY_SHELVES: usize = 8;
const MAX_DISCOVERY_CARDS_PER_SHELF: usize = 24;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub product_promise: String,
    pub quotes: Vec<DiscoveryQuoteConfig>,
    pub product_story: DiscoveryProductStoryConfig,
    pub dedupe_across_shelves: bool,
    pub collection_planner: CollectionPlannerConfig,
    pub card_signals: Vec<DiscoveryCardSignalConfig>,
    pub shelves: Vec<DiscoveryShelfConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryCardSignalConfig {
    pub id: String,
    pub fact_key: String,
    pub projection: DiscoverySignalProjection,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySignalProjection {
    Count,
    LatestNumeric,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CollectionPlannerConfig {
    pub max_collections: usize,
    pub card_limit: usize,
    pub minimum_cards: usize,
    pub fallback_count: usize,
    pub budget_expansion_factor: f64,
    pub nearby_max_hops: u8,
    pub nearby_max_distance_km: f64,
    pub currency: String,
    pub price_unit: u64,
    pub price_suffix: String,
    pub strategies: Vec<CollectionStrategyConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CollectionStrategyConfig {
    pub id: String,
    pub strategy: String,
    pub title_template: String,
    pub generic_title_template: String,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryProductStoryConfig {
    pub title: String,
    pub items: Vec<DiscoveryProductStoryItemConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryProductStoryItemConfig {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action_label: String,
    pub href: String,
    pub image_src: String,
    pub image_src_narrow: String,
    pub image_alt: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryQuoteConfig {
    pub text: String,
    pub tone: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryShelfConfig {
    pub id: String,
    pub title: String,
    pub quote: String,
    pub description: String,
    pub search_query: String,
    pub receipt_copy: String,
    pub card_limit: usize,
    pub minimum_cards: usize,
    pub require_image: bool,
    pub show_card_area: bool,
    #[serde(default)]
    pub group_by: Option<DiscoveryGroupConfig>,
    pub filters: Vec<DiscoveryFilterConfig>,
    pub sort: Vec<DiscoverySortConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryGroupConfig {
    pub field: String,
    pub limit: usize,
    pub title_template: String,
    pub description_template: String,
    pub search_query_template: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryFilterConfig {
    pub field: String,
    pub operator: DiscoveryFilterOperator,
    pub value: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryFilterOperator {
    Exists,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoverySortConfig {
    pub field: String,
    pub direction: DiscoverySortDirection,
}

/// Bounded landing-card projection built once with the immutable search runtime.
/// It deliberately excludes evidence blobs, descriptions, tags, and source panels.
#[derive(Clone, Debug, Serialize)]
pub struct BrowsePropertyCard {
    pub id: String,
    pub society_id: String,
    pub title: String,
    pub society_name: String,
    pub area: String,
    pub image: String,
    pub bhk: u32,
    pub price: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_min: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_max: Option<u64>,
    pub sqft: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_review_count: Option<u32>,
    #[serde(skip_serializing)]
    pub signals: BTreeMap<String, f64>,
    pub detail_href: String,
    pub save_id: String,
}

impl BrowsePropertyCard {
    pub fn from_runtime(
        property: &Property,
        society_entity_id: String,
        society_name: String,
        facts: &ServingFactIndex,
        signal_config: &[DiscoveryCardSignalConfig],
    ) -> Self {
        let projection = SocietyFactProjection::from_index(facts, &society_entity_id);
        let reviews = projection.project_google_reviews(GoogleReviewEvidence::default());
        let signals = signal_config
            .iter()
            .filter_map(|signal| {
                let value = match signal.projection {
                    DiscoverySignalProjection::Count => {
                        projection.records(&signal.fact_key).len() as f64
                    }
                    DiscoverySignalProjection::LatestNumeric => projection
                        .latest_numeric(&signal.fact_key)
                        .map(|fact| fact.value)?,
                };
                (value.is_finite() && value > 0.0).then(|| (signal.id.clone(), value))
            })
            .collect();
        let image = if property.hero_image.trim().is_empty() {
            property.images.first().cloned().unwrap_or_default()
        } else {
            property.hero_image.clone()
        };
        Self {
            id: property.id.clone(),
            society_id: society_entity_id,
            title: property.title.clone(),
            society_name,
            area: property.area.clone(),
            image,
            bhk: property.bhk,
            price: property.price,
            price_min: property.price_min,
            price_max: property.price_max,
            sqft: property.listed_area_sqft(),
            google_rating: reviews.rating,
            google_review_count: reviews.review_count,
            signals,
            detail_href: format!("/property/{}", property.id),
            save_id: property.id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySortDirection {
    Asc,
    Desc,
}

pub fn load_discovery_config() -> DiscoveryConfig {
    discovery_config().clone()
}

pub fn discovery_config() -> &'static DiscoveryConfig {
    static CONFIG: OnceLock<DiscoveryConfig> = OnceLock::new();
    CONFIG.get_or_init(parse_discovery_config)
}

fn parse_discovery_config() -> DiscoveryConfig {
    let config: DiscoveryConfig = serde_json::from_str(DISCOVERY_CONFIG_JSON)
        .expect("app/config/product/discovery_home.json must be valid JSON");
    assert!(
        (1..=3).contains(&config.collection_planner.max_collections)
            && (1..=12).contains(&config.collection_planner.card_limit)
            && config.collection_planner.minimum_cards <= config.collection_planner.card_limit
            && config.collection_planner.fallback_count
                <= config.collection_planner.max_collections
            && config.collection_planner.budget_expansion_factor >= 1.0
            && config.collection_planner.nearby_max_hops > 0
            && config.collection_planner.nearby_max_distance_km > 0.0
            && config.collection_planner.price_unit > 0
            && !config.collection_planner.strategies.is_empty(),
        "discovery collection planner must stay bounded and complete"
    );
    assert!(
        !config.shelves.is_empty() && config.shelves.len() <= MAX_DISCOVERY_SHELVES,
        "discovery config must contain a bounded number of shelves"
    );
    assert!(
        !config.product_story.title.is_empty()
            && (1..=3).contains(&config.product_story.items.len())
            && config.product_story.items.iter().all(|item| {
                !item.id.is_empty()
                    && !item.title.is_empty()
                    && !item.description.is_empty()
                    && !item.action_label.is_empty()
                    && item.href.starts_with('/')
                    && item.image_src.starts_with('/')
                    && item.image_src_narrow.starts_with('/')
                    && !item.image_alt.is_empty()
            }),
        "discovery product story must contain one to three complete actions"
    );
    assert!(
        !config.card_signals.is_empty()
            && config
                .card_signals
                .iter()
                .all(|signal| { !signal.id.is_empty() && !signal.fact_key.is_empty() }),
        "discovery browse-card signals must be config-owned and complete"
    );
    assert!(
        config.shelves.iter().all(|shelf| {
            (1..=MAX_DISCOVERY_CARDS_PER_SHELF).contains(&shelf.card_limit)
                && (1..=shelf.card_limit).contains(&shelf.minimum_cards)
                && !shelf.filters.is_empty()
                && !shelf.sort.is_empty()
                && shelf.group_by.as_ref().is_none_or(|group| {
                    !group.field.is_empty()
                        && group.limit > 0
                        && !group.title_template.is_empty()
                        && !group.description_template.is_empty()
                        && !group.search_query_template.is_empty()
                })
                && shelf.filters.iter().all(|filter| {
                    !filter.field.is_empty()
                        && (matches!(filter.operator, DiscoveryFilterOperator::Exists)
                            || filter.value.is_some())
                })
                && shelf.sort.iter().all(|sort| !sort.field.is_empty())
        }),
        "every discovery shelf needs bounded cards and valid filter/sort policy"
    );
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_config_loads_from_app_config_embed() {
        let config = load_discovery_config();
        assert!(!config.product_promise.is_empty());
        assert_eq!(config.product_story.items.len(), 2);
        assert!(!config.shelves.is_empty());
        assert!(
            config
                .shelves
                .iter()
                .all(|shelf| !shelf.receipt_copy.is_empty()),
            "every shelf needs receipt_copy"
        );
        assert!(config.dedupe_across_shelves);
    }
}

/// Immutable, uncapped property rankings. Selection and society deduplication happen
/// after journey eligibility, so an ineligible configuration cannot hide its sibling.
#[derive(Clone, Debug)]
pub struct RankedDiscoveryShelf {
    pub id: String,
    pub title: String,
    pub config_index: usize,
    pub property_ids: Vec<String>,
}

pub fn hydrate_shelf_rankings(
    cards: &HashMap<String, BrowsePropertyCard>,
) -> Vec<RankedDiscoveryShelf> {
    let config = discovery_config();
    let fields = cards
        .values()
        .map(|card| {
            let mut fields =
                serde_json::to_value(card).expect("browse cards serialize during hydration");
            fields["signals"] =
                serde_json::to_value(&card.signals).expect("ranking fields serialize");
            (card, fields)
        })
        .collect::<Vec<_>>();
    let mut shelves = Vec::new();
    for (config_index, shelf) in config.shelves.iter().enumerate() {
        let mut ranked = fields
            .iter()
            .filter(|(card, fields)| {
                (!shelf.require_image || !card.image.is_empty())
                    && shelf
                        .filters
                        .iter()
                        .all(|filter| matches_filter(fields, filter))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|(a, af), (b, bf)| {
            for sort in &shelf.sort {
                let ordering = compare_field(af, bf, &sort.field, sort.direction);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.id.cmp(&b.id)
        });
        if let Some(group) = &shelf.group_by {
            let mut groups = BTreeMap::<String, Vec<&BrowsePropertyCard>>::new();
            for (card, fields) in ranked {
                if let Some(value) = field(fields, &group.field)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    groups.entry(value.to_string()).or_default().push(card);
                }
            }
            let mut groups = groups.into_iter().collect::<Vec<_>>();
            let society_count = |cards: &[&BrowsePropertyCard]| {
                cards
                    .iter()
                    .map(|c| &c.society_id)
                    .collect::<HashSet<_>>()
                    .len()
            };
            groups.sort_by(|(a, ac), (b, bc)| {
                society_count(bc)
                    .cmp(&society_count(ac))
                    .then_with(|| a.cmp(b))
            });
            for (value, cards) in groups {
                shelves.push(RankedDiscoveryShelf {
                    id: format!("{}-{}", shelf.id, slug(&value)),
                    title: group.title_template.replace("{value}", &value),
                    config_index,
                    property_ids: cards.iter().map(|c| c.id.clone()).collect(),
                });
            }
        } else {
            shelves.push(RankedDiscoveryShelf {
                id: shelf.id.clone(),
                title: shelf.title.clone(),
                config_index,
                property_ids: ranked.iter().map(|(card, _)| card.id.clone()).collect(),
            });
        }
    }
    shelves
}

pub fn select_shelf_cards(
    shelf: &RankedDiscoveryShelf,
    cards: &HashMap<String, BrowsePropertyCard>,
    used: &HashSet<String>,
    eligible: Option<&HashSet<String>>,
    limit: usize,
) -> Vec<BrowsePropertyCard> {
    let mut seen = used.clone();
    shelf
        .property_ids
        .iter()
        .filter(|id| eligible.is_none_or(|ids| ids.contains(*id)))
        .filter_map(|id| cards.get(id))
        .filter(|card| seen.insert(card.society_id.clone()))
        .take(limit)
        .cloned()
        .collect()
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn field<'a>(fields: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.')
        .try_fold(fields, |value, segment| value.get(segment))
}

fn matches_filter(fields: &serde_json::Value, filter: &DiscoveryFilterConfig) -> bool {
    let candidate = field(fields, &filter.field);
    match filter.operator {
        DiscoveryFilterOperator::Exists => candidate.is_some_and(|value| !value.is_null()),
        DiscoveryFilterOperator::Gt => numeric_filter(candidate, filter.value, |a, b| a > b),
        DiscoveryFilterOperator::Gte => numeric_filter(candidate, filter.value, |a, b| a >= b),
        DiscoveryFilterOperator::Lt => numeric_filter(candidate, filter.value, |a, b| a < b),
        DiscoveryFilterOperator::Lte => numeric_filter(candidate, filter.value, |a, b| a <= b),
    }
}

fn numeric_filter(
    candidate: Option<&serde_json::Value>,
    configured: Option<f64>,
    predicate: impl Fn(f64, f64) -> bool,
) -> bool {
    candidate
        .and_then(serde_json::Value::as_f64)
        .zip(configured)
        .is_some_and(|(candidate, configured)| predicate(candidate, configured))
}

fn compare_field(
    a: &serde_json::Value,
    b: &serde_json::Value,
    path: &str,
    direction: DiscoverySortDirection,
) -> Ordering {
    let a = field(a, path).filter(|value| !value.is_null());
    let b = field(b, path).filter(|value| !value.is_null());
    let ordering = match (a, b) {
        (Some(serde_json::Value::Number(a)), Some(serde_json::Value::Number(b))) => a
            .as_f64()
            .zip(b.as_f64())
            .map_or(Ordering::Equal, |(a, b)| a.total_cmp(&b)),
        (Some(serde_json::Value::String(a)), Some(serde_json::Value::String(b))) => a.cmp(b),
        (Some(serde_json::Value::Bool(a)), Some(serde_json::Value::Bool(b))) => a.cmp(b),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        _ => Ordering::Equal,
    };
    match direction {
        DiscoverySortDirection::Asc => ordering,
        DiscoverySortDirection::Desc => ordering.reverse(),
    }
}

#[cfg(test)]
mod ranking_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn configured_numeric_filters_and_sorting_are_generic() {
        let fields = json!({ "price": 12_000_000, "google_rating": 4.6 });
        assert!(matches_filter(
            &fields,
            &DiscoveryFilterConfig {
                field: "price".to_string(),
                operator: DiscoveryFilterOperator::Lt,
                value: Some(15_000_000.0),
            },
        ));
        assert_eq!(
            compare_field(
                &json!({ "google_rating": 4.6 }),
                &json!({ "google_rating": 4.2 }),
                "google_rating",
                DiscoverySortDirection::Desc,
            ),
            Ordering::Less,
        );
        assert_eq!(
            compare_field(
                &json!({ "google_rating": 4.6 }),
                &json!({}),
                "google_rating",
                DiscoverySortDirection::Desc,
            ),
            Ordering::Less,
            "missing values stay last in either sort direction",
        );
    }
}
