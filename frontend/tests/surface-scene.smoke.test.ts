import assert from "node:assert/strict";
import test from "node:test";

import {
  availableLayers,
  buildNumberedPlaces,
  hasAroundThisHomePlate,
  placesForStory,
  resolveHomeAnchor,
} from "../src/lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import type { SurfaceSceneResponse } from "../src/lib/types.ts";

test("surface scene payload can drive the around-this-home plate", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:sample",
    servingBundleVersion: "bundle-smoke",
    entityRefs: {
      property_entity_id: "property:sample",
      society_entity_id: "society:sample",
      area_entity_id: "area:whitefield",
      source_entity_ids: ["society:sample", "place:metro", "place:school"],
    },
    anchor: {
      entityId: "society:sample",
      label: "Sample Society",
      area: "Whitefield",
      geometry: { type: "Point", coordinates: [77.75, 12.98] },
      coordinateQuality: "exact",
    },
    viewport: {
      center: [77.751, 12.981],
      radiusM: 3000,
    },
    layers: [
      {
        id: "metro",
        label: "Metro",
        icon: "train",
        tone: "positive",
        family: "access",
        renderKind: "pin",
        relationClass: "access",
        scaleMode: "nearby",
        priority: 1,
        showReviewMetrics: true,
        enabledByDefault: true,
        rank: 1,
        features: ["around_this_home:metro:place-metro"],
        availableCount: 1,
        shownCount: 1,
        fillState: "filled",
      },
      {
        id: "schools",
        label: "Schools",
        icon: "graduation-cap",
        tone: "positive",
        family: "access",
        renderKind: "pin",
        relationClass: "access",
        scaleMode: "nearby",
        priority: 2,
        showReviewMetrics: true,
        enabledByDefault: true,
        rank: 2,
        features: ["around_this_home:schools:place-school"],
        availableCount: 1,
        shownCount: 1,
        fillState: "filled",
      },
    ],
    features: [
      {
        id: "around_this_home:metro:place-metro",
        entityId: "place:metro",
        layerId: "metro",
        kind: "place",
        label: "Kadugodi Tree Park",
        geometry: { type: "Point", coordinates: [77.751, 12.981] },
        coordinateQuality: "exact",
        metrics: { distanceM: 700, rating: 4.4, reviewCount: 521 },
        display: { tone: "positive", icon: "train", priority: 1 },
        confidence: 0.86,
        receiptIds: ["receipt:metro"],
      },
      {
        id: "around_this_home:schools:place-school",
        entityId: "place:school",
        layerId: "schools",
        kind: "place",
        label: "Green School",
        geometry: { type: "Point", coordinates: [77.752, 12.982] },
        coordinateQuality: "exact",
        metrics: { distanceM: 950, rating: 4.2, reviewCount: 120 },
        display: { tone: "positive", icon: "graduation-cap", priority: 2 },
        confidence: 0.8,
        receiptIds: ["receipt:school"],
      },
    ],
    relations: [
      {
        fromId: "society:sample",
        toId: "around_this_home:metro:place-metro",
        edgeType: "near_place",
        relationClass: "access",
        direct: true,
        distanceM: 700,
        confidence: 0.86,
        receiptIds: ["receipt:metro"],
      },
    ],
    callouts: [],
    receipts: [
      {
        id: "receipt:metro",
        entityId: "society:sample",
        factKey: "nearby_metro_stations",
        claim: "Kadugodi Tree Park",
        sourceType: "Computed",
        learnedAt: "2026-07-27T00:00:00Z",
        confidence: 0.86,
        scope: "within 700 m",
      },
      {
        id: "receipt:school",
        entityId: "society:sample",
        factKey: "nearby_schools",
        claim: "Green School",
        sourceType: "Google",
        sourceUrl: "https://maps.example/school",
        learnedAt: "2026-07-27T00:00:00Z",
        confidence: 0.8,
        scope: "within 950 m",
      },
    ],
    fillRate: {
      filledLayers: 2,
      partialLayers: 0,
      emptyLayers: 0,
      shownFeatures: 2,
      availableFeatures: 2,
      value: 1,
    },
    gaps: [],
  };

  const context = propertyMapContextFromSurfaceScene(scene);
  assert.ok(context);
  assert.equal(hasAroundThisHomePlate(context), true);
  assert.deepEqual(availableLayers(context), ["metro", "schools"]);
  assert.equal(context.layers?.[0]?.icon, "train");
  assert.equal(context.layers?.[0]?.scaleMode, "nearby");
  assert.deepEqual(context.layers?.[0]?.features, ["around_this_home:metro:place-metro"]);

  const home = resolveHomeAnchor(context);
  assert.deepEqual(home, {
    latitude: 12.98,
    longitude: 77.75,
    approximated: false,
  });

  const metro = placesForStory(context, { kind: "layer", layer: "metro" });
  const numbered = buildNumberedPlaces(metro);
  assert.equal(numbered.length, 1);
  assert.equal(numbered[0].name, "Kadugodi Tree Park");
  assert.equal(numbered[0].distance_km, 0.7);
});

