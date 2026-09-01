import type { ProofFocus, SearchResultItem } from "./types.ts";

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
const DISCOVERY_MAP_CANDIDATE_LIMIT = 24;
export const DISCOVERY_CONTEXT_TTL_MS = 30 * 60 * 1_000;

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
  requestDiscoveryReturn(url);
}

export function clearDiscoveryContext(): void {
  if (typeof window === "undefined") return;
  window.sessionStorage.removeItem(DISCOVERY_STORAGE_KEY);
  window.sessionStorage.removeItem(DISCOVERY_RETURN_INTENT_KEY);
  const contextId = window.sessionStorage.getItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY);
  if (contextId) window.sessionStorage.removeItem(contextStorageKey(contextId));
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
      candidates,
    };
    window.sessionStorage.setItem(contextStorageKey(id), JSON.stringify(context));
    window.sessionStorage.setItem(DISCOVERY_MAP_CONTEXT_LATEST_KEY, id);
    if (previousId && previousId !== id) {
      window.sessionStorage.removeItem(contextStorageKey(previousId));
    }
    return id;
  } catch {
    return null;
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
      || now - candidate.createdAt > DISCOVERY_CONTEXT_TTL_MS
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
    return {
      version: 2,
      id: candidate.id,
      queryFingerprint: candidate.queryFingerprint,
      createdAt: candidate.createdAt,
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
  return context.candidates.some((candidate) =>
    candidate.propertyIds.includes(normalizedPropertyId))
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
