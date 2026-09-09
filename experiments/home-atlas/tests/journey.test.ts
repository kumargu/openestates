import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  advanceRoadDistance,
  blendCamera,
  buildAerialJourney,
  buildAtlasRoute,
  clampRoadPlaybackRate,
  pointAlongRoute,
  projectPointOntoRoute,
  projectStreetHandoff,
  roadFlightCamera,
  selectPrimaryAtlasRoute,
} from "../src/journey.ts";

type InventoryPoint = { lat: number; lng: number };
type InventoryFeature = {
  id: string;
  name: string | null;
  center: InventoryPoint;
  geometry: InventoryPoint[];
};
type BrigadeInventory = { features: InventoryFeature[] };

const inventory = JSON.parse(readFileSync(
  new URL("../prototype/web/brigade/inventory.json", import.meta.url),
  "utf8",
)) as BrigadeInventory;
const spinalRoad = inventory.features
  .filter((feature) => feature.name === "Brigade Orchards Spinal Road")
  .sort((left, right) => right.geometry.length - left.geometry.length)[0];
const stopIds = [
  "way/843807779",
  "way/843807778",
  "way/843854110",
  "way/843854109",
  "way/1297658656",
  "way/843854056",
  "way/843854052",
];

test("route geometry preserves OSM direction and projects stops exactly", () => {
  assert.ok(spinalRoad);
  const coordinates = spinalRoad.geometry.map(({ lat, lng }): [number, number] => [lng, lat]);
  const route = buildAtlasRoute({ type: "LineString", coordinates });

  assert.deepEqual(pointAlongRoute(route, 0), {
    latitude: spinalRoad.geometry[0].lat,
    longitude: spinalRoad.geometry[0].lng,
  });
  assert.deepEqual(pointAlongRoute(route, route.lengthM), {
    latitude: spinalRoad.geometry.at(-1)?.lat,
    longitude: spinalRoad.geometry.at(-1)?.lng,
  });
  const midpoint = pointAlongRoute(route, route.lengthM / 2);
  assert.ok(projectPointOntoRoute(route, midpoint).distanceFromRouteM < 0.001);
});

test("the Brigade journey is monotonic and has no timeline gaps", () => {
  const route = buildAtlasRoute({
    type: "LineString",
    coordinates: spinalRoad.geometry.map(({ lat, lng }) => [lng, lat]),
  });
  const stops = stopIds.map((id) => {
    const feature = inventory.features.find((candidate) => candidate.id === id);
    assert.ok(feature, `missing fixture stop ${id}`);
    return {
      id,
      point: { latitude: feature.center.lat, longitude: feature.center.lng },
    };
  });
  const scenes = buildAerialJourney(route, stops);

  assert.equal(scenes.filter((scene) => scene.kind === "focus").length, stopIds.length);
  for (let index = 1; index < scenes.length; index += 1) {
    assert.equal(scenes[index].startMs, scenes[index - 1].endMs);
    assert.ok(scenes[index].endMs > scenes[index].startMs);
  }
  const corridorDistances = scenes
    .filter((scene) => scene.kind === "arrival" || scene.kind === "corridor")
    .map((scene) => scene.toM ?? 0);
  assert.deepEqual(corridorDistances, [...corridorDistances].sort((left, right) => left - right));
});

test("camera blending takes the shortest path across north", () => {
  const camera = (heading: number) => ({
    center: { latitude: 12.98, longitude: 77.74, altitude: 920 },
    heading,
    range: 500,
    tilt: 55,
  });

  assert.equal(blendCamera(camera(359), camera(1), 0.5).heading, 0);
});


test("road selection keeps disconnected lines separate and direction explicit", () => {
  const geometries = [
    { type: "LineString" as const, coordinates: [[77, 12], [77, 12.0001]] as [number, number][] },
    {
      type: "LineString" as const,
      coordinates: [[77.1, 12.1], [77.1, 12.101], [77.101, 12.102]] as [number, number][],
    },
  ];
  const mapped = selectPrimaryAtlasRoute(geometries);
  const reversed = selectPrimaryAtlasRoute(geometries, { direction: "reverse" });

  assert.equal(mapped.coordinates.length, 3);
  assert.deepEqual(mapped.coordinates[0], [77.1, 12.1]);
  assert.deepEqual(reversed.coordinates[0], [77.101, 12.102]);
  assert.equal(mapped.coordinates.some(([longitude]) => longitude === 77), false);
  assert.throws(() => selectPrimaryAtlasRoute([]), /continuous LineString/);
});

test("road progression is elapsed-time based and clamps the speed lever", () => {
  const route = buildAtlasRoute({
    type: "LineString",
    coordinates: [[77, 12], [77, 12.01]],
  });

  assert.equal(clampRoadPlaybackRate(0.1), 0.5);
  assert.equal(clampRoadPlaybackRate(3), 2);
  assert.equal(advanceRoadDistance(route, 0, 1_000, 0.5), 6);
  assert.equal(advanceRoadDistance(route, 0, 1_000, 1), 12);
  assert.equal(advanceRoadDistance(route, 0, 1_000, 2), 24);
  assert.equal(advanceRoadDistance(route, route.lengthM - 1, 1_000, 2), route.lengthM);
});

test("road camera keeps the accepted aerial framing responsive", () => {
  const route = buildAtlasRoute({
    type: "LineString",
    coordinates: [[77, 12], [77.001, 12.001]],
  });
  const desktop = roadFlightCamera(route, 30, 900, 1_200);
  const mobile = roadFlightCamera(route, 30, 900, 500);

  assert.equal(desktop.center.altitude, 908);
  assert.equal(desktop.range, 270);
  assert.equal(desktop.tilt, 67);
  assert.equal(mobile.range, 350);
  assert.ok(Number.isFinite(desktop.heading));
});

test("Street View handoff projects back onto the aerial route", () => {
  const route = buildAtlasRoute({
    type: "LineString",
    coordinates: [[77, 12], [77, 12.01]],
  });
  const routePoint = pointAlongRoute(route, 40);
  const handoff = projectStreetHandoff(route, routePoint, 370);

  assert.ok(Math.abs(handoff.distanceAlongM - 40) < 0.001);
  assert.ok(handoff.distanceFromRouteM < 0.001);
  assert.equal(handoff.heading, 10);
  assert.deepEqual(handoff.routePoint, routePoint);
});
