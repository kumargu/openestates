import journeyPolicy from "../../../app/config/ui/search-journey.json" with { type: "json" };
import { validateWire } from "./wire.ts";
import type {
  PropertyCard,
  PropertyDetailResponse,
  PropertyEvidenceResponse,
  ReraEvidenceReportResponse,
  ProofFocus,
  PropertySurfacesResponse,
  RecommendationResponse,
  AreaListItem,
  AreaDetail,
  DiscoveryResponse,
  SearchResponse,
  SearchJourneyEnvelope,
  SearchRevisionTarget,
  SearchProofResolution,
  SurfaceBatchResponse,
  SurfaceSceneResponse,
} from "./types.ts";
import { API_ORIGIN } from "./runtimeConfig.ts";
import { projectSearchJourney } from "./search-journey.ts";

const META_ENV = (import.meta as ImportMeta & {
  env?: Record<string, string | boolean | undefined>;
}).env ?? {};
const ENABLE_DEV_FIXTURES = META_ENV.DEV === true
  && META_ENV.VITE_USE_FIXTURE_API === "true";
const inFlightSearches = new Map<string, Promise<SearchResponse>>();
const inFlightResumes = new Map<string, Promise<SearchJourneyEnvelope>>();
const inFlightSurfaceBatches = new Map<string, Promise<SurfaceBatchResponse>>();
const PROPERTY_CATALOG_CACHE_MS = 60_000;
const DISCOVERY_CACHE_MS = 60_000;
let cachedPropertyCatalog: { loadedAt: number; value: PropertyCard[] } | null = null;
let inFlightPropertyCatalog: Promise<PropertyCard[]> | null = null;
let cachedDiscovery: { loadedAt: number; value: DiscoveryResponse } | null = null;
let inFlightDiscovery: Promise<DiscoveryResponse> | null = null;
let propertyCatalogRequestGeneration = 0;
const DEFAULT_API_TIMEOUT_MS = 4_000;
const GET_ATTEMPT_COUNT = 2;
const GET_RETRY_DELAY_MS = 200;

type ApiFetchOptions = {
  signal?: AbortSignal;
  timeoutMs?: number;
  snapshotIdentity?: string;
};

type PropertyCatalogFetchOptions = ApiFetchOptions & {
  refresh?: boolean;
};

async function getDevFixture<T>(path: string): Promise<T | null> {
  if (
    typeof import.meta.env === "undefined"
    || import.meta.env.DEV !== true
    || META_ENV.VITE_USE_FIXTURE_API !== "true"
  ) return null;
  const { getFixtureResponse } = await import('./dev-fixtures.ts');
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

function withCallerAbort<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener("abort", abort, { once: true });
    promise.then(resolve, reject).finally(() => {
      signal.removeEventListener("abort", abort);
    });
  });
}

function decodeWire<T>(path: string, value: unknown): T {
  const route = path.split("?")[0];
  if (["/api/search", "/api/search/revisions", "/api/search/resume"].includes(route)) return validateWire<T>("journey", value);
  if (route === "/api/search/proofs/resolve") return validateWire<T>("proof", value);
  if (route === "/api/properties/batch") return validateWire<T>("summaries", value);
  if (/^\/api\/properties\/[^/]+$/.test(route)) return validateWire<T>("detail", value);
  if (/^\/api\/properties\/[^/]+\/surfaces\/[^/]+$/.test(route)) return validateWire<T>("context", value);
  return value as T;
}

