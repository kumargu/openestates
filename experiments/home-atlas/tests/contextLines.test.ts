import assert from "node:assert/strict";
import test from "node:test";

import { buildContextLines } from "../src/contextLines.ts";

const style = {
  strokeColor: "#d5a7ff",
  strokeWidth: 5,
  altitudeMode: "clamp_to_ground" as const,
  drawsOccludedSegments: false,
};

test("context lines preserve OSM ways, remove duplicates and clip distant parts", () => {
  const segment = {
    id: "metro-way",
    path: [
      { lat: 12.98, lng: 77.74 },
      { lat: 12.981, lng: 77.74 },
      { lat: 13.1, lng: 77.74 },
      { lat: 12.982, lng: 77.74 },
      { lat: 12.983, lng: 77.74 },
    ],
  };
  const lines = buildContextLines({
    segments: [segment, segment],
    origin: { lat: 12.98, lng: 77.74 },
    maximumDistanceM: 500,
    style,
  });

  assert.deepEqual(lines.map(({ id }) => id), ["metro-way:0", "metro-way:1"]);
  assert.deepEqual(lines.map(({ path }) => path.length), [2, 2]);
  assert.ok(lines.every((line) => line.style === style));
});

test("context line configuration rejects a non-positive radius", () => {
  assert.throws(() => buildContextLines({
    segments: [],
    origin: { lat: 12.98, lng: 77.74 },
    maximumDistanceM: 0,
    style,
  }), /positive/);
});
