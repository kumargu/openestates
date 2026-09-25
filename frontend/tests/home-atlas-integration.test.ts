import assert from "node:assert/strict";
import test from "node:test";
import { atlasFixtureScene } from "../src/lib/dev-atlas-fixtures.ts";
import { propertyMapContextFromSurfaceScene } from "../src/lib/surfaceSceneProjection.ts";
import {
  arrivalAtlasRoute,
  arrivalAtlasContextLines,
  lineColorFromName,
} from "../src/lib/homeAtlasProjection.ts";
import {
  advanceRoadDistance,
  dampHeading,
  projectStreetHandoff,
} from "../src/lib/atlas/journey.ts";
import { distanceMetres } from "../src/lib/atlas/geometry.ts";
import {
  bearingDegrees,
  fitCameraToScreen,
  projectCameraPointToScreen,
  type AtlasFitPoint,
  type AtlasScreenFrame,
} from "../src/lib/atlas/screenFit.ts";
import atlasPolicy from "../src/lib/atlasPolicy.ts";
import { AtlasCameraArbiter } from "../src/lib/atlasCameraArbiter.ts";
import {
  geometryForPlace,
  homeOrbitCamera,
  homeSceneCamera,
  nearbySceneCamera,
  nearbyRelationArc,
} from '../src/lib/atlasNearbyScene.ts';
import { buildNumberedPlaces, resolveHomeAnchor } from '../src/lib/nearbyPlateProjection.ts';

test('composed Atlas retains proof focus and every configured line layer', () => {
  const scene = atlasFixtureScene('arrival_story');
  const fallback = propertyMapContextFromSurfaceScene(scene)!;
  const proof = { surfaceId: 'around_this_home', layerId: 'custom-layer', factKey: 'custom-fact', reason: 'matched', entityId: 'custom-place' };
  fallback.proof_focus = proof;
  const line = { id: 'custom-line', name: 'Custom alignment', kind: 'line', coordinates: [[77.7, 12.9], [77.8, 12.9]] as [number, number][], source_type: 'test' };
  fallback.layer_lines = { ...fallback.layer_lines, 'custom-layer': [line] };
  const merged = propertyMapContextFromSurfaceScene(scene, fallback)!;
  assert.deepEqual(merged.proof_focus, proof);
  assert.deepEqual(merged.layer_lines?.['custom-layer'], [line]);
  assert.equal(new Set(merged.layer_lines?.approach.map((item) => item.id)).size, merged.layer_lines?.approach.length);
  const explicit = { ...proof, entityId: 'new-place' };
  assert.deepEqual(propertyMapContextFromSurfaceScene({ ...scene, proofFocus: explicit }, fallback)?.proof_focus, explicit);
});

test('home opening fits the complete boundary beside desktop chrome at different browser sizes', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = {...resolveHomeAnchor(context)!, name: context.home.name, boundary: context.home.boundary};
  for (const frame of [
    {width: 1280, height: 1000, left: 32, right: 32, top: 220, bottom: 120},
    {width: 1046, height: 800, left: 32, right: 420, top: 220, bottom: 120},
    {width: 1600, height: 1100, left: 32, right: 420, top: 220, bottom: 120},
    {width: 1166, height: 768, left: 32, right: 464, top: 226, bottom: 32},
    {width: 1046, height: 620, left: 32, right: 464, top: 226, bottom: 32},
  ]) {
    for (const tilt of [undefined, atlasPolicy.above.tilt]) {
      const camera = homeSceneCamera(home, 900, frame, tilt);
      assert.equal(camera.fov, atlasPolicy.cameraFit.homeFieldOfViewDegrees);
      assert.equal(camera.heading, atlasPolicy.cameraFit.homeHeadingDegrees);
      assert.equal(camera.tilt, tilt ?? atlasPolicy.cameraFit.homeTilt);
      assert.equal(camera.center.altitude, 900 + atlasPolicy.cameraFit.homeCenterAltitudeOffsetM);
      assert.ok(camera.range >= atlasPolicy.cameraFit.homeMinimumRangeM);
      const projected = home.boundary!.coordinates.map(([lng, lat]) =>
        projectCameraPointToScreen(camera, {lat, lng}, frame, camera.fov));
      const padding = atlasPolicy.cameraFit.opticalPaddingPx - 0.5;
      for (const point of projected) {
        assert.ok(point.x >= frame.left + padding && point.x <= frame.width - frame.right - padding);
        assert.ok(point.y >= frame.top + padding && point.y <= frame.height - frame.bottom - padding);
      }
      const width = Math.max(...projected.map(p => p.x)) - Math.min(...projected.map(p => p.x));
      const height = Math.max(...projected.map(p => p.y)) - Math.min(...projected.map(p => p.y));
      assert.ok(Math.max(width / (frame.width - frame.left - frame.right),
        height / (frame.height - frame.top - frame.bottom)) > 0.5, 'home uses the available canvas');
    }
  }
});

