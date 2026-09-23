import type { SceneGeometry } from "../types.ts";
import policy from "../atlasPolicy.ts";

export type AtlasPoint = { latitude: number; longitude: number };
export type AtlasRoute = {
  coordinates: [number, number][];
  cumulativeDistancesM: number[];
  lengthM: number;
};

export type AtlasCameraPose = {
  center: AtlasPoint & { altitude: number };
  heading: number;
  range: number;
  tilt: number;
};


export type AtlasRoadDirection = "as-mapped" | "reverse";

export type AtlasRoadFlightTuning = Readonly<{
  baseSpeedMps: number;
  minimumRate: number;
  maximumRate: number;
  lookBehindM: number;
  lookAheadM: number;
  altitudeOffsetM: number;
  flightHeightM: number;
  mobileFlightHeightM: number;
  maximumTilt: number;
  homeFocusWeight: number;
  mobileBreakpointPx: number;
}>;

export type AtlasStreetHandoff = {
  routePoint: AtlasPoint;
  distanceAlongM: number;
  distanceFromRouteM: number;
  heading: number;
};

export function selectPrimaryAtlasRoute(
  geometries: readonly SceneGeometry[],
  { direction = "as-mapped" }: { direction?: AtlasRoadDirection } = {},
): AtlasRoute {
  if (direction !== "as-mapped" && direction !== "reverse") {
    throw new Error("Atlas route direction must be as-mapped or reverse");
  }

  let selected: AtlasRoute | undefined;
  for (const geometry of geometries) {
    if (geometry.type !== "LineString" || geometry.coordinates.length < 2) continue;
    const candidate = buildAtlasRoute(geometry);
    if (!selected || candidate.lengthM > selected.lengthM) selected = candidate;
  }
  if (!selected) throw new Error("No continuous LineString is available for the Atlas route");
  if (direction === "as-mapped") return selected;

  return buildAtlasRoute({
    type: "LineString",
    coordinates: [...selected.coordinates].reverse(),
  });
}

export function clampRoadPlaybackRate(
  playbackRate: number,
  tuning: AtlasRoadFlightTuning = policy.road,
): number {
  return clamp(playbackRate, tuning.minimumRate, tuning.maximumRate);
}

export function advanceRoadDistance(
  route: AtlasRoute,
  currentDistanceM: number,
  elapsedMs: number,
  playbackRate: number,
  tuning: AtlasRoadFlightTuning = policy.road,
): number {
  const elapsedSeconds = Math.max(0, elapsedMs) / 1_000;
  const metres = currentDistanceM
    + elapsedSeconds * tuning.baseSpeedMps * clampRoadPlaybackRate(playbackRate, tuning);
  return clamp(metres, 0, route.lengthM);
}

export function roadFlightCamera(
  route: AtlasRoute,
  distanceAlongM: number,
  groundElevationM: number,
  home: AtlasPoint,
  flightHeightM: number,
  tuning: AtlasRoadFlightTuning = policy.road,
): AtlasCameraPose {
  const point = pointAlongRoute(route, distanceAlongM);
  const center = {
    latitude: mix(point.latitude, home.latitude, tuning.homeFocusWeight),
    longitude: mix(point.longitude, home.longitude, tuning.homeFocusWeight),
    altitude: groundElevationM + tuning.altitudeOffsetM,
  };
  const eastM = (center.longitude - point.longitude) * 111_320 * Math.cos(point.latitude * Math.PI / 180);
  const northM = (center.latitude - point.latitude) * 111_320;
  const horizontalM = Math.hypot(eastM, northM);
  const heightM = flightHeightM - tuning.altitudeOffsetM;
  return {
    center,
    heading: horizontalM > 1
      ? normalizeHeading(Math.atan2(eastM, northM) * 180 / Math.PI)
      : headingAlongRoute(route, distanceAlongM, tuning.lookBehindM, tuning.lookAheadM),
    range: Math.hypot(horizontalM, heightM),
    tilt: Math.atan2(horizontalM, heightM) * 180 / Math.PI,
  };
}

/** Clip a viewing itinerary to one existing line; never extend or connect roads. */
export function roadTourWindow(route: AtlasRoute, anchor: AtlasPoint, eitherSideM: number): AtlasRoute {
  const at = projectPointOntoRoute(route, anchor).distanceAlongM;
  const start = Math.max(0, at - eitherSideM);
  const end = Math.min(route.lengthM, at + eitherSideM);
  const first = pointAlongRoute(route, start);
  const last = pointAlongRoute(route, end);
  return buildAtlasRoute({
    type: 'LineString',
    coordinates: [
      [first.longitude, first.latitude],
      ...route.coordinates.filter((_, index) => route.cumulativeDistancesM[index] > start
        && route.cumulativeDistancesM[index] < end),
      [last.longitude, last.latitude],
    ],
  });
}

export function projectStreetHandoff(
  route: AtlasRoute,
  streetPoint: AtlasPoint,
  heading: number,
): AtlasStreetHandoff {
  const projection = projectPointOntoRoute(route, streetPoint);
  return {
    routePoint: pointAlongRoute(route, projection.distanceAlongM),
    distanceAlongM: projection.distanceAlongM,
    distanceFromRouteM: projection.distanceFromRouteM,
    heading: normalizeHeading(heading),
  };
}

