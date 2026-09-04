import type {
  ProofFocus,
  SearchResultItem,
  SearchRuntimeVersion,
} from "./types.ts";

export type NavigationMode = "landing" | "discovery" | "property-context" | "workspace";

export type DiscoveryContext = {
  version: 1;
  url: string;
  scrollY: number;
  resultCount?: number;
};

const DISCOVERY_STORAGE_KEY = "openestates:last-discovery:v1";
const DISCOVERY_RETURN_INTENT_KEY = "openestates:discovery-return-intent:v1";
const DISCOVERY_MAP_CONTEXT_KEY = "openestates:discovery-map-context:v2";
const DISCOVERY_MAP_CONTEXT_LATEST_KEY = "openestates:discovery-map-context:latest-v2";
const PROPERTY_SEARCH_CONTEXT_KEY = "openestates:property-search-context:v1";
const SEARCH_SPAN_INDEX_KEY = "openestates:search-span-index:v1";
const SEARCH_JOURNEY_PREFERENCE_KEY = "openestates:search-journey-preferences:v1";
const DISCOVERY_MAP_CANDIDATE_LIMIT = 24;
const SEARCH_SPAN_HISTORY_LIMIT = 6;
export const SEARCH_SPAN_TTL_MS = 4 * 60 * 60 * 1_000;
export const SEARCH_JOURNEY_PREFERENCES_CHANGED_EVENT =
  "openestates:search-journey-preferences-changed";
const SEARCH_SPAN_URL_PARAMS = ["context", "qf", "searchHome"] as const;

export type PropertySearchResult = {
  propertyId: string;
  title: string;
  societyName: string;
  area: string;
  price?: number;
  bhk?: number;
  sqft?: number;
  stateDisplay?: string;
  proofFocus?: ProofFocus;
};

/** Buyer-journey span; backend request traces may later correlate as child spans. */
export type StoredPropertySearchContext = {
  version: 1;
  id: string;
  queryFingerprint: string;
  queryLabel: string;
  returnUrl: string;
  returnScrollY: number;
  /** React Router history index for an exact return to the parent result page. */
  returnHistoryIndex?: number;
  createdAt: number;
  runtimeVersion: SearchRuntimeVersion;
  results: PropertySearchResult[];
};

export type PropertySearchContext = StoredPropertySearchContext & {
  selectedId: string;
};

export type SearchJourneyCursor = {
  position: number;
  total: number;
  previousResult?: PropertySearchResult;
  selectedResult: PropertySearchResult;
  nextResult?: PropertySearchResult;
};

export function searchJourneyCursor(
  context: PropertySearchContext,
  notForMeIds: ReadonlySet<string> = new Set(),
): SearchJourneyCursor | null {
  const selectedIndex = context.results.findIndex((result) =>
    result.propertyId === context.selectedId
  );
  const selectedResult = context.results[selectedIndex];
  if (!selectedResult) return null;
  return {
    position: selectedIndex + 1,
    total: context.results.length,
    previousResult: context.results
      .slice(0, selectedIndex)
      .reverse()
      .find((result) => !notForMeIds.has(result.propertyId)),
    selectedResult,
    nextResult: context.results
      .slice(selectedIndex + 1)
      .find((result) => !notForMeIds.has(result.propertyId)),
  };
}

export type SearchSpanReference = {
  id: string;
  queryFingerprint: string;
  selectedId?: string;
};

export function hrefWithSearchSpan(
  href: string,
  span: SearchSpanReference | null | undefined,
): string {
  if (!span) return href;
  const hashIndex = href.indexOf("#");
  const hash = hashIndex >= 0 ? href.slice(hashIndex) : "";
  const hrefWithoutHash = hashIndex >= 0 ? href.slice(0, hashIndex) : href;
  const [pathname, search = ""] = hrefWithoutHash.split("?", 2);
  const params = new URLSearchParams(search);
  params.set("context", span.id);
  params.set("qf", span.queryFingerprint);
  if (span.selectedId) params.set("searchHome", span.selectedId);
  else params.delete("searchHome");
  return `${pathname}?${params.toString()}${hash}`;
}

