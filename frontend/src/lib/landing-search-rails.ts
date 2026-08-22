import { societyKey } from "./property-filters.ts";
import { listingSatisfiesBudget } from "./listing-price.ts";
import type {
  SearchIntent,
  SearchResponse,
  SearchResultItem,
} from "./types.ts";

/** One row stays a glance, not a 17-page dump. ~2 pages at landing card width. */
export const LANDING_SEARCH_RAIL_CAP = 8;
export const LANDING_SEARCH_MIN_GROUP = 2;
/** A named area/price/BHK row should fill most of a screen. Thinner groups merge. */
export const LANDING_SEARCH_MIN_STANDALONE = 4;
export const LANDING_SEARCH_MAX_RAILS = 8;

export type LandingSearchRail = {
  id: string;
  label?: string;
  results: SearchResultItem[];
  siblings?: SearchResultItem[];
};

type QueryClause = {
  id: string;
  label: string;
  matches: (result: SearchResultItem) => boolean;
  requiresSplit: boolean;
};

type ResultGroup = {
  id: string;
  label: string;
  items: SearchResultItem[];
};

/**
 * Turn a search response into query-aware rows.
 *
 * Exact / strongest matches stay first. Later rows name one thing that changed
 * (a query clause, an area, a price band, a BHK). Homes keep API rank order
 * and appear in the first row they qualify for. No row is allowed to become
 * a paginated dump.
 */
export function composeLandingSearchRails(response: SearchResponse): LandingSearchRail[] {
  const focus = response.focus;
  const catalog = uniqueById([
    ...(focus?.focus_results ?? []),
    ...(focus?.sibling_configs ?? []),
    ...(focus?.more_homes ?? []),
    ...response.results,
  ]);
  if (catalog.length === 0) return [];

  const used = new Set<string>();
  const usedLabels = new Set<string>();
  const rails: LandingSearchRail[] = [];
  const societyName = focus?.society_name?.trim() ?? "";
  const namedSociety = focus?.mode === "named_society" && societyName.length > 0;
  const primary = focus?.focus_results?.length ? focus.focus_results : response.results;
  const siblings = namedSociety ? (focus?.sibling_configs ?? []) : [];
  const primaryShown = namedSociety
    ? primary
    : firstPerSociety(primary).slice(0, LANDING_SEARCH_RAIL_CAP);

  const pushRail = (
    rail: LandingSearchRail,
    claimed: SearchResultItem[],
  ) => {
    if (rails.length >= LANDING_SEARCH_MAX_RAILS) return false;
    if (claimed.length === 0 && (rail.siblings?.length ?? 0) === 0) return false;
    rails.push(rail);
    if (rail.label) usedLabels.add(normalizeText(rail.label));
    claimSocieties(used, claimed, catalog);
    return true;
  };

  const remaining = () => firstPerSociety(catalog.filter((item) => !used.has(item.id)));

  if (primaryShown.length > 0 || siblings.length > 0) {
    pushRail({
      id: namedSociety ? "exact" : "best",
      label: namedSociety ? societyName : undefined,
      results: primaryShown,
      siblings: siblings.length > 0 ? siblings : undefined,
    }, [...primaryShown, ...siblings]);
  }

  const clauses = queryClauses(response.intent, { skipArea: namedSociety });
  for (const clause of clauses) {
    if (rails.length >= LANDING_SEARCH_MAX_RAILS) break;
    const leftover = remaining();
    const hits = leftover.filter(clause.matches);
    if (hits.length === 0) continue;
    if (clause.requiresSplit && !isDistinctiveSubset(hits, leftover)) continue;
    if (usedLabels.has(normalizeText(clause.label))) continue;
    const taken = hits.slice(0, LANDING_SEARCH_RAIL_CAP);
    pushRail({ id: clause.id, label: clause.label, results: taken }, taken);
  }

  if (namedSociety) {
    const leftover = remaining();
    const sameArea = leftover.filter((item) => sharesAreaWith(item, primaryShown));
    const nearby = sameArea.length > 0 ? sameArea : leftover;
    if (nearby.length > 0) {
      const taken = nearby.slice(0, LANDING_SEARCH_RAIL_CAP);
      pushRail({
        id: "nearby",
        label: `Near ${societyName}`,
        results: taken,
      }, taken);
    }
  }

  const primaryAreas = new Set(
    primaryShown
      .map((item) => areaMarket(item.area)?.key)
      .filter((key): key is string => Boolean(key)),
  );

  while (rails.length < LANDING_SEARCH_MAX_RAILS) {
    const leftover = remaining();
    if (leftover.length === 0) break;
    if (leftover.length <= LANDING_SEARCH_RAIL_CAP) {
      pushRail({ id: "more", label: "More homes", results: leftover }, leftover);
      break;
    }

    const groups = pickLogicalGroups(leftover, {
      intent: response.intent,
      skipAreas: areasAlreadyTold(leftover, primaryShown, namedSociety, primaryAreas),
      usedLabels,
    });

    if (groups.length === 0) {
      pushRail({
        id: "more",
        label: "More homes",
        results: leftover.slice(0, LANDING_SEARCH_RAIL_CAP),
      }, leftover.slice(0, LANDING_SEARCH_RAIL_CAP));
      break;
    }

    let emitted = 0;
    let stop = false;
    for (const group of coalesceThinGroups(groups)) {
      if (rails.length >= LANDING_SEARCH_MAX_RAILS) break;
      const taken = group.items.slice(0, LANDING_SEARCH_RAIL_CAP);
      if (pushRail({ id: group.id, label: group.label, results: taken }, taken)) {
        emitted += 1;
        if (group.id === "more") stop = true;
      }
    }
    if (emitted === 0 || stop) break;
  }

  return rails;
}

