import type {
  PropertyCard,
  PropertyDetailResponse,
  AreaListItem,
  AreaDetail,
  ShortlistResponse,
  SearchResponse,
  SocietySearchResponse,
  Seller,
  Bid,
  BidStats,
  PlaceBidRequest,
  PlaceBidResponse,
  RegisterSellerRequest,
  SellerListingsResponse,
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

export function searchProperties(query: string, debug = false): Promise<SearchResponse> {
  const debugParam = debug ? "&debug=true" : "";
  return fetchJson(`/api/search?q=${encodeURIComponent(query)}${debugParam}`);
}

export function searchSocieties(query: string): Promise<SocietySearchResponse> {
  return fetchJson(`/api/societies/search?q=${encodeURIComponent(query)}`);
}

// ---------------------------------------------------------------------------
// Marketplace
// ---------------------------------------------------------------------------

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`API ${res.status}: ${text || res.statusText}`);
  }
  return res.json();
}

async function putJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`API ${res.status}: ${text || res.statusText}`);
  }
  return res.json();
}

export function registerSeller(body: RegisterSellerRequest): Promise<{ seller: Seller }> {
  return postJson("/api/sellers/register", body);
}

export function getSeller(id: string): Promise<Seller> {
  return fetchJson(`/api/sellers/${encodeURIComponent(id)}`);
}

export function getSellerListings(id: string): Promise<SellerListingsResponse> {
  return fetchJson(`/api/sellers/${encodeURIComponent(id)}/listings`);
}

export function placeBid(propertyId: string, body: PlaceBidRequest): Promise<PlaceBidResponse> {
  return postJson(`/api/properties/${encodeURIComponent(propertyId)}/bids`, body);
}

export function getPropertyBids(propertyId: string): Promise<{ bids: Bid[]; count: number }> {
  return fetchJson(`/api/properties/${encodeURIComponent(propertyId)}/bids`);
}

export function getBidStats(propertyId: string): Promise<BidStats> {
  return fetchJson(`/api/properties/${encodeURIComponent(propertyId)}/bid-stats`);
}

export function updateBidStatus(
  propertyId: string,
  bidId: string,
  status: "accepted" | "rejected",
): Promise<{ success: boolean }> {
  return putJson(`/api/properties/${encodeURIComponent(propertyId)}/bids/${encodeURIComponent(bidId)}/status`, { status });
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
