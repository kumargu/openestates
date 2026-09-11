import assert from "node:assert/strict";
import test from "node:test";
import { atlasFixtureScene } from "../src/lib/dev-atlas-fixtures.ts";
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import {
  arrivalAtlasRoute,
  arrivalAtlasContextLines,
} from "../src/lib/homeAtlasProjection.ts";
import {
  advanceRoadDistance,
  projectStreetHandoff,
} from "../../experiments/home-atlas/src/journey.ts";

test("Waterford API scene reaches production metro, boundary, nearby and road projections", () => {
  const scene = atlasFixtureScene("arrival_story");
  const context = propertyMapContextFromSurfaceScene(scene)!;
  assert.equal(context.metro_lines?.length, 16);
  assert.equal(context.home.boundary?.coordinates.length, 20);
  assert.equal(context.places.filter((p) => p.layer === "school").length, 2);
  assert.ok(Object.keys(context.layer_polygons ?? {}).length > 0);
  const roads = context.layer_lines!.approach;
  const route = arrivalAtlasRoute(roads)!;
  assert.ok(route.lengthM > 500);
  assert.deepEqual(route.coordinates, roads[0].coordinates);
  const halfway = advanceRoadDistance(route, 0, 1000, 1);
  assert.equal(halfway, 12);
  assert.equal(advanceRoadDistance(route, 0, 1000, 2), 24);
  const [longitude, latitude] = route.coordinates[0];
  assert.equal(
    projectStreetHandoff(route, { longitude, latitude }, 40).distanceAlongM,
    0,
  );
});

test("metro adapter preserves separate segments and backend scope including distant proof", () => {
  const context = propertyMapContextFromSurfaceScene(
    atlasFixtureScene("arrival_story"),
  )!;
  const lines = context.metro_lines!;
  const output = arrivalAtlasContextLines([...lines, lines[0]], {
    strokeColor: "#d5a7ff",
    strokeWidth: 5,
    altitudeMode: "clamp_to_ground",
    drawsOccludedSegments: false,
  });
  assert.equal(output.length, 16);
  output.forEach((line, index) =>
    assert.deepEqual(
      line.path,
      lines[index].coordinates.map(([lng, lat]) => ({ lng, lat })),
    ),
  );
});

test("missing and invalid geometry never creates a synthetic road", () => {
  assert.equal(arrivalAtlasRoute([]), null);
  const base = {
    id: "invalid",
    name: "Road",
    kind: "road",
    source_type: "OSM",
  };
  assert.equal(
    arrivalAtlasRoute([
      {
        ...base,
        coordinates: [
          [77, 12],
          [NaN, 12],
        ],
      },
    ]),
    null,
  );
  assert.equal(
    arrivalAtlasRoute([
      {
        ...base,
        coordinates: [
          [77, 12],
          [77, 12],
        ],
      },
    ]),
    null,
  );
});
