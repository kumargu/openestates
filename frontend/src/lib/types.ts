export type PropertyCard = {
  id: string;
  /**
   * Stable serving-bundle handles for this card.
   *
   * Treat these as the bridge from a compact search/listing response into the
   * richer property evidence responses. A result card should render its normal
   * fast fields first, then use these IDs when the UI needs drill-down evidence,
   * expandable cards, compare rows, or hover/side-panel context.
   *
   * Typical UI flow:
   * 1. Render the card from local fields such as price, BHK, area, match_reason.
   * 2. On expand, hover, compare, or detail prefetch, fetch the property detail
   *    or evidence endpoints for this card.
   * 3. Build optional UI sections only from returned facts/source panels.
   * 4. Hide sections with no confident facts instead of showing empty static
   *    placeholders.
   */
  kg_entity_refs: KgEntityRefs;
  title: string;
  area: string;
  price: number;
  price_min?: number;
  price_max?: number;
  price_per_sqft: number;
  bhk: number;
  sqft: number;
  carpet_area_sqft?: number;
  super_builtup_sqft?: number;
  society_name: string;
  builder_name: string;
  images?: string[];
  hero_image: string | null;
  transparency_tags: string[];
  description_summary: string;
  possession_status: string;
  metro_distance_mins: number;
  floor: number;
  total_floors: number;
  facing: string;
  google_rating?: number;
  google_review_count?: number;
  google_reviews_url?: string;
  /** RERA-backed project land extent; compare UI hides this dimension when absent. */
  society_land_acres?: number;
  /** Source-backed open or green-space share; compare UI hides this dimension when absent. */
  open_space_pct?: number;
  /** Delivery-record category supplied by a future DAG-backed buyer-surface view. */
  builder_category?: "A" | "B" | "C";
  root_source?: string;
  project_status?: string;
  /** Representative floor-plan preview for this listing BHK. */
  floor_plan_preview_url?: string;
  /** Plan carpet area for the matched configuration. */
  plan_carpet_area_sqft?: number;
  /** Plan sale area for usable-space compare. */
  plan_sale_area_sqft?: number;
  /** Matched configuration label, e.g. 3BHK. */
  plan_configuration_type?: string;
  project_status_display?: string;
  home_state_display?: string;
  builder_delivery_display?: string;
  data_freshness?: DataFreshness;
  /** Config-derived decision labels for compare, notes, and compact review surfaces. */
  decision_labels?: DecisionLabel[];
  /** Grouped compact checks for property details, notes, and compare. */
  decision_check_summary?: DecisionCheckSummary;
};

export type DecisionLabel = {
  key: string;
  label: string;
  severity: "info" | "positive" | "caution" | "risk" | string;
  scope: "project" | "builder" | "area" | string;
  visualId: string;
  value?: number;
  valueText?: string;
  unit?: string;
  surfaces?: string[];
  priority: number;
  sourceFactKeys?: string[];
  confidence: number;
  notebookLabels?: string[];
  compareGroup?: string;
  groupId: "attention" | "project_facts" | "documents" | "finance" | string;
  placement: "primary" | "more" | "audit" | string;
};

export type DecisionCheckGroup = {
  id: "attention" | "project_facts" | "documents" | "finance" | string;
  title: string;
  labels: DecisionLabel[];
};

export type DecisionCheckSummary = {
  tileLabel: string;
  tileCaption?: string;
  tone: "risk" | "caution" | "neutral" | "positive" | string;
  registrationNumber?: string;
  registrationNumberCompact?: string;
  registryUrl?: string;
  primaryCount: number;
  totalCount: number;
  primaryLabels?: DecisionLabel[];
  groups?: DecisionCheckGroup[];
};

export type DiscoveryQuote = {
  text: string;
  tone: "proof" | "intent" | "trust" | string;
};

export type DiscoveryShelfCard = {
  property: PropertyCard;
  reason: string;
};

export type DiscoveryShelf = {
  id: string;
  title: string;
  quote: string;
  description: string;
  search_query: string;
  receipt_copy: string;
  cards: DiscoveryShelfCard[];
};

export type DiscoveryResponse = {
  product_promise: string;
  quotes: DiscoveryQuote[];
  shelves: DiscoveryShelf[];
};

export type RecommendationLens = "proof" | "value" | "trust" | "commute";
export type RecommendationStatus = "pending" | "ready" | "unavailable";

export type RecommendationEnvelope = {
  status: RecommendationStatus;
  cache_key: string;
  engine_version: string;
  scoring_policy_version: number;
  serving_bundle_version?: string;
};

export type RecallChannelHit = {
  channel: string;
  score: number;
};

export type RecommendationBranch = {
  branch_id: string;
  lens: RecommendationLens;
  headline: string;
  property: PropertyCard;
  contrast: string;
  tradeoff?: string;
  evidence_delta: {
    fact_count: number;
    gap_count: number;
    confidence_pct?: number;
    fact_delta: number;
    gap_delta: number;
  };
  channels?: RecallChannelHit[];
  magnitude: number;
};

