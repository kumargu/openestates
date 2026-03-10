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

export type SearchIntent = {
  area: string | null;
  bhk: number | null;
  budget_max: number | null;
  preferences: string[];
};

export type SearchResultItem = PropertyCard & {
  match_score: number;
  match_label: string;
  match_reason: string;
  semantic_score?: number;
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
