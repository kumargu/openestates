import type { JourneyExpression, JourneyIntent, JourneyPredicate, SearchJourneyEnvelope, SearchResponse, SearchRevisionTarget } from "./types.ts";
import journeyPolicy from "../../../app/config/ui/search-journey.json" with { type: "json" };

export const journeyCopy = journeyPolicy.copy;
export const journeySuggestions = journeyPolicy.suggestions;
export const journeyNavigationCache = journeyPolicy.navigationCache;

/** Display typed values only; this never supplies parser or evaluator input. */
function predicateTargetLabel(predicate: JourneyPredicate): string {
  const labels = journeyPolicy.targetLabels;
  const units: Record<string, { divisor: number; prefix: string; suffix: string }> = labels.units;
  const unit = predicate.unit ? units[predicate.unit] : undefined;
  const format = (value: unknown): string => {
    if (typeof value === "number") return unit
      ? `${unit.prefix}${(value / unit.divisor).toLocaleString(labels.numberLocale, { maximumFractionDigits: 2 })}${unit.suffix}`
      : `${value.toLocaleString(labels.numberLocale)}${predicate.unit ? ` ${predicate.unit}` : ""}`;
    return typeof value === "string" ? value : "";
  };
  const value = predicate.value;
  let display = predicate.resolvedLabel ?? format(value);
  if (value && typeof value === "object") {
    if ("min" in value || "max" in value) {
      const range = value as { min?: number; max?: number };
      display = [range.min, range.max].filter((bound) => bound != null).map(format).join("–");
    } else if ("distance" in value) {
      display = [display, format(value.distance)].filter(Boolean).join(" · ");
    }
  }
  const operators: Record<string, string> = labels.operators;
  return [predicate.polarity === "negative" ? labels.negative : "", predicate.label,
    display ? operators[predicate.operator] ?? "" : "", display].filter(Boolean).join(" ");
}

export function journeyEditTargets(intent: JourneyIntent): Array<{ label: string; target: SearchRevisionTarget }> {
  return intent.branches.flatMap((branch, index) => {
    const prefix = intent.branches.length > 1 ? `${journeyPolicy.targetLabels.option} ${index + 1}: ` : "";
    const predicates = (expression: JourneyExpression): Array<{ label: string; target: SearchRevisionTarget }> => {
      if (expression.kind === "predicate") return [{
        label: `${prefix}${predicateTargetLabel(expression.predicate)}`,
        target: { kind: "predicate", branchId: branch.id, predicateId: expression.predicate.id },
      }];
      if (expression.kind === "not") return predicates(expression.clause);
      return expression.clauses.flatMap(predicates);
    };
    return [
      ...(prefix ? [{ label: `${journeyPolicy.targetLabels.option} ${index + 1}`, target: { kind: "branch" as const, branchId: branch.id } }] : []),
      ...predicates(branch.constraints),
      ...branch.preferences.map((preference) => ({
        label: `${prefix}${journeyPolicy.targetLabels.preferred} ${preference.polarity === "negative" ? `${journeyPolicy.targetLabels.negative} ` : ""}${preference.label}`,
        target: { kind: "predicate" as const, branchId: branch.id, predicateId: preference.id },
      })),
    ];
  });
}

/** One projection at the API boundary, not a second search or proof engine. */
export function projectSearchJourney(journey: SearchJourneyEnvelope, previous?: SearchResponse): SearchResponse {
  if (journey.contractVersion !== 1 || !journey.active?.revision?.stateToken) {
    throw new Error("Unsupported search journey response");
  }
  const results = journey.active.results;
  if (results.kind === "retained") {
    if (!previous?.journey || previous.journey.active.revision.resultFingerprint !== results.resultFingerprint
      || JSON.stringify(previous.orderedResultIds) !== JSON.stringify(results.orderedResultIds)) {
      throw new Error("Previous search results are unavailable");
    }
    return { ...previous, query: journey.active.buyerBrief, journey, runtimeVersion: journey.runtimeVersion };
  }
  return {
    journey,
    query: journey.active.buyerBrief,
    runtimeVersion: journey.runtimeVersion,
    resultSets: results.resultSets,
    orderedResultIds: results.orderedResultIds,
    totalMatches: results.totalMatches,
    state: results.state,
    searchGuidance: results.guidance,
  };
}

const STORAGE_KEY = "openestates:search-journeys:v1";
const ACTIVE_KEY = "openestates:active-search:v1";
export type SavedJourney = { query: string; response: SearchResponse; selectedId?: string; previousId?: string };
export type SearchCheckpoint = { token: string; ids: string[] };