export type RecommendationResponse = {
  status: RecommendationStatus;
  engine_version: string;
  scoring_policy_version: number;
  serving_bundle_version?: string;
  items: RecommendationBranch[];
};

export type PropertyDetailResponse = {
  property: {
    id: string;
    title: string;
    area: string;
    area_id: string;
    city: string;
    society_id: string;
    builder_name: string;
    property_type: string;
    listing_type: string;
    bhk: number;
    price: number;
    price_min?: number;
    price_max?: number;
    price_per_sqft: number;
    carpet_area_sqft: number;
    super_builtup_sqft: number;
    floor: number;
    total_floors: number;
    facing: string;
    possession_status: string;
    metro_distance_mins: number;
    maintenance_cost_monthly: number;
    society_quality_score: number | null;
    builder_quality_score: number | null;
    document_completeness_score: number | null;
    litigation_risk: number | null;
    noise_score: number | null;
    sunlight_score: number | null;
    airport_noise_score: number | null;
    waterlogging_risk_score: number | null;
    traffic_score: number | null;
    days_on_market: number;
    greenery_score?: number;
    open_space_score?: number;
    resale_strength_score?: number;
    interest_level?: "high" | "moderate" | "low";
    saves_last_7d?: number;
    offers_last_7d?: number;
    images: string[];
    hero_image: string;
    description_summary: string;
    transparency_tags: string[];
    source_reference: string;
  };
  /**
   * The same graph handle set as `property.kg_entity_refs` on result cards.
   *
   * Detail pages should prefer this top-level field because the nested
   * `property` object is the flat listing record, while `entity_refs` is the
   * explicit graph contract. Use it to fetch nodes, neighbors, and subgraphs
   * without guessing IDs from display names.
   */
  entity_refs: KgEntityRefs;
  society: {
    id: string;
    name: string;
    area: string;
    city: string;
    builder_name: string;
    year_built: number;
    total_units: number;
    summary: string;
    maintenance_sentiment: string;
    livability_sentiment: string;
    common_positives: string[];
    common_complaints: string[];
    review_summary: string;
    google_reviews_url?: string;
  } | null;
  area: {
    id: string;
    name: string;
    city: string;
    median_price_per_sqft: number;
    trend_direction: string;
    trend_summary: string;
    metro_access_summary: string;
    traffic_summary: string;
    waterlogging_summary: string;
    livability_summary: string;
    externality_tags: string[];
    infrastructure_tags: string[];
    community_notes: string;
  } | null;
  similar_properties: PropertyCard[];
  recommendation_branches?: RecommendationBranch[];
  recommendations?: RecommendationEnvelope;
  rera?: ReraInfo;
  rera_report_ref: ReraReportRef;
  area_intelligence?: AreaIntelligence;
  transparency_score?: TransparencyScore;
  area_price_range_low?: number;
  area_price_range_high?: number;
  interest_count?: number;
  root_source?: string;
  project_status_display?: string;
  project_status?: string;
  home_state_display?: string;
  builder_trust?: {
    delivery_rate?: number;
    project_count?: number;
    delivery_display?: string;
  };
  builder_portfolio?: BuilderPortfolio;
  data_freshness?: DataFreshness;
  confidence_score?: ConfidenceScore;
  evidence?: PropertyEvidenceResponse;
  external_reviews?: {
    google_rating?: number;
    google_review_count?: number;
    google_reviews_url?: string;
    reviews?: ExternalReviewCard[];
  };
  detail_signals?: DetailSignal[];
  /** Config-derived labels intended for notes and compare surfaces. */
  decision_labels?: DecisionLabel[];
  /** Grouped compact checks for property-detail decision labels. */
  decision_check_summary?: DecisionCheckSummary;
  livability_brief?: LivabilityBrief;
  /** Schematic neighborhood plate projected from nearby + water facts. */
  map_context?: PropertyMapContext;
  /** Site overview + floor plans promoted from RERA brochure pages. */
  plans?: ProjectPlansView;
};

export type ExternalReviewCard = {
  id: string;
  source: string;
  author?: string;
  rating?: number;
  date_label?: string;
  helpful_count?: number;
  text: string;
  tone: "positive" | "concern" | "neutral";
};

export type DetailSignal = {
  key: string;
  label: string;
  icon: string;
  count?: number;
};

export type SiteOverviewPlan = {
  artifact_id: string;
  label: string;
  preview_url: string;
  thumbnail_url?: string;
  source_url?: string;
  page?: number;
  confidence: number;
};

export type FloorPlanVariant = {
  id: string;
  artifact_id: string;
  configuration_type: string;
  unit_type_label?: string;
  bedroom_count: number;
  tab_label: string;
  title: string;
  preview_url: string;
  thumbnail_url?: string;
  source_url?: string;
  page?: number;
  carpet_area_sqft?: number;
  carpet_area_sqm?: number;
  sale_area_sqft?: number;
  sale_area_sqm?: number;
  usable_area_ratio?: number;
  confidence: number;
};