export function searchSpanReferenceFromUrl(search: string): SearchSpanReference | null {
  const params = new URLSearchParams(search);
  const id = params.get("context")?.trim();
  const queryFingerprintValue = params.get("qf")?.trim();
  if (!id || !queryFingerprintValue || !/^q[0-9a-z]+$/.test(queryFingerprintValue)) {
    return null;
  }
  const selectedId = params.get("searchHome")?.trim() || undefined;
  return { id, queryFingerprint: queryFingerprintValue, selectedId };
}

export function hasSearchSpanUrlParams(search: string): boolean {
  const params = new URLSearchParams(search);
  return SEARCH_SPAN_URL_PARAMS.some((name) => params.has(name));
}

export function stripSearchSpanUrlParams(search: string): string {
  const params = new URLSearchParams(search);
  for (const name of SEARCH_SPAN_URL_PARAMS) params.delete(name);
  const value = params.toString();
  return value ? `?${value}` : "";
}

export type DiscoveryMapCandidate = {
  propertyId: string;
  propertyIds: string[];
  societyId: string;
  societyName: string;
  rank: number;
  preview: {
    area: string;
    bhk: number;
    price: number;
    title: string;
  };
  proofFocus?: ProofFocus;
};

export type DiscoveryMapContext = {
  version: 2;
  id: string;
  queryFingerprint: string;
  createdAt: number;
  propertyIds: string[];
  candidates: DiscoveryMapCandidate[];
};

function normalizeQuery(query: string): string {
  return query.normalize("NFKC").trim().toLocaleLowerCase("en-IN").replace(/\s+/g, " ");
}

export function queryFingerprint(query: string): string | null {
  const normalized = normalizeQuery(query);
  if (!normalized) return null;
  let hash = 2_166_136_261;
  for (const character of normalized) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16_777_619);
  }
  return `q${(hash >>> 0).toString(36)}`;
}

function newContextId(): string {
  const randomUuid = globalThis.crypto?.randomUUID?.();
  if (randomUuid) return randomUuid;
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

function contextStorageKey(id: string): string {
  return `${DISCOVERY_MAP_CONTEXT_KEY}:${id}`;
}

function propertySearchContextStorageKey(id: string): string {
  return `${PROPERTY_SEARCH_CONTEXT_KEY}:${id}`;
}

function searchJourneyPreferenceStorageKey(id: string): string {
  return `${SEARCH_JOURNEY_PREFERENCE_KEY}:${id}`;
}

type SearchSpanIndexEntry = {
  id: string;
  touchedAt: number;
};

function readSearchSpanIndex(): SearchSpanIndexEntry[] {
  if (typeof window === "undefined") return [];
  try {
    const value: unknown = JSON.parse(
      window.sessionStorage.getItem(SEARCH_SPAN_INDEX_KEY) ?? "[]",
    );
    if (!Array.isArray(value)) return [];
    return value.filter((entry): entry is SearchSpanIndexEntry => Boolean(
      entry
      && typeof entry.id === "string"
      && entry.id.trim()
      && typeof entry.touchedAt === "number"
      && Number.isFinite(entry.touchedAt),
    ));
  } catch {
    return [];
  }
}

function removeSearchSpanStorage(id: string): void {
  window.sessionStorage.removeItem(contextStorageKey(id));
  window.sessionStorage.removeItem(propertySearchContextStorageKey(id));
  window.sessionStorage.removeItem(searchJourneyPreferenceStorageKey(id));
}

function forgetSearchSpan(id: string): void {
  removeSearchSpanStorage(id);
  const retained = readSearchSpanIndex().filter((entry) => entry.id !== id);
  if (retained.length > 0) {
    window.sessionStorage.setItem(SEARCH_SPAN_INDEX_KEY, JSON.stringify(retained));
  } else {
    window.sessionStorage.removeItem(SEARCH_SPAN_INDEX_KEY);
  }
  if (window.sessionStorage.getItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY) === id) {
    const previous = retained.at(-1)?.id;
    if (previous) window.sessionStorage.setItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY, previous);
    else window.sessionStorage.removeItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY);
  }
}

