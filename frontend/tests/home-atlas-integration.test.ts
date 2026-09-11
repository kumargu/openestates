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
import { geometryForPlace, nearbySceneCamera, nearbyRelationArc } from '../src/lib/atlasNearbyScene.ts';
import { buildNumberedPlaces, resolveHomeAnchor } from '../src/lib/nearbyPlateProjection.ts';

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

test('nearby relationship keeps home, uses real extents and never drops distant API evidence', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = resolveHomeAnchor(context)!;
  const places = buildNumberedPlaces(context.places.filter(p => p.layer === 'lake'));
  const polygons = context.layer_polygons!.lake;
  assert.ok(places.length > 1);
  const selected = places[0];
  assert.ok(geometryForPlace(selected, polygons, []).polygons.length > 0);
  const overview = nearbySceneCamera(home, places, polygons, [], null, 'overview', 900, 1400);
  assert.equal(overview.tilt, 25);
  const pair = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'pair', 900, 1400);
  assert.ok(pair.range >= 1000);
  const close = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'inspect', 900, 1400);
  assert.equal(close.tilt, 30);
  const distant = {...selected, latitude:home.latitude+0.15, longitude:home.longitude, feature_id:'distant', place_entity_id:'distant'};
  const broad = nearbySceneCamera(home, [distant], [], [], null, 'overview', 900, 1400);
  assert.ok(broad.range > 20000, 'backend-scoped evidence must not be silently radius-filtered');
  const arc = nearbyRelationArc(home, selected);
  assert.equal(arc.length, 41);
  assert.equal(arc[0].lat, home.latitude);
  assert.equal(arc.at(-1)!.lng, selected.longitude);
  assert.ok(arc[20].altitude > arc[0].altitude);
  assert.ok(nearbySceneCamera(home, places, polygons, [], null, 'overview', 900, 390).range > overview.range);
});

test('mapped shape ownership survives the API projection without guessing names', () => {
  const scene = atlasFixtureScene('arrival_story');
  const context = propertyMapContextFromSurfaceScene(scene)!;
  const roads = buildNumberedPlaces(context.places.filter(p => p.layer === 'road'));
  assert.ok(geometryForPlace(roads[0], [], context.layer_lines!.road).lines.length > 0);
  const lake = scene.features.find(f => f.kind === 'lake' && f.geometry.type === 'Polygon')!;
  const shapeOnly = propertyMapContextFromSurfaceScene({...scene,features:[lake]})!;
  assert.equal(shapeOnly.places.length, 1, 'polygon-only places remain discoverable');
  const place = buildNumberedPlaces(shapeOnly.places)[0];
  assert.equal(geometryForPlace(place, shapeOnly.layer_polygons!.lake, []).polygons.length, 1);
  const unrelated = {...shapeOnly.layer_polygons!.lake[0],id:'unrelated',entity_id:'unrelated'};
  assert.equal(geometryForPlace(place, [unrelated], []).polygons.length, 0);
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
