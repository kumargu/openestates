import type {
  MapOverlayLine,
  MapPlacePin,
  PropertyMapContext,
  ProofFocus,
} from "./types.ts";

export type PlateScaleMode = "nearby" | "area";
export type NearbyCameraMode = "home" | "evidence";
export type PlateStory =
  | { kind: "layer"; layer: string }
  | { kind: "water" };

export const PLATE_MAX_MAP_LABEL_LENGTH = 22;

const NEARBY_RADIUS_STEPS_KM = [0.35, 0.5, 0.8, 1.2, 1.8, 2.5] as const;
const AREA_RADIUS_STEPS_KM = [3, 5, 8, 10, 15] as const;
const CLUSTER_GAP_KM_NEARBY = 0.08;
const CLUSTER_GAP_KM_AREA = 0.35;
const LOCAL_METRO_CORRIDOR_BUFFER_KM = 0.9;
const LOCAL_METRO_MAX_SEGMENTS = 3;
/** Keep markers inside the canvas, not glued to the ring edge. */
const VIEWPORT_PADDING = 1.45;

export function hasAroundThisHomePlate(context?: PropertyMapContext | null): boolean {
  return Boolean(
    context && (
      context.places.length > 0
      || context.water
      || (context.metro_lines?.length ?? 0) > 0
      || (context.access_lines?.length ?? 0) > 0
      || (context.red_flag_lines?.length ?? 0) > 0
    ),
  );
}

export type NumberedPlace = MapPlacePin & {
  id: string;
  number: number;
  latitude: number;
  longitude: number;
};

export type PlaceCluster = {
  id: string;
  latitude: number;
  longitude: number;
  count: number;
  placeIds: string[];
  layer: string;
};

export type PlateViewport = {
  center: { latitude: number; longitude: number };
  radiusKm: number;
  zoom: number;
  paddingFactor: number;
};

export type CorridorCameraFocus = {
  latitude: number;
  longitude: number;
  heading: number;
};

export function cameraCenterForMode(
  mode: NearbyCameraMode,
  home: { latitude: number; longitude: number },
  viewport: PlateViewport,
): { latitude: number; longitude: number } {
  return mode === "evidence" ? viewport.center : home;
}

