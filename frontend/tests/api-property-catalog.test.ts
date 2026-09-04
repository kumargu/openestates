import assert from "node:assert/strict";
import test from "node:test";

let fetchCount = 0;
let resolveInitialFetch: ((response: Response) => void) | undefined;

Object.defineProperty(globalThis, "fetch", {
  value: () => {
    fetchCount += 1;
    if (fetchCount === 1) {
      return new Promise<Response>((resolve) => {
        resolveInitialFetch = resolve;
      });
    }
    const id = fetchCount === 2 ? "new-home" : "newest-home";
    return Promise.resolve(Response.json([{ id, price: 12_000_000 }]));
  },
  configurable: true,
});

const { getProperties } = await import("../src/lib/api.ts");

test("a runtime refresh bypasses the cached and in-flight property catalog", async () => {
  const initialRequest = getProperties();
  const refreshed = await getProperties({ refresh: true });
  resolveInitialFetch?.(Response.json([{ id: "old-home", price: 10_000_000 }]));
  const initial = await initialRequest;
  const refreshedCache = await getProperties();
  const newest = await getProperties({ refresh: true });

  assert.deepEqual(initial.map((property) => property.id), ["old-home"]);
  assert.deepEqual(refreshed.map((property) => property.id), ["new-home"]);
  assert.strictEqual(refreshedCache, refreshed);
  assert.deepEqual(newest.map((property) => property.id), ["newest-home"]);
  assert.equal(fetchCount, 3);
});
