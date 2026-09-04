import assert from "node:assert/strict";
import test from "node:test";
import { propertyDetailPath } from "../src/lib/api.ts";
import {
  captureDiscoveryDeparture,
  clearDiscoveryContext,
  consumeDiscoveryReturn,
  discoveryMapContextForProperty,
  hasSearchSpanUrlParams,
  hrefWithSearchSpan,
  navigationMode,
  propertyExploreHref,
  propertyHrefWithSearchSpan,
  propertySearchContextForProperty,
  queryFingerprint,
  readDiscoveryContext,
  readDiscoveryMapContext,
  readPropertySearchContext,
  readSearchSpanDismissedIds,
  reconcileSearchSpanAvailability,
  requestDiscoveryReturn,
  rotatePropertySearchResults,
  SEARCH_SPAN_TTL_MS,
  searchSpanReturnDelta,
  searchSpanContextFromLocation,
  searchSpanReferenceFromUrl,
  stripSearchSpanUrlParams,
  writeDiscoveryContext,
  writeDiscoveryMapContext,
  writeDiscoveryResultCount,
  writePropertySearchContext,
  writeSearchSpanDismissedIds,
  writeSearchJourneyContext,
} from "../src/lib/navigationContext.ts";
import type { SearchResultItem, SearchRuntimeVersion } from "../src/lib/types.ts";

const RUNTIME_VERSION: SearchRuntimeVersion = {
  servingBundleVersion: "serving-test-v1",
  scoringPolicyVersion: 1,
  searchEngineVersion: "search-test-v1",
};

function searchResult(id: string, title = `Home ${id}`): SearchResultItem {
  return {
    id,
    kg_entity_refs: {
      property_entity_id: `property:${id}`,
      society_entity_id: `society:${id}`,
      source_entity_ids: [],
    },
    title,
    area: "Whitefield",
    price: 20_000_000,
    price_per_sqft: 12_000,
    bhk: 3,
    sqft: 1_600,
    society_name: `Society ${id}`,
    builder_name: "Builder",
    hero_image: null,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    metro_distance_mins: 10,
    floor: 4,
    total_floors: 18,
    facing: "East",
    match_score: 0.8,
    match_label: "Strong match",
    match_reason: "Near school",
    match_tier: "exact",
    proof_focuses: [{
      surfaceId: "around_this_home",
      layerId: "schools",
      factKey: "nearby_schools",
      matchedLabel: `School ${id}`,
      reason: "Matched a nearby school",
    }],
  };
}

const sessionValues = new Map<string, string>();
Object.defineProperty(globalThis, "window", {
  value: {
    sessionStorage: {
      getItem: (key: string) => sessionValues.get(key) ?? null,
      setItem: (key: string, value: string) => sessionValues.set(key, value),
      removeItem: (key: string) => sessionValues.delete(key),
    },
  },
  configurable: true,
});

test("navigation modes follow route-owned context", () => {
  assert.equal(navigationMode("/"), "landing");
  assert.equal(navigationMode("/", "?q=quiet+3bhk"), "discovery");
  assert.equal(navigationMode("/", "?sort=price"), "landing");
  assert.equal(navigationMode("/property/home-1"), "property-context");
  assert.equal(navigationMode("/property/home-1/rera"), "property-context");
  assert.equal(navigationMode("/workspace"), "workspace");
  assert.equal(navigationMode("/workspace/compare", "?ids=one,two"), "workspace");
  assert.equal(navigationMode("/workspace/buy-vs-rent/home-1"), "workspace");
});

test("search span references round-trip through workspace destinations", () => {
  const href = hrefWithSearchSpan("/workspace/compare?ids=one%2Ctwo", {
    id: "span-1",
    queryFingerprint: "qspan1",
    selectedId: "two",
  });

  assert.equal(
    href,
    "/workspace/compare?ids=one%2Ctwo&context=span-1&qf=qspan1&searchHome=two",
  );
  assert.deepEqual(
    searchSpanReferenceFromUrl(new URL(href, "http://test.local").search),
    { id: "span-1", queryFingerprint: "qspan1", selectedId: "two" },
  );
  assert.equal(
    hrefWithSearchSpan("/property/two?view=map#schools", {
      id: "span-1",
      queryFingerprint: "qspan1",
      selectedId: "two",
    }),
    "/property/two?view=map&context=span-1&qf=qspan1&searchHome=two#schools",
  );
});

