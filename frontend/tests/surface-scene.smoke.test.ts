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
        family: "access",
        renderKind: "pin",
        relationClass: "access",
        enabledByDefault: true,
        rank: 1,
        availableCount: 1,
        shownCount: 1,
        fillState: "filled",
      },
      {
        id: "schools",
        label: "Schools",
        family: "access",
        renderKind: "pin",
        relationClass: "access",
        enabledByDefault: true,
        rank: 2,
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
      {
        id: "around_this_home:metro:access-route",
        entityId: "place:transit-access:sample",
        layerId: "metro",
        kind: "place",
        label: "ECC Road → Kadugodi Tree Park",
        geometry: {
          type: "LineString",
          coordinates: [[77.7409, 12.9814], [77.7475, 12.9855]],
        },
        coordinateQuality: "exact",
        metrics: { distanceM: 1120 },
        display: { tone: "positive", icon: "train", priority: 1 },
        confidence: 0.78,
        receiptIds: ["receipt:access"],
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
      {
        id: "receipt:access",
        entityId: "society:sample",
        factKey: "transit_access_route",
        claim: "ECC Road → Kadugodi Tree Park (1.1 km)",
        sourceType: "OpenStreetMap",
        sourceUrl: "https://www.openstreetmap.org/way/23213668",
        learnedAt: "2026-08-30T00:00:00Z",
        confidence: 0.78,
        scope: "within 1250 m",
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
  assert.equal(context.access_lines?.length, 1);
  assert.equal(context.access_lines?.[0].name, "ECC Road → Kadugodi Tree Park");

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

test("surface scene preserves configured buyer labels for red-flag lines", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:sample",
    servingBundleVersion: "bundle-smoke",
    entityRefs: {
      property_entity_id: "property:sample",
      society_entity_id: "society:sample",
      area_entity_id: "area:whitefield",
      source_entity_ids: [
        "society:sample",
        "place:transmission-line:one",
        "place:transmission-line:two",
      ],
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
        id: "red_flags",
        label: "Red flags",
        family: "risk",
        renderKind: "line",
        relationClass: "risk_externality",
        enabledByDefault: true,
        rank: 1,
        availableCount: 2,
        shownCount: 2,
        fillState: "filled",
      },
    ],
    features: [
      {
        id: "around_this_home:red_flags:place-transmission-line-one",
        entityId: "place:transmission-line:one",
        layerId: "red_flags",
        kind: "line",
        label: "KPTCL",
        shortLabel: "Transmission line",
        details: ["66 kV"],
        geometry: {
          type: "LineString",
          coordinates: [[77.745, 12.94], [77.747, 12.942]],
        },
        coordinateQuality: "exact",
        metrics: { distanceM: 42 },
        display: { tone: "risk", priority: 1 },
        confidence: 0.84,
        receiptIds: ["receipt:line-one"],
      },
      {
        id: "around_this_home:red_flags:place-transmission-line-two",
        entityId: "place:transmission-line:two",
        layerId: "red_flags",
        kind: "line",
        label: "POWERGRID",
        shortLabel: "Transmission line",
        details: ["220 kV"],
        geometry: {
          type: "LineString",
          coordinates: [[77.748, 12.943], [77.75, 12.945]],
        },
        coordinateQuality: "exact",
        metrics: { distanceM: 180 },
        display: { tone: "risk", priority: 1 },
        confidence: 0.82,
        receiptIds: ["receipt:line-two"],
      },
    ],
    relations: [
      {
        fromId: "society:sample",
        toId: "around_this_home:red_flags:place-transmission-line-one",
        edgeType: "has_fact",
        relationClass: "risk_externality",
        direct: true,
        distanceM: 42,
        confidence: 0.84,
        receiptIds: ["receipt:line-one"],
      },
      {
        fromId: "society:sample",
        toId: "around_this_home:red_flags:place-transmission-line-two",
        edgeType: "has_fact",
        relationClass: "risk_externality",
        direct: true,
        distanceM: 180,
        confidence: 0.82,
        receiptIds: ["receipt:line-two"],
      },
    ],
    callouts: [],
    receipts: [
      {
        id: "receipt:line-one",
        entityId: "society:sample",
        factKey: "high_voltage_transmission_line_nearby",
        claim: "KPTCL (42 m, 66 kV, severity: high)",
        sourceType: "OpenStreetMap",
        sourceUrl: "https://www.openstreetmap.org/way/1",
        learnedAt: "2026-07-27T00:00:00Z",
        confidence: 0.84,
        scope: "within 50 m",
      },
      {
        id: "receipt:line-two",
        entityId: "society:sample",
        factKey: "high_voltage_transmission_line_nearby",
        claim: "POWERGRID (180 m, 220 kV, severity: high)",
        sourceType: "OpenStreetMap",
        learnedAt: "2026-07-27T00:00:00Z",
        confidence: 0.82,
        scope: "within 200 m",
      },
    ],
    fillRate: {
      filledLayers: 1,
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
  assert.equal(context.places.length, 0);
  assert.equal(context.red_flag_lines?.length, 2);
  assert.equal(context.red_flag_lines?.[0]?.name, "KPTCL");
  assert.equal(context.red_flag_lines?.[0]?.label, "Transmission line");
  assert.equal(context.red_flag_lines?.[0]?.distance_km, 0.042);
  assert.deepEqual(context.red_flag_lines?.[0]?.details, ["66 kV"]);
  assert.equal(
    context.red_flag_lines?.[0]?.source_url,
    "https://www.openstreetmap.org/way/1",
  );
  assert.equal(context.red_flag_lines?.[1]?.name, "POWERGRID");
  assert.equal(context.red_flag_lines?.[1]?.distance_km, 0.18);
  assert.deepEqual(context.red_flag_lines?.[1]?.details, ["220 kV"]);
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
