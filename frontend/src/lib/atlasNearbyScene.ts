import { distanceMetres } from './atlas/geometry.ts';
import {
  bearingDegrees,
  fitCameraToScreen,
  type AtlasFitPoint,
  type AtlasScreenFrame,
} from './atlas/screenFit.ts';
import policy from './atlasPolicy.ts';
import type { MapOverlayLine, MapOverlayPolygon } from './types.ts';
import type { NumberedPlace } from './nearbyPlateProjection.ts';

export type NearbyDepth = 'overview' | 'pair' | 'inspect' | 'home';
export type NearbyCameraOrientation = 'category-stable' | 'selected-home-foreground';
export type AtlasSafeFrame = AtlasScreenFrame;
type Home = { latitude: number; longitude: number; name: string; boundary?: MapOverlayPolygon };
const points = (coordinates: [number, number][]) => coordinates.map(([lng, lat]) => ({ lat, lng }));

/** Fit the whole home to the available canvas, using the opening pose as a minimum. */
export function homeSceneCamera(home: Home, elevation: number, frame: AtlasSafeFrame, tiltOverride?: number) {
  const origin = {lat: home.latitude, lng: home.longitude};
  const ground = home.boundary?.coordinates.length ? points(home.boundary.coordinates) : [origin];
  const fieldOfViewDegrees = policy.cameraFit.homeFieldOfViewDegrees;
  const camera = fitCameraToScreen({
    points: [...ground, {...origin, heightM: policy.nearby.markerLiftM}],
    frame,
    heading: policy.cameraFit.homeHeadingDegrees,
    tilt: tiltOverride ?? policy.cameraFit.homeTilt,
    minimumRangeM: policy.cameraFit.homeMinimumRangeM,
    fieldOfViewDegrees,
    opticalPaddingPx: policy.cameraFit.opticalPaddingPx,
    altitudeM: elevation + policy.cameraFit.homeCenterAltitudeOffsetM,
  });
  return {...camera, fov: fieldOfViewDegrees};
}

/** Orbit preserves the complete-boundary opening instead of shrinking Home as motion starts. */
export function homeOrbitCamera(home: Home, elevation: number, frame: AtlasSafeFrame) {
  return homeSceneCamera(home, elevation, frame);
}

/** Join only explicit identity. Never guess ownership from names or proximity. */
export function geometryForPlace(place: NumberedPlace, polygons: MapOverlayPolygon[], lines: MapOverlayLine[]) {
  const owns = (shape: { id: string; entity_id?: string }) =>
    Boolean(place.place_entity_id && shape.entity_id === place.place_entity_id)
    || [place.feature_id, ...(place.feature_ids ?? [])].some(id =>
      Boolean(id && (shape.id === id || shape.id.startsWith(`${id}:`))));
  return { polygons: polygons.filter(owns), lines: lines.filter(owns) };
}

