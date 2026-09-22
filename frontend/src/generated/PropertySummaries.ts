/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type EvidenceId =
  | {
      id: string;
      kind: "observation";
    }
  | {
      id: string;
      kind: "derivation";
    }
  | {
      id: string;
      kind: "entity";
    }
  | {
      id: string;
      kind: "relationship";
    };
export type Availability = "available" | "unavailable";

export interface PropertySummaries {
  contractVersion: number;
  items: PropertyCard[];
  missingIds: string[];
  snapshotIdentity: string;
}
/**
 * UI-ready property card for the results page.
 */
export interface PropertyCard {
  area: string;
  area_measurement?: Measurement;
  availability: InventoryAvailability;
  bhk?: number;
  /**
   * Human-readable builder delivery track record, e.g. "Builder delivers on time: 100% of projects"
   */
  builder_delivery_display?: string;
  builder_name: string;
  /**
   * Data freshness — how recent and rich the underlying data is
   */
  data_freshness?: DataFreshness;
  /**
   * Grouped compact check summary for property details, compare, and notes.
   */
  decision_check_summary?: DecisionCheckSummary;
  /**
   * Config-derived decision labels for compare, notes, and compact review surfaces.
   */
  decision_labels?: DecisionLabel[];
  description_summary: string;
  facing: string;
  floor: number;
  /**
   * Representative floor-plan preview for this listing's BHK (compare-ready).
   */
  floor_plan_preview_url?: string;
  google_rating?: number;
  google_review_count?: number;
  google_reviews_url?: string;
  hero_image: string;
  /**
   * Compact buyer-facing state signal for result tiles, e.g. "Delivered · 5-10 yrs old".
   */
  home_state_display?: string;
  id: string;
  images?: string[];
  /**
   * Stable entity handles attached to the serving bundle.
   *
   * This is the contract that keeps cards and detail pages from becoming a
   * fixed list of hardcoded sections. The flat fields in `PropertyCard`
   * support fast first paint and search-result scanning. `kg_entity_refs`
   * supports the second layer: expandable evidence, compare rows, side
   * panels, source drill-down, and dynamic sections that only appear when
   * facts actually exist.
   *
   * Backend rules:
   * - Populate these IDs from app-owned entity identity, never from UI labels.
   * - Add new fact families to serving/source panels instead of adding
   *   one-off card fields unless the value is needed on the hot first-paint path.
   * - It is okay for some referenced concepts to have sparse facts. The UI
   *   should render from fact availability and confidence.
   */
  kg_entity_refs: KgEntityRefs;
  metro_distance_mins: number;
  /**
   * RERA-backed open-area percentage. Omitted when the source did not expose it clearly.
   */
  open_space_pct?: number;
  /**
   * Plan carpet area (sqft) for the matched configuration.
   */
  plan_carpet_area_sqft?: number;
  /**
   * Matched configuration label, e.g. "3BHK".
   */
  plan_configuration_type?: string;
  /**
   * Plan sale / super built-up area (sqft) for usable-space compare.
   */
  plan_sale_area_sqft?: number;
  possession_status: string;
  price?: number;
  /**
   * Inclusive listing band when the source is a range, not a point asking price.
   */
  price_max?: number;
  /**
   * Inclusive listing band when the source is a range, not a point asking price.
   */
  price_min?: number;
  price_per_sqft?: number;
  /**
   * Machine-readable project status: "ready_to_move", "under_construction", etc.
   */
  project_status?: string;
  /**
   * Human-readable project status from skill's display_template, e.g. "Ready to Move — delivered 31/01/2020"
   */
  project_status_display?: string;
  /**
   * Where the society data originally came from: "rera", "seller", "discovered", "legacy"
   */
  root_source?: string;
  /**
   * RERA-backed project land extent. Kept on the card because compare needs it at first paint.
   */
  society_land_acres?: number;
  society_name: string;
  sqft?: number;
  super_builtup_sqft?: number;
  title: string;
  total_floors: number;
}
/**
 * A measurement is never a display number: its basis, scope and receipt travel together.
 */
