use serde::{Deserialize, Serialize};

use crate::decision_labels::{DecisionCheckSummary, DecisionLabel};
use crate::routes::enrichment::DataFreshness;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub bhk: u32,
    pub price: u64,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_min: Option<u64>,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_max: Option<u64>,
    pub price_per_sqft: u64,
    pub carpet_area_sqft: u32,
    pub super_builtup_sqft: u32,
    pub floor: u32,
    pub total_floors: u32,
    pub facing: String,
    pub possession_status: String,
    pub metro_distance_mins: u32,
    pub maintenance_cost_monthly: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub society_quality_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_quality_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_completeness_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub litigation_risk: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunlight_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airport_noise_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waterlogging_risk_score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    /// Buyer-facing surfaces can show priced homes, or explicit project
    /// configurations whose price is still unavailable.
    pub fn is_listable(&self) -> bool {
        self.price > 0
            || (self.bhk > 0
                && self
                    .transparency_tags
                    .iter()
                    .any(|tag| tag.eq_ignore_ascii_case("Price unavailable")))
    }
}

/// UI-ready property card for the results page.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyCard {
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
    pub price: u64,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_min: Option<u64>,
    /// Inclusive listing band when the source is a range, not a point asking price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_max: Option<u64>,
    pub price_per_sqft: u64,
    pub bhk: u32,
    pub sqft: u32,
    pub carpet_area_sqft: u32,
    pub super_builtup_sqft: u32,
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
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_reviews_url: Option<String>,
    /// RERA-backed project land extent. Kept on the card because compare needs it at first paint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub society_land_acres: Option<f64>,
    /// RERA-backed open-area percentage. Omitted when the source did not expose it clearly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_space_pct: Option<f64>,
    /// Where the society data originally came from: "rera", "seller", "discovered", "legacy"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_source: Option<String>,
    /// Machine-readable project status: "ready_to_move", "under_construction", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status: Option<String>,
    /// Human-readable project status from skill's display_template, e.g. "Ready to Move — delivered 31/01/2020"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status_display: Option<String>,
    /// Compact buyer-facing state signal for result tiles, e.g. "Delivered · 5-10 yrs old".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_state_display: Option<String>,
    /// Human-readable builder delivery track record, e.g. "Builder delivers on time: 100% of projects"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_delivery_display: Option<String>,
    /// Data freshness — how recent and rich the underlying data is
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_freshness: Option<DataFreshness>,
    /// Representative floor-plan preview for this listing's BHK (compare-ready).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor_plan_preview_url: Option<String>,
    /// Plan carpet area (sqft) for the matched configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_carpet_area_sqft: Option<u32>,
    /// Plan sale / super built-up area (sqft) for usable-space compare.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_sale_area_sqft: Option<u32>,
    /// Matched configuration label, e.g. "3BHK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_configuration_type: Option<String>,
    /// Config-derived decision labels for compare, notes, and compact review surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_labels: Vec<DecisionLabel>,
    /// Grouped compact check summary for property details, compare, and notes.
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Serialize)]
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