export type ProjectPlansView = {
  provider: string;
  coverage_quality: string;
  source_url?: string;
  registration_number?: string;
  site_overview?: SiteOverviewPlan;
  floor_plans: FloorPlanVariant[];
  filed_plan_previews?: FiledPlanPreview[];
};

export type FiledPlanPreview = {
  artifact_id: string;
  kind: string;
  label: string;
  preview_url: string;
  thumbnail_url?: string;
  source_url?: string;
  page?: number;
  confidence: number;
};

export type MapNearbyLayer =
  | "metro"
  | "schools"
  | "hospitals"
  | "tech";

export type MapHomeAnchor = {
  entity_id: string;
  name: string;
  area?: string;
  latitude?: number;
  longitude?: number;
  boundary?: MapOverlayPolygon;
};

export type MapPlacePin = {
  feature_id?: string;
  place_entity_id?: string;
  layer: MapNearbyLayer | string;
  icon?: string;
  name: string;
  latitude?: number;
  longitude?: number;
  distance_km?: number;
  rating?: number;
  review_count?: number;
  note?: string;
  lines?: string[];
  source_url?: string;
  source_type: string;
  properties?: Record<string, string>;
};

export type MapWaterContext = {
  groundwater_class: string;
  summary: string;
  scope_radius_km?: number;
  source_type: string;
  source_url?: string;
  illustrative_zone: boolean;
};

export type MapOverlayLine = {
  id: string;
  name: string;
  label?: string;
  distance_km?: number;
  details?: string[];
  kind: string;
  coordinates: [number, number][];
  source_type: string;
  source_url?: string;
  properties?: Record<string, string>;
};

export type MapOverlayPolygon = {
  id: string;
  name: string;
  kind: string;
  coordinates: [number, number][];
  distance_km?: number;
  source_type: string;
};

export type MapComparisonHome = {
  id: string;
  name: string;
  latitude: number;
  longitude: number;
  href: string;
  boundary?: MapOverlayPolygon;
};

export type ArrivalSearchSociety = {
  href: string;
  propertyId: string;
  societyId: string;
  proofFocus?: ProofFocus;
  preview: {
    area: string;
    bhk: number;
    price: number;
    title: string;
  };
  home: {
    latitude: number;
    longitude: number;
    name: string;
    boundary?: MapOverlayPolygon;
  };
};

export type PropertyMapContext = {
  home: MapHomeAnchor;
  layers?: MapLayerMeta[];
  places: MapPlacePin[];
  proof_focus?: ProofFocus;
  arrivalExperience?: ArrivalSceneExperience;
  water?: MapWaterContext;
  metro_lines?: MapOverlayLine[];
  access_lines?: MapOverlayLine[];
  red_flag_lines?: MapOverlayLine[];
  layer_lines?: Record<string, MapOverlayLine[]>;
  green_patches?: MapOverlayPolygon[];
  lakes?: MapOverlayPolygon[];
};

export type MapLayerMeta = {
  id: string;
  label: string;
  renderKind?: string;
  mapPresentation?: MapPresentation;
  experience?: MapLayerExperience;
  emptyState?: string;
  featureValueLabels?: Record<string, Record<string, string>>;
  rank?: number;
  enabledByDefault?: boolean;
};

export type MapPresentation = "immersive_3d" | "readable_2d";

export type ArrivalSceneExperience = {
  revealDurationMs: number;
  startRangeM: number;
  finalRangeM: number;
  finalTilt: number;
  finalHeading: number;
  rotationArcDegrees: number;
  boundaryPadding: number;
  mobileBoundaryPadding: number;
  missingBoundaryState?: string;
  googleUnavailableState?: string;
  societyPlayLabel?: string;
  societyPauseLabel?: string;
  societyResumeLabel?: string;
  searchContextLabel?: string;
  searchContextViewHomeLabel?: string;
  backToSocietyLabel?: string;
};

export type MapLayerExperience = {
  kind: string;
  waypointSpacingM: number;
  overviewDwellMs?: number;
  dwellMs: number;
  anchorLookAheadM?: number;
  anchorPitch?: number;
  cameraAltitudeM: number;
  cameraRangeM: number;
  cameraTilt: number;
  cameraFov: number;
  streetViewZoom: number;
  transitionMs: number;
  targetDurationMs?: number;
  minimumDurationMs?: number;
  maximumDurationMs?: number;
  minimumFrameDwellMs?: number;
  entranceDwellMs?: number;
  maximumPanoramaGapM?: number;
  shortGapState?: string;
  endsHereState?: string;
  unavailableState?: string;
  pauseLabel?: string;
  resumeLabel?: string;
  replayLabel?: string;
  skipLabel?: string;
};

