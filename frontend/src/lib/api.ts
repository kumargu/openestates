import type {
  PropertyCard,
  PropertyDetailResponse,
  AreaListItem,
  AreaDetail,
  ShortlistResponse,
  SearchResponse,
  SocietySearchResponse,
  SocietySearchResult,
  SellerCard,
  Seller,
  InterestRequest,
  InterestResponse,
  InterestCount,
} from "./types.ts";

const API_BASE = import.meta.env.VITE_API_BASE ?? "http://localhost:4000";

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

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
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

export function searchProperties(query: string): Promise<SearchResponse> {
  return fetchJson(`/api/search?q=${encodeURIComponent(query)}`);
}

export function searchSocieties(query: string): Promise<SocietySearchResponse> {
  return fetchJson(`/api/societies/search?q=${encodeURIComponent(query)}`);
}

export function getSociety(slug: string): Promise<SocietySearchResult> {
  return fetchJson(`/api/societies/${encodeURIComponent(slug)}`);
}

export type ClaimRequest = {
  property_id: string;
  name: string;
  phone?: string;
  email?: string;
};

export type ClaimResponse = {
  status: string;
  property_id: string;
};

export function submitClaim(req: ClaimRequest): Promise<ClaimResponse> {
  return postJson("/api/claims", req);
}

// Seller API
export function getSellers(): Promise<SellerCard[]> {
  return fetchJson("/api/sellers");
}

export function getSeller(id: string): Promise<Seller> {
  return fetchJson(`/api/sellers/${encodeURIComponent(id)}`);
}

// Interest API
export function expressInterest(req: InterestRequest): Promise<InterestResponse> {
  return postJson("/api/interests", req);
}

export function getInterestCount(propertyId: string): Promise<InterestCount> {
  return fetchJson(`/api/properties/${encodeURIComponent(propertyId)}/interests/count`);
}

export type PlatformStats = {
  properties: number;
  societies: number;
  areas: number;
};

export async function getStats(): Promise<PlatformStats> {
  // Derive stats from existing endpoints in parallel
  const [props, areas] = await Promise.all([
    getProperties(),
    getAreas(),
  ]);
  return {
    properties: props.length,
    societies: new Set(props.map((p) => p.society_name)).size,
    areas: areas.length,
  };
}