export function corridorCameraFocus(
  lines: MapOverlayLine[],
  home: { latitude: number; longitude: number },
): CorridorCameraFocus | null {
  const longitudeMeters = 111_320 * Math.cos((home.latitude * Math.PI) / 180);
  const latitudeMeters = 110_570;
  let nearest:
    | { x: number; y: number; distance: number; heading: number }
    | null = null;

  for (const line of lines) {
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

      const progress = clamp(
        -(startX * dx + startY * dy) / lengthSquared,
        0,
        1,
      );
      const x = startX + progress * dx;
      const y = startY + progress * dy;
      const distance = Math.hypot(x, y);
      if (nearest && nearest.distance <= distance) continue;

      const bearing = (Math.atan2(dx, dy) * 180 / Math.PI + 360) % 360;
      nearest = {
        x,
        y,
        distance,
        // A road can be encoded in either direction. Keep the camera orientation
        // stable by choosing the north/east-facing direction of the same axis.
        heading: bearing >= 180 ? bearing - 180 : bearing,
      };
    }
  }

  if (!nearest) return null;
  return {
    latitude: home.latitude + nearest.y / latitudeMeters,
    longitude: home.longitude + nearest.x / longitudeMeters,
    heading: nearest.heading,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function compactPlaceLabel(name: string): string {
  const primaryName = name
    .split(/\s(?:-|–|\|)\s/)[0]
    .replace(/\s+(?:in|at)\s+[A-Za-z].*$/i, "")
    .trim();
  if (primaryName.length <= PLATE_MAX_MAP_LABEL_LENGTH) return primaryName;

  const words = primaryName.split(/\s+/);
  let compact = "";
  for (const word of words) {
    const candidate = compact ? `${compact} ${word}` : word;
    if (candidate.length > PLATE_MAX_MAP_LABEL_LENGTH - 1) break;
    compact = candidate;
  }
  return compact ? `${compact}…` : `${primaryName.slice(0, PLATE_MAX_MAP_LABEL_LENGTH - 1)}…`;
}

export function placeId(place: MapPlacePin, index = 0): string {
  return place.feature_id ?? place.place_entity_id ?? `${place.layer}-${place.name}-${index}`;
}

export function placeMatchesProofFocus(place: MapPlacePin, focus?: ProofFocus | null): boolean {
  if (!focus) return false;
  if (place.layer !== focus.layerId) return false;
  if (focus.featureId && place.feature_id === focus.featureId) return true;
  if (focus.entityId && place.place_entity_id === focus.entityId) return true;
  if (focus.matchedLabel && textContains(place.name, focus.matchedLabel)) return true;
  if (focus.matchedValue && textContains(focus.matchedValue, place.name)) return true;
  return false;
}

function textContains(value: string, needle: string): boolean {
  return value.toLocaleLowerCase("en-IN").includes(needle.toLocaleLowerCase("en-IN"));
}

export function resolveHomeAnchor(context: PropertyMapContext): {
  latitude: number;
  longitude: number;
  approximated: boolean;
} | null {
  if (
    typeof context.home.latitude === "number"
    && typeof context.home.longitude === "number"
  ) {
    return {
      latitude: context.home.latitude,
      longitude: context.home.longitude,
      approximated: false,
    };
  }

  const coords = context.places.filter(
    (place): place is MapPlacePin & { latitude: number; longitude: number } =>
      typeof place.latitude === "number" && typeof place.longitude === "number",
  );
  if (coords.length === 0) return null;

  // Prefer places that claim to be closest to the society — a full centroid
  // drifts toward far metro/tech parks when the home itself has no geo facts.
  const near = coords
    .slice()
    .sort((left, right) =>
      (left.distance_km ?? Number.POSITIVE_INFINITY)
      - (right.distance_km ?? Number.POSITIVE_INFINITY))
    .slice(0, 3);

  const latitude = near.reduce((sum, place) => sum + place.latitude, 0) / near.length;
  const longitude = near.reduce((sum, place) => sum + place.longitude, 0) / near.length;
  return { latitude, longitude, approximated: true };
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

function coordinatesForPlaces(places: NumberedPlace[]): [number, number][] {
  return places.map((place) => [place.longitude, place.latitude]);
}

function boundsCenter(
  coordinates: [number, number][],
): { latitude: number; longitude: number } {
  const longitudes = coordinates.map(([longitude]) => longitude);
  const latitudes = coordinates.map(([, latitude]) => latitude);
  return {
    latitude: (Math.min(...latitudes) + Math.max(...latitudes)) / 2,
    longitude: (Math.min(...longitudes) + Math.max(...longitudes)) / 2,
  };
}

export function metroLinesNearEvidence(
  home: { latitude: number; longitude: number },
  places: NumberedPlace[],
  metroLines: MapOverlayLine[],
  accessLines: MapOverlayLine[] = [],
): MapOverlayLine[] {
  if (metroLines.length <= LOCAL_METRO_MAX_SEGMENTS) return metroLines;

  const anchors: [number, number][] = [
    [home.longitude, home.latitude],
    ...coordinatesForPlaces(places),
    ...accessLines.flatMap((line) => line.coordinates),
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
      lineDistanceKm <= nearestDistanceKm + LOCAL_METRO_CORRIDOR_BUFFER_KM)
    .slice(0, LOCAL_METRO_MAX_SEGMENTS)
    .map(({ line }) => line);
}

export function placesForStory(
  context: PropertyMapContext,
  story: PlateStory,
): MapPlacePin[] {
  if (story.kind === "water") {
    return [];
  }

  return context.places.filter((place) => place.layer === story.layer);
}

export function linesForLayer(
  context: PropertyMapContext,
  layer: string,
): MapOverlayLine[] {
  const projected = context.layer_lines?.[layer];
  if (projected) return projected;
  if (layer === "metro") return context.access_lines ?? [];
  if (layer === "red_flags") return context.red_flag_lines ?? [];
  return [];
}

export function filterPlacesByScale(
  places: MapPlacePin[],
  scale: PlateScaleMode,
  focus?: ProofFocus | null,
): MapPlacePin[] {
  const farthestCuratedKm = Math.max(
    0,
    ...places
      .map((place) => place.distance_km)
      .filter((value): value is number => typeof value === "number" && Number.isFinite(value)),
  );
  const maxKm = scale === "nearby" ? Math.max(1.5, Math.min(2.5, farthestCuratedKm)) : 15;
  return places.filter((place) => placeMatchesProofFocus(place, focus)
    || (place.distance_km ?? 0) <= maxKm
    || typeof place.distance_km !== "number");
}

export function scaleForStory(
  story: PlateStory,
  focus?: ProofFocus | null,
  focusedPlaces: MapPlacePin[] = [],
): PlateScaleMode {
  if (story.kind === "water") return "area";
  if (focus && story.layer === focus.layerId) {
    const focusDistanceKm = typeof focus.distanceM === "number"
      ? focus.distanceM / 1000
      : focusedPlaces
        .filter((place) => placeMatchesProofFocus(place, focus))
        .map((place) => place.distance_km)
        .find((distance): distance is number => typeof distance === "number");
    const nearbyCapKm = NEARBY_RADIUS_STEPS_KM[NEARBY_RADIUS_STEPS_KM.length - 1];
    if (typeof focusDistanceKm === "number" && focusDistanceKm > nearbyCapKm) {
      return "area";
    }
  }
  if (story.kind === "layer" && (story.layer === "tech" || story.layer === "red_flags")) {
    return "area";
  }
  return "nearby";
}

export function metroStationsAroundHome(
  places: MapPlacePin[],
  home: { latitude: number; longitude: number },
  metroLines: MapOverlayLine[],
  focus?: ProofFocus | null,
): MapPlacePin[] {
  if (places.length <= 2) return places;

  const latitudeScale = Math.cos((home.latitude * Math.PI) / 180);
  const toLocalPoint = (longitude: number, latitude: number): [number, number] => [
    (longitude - home.longitude) * latitudeScale,
    latitude - home.latitude,
  ];

  let nearestSegment:
    | { projection: [number, number]; tangent: [number, number] }
    | undefined;
  let nearestDistance = Number.POSITIVE_INFINITY;

  for (const line of metroLines) {
    for (let index = 1; index < line.coordinates.length; index += 1) {
      const start = toLocalPoint(...line.coordinates[index - 1]);
      const end = toLocalPoint(...line.coordinates[index]);
      const dx = end[0] - start[0];
      const dy = end[1] - start[1];
      const lengthSquared = dx * dx + dy * dy;
      if (lengthSquared === 0) continue;
      const t = clamp(-(start[0] * dx + start[1] * dy) / lengthSquared, 0, 1);
      const projection: [number, number] = [start[0] + t * dx, start[1] + t * dy];
      const projectionDistance = Math.hypot(projection[0], projection[1]);
      if (projectionDistance < nearestDistance) {
        const length = Math.sqrt(lengthSquared);
        nearestDistance = projectionDistance;
        nearestSegment = {
          projection,
          tangent: [dx / length, dy / length],
        };
      }
    }
  }

  if (!nearestSegment) {
    return includeFocusedPlaces(places
      .slice()
      .sort((left, right) =>
        (left.distance_km ?? Number.POSITIVE_INFINITY)
        - (right.distance_km ?? Number.POSITIVE_INFINITY))
      .slice(0, 2), places, focus);
  }

  const segment = nearestSegment;
  const ranked = places
    .filter(
      (place): place is MapPlacePin & { latitude: number; longitude: number } =>
        typeof place.latitude === "number" && typeof place.longitude === "number",
    )
    .map((place) => {
      const point = toLocalPoint(place.longitude, place.latitude);
      const along = (point[0] - segment.projection[0]) * segment.tangent[0]
        + (point[1] - segment.projection[1]) * segment.tangent[1];
      return { place, along };
    });
  const before = ranked
    .filter((item) => item.along < 0)
    .sort((left, right) => Math.abs(left.along) - Math.abs(right.along))[0];
  const after = ranked
    .filter((item) => item.along >= 0)
    .sort((left, right) => Math.abs(left.along) - Math.abs(right.along))[0];

  if (before && after) return includeFocusedPlaces([before.place, after.place], places, focus);
  return includeFocusedPlaces(ranked
    .sort((left, right) => Math.abs(left.along) - Math.abs(right.along))
    .slice(0, 2)
    .map((item) => item.place), places, focus);
}

function includeFocusedPlaces(
  selected: MapPlacePin[],
  allPlaces: MapPlacePin[],
  focus?: ProofFocus | null,
): MapPlacePin[] {
  if (!focus) return selected;
  const focused = allPlaces.filter((place) => placeMatchesProofFocus(place, focus));
  const merged = [...selected];
  for (const place of focused) {
    if (!merged.some((existing) => placeId(existing) === placeId(place))) {
      merged.push(place);
    }
  }
  return merged;
}

export function chooseRadiusKm(
  places: MapPlacePin[],
  scale: PlateScaleMode,
  home?: { latitude: number; longitude: number },
  overlayCoordinates: [number, number][] = [],
  focus?: ProofFocus | null,
): number {
  const factDistances = places
    .map((place) => place.distance_km)
    .filter((value): value is number => typeof value === "number" && Number.isFinite(value));
  const factFar = factDistances.length > 0
    ? Math.max(...factDistances)
    : (scale === "nearby" ? 0.5 : 5);

  // When home is estimated, fact distances can disagree with map geometry.
  // Always size the frame from true map distance so dots stay on-screen.
  let mapFar = 0;
  if (home) {
    for (const place of places) {
      if (typeof place.latitude !== "number" || typeof place.longitude !== "number") continue;
      mapFar = Math.max(
        mapFar,
        distanceKm(home.latitude, home.longitude, place.latitude, place.longitude),
      );
    }
    for (const [longitude, latitude] of overlayCoordinates) {
      mapFar = Math.max(
        mapFar,
        distanceKm(home.latitude, home.longitude, latitude, longitude),
      );
    }
  }

  const focusFar = typeof focus?.distanceM === "number" ? focus.distanceM / 1000 : 0;
  const needed = Math.max(factFar, mapFar, focusFar) * VIEWPORT_PADDING;
  const floor = scale === "nearby" ? 0.35 : 2;
  const cap = scale === "nearby" ? 2.5 : 15;
  const target = focusFar > cap ? Math.max(floor, needed) : clamp(needed, floor, cap);
  const steps = scale === "nearby" ? NEARBY_RADIUS_STEPS_KM : AREA_RADIUS_STEPS_KM;
  return steps.find((step) => step >= target) ?? target;
}

export function zoomForRadiusKm(radiusKm: number): number {
  // Keep the frame tight around home, but always wide enough for plotted dots.
  if (radiusKm <= 0.4) return 15.6;
  if (radiusKm <= 0.6) return 15;
  if (radiusKm <= 0.9) return 14.5;
  if (radiusKm <= 1.3) return 14;
  if (radiusKm <= 1.8) return 13.5;
  if (radiusKm <= 2.5) return 13;
  if (radiusKm <= 4) return 12.4;
  if (radiusKm <= 6) return 11.8;
  if (radiusKm <= 8) return 11.3;
  return 10.9;
}

export function buildNumberedPlaces(
  places: MapPlacePin[],
  limit = places.length,
): NumberedPlace[] {
  const withCoords = places
    .filter(
      (place): place is MapPlacePin & { latitude: number; longitude: number } =>
        typeof place.latitude === "number" && typeof place.longitude === "number",
    )
    .slice()
    .slice(0, limit);

  return withCoords.map((place, index) => ({
    ...place,
    id: placeId(place, index),
    number: index + 1,
  }));
}

export function clusterClosePlaces(
  places: NumberedPlace[],
  scale: PlateScaleMode,
): { singles: NumberedPlace[]; clusters: PlaceCluster[] } {
  const gapKm = scale === "nearby" ? CLUSTER_GAP_KM_NEARBY : CLUSTER_GAP_KM_AREA;
  const used = new Set<string>();
  const singles: NumberedPlace[] = [];
  const clusters: PlaceCluster[] = [];

  for (const place of places) {
    if (used.has(place.id)) continue;
    const group = places.filter((candidate) => {
      if (used.has(candidate.id)) return false;
      return distanceKm(
        place.latitude,
        place.longitude,
        candidate.latitude,
        candidate.longitude,
      ) <= gapKm;
    });

    if (group.length <= 1) {
      used.add(place.id);
      singles.push(place);
      continue;
    }

    for (const member of group) used.add(member.id);
    const latitude = group.reduce((sum, item) => sum + item.latitude, 0) / group.length;
    const longitude = group.reduce((sum, item) => sum + item.longitude, 0) / group.length;
    clusters.push({
      id: `cluster-${group.map((item) => item.id).join("|")}`,
      latitude,
      longitude,
      count: group.length,
      placeIds: group.map((item) => item.id),
      layer: group[0]?.layer ?? "schools",
    });
  }

  return { singles, clusters };
}

export function buildPlateViewport(
  home: { latitude: number; longitude: number },
  places: NumberedPlace[],
  scale: PlateScaleMode,
  metroLines: MapOverlayLine[] = [],
  extraOverlayLines: MapOverlayLine[] = [],
  focus?: ProofFocus | null,
): PlateViewport {
  const overlayCoordinates = [
    ...metroLines.flatMap((line) => line.coordinates),
    ...extraOverlayLines.flatMap((line) => line.coordinates),
  ];
  const framingCoordinates: [number, number][] = [
    [home.longitude, home.latitude],
    ...coordinatesForPlaces(places),
    ...overlayCoordinates,
  ];
  const center = boundsCenter(framingCoordinates);
  const radiusKm = chooseRadiusKm(places, scale, center, framingCoordinates, focus);
  return {
    center,
    radiusKm,
    zoom: zoomForRadiusKm(radiusKm),
    paddingFactor: clamp(0.18 + radiusKm * 0.02, 0.18, 0.28),
  };
}

export function availableLayers(context: PropertyMapContext): string[] {
  if (context.layers && context.layers.length > 0) {
    const present = new Set(context.places.map((place) => place.layer));
    for (const [layer, lines] of Object.entries(context.layer_lines ?? {})) {
      if (lines.length > 0) present.add(layer);
    }
    if ((context.red_flag_lines?.length ?? 0) > 0) {
      present.add("red_flags");
    }
    if ((context.access_lines?.length ?? 0) > 0) {
      present.add("metro");
    }
    return context.layers
      .filter((layer) => present.has(layer.id))
      .map((layer) => layer.id);
  }
  const layers: string[] = [];
  for (const place of context.places) {
    if (!layers.includes(place.layer)) layers.push(place.layer);
  }
  if ((context.red_flag_lines?.length ?? 0) > 0 && !layers.includes("red_flags")) {
    layers.push("red_flags");
  }
  if ((context.access_lines?.length ?? 0) > 0 && !layers.includes("metro")) {
    layers.unshift("metro");
  }
  return layers;
}

export function layerLabel(layer: string, context?: Pick<PropertyMapContext, "layers">): string {
  const configured = context?.layers?.find((candidate) => candidate.id === layer);
  if (configured?.label) return configured.label;
  switch (layer) {
    case "metro":
      return "Metro";
    case "schools":
      return "Schools";
    case "hospitals":
      return "Hospitals";
    case "tech":
      return "Tech parks";
    case "fitness":
      return "Fitness";
    case "parks":
      return "Parks";
    case "lakes":
      return "Lakes";
    case "breweries":
      return "Breweries";
    case "graveyards":
      return "Burial grounds";
    case "red_flags":
      return "Red flags";
    default:
      return layer
        .split(/[_-]+/)
        .filter(Boolean)
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(" ");
  }
}
