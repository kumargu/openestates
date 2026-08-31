import assert from "node:assert/strict";
import test from "node:test";
import {
  parseProofFocusParam,
  propertyDetailPath,
  propertySurfacePath,
} from "../src/lib/api.ts";
import {
  availableLayers,
  clusterClosePlaces,
  filterPlacesByScale,
  hasAroundThisHomePlate,
  layerLabel,
  metroStationsAroundHome,
  placeMatchesProofFocus,
  scaleForStory,
} from "../src/lib/nearbyPlateProjection.ts";
import {
  arrivalEvidenceViewport,
  arrivalMarkerPlaces,
  corridorCameraFocus,
  corridorTourWaypoints,
  hasArrivalMap,
} from "../src/lib/arrivalMapProjection.ts";
import { mapMarkerPinOptions } from "../src/lib/mapMarkerVisual.ts";
import {
  shouldReorientStreetView,
  sideRoadHeading,
  streetViewAnchorHeading,
  streetViewAnchorFrame,
  streetViewPlayback,
  type StreetViewFrame,
} from "../src/hooks/useGuidedStreetViewTour.ts";
import { resolveBuyerProjectStatus } from "../src/lib/projectStatus.ts";
import {
  initialPropertySceneUrls,
  photoIndexFromMosaicSlot,
  propertySceneImageAt,
  wrapPhotoIndex,
} from "../src/lib/propertyScene.ts";
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import type { PropertyMapContext, ProofFocus, SurfaceSceneResponse } from "../src/lib/types.ts";

const emptyMapContext: PropertyMapContext = {
  home: {
    entity_id: "society:test",
    name: "Test society",
  },
  places: [],
};

