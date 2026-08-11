import type {
  AreaListItem,
  DiscoveryResponse,
  MatchExplanation,
  PropertyCard,
  PropertyDetailResponse,
  SearchAreaContext,
  SearchResponse,
} from "./types.ts";

const now = "2026-07-11T00:00:00.000Z";

const fixturePropertyRows: Array<Omit<PropertyCard, "kg_entity_refs">> = [
  {
    id: "fixture-prestige-lakeside-3bhk",
    title: "3 BHK at Prestige Lakeside Habitat",
    area: "Whitefield",
    price: 19000000,
    price_per_sqft: 14520,
    bhk: 3,
    sqft: 1308,
    society_name: "Prestige Lakeside Habitat",
    builder_name: "Prestige Group",
    hero_image: null,
    transparency_tags: ["RERA rooted", "Docs visible", "Market checked"],
    description_summary: "Lake-facing tower with strong source confidence and a clean ownership trail.",
    possession_status: "ready",
    metro_distance_mins: 12,
    floor: 12,
    total_floors: 28,
    facing: "East",
    google_rating: 4.2,
    google_review_count: 1800,
    root_source: "rera",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 5-10 yrs old",
    builder_delivery_display: "Strong delivery record",
    data_freshness: freshness(142, 54, { rera: 24, self_reported: 11, reviews: 19 }),
  },
  {
    id: "fixture-samadhura-capitol-3bhk",
    title: "3 BHK at Samadhura Capitol Residences",
    area: "Whitefield",
    price: 16000000,
    price_per_sqft: 13280,
    bhk: 3,
    sqft: 1205,
    society_name: "Samadhura Capitol Residences",
    builder_name: "Samadhura",
    hero_image: null,
    transparency_tags: ["Below median", "Plan visible", "RERA rooted"],
    description_summary: "Efficient resale option near the corridor with strong price discipline.",
    possession_status: "ready",
    metro_distance_mins: 8,
    floor: 9,
    total_floors: 24,
    facing: "North East",
    google_rating: 4.0,
    google_review_count: 960,
    root_source: "rera",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 1-5 yrs old",
    builder_delivery_display: "On-time pattern",
    data_freshness: freshness(96, 38, { rera: 18, reviews: 12 }),
  },
  {
    id: "fixture-vaswani-starlight-3bhk",
    title: "3 BHK at Vaswani Starlight",
    area: "Whitefield",
    price: 21000000,
    price_per_sqft: 15850,
    bhk: 3,
    sqft: 1325,
    society_name: "Vaswani Starlight",
    builder_name: "Vaswani",
    hero_image: null,
    transparency_tags: ["Self-reported", "Risk review", "Negotiation required"],
    description_summary: "Premium ask with stronger due-diligence needs before visit.",
    possession_status: "under_construction",
    metro_distance_mins: 15,
    floor: 16,
    total_floors: 30,
    facing: "West",
    google_rating: 3.8,
    google_review_count: 320,
    root_source: "seller",
    project_status: "under_construction",
    project_status_display: "Under construction",
    home_state_display: "Under construction",
    builder_delivery_display: "Verify handover",
    data_freshness: freshness(42, 91, { self_reported: 12, discovery: 18 }),
  },
  {
    id: "fixture-sobha-royal-pavilion-4bhk",
    title: "4 BHK at Sobha Royal Pavilion",
    area: "Sarjapur Road",
    price: 28500000,
    price_per_sqft: 17100,
    bhk: 4,
    sqft: 1667,
    society_name: "Sobha Royal Pavilion",
    builder_name: "Sobha",
    hero_image: null,
    transparency_tags: ["Premium society", "Amenity dense", "Resale depth"],
    description_summary: "Premium family-sized home with strong society signal and higher entry price.",
    possession_status: "ready",
    metro_distance_mins: 18,
    floor: 18,
    total_floors: 30,
    facing: "North",
    google_rating: 4.3,
    google_review_count: 1400,
    root_source: "rera",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 1-5 yrs old",
    builder_delivery_display: "Strong delivery record",
    data_freshness: freshness(118, 45, { rera: 21, reviews: 20 }),
  },
  {
    id: "fixture-prestige-city-3bhk",
    title: "3 BHK at The Prestige City",
    area: "Sarjapur Road",
    price: 17800000,
    price_per_sqft: 13760,
    bhk: 3,
    sqft: 1294,
    society_name: "The Prestige City",
    builder_name: "Prestige Group",
    hero_image: null,
    transparency_tags: ["Township", "School access", "Value band"],
    description_summary: "Township option with practical family tradeoffs and better price discipline.",
    possession_status: "under_construction",
    metro_distance_mins: 22,
    floor: 11,
    total_floors: 27,
    facing: "East",
    google_rating: 4.1,
    google_review_count: 860,
    root_source: "rera",
    project_status: "under_construction",
    project_status_display: "Under construction",
    home_state_display: "Under construction",
    builder_delivery_display: "Track phase handover",
    data_freshness: freshness(76, 62, { rera: 16, reviews: 11 }),
  },
  {
    id: "fixture-embassy-pristine-3bhk",
    title: "3 BHK at Embassy Pristine",
    area: "Bellandur",
    price: 25500000,
    price_per_sqft: 18200,
    bhk: 3,
    sqft: 1401,
    society_name: "Embassy Pristine",
    builder_name: "Embassy",
    hero_image: null,
    transparency_tags: ["Lake externality", "Premium resale", "Traffic caution"],
    description_summary: "Premium Bellandur option with strong resale demand and externality checks.",
    possession_status: "ready",
    metro_distance_mins: 14,
    floor: 10,
    total_floors: 19,
    facing: "East",
    google_rating: 4.0,
    google_review_count: 720,
    root_source: "rera",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 5-10 yrs old",
    builder_delivery_display: "Stable",
    data_freshness: freshness(64, 77, { rera: 12, reviews: 18 }),
  },
  {
    id: "fixture-adarsh-palm-retreat-3bhk",
    title: "3 BHK at Adarsh Palm Retreat",
    area: "Bellandur",
    price: 23000000,
    price_per_sqft: 16550,
    bhk: 3,
    sqft: 1390,
    society_name: "Adarsh Palm Retreat",
    builder_name: "Adarsh",
    hero_image: null,
    transparency_tags: ["Established community", "Negotiation room", "Commute tradeoff"],
    description_summary: "Established gated community with livability strength and commute tradeoffs.",
    possession_status: "ready",
    metro_distance_mins: 16,
    floor: 7,
    total_floors: 16,
    facing: "South East",
    google_rating: 4.4,
    google_review_count: 2100,
    root_source: "seller",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 10+ yrs old",
    builder_delivery_display: "Established society",
    data_freshness: freshness(38, 110, { self_reported: 9, reviews: 20 }),
  },
  {
    id: "fixture-karle-zenith-3bhk",
    title: "3 BHK at Karle Zenith Residences",
    area: "Hebbal",
    price: 24000000,
    price_per_sqft: 16800,
    bhk: 3,
    sqft: 1429,
    society_name: "Karle Zenith Residences",
    builder_name: "Karle",
    hero_image: null,
    transparency_tags: ["North corridor", "Premium tower", "Airport access"],
    description_summary: "North Bengaluru premium tower with airport access and higher carrying cost.",
    possession_status: "ready",
    metro_distance_mins: 20,
    floor: 21,
    total_floors: 32,
    facing: "West",
    google_rating: 4.1,
    google_review_count: 540,
    root_source: "rera",
    project_status: "ready_to_move",
    project_status_display: "Ready to move",
    home_state_display: "Delivered · 1-5 yrs old",
    builder_delivery_display: "Verify tower corpus",
    data_freshness: freshness(71, 83, { rera: 13, reviews: 16 }),
  },
];

