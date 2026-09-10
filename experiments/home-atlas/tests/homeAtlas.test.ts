import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { groupCamera, pairCamera, placeCamera, segmentCamera } from "../src/camera.ts";
import { distanceMetres, footprintCircle, nearestSegmentAnchor } from "../src/geometry.ts";
import { buildCategoryTour } from "../src/scenes.ts";
import { categoryFeatures, numberedCategoryFeatures, visibleFeaturesForScene } from "../src/selection.ts";
import type { AtlasCamera, AtlasDocument } from "../src/types.ts";

const fixturePath = fileURLToPath(new URL("../fixtures/waterford.sample.json", import.meta.url));
const document = JSON.parse(readFileSync(fixturePath, "utf8")) as AtlasDocument;
const home = document.features.find((feature) => feature.id === document.homeId);
if (!home) throw new Error("Fixture home is missing");

function assertCamera(camera: AtlasCamera): void {
  assert([camera.center.lat, camera.center.lng, camera.center.altitude, camera.heading, camera.tilt, camera.range].every(Number.isFinite));
  assert(camera.range > 0);
}

test("feature identity, source ownership, and boundaries remain explicit", () => {
  assert.equal(new Set(document.features.map((feature) => feature.id)).size, document.features.length);
  for (const feature of document.features) {
    assert(feature.evidence.location.providerId);
    assert(feature.evidence.distance.metres >= 0);
    if (feature.boundary) assert.deepEqual(feature.boundary[0], feature.boundary.at(-1));
  }
  assert.equal(home.evidence.geometry?.providerId, "openstreetmap");
  assert.equal(home.evidence.location.providerId, "google-places");
});

test("categories are stable, distance ordered, and share numbers with the map", () => {
  const metros = categoryFeatures(document, "metro");
  assert.deepEqual(metros.map((feature) => feature.id), ["metro-kadugodi-tree-park", "metro-pattandur-agrahara"]);
  assert.deepEqual(numberedCategoryFeatures(document, "metro").map(({ number }) => number), ["01", "02"]);
});

test("scene visibility distinguishes group, pair, and home", () => {
  const metroOverview = {
    id: "metro:overview", targetFeatureId: home.id,
    camera: placeCamera({ home, defaultElevationM: home.elevationM ?? 0, viewportWidthPx: 1280 }, home),
    visibility: { mode: "category" as const, categoryId: "metro" }, caption: "Metro", durationMs: 1000,
  };
  assert.deepEqual(visibleFeaturesForScene(document, metroOverview).map((feature) => feature.id), [
    home.id, "metro-kadugodi-tree-park", "metro-pattandur-agrahara",
  ]);
  const pair = { ...metroOverview, visibility: { mode: "pair" as const, featureId: "metro-pattandur-agrahara" } };
  assert.deepEqual(visibleFeaturesForScene(document, pair).map((feature) => feature.id), [home.id, "metro-pattandur-agrahara"]);
});

test("road descent uses an actual segment and responsive cameras stay finite", () => {
  const road = document.features.find((feature) => feature.id === "road-ecc");
  if (!road) throw new Error("Fixture road is missing");
  const anchor = nearestSegmentAnchor(home.position, road);
  assert(anchor);
  assert.equal(anchor.segmentId, "23213668");
  assert(anchor.distanceM < road.evidence.distance.metres);
  assert.equal(distanceMetres(home.position, home.position), 0);
  assert.equal(footprintCircle(home, 100).length, 49);
  for (const viewportWidthPx of [375, 1280]) {
    const context = { home, defaultElevationM: home.elevationM ?? 0, viewportWidthPx };
    [placeCamera(context, home), pairCamera(context, road), groupCamera(context, [road]), segmentCamera(context, road)].forEach(assertCamera);
  }
});

test("category tours declare visible context and return home", () => {
  const metros = categoryFeatures(document, "metro");
  const context = { home, defaultElevationM: home.elevationM ?? 0, viewportWidthPx: 1280 };
  const scenes = buildCategoryTour({
    categoryId: "metro", home, features: metros,
    cameras: {
      group: (features) => groupCamera(context, features), pair: (feature) => pairCamera(context, feature),
      focus: (feature) => placeCamera(context, feature), home: () => placeCamera(context, home),
    },
    copy: {
      overview: (features) => `${features.length} metro stations`, pair: (feature) => `${feature.name} with home`,
      focus: (feature) => feature.name, returnHome: () => "Back home",
    },
  });
  assert.equal(scenes.length, 6);
  assert.equal(scenes[0].visibility.mode, "category");
  assert.equal(scenes.at(-1)?.visibility.mode, "home");
  scenes.forEach((scene) => assertCamera(scene.camera));
});

