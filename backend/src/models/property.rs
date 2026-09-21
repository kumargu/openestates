use serde::{Deserialize, Serialize};

use crate::decision_labels::{DecisionCheckSummary, DecisionLabel};
use crate::routes::enrichment::DataFreshness;

pub fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
pub struct InventoryAvailability {
    pub bedrooms: Availability,
    pub price: Availability,
    pub area: Availability,
}

#[derive(schemars::JsonSchema, Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub id: String,
    pub title: String,
    pub area: String,
    pub area_id: String,
    pub city: String,
    pub society_id: String,
    pub builder_name: String,
    pub property_type: String,
    pub listing_type: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bhk: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price: u64,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_min: Option<u64>,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_max: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price_per_sqft: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub carpet_area_sqft: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub super_builtup_sqft: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "crate::models::Measurement")]
    pub area_measurement: Option<crate::models::Measurement>,
    pub floor: u32,
    pub total_floors: u32,
    pub facing: String,
    pub possession_status: String,
    pub metro_distance_mins: u32,
    pub maintenance_cost_monthly: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub society_quality_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub builder_quality_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub document_completeness_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub litigation_risk: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub noise_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub sunlight_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub airport_noise_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub waterlogging_risk_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub traffic_score: Option<f64>,
    pub days_on_market: u32,
    #[serde(default)]
    pub greenery_score: Option<f64>,
    #[serde(default)]
    pub open_space_score: Option<f64>,
    #[serde(default)]
    pub resale_strength_score: Option<f64>,
    #[serde(default)]
    pub interest_level: Option<String>,
    #[serde(default)]
    pub saves_last_7d: Option<u32>,
    #[serde(default)]
    pub offers_last_7d: Option<u32>,
    pub images: Vec<String>,
    pub hero_image: String,
    pub description_summary: String,
    pub transparency_tags: Vec<String>,
    pub source_reference: String,
}

impl Property {
    pub fn inventory_availability(&self) -> InventoryAvailability {
        let availability = |present| {
            if present {
                Availability::Available
            } else {
                Availability::Unavailable
            }
        };
        InventoryAvailability {
            bedrooms: availability(self.bhk > 0),
            price: availability(self.price > 0),
            area: availability(self.area_measurement.is_some()),
        }
    }

