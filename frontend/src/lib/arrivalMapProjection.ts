import type { MapOverlayLine, PropertyMapContext } from "./types.ts";
import {
  resolveHomeAnchor,
  zoomForRadiusKm,
  type NumberedPlace,
  type PlateViewport,
} from "./nearbyPlateProjection.ts";

export type ArrivalCameraMode = "home" | "evidence";

export type CorridorCameraFocus = {
  latitude: number;
  longitude: number;
  heading: number;
};

export type CorridorTourWaypoint = CorridorCameraFocus & {
  offsetM: number;
};

export type CorridorTourMode = "center_out_and_back" | "end_to_end";

type CorridorProjection = CorridorCameraFocus & {
  coordinates: [number, number][];
  segmentLengthsM: number[];
  distanceAlongM: number;
  totalDistanceM: number;
  directionSign: 1 | -1;
  longitudeMeters: number;
  latitudeMeters: number;
};

const METRO_CORRIDOR_BUFFER_KM = 0.9;
const METRO_MAX_SEGMENTS = 3;
const VIEWPORT_PADDING = 1.35;

export function hasArrivalMap(context?: PropertyMapContext | null): boolean {
  if (!context || !resolveHomeAnchor(context)) return false;
  const roadLayer = context.layers?.find((layer) => layer.renderKind === "terrain_corridor");
  const roadLines = roadLayer
    ? context.layer_lines?.[roadLayer.id] ?? context.access_lines ?? []
    : [];
  return Boolean(
    context.home.boundary
    || context.metro_lines?.length
    || (roadLines.length > 0 && roadLayer?.experience?.kind === "street_view_tour"),
  );
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function distanceKm(
  lat1: number,
  lng1: number,
  lat2: number,
  lng2: number,
): number {
  const dLat = (lat2 - lat1) * 110.57;
  const dLng = (lng2 - lng1) * 111.32 * Math.cos((lat1 * Math.PI) / 180);
  return Math.hypot(dLat, dLng);
}

function normalizeHeading(heading: number): number {
  return (heading % 360 + 360) % 360;
}

export function cameraCenterForMode(
  mode: ArrivalCameraMode,
  home: { latitude: number; longitude: number },
  viewport: PlateViewport,
): { latitude: number; longitude: number } {
  return mode === "evidence" ? viewport.center : home;
}

export function arrivalEvidenceViewport(
  home: { latitude: number; longitude: number },
  places: NumberedPlace[],
  lines: MapOverlayLine[],
): PlateViewport {
  const coordinates: [number, number][] = [
    [home.longitude, home.latitude],
    ...places.map((place): [number, number] => [place.longitude, place.latitude]),
    ...lines.flatMap((line) => line.coordinates),
  ];
  const longitudes = coordinates.map(([longitude]) => longitude);
  const latitudes = coordinates.map(([, latitude]) => latitude);
  const center = {
    latitude: (Math.min(...latitudes) + Math.max(...latitudes)) / 2,
    longitude: (Math.min(...longitudes) + Math.max(...longitudes)) / 2,
  };
  const radiusKm = clamp(
    Math.max(
      ...coordinates.map(([longitude, latitude]) =>
        distanceKm(center.latitude, center.longitude, latitude, longitude)),
    ) * VIEWPORT_PADDING,
    0.7,
    15,
  );
  return {
    center,
    radiusKm,
    zoom: zoomForRadiusKm(radiusKm),
    paddingFactor: 0.22,
  };
}

export function metroLinesNearArrival(
  home: { latitude: number; longitude: number },
  places: NumberedPlace[],
  metroLines: MapOverlayLine[],
): MapOverlayLine[] {
  if (metroLines.length <= METRO_MAX_SEGMENTS) return metroLines;
  const anchors: [number, number][] = [
    [home.longitude, home.latitude],
    ...places.map((place): [number, number] => [place.longitude, place.latitude]),
  ];
  const scored = metroLines
    .map((line) => ({
      line,
      distanceKm: Math.min(
        ...line.coordinates.flatMap(([longitude, latitude]) =>
          anchors.map(([anchorLongitude, anchorLatitude]) => distanceKm(
            latitude,
            longitude,
            anchorLatitude,
            anchorLongitude,
          ))),
      ),
    }))
    .sort((left, right) => left.distanceKm - right.distanceKm);
  const nearestDistanceKm = scored[0]?.distanceKm ?? 0;
  return scored
    .filter(({ distanceKm: lineDistanceKm }) =>
      lineDistanceKm <= nearestDistanceKm + METRO_CORRIDOR_BUFFER_KM)
    .slice(0, METRO_MAX_SEGMENTS)
    .map(({ line }) => line);
}

export function corridorCameraFocus(
  lines: MapOverlayLine[],
  home: { latitude: number; longitude: number },
): CorridorCameraFocus | null {
  const projection = nearestCorridorProjection(lines, home);
  if (!projection) return null;
  return {
    latitude: projection.latitude,
    longitude: projection.longitude,
    heading: projection.heading,
  };
}

export function corridorTourWaypoints(
  lines: MapOverlayLine[],
  home: { latitude: number; longitude: number },
  distanceEachDirectionM: number,
  waypointSpacingM: number,
  tourMode: CorridorTourMode = "center_out_and_back",
  additionalOffsetsM: number[] = [],
): CorridorTourWaypoint[] {
  const projection = nearestCorridorProjection(lines, home);
  if (!projection || distanceEachDirectionM <= 0 || waypointSpacingM <= 0) return [];
  const offsets = tourMode === "end_to_end"
    ? fullCorridorOffsets(projection, waypointSpacingM)
    : boundedCorridorOffsets(distanceEachDirectionM, waypointSpacingM);
  additionalOffsetsM.forEach((offsetM) => offsets.add(offsetM));

  return [...offsets]
    .sort((left, right) => left - right)
    .map((offsetM) => {
      const targetDistance = clamp(
        projection.distanceAlongM + offsetM * projection.directionSign,
        0,
        projection.totalDistanceM,
      );
      const point = pointAlongCorridor(projection, targetDistance);
      return {
        ...point,
        offsetM: (targetDistance - projection.distanceAlongM) / projection.directionSign,
      };
    })
    .filter((waypoint, index, waypoints) => index === 0
      || Math.abs(waypoint.offsetM - waypoints[index - 1].offsetM) >= 1);
}

function boundedCorridorOffsets(
  distanceEachDirectionM: number,
  waypointSpacingM: number,
): Set<number> {
  const offsets = new Set<number>([0]);
  for (
    let distance = waypointSpacingM;
    distance < distanceEachDirectionM;
    distance += waypointSpacingM
  ) {
    offsets.add(distance);
    offsets.add(-distance);
  }
  offsets.add(distanceEachDirectionM);
  offsets.add(-distanceEachDirectionM);
  return offsets;
}

function fullCorridorOffsets(
  projection: CorridorProjection,
  waypointSpacingM: number,
): Set<number> {
  const endpointOffsets = [
    (0 - projection.distanceAlongM) / projection.directionSign,
    (projection.totalDistanceM - projection.distanceAlongM) / projection.directionSign,
  ].sort((left, right) => left - right);
  const startOffset = endpointOffsets[0] ?? 0;
  const endOffset = endpointOffsets[1] ?? 0;
  const offsets = new Set<number>([startOffset, 0, endOffset]);
  for (
    let offset = startOffset + waypointSpacingM;
    offset < endOffset;
    offset += waypointSpacingM
  ) {
    offsets.add(offset);
  }
  return offsets;
}

function nearestCorridorProjection(
  lines: MapOverlayLine[],
  home: { latitude: number; longitude: number },
): CorridorProjection | null {
  const longitudeMeters = 111_320 * Math.cos((home.latitude * Math.PI) / 180);
  const latitudeMeters = 110_570;
  let nearest: (CorridorProjection & { distance: number }) | null = null;

  for (const line of lines) {
    const segmentLengthsM = line.coordinates.slice(1).map(([longitude, latitude], index) => {
      const [previousLongitude, previousLatitude] = line.coordinates[index];
      return Math.hypot(
        (longitude - previousLongitude) * longitudeMeters,
        (latitude - previousLatitude) * latitudeMeters,
      );
    });
    let distanceBeforeM = 0;
    for (let index = 1; index < line.coordinates.length; index += 1) {
      const [startLongitude, startLatitude] = line.coordinates[index - 1];
      const [endLongitude, endLatitude] = line.coordinates[index];
      const startX = (startLongitude - home.longitude) * longitudeMeters;
      const startY = (startLatitude - home.latitude) * latitudeMeters;
      const endX = (endLongitude - home.longitude) * longitudeMeters;
      const endY = (endLatitude - home.latitude) * latitudeMeters;
      const dx = endX - startX;
      const dy = endY - startY;
      const lengthSquared = dx * dx + dy * dy;
      if (lengthSquared === 0) continue;

      const progress = clamp(-(startX * dx + startY * dy) / lengthSquared, 0, 1);
      const x = startX + progress * dx;
      const y = startY + progress * dy;
      const distance = Math.hypot(x, y);
      if (!nearest || distance < nearest.distance) {
        const bearing = normalizeHeading(Math.atan2(dx, dy) * 180 / Math.PI);
        const directionSign = bearing >= 180 ? -1 : 1;
        nearest = {
          latitude: home.latitude + y / latitudeMeters,
          longitude: home.longitude + x / longitudeMeters,
          distance,
          heading: directionSign === 1 ? bearing : normalizeHeading(bearing + 180),
          coordinates: line.coordinates,
          segmentLengthsM,
          distanceAlongM: distanceBeforeM + progress * Math.sqrt(lengthSquared),
          totalDistanceM: segmentLengthsM.reduce((sum, length) => sum + length, 0),
          directionSign,
          longitudeMeters,
          latitudeMeters,
        };
      }
      distanceBeforeM += segmentLengthsM[index - 1] ?? 0;
    }
  }
  return nearest;
}

function pointAlongCorridor(
  corridor: CorridorProjection,
  distanceAlongM: number,
): CorridorCameraFocus {
  let distanceBeforeM = 0;
  for (let index = 1; index < corridor.coordinates.length; index += 1) {
    const segmentLengthM = corridor.segmentLengthsM[index - 1] ?? 0;
    const segmentEndM = distanceBeforeM + segmentLengthM;
    if (distanceAlongM <= segmentEndM || index === corridor.coordinates.length - 1) {
      const progress = segmentLengthM > 0
        ? clamp((distanceAlongM - distanceBeforeM) / segmentLengthM, 0, 1)
        : 0;
      const [startLongitude, startLatitude] = corridor.coordinates[index - 1];
      const [endLongitude, endLatitude] = corridor.coordinates[index];
      const bearing = normalizeHeading(Math.atan2(
        (endLongitude - startLongitude) * corridor.longitudeMeters,
        (endLatitude - startLatitude) * corridor.latitudeMeters,
      ) * 180 / Math.PI);
      return {
        latitude: startLatitude + (endLatitude - startLatitude) * progress,
        longitude: startLongitude + (endLongitude - startLongitude) * progress,
        heading: corridor.directionSign === 1
          ? bearing
          : normalizeHeading(bearing + 180),
      };
    }
    distanceBeforeM = segmentEndM;
  }
  return corridor;
}
