import type {
  PropertyCard,
  PropertyDetailResponse,
  PropertyEvidenceBatchResponse,
  PropertyEvidenceResponse,
  ReraEvidenceReportResponse,
  ProofFocus,
  PropertySurfacesResponse,
  RecommendationResponse,
  AreaListItem,
  AreaDetail,
  AreaTrackerResponse,
  DiscoveryResponse,
  SearchResponse,
  SurfaceBatchResponse,
  SurfaceSceneResponse,
} from "./types.ts";
import { getFixtureResponse } from "./dev-fixtures.ts";
import {
  filterListableProperties,
  isListableProperty,
} from "./property-filters.ts";
import { API_ORIGIN } from "./runtimeConfig.ts";

const META_ENV = (import.meta as ImportMeta & {
  env?: Record<string, string | boolean | undefined>;
}).env ?? {};
const ENABLE_DEV_FIXTURES = META_ENV.VITE_USE_FIXTURE_API === "true";
const inFlightSearches = new Map<string, Promise<SearchResponse>>();
const DEFAULT_API_TIMEOUT_MS = 4_000;
const GET_ATTEMPT_COUNT = 2;
const GET_RETRY_DELAY_MS = 200;

type ApiFetchOptions = {
  signal?: AbortSignal;
  timeoutMs?: number;
};

function getDevFixture<T>(path: string): T | null {
  if (!ENABLE_DEV_FIXTURES) return null;
  const fixture = getFixtureResponse(path);
  return fixture === null ? null : fixture as T;
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function requestSignal(options: ApiFetchOptions): AbortSignal {
  const timeoutSignal = AbortSignal.timeout(
    options.timeoutMs ?? DEFAULT_API_TIMEOUT_MS,
  );
  return options.signal
    ? AbortSignal.any([options.signal, timeoutSignal])
    : timeoutSignal;
}

function isRetryable(error: unknown): boolean {
  return error instanceof TypeError
    || (error instanceof DOMException && error.name === "TimeoutError")
    || (error instanceof Error && error.message.startsWith("API 5"));
}

function retryDelay(): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, GET_RETRY_DELAY_MS));
}

async function fetchJson<T>(path: string, options: ApiFetchOptions = {}): Promise<T> {
  for (let attempt = 0; attempt < GET_ATTEMPT_COUNT; attempt += 1) {
    try {
      const res = await fetch(`${API_ORIGIN}${path}`, {
        signal: requestSignal(options),
      });
      if (res.ok) return res.json();

      const fixture = getDevFixture<T>(path);
      if (fixture !== null) return fixture;

      const text = await res.text().catch(() => "");
      throw new Error(
        `API ${res.status}: ${text || res.statusText}`
      );
    } catch (error) {
      if (isAbortError(error) || options.signal?.aborted) throw error;
      const fixture = getDevFixture<T>(path);
      if (fixture !== null) return fixture;
      const canRetry = attempt + 1 < GET_ATTEMPT_COUNT && isRetryable(error);
      if (!canRetry) throw error;
      await retryDelay();
    }
  }
  throw new Error("API request failed");
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_ORIGIN}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: requestSignal({}),
  });
  if (!res.ok) {
    const fixture = getDevFixture<T>(path);
    if (fixture !== null) return fixture;

    const text = await res.text().catch(() => "");
    throw new Error(`API ${res.status}: ${text || res.statusText}`);
  }
  return res.json();
}

export function getHealth(): Promise<{
  service: string;
  status: string;
  process_started_at?: string;
  scoring_policy_version?: number;
  recommendation_engine_version?: string;
  serving_bundle_version?: string;
}> {
  return fetchJson("/api/health");
}

export function getProperties(options?: ApiFetchOptions): Promise<PropertyCard[]> {
  return fetchJson<PropertyCard[]>("/api/properties", options).then(filterListableProperties);
}

export function getProperty(id: string, options?: ApiFetchOptions): Promise<PropertyDetailResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}`, options);
}

export function getPropertyRecommendations(id: string): Promise<RecommendationResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/recommendations`);
}