export function landingSearchRailHomeCount(rails: LandingSearchRail[]): number {
  return rails.reduce(
    (count, rail) => count + rail.results.length + (rail.siblings?.length ?? 0),
    0,
  );
}

export function landingSearchRailTooLong(rails: LandingSearchRail[]): boolean {
  return rails.some((rail) => {
    if (rail.id === "exact") return false;
    return rail.results.length > LANDING_SEARCH_RAIL_CAP;
  });
}

function areasAlreadyTold(
  leftover: SearchResultItem[],
  primaryShown: SearchResultItem[],
  namedSociety: boolean,
  primaryAreas: Set<string>,
): Set<string> {
  const skip = new Set(namedSociety ? primaryAreas : []);
  const leftoverAreas = new Set(
    leftover.map((item) => areaMarket(item.area)?.key).filter((key): key is string => Boolean(key)),
  );
  if (leftoverAreas.size !== 1) return skip;
  const area = [...leftoverAreas][0];
  if (!area || !primaryAreas.has(area)) return skip;
  const primaryShare = primaryShown.filter((item) => areaMarket(item.area)?.key === area).length;
  if (primaryShare >= Math.ceil(primaryShown.length / 2)) skip.add(area);
  return skip;
}

function pickLogicalGroups(
  leftover: SearchResultItem[],
  options: {
    intent: SearchIntent;
    skipAreas: Set<string>;
    usedLabels: Set<string>;
  },
): ResultGroup[] {
  const areaGroups = areaPartitions(leftover, options.skipAreas, options.usedLabels);
  if (areaGroups.length >= 2) return areaGroups;
  if (areaGroups.length === 1 && leftover.length > LANDING_SEARCH_RAIL_CAP) return areaGroups;

  const priceGroups = pricePartitions(leftover, options.intent.budget_max, options.usedLabels);
  if (priceGroups.length >= 2) return priceGroups;

  const bhkGroups = bhkPartitions(leftover, options.usedLabels);
  if (bhkGroups.length >= 2) return bhkGroups;

  return [];
}

function areaMarket(area: string): { key: string; label: string } | null {
  const parts = area
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part && part.toLowerCase() !== "unknown");
  if (parts.length === 0) return null;
  const label = parts[parts.length - 1];
  const key = normalizeText(label);
  if (!key) return null;
  return { key, label };
}