    /// Shared projection of admitted runtime attributes. Serving overlays add
    /// supported society context; display fields never establish eligibility.
    pub fn to_card(&self, society_name: &str) -> PropertyCard {
        use crate::routes::enrichment::{property_node_id, society_node_id};
        let p = self;
        crate::models::PropertyCard {
            availability: self.inventory_availability(),
            id: p.id.clone(),
            kg_entity_refs: KgEntityRefs {
                property_entity_id: property_node_id(&p.id),
                society_entity_id: society_node_id(&p.society_id),
                area_entity_id: p.area_id.clone(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            title: p.title.clone(),
            area: p.area.clone(),
            price: p.price,
            price_min: p.price_min,
            price_max: p.price_max,
            price_per_sqft: p.price_per_sqft,
            bhk: p.bhk,
            sqft: p.listed_area_sqft(),
            carpet_area_sqft: p.carpet_area_sqft,
            super_builtup_sqft: p.super_builtup_sqft,
            area_measurement: p.area_measurement.clone(),
            society_name: society_name.to_string(),
            builder_name: p.builder_name.clone(),
            images: p.images.clone(),
            hero_image: p.hero_image.clone(),
            transparency_tags: crate::routes::enrichment::compact_transparency_tags(
                &p.transparency_tags,
            ),
            description_summary: p.description_summary.clone(),
            possession_status: p.possession_status.clone(),
            metro_distance_mins: p.metro_distance_mins,
            floor: p.floor,
            total_floors: p.total_floors,
            facing: p.facing.clone(),
            google_rating: None,
            google_review_count: None,
            google_reviews_url: None,
            society_land_acres: None,
            open_space_pct: None,
            root_source: None,
            project_status: None,
            project_status_display: None,
            home_state_display: None,
            builder_delivery_display: None,
            data_freshness: None,
            floor_plan_preview_url: None,
            plan_carpet_area_sqft: None,
            plan_sale_area_sqft: None,
            plan_configuration_type: None,
            decision_labels: Vec::new(),
            decision_check_summary: None,
        }
    }

    /// A runtime card is representable when it has a known commercial/config
    /// identity or usable media. The promoted serving eligibility policy owns
    /// the stricter buyer-visible media gate. Explicit budget and BHK
    /// constraints still fail closed on zero/unknown values.
    pub fn listed_area_sqft(&self) -> u32 {
        self.area_measurement
            .as_ref()
            .map(|area| area.value.round() as u32)
            .unwrap_or_else(|| self.carpet_area_sqft.max(self.super_builtup_sqft))
    }

    pub fn is_listable(&self) -> bool {
        !self.society_id.is_empty()
            || self.price > 0
            || self.bhk > 0
            || !self.hero_image.trim().is_empty()
            || self.images.iter().any(|image| !image.trim().is_empty())
    }
}

/// UI-ready property card for the results page.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
pub struct PropertyCard {
    pub availability: InventoryAvailability,
    pub id: String,
    /// Stable entity handles attached to the serving bundle.
    ///
    /// This is the contract that keeps cards and detail pages from becoming a
    /// fixed list of hardcoded sections. The flat fields in `PropertyCard`
    /// support fast first paint and search-result scanning. `kg_entity_refs`
    /// supports the second layer: expandable evidence, compare rows, side
    /// panels, source drill-down, and dynamic sections that only appear when
    /// facts actually exist.
    ///
    /// Backend rules:
    /// - Populate these IDs from app-owned entity identity, never from UI labels.
    /// - Add new fact families to serving/source panels instead of adding
    ///   one-off card fields unless the value is needed on the hot first-paint path.
    /// - It is okay for some referenced concepts to have sparse facts. The UI
    ///   should render from fact availability and confidence.
    pub kg_entity_refs: KgEntityRefs,
    pub title: String,
    pub area: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price: u64,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_min: Option<u64>,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_max: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price_per_sqft: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bhk: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sqft: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub carpet_area_sqft: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub super_builtup_sqft: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "crate::models::Measurement")]
    pub area_measurement: Option<crate::models::Measurement>,
    pub society_name: String,
    pub builder_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    pub hero_image: String,
    pub transparency_tags: Vec<String>,
    pub description_summary: String,
    pub possession_status: String,
    pub metro_distance_mins: u32,
    pub floor: u32,
    pub total_floors: u32,
    pub facing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub google_review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub google_reviews_url: Option<String>,
    /// RERA-backed project land extent. Kept on the card because compare needs it at first paint.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub society_land_acres: Option<f64>,
    /// RERA-backed open-area percentage. Omitted when the source did not expose it clearly.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub open_space_pct: Option<f64>,
    /// Where the society data originally came from: "rera", "seller", "discovered", "legacy"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub root_source: Option<String>,
    /// Machine-readable project status: "ready_to_move", "under_construction", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub project_status: Option<String>,
    /// Human-readable project status from skill's display_template, e.g. "Ready to Move — delivered 31/01/2020"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub project_status_display: Option<String>,
    /// Compact buyer-facing state signal for result tiles, e.g. "Delivered · 5-10 yrs old".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub home_state_display: Option<String>,
    /// Human-readable builder delivery track record, e.g. "Builder delivers on time: 100% of projects"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub builder_delivery_display: Option<String>,
    /// Data freshness — how recent and rich the underlying data is
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "DataFreshness")]
    pub data_freshness: Option<DataFreshness>,
    /// Representative floor-plan preview for this listing's BHK (compare-ready).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub floor_plan_preview_url: Option<String>,
    /// Plan carpet area (sqft) for the matched configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub plan_carpet_area_sqft: Option<u32>,
    /// Plan sale / super built-up area (sqft) for usable-space compare.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub plan_sale_area_sqft: Option<u32>,
    /// Matched configuration label, e.g. "3BHK".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub plan_configuration_type: Option<String>,
    /// Config-derived decision labels for compare, notes, and compact review surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_labels: Vec<DecisionLabel>,
    /// Grouped compact check summary for property details, compare, and notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "DecisionCheckSummary")]
    pub decision_check_summary: Option<DecisionCheckSummary>,
}

/// Minimal entity identity bundle attached to property/search/detail responses.
///
/// These fields are stable API identifiers, not display copy and not a complete
/// serving export. They exist so the UI can ask follow-up endpoints for richer
/// context when a user shows intent: opens a property, expands a card, compares
/// homes, clicks a source trail, or requests a nearby/risk/community breakdown.
///
/// Current usage pattern:
/// 1. Render fast listing data from `PropertyCard` or `PropertyDetailResponse`.
/// 2. Use `source_entity_ids` as opaque provenance handles for the property,
///    society, area, and builder.
/// 3. Use source/evidence read models when the UI needs a larger drill-down
///    such as builder portfolio, nearby projects, or lineage.
/// 4. Build optional UI sections from facts with source/confidence metadata.
/// 5. Hide sections that have no backed facts instead of rendering empty cards.
///
/// Important distinction: these are KG node IDs, not necessarily canonical RERA
/// IDs. Some societies have an alias node such as `society:prestige-park-grove`
/// while lake artifacts may also contain a RERA-rooted canonical ID. The UI
/// should not infer canonicalization from the string shape. It should treat the
/// IDs as opaque handles and follow the API.
#[derive(schemars::JsonSchema, Debug, Clone, Serialize)]
pub struct KgEntityRefs {
    /// Listing-level node for facts specific to this flat/unit/listing.
    pub property_entity_id: String,
    /// Society/project node for RERA, reviews, nearby places, amenities, and
    /// community evidence.
    pub society_entity_id: String,
    /// Area/locality node for traffic, waterlogging, metro, schools, price trend,
    /// and other externalities.
    pub area_entity_id: String,
    /// Builder node when the society has a known BuiltBy edge in the KG.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub builder_entity_id: Option<String>,
    /// Existing graph nodes the UI can safely prefetch first.
    ///
    /// This list is backend-filtered to nodes present in the current KG, sorted,
    /// and deduplicated. It may omit an otherwise valid field ID if that node has
    /// not been materialized yet. UI code should treat it as a convenient fetch
    /// plan, not as a complete semantic model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_entity_ids: Vec<String>,
}
