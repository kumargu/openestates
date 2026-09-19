import assert from "node:assert/strict";
import test from "node:test";

import { buildContextLines } from "../src/lib/atlas/contextLines.ts";
import { distanceMetres } from "../src/lib/atlas/geometry.ts";
import {
  buildAtlasRoute,
  pointAlongRoute,
  selectPrimaryAtlasRoute,
} from "../src/lib/atlas/journey.ts";
import { buildCategoryTour } from "../src/lib/atlas/scenes.ts";
import type { AtlasCamera, AtlasFeature } from "../src/lib/atlas/types.ts";

test("Atlas line projection keeps disconnected geometry separate and removes duplicates", () => {
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
  const style = {
    strokeColor: "purple",
    strokeWidth: 5,
    altitudeMode: "clamp_to_ground" as const,
    drawsOccludedSegments: false,
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

test("Atlas routes preserve source direction and do not join disconnected lines", () => {
  const short = {
    type: "LineString" as const,
    coordinates: [[77, 12], [77, 12.0001]] as [number, number][],
  };
  const long = {
    type: "LineString" as const,
    coordinates: [[77.1, 12.1], [77.1, 12.101], [77.101, 12.102]] as [number, number][],
  };
  const route = selectPrimaryAtlasRoute([short, long]);
  const reversed = selectPrimaryAtlasRoute([short, long], { direction: "reverse" });

  assert.deepEqual(route.coordinates, long.coordinates);
  assert.deepEqual(reversed.coordinates, [...long.coordinates].reverse());
  assert.deepEqual(pointAlongRoute(route, 0), { latitude: 12.1, longitude: 77.1 });
  assert.deepEqual(pointAlongRoute(route, route.lengthM), {
    latitude: 12.102,
    longitude: 77.101,
  });
  assert.throws(() => selectPrimaryAtlasRoute([]), /continuous LineString/);
  assert.equal(distanceMetres({ lat: 12.1, lng: 77.1 }, { lat: 12.1, lng: 77.1 }), 0);
  assert.ok(buildAtlasRoute(long).lengthM > buildAtlasRoute(short).lengthM);
});

test("Atlas category tours keep a deterministic scene and timing contract", () => {
  const camera: AtlasCamera = {
    center: { lat: 12.98, lng: 77.74, altitude: 920 },
    heading: 135,
    range: 700,
    tilt: 55,
  };
  const feature = (id: string, segments = false): AtlasFeature => ({
    id,
    categoryId: "metro",
    name: id,
    position: { lat: 12.98, lng: 77.74 },
    segments: segments ? [{ id: `${id}:line`, path: [{ lat: 12.98, lng: 77.74 }, { lat: 12.99, lng: 77.75 }] }] : undefined,
    evidence: {
      location: { providerId: "test" },
      distance: { metres: 100, method: "straight_line", target: "place_point" },
    },
  });
  const home = feature("home");
  const features = [feature("station-a"), feature("station-b", true)];
  const scenes = buildCategoryTour({
    categoryId: "metro",
    home,
    features,
    cameras: {
      group: () => camera,
      pair: () => camera,
      focus: () => camera,
      segment: () => camera,
      home: () => camera,
    },
    copy: {
      overview: () => "Metro",
      pair: ({ name }) => name,
      focus: ({ name }) => name,
      returnHome: () => "Home",
    },
    timing: {
      overviewMs: 10,
      pairMs: 20,
      focusMs: 30,
      segmentMs: 40,
      returnHomeMs: 50,
    },
  });

  assert.deepEqual(scenes.map(({ phase }) => phase), [
    "overview",
    "pair",
    "inspect",
    "pair",
    "inspect",
    "segment",
    "home",
  ]);
  assert.deepEqual(scenes.map(({ durationMs }) => durationMs), [10, 20, 30, 20, 30, 40, 50]);
  assert.equal(scenes[0].visibility.mode, "category");
  assert.equal(scenes.at(-1)?.visibility.mode, "home");
});