export const fixtureProperties: PropertyCard[] = fixturePropertyRows.map((property) => ({
  ...property,
  kg_entity_refs: fixtureKgEntityRefs(property),
}));

export const fixtureAreas: AreaListItem[] = [
  { id: "whitefield", name: "Whitefield", median_price_per_sqft: 14520, trend_direction: "up", primary_signal: "Metro access is now the key value unlock." },
  { id: "sarjapur-road", name: "Sarjapur Road", median_price_per_sqft: 15430, trend_direction: "up", primary_signal: "Township supply is deep, but phase risk matters." },
  { id: "bellandur", name: "Bellandur", median_price_per_sqft: 17350, trend_direction: "stable", primary_signal: "Premium demand with lake and traffic externalities." },
  { id: "hebbal", name: "Hebbal", median_price_per_sqft: 16800, trend_direction: "up", primary_signal: "Airport corridor premium is holding." },
];

export const fixtureDiscovery: DiscoveryResponse = {
  product_promise: "Tell us the life you want. We'll show homes with receipts.",
  quotes: [
    { text: "Fewer homes. Better reasons.", tone: "proof" },
    { text: "Search by tradeoff, not checkbox.", tone: "intent" },
    { text: "Receipts before recommendations.", tone: "trust" },
    { text: "Area context before site visits.", tone: "proof" },
  ],
  shelves: [
    {
      id: "verified_value",
      title: "Value with receipts",
      quote: "Good price, proof attached.",
      description: "Lower per-sqft options with visible source signals.",
      search_query: "good value with proof",
      receipt_copy: "Price + RERA + Google",
      cards: fixtureProperties
        .filter((property) => property.price_per_sqft > 0)
        .sort((a, b) => a.price_per_sqft - b.price_per_sqft)
        .slice(0, 3)
        .map((property) => ({
          property,
          reason: `${property.price_per_sqft.toLocaleString("en-IN")} /sqft with ${property.transparency_tags.length} source tags`,
        })),
    },
    {
      id: "low_commute_pain",
      title: "Low commute pain",
      quote: "Shorter commute, cleaner proof.",
      description: "Homes with closer metro access or stronger traffic signals.",
      search_query: "near metro low traffic",
      receipt_copy: "Metro + traffic signals",
      cards: fixtureProperties
        .filter((property) => property.metro_distance_mins > 0 && property.metro_distance_mins <= 15)
        .sort((a, b) => a.metro_distance_mins - b.metro_distance_mins)
        .slice(0, 3)
        .map((property) => ({
          property,
          reason: `${property.metro_distance_mins} min metro access`,
        })),
    },
    {
      id: "family_ready",
      title: "Family-ready societies",
      quote: "More life-fit, less guesswork.",
      description: "3BHK+ homes with society, risk, and review signals.",
      search_query: "family friendly 3BHK",
      receipt_copy: "Society + risk + reviews",
      cards: fixtureProperties
        .filter((property) => property.bhk >= 3)
        .slice(0, 3)
        .map((property) => ({
          property,
          reason: `${property.bhk} BHK with ${property.transparency_tags.length} visible signals`,
        })),
    },
  ],
};