test("partial search span parameters are detected and removed together", () => {
  assert.equal(hasSearchSpanUrlParams("?focus=map&context=span-1"), true);
  assert.equal(hasSearchSpanUrlParams("?focus=map&qf=invalid"), true);
  assert.equal(hasSearchSpanUrlParams("?focus=map"), false);
  assert.equal(
    stripSearchSpanUrlParams("?context=span-1&focus=map&qf=invalid&searchHome=one"),
    "?focus=map",
  );
});

test("unrelated and legacy paths do not gain property context", () => {
  assert.equal(navigationMode("/property/home-1/unknown"), "landing");
  assert.equal(navigationMode("/about"), "landing");
});

test("discovery scroll restores only after an explicit one-shot return request", () => {
  sessionValues.clear();
  const url = "/?q=quiet+3bhk";
  writeDiscoveryContext(url, 640);

  assert.equal(consumeDiscoveryReturn(url), null);
  requestDiscoveryReturn(url);
  assert.equal(consumeDiscoveryReturn(url), 640);
  assert.equal(consumeDiscoveryReturn(url), null);
});

test("a mismatched return target is discarded", () => {
  sessionValues.clear();
  writeDiscoveryContext("/?q=whitefield", 320);
  requestDiscoveryReturn("/?q=whitefield");

  assert.equal(consumeDiscoveryReturn("/?q=sarjapur"), null);
  assert.equal(consumeDiscoveryReturn("/?q=whitefield"), null);
});

test("leaving Explore prepares browser Back to restore the exact position", () => {
  sessionValues.clear();
  const url = "/?q=quiet+3bhk";

  captureDiscoveryDeparture(url, 912);

  assert.equal(consumeDiscoveryReturn(url), 912);
  assert.equal(consumeDiscoveryReturn(url), null);
});

test("discovery result count survives scroll-position updates", () => {
  sessionValues.clear();
  const url = "/?q=quiet+3bhk";
  writeDiscoveryContext(url, 120);
  writeDiscoveryResultCount(url, 18);
  writeDiscoveryContext(url, 912);

  assert.deepEqual(readDiscoveryContext(), {
    version: 1,
    url,
    scrollY: 912,
    resultCount: 18,
  });
});

test("a new discovery query does not inherit another query's count", () => {
  sessionValues.clear();
  writeDiscoveryContext("/?q=whitefield", 120);
  writeDiscoveryResultCount("/?q=whitefield", 8);
  writeDiscoveryContext("/?q=sarjapur", 0);

  assert.equal(readDiscoveryContext()?.resultCount, undefined);
});

test("starting Explore fresh forgets an old query and return position", () => {
  sessionValues.clear();
  const url = "/?q=whitefield";
  captureDiscoveryDeparture(url, 480);

  clearDiscoveryContext();

  assert.equal(consumeDiscoveryReturn(url), null);
});

test("property exploration preserves search context or falls back to area", () => {
  assert.equal(
    propertyExploreHref("Whitefield", "/?q=quiet+3bhk&sort=proof"),
    "/?q=quiet+3bhk&sort=proof",
  );
  assert.equal(
    propertyExploreHref("  Whitefield, Bengaluru  ", "/"),
    "/?q=Whitefield%2C+Bengaluru",
  );
  assert.equal(propertyExploreHref(" ", "/"), "/");
});

