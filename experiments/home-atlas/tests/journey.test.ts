import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  blendCamera,
  buildAerialJourney,
  buildAtlasRoute,
  pointAlongRoute,
  projectPointOntoRoute,
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
