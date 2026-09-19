import assert from "node:assert/strict";
import test from "node:test";
import propertyDetail from "../fixtures/prestige-waterford-api/property-detail.json" with { type: "json" };
import arrivalStory from "../fixtures/prestige-waterford-api/arrival-story.json" with { type: "json" };
import aroundThisHome from "../fixtures/prestige-waterford-api/around-this-home.json" with { type: "json" };
import manifest from "../fixtures/prestige-waterford-api/manifest.json" with { type: "json" };
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import { homeSceneCamera, homeOrbitCamera } from "../src/lib/atlasNearbyScene.ts";
import { resolveHomeAnchor } from "../src/lib/nearbyPlateProjection.ts";
import { projectCameraPointToScreen } from "../src/lib/atlas/screenFit.ts";
import atlasPolicy from "../src/lib/atlasPolicy.ts";
import type {
  PropertyDetailResponse,
  SurfaceSceneResponse,
} from "../src/lib/types.ts";

const waterfordFixturePropertyId = "discovered-prestige-waterford-3bhk";
const waterfordFixtureServingBundleVersion =
  "catalog-71-ffb4dc50-117e-453c-b26f-41822430324e";
const propertyPath = `/api/properties/${waterfordFixturePropertyId}`;
const detail = propertyDetail as unknown as PropertyDetailResponse;
const scenes = {
  arrival_story: arrivalStory as unknown as SurfaceSceneResponse,
  around_this_home: aroundThisHome as unknown as SurfaceSceneResponse,
};

function scene(surfaceId: keyof typeof scenes): SurfaceSceneResponse {
  return structuredClone(scenes[surfaceId]);
}

test("Waterford fixture exposes the production-shaped property response", () => {
  const response = structuredClone(detail);

  assert.equal(response.property.id, waterfordFixturePropertyId);
  assert.equal(
    response.entity_refs.society_entity_id,
    "society:prestige-waterford",
  );
  assert.equal(
    response.evidence.serving_bundle_version,
    waterfordFixtureServingBundleVersion,
  );
  assert.equal(response.map_context.places.length, 41);
  assert.equal(response.map_context.lakes?.length, 7);
});

test("Waterford manifest maps every property-page request to a snapshot", () => {
  assert.equal(manifest.property_id, waterfordFixturePropertyId);
  assert.equal(
    manifest.serving_bundle_version,
    waterfordFixtureServingBundleVersion,
  );
  assert.deepEqual(
    manifest.responses.map((response) => response.endpoint),
    [
      propertyPath,
      `${propertyPath}/surfaces/arrival_story`,
      `${propertyPath}/surfaces/around_this_home`,
    ],
  );
});

test("Waterford fixture keeps arrival and nearby scenes on one bundle", () => {
  const arrival = scene("arrival_story");
  const nearby = scene("around_this_home");

  for (const response of [arrival, nearby]) {
    assert.equal(response.propertyId, waterfordFixturePropertyId);
    assert.equal(
      response.servingBundleVersion,
      waterfordFixtureServingBundleVersion,
    );
  }

  assert.equal(arrival.anchor.boundary?.geometry.type, "Polygon");
  assert.equal(arrival.features.length, 4);
  assert.equal(arrival.relations.length, 4);
  assert.equal(nearby.features.length, 46);
  assert.equal(nearby.relations.length, 46);
});

test("composed property scenes retain every API metro segment when surfaces contain only stations", () => {
  const nearby = propertyMapContextFromSurfaceScene(scene("around_this_home"), detail.map_context);
  const arrival = propertyMapContextFromSurfaceScene(scene("arrival_story"), nearby)!;
  const expected = detail.map_context.metro_lines!;
  assert.ok(expected.length > 0);
  assert.deepEqual(arrival.layer_lines?.metro, expected);
  assert.deepEqual(arrival.metro_lines, expected);
  assert.equal(new Set(arrival.layer_lines?.metro.map(line => line.id)).size, expected.length);
});

test("captured home boundary fits desktop chrome and camera altitude follows supplied terrain", () => {
  const context = propertyMapContextFromSurfaceScene(scene("arrival_story"))!;
  const home = { ...resolveHomeAnchor(context)!, name: context.home.name, boundary: context.home.boundary };
  for (const frame of [
    { width: 1166, height: 900, left: 32, right: 112, top: 226, bottom: 32 },
    { width: 1046, height: 620, left: 32, right: 464, top: 226, bottom: 32 },
  ]) {
    const camera = homeSceneCamera(home, 0, frame);
    for (const [lng, lat] of home.boundary!.coordinates) {
      const point = projectCameraPointToScreen(camera, { lat, lng }, frame, camera.fov);
      assert.ok(point.x >= frame.left && point.x <= frame.width - frame.right);
      assert.ok(point.y >= frame.top && point.y <= frame.height - frame.bottom);
    }
    // A synthetic elevation tests translation only; it is not a Waterford ground claim.
    const elevated = homeSceneCamera(home, 123, frame);
    assert.equal(elevated.center.altitude, 123 + atlasPolicy.cameraFit.homeCenterAltitudeOffsetM);
    assert.equal(elevated.range, camera.range);
  }
  const orbit = homeOrbitCamera(home, 123, {
    width: 1166,
    height: 900,
    left: 32,
    right: 112,
    top: 226,
    bottom: 32,
  });
  assert.equal(orbit.center.altitude, 123 + atlasPolicy.cameraFit.homeCenterAltitudeOffsetM);
  assert.equal(orbit.fov, atlasPolicy.cameraFit.homeFieldOfViewDegrees);
});

test("Waterford nearby snapshot preserves points and exact lake footprints", () => {
  const nearby = scene("around_this_home");
  const lakes = nearby.features.filter((feature) => feature.layerId === "lakes");
  const points = nearby.features.filter(
    (feature) => feature.geometry.type === "Point",
  );

  assert.equal(points.length, 41);
  assert.equal(lakes.length, 5);
  assert.ok(lakes.every((feature) => feature.kind === "lake"));
  assert.ok(lakes.every((feature) => feature.geometry.type === "Polygon"));
  assert.equal(
    nearby.layers.find((layer) => layer.id === "lakes")?.availableCount,
    7,
  );
  assert.equal(
    nearby.layers.find((layer) => layer.id === "lakes")?.shownCount,
    5,
  );
});


test("composed scenes list each canonical station once despite different scene feature IDs", () => {
  const nearby = propertyMapContextFromSurfaceScene(scene("around_this_home"), detail.map_context)!;
  const arrival = propertyMapContextFromSurfaceScene(scene("arrival_story"), nearby)!;
  const expected = detail.map_context.places.filter(place => place.layer === 'metro');
  const stations = arrival.places.filter(place => place.layer === 'metro');
  assert.equal(stations.length, expected.length);
  assert.deepEqual(stations.map(place => place.place_entity_id).sort(), expected.map(place => place.place_entity_id).sort());
  assert.ok(stations.every(place => place.feature_id?.startsWith('arrival_story:')));
  const distinct = {...stations[0], place_entity_id: 'place:distinct-station', feature_id: 'other:station'};
  const otherLayer = {...stations[0], layer: 'another-layer', feature_id: 'other:layer'};
  const merged = propertyMapContextFromSurfaceScene(scene("arrival_story"),
    {...nearby, places: [...nearby.places, distinct, otherLayer]})!;
  assert.ok(merged.places.some(place => place.place_entity_id === distinct.place_entity_id));
  assert.ok(merged.places.some(place => place.layer === otherLayer.layer));
});