test("property journey context preserves every carried result in search order", () => {
  sessionValues.clear();
  const results = Array.from({ length: 15 }, (_, index) => ({
    id: `home-${index + 1}`,
    kg_entity_refs: {
      property_entity_id: `property:home-${index + 1}`,
      society_entity_id: `society:${Math.floor(index / 3)}`,
      source_entity_ids: [],
    },
    title: `Home ${index + 1}`,
    area: "Whitefield",
    price: 20_000_000 + index,
    price_per_sqft: 12_000,
    bhk: 3,
    sqft: 1_600 + index,
    society_name: `Society ${Math.floor(index / 3)}`,
    builder_name: "Builder",
    hero_image: null,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    home_state_display: "Delivered · 4 yrs old",
    metro_distance_mins: 10,
    floor: 4,
    total_floors: 18,
    facing: "East",
    match_score: 0.8,
    match_label: "Strong match",
    match_reason: "Near school",
    match_tier: "exact",
    proof_focuses: [{
      surfaceId: "around_this_home",
      layerId: "schools",
      factKey: "nearby_schools",
      matchedLabel: `School ${index + 1}`,
      reason: "Matched a nearby school",
    }],
  } satisfies SearchResultItem));

  writePropertySearchContext(
    "journey-1",
    "quiet 3bhk",
    "/?q=quiet+3bhk&area=Whitefield",
    results,
    RUNTIME_VERSION,
    (result) => result.proof_focuses?.[0],
    1_000,
    912,
  );
  const stored = readPropertySearchContext("journey-1", 1_001);
  const selected = propertySearchContextForProperty(
    stored,
    "home-10",
    queryFingerprint("quiet 3bhk"),
  );

  assert.equal(stored?.results.length, 15);
  assert.deepEqual(stored?.results.map((result) => result.propertyId), results.map((result) => result.id));
  assert.equal(selected?.selectedIndex, 9);
  assert.equal(selected?.totalResultCount, 15);
  assert.equal(stored?.results[0]?.stateDisplay, "Delivered · 4 yrs old");
  assert.equal(selected?.returnUrl, "/?q=quiet+3bhk&area=Whitefield");
  assert.equal(selected?.returnScrollY, 912);
  assert.deepEqual(selected?.runtimeVersion, RUNTIME_VERSION);
  assert.deepEqual(
    selected?.results[9]?.proofFocus,
    results[9]?.proof_focuses?.[0],
  );
  const selectedResult = selected?.results[9];
  assert.ok(selected && selectedResult);
  const carriedUrl = new URL(
    propertyDetailPath(
      selectedResult.propertyId,
      selectedResult.proofFocus,
      selected.id,
      selected.queryFingerprint,
    ),
    "http://test.local",
  );
  assert.equal(carriedUrl.searchParams.get("context"), "journey-1");
  assert.equal(
    carriedUrl.searchParams.get("qf"),
    queryFingerprint("quiet 3bhk"),
  );
  assert.deepEqual(
    JSON.parse(carriedUrl.searchParams.get("focus") ?? "null"),
    selectedResult.proofFocus,
  );
});

test("property search rail pins the current home and rotates the remaining results", () => {
  const results = ["one", "two", "three", "four"].map((propertyId, rank) => ({
    propertyId,
    title: propertyId,
    societyName: propertyId,
    area: "Whitefield",
    rank,
  }));

  assert.deepEqual(
    rotatePropertySearchResults(results, "three").map((result) => result.propertyId),
    ["three", "four", "one", "two"],
  );
  assert.deepEqual(
    rotatePropertySearchResults(results, "one").map((result) => result.propertyId),
    ["one", "two", "three", "four"],
  );
});

test("availability reconciliation drops removed homes without replacing the current result", () => {
  sessionValues.clear();
  writePropertySearchContext(
    "journey-availability",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [searchResult("one"), searchResult("two"), searchResult("three")],
    RUNTIME_VERSION,
    undefined,
    1_000,
  );
  const context = searchSpanContextFromLocation(
    "/property/two",
    `?context=journey-availability&qf=${queryFingerprint("quiet 3bhk")}`,
    1_001,
  );

  assert.deepEqual(
    reconcileSearchSpanAvailability(context, new Set(["two", "three"]))?.results.map(
      (result) => [result.propertyId, result.rank],
    ),
    [["two", 1], ["three", 2]],
  );
  assert.equal(
    reconcileSearchSpanAvailability(context, new Set(["two", "three"]))?.totalResultCount,
    3,
  );
  assert.equal(reconcileSearchSpanAvailability(context, new Set(["one", "three"])), null);
});