const areaContexts: Record<string, SearchAreaContext> = {
  whitefield: {
    id: "whitefield",
    name: "Whitefield",
    city: "Bengaluru",
    median_price_per_sqft: 14520,
    trend_direction: "up",
    trend_summary: "Ready homes near the metro corridor are holding value, while stretched asks need proof from recent resale comps.",
    metro_access_summary: "Operational metro access improves commute certainty, but station distance still matters tower by tower.",
    traffic_summary: "Peak-hour traffic remains heavy around ITPL and Varthur Road.",
    waterlogging_summary: "Waterlogging risk is localized; verify the final access road and basement history.",
    livability_summary: "Strong schools, offices, malls, and hospital access make the area resilient for family demand.",
    externality_tags: ["traffic-sensitive", "monsoon-check"],
    infrastructure_tags: ["metro-corridor", "office-demand"],
    community_notes: "Add homes by exact approach road, not only society name.",
  },
  "sarjapur road": {
    id: "sarjapur-road",
    name: "Sarjapur Road",
    city: "Bengaluru",
    median_price_per_sqft: 15430,
    trend_direction: "up",
    trend_summary: "Large township inventory gives choice, but phase handover and school commute should drive ranking.",
    metro_access_summary: "Metro is more future optionality than current daily convenience.",
    traffic_summary: "Outer Ring Road access is useful but can add time during school and office peaks.",
    waterlogging_summary: "Pockets vary; verify basement and entry-road drainage.",
    livability_summary: "Strong family demand because of schools, offices, and newer gated supply.",
    externality_tags: ["phase-risk", "school-commute"],
    infrastructure_tags: ["orr-access", "township-supply"],
    community_notes: "Prefer projects with visible handover history and active resident maintenance feedback.",
  },
  bellandur: {
    id: "bellandur",
    name: "Bellandur",
    city: "Bengaluru",
    median_price_per_sqft: 17350,
    trend_direction: "stable",
    trend_summary: "Premium communities trade at a corridor premium; discount homes often carry commute or externality risk.",
    metro_access_summary: "Metro access is indirect today; commute depends heavily on office side and daily timing.",
    traffic_summary: "Heavy traffic on ORR is the primary buyer tradeoff.",
    waterlogging_summary: "Lake-adjacent pockets need explicit monsoon and basement checks.",
    livability_summary: "Mature social infrastructure and premium gated stock keep resale demand deep.",
    externality_tags: ["lake-externality", "orr-traffic"],
    infrastructure_tags: ["office-demand", "premium-resale"],
    community_notes: "Do not compare Bellandur only by price per sqft; externality risk changes fair value.",
  },
  hebbal: {
    id: "hebbal",
    name: "Hebbal",
    city: "Bengaluru",
    median_price_per_sqft: 16800,
    trend_direction: "up",
    trend_summary: "North Bengaluru premium supply benefits from airport access and business-park demand.",
    metro_access_summary: "Current metro convenience is limited; airport and ring-road access matter more today.",
    traffic_summary: "Flyover bottlenecks are timing-sensitive.",
    waterlogging_summary: "Lower waterlogging risk in many pockets, but tower-level drainage still matters.",
    livability_summary: "Premium towers have improving retail and school access.",
    externality_tags: ["airport-corridor", "flyover-bottleneck"],
    infrastructure_tags: ["north-growth", "business-parks"],
    community_notes: "Rank by maintenance corpus and tower density, not just airport distance.",
  },
};

