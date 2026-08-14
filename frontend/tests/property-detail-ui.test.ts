import assert from "node:assert/strict";
import test from "node:test";
import {
  ApiRequestError,
  parseProofFocusParam,
  propertyDetailPath,
  propertySurfacePath,
} from "../src/lib/api.ts";
import {
  availableLayers,
  filterPlacesByScale,
  hasAroundThisHomePlate,
  layerLabel,
  metroStationsAroundHome,
  placeMatchesProofFocus,
  scaleForStory,
} from "../src/lib/nearbyPlateProjection.ts";
import { resolveBuyerProjectStatus } from "../src/lib/projectStatus.ts";
import {
  propertyMediaLabel,
  propertySceneMediaAt,
  trustedPropertyMedia,
} from "../src/lib/propertyScene.ts";
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import type { PropertyMapContext, PropertyMedia, ProofFocus, SurfaceSceneResponse } from "../src/lib/types.ts";

const emptyMapContext: PropertyMapContext = {
  home: {
    entity_id: "society:test",
    name: "Test society",
  },
  places: [],
};

test("typed API errors preserve not-ready and not-found contracts", () => {
  const unavailable = new ApiRequestError(
    409,
    "Conflict",
    JSON.stringify({ error: "property_not_ready", reason_codes: ["missing_price"] }),
  );
  assert.equal(unavailable.status, 409);
  assert.equal(unavailable.code, "property_not_ready");
  assert.deepEqual(unavailable.reasonCodes, ["missing_price"]);

  const notFound = new ApiRequestError(404, "Not Found", "");
  assert.equal(notFound.status, 404);
  assert.equal(notFound.code, null);
});

test("proof focus URL contract round-trips through detail and surface paths", () => {
  const focus: ProofFocus = {
    surfaceId: "around_this_home",
    layerId: "hospitals",
    factKey: "nearby_hospitals",
    entityId: "place:manipal",
    featureId: "around_this_home:hospitals:place-manipal",
    receiptId: "receipt:manipal",
    matchedLabel: "Manipal Hospital Whitefield",
    matchedValue: "2.7 km from Manipal Hospital Whitefield",
    requestedConstraint: "near Manipal Hospital Whitefield",
    distanceM: 2700,
    reason: "matched near Manipal Hospital Whitefield",
  };

  const detailUrl = new URL(propertyDetailPath("property id/with slash", focus), "http://test.local");
  const parsed = parseProofFocusParam(detailUrl.searchParams.get("focus"));
  assert.deepEqual(parsed, focus);

  const surfaceUrl = new URL(
    propertySurfacePath("property id/with slash", "around_this_home", parsed),
    "http://test.local",
  );
  assert.equal(surfaceUrl.pathname, "/api/properties/property%20id%2Fwith%20slash/surfaces/around_this_home");
  assert.deepEqual(parseProofFocusParam(surfaceUrl.searchParams.get("focus")), focus);
});

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
  assert.equal(hasAroundThisHomePlate({
    ...emptyMapContext,
    red_flag_lines: [{
      id: "line-one",
      name: "High voltage transmission line",
      kind: "place",
      coordinates: [[77.7, 12.9], [77.71, 12.91]],
      source_type: "OpenStreetMap",
    }],
  }), true);
});

test("surface scene projects to existing around-this-home plate shape", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:test",
    entityRefs: {
      property_entity_id: "property:test",
      society_entity_id: "society:test",
      area_entity_id: "area:test",
    },
    anchor: {
      entityId: "society:test",
      label: "Test society",
      area: "Whitefield",
      geometry: { type: "Point", coordinates: [77.75, 12.98] },
      coordinateQuality: "exact",
    },
    viewport: {},
    layers: [],
    features: [{
      id: "around_this_home:schools:place-school",
      entityId: "place:school",
      layerId: "schools",
      kind: "place",
      label: "Green School",
      geometry: { type: "Point", coordinates: [77.751, 12.981] },
      coordinateQuality: "exact",
      metrics: { distanceM: 650, rating: 4.2, reviewCount: 120 },
      display: { tone: "positive", priority: 1 },
      confidence: 0.8,
      receiptIds: ["receipt:school"],
    }],
    relations: [],
    callouts: [],
    receipts: [{
      id: "receipt:school",
      entityId: "society:test",
      factKey: "nearby_schools",
      claim: "Green School",
      sourceType: "Google",
      sourceUrl: "https://maps.example/school",
      learnedAt: "2026-07-27T00:00:00Z",
      confidence: 0.8,
    }],
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
  assert.equal(context?.home.name, "Test society");
  assert.equal(context?.home.latitude, 12.98);
  assert.equal(context?.places[0]?.feature_id, "around_this_home:schools:place-school");
  assert.equal(context?.places[0]?.distance_km, 0.65);
  assert.equal(context?.places[0]?.source_type, "Google");
  assert.equal(hasAroundThisHomePlate(context), true);
});