function areaPartitions(
  items: SearchResultItem[],
  skipAreas: Set<string>,
  usedLabels: Set<string>,
): ResultGroup[] {
  const groups = new Map<string, ResultGroup>();
  for (const item of items) {
    const market = areaMarket(item.area);
    if (!market || skipAreas.has(market.key) || usedLabels.has(market.key)) continue;
    const existing = groups.get(market.key);
    if (existing) existing.items.push(item);
    else {
      groups.set(market.key, {
        id: `area-${slugify(market.label)}`,
        label: market.label,
        items: [item],
      });
    }
  }
  return [...groups.values()].filter((group) => group.items.length >= LANDING_SEARCH_MIN_GROUP);
}

function coalesceThinGroups(groups: ResultGroup[]): ResultGroup[] {
  const fat: ResultGroup[] = [];
  const thinItems: SearchResultItem[] = [];
  for (const group of groups) {
    if (group.items.length >= LANDING_SEARCH_MIN_STANDALONE) fat.push(group);
    else thinItems.push(...group.items);
  }
  if (thinItems.length === 0) return fat;
  return [
    ...fat,
    {
      id: "more",
      label: "More homes",
      items: thinItems,
    },
  ];
}

function bhkPartitions(
  items: SearchResultItem[],
  usedLabels: Set<string>,
): ResultGroup[] {
  const groups = new Map<number, ResultGroup>();
  for (const item of items) {
    if (!item.bhk || item.bhk <= 0) continue;
    const label = `${item.bhk} BHK`;
    if (usedLabels.has(normalizeText(label))) continue;
    const existing = groups.get(item.bhk);
    if (existing) existing.items.push(item);
    else {
      groups.set(item.bhk, {
        id: `bhk-${item.bhk}`,
        label,
        items: [item],
      });
    }
  }
  return [...groups.values()]
    .filter((group) => group.items.length >= LANDING_SEARCH_MIN_GROUP)
    .sort((left, right) => (left.items[0]?.bhk ?? 0) - (right.items[0]?.bhk ?? 0));
}

function pricePartitions(
  items: SearchResultItem[],
  budgetMax: number | null | undefined,
  usedLabels: Set<string>,
): ResultGroup[] {
  const priced = items.filter((item) => item.price > 0);
  if (priced.length < 4) return [];

  const min = Math.min(...priced.map((item) => item.price));
  const max = Math.max(...priced.map((item) => item.price));
  if (max < min * 1.35) return [];

  const cuts = choosePriceCuts(priced, budgetMax);
  if (cuts.length === 0) return [];

  const bands = applyPriceCuts(items, cuts)
    .filter((group) => group.items.length >= LANDING_SEARCH_MIN_GROUP)
    .filter((group) => !usedLabels.has(normalizeText(group.label)));

  return bands.length >= 2 ? bands : [];
}

const CRORE = 10_000_000;
const PRICE_CUT_CANDIDATES = [1, 1.5, 2, 2.5, 3, 3.5, 4, 5, 6, 8, 10].map(
  (crore) => crore * CRORE,
);

function choosePriceCuts(
  priced: SearchResultItem[],
  budgetMax: number | null | undefined,
): number[] {
  if (typeof budgetMax === "number" && budgetMax > 0 && splitScore(priced, [budgetMax]) > 0) {
    return [budgetMax];
  }

  const min = Math.min(...priced.map((item) => item.price));
  const max = Math.max(...priced.map((item) => item.price));
  const candidates = PRICE_CUT_CANDIDATES.filter((cut) => cut > min && cut < max);

  let bestCuts: number[] = [];
  let bestScore = 0;
  const consider = (cuts: number[]) => {
    const score = splitScore(priced, cuts);
    if (score > bestScore) {
      bestCuts = cuts;
      bestScore = score;
    }
  };

  for (const cut of candidates) consider([cut]);
  for (let i = 0; i < candidates.length; i += 1) {
    for (let j = i + 1; j < candidates.length; j += 1) {
      consider([candidates[i], candidates[j]]);
    }
  }

  return bestCuts;
}