export function getFixtureResponse(path: string): unknown | null {
  const [pathname, queryString = ""] = path.split("?");
  const params = new URLSearchParams(queryString);

  if (pathname === "/api/properties") return fixtureProperties;
  if (pathname === "/api/areas") return fixtureAreas;
  if (pathname === "/api/discovery") return fixtureDiscovery;

  if (pathname === "/api/search") {
    return searchFixtureProperties(params.get("q") ?? "");
  }

  const propertyMatch = pathname.match(/^\/api\/properties\/([^/]+)$/);
  if (propertyMatch) {
    const id = decodeURIComponent(propertyMatch[1]);
    const card = fixtureProperties.find((property) => property.id === id);
    return card ? makeDetail(card) : null;
  }

  return null;
}

function searchFixtureProperties(query: string): SearchResponse {
  const normalized = query.trim().toLowerCase();
  const intent = parseIntent(normalized);
  const areaContext = intent.area ? areaContexts[intent.area.toLowerCase()] ?? null : null;
  const scored = fixtureProperties
    .map((property) => {
      const match = scoreFixtureProperty(property, normalized, intent);
      return { property, ...match };
    })
    .filter((result) => result.score > 0 || normalized.length === 0)
    .sort((a, b) => b.score - a.score);

  const results = scored.map(({ property, score, reason }) => ({
    ...property,
    match_score: Math.round(score),
    match_label: score >= 82 ? "Strong match" : score >= 68 ? "Good match" : score >= 54 ? "Value pick" : "Good match",
    match_reason: reason,
    match_explanation: makeMatchExplanation(property, intent),
    confidence_score: confidenceFor(property),
  }));

  return {
    query,
    intent: {
      area: intent.area,
      bhk: intent.bhk,
      budget_max: intent.budgetMax,
      preferences: intent.preferences,
    },
    results,
    area_context: areaContext,
    total_results: results.length,
    knowledge_context: {
      claims: areaContext
        ? [
            {
              entity_name: areaContext.name,
              claim: areaContext.trend_summary,
              confidence: 0.74,
              source_type: "fixture",
            },
          ]
        : [],
      nodes_consulted: areaContext ? 4 : 2,
      learning_gaps: ["Live backend unavailable; showing local product-review fixtures."],
    },
  };
}