function registerSearchSpan(id: string, now: number): void {
  const retained = readSearchSpanIndex()
    .filter((entry) => entry.id !== id && now - entry.touchedAt <= SEARCH_SPAN_TTL_MS)
    .concat({ id, touchedAt: now })
    .slice(-SEARCH_SPAN_HISTORY_LIMIT);
  const retainedIds = new Set(retained.map((entry) => entry.id));
  for (const entry of readSearchSpanIndex()) {
    if (!retainedIds.has(entry.id)) removeSearchSpanStorage(entry.id);
  }
  window.sessionStorage.setItem(SEARCH_SPAN_INDEX_KEY, JSON.stringify(retained));
}

export function navigationMode(pathname: string, search = ""): NavigationMode {
  if (pathname.startsWith("/workspace") || pathname === "/notebook" || pathname === "/compare") {
    return "workspace";
  }
  if (/^\/property\/[^/]+(?:\/rera)?$/.test(pathname)) return "property-context";
  if (pathname === "/" && new URLSearchParams(search).get("q")?.trim()) return "discovery";
  return "landing";
}

export function readDiscoveryContext(): DiscoveryContext | null {
  if (typeof window === "undefined") return null;
  try {
    const parsed: unknown = JSON.parse(window.sessionStorage.getItem(DISCOVERY_STORAGE_KEY) ?? "null");
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Partial<DiscoveryContext>;
    if (
      candidate.version !== 1
      || typeof candidate.url !== "string"
      || !candidate.url.startsWith("/?")
      || !new URLSearchParams(candidate.url.slice(2)).get("q")?.trim()
    ) return null;
    return {
      version: 1,
      url: candidate.url,
      scrollY: typeof candidate.scrollY === "number" && candidate.scrollY >= 0
        ? candidate.scrollY
        : 0,
      resultCount: typeof candidate.resultCount === "number"
        && Number.isInteger(candidate.resultCount)
        && candidate.resultCount >= 0
        ? candidate.resultCount
        : undefined,
    };
  } catch {
    return null;
  }
}

export function writeDiscoveryContext(url: string, scrollY = 0): void {
  if (typeof window === "undefined") return;
  const params = url.startsWith("/?") ? new URLSearchParams(url.slice(2)) : null;
  if (!params?.get("q")?.trim()) return;
  const previous = readDiscoveryContext();
  const context: DiscoveryContext = {
    version: 1,
    url,
    scrollY: Math.max(0, Math.round(scrollY)),
    resultCount: previous?.url === url ? previous.resultCount : undefined,
  };
  window.sessionStorage.setItem(DISCOVERY_STORAGE_KEY, JSON.stringify(context));
}

export function writeDiscoveryResultCount(url: string, resultCount: number): void {
  if (typeof window === "undefined" || !Number.isInteger(resultCount) || resultCount < 0) return;
  const params = url.startsWith("/?") ? new URLSearchParams(url.slice(2)) : null;
  if (!params?.get("q")?.trim()) return;
  const previous = readDiscoveryContext();
  const context: DiscoveryContext = {
    version: 1,
    url,
    scrollY: previous?.url === url ? previous.scrollY : Math.max(0, Math.round(window.scrollY)),
    resultCount,
  };
  window.sessionStorage.setItem(DISCOVERY_STORAGE_KEY, JSON.stringify(context));
}

export function discoveryReturnHref(): string {
  return readDiscoveryContext()?.url ?? "/";
}

export function propertyExploreHref(
  area: string,
  rememberedHref = discoveryReturnHref(),
): string {
  if (rememberedHref !== "/") return rememberedHref;
  const query = area.trim();
  if (!query) return "/";
  const params = new URLSearchParams({ q: query });
  return `/?${params.toString()}`;
}

export function requestDiscoveryReturn(url: string): void {
  if (typeof window === "undefined") return;
  const context = readDiscoveryContext();
  if (!context || context.url !== url) return;
  window.sessionStorage.setItem(DISCOVERY_RETURN_INTENT_KEY, JSON.stringify(context));
}