test("search return uses the exact parent history entry when it is still behind us", () => {
  sessionValues.clear();
  Object.assign(window, { history: { state: { idx: 7 } } });
  writePropertySearchContext(
    "journey-history",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [searchResult("one")],
    RUNTIME_VERSION,
    undefined,
    1_000,
    0,
    4,
  );
  const context = searchSpanContextFromLocation(
    "/property/one",
    `?context=journey-history&qf=${queryFingerprint("quiet 3bhk")}`,
    1_001,
  );

  assert.ok(context);
  assert.equal(searchSpanReturnDelta(context), -3);
  Object.assign(window, { history: { state: { idx: 3 } } });
  assert.equal(searchSpanReturnDelta(context), null);
});

test("route identity wins over compare focus and carried search cursor", () => {
  sessionValues.clear();
  const results = [searchResult("one"), searchResult("two"), searchResult("three")];
  assert.equal(writePropertySearchContext(
    "journey-precedence",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    results,
    RUNTIME_VERSION,
    undefined,
    1_000,
  ), true);
  const reference = `?context=journey-precedence&qf=${queryFingerprint("quiet 3bhk")}&searchHome=three`;

  assert.equal(
    searchSpanContextFromLocation("/property/two", reference, 1_001)?.selectedId,
    "two",
  );
  assert.equal(
    searchSpanContextFromLocation(
      "/workspace/compare",
      `${reference}&focus=one`,
      1_001,
    )?.selectedId,
    "one",
  );
  assert.equal(
    searchSpanContextFromLocation("/workspace", reference, 1_001)?.selectedId,
    "three",
  );
});

test("off-span saved homes retain the prior search cursor", () => {
  sessionValues.clear();
  const results = [searchResult("one"), searchResult("two")];
  writePropertySearchContext(
    "journey-membership",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    results,
    RUNTIME_VERSION,
    undefined,
    1_000,
  );
  const reference = `?context=journey-membership&qf=${queryFingerprint("quiet 3bhk")}&searchHome=one`;

  assert.equal(
    searchSpanContextFromLocation("/property/outside", reference, 1_001)?.selectedId,
    "one",
  );
  assert.equal(
    searchSpanContextFromLocation(
      "/workspace/buy-vs-rent/outside",
      reference,
      1_001,
    )?.selectedId,
    "one",
  );
  assert.equal(
    searchSpanContextFromLocation(
      "/workspace/compare",
      `${reference}&focus=outside`,
      1_001,
    )?.selectedId,
    "one",
  );

  const context = searchSpanContextFromLocation("/property/one", reference, 1_001);
  assert.ok(context);
  const href = new URL(propertyHrefWithSearchSpan("outside", context), "http://test.local");
  assert.equal(href.pathname, "/property/outside");
  assert.equal(href.searchParams.get("context"), "journey-membership");
  assert.equal(href.searchParams.get("qf"), queryFingerprint("quiet 3bhk"));
  assert.equal(href.searchParams.get("searchHome"), "one");
});

test("property links restore each result's proof focus", () => {
  sessionValues.clear();
  const results = [searchResult("one"), searchResult("two")];
  writePropertySearchContext(
    "journey-proof",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    results,
    RUNTIME_VERSION,
    undefined,
    1_000,
  );
  const context = searchSpanContextFromLocation(
    "/property/one",
    `?context=journey-proof&qf=${queryFingerprint("quiet 3bhk")}`,
    1_001,
  );
  const href = new URL(propertyHrefWithSearchSpan("two", context), "http://test.local");

  assert.equal(href.pathname, "/property/two");
  assert.equal(href.searchParams.get("searchHome"), "two");
  assert.deepEqual(
    JSON.parse(href.searchParams.get("focus") ?? "null"),
    results[1]?.proof_focuses?.[0],
  );
});

