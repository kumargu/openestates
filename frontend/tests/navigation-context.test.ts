import assert from "node:assert/strict";
import test from "node:test";
import {
  captureDiscoveryDeparture,
  clearDiscoveryContext,
  consumeDiscoveryReturn,
  DISCOVERY_CONTEXT_TTL_MS,
  discoveryMapContextForProperty,
  navigationMode,
  propertyExploreHref,
  queryFingerprint,
  readDiscoveryMapContext,
  requestDiscoveryReturn,
  writeDiscoveryContext,
  writeDiscoveryMapContext,
} from "../src/lib/navigationContext.ts";
import type { SearchResultItem } from "../src/lib/types.ts";

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

  assert.equal(discoveryMapContextForProperty(context, "one")?.id, "bound-context");
  assert.equal(discoveryMapContextForProperty(context, "two")?.id, "bound-context");
  assert.equal(discoveryMapContextForProperty(context, "unrelated"), null);
});

test("discovery map context requires its URL token and expires after thirty minutes", () => {
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
  assert.equal(readDiscoveryMapContext("token", 5_000 + DISCOVERY_CONTEXT_TTL_MS + 1), null);

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