export function buildAtlasRoute(geometry: SceneGeometry): AtlasRoute {
  if (geometry.type !== "LineString" || geometry.coordinates.length < 2) {
    throw new Error("An Atlas route requires one continuous LineString");
  }
  const cumulativeDistancesM = [0];
  for (let index = 1; index < geometry.coordinates.length; index += 1) {
    cumulativeDistancesM.push(
      cumulativeDistancesM[index - 1]
      + distanceM(geometry.coordinates[index - 1], geometry.coordinates[index]),
    );
  }
  return {
    coordinates: geometry.coordinates.map((coordinate) => [...coordinate]),
    cumulativeDistancesM,
    lengthM: cumulativeDistancesM.at(-1) ?? 0,
  };
}

export function pointAlongRoute(route: AtlasRoute, distanceAlongM: number): AtlasPoint {
  const distance = clamp(distanceAlongM, 0, route.lengthM);
  let index = 1;
  while (index < route.coordinates.length - 1 && route.cumulativeDistancesM[index] < distance) {
    index += 1;
  }
  const startDistance = route.cumulativeDistancesM[index - 1];
  const segmentLength = route.cumulativeDistancesM[index] - startDistance;
  const progress = segmentLength > 0 ? (distance - startDistance) / segmentLength : 0;
  const [startLongitude, startLatitude] = route.coordinates[index - 1];
  const [endLongitude, endLatitude] = route.coordinates[index];
  return {
    latitude: mix(startLatitude, endLatitude, progress),
    longitude: mix(startLongitude, endLongitude, progress),
  };
}

export function headingAlongRoute(
  route: AtlasRoute,
  distanceAlongM: number,
  lookBehindM = 25,
  lookAheadM = 55,
): number {
  const before = pointAlongRoute(route, distanceAlongM - lookBehindM);
  const after = pointAlongRoute(route, distanceAlongM + lookAheadM);
  const longitude = (after.longitude - before.longitude)
    * Math.cos(before.latitude * Math.PI / 180);
  return normalizeHeading(Math.atan2(longitude, after.latitude - before.latitude) * 180 / Math.PI);
}

export function dampHeading(
  currentHeading: number,
  targetHeading: number,
  damping: number,
  elapsedSeconds: number,
): number {
  const delta = (targetHeading - currentHeading + 540) % 360 - 180;
  const progress = 1 - Math.exp(-Math.max(0, elapsedSeconds) * Math.max(0, damping));
  return normalizeHeading(currentHeading + delta * progress);
}

export function projectPointOntoRoute(
  route: AtlasRoute,
  point: AtlasPoint,
): { distanceAlongM: number; distanceFromRouteM: number } {
  const longitudeScale = 111_320 * Math.cos(point.latitude * Math.PI / 180);
  const latitudeScale = 110_570;
  let nearest = { distanceAlongM: 0, distanceFromRouteM: Number.POSITIVE_INFINITY };

  for (let index = 1; index < route.coordinates.length; index += 1) {
    const segment = projectOntoSegment(
      route.coordinates[index - 1],
      route.coordinates[index],
      point,
      longitudeScale,
      latitudeScale,
    );
    if (segment.distanceM < nearest.distanceFromRouteM) {
      const segmentLength = route.cumulativeDistancesM[index] - route.cumulativeDistancesM[index - 1];
      nearest = {
        distanceAlongM: route.cumulativeDistancesM[index - 1] + segment.progress * segmentLength,
        distanceFromRouteM: segment.distanceM,
      };
    }
  }
  return nearest;
}

export function blendCamera(
  left: AtlasCameraPose,
  right: AtlasCameraPose,
  progress: number,
): AtlasCameraPose {
  const eased = smoothStep(clamp(progress, 0, 1));
  const headingDelta = (right.heading - left.heading + 540) % 360 - 180;
  return {
    center: {
      latitude: mix(left.center.latitude, right.center.latitude, eased),
      longitude: mix(left.center.longitude, right.center.longitude, eased),
      altitude: mix(left.center.altitude, right.center.altitude, eased),
    },
    heading: normalizeHeading(left.heading + headingDelta * eased),
    range: mix(left.range, right.range, eased),
    tilt: mix(left.tilt, right.tilt, eased),
  };
}

function projectOntoSegment(
  start: [number, number],
  end: [number, number],
  point: AtlasPoint,
  longitudeScale: number,
  latitudeScale: number,
): { progress: number; distanceM: number } {
  const startX = (start[0] - point.longitude) * longitudeScale;
  const startY = (start[1] - point.latitude) * latitudeScale;
  const endX = (end[0] - point.longitude) * longitudeScale;
  const endY = (end[1] - point.latitude) * latitudeScale;
  const dx = endX - startX;
  const dy = endY - startY;
  const lengthSquared = dx * dx + dy * dy;
  const progress = lengthSquared > 0 ? clamp(-(startX * dx + startY * dy) / lengthSquared, 0, 1) : 0;
  return { progress, distanceM: Math.hypot(startX + progress * dx, startY + progress * dy) };
}

function distanceM(left: [number, number], right: [number, number]): number {
  const meanLatitude = (left[1] + right[1]) * Math.PI / 360;
  const longitudeM = (right[0] - left[0]) * 111_320 * Math.cos(meanLatitude);
  const latitudeM = (right[1] - left[1]) * 110_570;
  return Math.hypot(longitudeM, latitudeM);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

function mix(left: number, right: number, progress: number): number {
  return left + (right - left) * progress;
}

function smoothStep(value: number): number {
  return value * value * (3 - 2 * value);
}

function normalizeHeading(value: number): number {
  return (value % 360 + 360) % 360;
}