test("writer removes malformed duplicates and restores contiguous canonical ranks", () => {
  sessionValues.clear();
  const one = searchResult("one");
  assert.equal(writePropertySearchContext(
    "journey-normalized",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [one, searchResult("bad", " "), one, searchResult("three")],
    RUNTIME_VERSION,
    undefined,
    1_000,
  ), true);

  assert.deepEqual(
    readPropertySearchContext("journey-normalized", 1_001)?.results.map(
      ({ propertyId, rank }) => ({ propertyId, rank }),
    ),
    [{ propertyId: "one", rank: 0 }, { propertyId: "three", rank: 1 }],
  );
});

test("dismissed search homes persist per span without hiding the current home", () => {
  sessionValues.clear();
  const results = [searchResult("one"), searchResult("two"), searchResult("three")];
  writePropertySearchContext(
    "journey-dismissed",
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    results,
    RUNTIME_VERSION,
    undefined,
    1_000,
  );
  const context = searchSpanContextFromLocation(
    "/property/two",
    `?context=journey-dismissed&qf=${queryFingerprint("quiet 3bhk")}`,
    1_001,
  );
  assert.ok(context);
  writeSearchSpanDismissedIds(context, ["one", "two", "one", "outside"]);

  assert.deepEqual(readSearchSpanDismissedIds(context), ["one"]);
  const nextContext = searchSpanContextFromLocation(
    "/property/one",
    `?context=journey-dismissed&qf=${queryFingerprint("quiet 3bhk")}`,
    1_001,
  );
  assert.ok(nextContext);
  assert.deepEqual(readSearchSpanDismissedIds(nextContext), []);
});

test("property journey context fails closed for direct, stale, and unrelated visits", () => {
  sessionValues.clear();
  assert.equal(readPropertySearchContext(null), null);

  sessionValues.set("openestates:property-search-context:v1:stale", JSON.stringify({
    version: 1,
    id: "stale",
    queryFingerprint: queryFingerprint("quiet 3bhk"),
    queryLabel: "quiet 3bhk",
    returnUrl: "/?q=quiet+3bhk",
    returnScrollY: 0,
    createdAt: 1_000,
    runtimeVersion: RUNTIME_VERSION,
    results: [{
      propertyId: "home-1",
      title: "Home 1",
      societyName: "Society 1",
      area: "Whitefield",
      rank: 0,
    }],
  }));

  const stored = readPropertySearchContext("stale", 1_001);
  assert.equal(propertySearchContextForProperty(stored, "unrelated", queryFingerprint("quiet 3bhk")), null);
  assert.equal(propertySearchContextForProperty(stored, "home-1", queryFingerprint("another search")), null);
  assert.equal(readPropertySearchContext("stale", 1_000 + SEARCH_SPAN_TTL_MS + 1), null);
});

test("map context keeps ranked search societies without duplicate configurations", () => {
  sessionValues.clear();
  const result = (id: string, societyId: string, societyName: string): SearchResultItem => ({
    id,
    kg_entity_refs: {
      property_entity_id: `property:${id}`,
      society_entity_id: societyId,
      source_entity_ids: [],
    },
    title: societyName,
    area: "Whitefield",
    price: 20_000_000,
    price_per_sqft: 12_000,
    bhk: 3,
    sqft: 1_600,
    society_name: societyName,
    builder_name: "Builder",
    hero_image: null,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    metro_distance_mins: 10,
    floor: 4,
    total_floors: 18,
    facing: "East",
    match_score: 0.8,
    match_label: "Strong match",
    match_reason: "Near school",
    match_tier: "exact",
  });
  const contextId = writeDiscoveryMapContext("quiet 3bhk", [
    result("one", "society:a", "Alpha"),
    result("two", "society:a", "Alpha"),
    result("three", "society:b", "Beta"),
  ], { id: "context-one", now: 1_000 });

  assert.equal(contextId, "context-one");
  assert.deepEqual(readDiscoveryMapContext(contextId, 1_001), {
    version: 2,
    id: "context-one",
    queryFingerprint: queryFingerprint("quiet 3bhk"),
    createdAt: 1_000,
    propertyIds: ["one", "two", "three"],
    candidates: [
      {
        propertyId: "one",
        propertyIds: ["one", "two"],
        societyId: "society:a",
        societyName: "Alpha",
        rank: 0,
        preview: { title: "Alpha", area: "Whitefield", bhk: 3, price: 20_000_000 },
      },
      {
        propertyId: "three",
        propertyIds: ["three"],
        societyId: "society:b",
        societyName: "Beta",
        rank: 2,
        preview: { title: "Beta", area: "Whitefield", bhk: 3, price: 20_000_000 },
      },
    ],
  });
});