function parseIntent(query: string): {
  area: string | null;
  bhk: number | null;
  budgetMax: number | null;
  preferences: string[];
} {
  const area = Object.values(areaContexts).find((ctx) => query.includes(ctx.name.toLowerCase()))?.name ?? null;
  const bhk = Number(query.match(/\b([1-5])\s*bhk\b/)?.[1] ?? "") || null;
  const croreMatch = query.match(/(?:under|below|upto|up to)\s*([0-9.]+)\s*(?:cr|crore)/);
  const lakhMatch = query.match(/(?:under|below|upto|up to)\s*([0-9.]+)\s*(?:l|lac|lakh)/);
  const budgetMax = croreMatch
    ? Math.round(Number(croreMatch[1]) * 10_000_000)
    : lakhMatch
      ? Math.round(Number(lakhMatch[1]) * 100_000)
      : null;
  const preferences = [
    query.includes("metro") ? "near metro" : null,
    query.includes("family") || query.includes("school") ? "family friendly" : null,
    query.includes("quiet") ? "low noise" : null,
    query.includes("ready") ? "ready to move" : null,
    query.includes("premium") ? "premium society" : null,
  ].filter((value): value is string => value !== null);

  return { area, bhk, budgetMax, preferences };
}

function levenshtein(left: string, right: string): number {
  if (left === right) return 0;
  if (left.length === 0) return right.length;
  if (right.length === 0) return left.length;

  const prev = Array.from({ length: right.length + 1 }, (_, index) => index);
  const curr = new Array<number>(right.length + 1);

  for (let i = 0; i < left.length; i += 1) {
    curr[0] = i + 1;
    for (let j = 0; j < right.length; j += 1) {
      const cost = left[i] === right[j] ? 0 : 1;
      curr[j + 1] = Math.min(curr[j] + 1, prev[j + 1] + 1, prev[j] + cost);
    }
    for (let j = 0; j < prev.length; j += 1) {
      prev[j] = curr[j];
    }
  }

  return prev[right.length];
}

function scoreFixtureProperty(
  property: PropertyCard,
  query: string,
  intent: ReturnType<typeof parseIntent>,
): { score: number; reason: string } {
  let score = 48;
  const reasons: string[] = [];
  const searchable = [
    property.title,
    property.area,
    property.society_name,
    property.builder_name,
    property.description_summary,
    ...property.transparency_tags,
  ].join(" ").toLowerCase();

  if (!query) return { score: 70, reason: "Showing all locally available properties." };
  if (intent.area && property.area.toLowerCase() !== intent.area.toLowerCase()) {
    return { score: 0, reason: "" };
  }
  if (intent.bhk && property.bhk !== intent.bhk) {
    return { score: 0, reason: "" };
  }
  if (intent.budgetMax && property.price > intent.budgetMax * 1.15) {
    return { score: 0, reason: "" };
  }
  if (intent.area && property.area.toLowerCase() === intent.area.toLowerCase()) {
    score += 24;
    reasons.push(`matches ${intent.area}`);
  }
  if (intent.bhk && property.bhk === intent.bhk) {
    score += 16;
    reasons.push(`${intent.bhk} BHK`);
  }
  if (intent.budgetMax && property.price <= intent.budgetMax) {
    score += 14;
    reasons.push("within budget");
  } else if (intent.budgetMax) {
    score -= 10;
    reasons.push("slightly above budget");
  }
  if (query.includes("metro") && property.metro_distance_mins <= 15) {
    score += 10;
    reasons.push("metro-accessible");
  }
  if (query.includes("ready") && property.project_status === "ready_to_move") {
    score += 10;
    reasons.push("ready to move");
  }
  if (searchable.includes(query)) score += 20;
  const queryTokens = query.split(/\s+/).filter((token) => token.length >= 4);
  if (
    queryTokens.some((token) =>
      searchable.split(/[^a-z0-9]+/).some((word) => {
        if (word.length < 4) return false;
        if (word.includes(token) || token.includes(word)) return true;
        return levenshtein(token, word) <= (Math.max(token.length, word.length) >= 8 ? 2 : 1);
      }),
    )
  ) {
    score += 20;
    reasons.push(`matched "${query}" with fuzzy society recall`);
  }
  if (property.root_source === "rera") score += 5;

  return {
    score: Math.max(0, Math.min(96, score)),
    reason: reasons.length > 0
      ? `Matched on ${reasons.join(", ")} with trust and market signals available.`
      : "Relevant fallback result with trust and market signals available.",
  };
}

