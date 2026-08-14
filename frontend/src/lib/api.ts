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

const META_ENV = (import.meta as ImportMeta & {
  env?: Record<string, string | boolean | undefined>;
}).env ?? {};
const API_BASE = typeof META_ENV.VITE_API_BASE === "string"
  ? META_ENV.VITE_API_BASE
  : "";
const ENABLE_DEV_FIXTURES = META_ENV.VITE_USE_FIXTURE_API === "true";
const inFlightSearches = new Map<string, Promise<SearchResponse>>();

type ApiFetchOptions = {
  signal?: AbortSignal;
};

type ApiErrorPayload = {
  error?: string;
  reason_codes?: string[];
};

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string | null;
  readonly reasonCodes: string[];

  constructor(status: number, statusText: string, body: string) {
    let payload: ApiErrorPayload = {};
    try {
      payload = JSON.parse(body) as ApiErrorPayload;
    } catch {
      // Non-JSON upstream failures still retain their HTTP status.
    }
    super(payload.error || body || statusText || `Request failed (${status})`);
    this.name = "ApiRequestError";
    this.status = status;
    this.code = payload.error ?? null;
    this.reasonCodes = Array.isArray(payload.reason_codes) ? payload.reason_codes : [];
  }
}

function getDevFixture<T>(path: string): T | null {
  if (!ENABLE_DEV_FIXTURES) return null;
  const fixture = getFixtureResponse(path);
  return fixture === null ? null : fixture as T;
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

async function fetchJson<T>(path: string, options: ApiFetchOptions = {}): Promise<T> {
  try {
    const res = await fetch(`${API_BASE}${path}`, { signal: options.signal });
    if (!res.ok) {
      const fixture = getDevFixture<T>(path);
      if (fixture !== null) return fixture;

      const text = await res.text().catch(() => "");
      throw new ApiRequestError(res.status, res.statusText, text);
    }
    return res.json();
  } catch (error) {
    if (isAbortError(error)) throw error;
    const fixture = getDevFixture<T>(path);
    if (fixture !== null) return fixture;
    throw error;
  }
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const fixture = getDevFixture<T>(path);
    if (fixture !== null) return fixture;

    const text = await res.text().catch(() => "");
    throw new ApiRequestError(res.status, res.statusText, text);
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
  return fetchJson<PropertyCard[]>("/api/properties", options);
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

export function propertyDetailPath(id: string, focus?: ProofFocus): string {
  const params = focus ? `?focus=${encodeURIComponent(JSON.stringify(focus))}` : "";
  return `/property/${encodeURIComponent(id)}${params}`;
}

export function propertySurfacePath(id: string, surfaceId: string, focus?: ProofFocus): string {
  const params = focus ? `?focus=${encodeURIComponent(JSON.stringify(focus))}` : "";
  return `/api/properties/${encodeURIComponent(id)}/surfaces/${encodeURIComponent(surfaceId)}${params}`;
}

export function propertyDetailSurfaceId(focus?: ProofFocus): string {
  return focus?.surfaceId ?? "around_this_home";
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
  return fetchJson<DiscoveryResponse>("/api/discovery", options);
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
