import assert from "node:assert/strict";
import test from "node:test";
import { fittedRailPageSize } from "../src/lib/rail-pagination.ts";

test("fitted rail page size keeps only complete cards", () => {
  assert.equal(fittedRailPageSize(1180), 5);
  assert.equal(fittedRailPageSize(720), 3);
  assert.equal(fittedRailPageSize(1180, { compact: true }), 1);
  assert.equal(fittedRailPageSize(0), 1);
});

test("wider rails can fit six complete cards without a peek", () => {
  assert.equal(fittedRailPageSize(1400), 6);
});