export function nearbySceneCamera(home: Home, places: NumberedPlace[], polygons: MapOverlayPolygon[],
  lines: MapOverlayLine[], selectedId: string | null, depth: NearbyDepth, elevation: number, width: number,
  safeFrame?: AtlasSafeFrame, tiltOverride?: number,
  orientation: NearbyCameraOrientation = 'category-stable', orientationPlaces = places) {
  const origin = { lat: home.latitude, lng: home.longitude };
  const selected = places.find(p => (p.id) === selectedId);
  const geometry = selected && depth !== 'overview' ? geometryForPlace(selected, polygons, lines) : {polygons, lines};
  const frame = safeFrame ?? {
    width,
    height: Math.max(390, width * 0.7),
    left: policy.cameraFit.safeMarginPx,
    right: policy.cameraFit.safeMarginPx,
    top: policy.cameraFit.safeMarginPx,
    bottom: policy.cameraFit.safeMarginPx,
  };
  const homeGround = home.boundary?.coordinates.length
    ? points(home.boundary.coordinates)
    : [origin];
  const selectedAnchor = selected
    ? {lat: selected.latitude, lng: selected.longitude}
    : undefined;
  const geometryGround = [
    ...geometry.polygons.flatMap((polygon) => points(polygon.coordinates)),
    ...geometry.lines.flatMap((line) => points(line.coordinates)),
  ];
  const marker = (point: {lat: number; lng: number}): AtlasFitPoint => ({
    ...point,
    heightM: policy.nearby.markerLiftM,
  });
  let fitPoints: AtlasFitPoint[];
  if (depth === 'home') {
    fitPoints = [...homeGround, marker(origin)];
  } else if (selected && selectedAnchor && depth !== 'overview') {
    fitPoints = [
      ...homeGround,
      ...geometryGround,
      marker(origin),
      marker(selectedAnchor),
      ...nearbyRelationArc(home, selected).map((point) => ({
        lat: point.lat,
        lng: point.lng,
        heightM: point.altitude,
      })),
    ];
  } else {
    fitPoints = [
      ...homeGround,
      ...geometryGround,
      marker(origin),
      ...places.map((place) => marker({lat: place.latitude, lng: place.longitude})),
    ];
  }
  const categoryTarget = orientationPlaces.length
    ? {
      lat: orientationPlaces.reduce((total, place) => total + place.latitude, 0) / orientationPlaces.length,
      lng: orientationPlaces.reduce((total, place) => total + place.longitude, 0) / orientationPlaces.length,
    }
    : origin;
  // Ordinary Nearby inspection follows the selected relationship so Home can
  // remain in the foreground. Metro keeps its category-stable network view.
  const selectedHomeForeground = orientation === 'selected-home-foreground'
    && selectedAnchor
    && (depth === 'pair' || depth === 'inspect');
  const relationshipHeading = bearingDegrees(origin,
    selectedHomeForeground ? selectedAnchor : categoryTarget);
  const heading = depth === 'home'
    ? policy.cameraFit.homeHeadingDegrees
    : selectedHomeForeground
    ? relationshipHeading + policy.cameraFit.selectedHomeForegroundHeadingOffsetDegrees
    : relationshipHeading + (depth === 'inspect'
      ? policy.cameraFit.inspectHeadingOffsetDegrees
      : depth === 'pair'
      ? policy.cameraFit.pairHeadingOffsetDegrees
      : policy.cameraFit.overviewHeadingOffsetDegrees);
  const tilt = depth === 'home'
    ? policy.cameraFit.homeTilt
    : depth === 'inspect'
    ? policy.cameraFit.inspectTilt
    : depth === 'pair'
    ? policy.cameraFit.pairTilt
    : policy.cameraFit.overviewTilt;
  const minimumRangeM = depth === 'home'
    ? policy.cameraFit.homeMinimumRangeM
    : depth === 'inspect'
    ? policy.cameraFit.inspectMinimumRangeM
    : depth === 'pair'
    ? policy.nearby.pairMinimumRangeM
    : policy.cameraFit.overviewMinimumRangeM;
  const fieldOfViewDegrees = depth === 'home'
    ? policy.cameraFit.homeFieldOfViewDegrees
    : policy.cameraFit.fieldOfViewDegrees;
  const fitInput = {
    points: fitPoints,
    frame,
    heading,
    tilt: tiltOverride ?? tilt,
    // Home remains the visual anchor; sourced context is still a hard fit constraint.
    focusPoint: policy.cameraFit.preferredSubject === 'home' ? marker(origin)
      : depth === 'inspect' && selectedAnchor ? marker(selectedAnchor) : undefined,
    fieldOfViewDegrees,
    minimumRangeM: width < policy.road.mobileBreakpointPx
      ? minimumRangeM * policy.society.mobileRangeScale
      : minimumRangeM,
    opticalPaddingPx: policy.cameraFit.opticalPaddingPx,
    altitudeM: elevation + (depth === 'home'
      ? policy.cameraFit.homeCenterAltitudeOffsetM
      : policy.cameraFit.centerAltitudeOffsetM),
  };
  const camera = fitCameraToScreen(fitInput);
  const framed = depth === 'pair' ? fitCameraToScreen({...fitInput,
    minimumRangeM: camera.range * policy.cameraFit.pairContextScale}) : camera;
  return { ...framed, fov: fieldOfViewDegrees };
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
