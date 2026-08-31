import assert from "node:assert/strict";
import test from "node:test";
import {
  captureDiscoveryDeparture,
  clearDiscoveryContext,
  consumeDiscoveryReturn,
  navigationMode,
  propertyExploreHref,
  readDiscoveryContext,
  requestDiscoveryReturn,
  writeDiscoveryContext,
  writeDiscoveryResultCount,
} from "../src/lib/navigationContext.ts";

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
