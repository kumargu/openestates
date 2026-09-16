import {
  bearingDegrees,
  fitCameraToScreen,
  type AtlasFitPoint,
  type AtlasScreenFrame,
} from '../../../experiments/home-atlas/src/screenFit.ts';
import { atlasPolicy as policy } from './atlasUiPolicy.ts';
import type { MapOverlayLine, MapOverlayPolygon } from './types.ts';
import type { NumberedPlace } from './nearbyPlateProjection.ts';

export type NearbyDepth = 'overview' | 'pair' | 'inspect' | 'home';
export type AtlasSafeFrame = AtlasScreenFrame;
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
  safeFrame?: AtlasSafeFrame, tiltOverride?: number, focusHome = false, headingOffset = 0) {
  const origin = { lat: home.latitude, lng: home.longitude };
  const selected = places.find(p => (p.feature_id ?? p.name) === selectedId);
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
  } else if (selectedAnchor && depth === 'inspect') {
    // Inspection fits the actual destination, independent of its distance from home.
    fitPoints = [...geometryGround, selectedAnchor, marker(selectedAnchor)];
  } else if (selected && selectedAnchor && depth !== 'overview') {
    fitPoints = [
      ...homeGround,
      ...geometryGround,
      marker(origin),
      marker(selectedAnchor),
    ];
  } else {
    fitPoints = [
      ...homeGround,
      ...geometryGround,
      marker(origin),
      ...places.map((place) => marker({lat: place.latitude, lng: place.longitude})),
    ];
  }
  // A selected pair uses its own relationship axis so distance is spent across
  // the wide screen dimension instead of forcing a category-oriented zoom-out.
  const target = selectedAnchor && (depth === 'pair' || depth === 'inspect')
    ? selectedAnchor
    : places.length
    ? {
      lat: places.reduce((total, place) => total + place.latitude, 0) / places.length,
      lng: places.reduce((total, place) => total + place.longitude, 0) / places.length,
    }
    : origin;
  const relationshipHeading = bearingDegrees(origin, target);
  const heading = depth === 'home'
    ? policy.cameraFit.homeHeadingDegrees
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
    ? policy.focus.rangeM
    : depth === 'inspect'
    ? policy.cameraFit.inspectMinimumRangeM
    : depth === 'pair'
    ? policy.nearby.pairMinimumRangeM
    : policy.cameraFit.overviewMinimumRangeM;
  const fit = (heading: number) => fitCameraToScreen({
    points: fitPoints,
    frame,
    heading: heading + headingOffset,
    tilt: tiltOverride ?? tilt,
    focusPoint: depth === 'inspect' && selectedAnchor
      ? marker(selectedAnchor)
      : focusHome
      ? marker(origin)
      : undefined,
    fieldOfViewDegrees: policy.cameraFit.fieldOfViewDegrees,
    minimumRangeM: width < policy.road.mobileBreakpointPx
      ? minimumRangeM * policy.society.mobileRangeScale
      : minimumRangeM,
    opticalPaddingPx: policy.cameraFit.opticalPaddingPx,
    altitudeM: elevation + policy.cameraFit.centerAltitudeOffsetM,
  });
  const camera = fit(heading);
  if (depth !== 'pair' || headingOffset !== 0) return camera;
  // Let portrait and narrow split views use height when that gives the
  // relationship more presence. Fit the actual geometry in both orientations.
  const portrait = fit(heading - policy.cameraFit.pairHeadingOffsetDegrees);
  return portrait.range < camera.range ? portrait : camera;
}