function makeMatchExplanation(property: PropertyCard, intent: ReturnType<typeof parseIntent>): MatchExplanation {
  const reasons = [
    intent.area && property.area.toLowerCase() === intent.area.toLowerCase()
      ? reason("area", "located_in", `${property.society_name} is in ${property.area}.`, 1)
      : null,
    intent.bhk && property.bhk === intent.bhk
      ? reason("configuration", "bhk", `${property.bhk} BHK matches the requested configuration.`, 0.95)
      : null,
    property.root_source === "rera"
      ? reason("trust", "root_source", "Listing has an RERA-rooted source chain.", 0.88)
      : reason("trust", "root_source", "Self-reported listing needs source-chain verification.", 0.55),
  ].filter((value): value is ReturnType<typeof reason> => value !== null);

  return {
    reasons,
    preference_coverage: [
      { preference: "location", status: intent.area ? "matched" : "partial", fact_key: "located_in" },
      { preference: "trust", status: "matched", fact_key: "root_source" },
      { preference: "budget", status: intent.budgetMax ? property.price <= intent.budgetMax ? "matched" : "partial" : "no_data", fact_key: intent.budgetMax ? "price" : null },
    ],
    graph_driven_pct: property.root_source === "rera" ? 72 : 48,
    total_facts_consulted: 12,
  };
}

function reason(preference: string, factKey: string, display: string, score: number) {
  return {
    preference,
    fact_key: factKey,
    display,
    score,
    confidence: score,
    source_type: "fixture",
    scoring_method: "graph" as const,
  };
}

function makeDetail(card: PropertyCard): PropertyDetailResponse {
  const area = areaContexts[card.area.toLowerCase()] ?? areaContexts.whitefield;
  const trust = confidenceFor(card);
  const risk = card.area === "Bellandur" ? 0.38 : card.root_source === "seller" ? 0.34 : 0.18;
  const carpetRatio = card.price_per_sqft > 17000 ? 0.69 : 0.74;

  return {
    property: {
      id: card.id,
      title: card.title,
      area: card.area,
      area_id: area.id,
      city: "Bengaluru",
      society_id: slug(card.society_name),
      builder_name: card.builder_name,
      property_type: "Apartment",
      listing_type: "resale",
      bhk: card.bhk,
      price: card.price,
      price_per_sqft: card.price_per_sqft,
      carpet_area_sqft: Math.round(card.sqft * carpetRatio),
      super_builtup_sqft: card.sqft,
      floor: card.floor,
      total_floors: card.total_floors,
      facing: card.facing,
      possession_status: card.possession_status,
      metro_distance_mins: card.metro_distance_mins,
      maintenance_cost_monthly: Math.round(card.sqft * 5.5),
      society_quality_score: card.google_rating ? card.google_rating / 5 : 0.72,
      builder_quality_score: card.root_source === "rera" ? 0.82 : 0.58,
      document_completeness_score: card.root_source === "rera" ? 0.84 : 0.48,
      litigation_risk: risk,
      noise_score: card.area === "Bellandur" ? 0.42 : 0.24,
      sunlight_score: 0.72,
      airport_noise_score: card.area === "Hebbal" ? 0.28 : 0.12,
      waterlogging_risk_score: card.area === "Bellandur" ? 0.46 : 0.18,
      traffic_score: card.area === "Whitefield" || card.area === "Bellandur" ? 0.52 : 0.64,
      days_on_market: 28,
      greenery_score: card.area === "Bellandur" ? 0.76 : 0.62,
      open_space_score: 0.68,
      resale_strength_score: card.root_source === "rera" ? 0.78 : 0.62,
      interest_level: card.price_per_sqft > 17000 ? "moderate" : "high",
      saves_last_7d: card.root_source === "rera" ? 18 : 7,
      offers_last_7d: card.root_source === "rera" ? 2 : 0,
      images: [],
      hero_image: "",
      description_summary: card.description_summary,
      transparency_tags: card.transparency_tags,
      source_reference: "Local development fixture",
    },
    entity_refs: card.kg_entity_refs,
    society: {
      id: slug(card.society_name),
      name: card.society_name,
      area: card.area,
      city: "Bengaluru",
      builder_name: card.builder_name,
      year_built: card.project_status === "ready_to_move" ? 2021 : 2027,
      total_units: 980,
      summary: `${card.society_name} is tracked with source, pricing, externality, and livability signals in the local fixture set.`,
      maintenance_sentiment: "Generally positive with tower-level variation.",
      livability_sentiment: "Strong for families when commute timing works.",
      common_positives: ["Amenity depth", "Resale demand", "Neighbourhood access"],
      common_complaints: ["Peak traffic", "Maintenance variation"],
      review_summary: "Resident feedback is directionally positive, but exact tower and access-road checks still matter.",
    },
    area: {
      id: area.id,
      name: area.name,
      city: area.city,
      median_price_per_sqft: area.median_price_per_sqft,
      trend_direction: area.trend_direction,
      trend_summary: area.trend_summary,
      metro_access_summary: area.metro_access_summary,
      traffic_summary: area.traffic_summary,
      waterlogging_summary: area.waterlogging_summary,
      livability_summary: area.livability_summary,
      externality_tags: area.externality_tags,
      infrastructure_tags: area.infrastructure_tags,
      community_notes: area.community_notes,
    },
    similar_properties: fixtureProperties.filter((property) => property.id !== card.id && property.area === card.area).slice(0, 3),
    rera_report_ref: {
      registration_ids: [],
      href: `/property/${card.id}/rera`,
      availability: card.root_source === "rera" ? "partial" : "unavailable",
    },
    transparency_score: {
      overall: Math.round(trust.overall * 100),
      components: [
        { label: "Source chain", score: card.root_source === "rera" ? 30 : 16, max_score: 30 },
        { label: "Documents", score: card.root_source === "rera" ? 24 : 12, max_score: 30 },
      ],
      explainer: "Local fixture score mirrors the production trust model shape for UI review.",
    },
    root_source: card.root_source,
    project_status: card.project_status,
    project_status_display: card.project_status_display,
    home_state_display: card.home_state_display,
    builder_trust: {
      delivery_rate: card.root_source === "rera" ? 0.84 : 0.62,
      project_count: 12,
      delivery_display: card.builder_delivery_display,
    },
    data_freshness: card.data_freshness,
    confidence_score: trust,
  };
}