export function captureDiscoveryDeparture(url: string, scrollY: number): void {
  writeDiscoveryContext(url, scrollY);
  if (typeof window === "undefined") return;
  const contextId = window.sessionStorage.getItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY);
  const searchContext = readPropertySearchContext(contextId);
  if (contextId && searchContext?.returnUrl === url) {
    window.sessionStorage.setItem(
      propertySearchContextStorageKey(contextId),
      JSON.stringify({
        ...searchContext,
        returnScrollY: Math.max(0, Math.round(scrollY)),
      }),
    );
  }
  requestDiscoveryReturn(url);
}

export function clearDiscoveryContext(): void {
  if (typeof window === "undefined") return;
  window.sessionStorage.removeItem(DISCOVERY_STORAGE_KEY);
  window.sessionStorage.removeItem(DISCOVERY_RETURN_INTENT_KEY);
  for (const entry of readSearchSpanIndex()) removeSearchSpanStorage(entry.id);
  window.sessionStorage.removeItem(SEARCH_SPAN_INDEX_KEY);
  window.sessionStorage.removeItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY);
}

export function writeDiscoveryMapContext(
  query: string,
  results: SearchResultItem[],
  focusOrOptions: ((result: SearchResultItem) => ProofFocus | undefined) | {
    id?: string;
    now?: number;
  } = (result) => result.proof_focuses?.[0],
  options: { id?: string; now?: number } = {},
): string | null {
  const fingerprint = queryFingerprint(query);
  if (typeof window === "undefined" || !fingerprint) return null;
  const focusForResult = typeof focusOrOptions === "function"
    ? focusOrOptions
    : (result: SearchResultItem) => result.proof_focuses?.[0];
  const contextOptions = typeof focusOrOptions === "function" ? options : focusOrOptions;
  const societies = new Map<string, DiscoveryMapCandidate>();
  const candidates: DiscoveryMapCandidate[] = [];
  const propertyIds = [...new Set(
    results.map((result) => result.id.trim()).filter(Boolean),
  )];
  for (const [rank, result] of results.entries()) {
    const societyName = result.society_name.trim() || result.title.trim();
    const societyId = result.kg_entity_refs?.society_entity_id?.trim();
    if (!societyName || !societyId) continue;
    const existing = societies.get(societyId);
    if (existing) {
      if (!existing.propertyIds.includes(result.id)) existing.propertyIds.push(result.id);
      continue;
    }
    const candidate: DiscoveryMapCandidate = {
      propertyId: result.id,
      propertyIds: [result.id],
      societyId,
      societyName,
      rank,
      preview: {
        area: result.area,
        bhk: result.bhk,
        price: result.price,
        title: result.title,
      },
      proofFocus: focusForResult(result),
    };
    societies.set(societyId, candidate);
    candidates.push(candidate);
    if (candidates.length === DISCOVERY_MAP_CANDIDATE_LIMIT) break;
  }
  const now = contextOptions.now ?? Date.now();
  try {
    const previousId = window.sessionStorage.getItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY);
    const previous = readDiscoveryMapContext(previousId, now);
    const id = contextOptions.id
      ?? (previous?.queryFingerprint === fingerprint ? previous.id : newContextId());
    const context: DiscoveryMapContext = {
      version: 2,
      id,
      queryFingerprint: fingerprint,
      createdAt: now,
      propertyIds,
      candidates,
    };
    window.sessionStorage.setItem(contextStorageKey(id), JSON.stringify(context));
    window.sessionStorage.setItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY, id);
    registerSearchSpan(id, now);
    return id;
  } catch {
    return null;
  }
}

function knownPositiveNumber(value: number | null | undefined): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value > 0
    ? value
    : undefined;
}

function isProofFocus(value: unknown): value is ProofFocus {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ProofFocus>;
  return typeof candidate.surfaceId === "string"
    && typeof candidate.layerId === "string"
    && typeof candidate.factKey === "string"
    && typeof candidate.reason === "string";
}