async function fetchJson<T>(path: string, options: ApiFetchOptions = {}): Promise<T> {
  const localFixture = await getDevFixture<T>(path);
  if (options.signal?.aborted) throw new DOMException('Aborted', 'AbortError');
  if (localFixture !== null) return localFixture;
  for (let attempt = 0; attempt < GET_ATTEMPT_COUNT; attempt += 1) {
    try {
      const res = await fetch(`${API_ORIGIN}${path}`, {
        signal: requestSignal(options),
      });
      if (res.ok) return decodeWire<T>(path, await res.json());

      const fixture = await getDevFixture<T>(path);
      if (fixture !== null) return fixture;

      const payload = await res.json().catch(() => ({}));
      if (res.status === 409) invalidateSnapshotCaches();
      throw new ApiError(res.status, payload);
    } catch (error) {
      if (isAbortError(error) || options.signal?.aborted) throw error;
      const fixture = await getDevFixture<T>(path);
      if (fixture !== null) return fixture;
      const canRetry = attempt + 1 < GET_ATTEMPT_COUNT && isRetryable(error);
      if (!canRetry) throw error;
      await retryDelay();
    }
  }
  throw new Error("API request failed");
}

function invalidateSnapshotCaches() {
  cachedPropertyCatalog = null;
  cachedDiscovery = null;
  propertyCatalogRequestGeneration += 1;
  inFlightSearches.clear();
  inFlightResumes.clear();
  inFlightSurfaceBatches.clear();
}

export class ApiError extends Error {
  status: number;
  code?: string;
  constructor(status: number, payload: { code?: string; error?: string }) {
    super(`API ${status}: ${payload.code ?? payload.error ?? "request_failed"}`);
    this.status = status;
    this.code = payload.code ?? payload.error;
  }
}

async function postJson<T>(path: string, body: unknown, options: ApiFetchOptions = {}): Promise<T> {
  if (ENABLE_DEV_FIXTURES) {
    const { getFixtureSearchMutation } = await import("./dev-fixtures.ts");
    const fixture = getFixtureSearchMutation(path, body);
    if (options.signal?.aborted) throw new DOMException("Aborted", "AbortError");
    if (fixture) return fixture as T;
  }
  const res = await fetch(`${API_ORIGIN}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: requestSignal(options),
  });
  if (!res.ok) {
    const fixture = await getDevFixture<T>(path);
    if (fixture !== null) return fixture;

    const payload = await res.json().catch(() => ({}));
    if (res.status === 409) invalidateSnapshotCaches();
    if (path === "/api/search/proofs/resolve" && "resolutionStatus" in payload) {
      validateWire("proofFailure", payload);
    }
    throw new ApiError(res.status, payload);
  }
  return decodeWire<T>(path, await res.json());
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

function requestPropertyCatalog(
  options: ApiFetchOptions,
  generation: number,
): Promise<PropertyCard[]> {
  return fetchJson<PropertyCard[]>("/api/properties", options)
    .then((value) => {
      if (generation === propertyCatalogRequestGeneration) {
        cachedPropertyCatalog = { loadedAt: Date.now(), value };
      }
      return value;
    });
}

function startPropertyCatalogRequest(options: ApiFetchOptions): Promise<PropertyCard[]> {
  const generation = ++propertyCatalogRequestGeneration;
  const request = requestPropertyCatalog(options, generation);
  const clearRequest = () => {
    if (inFlightPropertyCatalog === request) inFlightPropertyCatalog = null;
  };
  inFlightPropertyCatalog = request;
  void request.then(clearRequest, clearRequest);
  return request;
}

export function getProperties(options: PropertyCatalogFetchOptions = {}): Promise<PropertyCard[]> {
  if (options.refresh) {
    return startPropertyCatalogRequest({
      signal: options.signal,
      timeoutMs: options.timeoutMs,
    });
  }

  const now = Date.now();
  if (cachedPropertyCatalog && now - cachedPropertyCatalog.loadedAt < PROPERTY_CATALOG_CACHE_MS) {
    return withCallerAbort(Promise.resolve(cachedPropertyCatalog.value), options.signal);
  }
  const request = inFlightPropertyCatalog
    ?? startPropertyCatalogRequest({ timeoutMs: options.timeoutMs });
  return withCallerAbort(request, options.signal);
}

export async function getPropertyCardsByIds(ids: readonly string[], options?: ApiFetchOptions): Promise<PropertyCard[]> {
  const propertyIds = [...new Set(ids)].filter(Boolean);
  if (propertyIds.length === 0) return [];
  const items: PropertyCard[] = [];
  let snapshotIdentity = options?.snapshotIdentity;
  for (let start = 0; start < propertyIds.length; start += journeyPolicy.summaryBatchLimit) {
    const response = await postJson<{ snapshotIdentity: string; items: PropertyCard[] }>(
      "/api/properties/batch", { propertyIds: propertyIds.slice(start, start + journeyPolicy.summaryBatchLimit), snapshotIdentity }, options,
    );
    snapshotIdentity = response.snapshotIdentity;
    items.push(...response.items);
  }
  return items;
}

export function getProperty(id: string, options?: ApiFetchOptions): Promise<PropertyDetailResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}${options?.snapshotIdentity ? `?snapshotIdentity=${encodeURIComponent(options.snapshotIdentity)}` : ""}`, options);
}

