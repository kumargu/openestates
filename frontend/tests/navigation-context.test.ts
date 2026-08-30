import assert from "node:assert/strict";
import test from "node:test";
import {
  captureDiscoveryDeparture,
  clearDiscoveryContext,
  consumeDiscoveryReturn,
  navigationMode,
  propertyExploreHref,
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
  writeDiscoveryMapContext("quiet 3bhk", [
    result("one", "society:a", "Alpha"),
    result("two", "society:a", "Alpha"),
    result("three", "society:b", "Beta"),
  ]);

  assert.deepEqual(readDiscoveryMapContext(), {
    version: 1,
    query: "quiet 3bhk",
    candidates: [
      { id: "one", propertyIds: ["one", "two"], societyName: "Alpha" },
      { id: "three", propertyIds: ["three"], societyName: "Beta" },
    ],
  });
});