/** The fragment is never sent in the page request or HTTP referrer. Only the
 * API validates the signed state; this transport must not reinterpret intent. */
export function journeyUrl(query: string, response: SearchResponse): string {
  const revision = response.journey?.active.revision;
  const params = new URLSearchParams({ q: query });
  if (!revision) return `/?${params}`;
  params.set("journey", revision.id);
  const checkpoint: SearchCheckpoint = { token: revision.stateToken, ids: response.orderedResultIds };
  return `/?${params}#search=${encodeURIComponent(JSON.stringify(checkpoint))}`;
}

export function readSearchCheckpoint(hash: string): SearchCheckpoint | undefined {
  if (!hash.startsWith("#search=")) return undefined;
  try {
    const value = JSON.parse(decodeURIComponent(hash.slice("#search=".length)));
    if (typeof value?.token !== "string" || !value.token
      || !Array.isArray(value.ids) || !value.ids.every((id: unknown) => typeof id === "string")) return undefined;
    return { token: value.token, ids: value.ids };
  } catch { return undefined; }
}

function savedJourneys(): SavedJourney[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    if (!Array.isArray(saved)) return [];
    return saved.filter((entry): entry is SavedJourney =>
      typeof entry?.query === "string" && entry?.response?.journey?.contractVersion === 1
      && typeof entry.response.journey.active?.revision?.stateToken === "string"
      && Array.isArray(entry.response.orderedResultIds) && Array.isArray(entry.response.resultSets));
  } catch { return []; }
}

export function readSavedJourney(id: string | null): SavedJourney | undefined {
  return savedJourneys().find((entry) => entry.response.journey?.active.revision.id === id);
}

export function readActiveJourney(): SavedJourney | undefined {
  try { return readSavedJourney(localStorage.getItem(ACTIVE_KEY)); } catch { return undefined; }
}

export function clearActiveJourney(): void {
  try { localStorage.removeItem(ACTIVE_KEY); } catch { /* Storage is optional. */ }
}

/** Follow explicit accepted-edit ancestry; a resume is not a new buyer decision. */
export function journeyHistory(response: SearchResponse | null): SavedJourney[] {
  const history: SavedJourney[] = [];
  const seen = new Set<string>();
  const savedById = new Map(savedJourneys().map((saved) => [saved.response.journey?.active.revision.id, saved]));
  let id = response?.journey?.active.revision.id;
  while (id && !seen.has(id)) {
    seen.add(id);
    const saved = savedById.get(id);
    if (!saved) break;
    history.push(saved);
    id = saved.previousId;
  }
  return history;
}

/** A view over the current backend order, never a second ranking pass. */
export function catalogAddedIds(response: SearchResponse | null): string[] {
  const journey = response?.journey;
  if (!journey?.attempt.catalogRebased || journey.active.results.kind !== "current") return [];
  const added = new Set(journey.attempt.catalogDelta?.added ?? []);
  return journey.active.results.orderedResultIds.filter((id) => added.has(id));
}

export function saveJourney(query: string, response: SearchResponse, selectedId?: string, previousId?: string): void {
  const journey = response.journey;
  const id = journey?.active.revision.id;
  if (!journey || !id) return;
  const saved = savedJourneys().filter((entry) => {
    if (entry.response.journey?.active.revision.id === id) return false;
    // Resume refreshes the same buyer decision against a newer snapshot. It is
    // not another history entry and must not duplicate a large card payload.
    if (journey.attempt.kind !== "resume") return true;
    return entry.query !== query
      || entry.previousId !== previousId
      || entry.response.journey?.active.buyerBrief !== journey.active.buyerBrief;
  });
  try {
    const retained = [...saved, { query, response, selectedId, previousId }]
      .slice(-journeyPolicy.historyLimit);
    let serialized = JSON.stringify(retained);
    while (retained.length > 1 && serialized.length > journeyPolicy.persistenceByteLimit) {
      retained.shift();
      serialized = JSON.stringify(retained);
    }
    localStorage.setItem(STORAGE_KEY, serialized);
    localStorage.setItem(ACTIVE_KEY, id);
  } catch { /* Search remains usable if browser storage is unavailable. */ }
}

export function selectJourneyProperty(returnUrl: string, propertyId: string): void {
  const id = new URL(returnUrl, "https://local.invalid").searchParams.get("journey");
  const saved = readSavedJourney(id);
  if (saved?.response.orderedResultIds.includes(propertyId)) saveJourney(saved.query, saved.response, propertyId, saved.previousId);
}
