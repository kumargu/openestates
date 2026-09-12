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
  dampHeading,
  projectStreetHandoff,
} from "../../experiments/home-atlas/src/journey.ts";
import { distanceMetres } from "../../experiments/home-atlas/src/geometry.ts";
import {
  bearingDegrees,
  fitCameraToScreen,
  projectCameraPointToScreen,
  type AtlasFitPoint,
  type AtlasScreenFrame,
} from "../../experiments/home-atlas/src/screenFit.ts";
import atlasPolicy from "../../app/config/ui/home-atlas.json" with { type: "json" };
import { AtlasCameraArbiter } from "../src/lib/atlasCameraArbiter.ts";
import {
  geometryForPlace,
  nearbySceneCamera,
  nearbyRelationArc,
} from '../src/lib/atlasNearbyScene.ts';
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
  const reversed = arrivalAtlasRoute(roads, "reverse")!;
  assert.deepEqual(reversed.coordinates, [...roads[0].coordinates].reverse());
  assert.equal(
    context.layers?.find((layer) => layer.id === "approach")?.experience?.routeDirection,
    "reverse",
  );
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
  const pairDistance = distanceMetres(
    {lat: home.latitude, lng: home.longitude},
    {lat: selected.latitude, lng: selected.longitude},
  );
  assert.ok(pair.range >= 950);
  assert.ok(pair.range <= pairDistance * 2.2, 'pair stays prominent instead of framing excess geography');
  const close = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'inspect', 900, 1400);
  assert.equal(close.tilt, atlasPolicy.cameraFit.inspectTilt);
  assert.equal(
    close.heading,
    (bearingDegrees(
      {lat: home.latitude, lng: home.longitude},
      {lat: selected.latitude, lng: selected.longitude},
    ) + atlasPolicy.cameraFit.inspectHeadingOffsetDegrees) % 360,
  );
  const distant = {...selected, latitude:home.latitude+0.15, longitude:home.longitude, feature_id:'distant', place_entity_id:'distant'};
  const broad = nearbySceneCamera(home, [distant], [], [], null, 'overview', 900, 1400);
  assert.ok(broad.range > overview.range, 'backend-scoped evidence must not be silently radius-filtered');
  const arc = nearbyRelationArc(home, selected);
  assert.equal(arc.length, 41);
  assert.equal(arc[0].lat, home.latitude);
  assert.equal(arc.at(-1)!.lng, selected.longitude);
  assert.ok(arc[20].altitude > arc[0].altitude);
  assert.ok(nearbySceneCamera(home, places, polygons, [], null, 'overview', 900, 390).range > overview.range);
  const returnedHome = nearbySceneCamera(home, places, polygons, [], null, 'home', 900, 1400);
  assert.equal(returnedHome.tilt, 55);
});

test('screen-space fitting contains point, polygon, line, and lifted anchors at any coordinate', () => {
  const frame: AtlasScreenFrame = {
    width: 1440,
    height: 1000,
    left: 32,
    right: 420,
    top: 96,
    bottom: 120,
  };
  const padding = atlasPolicy.cameraFit.opticalPaddingPx;
  const localPoint = (origin: {lat: number; lng: number}, eastM: number, northM: number, heightM = 0): AtlasFitPoint => ({
    lat: origin.lat + northM / 111_320,
    lng: origin.lng + eastM / (111_320 * Math.cos(origin.lat * Math.PI / 180)),
    heightM,
  });
  for (const origin of [{lat: 12.98, lng: 77.74}, {lat: -33.86, lng: 151.21}]) {
    for (const distanceM of [220, 1100, 4200]) {
      for (const rotation of [0, 47, 133]) {
        const radians = rotation * Math.PI / 180;
        const rotate = (eastM: number, northM: number, heightM = 0) => localPoint(
          origin,
          eastM * Math.cos(radians) - northM * Math.sin(radians),
          eastM * Math.sin(radians) + northM * Math.cos(radians),
          heightM,
        );
        const home = rotate(0, 0, 45);
        const selected = rotate(distanceM, 0, 45);
        const points: AtlasFitPoint[] = [
          home,
          selected,
          rotate(distanceM - 90, -70),
          rotate(distanceM + 110, -60),
          rotate(distanceM + 80, 95),
          rotate(distanceM - 120, 80),
          rotate(distanceM * 0.35, -40),
          rotate(distanceM * 0.65, 55),
          rotate(distanceM * 0.5, 0, 150),
        ];
        const camera = fitCameraToScreen({
          points,
          frame,
          heading: bearingDegrees(home, selected) + atlasPolicy.cameraFit.pairHeadingOffsetDegrees,
          tilt: atlasPolicy.cameraFit.pairTilt,
          fieldOfViewDegrees: atlasPolicy.cameraFit.fieldOfViewDegrees,
          minimumRangeM: 0,
          opticalPaddingPx: padding,
          altitudeM: 900,
        });
        for (const point of points) {
          const screen = projectCameraPointToScreen(
            camera,
            point,
            frame,
            atlasPolicy.cameraFit.fieldOfViewDegrees,
          );
          assert.ok(screen.x >= frame.left + padding - 0.5);
          assert.ok(screen.x <= frame.width - frame.right - padding + 0.5);
          assert.ok(screen.y >= frame.top + padding - 0.5);
          assert.ok(screen.y <= frame.height - frame.bottom - padding + 0.5);
        }
      }
    }
  }
});