function isSearchRuntimeVersion(value: unknown): value is SearchRuntimeVersion {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<SearchRuntimeVersion>;
  return typeof candidate.servingBundleVersion === "string"
    && Boolean(candidate.servingBundleVersion.trim())
    && typeof candidate.scoringPolicyVersion === "number"
    && Number.isInteger(candidate.scoringPolicyVersion)
    && candidate.scoringPolicyVersion >= 0
    && typeof candidate.searchEngineVersion === "string"
    && Boolean(candidate.searchEngineVersion.trim());
}

function searchUrlFingerprint(url: string): string | null {
  if (!url.startsWith("/?")) return null;
  return queryFingerprint(new URLSearchParams(url.slice(2)).get("q") ?? "");
}

export function writePropertySearchContext(
  id: string,
  query: string,
  returnUrl: string,
  results: SearchResultItem[],
  runtimeVersion: SearchRuntimeVersion | null | undefined,
  focusForResult: (result: SearchResultItem) => ProofFocus | undefined =
    (result) => result.proof_focuses?.[0],
  now = Date.now(),
  returnScrollY = typeof window === "undefined" ? 0 : window.scrollY,
  returnHistoryIndex = currentHistoryIndex(),
): boolean {
  const fingerprint = queryFingerprint(query);
  if (
    typeof window === "undefined"
    || !id.trim()
    || !fingerprint
    || searchUrlFingerprint(returnUrl) !== fingerprint
    || !isSearchRuntimeVersion(runtimeVersion)
  ) return false;
  const carriedResults = results.flatMap((result) => {
    const propertyId = result.id.trim();
    const title = result.title.trim();
    if (!propertyId || !title) return [];
    const proofFocus = focusForResult(result);
    return [{
      propertyId,
      title,
      societyName: result.society_name.trim(),
      area: result.area.trim(),
      price: knownPositiveNumber(result.price),
      bhk: knownPositiveNumber(result.bhk),
      sqft: knownPositiveNumber(result.sqft),
      stateDisplay: result.home_state_display?.trim()
        || result.project_status_display?.trim()
        || undefined,
      proofFocus: isProofFocus(proofFocus) ? proofFocus : undefined,
    } satisfies PropertySearchResult];
  }).filter((result, index, allResults) =>
    allResults.findIndex((candidate) => candidate.propertyId === result.propertyId) === index
  );
  const context: StoredPropertySearchContext = {
    version: 1,
    id,
    queryFingerprint: fingerprint,
    queryLabel: query.trim(),
    returnUrl,
    returnScrollY: Number.isFinite(returnScrollY)
      ? Math.max(0, Math.round(returnScrollY))
      : 0,
    returnHistoryIndex,
    createdAt: now,
    runtimeVersion,
    results: carriedResults,
  };
  try {
    window.sessionStorage.setItem(
      propertySearchContextStorageKey(id),
      JSON.stringify(context),
    );
    registerSearchSpan(id, now);
    return true;
  } catch {
    // Search remains usable when storage is unavailable; direct visits omit the rail.
    return false;
  }
}

export function writeSearchJourneyContext(
  query: string,
  returnUrl: string,
  results: SearchResultItem[],
  runtimeVersion: SearchRuntimeVersion | null | undefined,
  focusForResult: (result: SearchResultItem) => ProofFocus | undefined =
    (result) => result.proof_focuses?.[0],
  now = Date.now(),
): SearchSpanReference | null {
  if (results.length === 0) return null;
  const journeyId = newContextId();
  const id = writeDiscoveryMapContext(query, results, focusForResult, {
    id: journeyId,
    now,
  });
  const fingerprint = queryFingerprint(query);
  if (
    !id
    || !fingerprint
    || !writePropertySearchContext(
      id,
      query,
      returnUrl,
      results,
      runtimeVersion,
      focusForResult,
      now,
    )
  ) {
    if (id && typeof window !== "undefined") forgetSearchSpan(id);
    return null;
  }
  return { id, queryFingerprint: fingerprint };
}