export function getPropertyEvidence(id: string): Promise<PropertyEvidenceResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/evidence`);
}

export function getPropertyRera(id: string): Promise<ReraEvidenceReportResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/rera`);
}

export function getPropertySurface(
  id: string,
  surfaceId: string,
  focus?: ProofFocus,
): Promise<SurfaceSceneResponse> {
  return fetchJson(propertySurfacePath(id, surfaceId, focus));
}

export function propertyDetailPath(
  id: string,
  focus?: ProofFocus,
  discoveryContextId?: string | null,
): string {
  const params = new URLSearchParams();
  if (focus) params.set("focus", JSON.stringify(focus));
  if (discoveryContextId?.trim()) params.set("context", discoveryContextId);
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  return `/property/${encodeURIComponent(id)}${suffix}`;
}

export function propertySurfacePath(id: string, surfaceId: string, focus?: ProofFocus): string {
  const params = focus ? `?focus=${encodeURIComponent(JSON.stringify(focus))}` : "";
  return `/api/properties/${encodeURIComponent(id)}/surfaces/${encodeURIComponent(surfaceId)}${params}`;
}

export function parseProofFocusParam(value: string | null): ProofFocus | undefined {
  if (!value) return undefined;
  try {
    const parsed = JSON.parse(value) as Partial<ProofFocus>;
    if (
      typeof parsed.surfaceId !== "string"
      || typeof parsed.layerId !== "string"
      || typeof parsed.factKey !== "string"
      || typeof parsed.reason !== "string"
    ) {
      return undefined;
    }
    return parsed as ProofFocus;
  } catch {
    return undefined;
  }
}

export function getPropertySurfaces(
  id: string,
  surfaceIds: string[] = ["around_this_home"],
): Promise<PropertySurfacesResponse> {
  const ids = surfaceIds.join(",");
  return fetchJson(
    `/api/properties/${encodeURIComponent(id)}/surfaces?ids=${encodeURIComponent(ids)}`,
  );
}

export function getPropertySurfacesBatch(
  propertyIds: string[],
  surfaceIds: string[] = ["around_this_home"],
): Promise<SurfaceBatchResponse> {
  return postJson("/api/properties/surfaces/batch", {
    propertyIds,
    surfaceIds,
  });
}

export function getPropertyEvidenceBatch(
  propertyIds: string[],
  limit?: number,
): Promise<PropertyEvidenceBatchResponse> {
  return postJson("/api/properties/evidence/batch", {
    property_ids: propertyIds,
    limit,
  });
}

export function getAreas(options?: ApiFetchOptions): Promise<AreaListItem[]> {
  return fetchJson("/api/areas", options);
}

export function getArea(id: string): Promise<AreaDetail> {
  return fetchJson(`/api/areas/${encodeURIComponent(id)}`);
}

export function getAreaTracker(options?: ApiFetchOptions): Promise<AreaTrackerResponse> {
  return fetchJson("/api/areas/tracker", options);
}

export function searchProperties(query: string): Promise<SearchResponse> {
  const key = query.trim();
  const existing = inFlightSearches.get(key);
  if (existing) return existing;

  const request = fetchJson<SearchResponse>(`/api/search?q=${encodeURIComponent(query)}`)
    .finally(() => {
      if (inFlightSearches.get(key) === request) {
        inFlightSearches.delete(key);
      }
    });
  inFlightSearches.set(key, request);
  return request;
}

export function getDiscovery(options?: ApiFetchOptions): Promise<DiscoveryResponse> {
  return fetchJson<DiscoveryResponse>("/api/discovery", options).then((response) => ({
    ...response,
    shelves: response.shelves.map((shelf) => ({
      ...shelf,
      cards: shelf.cards.filter((card) => isListableProperty(card.property)),
    })),
  }));
}

export type PlatformStats = {
  properties: number;
  societies: number;
  areas: number;
};

export async function getStats(options?: ApiFetchOptions): Promise<PlatformStats> {
  const [props, areas] = await Promise.all([
    getProperties(options),
    getAreas(options),
  ]);
  const societyCount = new Set(props.map((p) => p.society_name).filter(Boolean)).size;

  return {
    properties: props.length,
    societies: societyCount,
    areas: areas.length,
  };
}