test("map membership covers every result while previews stay within the serving limit", () => {
  sessionValues.clear();
  const results = Array.from({ length: 32 }, (_, index) => searchResult(`home-${index}`));
  writeDiscoveryMapContext("quiet 3bhk", results, { id: "full-map", now: 1_000 });
  const context = readDiscoveryMapContext("full-map", 1_001);

  assert.equal(context?.candidates.length, 24);
  assert.equal(context?.propertyIds.length, 32);
  assert.equal(
    discoveryMapContextForProperty(
      context,
      "home-31",
      queryFingerprint("quiet 3bhk"),
    )?.id,
    "full-map",
  );
});

test("map context is consumed only by a property carried by its URL token", () => {
  sessionValues.clear();
  const result = (id: string, societyId: string): SearchResultItem => ({
    id,
    kg_entity_refs: {
      property_entity_id: `property:${id}`,
      society_entity_id: societyId,
      source_entity_ids: [],
    },
    title: id,
    area: "Whitefield",
    price: 20_000_000,
    price_per_sqft: 12_000,
    bhk: 3,
    sqft: 1_600,
    society_name: societyId,
    builder_name: "Builder",
    hero_image: null,
    transparency_tags: [],
    description_summary: "",
    possession_status: "Ready",
    metro_distance_mins: 10,
    floor: 4,
    total_floors: 18,
    facing: "East",
    match_score: 0.8,
    match_label: "Strong match",
    match_reason: "Near school",
    match_tier: "exact",
  });
  writeDiscoveryMapContext("quiet 3bhk", [
    result("one", "society:a"),
    result("two", "society:a"),
  ], { id: "bound-context", now: 1_000 });
  const context = readDiscoveryMapContext("bound-context", 1_001);
  const fingerprint = queryFingerprint("quiet 3bhk");

  assert.equal(discoveryMapContextForProperty(context, "one", fingerprint)?.id, "bound-context");
  assert.equal(discoveryMapContextForProperty(context, "two", fingerprint)?.id, "bound-context");
  assert.equal(discoveryMapContextForProperty(context, "unrelated", fingerprint), null);
  assert.equal(discoveryMapContextForProperty(context, "one", null), null);
  assert.equal(
    discoveryMapContextForProperty(context, "one", queryFingerprint("another search")),
    null,
  );
});

test("discovery map context requires its URL token and shares the journey lifetime", () => {
  sessionValues.clear();
  const result = {
    id: "one",
    kg_entity_refs: { property_entity_id: "property:one", society_entity_id: "society:a", source_entity_ids: [] },
    title: "Alpha", area: "Whitefield", price: 20_000_000, price_per_sqft: 12_000,
    bhk: 3, sqft: 1_600, society_name: "Alpha", builder_name: "Builder", hero_image: null,
    transparency_tags: [], description_summary: "", possession_status: "Ready", metro_distance_mins: 10,
    floor: 4, total_floors: 18, facing: "East", match_score: 0.8, match_label: "Strong match",
    match_reason: "Near school", match_tier: "exact",
  } satisfies SearchResultItem;
  writeDiscoveryMapContext("Quiet   3BHK", [result], { id: "token", now: 5_000 });

  assert.equal(queryFingerprint(" quiet 3bhk "), queryFingerprint("Quiet   3BHK"));
  assert.equal(readDiscoveryMapContext(null, 5_001), null);
  assert.equal(readDiscoveryMapContext("wrong-token", 5_001), null);
  assert.equal(readDiscoveryMapContext("token", 5_000 + SEARCH_SPAN_TTL_MS + 1), null);

  sessionValues.set("openestates:discovery-map-context:v2:malformed", JSON.stringify({
    version: 2,
    id: "malformed",
    queryFingerprint: queryFingerprint("quiet 3bhk"),
    createdAt: 5_000,
    candidates: [{
      propertyId: "missing-preview",
      propertyIds: ["missing-preview"],
      societyId: "society:a",
      societyName: "Alpha",
      rank: 0,
    }],
  }));
  assert.equal(readDiscoveryMapContext("malformed", 5_001), null);

  sessionValues.set("openestates:discovery-map-context:v2:bad-fingerprint", JSON.stringify({
    version: 2,
    id: "bad-fingerprint",
    queryFingerprint: "quiet 3bhk",
    createdAt: 5_000,
    candidates: [],
  }));
  assert.equal(readDiscoveryMapContext("bad-fingerprint", 5_001), null);
});