export function getPropertyRecommendations(id: string, options?: ApiFetchOptions): Promise<RecommendationResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/recommendations${options?.snapshotIdentity ? `?snapshotIdentity=${encodeURIComponent(options.snapshotIdentity)}` : ""}`, options);
}

export function getPropertyEvidence(id: string, options?: ApiFetchOptions): Promise<PropertyEvidenceResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/evidence${options?.snapshotIdentity ? `?snapshotIdentity=${encodeURIComponent(options.snapshotIdentity)}` : ""}`, options);
}

export function getPropertyRera(id: string, options?: ApiFetchOptions): Promise<ReraEvidenceReportResponse> {
  return fetchJson(`/api/properties/${encodeURIComponent(id)}/rera${options?.snapshotIdentity ? `?snapshotIdentity=${encodeURIComponent(options.snapshotIdentity)}` : ""}`, options);
}

export function getPropertySurface(
  id: string,
  surfaceId: string,
  focus?: ProofFocus,
  options?: ApiFetchOptions,
): Promise<SurfaceSceneResponse> {
  const path = propertySurfacePath(id, surfaceId, focus);
  const separator = path.includes("?") ? "&" : "?";
  return fetchJson(`${path}${options?.snapshotIdentity ? `${separator}snapshotIdentity=${encodeURIComponent(options.snapshotIdentity)}` : ""}`, options);
}

export function propertyDetailPath(
  id: string,
  focus?: ProofFocus,
  discoveryContextId?: string | null,
  discoveryQueryFingerprint?: string | null,
): string {
  const params = new URLSearchParams();
  if (focus?.proofToken) params.set("proofToken", focus.proofToken);
  if (discoveryContextId?.trim()) params.set("context", discoveryContextId);
  if (discoveryQueryFingerprint?.trim()) params.set("qf", discoveryQueryFingerprint);
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  return `/property/${encodeURIComponent(id)}${suffix}`;
}

export function propertySurfacePath(id: string, surfaceId: string, focus?: ProofFocus): string {
  const params = focus?.proofToken ? `?proofToken=${encodeURIComponent(focus.proofToken)}` : "";
  return `/api/properties/${encodeURIComponent(id)}/surfaces/${encodeURIComponent(surfaceId)}${params}`;
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
  options?: ApiFetchOptions,
): Promise<SurfaceBatchResponse> {
  const key = JSON.stringify([propertyIds, surfaceIds, options?.snapshotIdentity]);
  const existing = inFlightSurfaceBatches.get(key);
  if (existing) return withCallerAbort(existing, options?.signal);
  const request = postJson<SurfaceBatchResponse>("/api/properties/surfaces/batch", {
    propertyIds,
    surfaceIds,
    snapshotIdentity: options?.snapshotIdentity,
  }, { timeoutMs: options?.timeoutMs }).finally(() => {
    if (inFlightSurfaceBatches.get(key) === request) inFlightSurfaceBatches.delete(key);
  });
  inFlightSurfaceBatches.set(key, request);
  return withCallerAbort(request, options?.signal);
}

export function getAreas(options?: ApiFetchOptions): Promise<AreaListItem[]> {
  return fetchJson("/api/areas", options);
}

