import type {
  PropertyCard,
  PropertyDetailResponse,
  AreaListItem,
  AreaDetail,
  ShortlistResponse,
} from "./types.ts";

const API_BASE = "http://localhost:4000";

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(
      `API ${res.status}: ${text || res.statusText}`
    );
  }
  return res.json();
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

export function getShortlist(): Promise<ShortlistResponse> {
  return fetchJson("/api/shortlist");
}
