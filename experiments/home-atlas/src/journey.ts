import type { SceneGeometry } from "../../../frontend/src/lib/types.ts";

export type AtlasPoint = { latitude: number; longitude: number };
export type AtlasRoute = {
  coordinates: [number, number][];
  cumulativeDistancesM: number[];
  lengthM: number;
};

export type AtlasJourneyStop = {
  id: string;
  point: AtlasPoint;
};

export type AtlasJourneyScene = {
  kind: "overview" | "arrival" | "corridor" | "focus" | "return";
  startMs: number;
  endMs: number;
  durationMs: number;
  stopId?: string;
  fromM?: number;
  toM?: number;
};

export type AtlasCameraPose = {
  center: AtlasPoint & { altitude: number };
  heading: number;
  range: number;
  tilt: number;
};

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

export function headingAlongRoute(route: AtlasRoute, distanceAlongM: number): number {
  const before = pointAlongRoute(route, distanceAlongM - 25);
  const after = pointAlongRoute(route, distanceAlongM + 55);
  const longitude = (after.longitude - before.longitude)
    * Math.cos(before.latitude * Math.PI / 180);
  return normalizeHeading(Math.atan2(longitude, after.latitude - before.latitude) * 180 / Math.PI);
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

export function buildAerialJourney(
  route: AtlasRoute,
  stops: AtlasJourneyStop[],
): AtlasJourneyScene[] {
  const orderedStops = stops
    .map((stop) => ({
      ...stop,
      distanceAlongM: projectPointOntoRoute(route, stop.point).distanceAlongM,
    }))
    .sort((left, right) => left.distanceAlongM - right.distanceAlongM);
  const scenes: Omit<AtlasJourneyScene, "startMs" | "endMs">[] = [
    { kind: "overview", durationMs: 6_500 },
  ];
  let previousDistanceM = 0;

  orderedStops.forEach((stop, index) => {
    scenes.push({
      kind: index === 0 ? "arrival" : "corridor",
      stopId: stop.id,
      fromM: previousDistanceM,
      toM: stop.distanceAlongM,
      durationMs: index === 0
        ? 6_000
        : Math.max(6_000, (stop.distanceAlongM - previousDistanceM) / 20 * 1_000),
    });
    scenes.push({
      kind: "focus",
      stopId: stop.id,
      fromM: stop.distanceAlongM,
      toM: stop.distanceAlongM,
      durationMs: 11_000,
    });
    previousDistanceM = stop.distanceAlongM;
  });
  scenes.push({ kind: "return", fromM: previousDistanceM, durationMs: 6_500 });

  let elapsedMs = 0;
  return scenes.map((scene) => {
    const timed = { ...scene, startMs: elapsedMs, endMs: elapsedMs + scene.durationMs };
    elapsedMs = timed.endMs;
    return timed;
  });
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