test('Home orbit derives its range from mapped geometry and the available desktop frame', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = {...resolveHomeAnchor(context)!, name: context.home.name, boundary: context.home.boundary};
  const frame = {width: 1440, height: 1000, left: 32, right: 420, top: 96, bottom: 120};
  const opening = homeSceneCamera(home, 900, frame);
  const camera = homeOrbitCamera(home, 900, frame);
  assert.deepEqual(camera.center, {...opening.center,
    altitude: 900 + atlasPolicy.homeOrbit.altitudeOffsetM});
  assert.equal(camera.range, Math.max(atlasPolicy.homeOrbit.desktopMinimumRangeM,
    opening.range * atlasPolicy.homeOrbit.rangeScale));
  assert.equal(camera.tilt, atlasPolicy.homeOrbit.tilt);
  assert.equal(camera.heading, opening.heading);
  assert.equal(camera.fov, opening.fov);
  assert.equal(atlasPolicy.homeOrbit.descentMs, atlasPolicy.road.descentMs);
  assert.equal(atlasPolicy.homeOrbit.cycleMs, 180_000);

  const atMetres = (east: number, north: number): [number, number] => [
    home.longitude + east / (111_320 * Math.cos(home.latitude * Math.PI / 180)),
    home.latitude + north / 111_320,
  ];
  const boundaries = [
    undefined,
    {id: 'small', name: 'Small site', kind: 'boundary', source_type: 'test', coordinates: [
      atMetres(-40, -40), atMetres(40, -40), atMetres(40, 40), atMetres(-40, 40), atMetres(-40, -40),
    ]},
    {id: 'large', name: 'Large site', kind: 'boundary', source_type: 'test', coordinates: [
      atMetres(-500, -350), atMetres(500, -350), atMetres(500, 350), atMetres(-500, 350), atMetres(-500, -350),
    ]},
    {id: 'long', name: 'Elongated site', kind: 'boundary', source_type: 'test', coordinates: [
      atMetres(-700, -45), atMetres(700, -45), atMetres(700, 45), atMetres(-700, 45), atMetres(-700, -45),
    ]},
  ];
  for (const desktopFrame of [
    frame,
    {width: 1166, height: 768, left: 32, right: 464, top: 226, bottom: 32},
  ]) {
    const ranges = boundaries.map((boundary) => homeOrbitCamera({...home, boundary}, 900, desktopFrame).range);
    assert.ok(ranges[0] >= atlasPolicy.cameraFit.homeMinimumRangeM);
    assert.ok(ranges[1] >= atlasPolicy.cameraFit.homeMinimumRangeM);
    assert.ok(ranges[2] > Math.max(ranges[0], ranges[1]));
    assert.ok(ranges[3] > Math.max(ranges[0], ranges[1]));
  }
});