export type SurfaceSceneResponse = {
  contractVersion: 1;
  surfaceId: string;
  propertyId: string;
  servingBundleVersion?: string;
  entityRefs: KgEntityRefs;
  anchor: SceneAnchor;
  experience?: ArrivalSceneExperience;
  viewport: SceneViewport;
  proofFocus?: ProofFocus;
  layers: SceneLayer[];
  features: SceneFeature[];
  relations: SceneRelation[];
  callouts: SceneCallout[];
  receipts: SceneReceipt[];
  fillRate: SceneFillRate;
  gaps: SceneGap[];
};

export type ProofFocus = {
  surfaceId: string;
  layerId: string;
  factKey: string;
  destinationKind?: "scene" | "section" | string;
  targetId?: string;
  entityId?: string;
  featureId?: string;
  receiptId?: string;
  matchedLabel?: string;
  matchedValue?: string;
  requestedConstraint?: string;
  distanceM?: number;
  reason: string;
};

export type PropertySurfacesResponse = {
  contractVersion: 1;
  propertyId: string;
  scenes: SurfaceSceneResponse[];
  missing: SurfaceSceneMissing[];
};

export type SurfaceSceneMissing = {
  surfaceId: string;
  reason: string;
};

export type SurfaceBatchResponse = {
  contractVersion: 1;
  items: PropertySurfacesResponse[];
};

export type SceneAnchor = {
  entityId: string;
  label: string;
  area?: string;
  geometry?: SceneGeometry;
  boundary?: SceneBoundary;
  coordinateQuality: SceneCoordinateQuality;
};

export type SceneBoundary = {
  geometry: SceneGeometry;
  sourceType: string;
  sourceUrl?: string;
  confidence: number;
};

export type SceneViewport = {
  center?: [number, number];
  bounds?: SceneBounds;
  radiusM?: number;
};

export type SceneBounds = {
  west: number;
  south: number;
  east: number;
  north: number;
};

export type SceneLayer = {
  id: string;
  label: string;
  family: "access" | "risk" | "environment" | "market" | "context" | string;
  renderKind: "pin" | "line" | "polygon" | "corridor" | "evidence_list" | string;
  mapPresentation?: MapPresentation;
  experience?: MapLayerExperience;
  emptyState?: string;
  featureValueLabels?: Record<string, Record<string, string>>;
  relationClass: "access" | "risk_externality" | "context" | string;
  enabledByDefault: boolean;
  rank: number;
  availableCount: number;
  shownCount: number;
  fillState: SceneFillState;
};

export type SceneFeature = {
  id: string;
  entityId?: string;
  layerId: string;
  kind: string;
  label: string;
  shortLabel?: string;
  details?: string[];
  geometry: SceneGeometry;
  coordinateQuality: SceneCoordinateQuality;
  metrics?: SceneMetrics;
  display: SceneFeatureDisplay;
  properties?: Record<string, string>;
  confidence: number;
  receiptIds: string[];
};

export type SceneGeometry =
  | { type: "Point"; coordinates: [number, number] }
  | { type: "LineString"; coordinates: [number, number][] }
  | { type: "Polygon"; coordinates: [number, number][][] };

export type SceneCoordinateQuality = "exact" | "derived" | "approximate" | "missing";

export type SceneMetrics = {
  distanceM?: number;
  travelTimeMin?: number;
  rating?: number;
  reviewCount?: number;
  severity?: "low" | "medium" | "high" | string;
};

export type SceneFeatureDisplay = {
  tone: "positive" | "neutral" | "caution" | "risk";
  icon?: string;
  priority: number;
};

export type SceneRelation = {
  fromId: string;
  toId: string;
  edgeType: string;
  relationClass: "access" | "risk_externality" | "context" | string;
  direct: boolean;
  distanceM?: number;
  confidence: number;
  receiptIds: string[];
};

export type SceneCallout = {
  id: string;
  tone: "positive" | "neutral" | "caution" | "risk";
  label: string;
  featureIds: string[];
  receiptIds: string[];
};

export type SceneReceipt = {
  id: string;
  entityId: string;
  factKey: string;
  claim: string;
  sourceType: string;
  sourceUrl?: string;
  learnedAt: string;
  confidence: number;
  scope?: string;
};

export type SceneFillRate = {
  filledLayers: number;
  partialLayers: number;
  emptyLayers: number;
  shownFeatures: number;
  availableFeatures: number;
  value: number;
};

export type SceneGap = {
  layerId: string;
  fillState: SceneFillState;
};

export type SceneFillState = "filled" | "partial" | "empty";

