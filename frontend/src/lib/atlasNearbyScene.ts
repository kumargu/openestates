import {
  featureCamera,
  groupCamera,
  pairCamera,
  placeCamera,
} from '../../../experiments/home-atlas/src/camera.ts';
import { distanceMetres } from '../../../experiments/home-atlas/src/geometry.ts';
import type { AtlasCamera, AtlasFeature } from '../../../experiments/home-atlas/src/types.ts';
import policy from '../../../app/config/ui/home-atlas.json' with { type: 'json' };
import type { MapOverlayLine, MapOverlayPolygon } from './types.ts';
import type { NumberedPlace } from './nearbyPlateProjection.ts';

export type NearbyDepth = 'overview' | 'pair' | 'inspect' | 'home';
export type AtlasSafeFrame = Readonly<{
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
}>;
type Home = { latitude: number; longitude: number; name: string; boundary?: MapOverlayPolygon };
const points = (coordinates: [number, number][]) => coordinates.map(([lng, lat]) => ({ lat, lng }));

/** Join only explicit identity. Never guess ownership from names or proximity. */
export function geometryForPlace(place: NumberedPlace, polygons: MapOverlayPolygon[], lines: MapOverlayLine[]) {
  const owns = (shape: { id: string; entity_id?: string }) =>
    Boolean(place.place_entity_id && shape.entity_id === place.place_entity_id)
    || Boolean(place.feature_id && (shape.id === place.feature_id || shape.id.startsWith(`${place.feature_id}:`)));
  return { polygons: polygons.filter(owns), lines: lines.filter(owns) };
}

export function nearbySceneCamera(home: Home, places: NumberedPlace[], polygons: MapOverlayPolygon[],
  lines: MapOverlayLine[], selectedId: string | null, depth: NearbyDepth, elevation: number, width: number,
  safeFrame?: AtlasSafeFrame) {
  const origin = { lat: home.latitude, lng: home.longitude };
  const makeFeature = (id: string, position: {lat: number; lng: number}, boundary?: MapOverlayPolygon): AtlasFeature => ({
    id, name: id, categoryId: 'context', position,
    boundary: boundary ? points(boundary.coordinates) : undefined,
    evidence: { location: {providerId: 'scene'}, distance: {
      metres: distanceMetres(origin, position), method: 'straight_line', target: 'place_point',
    } },
  });
  const anchor = makeFeature('home', origin, home.boundary);
  const context = {home: anchor, defaultElevationM: elevation, viewportWidthPx: width,
    tuning: {
      maxContextDistanceM: Number.POSITIVE_INFINITY,
      minimumPairRangeM: policy.nearby.pairMinimumRangeM,
      pairRangeMultiplier: policy.nearby.pairRangeMultiplier,
    }};
  const selected = places.find(p => (p.feature_id ?? p.name) === selectedId);
  const active = selected && depth !== 'overview' ? [selected] : places;
  const geometry = selected && depth !== 'overview' ? geometryForPlace(selected, polygons, lines) : {polygons, lines};
  const features = [
    ...active.map(p => makeFeature(p.feature_id ?? p.name, {lat:p.latitude, lng:p.longitude})),
    ...geometry.polygons.filter(p => p.coordinates.length).map(p => makeFeature(p.id, points(p.coordinates)[0], p)),
    ...geometry.lines.filter(l => l.coordinates.length).map(l => ({...makeFeature(l.id, points(l.coordinates)[0]), segments:[{id:l.id,path:points(l.coordinates)}]})),
  ];
  if (depth === 'home') {
    return fitCameraToSafeFrame(placeCamera(context, anchor, {
      heading: 210,
      range: policy.focus.rangeM,
      tilt: policy.focus.tilt,
    }), safeFrame, 'focus');
  }
  if (depth === 'inspect' && selected) {
    const focus = makeFeature(selected.feature_id ?? selected.name, {lat:selected.latitude,lng:selected.longitude});
    const camera = !geometry.polygons.length && !geometry.lines.length
      ? featureCamera(context, focus)
      : {...groupCamera(context, features), tilt: geometry.polygons.length ? 30 : 50,
        heading: geometry.polygons.length ? 210 : 25};
    return fitCameraToSafeFrame(camera, safeFrame, 'focus');
  }
  if (depth === 'pair' && selected) {
    return fitCameraToSafeFrame(pairCamera(context, makeFeature(
      selected.feature_id ?? selected.name,
      {lat:selected.latitude,lng:selected.longitude},
    )), safeFrame, 'focus');
  }
  return fitCameraToSafeFrame(groupCamera(context, [anchor, ...features]), safeFrame);
}

/**
 * Google Maps 3D has no Three.js-style setViewOffset. Translate the geographic
 * camera target into the measured clear rectangle and grow its range instead.
 */
export function fitCameraToSafeFrame(
  camera: AtlasCamera,
  frame?: AtlasSafeFrame,
  mode: 'contain' | 'focus' = 'contain',
): AtlasCamera {
  if (!frame || frame.width <= 0 || frame.height <= 0) return camera;
  const availableWidth = Math.max(160, frame.width - frame.left - frame.right);
  const availableHeight = Math.max(120, frame.height - frame.top - frame.bottom);
  const safeCenterX = frame.left + availableWidth / 2;
  const safeCenterY = frame.top + availableHeight / 2;
  const dx = safeCenterX - frame.width / 2;
  const dy = safeCenterY - frame.height / 2;
  // Focus shots already fit their selected geometry. Enlarging them to make
  // up for every piece of overlaid chrome makes the buyer's two anchors tiny;
  // move the composition into the clear canvas instead.
  const rangeScale = mode === 'focus' ? 1 : Math.max(
    1,
    frame.width / availableWidth,
    frame.height / availableHeight,
  );
  const range = camera.range * rangeScale;
  const metresPerPixel = 2 * range
    * Math.tan(policy.cameraFit.fieldOfViewDegrees * Math.PI / 360)
    / frame.height;
  const heading = camera.heading * Math.PI / 180;
  const eastM = -(dx * Math.cos(heading) - dy * Math.sin(heading)) * metresPerPixel;
  const northM = -(-dx * Math.sin(heading) - dy * Math.cos(heading)) * metresPerPixel;
  return {
    ...camera,
    center: {
      ...camera.center,
      lat: camera.center.lat + northM / 111_320,
      lng: camera.center.lng + eastM / (
        111_320 * Math.max(0.2, Math.cos(camera.center.lat * Math.PI / 180))
      ),
    },
    range,
  };
}

/** Elevated straight-line relationship, not a claimed walking route. */
export function nearbyRelationArc(home: Home, place: NumberedPlace) {
  const distance = distanceMetres({lat:home.latitude,lng:home.longitude}, {lat:place.latitude,lng:place.longitude});
  return Array.from({length:41}, (_, i) => {
    const t = i / 40;
    return {lat:home.latitude+(place.latitude-home.latitude)*t,
      lng:home.longitude+(place.longitude-home.longitude)*t,
      altitude:28+Math.sin(Math.PI*t)*Math.min(170,distance*0.13)};
  });
}
