import type {
  MapOverlayLine,
  MapLayerMeta,
  MapPlacePin,
  PropertyMapContext,
  SceneFeature,
  SceneGeometry,
  SceneReceipt,
  SurfaceSceneResponse,
} from "./types.ts";
import { PUBLIC_BRAND_NAME } from "./brand.ts";

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
  const accessLines = scene.features
    .filter((feature) => feature.layerId === "metro")
    .map((feature) => mapLineFromFeature(feature, receiptsById))
    .filter((line): line is MapOverlayLine => Boolean(line));
  const layerLines = mapLinesByLayer(scene, receiptsById);

  const mergedAccessLines = mergeLines(accessLines, fallback?.access_lines ?? []);
  const mergedRedFlagLines = [
    ...redFlagLines,
    ...(fallback?.red_flag_lines ?? []).filter((line) =>
      !redFlagLines.some((candidate) => candidate.id === line.id)),
  ];
  layerLines.metro = mergedAccessLines;
  layerLines.red_flags = mergedRedFlagLines;
  const layers = mergedLayers(scene, fallback, mergedRedFlagLines);

  return {
    home: {
      entity_id: scene.anchor.entityId,
      name: scene.anchor.label,
      area: scene.anchor.area,
      latitude: anchorCoordinates?.latitude,
      longitude: anchorCoordinates?.longitude,
    },
    layers,
    places,
    proof_focus: scene.proofFocus,
    water: fallback?.water,
    metro_lines: fallback?.metro_lines,
    access_lines: mergedAccessLines,
    red_flag_lines: mergedRedFlagLines,
    layer_lines: layerLines,
    green_patches: fallback?.green_patches,
    lakes: fallback?.lakes,
  };
}

function mapLinesByLayer(
  scene: SurfaceSceneResponse,
  receiptsById: Map<string, SceneReceipt>,
): Record<string, MapOverlayLine[]> {
  const lines: Record<string, MapOverlayLine[]> = {};
  for (const feature of scene.features) {
    if (feature.geometry.type !== "LineString") continue;
    const line = mapLineFromFeature(feature, receiptsById);
    if (!line) continue;
    (lines[feature.layerId] ??= []).push(line);
  }
  return lines;
}

function mergeLines(primary: MapOverlayLine[], fallback: MapOverlayLine[]): MapOverlayLine[] {
  return [
    ...primary,
    ...fallback.filter((line) => !primary.some((candidate) => candidate.id === line.id)),
  ];
}

function mergedLayers(
  scene: SurfaceSceneResponse,
  fallback: PropertyMapContext | null | undefined,
  redFlagLines: MapOverlayLine[],
): MapLayerMeta[] {
  const byId = new Map<string, MapLayerMeta>();
  const addLayer = (layer: MapLayerMeta) => {
    if (!byId.has(layer.id)) byId.set(layer.id, layer);
  };

  for (const layer of scene.layers) {
    addLayer({
      id: layer.id,
      label: layer.label,
      renderKind: layer.renderKind,
      experience: layer.experience,
      rank: layer.rank,
      enabledByDefault: layer.enabledByDefault,
    });
  }
  for (const layer of fallback?.layers ?? []) {
    addLayer(layer);
  }
  if (redFlagLines.length > 0) {
    addLayer({
      id: "red_flags",
      label: "Red flags",
      rank: 9,
      enabledByDefault: true,
    });
  }

  return [...byId.values()].sort((left, right) =>
    (left.rank ?? Number.MAX_SAFE_INTEGER) - (right.rank ?? Number.MAX_SAFE_INTEGER)
    || left.label.localeCompare(right.label));
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
    source_type: receipt?.sourceType ?? PUBLIC_BRAND_NAME,
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
    label: feature.shortLabel,
    distance_km: typeof feature.metrics?.distanceM === "number"
      ? feature.metrics.distanceM / 1000
      : undefined,
    details: feature.details,
    kind: feature.kind,
    coordinates,
    source_type: receipt?.sourceType ?? PUBLIC_BRAND_NAME,
    source_url: receipt?.sourceUrl,
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