/**
 * Knowledge Graph entity references exposed by property/search/detail APIs.
 *
 * This type is intentionally small and boring: it is a set of stable IDs, not
 * display content. The point is to let the UI stay dynamic without baking every
 * possible evidence dimension into React. New fact families can appear in the KG
 * and serving bundle, and the UI can discover/render them by dereferencing these
 * IDs instead of waiting for a new hardcoded property-card field.
 *
 * How to use it well:
 * - Use `property_entity_id` when the UI needs listing-specific facts such as
 *   BHK, carpet area, price, self-reported source, or per-listing market activity.
 * - Use `society_entity_id` for project/society evidence such as RERA facts,
 *   Google review facts, nearby places, amenities, complaints, community pulse,
 *   and builder relationships.
 * - Use `area_entity_id` for locality evidence such as traffic, waterlogging,
 *   metro access, school clusters, price trend, and externalities.
 * - Use `builder_entity_id` only when present. Some projects may not have a
 *   known builder node yet; the UI should simply omit builder cards in that
 *   case.
 * - Use `source_entity_ids` for prefetching a compact evidence bundle. It only
 *   contains IDs the backend knows are currently present in the KG, so it is a
 *   safer "fetch these first" list than rebuilding IDs in the browser.
 *
 * What not to do:
 * - Do not parse display names to create graph IDs. Slug rules and canonical
 *   IDs can evolve; use these fields as the contract.
 * - Do not assume every ID has the same richness. A returned node can have 2
 *   facts or 200 facts. Render from available facts and confidence.
 * - Do not make source panels static. If there is no school/metro/review/RERA
 *   evidence, omit that section or show a small explicit data gap.
 * - Do not treat these IDs as permanent public URLs. They are API identifiers
 *   for the current KG, suitable for app state, cache keys, and fetches.
 *
 * Example dynamic section loader:
 *
 * const ids = refs.source_entity_ids?.length
 *   ? refs.source_entity_ids
 *   : [refs.property_entity_id, refs.society_entity_id, refs.area_entity_id];
 *
 * const nodes = await Promise.all(ids.map((id) => getKnowledgeNode(id)));
 * const sections = buildSectionsFromFacts(nodes, {
 *   minConfidencePct: 60,
 *   preferredKinds: ["rera", "nearby", "reviews", "community", "risk"],
 * });
 *
 * In that model, React components own layout and interaction, while KG facts own
 * whether a card exists and what evidence backs it.
 */
export type KgEntityRefs = {
  /** Listing-level KG node, e.g. `property:discovered-prestige-lavender-fields-3bhk`. */
  property_entity_id: string;
  /** Society/project KG node, e.g. `society:prestige-lavender-fields`. */
  society_entity_id: string;
  /** Area/locality KG node, e.g. `area:varthur`. */
  area_entity_id: string;
  /** Builder KG node when known, e.g. `builder:prestige-group`. */
  builder_entity_id?: string;
  /**
   * Existing KG nodes worth prefetching for a dynamic card/detail/compare view.
   *
   * This is deduped and sorted by the backend. It may be smaller than the set of
   * individual fields above when a node ID is valid but not yet present in KG.
   */
  source_entity_ids?: string[];
};

export type SourcePanel = {
  kind?: string;
  title: string;
  subtitle: string;
  scope?: string;
  relationship?: string;
  items: SourceItem[];
  missing: string[];
  media?: EvidenceMediaStrip[];
};

export type SourceItem = {
  entity_id: string;
  key?: string;
  label: string;
  value: string;
  scope?: string;
  relationship?: string;
  values?: string[];
  source_type: string;
  source_url?: string;
  attributions?: SourceAttribution[];
  learned_at: string;
};

export type EvidenceMediaFrame = {
  label: string;
  distance_from_gate_m: number;
  image_url: string;
  heading: number;
  pitch: number;
  fov: number;
  capture_date: string;
  source_url: string;
};

export type EvidenceMediaStrip = {
  kind: string;
  provider: string;
  title: string;
  caption: string;
  capture_date_label: string;
  coverage_quality: "strong" | "usable" | string;
  frames: EvidenceMediaFrame[];
};

export type EvidenceConstellation =
  | "value"
  | "trust"
  | "lifestyle"
  | "risk"
  | "commute"
  | "investment";

export type SourceAttribution = {
  value: string;
  source_url?: string;
  source_type: string;
  learned_at: string;
};

export type EvidenceSection = {
  kind: string;
  title: string;
  summary: string;
  subtitle: string;
  scope?: string;
  relationship?: string;
  priority: number;
  constellation?: EvidenceConstellation;
  header_meta?: string;
  source_types: string[];
  entity_ids: string[];
  presentation?: EvidencePresentation;
  items: SourceItem[];
  missing: string[];
  media?: EvidenceMediaStrip[];
  community_pulse?: CommunityPulse;
};

export type LivabilityBriefBlock = {
  lens: string;
  title: string;
  paragraph: string;
  themes: string[];
  fact_keys?: string[];
};

export type LivabilityBrief = {
  summary_paragraph?: string;
  blocks?: LivabilityBriefBlock[];
  lifecycle_flag?: string;
  source_urls: string[];
};

export type CommunityPulseQuote = {
  text: string;
  source_type: string;
  source_url?: string;
  polarity: "positive" | "concern" | "neutral" | string;
};

export type CommunityPulse = {
  source_label: string;
  sentiment_band: string;
  paragraph: string;
  positives: string[];
  concerns: string[];
  quotes: CommunityPulseQuote[];
  source_urls: string[];
};

