import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import type {
  SceneFeature,
  SceneGeometry,
  SceneLayer,
} from "../../../frontend/src/lib/types.ts";
import {
  resolveAtlasPresentation,
  type AtlasSceneInput,
} from "../src/presentation.ts";

type InventoryPoint = { lat: number; lng: number };
type InventoryFeature = {
  id: string;
  category: string;
  name: string | null;
  geometry: InventoryPoint[];
};
type BrigadeInventory = {
  site: {
    id: string;
    name: string;
    boundary: InventoryPoint[];
    bounds: {
      west: number;
      south: number;
      east: number;
      north: number;
    };
  };
  features: InventoryFeature[];
};

function sceneLayer(id: string, renderKind: string): SceneLayer {
  return {
    id,
    label: id,
    family: "context",
    renderKind,
    relationClass: "context",
    enabledByDefault: true,
    rank: 1,
    availableCount: 0,
    shownCount: 0,
    fillState: "filled",
  };
}

function sceneFeature(feature: InventoryFeature): SceneFeature | null {
  const layerId = `${feature.category}s`;
  const coordinates = feature.geometry.map(({ lat, lng }): [number, number] => [lng, lat]);
  const geometry: SceneGeometry = feature.category === "road"
    ? { type: "LineString", coordinates }
    : feature.category === "precinct" || feature.category === "building"
    ? { type: "Polygon", coordinates: [coordinates] }
    : coordinates[0]
    ? { type: "Point", coordinates: coordinates[0] }
    : { type: "Point", coordinates: [0, 0] };
  if (feature.category === "road" && coordinates.length < 2) return null;

  return {
    id: feature.id,
    layerId,
    kind: feature.category,
    label: feature.name ?? feature.category,
    geometry,
    coordinateQuality: "exact",
    display: { tone: "neutral", priority: 1 },
    confidence: 1,
    receiptIds: [],
  };
}

function brigadeScene(): AtlasSceneInput {
  const inventory = JSON.parse(readFileSync(
    new URL("../prototype/web/brigade/inventory.json", import.meta.url),
    "utf8",
  )) as BrigadeInventory;
  return {
    anchor: {
      entityId: inventory.site.id,
      label: inventory.site.name,
      coordinateQuality: "exact",
      boundary: {
        geometry: {
          type: "Polygon",
          coordinates: [inventory.site.boundary.map(({ lat, lng }) => [lng, lat])],
        },
        sourceType: "OpenStreetMap",
        confidence: 1,
      },
    },
    viewport: { bounds: inventory.site.bounds },
    layers: [
      sceneLayer("roads", "internal_route"),
      sceneLayer("precincts", "precinct"),
      sceneLayer("buildings", "building"),
    ],
    features: inventory.features
      .filter((feature) => ["road", "precinct", "building"].includes(feature.category))
      .map(sceneFeature)
      .filter((feature): feature is SceneFeature => Boolean(feature)),
  };
}

test("a small boundary resolves to a single-surface society presentation", () => {
  const boundary: [number, number][] = [
    [77.7400, 12.9800],
    [77.7418, 12.9800],
    [77.7418, 12.9822],
    [77.7400, 12.9822],
    [77.7400, 12.9800],
  ];
  const scene: AtlasSceneInput = {
    anchor: {
      entityId: "society:waterford",
      label: "Prestige Waterford",
      coordinateQuality: "exact",
      boundary: {
        geometry: { type: "Polygon", coordinates: [boundary] },
        sourceType: "OpenStreetMap",
        confidence: 1,
      },
    },
    viewport: {},
    layers: [],
    features: [],
  };

  const presentation = resolveAtlasPresentation(scene);

  assert.equal(presentation.profile, "society");
  assert.equal(presentation.backdrop.layout, "single");
  assert.equal(presentation.route.enabled, false);
});

test("the Brigade fixture resolves from scene geometry without society-specific rules", () => {
  const presentation = resolveAtlasPresentation(brigadeScene());

  assert.equal(presentation.profile, "township");
  assert.equal(presentation.backdrop.context, "locator-route");
  assert.equal(presentation.labels.building, "texture");
  assert.ok(presentation.metrics.areaAcres > 100);
  assert.ok(presentation.metrics.precinctCount >= 4);
  assert.equal(presentation.route.enabled, true);
  assert.ok(presentation.route.featureId);
});
