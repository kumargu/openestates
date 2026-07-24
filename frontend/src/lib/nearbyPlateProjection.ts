import type {
  MapNearbyLayer,
  MapOverlayLine,
  MapPlacePin,
  PropertyMapContext,
} from "./types.ts";

export type PlateScaleMode = "nearby" | "area";
export type PlateStory =
  | { kind: "essentials" }
  | { kind: "layer"; layer: MapNearbyLayer };

export const PLATE_MAX_MAP_LABEL_LENGTH = 22;
export const PLATE_LIST_LIMIT = 5;
export const ESSENTIAL_LAYERS: MapNearbyLayer[] = ["metro", "schools", "hospitals"];
export const NEARBY_LAYERS: MapNearbyLayer[] = [
  "metro",
  "schools",
  "hospitals",
  "tech",
];

/** Muted OSM basemap — no API key. */
export const NEARBY_MAP_STYLE = "https://tiles.openfreemap.org/styles/positron";

const NEARBY_RADIUS_STEPS_KM = [0.35, 0.5, 0.8, 1.2, 1.8, 2.5] as const;
const AREA_RADIUS_STEPS_KM = [3, 5, 8, 10] as const;
const CLUSTER_GAP_KM_NEARBY = 0.08;
const CLUSTER_GAP_KM_AREA = 0.35;
/** Keep markers inside the canvas, not glued to the ring edge. */
const VIEWPORT_PADDING = 1.45;

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
  return place.place_entity_id ?? `${place.layer}-${place.name}-${index}`;
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

export function placesForStory(
  context: PropertyMapContext,
  story: PlateStory,
): MapPlacePin[] {
  if (story.kind === "layer") {
    return context.places.filter((place) => place.layer === story.layer);
  }

  const picked: MapPlacePin[] = [];
  for (const layer of ESSENTIAL_LAYERS) {
    const nearest = context.places
      .filter((place) => place.layer === layer)
      .slice()
      .sort((left, right) =>
        (left.distance_km ?? Number.POSITIVE_INFINITY)
        - (right.distance_km ?? Number.POSITIVE_INFINITY))[0];
    if (nearest) picked.push(nearest);
  }
  return picked;
}

export function filterPlacesByScale(
  places: MapPlacePin[],
  scale: PlateScaleMode,
): MapPlacePin[] {
  // Nearby stays tight like a Strava activity frame — not city-wide.
  const maxKm = scale === "nearby" ? 1.5 : 10;
  return places.filter((place) => (place.distance_km ?? 0) <= maxKm
    || typeof place.distance_km !== "number");
}

export function chooseRadiusKm(
  places: MapPlacePin[],
  scale: PlateScaleMode,
  home?: { latitude: number; longitude: number },
  overlayCoordinates: [number, number][] = [],
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

  const needed = Math.max(factFar, mapFar) * VIEWPORT_PADDING;
  const floor = scale === "nearby" ? 0.35 : 2;
  const cap = scale === "nearby" ? 2.5 : 10;
  const target = clamp(needed, floor, cap);
  const steps = scale === "nearby" ? NEARBY_RADIUS_STEPS_KM : AREA_RADIUS_STEPS_KM;
  return steps.find((step) => step >= target) ?? steps[steps.length - 1];
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
  limit = PLATE_LIST_LIMIT,
): NumberedPlace[] {
  const withCoords = places
    .filter(
      (place): place is MapPlacePin & { latitude: number; longitude: number } =>
        typeof place.latitude === "number" && typeof place.longitude === "number",
    )
    .slice()
    .sort((left, right) =>
      (left.distance_km ?? Number.POSITIVE_INFINITY)
      - (right.distance_km ?? Number.POSITIVE_INFINITY))
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
): PlateViewport {
  const overlayCoordinates = metroLines.flatMap((line) => line.coordinates);
  const radiusKm = chooseRadiusKm(places, scale, home, overlayCoordinates);
  return {
    center: home,
    radiusKm,
    zoom: zoomForRadiusKm(radiusKm),
    paddingFactor: clamp(0.18 + radiusKm * 0.02, 0.18, 0.28),
  };
}

export function availableLayers(context: PropertyMapContext): MapNearbyLayer[] {
  const present = new Set(context.places.map((place) => place.layer));
  return NEARBY_LAYERS.filter((layer) => present.has(layer));
}

export function layerLabel(layer: string): string {
  switch (layer) {
    case "metro":
      return "Metro";
    case "schools":
      return "Schools";
    case "hospitals":
      return "Hospitals";
    case "tech":
      return "Tech parks";
    default:
      return layer;
  }
}