export type EvidencePresentation = {
  variant: "fact_list" | "fact_grid" | "risk_grid" | "timeline" | "media_grid" | "story" | string;
  density: "compact" | "standard" | string;
  max_preview_items: number;
};

export type PropertyEvidenceResponse = {
  property_id: string;
  entity_refs: KgEntityRefs;
  serving_bundle_version?: string;
  sections: EvidenceSection[];
};

export type PropertyEvidenceBatchResponse = {
  serving_bundle_version?: string;
  results: PropertyEvidenceResponse[];
  missing_property_ids: string[];
};

export type BuilderPortfolio = {
  builder_name: string;
  tracked_projects: number;
  rera_registered_projects: number;
  delayed_projects: number;
  complaint_projects: number;
  revocations?: number;
  projects: BuilderProjectRecord[];
};

export type BuilderProjectRecord = {
  property_id: string;
  project_name: string;
  area: string;
  rera_number?: string;
  rera_portal_url?: string;
  rera_status?: string;
  rera_registered: boolean;
  start_date?: string;
  completion_date?: string;
  delay_months?: number;
  complaints_count?: number;
  project_status_display?: string;
  current: boolean;
};

export type TransparencyComponent = {
  label: string;
  score: number;
  max_score: number;
};

export type TransparencyScore = {
  overall: number;
  components: TransparencyComponent[];
  explainer: string;
};

export type AreaListItem = {
  id: string;
  name: string;
  median_price_per_sqft: number;
  trend_direction: string;
  primary_signal: string;
  signals?: string[];
};

export type AreaDetail = {
  id: string;
  name: string;
  city: string;
  median_price_per_sqft: number;
  trend_direction: string;
  trend_summary: string;
  metro_access_summary: string;
  traffic_summary: string;
  waterlogging_summary: string;
  livability_summary: string;
  externality_tags: string[];
  infrastructure_tags: string[];
  community_notes: string;
};

export type AreaTrackerMarket = {
  id: string;
  name: string;
  city: string;
  listing_count: number;
  avg_price_per_sqft: number;
  price_min: number;
  price_max: number;
  bhks: number[];
  ready_to_move: number;
  near_metro: number;
  top_builder: string;
  societies: number;
  median_price_per_sqft: number;
  price_range_per_sqft: {
    low: number;
    high: number;
  };
  trend_direction: string;
  primary_signal: string;
  demand_score: number;
  recent_searches: number;
  last_searched_at?: string;
  evidence_gap_count: number;
  sample_size: number;
  last_updated: string;
};

export type AreaTrackerResponse = {
  generated_at: string;
  total_areas: number;
  total_listings: number;
  markets: AreaTrackerMarket[];
};

export type UpcomingLaunchCard = {
  id: string;
  builder_name: string;
  project_name: string;
  micro_market: string;
  city: string;
  launch_stage: string;
  starting_price_label: string;
  project_type_label: string;
  hero_image: string;
  image_alt: string;
  primary_highlight: string;
  secondary_highlight?: string;
  source_url: string;
  sponsored: boolean;
};

export type SearchSourceSpan = {
  start: number;
  end: number;
  raw_text?: string;
};

export type SearchIntent = {
  area: string | null;
  excluded_areas?: string[];
  excluded_societies?: string[];
  excluded_builders?: string[];
  areas?: string[];
  bhk: number | null;
  bhks?: number[];
  exclude_bhks?: number[];
  bhk_spans?: SearchSourceSpan[];
  budget_min?: number | null;
  budget_max: number | null;
  hard_constraints?: HardConstraint[];
  preferences: string[];
  positive_preferences?: PreferenceSignal[];
  negative_preferences?: PreferenceSignal[];
  ranking_priorities?: string[];
  buyer_archetype?: BuyerArchetype | null;
};

export type HardConstraint = {
  field: string;
  operator: "min" | "max";
  value: number;
  unit: string;
  raw_text: string;
};

export type PreferenceSignal = {
  raw_text: string;
  polarity: "positive" | "negative";
  expanded_keys: string[];
  gap_keys?: string[];
  weight: number;
};

export type BuyerArchetype =
  | "family"
  | "investor"
  | "risk_averse"
  | "value_buyer"
  | "luxury_buyer"
  | "end_user";

export type MatchReason = {
  preference: string;
  fact_key: string;
  display: string;
  score: number;
  confidence: number;
  source_type: string;
  scoring_method: string;
};

export type PreferenceCoverage = {
  preference: string;
  status: "matched" | "partial" | "no_data";
  fact_key: string | null;
};

export type MatchExplanation = {
  reasons: MatchReason[];
  preference_coverage: PreferenceCoverage[];
  graph_driven_pct: number;
  total_facts_consulted: number;
};

export type DataFreshness = {
  last_enriched: string;
  days_ago: number;
  freshness_label: string;
  fact_count: number;
  source_breakdown: Record<string, number>;
};

