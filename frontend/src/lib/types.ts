export type PropertyCard = {
  id: string;
  title: string;
  area: string;
  price: number;
  price_per_sqft: number;
  bhk: number;
  sqft: number;
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
    society_quality_score: number;
    builder_quality_score: number;
    document_completeness_score: number;
    litigation_risk: number;
    noise_score: number;
    sunlight_score: number;
    airport_noise_score: number;
    waterlogging_risk_score: number;
    traffic_score: number;
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
  themes: CompareThemes;
  tradeoffs: TradeoffsResponse;
  market_activity: MarketActivityResponse;
  similar_properties: PropertyCard[];
  rera?: ReraInfo;
  area_intelligence?: AreaIntelligence;
};

export type ThemeLabel = "strong" | "good" | "mixed" | "weak";

export type ThemeResult = {
  label: ThemeLabel;
  summary: string;
};

export type CompareThemes = {
  value: ThemeResult;
  commute: ThemeResult;
  society: ThemeResult;
  greenery: ThemeResult;
  risk: ThemeResult;
  resale: ThemeResult;
  market: ThemeResult;
};

export type ThemeComponent = {
  label: string;
  level: ThemeLabel;
};

export type TradeoffsResponse = {
  headline: string;
  strengths: string[];
  cautions: string[];
  components: ThemeComponent[];
};

export type PriceVsMedian = {
  pct_diff: number;
  verdict: string;
  verdict_class: string;
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

export type ShortlistResponse = {
  shortlist: string[];
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

export type Polarity = "positive" | "negative";

export type BuyerArchetype =
  | "family"
  | "investor"
  | "endUser"
  | "riskAverse"
  | "valueBuyer"
  | "luxuryBuyer";

export type PreferenceSignal = {
  rawText: string;
  canonicalKey: string;
  expandedKeys: string[];
  polarity: Polarity;
  weight: number;
};

export type SearchIntent = {
  area: string | null;
  bhk: number | null;
  budget_max: number | null;
  /** Flat list for backward compat */
  preferences: string[];
  positivePreferences: PreferenceSignal[];
  negativePreferences: PreferenceSignal[];
  buyerArchetype: BuyerArchetype | null;
};

export type MatchReason = {
  preference: string;
  fact_key: string;
  display: string;
  score: number;
  confidence: number;
  source_type: string;
  scoring_method: "graph" | "legacy";
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

export type ExplanationReason = {
  text: string;
  preference: string;
  evidenceStrength: "strong" | "moderate" | "limited";
  sources: string[];
};

export type ExplanationConcern = {
  text: string;
  preference: string;
  severity: "caution" | "warning";
  sourceLevel: "society-specific" | "area-level";
  note?: string;
};

export type EvidenceSummary = {
  factsConsulted: number;
  sources: string[];
  graphDrivenPct: number;
};

export type ExplanationCard = {
  whyMatches: ExplanationReason[];
  concerns: ExplanationConcern[];
  unmatched: string[];
  confidenceLabel: "high" | "medium" | "low";
  evidenceSummary: EvidenceSummary;
};

export type Concern = {
  preference: string;
  display: string;
  confidence: number;
  sourceLevel: string;
  severity: "caution" | "warning";
};

export type SearchResultItem = PropertyCard & {
  match_score: number;
  match_label: string;
  match_reason: string;
  match_explanation?: MatchExplanation;
  semantic_score?: number;
  societyScore?: number;
  societyConfidence?: "high" | "medium" | "low";
  concerns?: Concern[];
  unmatchedPreferences?: string[];
  explanationCard?: ExplanationCard;
  active_seller_count?: number;
  bid_stats?: BidStats;
};

// ---------------------------------------------------------------------------
// Marketplace — sellers + bids
// ---------------------------------------------------------------------------

export type SellerVerificationTier = "identity_verified" | "rera_verified";

export type ListingStatus = "active" | "under_offer" | "matched" | "sold";

export type Seller = {
  id: string;
  name: string;
  phone: string;
  verificationTier: SellerVerificationTier;
  reraNumber?: string;
  listingIds: string[];
  registeredAt: string;
};

export type BidStatus = "pending" | "accepted" | "rejected";

export type Bid = {
  id: string;
  amount: number;
  createdAt: string;
  status: BidStatus;
};

export type BidStats = {
  propertyId: string;
  count: number;
  average: number;
  p100: number;
  lastBidAt?: string;
};

export type PlaceBidRequest = {
  bidderName: string;
  bidderPhone?: string;
  amount: number;
};

export type PlaceBidResponse = {
  bid: Bid;
  bidStats: BidStats;
};

export type RegisterSellerRequest = {
  name: string;
  phone: string;
  reraNumber?: string;
};

export type SellerListingItem = {
  propertyId: string;
  listingStatus: ListingStatus;
  bidStats: BidStats;
};

export type SellerListingsResponse = {
  seller: Seller;
  listings: SellerListingItem[];
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

export type PreferenceDebugScore = {
  preference: string;
  polarity: string;
  matched_fact_key: string | null;
  matched_fact_value: string | null;
  raw_score: number;
  weighted_score: number;
  source: string | null;
  confidence: number;
};

export type SocietyDebugScore = {
  society_id: string;
  society_name: string;
  final_score: number;
  confidence: number;
  archetype_modifier: number;
  area_signals_used: string[];
  facts_consulted: number;
  preference_scores: PreferenceDebugScore[];
};

export type SearchDebugTrace = {
  timestamp: string;
  query: string;
  candidate_count: number;
  also_consider_triggered: boolean;
  also_consider_reason: string | null;
  discovery_triggered: boolean;
  total_latency_ms: number;
  intent_parse_ms: number;
  scoring_ms: number;
  embedding_ms: number;
  societies_scored: SocietyDebugScore[];
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
  also_consider?: SearchResultItem[];
  debug?: SearchDebugTrace;
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

export type ApiError = {
  error: string;
};

// Society search types — aligned with pipeline/society_scorer.py output
export type SocietySearchResult = {
  slug: string;
  name: string;
  builder: string;
  year_built?: number;
  total_units?: number;
  unit_types?: string;
  price_range?: string;
  summary: string;
  overall_score: number;
  rank: number;
  best_for_label: string;
  life_fit_reason: string;
  dimension_scores: Record<string, number>;
  confidence: string; // "high" | "moderate" | "low"
  evidence: {
    reddit_threads: number;
    society_threads: number;
    area_threads: number;
    has_seed_data: boolean;
    reddit_confidence: string;
  };
  photos: string[];
  signals: string[];
  cautions: string[];
  resident_quote: string | null;
  why_above_next: string;
};

export type SocietySearchResponse = {
  query_interpreted: {
    original: string;
    area: string;
    city: string;
    intent: string;
    weights_applied: Record<string, number>;
  };
  results: SocietySearchResult[];
  result_count: number;
  area_context: {
    name: string;
    city: string;
    median_price_per_sqft: number;
    trend_direction: string;
    trend_summary: string;
    metro_access_summary: string;
    traffic_summary: string;
    livability_summary: string;
    infrastructure_tags: string[];
    externality_tags: string[];
    community_notes: string;
  } | null;
  enrichment_status: {
    societies_discovered: number;
    societies_scored: number;
    reddit_enriched: number;
    seed_matched: number;
    photos_available: number;
    scored_at: string;
  };
};