test("surface scene projection preserves enriched map context overlays", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:test",
    entityRefs: {
      property_entity_id: "property:test",
      society_entity_id: "society:test",
      area_entity_id: "area:test",
    },
    anchor: {
      entityId: "society:test",
      label: "Test society",
      area: "Whitefield",
      geometry: { type: "Point", coordinates: [77.75, 12.98] },
      coordinateQuality: "exact",
    },
    viewport: {},
    layers: [{
      id: "tech",
      label: "Tech parks",
      family: "work",
      renderKind: "pin",
      relationClass: "amenity",
      enabledByDefault: true,
      rank: 4,
      availableCount: 1,
      shownCount: 1,
      fillState: "filled",
    }],
    features: [{
      id: "around_this_home:tech:bagmane",
      entityId: "place:bagmane",
      layerId: "tech",
      kind: "place",
      label: "Bagmane Tech Park",
      geometry: { type: "Point", coordinates: [77.71, 12.99] },
      coordinateQuality: "exact",
      metrics: { distanceM: 5300, rating: 4.3, reviewCount: 205 },
      display: { tone: "positive", priority: 4 },
      confidence: 0.82,
      receiptIds: ["receipt:bagmane"],
    }],
    relations: [],
    callouts: [],
    receipts: [{
      id: "receipt:bagmane",
      entityId: "society:test",
      factKey: "nearby_tech_parks",
      claim: "Bagmane Tech Park (5.3 km)",
      sourceType: "Computed",
      sourceUrl: "https://maps.example/bagmane",
      learnedAt: "2026-07-27T00:00:00Z",
      confidence: 0.82,
    }],
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
  const fallback: PropertyMapContext = {
    ...emptyMapContext,
    water: {
      groundwater_class: "Moderate",
      summary: "Area context",
      scope_radius_km: 3,
      source_type: "OpenCity",
      illustrative_zone: false,
    },
    metro_lines: [{
      id: "purple",
      name: "Purple Line",
      kind: "metro_line",
      coordinates: [[77.7, 12.9], [77.71, 12.91]],
      source_type: "OpenStreetMap",
    }],
    red_flag_lines: [{
      id: "line-one",
      name: "High voltage transmission line",
      kind: "place",
      coordinates: [[77.755, 12.985], [77.756, 12.986]],
      source_type: "OpenStreetMap",
    }],
  };

  const context = propertyMapContextFromSurfaceScene(scene, fallback);
  assert.equal(context?.places[0]?.name, "Bagmane Tech Park");
  assert.equal(context?.places.length, 1);
  assert.equal(context?.water?.groundwater_class, "Moderate");
  assert.equal(context?.metro_lines?.[0]?.name, "Purple Line");
  assert.equal(context?.red_flag_lines?.[0]?.name, "High voltage transmission line");
  assert.deepEqual(availableLayers(context!), ["tech", "red_flags"]);
});

test("surface scene projection merges fallback places additively", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:test",
    entityRefs: {
      property_entity_id: "property:test",
      society_entity_id: "society:test",
      area_entity_id: "area:test",
    },
    anchor: {
      entityId: "society:test",
      label: "Test society",
      geometry: { type: "Point", coordinates: [77.75, 12.98] },
      coordinateQuality: "exact",
    },
    viewport: {},
    layers: [],
    features: [{
      id: "around_this_home:hospitals:place-manipal",
      entityId: "place:manipal",
      layerId: "hospitals",
      kind: "place",
      label: "Manipal Hospital Whitefield",
      geometry: { type: "Point", coordinates: [77.751, 12.981] },
      coordinateQuality: "exact",
      metrics: { distanceM: 2700 },
      display: { tone: "positive", priority: 1 },
      confidence: 0.8,
      receiptIds: ["receipt:manipal"],
    }],
    relations: [],
    callouts: [],
    receipts: [{
      id: "receipt:manipal",
      entityId: "society:test",
      factKey: "nearby_hospitals",
      claim: "Manipal Hospital Whitefield (2.7 km)",
      sourceType: "Google",
      learnedAt: "2026-07-27T00:00:00Z",
      confidence: 0.8,
    }],
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
  const fallback: PropertyMapContext = {
    ...emptyMapContext,
    places: [
      { layer: "hospitals", name: "Aster", distance_km: 3.2, source_type: "Google" },
      { place_entity_id: "place:manipal", layer: "hospitals", name: "Manipal Hospital Whitefield", distance_km: 2.7, source_type: "Google" },
    ],
  };

  const context = propertyMapContextFromSurfaceScene(scene, fallback);
  assert.deepEqual(
    context?.places.map((place) => place.name),
    ["Manipal Hospital Whitefield", "Aster"],
  );
});