export type ConfidenceComponent = {
  dimension: string;
  score: number;
  weight: number;
  explanation: string;
};

export type ConfidenceScore = {
  overall: number;
  label: string;
  components: ConfidenceComponent[];
};

export type SearchResultItem = PropertyCard & {
  match_score: number;
  match_label: string;
  match_reason: string;
  match_explanation?: MatchExplanation;
  proof_focuses?: ProofFocus[];
  confidence_score?: ConfidenceScore;
  match_tier: "exact" | "supported";
  tradeoff_label?: string;
};

export type SearchAreaContext = {
  id: string;
  name: string;
  city: string;
  median_price_per_sqft: number;
  trend_direction: string;
  trend_summary: string;
  metro_access_summary: string;
  traffic_summary: string;
  waterlogging_summary: string;
  livability_summary: string;
  externality_tags: string[];
  infrastructure_tags: string[];
  community_notes: string;
};

export type SourcedClaim = {
  entity_name: string;
  claim: string;
  confidence: number;
  source_type: string;
};

export type KnowledgeContext = {
  claims: SourcedClaim[];
  nodes_consulted: number;
  learning_gaps: string[];
};

export type SearchGuidance = {
  mode: string;
  title: string;
  message: string;
  suggestions: string[];
};

export type SearchResultFocus = {
  mode: "named_society" | "ranked_matches" | string;
  society_id?: string | null;
  society_name?: string | null;
  focus_results: SearchResultItem[];
  sibling_configs?: SearchResultItem[];
  more_homes?: SearchResultItem[];
};

export type SearchResultSet = {
  branchId: string;
  label: string;
  results: SearchResultItem[];
};

export type SearchResponse = {
  query: string;
  resultSets: SearchResultSet[];
  totalMatches: number;
  areaContext?: SearchAreaContext;
  state: "results" | "no_matches";
  searchGuidance?: SearchGuidance;
};

export type ReraInfo = {
  registered: boolean;
  registration_number?: string;
  status?: string;
  start_date?: string;
  completion_date?: string;
  original_completion_date?: string;
  delay_months?: number;
  total_units?: number;
  total_land_area_sqm?: number;
  total_land_area_acres?: number;
  open_area_pct?: number;
  total_project_cost_inr?: number;
  land_cost_inr?: number;
  construction_cost_inr?: number;
  cost_per_unit_inr?: number;
  complaints_count?: number;
  complaints_resolved_pct?: number;
  project_complaints_count?: number;
  project_complaints_open_count?: number;
  project_complaints_disposed_count?: number;
  promoter_complaints_count?: number;
  promoter_complaints_open_count?: number;
  promoter_complaints_disposed_count?: number;
  complaint_summaries?: ReraComplaintScopeSummary[];
  document_manifest?: ReraDocumentManifestItem[];
  document_groups?: ReraDocumentGroupSummary[];
  affidavit_only_visible?: boolean;
  builder_total_projects?: number;
  builder_states?: string[];
  land_litigation?: boolean;
  escrow_bank?: string;
  has_borrowing?: boolean;
  has_mortgage?: boolean;
  lat_lng?: string;
  rera_portal_url?: string;
  last_verified?: string;
  decision_cards?: ReraDecisionCard[];
};

export type ReraReportRef = {
  registration_ids: string[];
  href: string;
  availability: "available" | "partial" | "unavailable";
};

export type ReraEvidenceReportResponse = {
  availability: "available" | "partial" | "unavailable";
  evidence: ReraEvidenceProjection;
  surface: ReraReportSurface;
  buyer_report?: ReraBuyerReport;
};

export type ReraBuyerReport = {
  fact_sections: ReraBuyerFactSection[];
  builder_portfolio?: BuilderPortfolio;
  complaints?: ReraBuyerComplaintSummary[];
  schedules?: ReraScheduleSection[];
  documents?: ReraBuyerDocument[];
  registry_url?: string;
};

export type ReraBuyerFactSection = {
  id: string;
  title: string;
  facts: ReraBuyerFact[];
};

export type ReraBuyerFact = {
  key: string;
  label: string;
  value: string;
  source_url?: string;
  learned_at: string;
};

export type ReraBuyerComplaintSummary = {
  scope: string;
  total: number;
  open: number;
  disposed: number;
  rows_parsed: number;
  status_counts_complete: boolean;
  theme_counts: Record<string, number>;
  sample_subjects?: string[];
};

export type ReraBuyerDocument = {
  id: string;
  label: string;
  group: string;
  group_label: string;
  url: string;
};

export type ReraEvidenceProjection = {
  schema_version: string;
  property_id: string;
  bundle_id: string;
  generated_at: string;
  registration_ids: string[];
  entities: ReraEvidenceEntity[];
  claims: ReraEvidenceClaim[];
  events: ReraEvidenceEvent[];
  series: ReraEvidenceSeries[];
  discrepancies: ReraEvidenceDiscrepancy[];
  regulatory_coverage: ReraRegulatoryCoverage[];
  source_index: ReraEvidenceSource[];
};

