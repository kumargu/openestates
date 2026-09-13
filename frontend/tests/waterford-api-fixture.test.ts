import assert from "node:assert/strict";
import test from "node:test";
import { getFixtureResponse } from "../src/lib/dev-fixtures.ts";
import {
  getWaterfordApiFixtureResponse,
  waterfordFixturePropertyId,
  waterfordFixtureServingBundleVersion,
} from "../src/lib/waterford-api-fixtures.ts";
import type {
  PropertyDetailResponse,
  SurfaceSceneResponse,
} from "../src/lib/types.ts";

const propertyPath = `/api/properties/${waterfordFixturePropertyId}`;

function scene(surfaceId: string): SurfaceSceneResponse {
  const response = getWaterfordApiFixtureResponse(
    `${propertyPath}/surfaces/${surfaceId}`,
  );
  assert.ok(response);
  return response as SurfaceSceneResponse;
}

test("Waterford fixture exposes the production-shaped property response", () => {
  const response = getFixtureResponse(propertyPath) as PropertyDetailResponse;

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

test("Waterford fixture ignores focus query data without changing snapshots", () => {
  const plain = scene("around_this_home");
  const focused = getFixtureResponse(
    `${propertyPath}/surfaces/around_this_home?focus=${encodeURIComponent("{}")}`,
  );

  assert.deepEqual(focused, plain);
  assert.equal(
    getWaterfordApiFixtureResponse(`${propertyPath}/surfaces/not_configured`),
    null,
  );
});
