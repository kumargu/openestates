export type PropertyCard = {
  id: string;
  title: string;
  area: string;
  price: number;
  price_per_sqft: number;
  bhk: number;
  sqft: number;
  society_name: string;
  hero_image: string | null;
  transparency_tags: string[];
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

export type MarketActivity = {
  interest_level: "high" | "moderate" | "low";
  saves_last_7d: number;
  offers_last_7d: number;
  days_on_market: number;
  area_trend_summary: string;
};

export type AreaListItem = {
  id: string;
  name: string;
  median_price_per_sqft: number;
  trend_direction: string;
  primary_signal: string;
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

export type ApiError = {
  error: string;
};