export type ReraEvidenceEntity = {
  entity_id: string;
  entity_type: string;
  label?: string;
  registration_id?: string;
};

export type ReraEvidenceClaimValue =
  | { type: "boolean"; data: boolean }
  | { type: "number"; data: number }
  | { type: "text" | "date" | "document_ref"; data: string }
  | { type: "money"; data: { amount: string; currency: string } }
  | { type: "entity_ref"; data: { entity_id: string; entity_type: string } };

export type ReraEvidenceClaim = {
  claim_id: string;
  subject: { entity_id: string; entity_type: string };
  predicate: string;
  value: ReraEvidenceClaimValue;
  unit?: string;
  effective_time?: { start?: string; end?: string; precision: string };
  assertion_mode: "registry_record" | "promoter_declaration" | "complainant_allegation" | "authority_order" | "system_derivation";
  source_trust: string;
  evidence: Array<{
    source_record_id: string;
    receipt_id: string;
    capture_id: string;
    locator: string;
    page?: number;
    supporting_quote?: string;
  }>;
  derivation?: { rule_id: string; rule_version: string; input_claim_ids: string[] };
};

export type ReraEvidenceEvent = {
  event_id: string;
  registration_id: string;
  promoter_id?: string;
  event_class: string;
  event_type: string;
  occurred_at: string;
  issuer: string;
  proceeding_ref?: string;
  decision_stage: string;
  disposition?: string;
  current_effect: string;
  affected_scope?: string;
  claim_ids: string[];
  source_ids: string[];
};

export type ReraEvidenceSeries = {
  series_id: string;
  registration_id: string;
  series_type: string;
  points: Array<{
    point_id: string;
    effective_at: string;
    quarter?: string;
    financial_year?: string;
    tower_count?: number;
    total_units?: number;
    booked_units?: number;
    unsold_units?: number;
    claim_ids: string[];
  }>;
};

export type ReraEvidenceDiscrepancy = {
  registration_id: string;
  rule_id: string;
  rule_version: string;
  comparisons: Array<{
    id: string;
    unit: string;
    relationship: "matching_values" | "different_values";
    left: Array<{ claim_id: string; predicate: string; value: number }>;
    right: Array<{ claim_id: string; predicate: string; value: number }>;
    observed_deltas: number[];
    input_claim_ids: string[];
  }>;
};

export type ReraRegulatoryCoverage = {
  source: string;
  checked_at: string;
  status: string;
};

export type ReraEvidenceSource = {
  receipt_id: string;
  capture_id: string;
  source_url: string;
  captured_at: string;
  content_type: string;
};

export type ReraReportSurface = {
  version: number;
  coverage_note: string;
  regulatory_event_order: string[];
  sections: ReraReportSurfaceSection[];
};

export type ReraReportSurfaceSection = {
  id: string;
  title: string;
  renderer: "fact_list" | "timeline" | "series" | "table" | "documents" | "regulatory_record";
  selectors: Array<{ key: string; label: string; format?: string }>;
  items_per_page?: number;
  preview_kinds: string[];
  empty_behavior: "omit";
};

export type ReraDecisionCard = {
  id: string;
  title: string;
  detail: string;
  tone: "positive" | "watch" | "neutral" | string;
  source: string;
  labels: string[];
  facts: Record<string, unknown>;
  actions: ReraDecisionAction[];
  confidence: number;
  validation_notes: string[];
};

export type ReraDecisionAction = {
  kind: string;
  label: string;
};

export type ReraComplaintScopeSummary = {
  scope: string;
  total_count_from_tab_label?: number;
  row_count_parsed: number;
  disposed_count: number;
  open_count: number;
  theme_counts: Record<string, number>;
  sample_subjects: string[];
  confidence: number;
  validation_notes: string[];
};

export type ReraDocumentManifestItem = {
  artifact_id: string;
  kind: string;
  label: string;
  source_url?: string;
  source_tab?: string;
  source_field_label?: string;
  document_group: string;
  buyer_visibility?: string;
  preview_policy?: string;
  configuration_type?: string;
  bedroom_count?: number;
  confidence?: number;
};

export type ReraDocumentGroupSummary = {
  group: string;
  count: number;
};

export type ReraScheduleSection = {
  group: string;
  label: string;
  rows: ReraScheduleRow[];
};

export type ReraScheduleRow = {
  label: string;
  available?: boolean;
  area_sqm?: number;
  value?: string;
  confidence?: number;
};

export type AreaIntelligence = {
  safety?: string;
  commute_reality?: string;
  water_supply?: string;
  noise_level?: string;
  green_cover?: string;
  community_vibe?: string;
  walkability?: string;
  school_quality?: string;
  grocery_shopping?: string;
  healthcare_access?: string;
  recurring_complaints?: string[];
  hidden_gems?: string[];
  deal_breakers?: string[];
  overall_sentiment?: string;
  source_count?: number;
  last_updated?: string;
};