export interface Measurement {
  basis: string;
  entityId: string;
  evidence: EvidenceRef;
  maximum?: number;
  minimum?: number;
  unit: string;
  value: number;
}
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
}
export interface InventoryAvailability {
  area: Availability;
  bedrooms: Availability;
  price: Availability;
}
/**
 * Legacy optional API shape. Search and detail responses do not calculate
 * freshness or age from timestamps.
 */
export interface DataFreshness {
  /**
   * How many days ago the node was last updated
   */
  days_ago: number;
  /**
   * Total number of facts on the node
   */
  fact_count: number;
  /**
   * Human-readable label: "Fresh", "Recent", "Stale", "Very stale"
   */
  freshness_label: string;
  /**
   * ISO timestamp of last enrichment
   */
  last_enriched: string;
  /**
   * Breakdown of facts by source type, e.g. {"Rera": 5, "Reddit": 3}
   */
  source_breakdown: {
    [k: string]: number;
  };
}
export interface DecisionCheckSummary {
  groups?: DecisionLabelGroup[];
  primaryCount: number;
  primaryLabels?: DecisionLabel[];
  registrationNumber?: string;
  registrationNumberCompact?: string;
  registryUrl?: string;
  tileCaption?: string;
  tileLabel: string;
  tone: string;
  totalCount: number;
}
export interface DecisionLabelGroup {
  id: string;
  labels: DecisionLabel[];
  title: string;
}
export interface DecisionLabel {
  compareGroup?: string;
  confidence: number;
  groupId: string;
  key: string;
  label: string;
  notebookLabels?: string[];
  placement: string;
  priority: number;
  scope: string;
  severity: string;
  sourceFactKeys?: string[];
  surfaces?: string[];
  unit?: string;
  value?: number;
  valueText?: string;
  visualId: string;
}
/**
 * Minimal entity identity bundle attached to property/search/detail responses.
 *
 * These fields are stable API identifiers, not display copy and not a complete
 * serving export. They exist so the UI can ask follow-up endpoints for richer
 * context when a user shows intent: opens a property, expands a card, compares
 * homes, clicks a source trail, or requests a nearby/risk/community breakdown.
 *
 * Current usage pattern:
 * 1. Render fast listing data from `PropertyCard` or `PropertyDetailResponse`.
 * 2. Use `source_entity_ids` as opaque provenance handles for the property,
 *    society, area, and builder.
 * 3. Use source/evidence read models when the UI needs a larger drill-down
 *    such as builder portfolio, nearby projects, or lineage.
 * 4. Build optional UI sections from facts with source/confidence metadata.
 * 5. Hide sections that have no backed facts instead of rendering empty cards.
 *
 * Important distinction: these are KG node IDs, not necessarily canonical RERA
 * IDs. Some societies have an alias node such as `society:prestige-park-grove`
 * while lake artifacts may also contain a RERA-rooted canonical ID. The UI
 * should not infer canonicalization from the string shape. It should treat the
 * IDs as opaque handles and follow the API.
 */
export interface KgEntityRefs {
  /**
   * Area/locality node for traffic, waterlogging, metro, schools, price trend,
   * and other externalities.
   */
  area_entity_id: string;
  /**
   * Builder node when the society has a known BuiltBy edge in the KG.
   */
  builder_entity_id?: string;
  /**
   * Listing-level node for facts specific to this flat/unit/listing.
   */
  property_entity_id: string;
  /**
   * Society/project node for RERA, reviews, nearby places, amenities, and
   * community evidence.
   */
  society_entity_id: string;
  /**
   * Existing graph nodes the UI can safely prefetch first.
   *
   * This list is backend-filtered to nodes present in the current KG, sorted,
   * and deduplicated. It may omit an otherwise valid field ID if that node has
   * not been materialized yet. UI code should treat it as a convenient fetch
   * plan, not as a complete semantic model.
   */
  source_entity_ids?: string[];
}
