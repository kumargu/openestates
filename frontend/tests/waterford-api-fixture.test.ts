import assert from "node:assert/strict";
import test from "node:test";
import propertyDetail from "../fixtures/prestige-waterford-api/property-detail.json" with { type: "json" };
import arrivalStory from "../fixtures/prestige-waterford-api/arrival-story.json" with { type: "json" };
import aroundThisHome from "../fixtures/prestige-waterford-api/around-this-home.json" with { type: "json" };
import manifest from "../fixtures/prestige-waterford-api/manifest.json" with { type: "json" };
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
