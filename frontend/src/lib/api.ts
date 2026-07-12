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
  SellerDashboard,
  InterestRequest,
  InterestResponse,
  InterestCount,
  RegistrationDraft,
  RegistrationCreated,
  StepUpdated,
  PublishResult,
  Step1Payload,
  Step2Payload,
  Step3Payload,
  Step4Payload,
  Step5Payload,
  Step6Payload,
  Step7Payload,
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

async function postJson<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(
      `API ${res.status}: ${text || res.statusText}`
    );
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

export function getSellerDashboard(id: string): Promise<SellerDashboard> {
  return fetchJson(`/api/sellers/${encodeURIComponent(id)}/dashboard`);
}

// Interest API
export function expressInterest(req: InterestRequest): Promise<InterestResponse> {
  return postJson("/api/interests", req);
}

export function getInterestCount(propertyId: string): Promise<InterestCount> {
  return fetchJson(`/api/properties/${encodeURIComponent(propertyId)}/interests/count`);
}

// Registration API
export function createRegistration(): Promise<RegistrationCreated> {
  return postJson("/api/registrations");
}

export function getRegistration(id: string): Promise<RegistrationDraft> {
  return fetchJson(`/api/registrations/${encodeURIComponent(id)}`);
}

export function updateRegistrationStep(
  id: string,
  step: number,
  payload: Step1Payload | Step2Payload | Step3Payload | Step4Payload | Step5Payload | Step6Payload | Step7Payload
): Promise<StepUpdated> {
  return putJson(
    `/api/registrations/${encodeURIComponent(id)}/step/${step}`,
    payload
  );
}

export function publishRegistration(draftId: string): Promise<PublishResult> {
  return postJson(`/api/registrations/${encodeURIComponent(draftId)}/publish`);
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
