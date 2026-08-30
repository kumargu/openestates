import type { ProofFocus, SearchResultItem } from "./types.ts";

export type NavigationMode = "landing" | "discovery" | "property-context" | "workspace";

export type DiscoveryContext = {
  version: 1;
  url: string;
  scrollY: number;
};

const DISCOVERY_STORAGE_KEY = "openestates:last-discovery:v1";
const DISCOVERY_RETURN_INTENT_KEY = "openestates:discovery-return-intent:v1";
const DISCOVERY_MAP_CONTEXT_KEY = "openestates:discovery-map-context:v1";
const DISCOVERY_MAP_CANDIDATE_LIMIT = 24;

export type DiscoveryMapCandidate = {
  id: string;
  propertyIds: string[];
  societyName: string;
  proofFocus?: ProofFocus;
};

export type DiscoveryMapContext = {
  version: 1;
  query: string;
  candidates: DiscoveryMapCandidate[];
};

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
    };
  } catch {
    return null;
  }
}

export function writeDiscoveryContext(url: string, scrollY = 0): void {
  if (typeof window === "undefined") return;
  const params = url.startsWith("/?") ? new URLSearchParams(url.slice(2)) : null;
  if (!params?.get("q")?.trim()) return;
  const context: DiscoveryContext = {
    version: 1,
    url,
    scrollY: Math.max(0, Math.round(scrollY)),
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
  window.sessionStorage.removeItem(DISCOVERY_MAP_CONTEXT_KEY);
}

export function writeDiscoveryMapContext(
  query: string,
  results: SearchResultItem[],
  focusForResult: (result: SearchResultItem) => ProofFocus | undefined =
    (result) => result.proof_focuses?.[0],
): void {
  if (typeof window === "undefined" || !query.trim()) return;
  const societies = new Map<string, DiscoveryMapCandidate>();
  const candidates: DiscoveryMapCandidate[] = [];
  for (const result of results) {
    const societyName = result.society_name.trim() || result.title.trim();
    const societyKey = result.kg_entity_refs?.society_entity_id
      || societyName.toLocaleLowerCase("en-IN");
    if (!societyName) continue;
    const existing = societies.get(societyKey);
    if (existing) {
      if (!existing.propertyIds.includes(result.id)) existing.propertyIds.push(result.id);
      continue;
    }
    const candidate: DiscoveryMapCandidate = {
      id: result.id,
      propertyIds: [result.id],
      societyName,
      proofFocus: focusForResult(result),
    };
    societies.set(societyKey, candidate);
    candidates.push(candidate);
    if (candidates.length === DISCOVERY_MAP_CANDIDATE_LIMIT) break;
  }
  window.sessionStorage.setItem(DISCOVERY_MAP_CONTEXT_KEY, JSON.stringify({
    version: 1,
    query: query.trim(),
    candidates,
  } satisfies DiscoveryMapContext));
}

export function readDiscoveryMapContext(): DiscoveryMapContext | null {
  if (typeof window === "undefined") return null;
  try {
    const parsed: unknown = JSON.parse(
      window.sessionStorage.getItem(DISCOVERY_MAP_CONTEXT_KEY) ?? "null",
    );
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Partial<DiscoveryMapContext>;
    if (
      candidate.version !== 1
      || typeof candidate.query !== "string"
      || !candidate.query.trim()
      || !Array.isArray(candidate.candidates)
    ) return null;
    const candidates = candidate.candidates.filter(
      (item): item is DiscoveryMapCandidate => Boolean(
        item
        && typeof item.id === "string"
        && item.id.trim()
        && Array.isArray(item.propertyIds)
        && item.propertyIds.every((id) => typeof id === "string" && id.trim())
        && typeof item.societyName === "string"
        && item.societyName.trim(),
      ),
    );
    return { version: 1, query: candidate.query.trim(), candidates };
  } catch {
    return null;
  }
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