export function readPropertySearchContext(
  contextId: string | null,
  now = Date.now(),
): StoredPropertySearchContext | null {
  if (typeof window === "undefined" || !contextId?.trim()) return null;
  try {
    const parsed: unknown = JSON.parse(
      window.sessionStorage.getItem(propertySearchContextStorageKey(contextId)) ?? "null",
    );
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Partial<StoredPropertySearchContext>;
    if (
      candidate.version !== 1
      || candidate.id !== contextId
      || typeof candidate.queryFingerprint !== "string"
      || !/^q[0-9a-z]+$/.test(candidate.queryFingerprint)
      || typeof candidate.queryLabel !== "string"
      || !candidate.queryLabel.trim()
      || typeof candidate.returnUrl !== "string"
      || searchUrlFingerprint(candidate.returnUrl) !== candidate.queryFingerprint
      || typeof candidate.returnScrollY !== "number"
      || !Number.isFinite(candidate.returnScrollY)
      || candidate.returnScrollY < 0
      || (candidate.returnHistoryIndex !== undefined && (
        typeof candidate.returnHistoryIndex !== "number"
        || !Number.isInteger(candidate.returnHistoryIndex)
        || candidate.returnHistoryIndex < 0
      ))
      || typeof candidate.createdAt !== "number"
      || candidate.createdAt > now
      || now - candidate.createdAt > SEARCH_SPAN_TTL_MS
      || !isSearchRuntimeVersion(candidate.runtimeVersion)
      || !Array.isArray(candidate.results)
    ) return null;
    const results = candidate.results.filter(
      (result): result is PropertySearchResult => Boolean(
        result
        && typeof result.propertyId === "string"
        && result.propertyId.trim()
        && typeof result.title === "string"
        && result.title.trim()
        && typeof result.societyName === "string"
        && typeof result.area === "string"
        && (result.price === undefined || knownPositiveNumber(result.price) !== undefined)
        && (result.bhk === undefined || knownPositiveNumber(result.bhk) !== undefined)
        && (result.sqft === undefined || knownPositiveNumber(result.sqft) !== undefined)
        && (result.stateDisplay === undefined || (
          typeof result.stateDisplay === "string" && Boolean(result.stateDisplay.trim())
        ))
        && (result.proofFocus === undefined || isProofFocus(result.proofFocus)),
      ),
    );
    if (
      results.length !== candidate.results.length
      || new Set(results.map((result) => result.propertyId)).size !== results.length
    ) return null;
    return {
      version: 1,
      id: candidate.id,
      queryFingerprint: candidate.queryFingerprint,
      queryLabel: candidate.queryLabel.trim(),
      returnUrl: candidate.returnUrl,
      returnScrollY: candidate.returnScrollY,
      returnHistoryIndex: candidate.returnHistoryIndex,
      createdAt: candidate.createdAt,
      runtimeVersion: candidate.runtimeVersion,
      results,
    };
  } catch {
    return null;
  }
}

export function propertySearchContextForProperty(
  context: StoredPropertySearchContext | null,
  propertyId: string,
  expectedQueryFingerprint: string | null,
): PropertySearchContext | null {
  const selectedId = propertyId.trim();
  if (
    !context
    || !selectedId
    || !expectedQueryFingerprint
    || context.queryFingerprint !== expectedQueryFingerprint
    || !context.results.some((result) => result.propertyId === selectedId)
  ) return null;
  return {
    ...context,
    selectedId,
  };
}

export function reconcileSearchSpanAvailability(
  context: PropertySearchContext | null,
  availablePropertyIds: ReadonlySet<string>,
): PropertySearchContext | null {
  if (!context || !availablePropertyIds.has(context.selectedId)) return null;
  const results = context.results.filter((result) =>
    availablePropertyIds.has(result.propertyId)
  );
  return results.some((result) => result.propertyId === context.selectedId)
    ? {
        ...context,
        results,
      }
    : null;
}