function splitScore(items: SearchResultItem[], cuts: number[]): number {
  const groups = applyPriceCuts(items, cuts);
  if (groups.length < 2) return 0;
  if (groups.some((group) => group.items.length < LANDING_SEARCH_MIN_GROUP)) return 0;
  const sizes = groups.map((group) => group.items.length);
  const total = sizes.reduce((sum, size) => sum + size, 0);
  const largest = Math.max(...sizes);
  if (largest / total > 0.8) return 0;
  const evenness = 1 - (largest / total - 1 / groups.length);
  return groups.length * 10 + evenness * 5 + Math.min(...sizes);
}

function applyPriceCuts(items: SearchResultItem[], cuts: number[]): ResultGroup[] {
  const sortedCuts = [...cuts].sort((left, right) => left - right);
  const bands: ResultGroup[] = [];
  let previous = 0;

  for (const [index, cut] of sortedCuts.entries()) {
    bands.push({
      id: `price-${index}`,
      label: `Under ${formatBudgetInr(cut)}`,
      items: [],
    });
    previous = cut;
  }
  bands.push({
    id: `price-${sortedCuts.length}`,
    label: `${formatBudgetInr(previous)} and above`,
    items: [],
  });

  for (const item of items) {
    if (item.price <= 0) continue;
    const index = sortedCuts.findIndex((cut) => item.price <= cut);
    const band = bands[index === -1 ? bands.length - 1 : index];
    band?.items.push(item);
  }

  return bands.filter((band) => band.items.length > 0);
}

function queryClauses(
  intent: SearchIntent,
  options: { skipArea: boolean },
): QueryClause[] {
  const clauses: QueryClause[] = [];

  if (
    (typeof intent.budget_min === "number" && intent.budget_min > 0) ||
    (typeof intent.budget_max === "number" && intent.budget_max > 0)
  ) {
    const min = intent.budget_min;
    const max = intent.budget_max;
    clauses.push({
      id: "budget",
      label: budgetClauseLabel(min, max),
      matches: (result) =>
        result.price > 0 &&
        listingSatisfiesBudget(result, min, max),
      requiresSplit: false,
    });
  }

  for (const preference of preferenceTexts(intent)) {
    const label = compactRailLabel(preference);
    if (!label) continue;
    clauses.push({
      id: `pref-${slugify(label)}`,
      label,
      matches: (result) => resultMentionsPreference(result, preference),
      requiresSplit: false,
    });
  }

  const bhks = requestedBhks(intent);
  if (bhks.length > 0) {
    clauses.push({
      id: `bhk-${bhks.join("-")}`,
      label:
        bhks.length === 1
          ? `${bhks[0]} BHK`
          : `${bhks.join(" or ")} BHK`,
      matches: (result) => bhks.includes(result.bhk),
      requiresSplit: true,
    });
  }

  const areas = requestedAreas(intent);
  if (!options.skipArea && areas.length > 0) {
    clauses.push({
      id: "area",
      label: areas.length === 1 ? areas[0] : areas.join(" or "),
      matches: (result) => areas.some((area) => areaOverlaps(result.area, area)),
      requiresSplit: true,
    });
  }

  return clauses;
}

function preferenceTexts(intent: SearchIntent): string[] {
  const signals = intent.positive_preferences ?? [];
  if (signals.length > 0) {
    return uniqueTexts(signals.map((signal) => signal.raw_text));
  }
  return uniqueTexts(intent.preferences ?? []);
}

function resultMentionsPreference(result: SearchResultItem, preference: string): boolean {
  if (textOverlaps(result.match_reason, preference)) return true;
  for (const reason of result.match_explanation?.reasons ?? []) {
    if (textOverlaps(reason.preference, preference) || textOverlaps(reason.display, preference)) {
      return true;
    }
  }
  for (const coverage of result.match_explanation?.preference_coverage ?? []) {
    if (coverage.status === "no_data") continue;
    if (textOverlaps(coverage.preference, preference)) return true;
  }
  return false;
}