test("proof focus URL contract round-trips through detail and surface paths", () => {
  const focus: ProofFocus = {
    surfaceId: "around_this_home",
    layerId: "hospitals",
    factKey: "nearby_hospitals",
    destinationKind: "scene",
    targetId: "around-this-home",
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

test("around-this-home keeps mainline visibility semantics", () => {
  const road = {
    id: "ecc-road",
    name: "ECC Road",
    coordinates: [[77.73, 12.98], [77.74, 12.99]] as [number, number][],
    source_type: "OpenStreetMap",
  };
  assert.equal(hasAroundThisHomePlate({
    ...emptyMapContext,
    access_lines: [road],
    layer_lines: { approach_road: [road] },
  }), false);
});

test("the arrival tile owns society and guided-road 3D evidence", () => {
  const road = {
    id: "ecc-road",
    name: "ECC Road",
    coordinates: [[77.73, 12.98], [77.74, 12.99]] as [number, number][],
    source_type: "OpenStreetMap",
  };
  const context: PropertyMapContext = {
    ...emptyMapContext,
    home: {
      ...emptyMapContext.home,
      latitude: 12.98,
      longitude: 77.74,
    },
    layers: [{
      id: "approach_road",
      label: "Approach road",
      renderKind: "terrain_corridor",
      experience: {
        kind: "street_view_tour",
        distanceEachDirectionM: 300,
        waypointSpacingM: 60,
        dwellMs: 3_600,
        curveDwellMs: 2_400,
        sideRoadDwellMs: 2_400,
        cameraAltitudeM: 8,
        cameraRangeM: 30,
        cameraTilt: 82,
        cameraFov: 52,
        streetViewZoom: 1,
        transitionMs: 1_000,
      },
    }],
    layer_lines: { approach_road: [road] },
  };

  assert.equal(hasArrivalMap(context), true);
  assert.equal(hasAroundThisHomePlate(context), false);
});

test("arrival entrance labels and icons follow scene config and status", () => {
  const layer = {
    id: "entrance",
    label: "Entrance",
    renderKind: "arrival_marker",
    emptyState: "Entrance not mapped",
    featureValueLabels: {
      status: { verified: "Entrance", inferred: "Likely entrance" },
    },
  };
  const base = {
    ...emptyMapContext,
    places: [{
      feature_id: "entrance-one",
      layer: "entrance",
      name: "Entrance",
      latitude: 12.98,
      longitude: 77.74,
      source_type: "OpenStreetMap",
      properties: { status: "inferred" },
    }],
  } satisfies PropertyMapContext;

  const inferred = arrivalMarkerPlaces(base, layer);
  assert.equal(inferred[0]?.name, "Likely entrance");
  assert.equal(inferred[0]?.icon, "entrance-likely");
  assert.deepEqual(arrivalMarkerPlaces({ ...base, places: [] }, layer), []);
  const verified = arrivalMarkerPlaces({
    ...base,
    places: [{ ...base.places[0], properties: { status: "verified" } }],
  }, layer);
  assert.equal(verified[0]?.name, "Entrance");
  assert.equal(verified[0]?.icon, "entrance");
});

test("arrival metro framing shifts from the society toward its evidence", () => {
  const home = { latitude: 12.98, longitude: 77.74 };
  const viewport = arrivalEvidenceViewport(home, [], [{
    id: "purple",
    name: "Purple Line",
    coordinates: [[77.75, 12.99], [77.76, 13]],
    source_type: "OpenStreetMap",
  }]);

  assert.ok(viewport.center.latitude > home.latitude);
  assert.ok(viewport.center.longitude > home.longitude);
  assert.ok(viewport.radiusKm >= 0.7);
});

test("approach-road camera targets the nearest road segment and looks along it", () => {
  const road = {
    id: "ecc-road",
    name: "ECC Road",
    coordinates: [
      [77.744, 12.984],
      [77.7435, 12.982],
      [77.743, 12.98],
    ],
    source_type: "OpenStreetMap",
  };
  const home = {
    latitude: 12.982,
    longitude: 77.742,
  };
  const focus = corridorCameraFocus([road], home);

  assert.ok(focus);
  assert.ok(Math.abs(focus.latitude - 12.98166) < 0.0001);
  assert.ok(Math.abs(focus.longitude - 77.74341) < 0.0001);
  assert.ok(focus.heading > 10 && focus.heading < 20);

  const waypoints = corridorTourWaypoints([road], home, 150, 60);
  assert.equal(waypoints.some((waypoint) => waypoint.offsetM === 0), true);
  assert.equal(waypoints.some((waypoint) => waypoint.offsetM === 150), true);
  assert.equal(waypoints.some((waypoint) => waypoint.offsetM === -150), true);
  assert.ok(waypoints.every((waypoint) => waypoint.heading > 10 && waypoint.heading < 20));

  const fullRoad = corridorTourWaypoints([road], home, 150, 60, "end_to_end", [35]);
  assert.ok(Math.abs(fullRoad[0].latitude - 12.98) < 0.0001);
  assert.ok(Math.abs(fullRoad.at(-1)!.latitude - 12.984) < 0.0001);
  assert.equal(fullRoad.some((waypoint) => waypoint.offsetM === 0), true);
  assert.equal(fullRoad.some((waypoint) => waypoint.offsetM === 35), true);
});

test("guided road playback covers both directions and returns to its start", () => {
  const frames = [-120, -60, 0, 60, 120].map((offsetM) => ({
    links: [],
    pano: `pano-${offsetM}`,
    waypoint: {
      latitude: 12.98,
      longitude: 77.74,
      heading: 15,
      offsetM,
    },
  } satisfies StreetViewFrame));

  assert.deepEqual(
    streetViewPlayback(frames).map((frame) => frame.waypoint.offsetM),
    [0, 60, 120, 60, 0, -60, -120, -60, 0],
  );
});

test("end-to-end road playback passes the gate once without reversing", () => {
  const frames = [120, -120, 0, 60, -60].map((offsetM) => ({
    links: [],
    pano: `pano-${offsetM}`,
    waypoint: {
      latitude: 12.982,
      longitude: 77.7435,
      heading: 15,
      offsetM,
    },
  } satisfies StreetViewFrame));

  assert.deepEqual(
    streetViewPlayback(frames, "end_to_end").map((frame) => frame.waypoint.offsetM),
    [-120, -60, 0, 60, 120],
  );
  const gateHeading = streetViewAnchorHeading(
    frames[2].waypoint,
    { latitude: 12.982, longitude: 77.742 },
  );
  assert.ok(gateHeading > 260 && gateHeading < 280);
});

test("guided road playback can pause just beyond the gate", () => {
  const frames = [-60, 0, 30, 60].map((offsetM) => ({
    links: [],
    pano: `pano-${offsetM}`,
    waypoint: {
      latitude: 12.982,
      longitude: 77.7435,
      heading: 15,
      offsetM,
    },
  } satisfies StreetViewFrame));

  assert.equal(streetViewAnchorFrame(frames)?.waypoint.offsetM, 0);
  assert.equal(streetViewAnchorFrame(frames, 35)?.waypoint.offsetM, 30);
  assert.equal(streetViewAnchorFrame([], 35), null);
});

test("guided road playback recognizes a side-road view", () => {
  assert.equal(sideRoadHeading([
    { heading: 15, pano: "forward" },
    { heading: 105, pano: "side-road" },
    { heading: 195, pano: "backward" },
  ], 15), 105);
  assert.equal(shouldReorientStreetView(15, 21), false);
  assert.equal(shouldReorientStreetView(15, 35), true);
});

test("map marker visuals distinguish categories and focus", () => {
  const school = mapMarkerPinOptions("graduation-cap", "active");
  const hospital = mapMarkerPinOptions("hospital", "active");
  const selected = mapMarkerPinOptions("graduation-cap", "selected");
  const subdued = mapMarkerPinOptions("graduation-cap", "subdued");

  assert.notEqual(school.background, hospital.background);
  assert.notEqual(school.glyphSrc, hospital.glyphSrc);
  assert.ok(selected.scale > school.scale);
  assert.ok(school.scale > subdued.scale);

  const clustered = clusterClosePlaces([
    {
      id: "school-1",
      number: 1,
      layer: "schools",
      icon: "graduation-cap",
      name: "One School",
      latitude: 12.98,
      longitude: 77.75,
      source_type: "Google",
    },
    {
      id: "school-2",
      number: 2,
      layer: "schools",
      icon: "graduation-cap",
      name: "Two School",
      latitude: 12.9801,
      longitude: 77.7501,
      source_type: "Google",
    },
  ], "nearby");
  assert.equal(clustered.clusters[0]?.layer, "schools");
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
      boundary: {
        geometry: {
          type: "Polygon",
          coordinates: [[
            [77.74, 12.97],
            [77.76, 12.97],
            [77.76, 12.99],
            [77.74, 12.97],
          ]],
        },
        sourceType: "OpenStreetMap",
        sourceUrl: "https://www.openstreetmap.org/way/1",
        confidence: 0.78,
      },
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
      display: { tone: "positive", icon: "graduation-cap", priority: 1 },
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
  assert.equal(context?.home.boundary?.coordinates.length, 4);
  assert.equal(context?.home.boundary?.source_type, "OpenStreetMap");
  assert.equal(context?.places[0]?.feature_id, "around_this_home:schools:place-school");
  assert.equal(context?.places[0]?.icon, "graduation-cap");
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

test("photo walker wraps and opens the clicked mosaic slot", () => {
  assert.equal(wrapPhotoIndex(0, 5), 0);
  assert.equal(wrapPhotoIndex(5, 5), 0);
  assert.equal(wrapPhotoIndex(-1, 5), 4);
  assert.equal(wrapPhotoIndex(2, 0), 0);
  assert.equal(photoIndexFromMosaicSlot("lead"), 0);
  assert.equal(photoIndexFromMosaicSlot("all"), 0);
  assert.equal(photoIndexFromMosaicSlot({ tile: 0 }), 1);
  assert.equal(photoIndexFromMosaicSlot({ tile: 3 }), 4);
});

test("recommendation scenes are stable and wrap after exhaustion", () => {
  const scenes = ["one.jpg", "two.jpg", "three.jpg"];
  assert.equal(propertySceneImageAt(scenes, 0), "one.jpg");
  assert.equal(propertySceneImageAt(scenes, 1), "two.jpg");
  assert.equal(propertySceneImageAt(scenes, 4), "two.jpg");
  assert.equal(propertySceneImageAt(scenes, -1), "one.jpg");
  assert.equal(propertySceneImageAt([], 2, "fallback.jpg"), "fallback.jpg");
});

test("property scene URLs are returned immediately from the serving payload", () => {
  assert.deepEqual(
    initialPropertySceneUrls({
      heroImage: "/media/images/sha256/aa/hero.avif",
      images: [
        "/media/images/sha256/aa/hero.avif",
        "/media/images/sha256/bb/gallery.avif",
      ],
    }),
    [
      "/media/images/sha256/aa/hero.avif",
      "/media/images/sha256/bb/gallery.avif",
    ],
  );
});
