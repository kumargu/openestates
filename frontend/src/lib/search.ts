import type { PropertyCard } from "./types.ts";

export interface SearchIntent {
  query: string;
  area?: string;
  bhk?: number;
  budgetMax?: number;
  preferences: string[];
}

const AREA_ALIASES: Record<string, string> = {
  whitefield: "Whitefield",
  sarjapur: "Sarjapur Road",
  "sarjapur road": "Sarjapur Road",
  hsr: "HSR Layout",
  "hsr layout": "HSR Layout",
  mahadevapura: "Mahadevapura",
  "electronic city": "Electronic City",
  "e-city": "Electronic City",
  ecity: "Electronic City",
};

const PREFERENCE_PATTERNS: [RegExp, string][] = [
  [/near\s*metro|metro\s*access|metro\s*nearby/i, "metro access"],
  [/good\s*society|strong\s*society|quality\s*society/i, "good society"],
  [/low\s*noise|quiet|peaceful/i, "low noise"],
  [/strong\s*docs?|safe\s*docs?|document/i, "strong documents"],
  [/sunlight|sun\s*light|bright/i, "good sunlight"],
  [/value|below\s*median|good\s*deal|affordable|budget/i, "value pick"],
  [/premium|luxury|high\s*end/i, "premium"],
];

export function parseSearch(raw: string): SearchIntent {
  const query = raw.trim();
  const lower = query.toLowerCase();
  const intent: SearchIntent = { query, preferences: [] };

  // Extract BHK
  const bhkMatch = lower.match(/(\d)\s*bhk/);
  if (bhkMatch) intent.bhk = parseInt(bhkMatch[1]);

  // Extract budget
  const crMatch = lower.match(/(?:under|below|max|upto|up\s*to|around|within)\s*(\d+\.?\d*)\s*cr/i);
  if (crMatch) intent.budgetMax = parseFloat(crMatch[1]) * 10_000_000;
  const lMatch = lower.match(/(?:under|below|max|upto|up\s*to|around|within)\s*(\d+\.?\d*)\s*(?:l|lakh|lakhs)/i);
  if (lMatch) intent.budgetMax = parseFloat(lMatch[1]) * 100_000;

  // Extract area
  for (const [alias, name] of Object.entries(AREA_ALIASES)) {
    if (lower.includes(alias)) {
      intent.area = name;
      break;
    }
  }

  // Extract preferences
  for (const [pattern, label] of PREFERENCE_PATTERNS) {
    if (pattern.test(lower)) {
      intent.preferences.push(label);
    }
  }

  return intent;
}

export function filterProperties(properties: PropertyCard[], intent: SearchIntent): PropertyCard[] {
  return properties.filter((p) => {
    if (intent.area && !p.area.toLowerCase().includes(intent.area.toLowerCase())) return false;
    if (intent.bhk && p.bhk !== intent.bhk) return false;
    if (intent.budgetMax && p.price > intent.budgetMax) return false;
    return true;
  });
}

export type MatchLabel = "Strong match" | "Good match" | "Value pick" | "Premium match";

export interface MatchResult {
  label: MatchLabel;
  reason: string;
}

export function computeMatch(property: PropertyCard, intent: SearchIntent): MatchResult {
  const reasons: string[] = [];
  let score = 0;

  // Area match
  if (intent.area && property.area.toLowerCase().includes(intent.area.toLowerCase())) {
    reasons.push(intent.area);
    score += 2;
  }

  // BHK match
  if (intent.bhk && property.bhk === intent.bhk) {
    reasons.push(`${intent.bhk} BHK`);
    score += 1;
  }

  // Budget match
  if (intent.budgetMax && property.price <= intent.budgetMax) {
    reasons.push("within budget");
    score += 1;
  }

  // Tag-based signals
  const tags = property.transparency_tags.map((t) => t.toLowerCase());

  if (intent.preferences.includes("metro access") && tags.some((t) => t.includes("metro"))) {
    reasons.push("metro nearby");
    score += 1;
  }
  if (intent.preferences.includes("good society") && tags.some((t) => t.includes("society") || t.includes("premium"))) {
    reasons.push("strong society");
    score += 1;
  }
  if (intent.preferences.includes("strong documents") && tags.some((t) => t.includes("doc"))) {
    reasons.push("strong docs");
    score += 1;
  }
  if (intent.preferences.includes("value pick") && tags.some((t) => t.includes("below") || t.includes("value"))) {
    reasons.push("below area median");
    score += 1;
  }
  if (intent.preferences.includes("good sunlight") && tags.some((t) => t.includes("sunlight"))) {
    reasons.push("good sunlight");
    score += 1;
  }

  // Fallback reasons from tags if no search-specific reasons
  if (reasons.length === 0) {
    if (tags.some((t) => t.includes("below"))) reasons.push("below area median");
    if (tags.some((t) => t.includes("metro"))) reasons.push("metro access");
    if (tags.some((t) => t.includes("doc"))) reasons.push("strong docs");
    if (tags.some((t) => t.includes("society") || t.includes("premium"))) reasons.push("quality society");
  }

  // Determine label
  let label: MatchLabel;
  if (tags.some((t) => t.includes("premium"))) {
    label = "Premium match";
  } else if (tags.some((t) => t.includes("below") || t.includes("value"))) {
    label = "Value pick";
  } else if (score >= 3) {
    label = "Strong match";
  } else {
    label = "Good match";
  }

  const reasonText = reasons.length > 0
    ? `Matches ${reasons.join(", ")}`
    : "Solid transparency signals across key dimensions";

  return { label, reason: reasonText };
}

export function formatSearchSummary(intent: SearchIntent): string {
  const parts: string[] = [];
  if (intent.area) parts.push(intent.area);
  if (intent.bhk) parts.push(`${intent.bhk} BHK`);
  if (intent.budgetMax) {
    const cr = intent.budgetMax / 10_000_000;
    parts.push(cr >= 1 ? `under ${cr} Cr` : `under ${(intent.budgetMax / 100_000).toFixed(0)}L`);
  }
  parts.push(...intent.preferences);

  if (parts.length === 0) return "Showing all properties ranked by transparency signals";
  return `Ranking based on ${parts.join(", ")}`;
}
