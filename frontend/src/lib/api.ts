import type {
  PropertyCard,
  PropertyDetailResponse,
  PropertyEvidenceBatchResponse,
  PropertyEvidenceResponse,
  PropertySummaryJobResponse,
  AreaListItem,
  AreaDetail,
  AreaTrackerResponse,
  DiscoveryResponse,
  SearchResponse,
} from "./types.ts";
import { getFixtureResponse } from "./dev-fixtures.ts";
import {
  filterListableProperties,
  filterListableSearchResponse,
  isListableProperty,
} from "./property-filters.ts";

const API_BASE = import.meta.env.VITE_API_BASE
  ?? (import.meta.env.DEV ? "" : "http://127.0.0.1:4000");
const ENABLE_DEV_FIXTURES = import.meta.env.VITE_USE_FIXTURE_API !== "false"
  && (import.meta.env.DEV || import.meta.env.VITE_USE_FIXTURE_API === "true");
const inFlightSearches = new Map<string, Promise<SearchResponse>>();

type ApiFetchOptions = {
  signal?: AbortSignal;
};

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
      throw new Error(
        `API ${res.status}: ${text || res.statusText}`
      );
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
    throw new Error(`API ${res.status}: ${text || res.statusText}`);
  }
  return res.json();
}

export function getHealth(): Promise<{ service: string; status: string }> {
  return fetchJson("/api/health");
}

export function getProperties(options?: ApiFetchOptions): Promise<PropertyCard[]> {
  return fetchJson<PropertyCard[]>("/api/properties", options).then(filterListableProperties);
}

export function getProperty(id: string): Promise<PropertyDetailResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}`);
}

export function getPropertyEvidence(id: string): Promise<PropertyEvidenceResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/evidence`);
}

export function createPropertySummaryJob(id: string): Promise<PropertySummaryJobResponse> {
  return postJson(`/api/properties/${encodeURIComponent(id)}/summary-jobs`, {});
}

export function getPropertySummaryJob(
  id: string,
  jobId: string,
  options: ApiFetchOptions = {},
): Promise<PropertySummaryJobResponse> {
  return fetchJson(
    `/api/properties/${encodeURIComponent(id)}/summary-jobs/${encodeURIComponent(jobId)}`,
    options,
  );
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
    .then(filterListableSearchResponse)
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