test("surface scene line features do not break legacy point projection", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "flooding",
    propertyId: "property:sample",
    servingBundleVersion: "bundle-smoke",
    entityRefs: {
      property_entity_id: "property:sample",
      society_entity_id: "society:sample",
      area_entity_id: "area:whitefield",
      source_entity_ids: ["society:sample", "place:stormwater-drain:one"],
    },
    anchor: {
      entityId: "society:sample",
      label: "Sample Society",
      area: "Whitefield",
      geometry: { type: "Point", coordinates: [77.745, 12.94] },
      coordinateQuality: "exact",
    },
    viewport: {
      center: [77.746, 12.941],
      radiusM: 3000,
    },
    layers: [
      {
        id: "drains",
        label: "Drains",
        icon: "flag",
        tone: "risk",
        family: "risk",
        renderKind: "line",
        relationClass: "risk_externality",
        scaleMode: "area",
        priority: 1,
        showReviewMetrics: false,
        enabledByDefault: true,
        rank: 1,
        features: ["flooding:drains:place-stormwater-drain-one"],
        availableCount: 1,
        shownCount: 1,
        fillState: "filled",
      },
    ],
    features: [
      {
        id: "flooding:drains:place-stormwater-drain-one",
        entityId: "place:stormwater-drain:one",
        layerId: "drains",
        kind: "line",
        label: "Varthur Rajakaluve",
        geometry: {
          type: "LineString",
          coordinates: [[77.745, 12.94], [77.747, 12.942]],
        },
        coordinateQuality: "exact",
        metrics: { distanceM: 42 },
        display: { tone: "risk", priority: 1 },
        confidence: 0.84,
        receiptIds: ["receipt:drain"],
      },
    ],
    relations: [
      {
        fromId: "society:sample",
        toId: "flooding:drains:place-stormwater-drain-one",
        edgeType: "has_fact",
        relationClass: "risk_externality",
        direct: true,
        distanceM: 42,
        confidence: 0.84,
        receiptIds: ["receipt:drain"],
      },
    ],
    callouts: [],
    receipts: [
      {
        id: "receipt:drain",
        entityId: "society:sample",
        factKey: "stormwater_drain_nearby",
        claim: "Varthur Rajakaluve (42 m, severity: high)",
        sourceType: "OpenCity",
        learnedAt: "2026-07-27T00:00:00Z",
        confidence: 0.84,
        scope: "within 50 m",
      },
    ],
    fillRate: {
      filledLayers: 1,
      partialLayers: 0,
      emptyLayers: 0,
      shownFeatures: 1,
      availableFeatures: 1,
      value: 1,
    },
    gaps: [],
  };

  const context = propertyMapContextFromSurfaceScene(scene);
  assert.ok(context);
  assert.equal(context.places.length, 0);
  assert.deepEqual(resolveHomeAnchor(context), {
    latitude: 12.94,
    longitude: 77.745,
    approximated: false,
  });
});

test("map projection does not silently cap backend-selected places at five", () => {
  const places = Array.from({ length: 6 }, (_, index) => ({
    layer: "tech",
    name: `Tech Park ${index + 1}`,
    latitude: 12.98 + index * 0.001,
    longitude: 77.75 + index * 0.001,
    distance_km: index + 1,
    source_type: "Google",
  }));

  assert.equal(buildNumberedPlaces(places).length, 6);
  assert.equal(buildNumberedPlaces(places, 5).length, 5);
});
