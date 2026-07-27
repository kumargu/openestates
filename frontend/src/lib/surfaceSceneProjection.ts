import type {
  MapOverlayLine,
  MapPlacePin,
  PropertyMapContext,
  SceneFeature,
  SceneGeometry,
  SceneReceipt,
  SurfaceSceneResponse,
} from "./types.ts";

export function propertyMapContextFromSurfaceScene(
  scene: SurfaceSceneResponse | null | undefined,
  fallback?: PropertyMapContext | null,
): PropertyMapContext | null {
  if (!scene) return fallback ?? null;

  const anchorCoordinates = pointCoordinates(scene.anchor.geometry);
  const receiptsById = new Map(scene.receipts.map((receipt) => [receipt.id, receipt]));
  const scenePlaces = scene.features
    .map((feature) => mapPlacePinFromFeature(feature, receiptsById))
    .filter((place): place is MapPlacePin => Boolean(place));
  const places = [
    ...scenePlaces,
    ...(fallback?.places ?? []).filter((place) =>
      !scenePlaces.some((candidate) => samePlacePin(candidate, place))),
  ];
  const redFlagLines = scene.features
    .filter((feature) => feature.layerId === "red_flags")
    .map((feature) => mapLineFromFeature(feature, receiptsById))
    .filter((line): line is MapOverlayLine => Boolean(line));

  const mergedRedFlagLines = [
    ...redFlagLines,
    ...(fallback?.red_flag_lines ?? []).filter((line) =>
      !redFlagLines.some((candidate) => candidate.id === line.id)),
  ];

  return {
    home: {
      entity_id: scene.anchor.entityId,
      name: scene.anchor.label,
      area: scene.anchor.area,
      latitude: anchorCoordinates?.latitude,
      longitude: anchorCoordinates?.longitude,
    },
    places,
    proof_focus: scene.proofFocus,
    water: fallback?.water,
    metro_lines: fallback?.metro_lines,
    red_flag_lines: mergedRedFlagLines,
    green_patches: fallback?.green_patches,
    lakes: fallback?.lakes,
  };
}

function samePlacePin(left: MapPlacePin, right: MapPlacePin): boolean {
  if (left.feature_id && right.feature_id) return left.feature_id === right.feature_id;
  if (left.place_entity_id && right.place_entity_id) {
    return left.place_entity_id === right.place_entity_id && left.layer === right.layer;
  }
  return left.layer === right.layer && left.name === right.name;
}

function mapPlacePinFromFeature(
  feature: SceneFeature,
  receiptsById: Map<string, SceneReceipt>,
): MapPlacePin | null {
  const coordinates = pointCoordinates(feature.geometry);
  if (!coordinates) return null;
  const receipt = feature.receiptIds
    .map((id) => receiptsById.get(id))
    .find((candidate): candidate is SceneReceipt => Boolean(candidate));

  return {
    feature_id: feature.id,
    place_entity_id: feature.entityId,
    layer: feature.layerId,
    name: feature.label,
    latitude: coordinates.latitude,
    longitude: coordinates.longitude,
    distance_km: typeof feature.metrics?.distanceM === "number"
      ? feature.metrics.distanceM / 1000
      : undefined,
    rating: feature.metrics?.rating,
    review_count: feature.metrics?.reviewCount,
    source_url: receipt?.sourceUrl,
    source_type: receipt?.sourceType ?? "OpenEstates",
  };
}

function mapLineFromFeature(
  feature: SceneFeature,
  receiptsById: Map<string, SceneReceipt>,
): MapOverlayLine | null {
  const coordinates = lineCoordinates(feature.geometry);
  if (!coordinates) return null;
  const receipt = feature.receiptIds
    .map((id) => receiptsById.get(id))
    .find((candidate): candidate is SceneReceipt => Boolean(candidate));

  return {
    id: feature.id,
    name: feature.label,
    kind: feature.kind,
    coordinates,
    source_type: receipt?.sourceType ?? "OpenEstates",
  };
}

function pointCoordinates(geometry?: SceneGeometry): {
  latitude: number;
  longitude: number;
} | null {
  if (!geometry || geometry.type !== "Point") return null;
  const [longitude, latitude] = geometry.coordinates;
  if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) return null;
  return { latitude, longitude };
}

function lineCoordinates(geometry?: SceneGeometry): [number, number][] | null {
  if (!geometry || geometry.type !== "LineString") return null;
  if (geometry.coordinates.length < 2) return null;
  return geometry.coordinates.every(([longitude, latitude]) =>
    Number.isFinite(latitude) && Number.isFinite(longitude))
    ? geometry.coordinates
    : null;
}