function confidenceFor(property: PropertyCard) {
  const high = property.root_source === "rera";
  return {
    overall: high ? 0.86 : 0.54,
    label: high ? "High" : "Medium",
    components: [
      {
        dimension: "Source chain",
        score: high ? 0.9 : 0.55,
        weight: 0.35,
        explanation: high ? "RERA-rooted source chain." : "Self-reported source needs verification.",
      },
      {
        dimension: "Market support",
        score: property.price_per_sqft < 16_000 ? 0.82 : 0.64,
        weight: 0.25,
        explanation: "Benchmarked against local fixture medians.",
      },
    ],
  };
}

function fixtureKgEntityRefs(
  property: Pick<PropertyCard, "id" | "area" | "society_name" | "builder_name">,
): PropertyCard["kg_entity_refs"] {
  const propertyEntityId = `property:${slug(property.id)}`;
  const societyEntityId = `society:${slug(property.society_name)}`;
  const areaEntityId = `area:${slug(property.area)}`;
  const builderEntityId = `builder:${slug(property.builder_name)}`;

  return {
    property_entity_id: propertyEntityId,
    society_entity_id: societyEntityId,
    area_entity_id: areaEntityId,
    builder_entity_id: builderEntityId,
    source_entity_ids: [propertyEntityId, societyEntityId, areaEntityId, builderEntityId].sort(),
  };
}

function freshness(factCount: number, daysAgo: number, sourceBreakdown: Record<string, number>) {
  return {
    last_enriched: now,
    days_ago: daysAgo,
    freshness_label: daysAgo <= 45 ? "Fresh" : daysAgo <= 90 ? "Needs refresh" : "Stale",
    fact_count: factCount,
    source_breakdown: sourceBreakdown,
  };
}

function slug(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");
}