export function getArea(id: string): Promise<AreaDetail> {
  return fetchJson(`/api/areas/${encodeURIComponent(id)}`);
}

export function searchProperties(
  query: string,
  options?: ApiFetchOptions,
): Promise<SearchResponse> {
  const key = query.trim();
  const existing = inFlightSearches.get(key);
  if (existing) return withCallerAbort(existing, options?.signal);

  const request = fetchJson<SearchJourneyEnvelope>(`/api/search?q=${encodeURIComponent(query)}`, {
    timeoutMs: options?.timeoutMs,
  })
    .then((journey) => projectSearchJourney(journey))
    .finally(() => {
      if (inFlightSearches.get(key) === request) {
        inFlightSearches.delete(key);
      }
    });
  inFlightSearches.set(key, request);
  return withCallerAbort(request, options?.signal);
}

export async function reviseSearch(parent: SearchResponse, utterance: string, clientMutationId: string,
  target?: SearchRevisionTarget, selectedPropertyId?: string, options?: ApiFetchOptions): Promise<SearchResponse> {
  const journey = await postJson<SearchJourneyEnvelope>("/api/search/revisions", {
    parentToken: parent.journey?.active.revision.stateToken,
    parentResultIds: parent.orderedResultIds, utterance, clientMutationId, target,
    selectedPropertyId: selectedPropertyId && parent.orderedResultIds.includes(selectedPropertyId) ? selectedPropertyId : undefined,
  }, options);
  return projectSearchJourney(journey, parent);
}

export async function resumeSearch(parent: SearchResponse, options?: ApiFetchOptions): Promise<SearchResponse> {
  const journey = await resumeJourney({
    token: parent.journey?.active.revision.stateToken ?? "",
    ids: parent.orderedResultIds,
  }, options);
  return projectSearchJourney(journey, parent);
}

export async function resumeSearchCheckpoint(checkpoint: { token: string; ids: string[] }, options?: ApiFetchOptions): Promise<SearchResponse> {
  const journey = await resumeJourney(checkpoint, options);
  return projectSearchJourney(journey);
}

function resumeJourney(checkpoint: { token: string; ids: string[] }, options?: ApiFetchOptions): Promise<SearchJourneyEnvelope> {
  const key = JSON.stringify([checkpoint.token, checkpoint.ids]);
  const existing = inFlightResumes.get(key);
  if (existing) return withCallerAbort(existing, options?.signal);
  const request = postJson<SearchJourneyEnvelope>("/api/search/resume", {
    parentToken: checkpoint.token,
    knownResultIds: checkpoint.ids,
  }, { timeoutMs: options?.timeoutMs }).finally(() => {
    if (inFlightResumes.get(key) === request) inFlightResumes.delete(key);
  });
  inFlightResumes.set(key, request);
  return withCallerAbort(request, options?.signal);
}

export function resolveSearchProof(proofToken: string, propertyId: string, options?: ApiFetchOptions): Promise<SearchProofResolution> {
  return postJson("/api/search/proofs/resolve", { proofToken, propertyId }, options);
}

export function getDiscovery(options?: ApiFetchOptions): Promise<DiscoveryResponse> {
  const now = Date.now();
  if (cachedDiscovery && now - cachedDiscovery.loadedAt < DISCOVERY_CACHE_MS) {
    return withCallerAbort(Promise.resolve(cachedDiscovery.value), options?.signal);
  }
  const request = inFlightDiscovery ?? fetchJson<DiscoveryResponse>("/api/discovery", {
    timeoutMs: options?.timeoutMs,
  }).then((value) => {
    cachedDiscovery = { loadedAt: Date.now(), value };
    return value;
  });
  if (!inFlightDiscovery) {
    inFlightDiscovery = request;
    const clearRequest = () => {
      if (inFlightDiscovery === request) inFlightDiscovery = null;
    };
    void request.then(clearRequest, clearRequest);
  }
  return withCallerAbort(request, options?.signal);
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