test("surface scene projection preserves proof focus handles", () => {
  const scene: SurfaceSceneResponse = {
    contractVersion: 1,
    surfaceId: "around_this_home",
    propertyId: "property:test",
    entityRefs: {
      property_entity_id: "property:test",
      society_entity_id: "society:test",
      area_entity_id: "area:test",
    },
    anchor: {
      entityId: "society:test",
      label: "Test society",
      geometry: { type: "Point", coordinates: [77.75, 12.98] },
      coordinateQuality: "exact",
    },
    viewport: {},
    proofFocus: {
      surfaceId: "around_this_home",
      layerId: "hospitals",
      factKey: "nearby_hospitals",
      entityId: "place:manipal",
      featureId: "around_this_home:hospitals:place-manipal",
      receiptId: "receipt:manipal",
      matchedLabel: "Manipal Hospital Whitefield",
      matchedValue: "2.7 km from Manipal Hospital Whitefield",
      requestedConstraint: "near Manipal Hospital Whitefield",
      distanceM: 2700,
      reason: "matched near Manipal Hospital Whitefield",
    },
    layers: [],
    features: [{
      id: "around_this_home:hospitals:place-manipal",
      entityId: "place:manipal",
      layerId: "hospitals",
      kind: "place",
      label: "Manipal Hospital Whitefield",
      geometry: { type: "Point", coordinates: [77.751, 12.981] },
      coordinateQuality: "exact",
      metrics: { distanceM: 2700 },
      display: { tone: "positive", priority: 1 },
      confidence: 0.8,
      receiptIds: ["receipt:manipal"],
    }],
    relations: [],
    callouts: [],
    receipts: [{
      id: "receipt:manipal",
      entityId: "society:test",
      factKey: "nearby_hospitals",
      claim: "Manipal Hospital Whitefield (2.7 km)",
      sourceType: "Google",
      learnedAt: "2026-07-27T00:00:00Z",
      confidence: 0.8,
    }],
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
  assert.equal(context?.proof_focus?.featureId, "around_this_home:hospitals:place-manipal");
  assert.equal(context?.places[0]?.feature_id, "around_this_home:hospitals:place-manipal");
  assert.equal(placeMatchesProofFocus(context!.places[0], context!.proof_focus), true);
});

test("around-this-home layer discovery follows returned scene layers", () => {
  const context: PropertyMapContext = {
    ...emptyMapContext,
    places: [
      { layer: "metro", name: "Metro", source_type: "Google" },
      { layer: "lakes", name: "Lake", source_type: "Computed" },
      { layer: "lakes", name: "Lake duplicate", source_type: "Computed" },
    ],
  };
  assert.deepEqual(availableLayers(context), ["metro", "lakes"]);
  assert.equal(layerLabel("lakes"), "Lakes");
  assert.equal(layerLabel("red_flags"), "Red flags");
  assert.equal(layerLabel("stormwater_drains"), "Stormwater Drains");
  assert.equal(
    layerLabel("stormwater_drains", {
      layers: [{ id: "stormwater_drains", label: "Drain lines" }],
    }),
    "Drain lines",
  );
});

test("around-this-home exposes line-only red flags as a layer", () => {
  const context: PropertyMapContext = {
    ...emptyMapContext,
    red_flag_lines: [{
      id: "red-line",
      name: "High voltage transmission line",
      kind: "place",
      coordinates: [[77.75, 12.98], [77.752, 12.982]],
      source_type: "OpenStreetMap",
    }],
  };
  assert.deepEqual(availableLayers(context), ["red_flags"]);
});

test("nearby scale keeps backend-curated places just beyond 1.5 km", () => {
  const places = [
    { layer: "hospitals", name: "Aster", distance_km: 1.2, source_type: "Google" },
    { layer: "hospitals", name: "Manipal", distance_km: 1.6, source_type: "Google" },
    { layer: "hospitals", name: "Far hospital", distance_km: 3.2, source_type: "Google" },
  ];
  assert.deepEqual(
    filterPlacesByScale(places, "nearby").map((place) => place.name),
    ["Aster", "Manipal"],
  );
});

test("proof focus keeps matched nearby item outside normal nearby cap", () => {
  const places = [
    { feature_id: "aster", layer: "hospitals", name: "Aster", distance_km: 1.2, source_type: "Google" },
    { feature_id: "manipal", place_entity_id: "place:manipal", layer: "hospitals", name: "Manipal Hospital Whitefield", distance_km: 2.7, source_type: "Google" },
    { feature_id: "far", layer: "hospitals", name: "Far hospital", distance_km: 3.2, source_type: "Google" },
  ];
  const focus = {
    surfaceId: "around_this_home",
    layerId: "hospitals",
    factKey: "nearby_hospitals",
    entityId: "place:manipal",
    matchedLabel: "Manipal Hospital Whitefield",
    distanceM: 2700,
    reason: "matched near Manipal Hospital Whitefield",
  };

  assert.deepEqual(
    filterPlacesByScale(places, "nearby", focus).map((place) => place.name),
    ["Aster", "Manipal Hospital Whitefield"],
  );
  assert.equal(
    scaleForStory({ kind: "layer", layer: "hospitals" }, focus, places),
    "area",
  );
});

test("metro trimming keeps focused station additively", () => {
  const places = [
    { feature_id: "first", layer: "metro", name: "First Metro", latitude: 12.980, longitude: 77.750, distance_km: 0.5, source_type: "Google" },
    { feature_id: "second", layer: "metro", name: "Second Metro", latitude: 12.981, longitude: 77.751, distance_km: 0.7, source_type: "Google" },
    { feature_id: "focus", place_entity_id: "place:metro-focus", layer: "metro", name: "Focused Metro", latitude: 12.982, longitude: 77.752, distance_km: 1.2, source_type: "Google" },
  ];
  const focus = {
    surfaceId: "around_this_home",
    layerId: "metro",
    factKey: "nearby_metro_stations",
    entityId: "place:metro-focus",
    matchedLabel: "Focused Metro",
    distanceM: 1200,
    reason: "matched near Focused Metro",
  };

  const selected = metroStationsAroundHome(
    places,
    { latitude: 12.98, longitude: 77.75 },
    [],
    focus,
  );
  assert.equal(selected.some((place) => place.name === "Focused Metro"), true);
  assert.equal(selected.length, 3);
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
  const scenes = [media("one", 0), media("two", 1), media("three", 2)];
  assert.equal(propertySceneMediaAt(scenes, 0)?.id, "one");
  assert.equal(propertySceneMediaAt(scenes, 4)?.id, "two");
  assert.equal(propertySceneMediaAt(scenes, -1)?.id, "one");
  assert.equal(propertySceneMediaAt([], 2), null);
});

test("property media accepts only validated typed assets in deterministic order", () => {
  const renderHash = mediaHash(3);
  const renderId = `sha256:${renderHash}`;
  const conflictHash = mediaHash(7);
  const conflictId = `sha256:${conflictHash}`;
  const accepted = trustedPropertyMedia([
    media("render", 2, {
      id: renderId,
      media_kind: "render",
      content_sha256: renderHash,
      hero_eligible: true,
      gallery_eligible: false,
    }),
    media("hero-only", 8, { hero_eligible: true, gallery_eligible: false }),
    media("mismatch", 0, { validation_state: "source_identity_mismatch" }),
    media("photo", 1),
    media("render", 5, {
      id: renderId,
      media_kind: "render",
      content_sha256: renderHash,
      hero_eligible: false,
      gallery_eligible: true,
    }),
    media("conflict-photo", 6, {
      id: conflictId,
      media_kind: "site_photo",
      content_sha256: conflictHash,
    }),
    media("conflict-render", 6, {
      id: conflictId,
      media_kind: "render",
      content_sha256: conflictHash,
    }),
    media("unknown", 6, { media_kind: "unknown" }),
    media("ad", 3, { media_kind: "marketing_artwork", gallery_eligible: false }),
  ]);
  assert.deepEqual(accepted.map((asset) => asset.id), [renderId, "hero-only", "photo"]);
  assert.equal(propertyMediaLabel(accepted[0]), "Render");
  assert.equal(propertyMediaLabel(accepted[1]), null);
  assert.equal(accepted[0]?.hero_eligible, true);
  assert.equal(accepted[0]?.gallery_eligible, true);
});

test("property media handles empty, single, mosaic, and long galleries", () => {
  for (const count of [0, 1, 4, 9]) {
    const assets = Array.from({ length: count }, (_, index) => media(`asset-${index}`, index));
    assert.equal(trustedPropertyMedia(assets).length, count);
  }
});

function media(
  id: string,
  display_order: number,
  overrides: Partial<PropertyMedia> = {},
): PropertyMedia {
  return {
    id,
    url: `/media/images/sha256/aa/${id}.jpg`,
    media_kind: "site_photo",
    canonical_entity_id: "society:test",
    validation_state: "source_identity_matched",
    source_type: "external_image",
    source_name: "Fixture",
    source_url: "https://example.test/source",
    observed_at: "2026-08-13T00:00:00Z",
    content_sha256: mediaHash(display_order + 1),
    quality_flags: [],
    hero_eligible: display_order === 0,
    gallery_eligible: true,
    display_order,
    ...overrides,
  };
}

function mediaHash(value: number): string {
  return Math.max(1, value).toString(16).padStart(64, "0");
}