test('nearby comparisons preserve orientation when selecting opposite-side alternatives', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = {...resolveHomeAnchor(context)!, boundary: context.home.boundary};
  const template = buildNumberedPlaces(context.places)[0];
  const places = [
    {...template, feature_id: 'east', latitude: home.latitude, longitude: home.longitude + 0.012},
    {...template, feature_id: 'west', latitude: home.latitude + 0.003, longitude: home.longitude - 0.008},
    {...template, feature_id: 'north', latitude: home.latitude + 0.01, longitude: home.longitude},
  ];
  for (const frame of [
    {width: 1440, height: 1000, left: 32, right: 420, top: 96, bottom: 120},
    {width: 390, height: 844, left: 16, right: 16, top: 160, bottom: 420},
  ]) {
    const overview = nearbySceneCamera(home, places, [], [], null, 'overview', 900, frame.width, frame);
    for (const selected of places) {
      const pair = nearbySceneCamera(home, places, [], [], selected.feature_id, 'pair', 900, frame.width, frame);
      assert.equal(pair.heading, overview.heading, 'selection must not rotate the comparison world');
      assert.equal(pair.tilt, atlasPolicy.cameraFit.pairTilt);
      const reversed = nearbySceneCamera(home, [...places].reverse(), [], [], selected.feature_id,
        'pair', 900, frame.width, frame);
      assert.ok(Math.abs(pair.heading - reversed.heading) < 1e-6, 'list order must not change orientation');
      const anchors = [
        {lat: home.latitude, lng: home.longitude, heightM: atlasPolicy.nearby.markerLiftM},
        {lat: selected.latitude, lng: selected.longitude, heightM: atlasPolicy.nearby.markerLiftM},
        ...nearbyRelationArc(home, selected).map(p => ({lat: p.lat, lng: p.lng, heightM: p.altitude})),
      ];
      for (const anchor of anchors) {
        const screen = projectCameraPointToScreen(pair, anchor, frame, atlasPolicy.cameraFit.fieldOfViewDegrees);
        assert.ok(screen.x >= frame.left && screen.x <= frame.width - frame.right);
        assert.ok(screen.y >= frame.top && screen.y <= frame.height - frame.bottom);
      }
    }
  }
});

test('inspect framing gives selected geometry more screen presence while retaining home', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = resolveHomeAnchor(context)!;
  const places = buildNumberedPlaces(context.places.filter(place => place.layer === 'lake'));
  const selected = places[0];
  const polygons = geometryForPlace(selected, context.layer_polygons!.lake, []).polygons;
  const frame: AtlasScreenFrame = {width: 1440, height: 1000, left: 32, right: 420, top: 96, bottom: 120};
  const pair = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'pair', 900, 1440, frame);
  const inspect = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'inspect', 900, 1440, frame);
  const geometry = polygons.flatMap(polygon => polygon.coordinates.map(([lng, lat]) => ({lat, lng})));
  const span = (camera: typeof pair) => {
    const projected = geometry.map(point => projectCameraPointToScreen(
      camera,
      point,
      frame,
      atlasPolicy.cameraFit.fieldOfViewDegrees,
    ));
    return Math.hypot(
      Math.max(...projected.map(point => point.x)) - Math.min(...projected.map(point => point.x)),
      Math.max(...projected.map(point => point.y)) - Math.min(...projected.map(point => point.y)),
    );
  };
  assert.ok(span(inspect) > span(pair));
  const homeMarker = projectCameraPointToScreen(inspect, {
    lat: home.latitude,
    lng: home.longitude,
    heightM: atlasPolicy.nearby.markerLiftM,
  }, frame, atlasPolicy.cameraFit.fieldOfViewDegrees);
  assert.ok(homeMarker.x >= frame.left && homeMarker.x <= frame.width - frame.right);
  assert.ok(homeMarker.y >= frame.top && homeMarker.y <= frame.height - frame.bottom);
});

test('missing place collections stay a calm sparse scene', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const sparse = {...context, places: undefined} as unknown as typeof context;
  assert.doesNotThrow(() => resolveHomeAnchor(sparse));
});

