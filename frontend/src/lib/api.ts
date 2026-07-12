import type {
  PropertyCard,
  PropertyDetailResponse,
  AreaListItem,
  AreaDetail,
  SearchResponse,
  SocietySearchResponse,
  SocietySearchResult,
} from "./types.ts";
import { getFixtureResponse } from "./dev-fixtures.ts";

const API_BASE = import.meta.env.VITE_API_BASE ?? "http://localhost:4000";
const ENABLE_DEV_FIXTURES = import.meta.env.VITE_USE_FIXTURE_API !== "false"
  && (import.meta.env.DEV || import.meta.env.VITE_USE_FIXTURE_API === "true");

function getDevFixture<T>(path: string): T | null {
  if (!ENABLE_DEV_FIXTURES) return null;
  const fixture = getFixtureResponse(path);
  return fixture === null ? null : fixture as T;
}

async function fetchJson<T>(path: string): Promise<T> {
  try {
    const res = await fetch(`${API_BASE}${path}`);
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
    const fixture = getDevFixture<T>(path);
    if (fixture !== null) return fixture;
    throw error;
  }
}

export function getHealth(): Promise<{ service: string; status: string }> {
  return fetchJson("/api/health");
}

export function getProperties(): Promise<PropertyCard[]> {
  return fetchJson("/api/properties");
}

export function getProperty(id: string): Promise<PropertyDetailResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}`);
}

export function getAreas(): Promise<AreaListItem[]> {
  return fetchJson("/api/areas");
}

export function getArea(id: string): Promise<AreaDetail> {
  return fetchJson(`/api/areas/${encodeURIComponent(id)}`);
}

export function searchProperties(query: string): Promise<SearchResponse> {
  return fetchJson(`/api/search?q=${encodeURIComponent(query)}`);
}

export function searchSocieties(query: string): Promise<SocietySearchResponse> {
  return fetchJson(`/api/societies/search?q=${encodeURIComponent(query)}`);
}

export function getSociety(slug: string): Promise<SocietySearchResult> {
  return fetchJson(`/api/societies/${encodeURIComponent(slug)}`);
}

export type PlatformStats = {
  properties: number;
  societies: number;
  areas: number;
};

export async function getStats(): Promise<PlatformStats> {
  const [props, areas, societyNodes] = await Promise.all([
    getProperties(),
    getAreas(),
    fetchJson<unknown[]>("/api/knowledge/nodes?type=society").catch(() => null),
  ]);
  const societyCount = Array.isArray(societyNodes) && societyNodes.length > 0
    ? societyNodes.length
    : new Set(props.map((p) => p.society_name).filter(Boolean)).size;

  return {
    properties: props.length,
    societies: societyCount,
    areas: areas.length,
  };
}
