import assert from "node:assert/strict";
import test from "node:test";

import {
  buildNumberedPlaces,
} from "../src/lib/nearbyPlateProjection.ts";
import {
  arrivalEvidenceViewport,
  cameraCenterForMode,
  metroLinesNearArrival,
} from "../src/lib/arrivalMapProjection.ts";
import type { MapOverlayLine, MapPlacePin } from "../src/lib/types.ts";

const home = { latitude: 12.9819914, longitude: 77.7421819 };

function line(
  id: string,
  coordinates: [number, number][],
): MapOverlayLine {
  return {
    id,
    name: "Purple Line",
    kind: "metro",
    coordinates,
    source_type: "OpenStreetMap",
  };
}

const metroPlace: MapPlacePin = {
  layer: "metro",
  name: "Kadugodi Tree Park",
  latitude: 12.985711,
  longitude: 77.746842,
  distance_km: 0.7,
  source_type: "Google",
};

test("the camera preserves the home portrait until evidence is requested", () => {
  const viewport = {
    center: { latitude: 12.984, longitude: 77.745 },
    radiusKm: 1.2,
    zoom: 14,
    paddingFactor: 0.2,
  };

  assert.deepEqual(cameraCenterForMode("home", home, viewport), home);
  assert.deepEqual(cameraCenterForMode("evidence", home, viewport), viewport.center);
});

test("metro framing keeps the local corridor and drops far Purple Line segments", () => {
  const places = buildNumberedPlaces([metroPlace]);
  const lines = [
    line("west-far", [[77.688, 12.998], [77.701, 12.994]]),
    line("home-west", [[77.7353, 12.9876], [77.74, 12.9876]]),
    line("home-station", [[77.74, 12.9876], [77.747, 12.9856]]),
    line("station-east", [[77.747, 12.9856], [77.751, 12.9842]]),
    line("east-far", [[77.758, 12.996], [77.768, 13.004]]),
  ];

  const visible = metroLinesNearArrival(home, places, lines);

  assert.deepEqual(
    new Set(visible.map(({ id }) => id)),
    new Set(["home-west", "home-station", "station-east"]),
  );
});

test("arrival viewport shifts toward the Metro evidence", () => {
  const places = buildNumberedPlaces([metroPlace]);
  const metro = [line("home-station", [
    [77.7399579, 12.9875824],
    [77.7470097, 12.9856238],
  ])];
  const viewport = arrivalEvidenceViewport(home, places, metro);

  assert.ok(viewport.center.longitude > home.longitude);
  assert.ok(viewport.center.longitude < metro[0].coordinates.at(-1)![0]);
  assert.ok(viewport.center.latitude > home.latitude);
  assert.ok(viewport.radiusKm >= 0.7);
});
