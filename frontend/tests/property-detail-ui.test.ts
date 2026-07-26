import assert from "node:assert/strict";
import test from "node:test";
import { hasAroundThisHomePlate } from "../src/lib/nearbyPlateProjection.ts";
import { resolveBuyerProjectStatus } from "../src/lib/projectStatus.ts";
import { propertySceneImageAt } from "../src/lib/propertyScene.ts";
import type { PropertyMapContext } from "../src/lib/types.ts";

const emptyMapContext: PropertyMapContext = {
  home: {
    entity_id: "society:test",
    name: "Test society",
  },
  places: [],
};

test("around-this-home stays hidden without usable context", () => {
  assert.equal(hasAroundThisHomePlate(null), false);
  assert.equal(hasAroundThisHomePlate(emptyMapContext), false);
});

test("around-this-home accepts places, water, or metro evidence", () => {
  assert.equal(hasAroundThisHomePlate({
    ...emptyMapContext,
    places: [{
      layer: "hospitals",
      name: "Clinic",
      source_type: "google",
    }],
  }), true);
  assert.equal(hasAroundThisHomePlate({
    ...emptyMapContext,
    water: {
      groundwater_class: "safe",
      summary: "Within expected range",
      source_type: "government",
      illustrative_zone: false,
    },
  }), true);
  assert.equal(hasAroundThisHomePlate({
    ...emptyMapContext,
    metro_lines: [{
      id: "purple",
      name: "Purple Line",
      kind: "metro",
      coordinates: [[77.7, 12.9], [77.71, 12.91]],
      source_type: "open-data",
    }],
  }), true);
});

test("project status exposes only known buyer labels", () => {
  assert.deepEqual(resolveBuyerProjectStatus({ status: "ready_to_move" }), {
    key: "ready_to_move",
    label: "Ready to move",
  });
  assert.deepEqual(resolveBuyerProjectStatus({
    possessionStatus: "Under construction",
    displayText: "Delivery expected 2028",
  }), {
    key: "under_construction",
    label: "Delivery expected 2028",
  });
  assert.deepEqual(resolveBuyerProjectStatus({ possessionStatus: "ready" }), {
    key: "ready_to_move",
    label: "Ready to move",
  });
  assert.equal(resolveBuyerProjectStatus({ status: "ready" }), null);
  assert.equal(resolveBuyerProjectStatus({ status: "Under construction" }), null);
  assert.equal(resolveBuyerProjectStatus({ possessionStatus: "approved" }), null);
  assert.equal(resolveBuyerProjectStatus({ status: "constructor" }), null);
});

test("recommendation scenes are stable and wrap after exhaustion", () => {
  const scenes = ["one.jpg", "two.jpg", "three.jpg"];
  assert.equal(propertySceneImageAt(scenes, 0), "one.jpg");
  assert.equal(propertySceneImageAt(scenes, 1), "two.jpg");
  assert.equal(propertySceneImageAt(scenes, 4), "two.jpg");
  assert.equal(propertySceneImageAt(scenes, -1), "one.jpg");
  assert.equal(propertySceneImageAt([], 2, "fallback.jpg"), "fallback.jpg");
});
