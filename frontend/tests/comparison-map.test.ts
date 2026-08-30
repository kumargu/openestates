import assert from "node:assert/strict";
import test from "node:test";
import { nearestComparisonHomes } from "../src/lib/nearbyPlateProjection.ts";
import type { MapComparisonHome } from "../src/lib/types.ts";

function candidate(id: string, latitude: number): MapComparisonHome {
  return {
    id,
    name: id,
    latitude,
    longitude: 77.75,
    href: `/property/${id}`,
  };
}

test("comparison map keeps the four closest query-ranked societies", () => {
  const matches = nearestComparisonHomes(
    { latitude: 12.98, longitude: 77.75 },
    [
      candidate("far", 13.08),
      candidate("near-a", 12.981),
      candidate("near-b", 12.982),
      candidate("near-c", 12.983),
      candidate("near-d", 12.984),
      candidate("near-e", 12.985),
    ],
  );

  assert.deepEqual(matches.map((match) => match.id), [
    "near-a",
    "near-b",
    "near-c",
    "near-d",
  ]);
});