test('Metro segments derive CSS colors from canonical API line names', () => {
  const line = (id: string, name: string) => ({
    id,
    name,
    kind: 'metro_line',
    coordinates: [[77.74, 12.98], [77.75, 12.99]] as [number, number][],
    source_type: 'OpenStreetMap',
  });
  const contextLines = arrivalAtlasContextLines([
    line('purple', 'Purple Line'),
    line('yellow', 'Yellow Line'),
    line('unknown', 'Namma Metro'),
  ], metroLine => ({
    strokeColor: lineColorFromName(metroLine.name, atlasPolicy.metro.strokeColor),
    strokeWidth: atlasPolicy.metro.strokeWidth,
    altitudeMode: 'clamp_to_ground',
    drawsOccludedSegments: atlasPolicy.metro.drawsOccludedSegments,
  }));

  assert.deepEqual(contextLines.map(item => item.style.strokeColor), [
    'purple',
    'yellow',
    atlasPolicy.metro.strokeColor,
  ]);
  assert.ok(contextLines.every(item => item.style.strokeWidth === atlasPolicy.metro.strokeWidth));
});

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
  // The production policy starts at 2×, with slower and faster inspection available.
  assert.equal(atlasPolicy.road.defaultRate, 2);
  assert.equal(advanceRoadDistance(route, 0, 1000, atlasPolicy.road.defaultRate, atlasPolicy.road), 24);
  assert.equal(advanceRoadDistance(route, 0, 1000, atlasPolicy.road.minimumRate, atlasPolicy.road), 6);
  assert.equal(advanceRoadDistance(route, 0, 1000, atlasPolicy.road.maximumRate, atlasPolicy.road), 48);
  assert.equal(advanceRoadDistance(route, route.lengthM - 1, 1000, atlasPolicy.road.maximumRate, atlasPolicy.road), route.lengthM);
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
  assert.equal(overview.tilt, atlasPolicy.cameraFit.overviewTilt);
  const pair = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'pair', 900, 1400);
  const pairDistance = distanceMetres(
    {lat: home.latitude, lng: home.longitude},
    {lat: selected.latitude, lng: selected.longitude},
  );
  assert.ok(pair.range >= 950);
  assert.ok(pair.range <= pairDistance * 2.2, 'pair stays prominent instead of framing excess geography');
  const close = nearbySceneCamera(home, places, polygons, [], selected.feature_id!, 'inspect', 900, 1400);
  assert.equal(close.tilt, atlasPolicy.cameraFit.inspectTilt);
  const revealTurn = ((close.heading - overview.heading + 540) % 360) - 180;
  assert.ok(Math.abs(revealTurn) <= 20, 'reveal keeps the category orientation legible');
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
  assert.equal(returnedHome.tilt, atlasPolicy.cameraFit.homeTilt);
  assert.equal(returnedHome.heading, atlasPolicy.cameraFit.homeHeadingDegrees);
  assert.equal(returnedHome.fov, atlasPolicy.cameraFit.homeFieldOfViewDegrees);
  assert.equal(returnedHome.center.altitude, 900 + atlasPolicy.cameraFit.homeCenterAltitudeOffsetM);
  assert.ok(returnedHome.range >= atlasPolicy.cameraFit.homeMinimumRangeM);
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

test('ordinary Nearby turns each selected relationship to keep Home prominent', () => {
  const context = propertyMapContextFromSurfaceScene(atlasFixtureScene('arrival_story'))!;
  const home = {...resolveHomeAnchor(context)!, boundary: context.home.boundary};
  const places = buildNumberedPlaces(context.places.filter(place => place.layer === 'school'));
  const frame: AtlasScreenFrame = {
    width: 1440,
    height: 1000,
    left: 32,
    right: 420,
    top: 96,
    bottom: 120,
  };
  const headings = new Set<number>();
  for (const selected of places) {
    const stable = nearbySceneCamera(home, places, [], [], selected.feature_id!, 'inspect',
      900, frame.width, frame);
    const pair = nearbySceneCamera(home, places, [], [], selected.feature_id!, 'pair',
      900, frame.width, frame, undefined, 'selected-home-foreground');
    const inspect = nearbySceneCamera(home, places, [], [], selected.feature_id!, 'inspect',
      900, frame.width, frame, undefined, 'selected-home-foreground');
    const expected = (bearingDegrees(
      {lat: home.latitude, lng: home.longitude},
      {lat: selected.latitude, lng: selected.longitude},
    ) + atlasPolicy.cameraFit.selectedHomeForegroundHeadingOffsetDegrees) % 360;
    assert.ok(Math.abs(inspect.heading - expected) < 1e-6);
    assert.equal(pair.heading, inspect.heading, 'pair and reveal must not introduce a second turn');
    assert.ok(inspect.range < stable.range, 'fixture Home should gain screen presence');
    headings.add(inspect.heading);
  }
  assert.equal(headings.size, places.length, 'selection, not category centroid, owns the inspect bearing');
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