function sharesAreaWith(result: SearchResultItem, primary: SearchResultItem[]): boolean {
  const areas = new Set(
    primary
      .map((item) => item.area.trim().toLowerCase())
      .filter(Boolean),
  );
  if (areas.size === 0) return false;
  return areas.has(result.area.trim().toLowerCase());
}

function isDistinctiveSubset(
  hits: SearchResultItem[],
  leftover: SearchResultItem[],
): boolean {
  return hits.length > 0 && hits.length < leftover.length;
}

function firstPerSociety(items: SearchResultItem[]): SearchResultItem[] {
  const seen = new Set<string>();
  const out: SearchResultItem[] = [];
  for (const item of items) {
    const key = societyKey(item);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  return out;
}

function claimSocieties(
  used: Set<string>,
  claimed: SearchResultItem[],
  catalog: SearchResultItem[],
) {
  const societies = new Set(claimed.map((item) => societyKey(item)));
  for (const item of catalog) {
    if (societies.has(societyKey(item))) used.add(item.id);
  }
}

function uniqueById(items: SearchResultItem[]): SearchResultItem[] {
  const seen = new Set<string>();
  const out: SearchResultItem[] = [];
  for (const item of items) {
    if (seen.has(item.id)) continue;
    seen.add(item.id);
    out.push(item);
  }
  return out;
}

function uniqueTexts(values: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const value of values) {
    const trimmed = value.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(trimmed);
  }
  return out;
}

function requestedBhks(intent: SearchIntent): number[] {
  if (intent.bhks && intent.bhks.length > 0) {
    return intent.bhks.filter((value) => value > 0);
  }
  return typeof intent.bhk === "number" && intent.bhk > 0 ? [intent.bhk] : [];
}

function requestedAreas(intent: SearchIntent): string[] {
  if (intent.areas && intent.areas.length > 0) {
    return intent.areas.map((area) => area.trim()).filter(Boolean);
  }
  const area = intent.area?.trim() ?? "";
  return area ? [area] : [];
}

function areaOverlaps(left: string, right: string): boolean {
  const a = left.trim().toLowerCase();
  const b = right.trim().toLowerCase();
  if (!a || !b) return false;
  return a === b || a.includes(b) || b.includes(a);
}

function textOverlaps(left: string | undefined, right: string | undefined): boolean {
  const a = normalizeText(left);
  const b = normalizeText(right);
  if (!a || !b) return false;
  if (a === b) return true;
  if (a.length >= 4 && b.includes(a)) return true;
  if (b.length >= 4 && a.includes(b)) return true;
  return false;
}

function normalizeText(value: string | undefined): string {
  return (value ?? "").trim().toLowerCase().replace(/\s+/g, " ");
}

function compactRailLabel(value: string): string {
  const cleaned = value.replace(/\s+/g, " ").trim();
  if (!cleaned) return "";
  const words = cleaned.split(" ");
  const short = words.length > 4 ? words.slice(0, 4).join(" ") : cleaned;
  return short.replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "clause";
}

function budgetClauseLabel(
  min: number | null | undefined,
  max: number | null | undefined,
): string {
  if (typeof min === "number" && min > 0 && typeof max === "number" && max > 0) {
    return `${formatBudgetInr(min)}–${formatBudgetInr(max)}`;
  }
  if (typeof min === "number" && min > 0) {
    return `From ${formatBudgetInr(min)}`;
  }
  if (typeof max === "number" && max > 0) {
    return `Under ${formatBudgetInr(max)}`;
  }
  return "Budget";
}

export function formatBudgetInr(value: number): string {
  if (value >= 10_000_000) {
    const crore = value / 10_000_000;
    return `₹${trimNumber(crore)}Cr`;
  }
  if (value >= 100_000) {
    const lakh = value / 100_000;
    return `₹${trimNumber(lakh)}L`;
  }
  return `₹${value.toLocaleString("en-IN")}`;
}

function trimNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1).replace(/\.0$/, "");
}
