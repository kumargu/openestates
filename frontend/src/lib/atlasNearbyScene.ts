import { groupCamera, featureCamera } from '../../../experiments/home-atlas/src/camera.ts';
import { distanceMetres } from '../../../experiments/home-atlas/src/geometry.ts';
import type { AtlasFeature } from '../../../experiments/home-atlas/src/types.ts';
import type { MapOverlayLine, MapOverlayPolygon } from './types.ts';
import type { NumberedPlace } from './nearbyPlateProjection.ts';

export type NearbyDepth = 'overview' | 'pair' | 'inspect' | 'home';
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
  lines: MapOverlayLine[], selectedId: string | null, depth: NearbyDepth, elevation: number, width: number) {
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
    tuning: {maxContextDistanceM: Number.POSITIVE_INFINITY}};
  const selected = places.find(p => (p.feature_id ?? p.name) === selectedId);
  const active = selected && depth !== 'overview' ? [selected] : places;
  const geometry = selected && depth !== 'overview' ? geometryForPlace(selected, polygons, lines) : {polygons, lines};
  const features = [
    ...active.map(p => makeFeature(p.feature_id ?? p.name, {lat:p.latitude, lng:p.longitude})),
    ...geometry.polygons.filter(p => p.coordinates.length).map(p => makeFeature(p.id, points(p.coordinates)[0], p)),
    ...geometry.lines.filter(l => l.coordinates.length).map(l => ({...makeFeature(l.id, points(l.coordinates)[0]), segments:[{id:l.id,path:points(l.coordinates)}]})),
  ];
  if (depth === 'home') return featureCamera(context, anchor);
  if (depth === 'inspect' && selected) {
    const focus = makeFeature(selected.feature_id ?? selected.name, {lat:selected.latitude,lng:selected.longitude});
    if (!geometry.polygons.length && !geometry.lines.length) return featureCamera(context, focus);
    return {...groupCamera({...context, home:focus}, features), tilt: geometry.polygons.length ? 30 : 50};
  }
  return groupCamera(context, [anchor, ...features]);
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
