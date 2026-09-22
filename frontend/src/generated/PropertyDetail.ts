/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type Availability = "available" | "unavailable";
export type ContextGeometry =
  | {
      /**
       * @minItems 2
       * @maxItems 2
       */
      coordinates: [number, number];
      type: "Point";
    }
  | {
      coordinates: [number, number][];
      type: "LineString";
    }
  | {
      coordinates: [number, number][][];
      type: "Polygon";
    }
  | {
      coordinates: [number, number][][][];
      type: "MultiPolygon";
    };
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
/**
 * What kind of value a fact holds.
 */
export type FactValue =
  | {
      data: number;
      type: "Numeric";
    }
  | {
      data: string;
      type: "Text";
    }
  | {
      data: boolean;
      type: "Bool";
    }
  | {
      data: string[];
      type: "Tags";
    }
  | {
      data: {
        explanation: string;
        value: number;
      };
      type: "Score";
    };
/**
 * Promoted catalog records support identity claims without fabricated observations.
 */
export type CatalogEvidence =
  | {
      entityId: string;
      entityType: string;
      kind: "entity";
      name: string;
    }
  | {
      confidence: number;
      fromEntityId: string;
      kind: "relationship";
      relation: string;
      relationshipId: string;
      sourceType: string;
      toEntityId: string;
    };
export type ConstraintOperator = "min" | "max";
export type ProofResolutionStatus = "resolved";
export type ReviewTone = "positive" | "concern" | "neutral";
export type BranchLens = "proof" | "value" | "trust" | "commute";
export type RecommendationStatus = "pending" | "ready" | "unavailable";
export type ReraEvidenceAvailability = "available" | "partial" | "unavailable";

