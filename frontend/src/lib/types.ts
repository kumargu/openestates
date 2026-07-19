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
  price_per_sqft: number;
  bhk: number;
  sqft: number;
  carpet_area_sqft?: number;
  super_builtup_sqft?: number;
  society_name: string;
  builder_name: string;
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
  seller_id?: string;
  seller_completeness_pct?: number;
  documents_provided?: string[];
  seller_verified?: boolean;
  root_source?: string;
  project_status?: string;
  project_status_display?: string;
  home_state_display?: string;
  builder_delivery_display?: string;
  data_freshness?: DataFreshness;
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
  proof_label: string;
  cards: DiscoveryShelfCard[];
};

export type DiscoveryResponse = {
  product_promise: string;
  quotes: DiscoveryQuote[];
  shelves: DiscoveryShelf[];
};

export type RecommendationLens = "proof" | "value" | "trust" | "commute";

export type RecommendationBranch = {
  lens: RecommendationLens;
  headline: string;
  property: PropertyCard;
  contrast: string;
  tradeoff?: string;
  evidence_delta: {
    fact_count: number;
    gap_count: number;
    fact_delta: number;
    gap_delta: number;
  };
  magnitude: number;
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
    seller_id?: string;
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
  market_activity: MarketActivityResponse;
  similar_properties: PropertyCard[];
  recommendation_branches?: RecommendationBranch[];
  rera?: ReraInfo;
  area_intelligence?: AreaIntelligence;
  transparency_score?: TransparencyScore;
  area_price_range_low?: number;
  area_price_range_high?: number;
  seller?: SellerSummary;
  interest_count?: number;
  root_source?: string;
  project_status_display?: string;
  project_status?: string;
  home_state_display?: string;
  builder_trust?: {
    delivery_rate?: number;
    project_count?: number;
    delivery_display?: string;
    zero_revocations?: boolean;
  };
  builder_portfolio?: BuilderPortfolio;
  source_panels?: SourcePanel[];
  data_freshness?: DataFreshness;
  confidence_score?: ConfidenceScore;
  /** Backend-shaped dynamic evidence cards — prefer over source_panels. */
  evidence?: PropertyEvidenceResponse;
  external_reviews?: {
    google_rating?: number;
    google_review_count?: number;
    google_reviews_url?: string;
  };
  livability_brief?: LivabilityBrief;
};

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
 *   BHK, carpet area, price, seller source, or per-listing market activity.
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
  blocks: LivabilityBriefBlock[];
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
  completion_date?: string;
  delay_months?: number;
  complaints_count?: number;
  project_status_display?: string;
  current: boolean;
};

export type SellerSummary = {
  id: string;
  name: string;
  verified: boolean;
  completeness_pct: number;
  property_prompt?: string;
  documents_provided: string[];
};

export type PriceVsMedian = {
  pct_diff: number;
  verdict: string;
  verdict_class: string;
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

export type MarketActivityResponse = {
  interest_level: string;
  saves_last_7d: number | null;
  offers_last_7d: number | null;
  days_on_market: number;
  days_on_market_label: string;
  interest_label: string;
  area_trend_summary: string;
  price_vs_median: PriceVsMedian | null;
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

export type SearchIntent = {
  area: string | null;
  bhk: number | null;
  budget_max: number | null;
  hard_constraints?: HardConstraint[];
  preferences: string[];
  positive_preferences?: PreferenceSignal[];
  negative_preferences?: PreferenceSignal[];
  buyer_archetype?: BuyerArchetype | null;
};

export type HardConstraint = {
  field: string;
  operator: "min";
  value: number;
  unit: string;
  raw_text: string;
};

export type PreferenceSignal = {
  raw_text: string;
  polarity: "positive" | "negative";
  expanded_keys: string[];
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
  semantic_score?: number;
  confidence_score?: ConfidenceScore;
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

export type SearchResponse = {
  query: string;
  intent: SearchIntent;
  results: SearchResultItem[];
  area_context: SearchAreaContext | null;
  total_results: number;
  knowledge_context: KnowledgeContext | null;
  discovery_status?: string;
  discovery_count?: number;
};

export type ReraInfo = {
  registered: boolean;
  registration_number?: string;
  status?: string;
  completion_date?: string;
  original_completion_date?: string;
  delay_months?: number;
  total_units?: number;
  total_project_cost_inr?: number;
  land_cost_inr?: number;
  construction_cost_inr?: number;
  cost_per_unit_inr?: number;
  complaints_count?: number;
  complaints_resolved_pct?: number;
  builder_total_projects?: number;
  builder_revocations?: number;
  builder_states?: string[];
  land_litigation?: boolean;
  escrow_bank?: string;
  has_borrowing?: boolean;
  has_mortgage?: boolean;
  lat_lng?: string;
  rera_portal_url?: string;
  last_verified?: string;
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
