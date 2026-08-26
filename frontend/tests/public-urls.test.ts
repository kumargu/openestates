import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizePublicOrigin,
  resolveBackendOwnedUrl,
  resolveSiteUrl,
} from "../src/lib/publicUrls.ts";

test("production origins normalize one trailing slash", () => {
  assert.equal(
    normalizePublicOrigin("https://api.80feet.app/", "VITE_API_BASE", {
      required: true,
      httpsOnly: true,
    }),
    "https://api.80feet.app",
  );
  assert.equal(
    normalizePublicOrigin("https://api.80feet.app", "VITE_API_BASE"),
    "https://api.80feet.app",
  );
});

test("production origins fail closed when missing or unsafe", () => {
  assert.throws(
    () => normalizePublicOrigin("", "VITE_API_BASE", { required: true }),
    /must be set/,
  );
  assert.throws(
    () => normalizePublicOrigin("http://api.80feet.app", "VITE_API_BASE", { httpsOnly: true }),
    /must use https/,
  );
  assert.throws(
    () => normalizePublicOrigin("https://api.80feet.app/v1", "VITE_API_BASE"),
    /without a path/,
  );
});

test("backend paths preserve encoded identifiers and prefix only owned routes", () => {
  const origin = "https://api.80feet.app";
  assert.equal(
    resolveBackendOwnedUrl("/api/properties/a%2Fb", origin),
    "https://api.80feet.app/api/properties/a%2Fb",
  );
  assert.equal(
    resolveBackendOwnedUrl("/media/images/sha256/aa/hero.webp", origin),
    "https://api.80feet.app/media/images/sha256/aa/hero.webp",
  );
  assert.equal(resolveBackendOwnedUrl("/landing/hero.webp", origin), "/landing/hero.webp");
});

test("absolute, data, and blob URLs remain unchanged", () => {
  const origin = "https://api.80feet.app";
  for (const value of [
    "https://images.example/hero.webp",
    "http://images.example/hero.webp",
    "data:image/webp;base64,AAAA",
    "blob:https://80feet.app/id",
  ]) {
    assert.equal(resolveBackendOwnedUrl(value, origin), value);
  }
});

test("site URLs use the configured canonical origin", () => {
  assert.equal(
    resolveSiteUrl("property/home%3Aone", "https://80feet.app"),
    "https://80feet.app/property/home%3Aone",
  );
});