export interface PropertyDetail {
  /**
   * Highest price_per_sqft among properties in the same area.
   */
  area_price_range_high: number | null;
  /**
   * Lowest price_per_sqft among properties in the same area.
   */
  area_price_range_low: number | null;
  availability: InventoryAvailability;
  /**
   * Other locally tracked projects tied to the same normalized legal promoter name.
   */
  builder_portfolio?: BuilderPortfolio | null;
  context: PropertyContext;
  contract_version: number;
  /**
   * Grouped project-check read model for the buyer-facing detail page.
   */
  decision_check_summary?: DecisionCheckSummary | null;
  /**
   * Config-derived labels intended for notes and compare surfaces.
   */
  decision_labels?: DecisionLabel[];
  /**
   * Buyer-facing positive themes from external reviews and resident feedback.
   */
  detail_signals?: DetailSignal[];
  /**
   * Stable graph IDs the UI can dereference to render dynamic KG-backed sections.
   */
  entity_refs: KgEntityRefs;
  /**
   * Canonical UI read model for dynamic proof-backed cards on the property page.
   *
   * New UI should render optional property-page sections from this field
   * instead of hardcoding cards or calling legacy KG endpoints directly.
   * `source_panels` below is retained as a compatibility field for the
   * current frontend while it migrates.
   */
  evidence: PropertyEvidenceResponse;
  /**
   * Current external review evidence projected from the Parquet serving bundle.
   */
  external_reviews?: ExternalReviews | null;
  /**
   * Compact buyer-facing state signal for first-scan UI.
   */
  home_state_display?: string | null;
  /**
   * Number of buyers who have expressed interest.
   */
  interest_count: number;
  /**
   * Receipt-backed livability diligence brief composed from DAG facts and mined themes.
   */
  livability_brief?: LivabilityBrief | null;
  /**
   * Buyer-facing site overview + floor plans (RERA brochure promotions).
   */
  plans?: ProjectPlansView | null;
  /**
   * Machine-readable project status: "ready_to_move", "under_construction", etc.
   */
  project_status?: string | null;
  /**
   * Human-readable project status from skill's display_template
   */
  project_status_display?: string | null;
  property: PropertyAttributes;
  /**
   * Counterfactual branches — why you might consider an alternative instead.
   */
  recommendation_branches?: RecommendationBranch[];
  /**
   * Async recommendation status. The detail page should fetch the branch cards
   * from `/api/properties/{id}/recommendations` instead of doing this work inline.
   */
  recommendations: RecommendationEnvelope;
  /**
   * RERA regulatory data from the knowledge graph (None if not yet enriched).
   */
  rera?: ReraInfo | null;
  /**
   * Compact link to the dedicated RERA evidence report.
   */
  rera_report_ref: ReraReportRef;
  /**
   * Where the society data originally came from: "rera", "seller", "discovered", "legacy"
   */
  root_source?: string | null;
  snapshot_identity: string;
  society: SocietySummary | null;
}
export interface InventoryAvailability {
  area: Availability;
  bedrooms: Availability;
  price: Availability;
}
export interface BuilderPortfolio {
  builder_name: string;
  complaint_projects: number;
  delayed_projects: number;
  projects: BuilderProjectRecord[];
  rera_registered_projects: number;
  revocations?: number;
  tracked_projects: number;
}
export interface BuilderProjectRecord {
  area: string;
  complaints_count?: number;
  completion_date?: string;
  current: boolean;
  delay_months?: number;
  project_name: string;
  project_status_display?: string;
  property_id: string;
  rera_number?: string;
  rera_portal_url?: string;
  rera_registered: boolean;
  rera_status?: string;
  start_date?: string;
}
export interface PropertyContext {
  anchor: ContextEntity;
  contractVersion: number;
  entityRefs: KgEntityRefs;
  features: ContextFeature[];
  matchedProof: ProofResolution | null;
  propertyId: string;
  snapshotIdentity: string;
  truncated: boolean;
}
export interface ContextEntity {
  entityId: string;
  geometry: ContextGeometry | null;
  geometryEvidence: EvidenceRef[];
  geometrySource: ContextFact | null;
  name: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  point: [number, number] | null;
}
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
}
export interface ContextFact {
  confidence: number;
  entityId: string;
  evidence: EvidenceRef;
  factKey: string;
  id: string;
  observedAt: string;
  sourceType: string;
  sourceUrl: string | null;
  value: FactValue;
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
export interface ContextFeature {
  attributes: ContextFact[];
  distance: DerivedEvidence | null;
  fact: ContextFact;
  target: ContextEntity | null;
}
export interface DerivedEvidence {
  algorithm_version: string;
  confidence: number;
  derivation_id: string;
  input_evidence: EvidenceRef[];
  metric: string;
  relation: string;
  snapshot_identity: string;
  subject_entity_id: string;
  target_entity_id: string | null;
  unit: string | null;
  value: number | null;
}
export interface ProofResolution {
  branchId: string;
  catalogEvidence?: CatalogEvidence[];
  claim?: EvaluatedClaim;
  constraint?: HardConstraint;
  contractVersion: number;
  derivationChain: DerivedEvidence[];
  factKey: string;
  geometry?: unknown;
  predicateId: string;
  propertyId: string;
  relation: string;
  resolutionStatus: ProofResolutionStatus;
  semanticFingerprint: string;
  snapshotIdentity: string;
  sourceObservations: ResolvedSourceObservation[];
  subjectEntityId: string;
  targetEntityId?: string;
  targetLabel?: string;
  unit?: string;
  value?: FactValue;
}
export interface EvaluatedClaim {
  dimension: string;
  unit: string;
  value: number;
}
export interface HardConstraint {
  /**
   * Registry dimension, e.g. "land_area".
   */
  field: string;
  operator: ConstraintOperator;
  raw_text: string;
  unit: string;
  value: number;
}
export interface ResolvedSourceObservation {
  assetLineage: string[];
  observationId: string;
  observedAt: string;
  provider: string;
  providerObservationId: string;
  sourceUrl?: string;
  subjectEntityId: string;
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
export interface DetailSignal {
  count?: number;
  icon: string;
  key: string;
  label: string;
}
export interface PropertyEvidenceResponse {
  entity_refs: KgEntityRefs;
  property_id: string;
  sections: EvidenceSection[];
  serving_bundle_version?: string;
}
export interface EvidenceSection {
  community_pulse?: CommunityPulse;
  confidence_pct: number;
  constellation: string;
  entity_ids: string[];
  header_meta: string;
  items: SourceItem[];
  kind: string;
  media?: EvidenceMediaStrip[];
  missing: string[];
  presentation: EvidencePresentation;
  priority: number;
  relationship?: string;
  scope: string;
  source_types: string[];
  subtitle: string;
  summary: string;
  title: string;
}
export interface CommunityPulse {
  concerns: string[];
  paragraph: string;
  positives: string[];
  quotes: CommunityPulseQuote[];
  sentiment_band: string;
  source_label: string;
  source_urls: string[];
}
export interface CommunityPulseQuote {
  polarity: string;
  source_type: string;
  source_url?: string;
  text: string;
}
export interface SourceItem {
  attributions?: SourceAttribution[];
  entity_id: string;
  evidence: EvidenceRef[];
  key: string;
  label: string;
  learned_at: string;
  relationship?: string;
  scope: string;
  source_type: string;
  source_url?: string;
  value: string;
  values?: string[];
}
export interface SourceAttribution {
  evidence: EvidenceRef;
  learned_at: string;
  source_type: string;
  source_url?: string;
  value: string;
}
export interface EvidenceMediaStrip {
  caption: string;
  capture_date_label: string;
  coverage_quality: string;
  frames: EvidenceMediaFrame[];
  kind: string;
  provider: string;
  title: string;
}
export interface EvidenceMediaFrame {
  capture_date: string;
  distance_from_gate_m: number;
  fov: number;
  heading: number;
  image_url: string;
  label: string;
  pitch: number;
  source_url: string;
}
export interface EvidencePresentation {
  density: string;
  max_preview_items: number;
  variant: string;
}
export interface ExternalReviews {
  google_rating?: number;
  google_review_count?: number;
  google_reviews_url?: string;
  reviews?: ExternalReviewCard[];
}
export interface ExternalReviewCard {
  author?: string;
  date_label?: string;
  helpful_count?: number;
  id: string;
  rating?: number;
  source: string;
  text: string;
  tone: ReviewTone;
}
export interface LivabilityBrief {
  blocks?: LivabilityBriefBlock[];
  lifecycle_flag?: string;
  source_urls?: string[];
  summary_paragraph?: string;
}
export interface LivabilityBriefBlock {
  fact_keys?: string[];
  lens: string;
  paragraph: string;
  themes: string[];
  title: string;
}
export interface ProjectPlansView {
  coverage_quality: string;
  filed_plan_previews?: FiledPlanPreview[];
  floor_plans?: FloorPlanVariant[];
  provider: string;
  registration_number?: string;
  site_overview?: SiteOverviewPlan;
  source_url?: string;
}
export interface FiledPlanPreview {
  artifact_id: string;
  confidence: number;
  kind: string;
  label: string;
  page?: number;
  preview_url: string;
  source_url?: string;
  thumbnail_url?: string;
}
export interface FloorPlanVariant {
  artifact_id: string;
  bedroom_count: number;
  carpet_area_sqft?: number;
  carpet_area_sqm?: number;
  confidence: number;
  configuration_type: string;
  id: string;
  page?: number;
  preview_url: string;
  sale_area_sqft?: number;
  sale_area_sqm?: number;
  source_url?: string;
  tab_label: string;
  thumbnail_url?: string;
  title: string;
  unit_type_label?: string;
  usable_area_ratio?: number;
}
export interface SiteOverviewPlan {
  artifact_id: string;
  confidence: number;
  label: string;
  page?: number;
  preview_url: string;
  source_url?: string;
  thumbnail_url?: string;
}
export interface PropertyAttributes {
  area: string;
  area_id: string;
  area_measurement?: Measurement;
  bhk?: number;
  builder_name: string;
  carpet_area_sqft?: number;
  city: string;
  description_summary: string;
  hero_image: string;
  id: string;
  images: string[];
  listing_type: string;
  possession_status: string;
  price?: number;
  price_max?: number;
  price_min?: number;
  price_per_sqft?: number;
  property_type: string;
  society_id: string;
  super_builtup_sqft?: number;
  title: string;
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
export interface RecommendationBranch {
  branch_id: string;
  channels?: RecallChannelHit[];
  contrast: string;
  evidence_delta: EvidenceDelta;
  headline: string;
  lens: BranchLens;
  /**
   * Normalized 0..1 strength of this branch on its lens — how far it departs
   * from the current property. Drives spatial distance in the decision compass.
   */
  magnitude: number;
  property: PropertyCard;
  tradeoff?: string;
}
export interface RecallChannelHit {
  channel: string;
  score: number;
}
export interface EvidenceDelta {
  confidence_pct: number;
  fact_count: number;
  fact_delta: number;
  gap_count: number;
  gap_delta: number;
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
  carpet_area_sqft?: number;
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
export interface RecommendationEnvelope {
  cache_key: string;
  engine_version: string;
  scoring_policy_version: number;
  serving_bundle_version?: string;
  status: RecommendationStatus;
}
export interface ReraInfo {
  affidavit_only_visible: boolean | null;
  builder_revocations: number | null;
  builder_states: string[];
  builder_total_projects: number | null;
  complaint_summaries: ReraComplaintScopeSummary[];
  complaints_count: number | null;
  complaints_resolved_pct: number | null;
  completion_date: string | null;
  construction_cost_inr: number | null;
  cost_per_unit_inr: number | null;
  decision_cards: ReraDecisionCard[];
  delay_months: number | null;
  document_groups: ReraDocumentGroupSummary[];
  document_manifest: ReraDocumentManifestItem[];
  escrow_bank: string | null;
  has_borrowing: boolean | null;
  has_mortgage: boolean | null;
  land_cost_inr: number | null;
  land_litigation: boolean | null;
  last_verified: string | null;
  open_area_pct: number | null;
  original_completion_date: string | null;
  project_complaints_count: number | null;
  project_complaints_disposed_count: number | null;
  project_complaints_open_count: number | null;
  promoter_complaints_count: number | null;
  promoter_complaints_disposed_count: number | null;
  promoter_complaints_open_count: number | null;
  registered: boolean;
  registration_number: string | null;
  rera_portal_url: string | null;
  schedule_sections: ReraScheduleSection[];
  start_date: string | null;
  status: string | null;
  total_land_area_acres: number | null;
  total_land_area_sqm: number | null;
  total_project_cost_inr: number | null;
  total_units: number | null;
}
export interface ReraComplaintScopeSummary {
  confidence: number;
  disposed_count: number;
  open_count: number;
  row_count_parsed: number;
  sample_subjects: string[];
  scope: string;
  theme_counts: {
    [k: string]: number;
  };
  total_count_from_tab_label: number | null;
  validation_notes: string[];
}
export interface ReraDecisionCard {
  actions: ReraDecisionAction[];
  confidence: number;
  detail: string;
  facts: unknown;
  id: string;
  labels: string[];
  source: string;
  title: string;
  tone: string;
  validation_notes: string[];
}
export interface ReraDecisionAction {
  kind: string;
  label: string;
}
export interface ReraDocumentGroupSummary {
  count: number;
  group: string;
}
export interface ReraDocumentManifestItem {
  artifact_id: string;
  bedroom_count: number | null;
  buyer_visibility: string | null;
  confidence: number | null;
  configuration_type: string | null;
  document_group: string;
  kind: string;
  label: string;
  preview_policy: string | null;
  source_field_label: string | null;
  source_tab: string | null;
  source_url: string | null;
}
export interface ReraScheduleSection {
  group: string;
  label: string;
  rows: ReraScheduleRow[];
}
export interface ReraScheduleRow {
  area_sqm: number | null;
  available: boolean | null;
  confidence: number | null;
  label: string;
  value: string | null;
}
export interface ReraReportRef {
  availability: ReraEvidenceAvailability;
  href: string;
  registration_ids: string[];
}
export interface SocietySummary {
  area: string;
  builder_name: string;
  city: string;
  id: string;
  name: string;
}