test('inspect spends spare frame space on selection without sacrificing home or scale', () => {
  const home = {lat: 12.98, lng: 77.74, heightM: 45};
  const selected = {...home, lng: home.lng + 0.0015};
  for (const frame of [
    {width: 1440, height: 1000, left: 32, right: 420, top: 96, bottom: 120},
    {width: 390, height: 844, left: 16, right: 16, top: 160, bottom: 420},
    {width: 844, height: 390, left: 16, right: 300, top: 70, bottom: 100},
  ]) {
    const input = {points: [home, selected], frame, heading: 35, tilt: 44,
      fieldOfViewDegrees: 38, minimumRangeM: 1200, opticalPaddingPx: 16, altitudeM: 920};
    const balanced = fitCameraToScreen(input);
    const focused = fitCameraToScreen({...input, focusPoint: selected});
    assert.equal(focused.range, balanced.range, 'focus must not distort comparison scale');
    const project = (camera: typeof focused, point: typeof home) =>
      projectCameraPointToScreen(camera, point, frame, input.fieldOfViewDegrees);
    const centre = {x: (frame.left + frame.width - frame.right) / 2,
      y: (frame.top + frame.height - frame.bottom) / 2};
    const error = (camera: typeof focused) => {
      const p = project(camera, selected);
      return Math.hypot(p.x - centre.x, p.y - centre.y);
    };
    assert.ok(error(focused) < error(balanced), 'selection moves toward the usable centre');
    for (const point of [home, selected]) {
      const p = project(focused, point);
      assert.ok(p.x >= frame.left + 15.5 && p.x <= frame.width - frame.right - 15.5);
      assert.ok(p.y >= frame.top + 15.5 && p.y <= frame.height - frame.bottom - 15.5);
    }
  }
});

test('Aerial refits the selected relationship at its final tilt, including mobile', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = {...resolveHomeAnchor(context)!, boundary: context.home.boundary};
  const places = buildNumberedPlaces(context.places.filter(place => place.layer === 'lake'));
  const selected = places[0];
  const polygons = geometryForPlace(selected, context.layer_polygons!.lake, []).polygons;
  const points = [
    ...home.boundary!.coordinates.map(([lng, lat]) => ({lat, lng, heightM: 0})),
    ...polygons.flatMap(p => p.coordinates.map(([lng, lat]) => ({lat, lng, heightM: 0}))),
    {lat: home.latitude, lng: home.longitude, heightM: atlasPolicy.nearby.markerLiftM},
    {lat: selected.latitude, lng: selected.longitude, heightM: atlasPolicy.nearby.markerLiftM},
    ...nearbyRelationArc(home, selected).map(p => ({lat: p.lat, lng: p.lng, heightM: p.altitude})),
  ];
  for (const frame of [
    {width: 1440, height: 1000, left: 32, right: 420, top: 96, bottom: 120},
    {width: 390, height: 844, left: 16, right: 16, top: 160, bottom: 420},
  ]) {
    for (const depth of ['pair', 'inspect'] as const) {
      const camera = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, depth,
        900, frame.width, frame, atlasPolicy.above.tilt);
      assert.equal(camera.tilt, atlasPolicy.above.tilt);
      for (const point of points) {
        const p = projectCameraPointToScreen(camera, point, frame, atlasPolicy.cameraFit.fieldOfViewDegrees);
        const padding = atlasPolicy.cameraFit.opticalPaddingPx - 0.5;
        assert.ok(p.x >= frame.left + padding && p.x <= frame.width - frame.right - padding);
        assert.ok(p.y >= frame.top + padding && p.y <= frame.height - frame.bottom - padding);
      }
    }
  }
});

test('road heading damping crosses north through the shortest arc', () => {
  const damped = dampHeading(359, 1, 3, 0.25);
  assert.ok(damped > 359 || damped < 1);
});

test('camera ownership prevents inactive scene families from moving the persistent map', () => {
  const arbiter = new AtlasCameraArbiter();
  const applied: string[] = [];
  assert.equal(arbiter.submit('nearby', () => applied.push('nearby')), false);
  assert.equal(arbiter.submit('society', () => applied.push('society')), true);
  arbiter.activate('road');
  assert.equal(arbiter.submit('society', () => applied.push('stale society')), false);
  assert.equal(arbiter.submit('road', () => applied.push('road')), true);
  arbiter.activate('nearby');
  assert.equal(arbiter.submit('road', () => applied.push('stale road')), false);
  assert.equal(arbiter.submit('nearby', () => applied.push('nearby')), true);
  assert.deepEqual(applied, ['society', 'road', 'nearby']);
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