test("discovery map context reuses one bounded slot for result updates", () => {
  sessionValues.clear();
  const result = {
    id: "one",
    kg_entity_refs: { property_entity_id: "property:one", society_entity_id: "society:a", source_entity_ids: [] },
    title: "Alpha", area: "Whitefield", price: 20_000_000, price_per_sqft: 12_000,
    bhk: 3, sqft: 1_600, society_name: "Alpha", builder_name: "Builder", hero_image: null,
    transparency_tags: [], description_summary: "", possession_status: "Ready", metro_distance_mins: 10,
    floor: 4, total_floors: 18, facing: "East", match_score: 0.8, match_label: "Strong match",
    match_reason: "Near school", match_tier: "exact",
  } satisfies SearchResultItem;

  const firstId = writeDiscoveryMapContext("quiet 3bhk", [result], { now: 5_000 });
  assert.ok(firstId);
  writePropertySearchContext(
    firstId,
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [result],
    RUNTIME_VERSION,
    undefined,
    5_000,
  );
  const updatedId = writeDiscoveryMapContext("quiet 3bhk", [result], { now: 5_001 });
  const nextQueryId = writeDiscoveryMapContext("near metro", [result], { now: 5_002 });

  assert.equal(updatedId, firstId);
  assert.notEqual(nextQueryId, firstId);
  assert.equal(readDiscoveryMapContext(firstId, 5_003)?.id, firstId);
  assert.equal(readPropertySearchContext(firstId, 5_003)?.id, firstId);
  assert.equal(readDiscoveryMapContext(nextQueryId, 5_003)?.id, nextQueryId);
});

test("search span history keeps six recent journeys and expires the oldest", () => {
  sessionValues.clear();
  const result = searchResult("one");
  for (let index = 0; index < 7; index += 1) {
    writeDiscoveryMapContext(`query ${index}`, [result], {
      id: `journey-${index}`,
      now: 10_000 + index,
    });
  }

  assert.equal(readDiscoveryMapContext("journey-0", 10_010), null);
  assert.equal(readDiscoveryMapContext("journey-1", 10_010)?.id, "journey-1");
  assert.equal(readDiscoveryMapContext("journey-6", 10_010)?.id, "journey-6");
});

test("repeating a query creates an immutable journey instead of overwriting history", () => {
  sessionValues.clear();
  const first = writeSearchJourneyContext(
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [searchResult("one")],
    RUNTIME_VERSION,
    undefined,
    20_000,
  );
  const second = writeSearchJourneyContext(
    "quiet 3bhk",
    "/?q=quiet+3bhk",
    [searchResult("two")],
    { ...RUNTIME_VERSION, scoringPolicyVersion: 2 },
    undefined,
    20_001,
  );

  assert.ok(first && second);
  assert.notEqual(first.id, second.id);
  assert.equal(readPropertySearchContext(first.id, 20_002)?.results[0]?.propertyId, "one");
  assert.equal(readPropertySearchContext(second.id, 20_002)?.results[0]?.propertyId, "two");
});