function routeOwnedPropertyId(pathname: string): string | null {
  const match = pathname.match(/^\/property\/([^/]+)(?:\/rera)?$/)
    ?? pathname.match(/^\/workspace\/buy-vs-rent\/([^/]+)$/);
  if (!match?.[1]) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

export function searchSpanContextFromLocation(
  pathname: string,
  search: string,
  now = Date.now(),
): PropertySearchContext | null {
  const reference = searchSpanReferenceFromUrl(search);
  if (!reference) return null;
  const stored = readPropertySearchContext(reference.id, now);
  if (!stored || stored.queryFingerprint !== reference.queryFingerprint) return null;
  const params = new URLSearchParams(search);
  const compareFocus = pathname === "/workspace/compare"
    ? params.get("focus")
    : null;
  const selectedId = [
    routeOwnedPropertyId(pathname),
    compareFocus,
    reference.selectedId,
    stored.results[0]?.propertyId,
  ].find((candidate) => stored.results.some(
    (result) => result.propertyId === candidate,
  ));
  return selectedId
    ? propertySearchContextForProperty(stored, selectedId, reference.queryFingerprint)
    : null;
}

export function searchSpanReferenceForTarget(
  context: PropertySearchContext | SearchSpanReference | null,
  propertyId?: string | null,
): SearchSpanReference | null {
  if (!context) return null;
  const selectedId = propertyId
    && (!("results" in context) || context.results.some(
      (result) => result.propertyId === propertyId,
    ))
    ? propertyId
    : context.selectedId;
  return {
    id: context.id,
    queryFingerprint: context.queryFingerprint,
    selectedId,
  };
}

export function propertyHrefWithSearchSpan(
  propertyId: string,
  context: PropertySearchContext | SearchSpanReference | null,
  suffix = "",
): string {
  const result = context && "results" in context
    ? context.results.find((candidate) => candidate.propertyId === propertyId)
    : undefined;
  const href = hrefWithSearchSpan(
    `/property/${encodeURIComponent(propertyId)}${suffix}`,
    searchSpanReferenceForTarget(context, propertyId),
  );
  if (suffix || !result?.proofFocus) return href;
  const [pathname, search = ""] = href.split("?", 2);
  const params = new URLSearchParams(search);
  params.set("focus", JSON.stringify(result.proofFocus));
  return `${pathname}?${params.toString()}`;
}

export function requestSearchSpanReturn(context: PropertySearchContext): void {
  if (typeof window === "undefined") return;
  const discoveryContext: DiscoveryContext = {
    version: 1,
    url: context.returnUrl,
    scrollY: context.returnScrollY,
    resultCount: context.results.length,
  };
  try {
    const serialized = JSON.stringify(discoveryContext);
    window.sessionStorage.setItem(DISCOVERY_STORAGE_KEY, serialized);
    window.sessionStorage.setItem(DISCOVERY_RETURN_INTENT_KEY, serialized);
  } catch {
    // Navigation still succeeds when browser storage is unavailable.
  }
}

function currentHistoryIndex(): number | undefined {
  if (typeof window === "undefined") return undefined;
  const value: unknown = window.history?.state?.idx;
  return typeof value === "number" && Number.isInteger(value) && value >= 0
    ? value
    : undefined;
}

export function searchSpanReturnDelta(
  context: PropertySearchContext,
): number | null {
  const currentIndex = currentHistoryIndex();
  const returnIndex = context.returnHistoryIndex;
  return currentIndex !== undefined
    && returnIndex !== undefined
    && currentIndex > returnIndex
    ? returnIndex - currentIndex
    : null;
}

export function readSearchJourneyNotForMeIds(
  context: PropertySearchContext | null,
): string[] {
  if (typeof window === "undefined" || !context) return [];
  try {
    const value: unknown = JSON.parse(
      window.sessionStorage.getItem(searchJourneyPreferenceStorageKey(context.id)) ?? "null",
    );
    if (!value || typeof value !== "object") return [];
    const candidate = value as { version?: unknown; notForMePropertyIds?: unknown };
    if (candidate.version !== 1 || !Array.isArray(candidate.notForMePropertyIds)) return [];
    const resultIds = new Set(context.results.map((result) => result.propertyId));
    return [...new Set(candidate.notForMePropertyIds)].filter(
      (id): id is string => typeof id === "string"
        && id !== context.selectedId
        && resultIds.has(id),
    );
  } catch {
    return [];
  }
}

export function writeSearchJourneyNotForMeIds(
  context: PropertySearchContext,
  propertyIds: string[],
): boolean {
  if (typeof window === "undefined") return false;
  const resultIds = new Set(context.results.map((result) => result.propertyId));
  const notForMePropertyIds = [...new Set(propertyIds)].filter((id) =>
    id !== context.selectedId && resultIds.has(id)
  );
  try {
    window.sessionStorage.setItem(
      searchJourneyPreferenceStorageKey(context.id),
      JSON.stringify({ version: 1, notForMePropertyIds }),
    );
    window.dispatchEvent?.(new Event(SEARCH_JOURNEY_PREFERENCES_CHANGED_EVENT));
    return true;
  } catch {
    return false;
  }
}

export function readDiscoveryMapContext(
  contextId: string | null,
  now = Date.now(),
): DiscoveryMapContext | null {
  if (typeof window === "undefined" || !contextId?.trim()) return null;
  try {
    const parsed: unknown = JSON.parse(
      window.sessionStorage.getItem(contextStorageKey(contextId)) ?? "null",
    );
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Partial<DiscoveryMapContext>;
    if (
      candidate.version !== 2
      || candidate.id !== contextId
      || typeof candidate.queryFingerprint !== "string"
      || !/^q[0-9a-z]+$/.test(candidate.queryFingerprint)
      || typeof candidate.createdAt !== "number"
      || candidate.createdAt > now
      || now - candidate.createdAt > SEARCH_SPAN_TTL_MS
      || !Array.isArray(candidate.candidates)
    ) return null;
    const candidates = candidate.candidates.filter(
      (item): item is DiscoveryMapCandidate => Boolean(
        item
        && typeof item.propertyId === "string"
        && item.propertyId.trim()
        && Array.isArray(item.propertyIds)
        && item.propertyIds.length > 0
        && item.propertyIds.every((propertyId) =>
          typeof propertyId === "string" && propertyId.trim())
        && item.propertyIds.includes(item.propertyId)
        && typeof item.societyId === "string"
        && item.societyId.trim()
        && typeof item.societyName === "string"
        && item.societyName.trim()
        && typeof item.rank === "number"
        && Number.isInteger(item.rank)
        && item.rank >= 0
        && item.preview
        && typeof item.preview.title === "string"
        && typeof item.preview.area === "string"
        && typeof item.preview.bhk === "number"
        && typeof item.preview.price === "number",
      ),
    );
    if (candidates.length !== candidate.candidates.length) return null;
    const propertyIds = Array.isArray(candidate.propertyIds)
      ? [...new Set(candidate.propertyIds)].filter(
        (propertyId): propertyId is string => typeof propertyId === "string"
          && Boolean(propertyId.trim()),
      )
      : [...new Set(candidates.flatMap((item) => item.propertyIds))];
    if (
      propertyIds.length === 0
      || (candidate.propertyIds !== undefined
        && propertyIds.length !== candidate.propertyIds.length)
    ) return null;
    return {
      version: 2,
      id: candidate.id,
      queryFingerprint: candidate.queryFingerprint,
      createdAt: candidate.createdAt,
      propertyIds,
      candidates,
    };
  } catch {
    return null;
  }
}

export function discoveryMapContextForProperty(
  context: DiscoveryMapContext | null,
  propertyId: string,
  expectedQueryFingerprint: string | null,
): DiscoveryMapContext | null {
  const normalizedPropertyId = propertyId.trim();
  if (
    !context
    || !normalizedPropertyId
    || !expectedQueryFingerprint
    || context.queryFingerprint !== expectedQueryFingerprint
  ) return null;
  return context.propertyIds.includes(normalizedPropertyId)
    ? context
    : null;
}

export function consumeDiscoveryReturn(url: string): number | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.sessionStorage.getItem(DISCOVERY_RETURN_INTENT_KEY);
    window.sessionStorage.removeItem(DISCOVERY_RETURN_INTENT_KEY);
    if (!raw) return null;
    const candidate = JSON.parse(raw) as Partial<DiscoveryContext>;
    if (
      candidate.version !== 1
      || candidate.url !== url
      || typeof candidate.scrollY !== "number"
      || candidate.scrollY < 0
    ) return null;
    return candidate.scrollY;
  } catch {
    window.sessionStorage.removeItem(DISCOVERY_RETURN_INTENT_KEY);
    return null;
  }
}
