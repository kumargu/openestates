import type {
  SceneFeature,
  SceneGeometry,
  SurfaceSceneResponse,
} from "../../../frontend/src/lib/types.ts";

export type AtlasScale = "society" | "estate" | "township";
export type AtlasBackdropLayout = "single" | "split-context";

export type AtlasSceneInput = Pick<
  SurfaceSceneResponse,
  "anchor" | "viewport" | "layers" | "features"
>;

export type AtlasPresentationThresholds = {
  estateAreaAcres: number;
  estateLongestSpanM: number;
  estatePrecinctCount: number;
  townshipAreaAcres: number;
  townshipLongestSpanM: number;
  townshipPrecinctCount: number;
  minimumRouteLengthM: number;
};

export type AtlasPresentationConfig = {
  profile?: AtlasScale | "auto";
  routeLayerIds?: string[];
  precinctLayerIds?: string[];
  buildingLayerIds?: string[];
  thresholds?: Partial<AtlasPresentationThresholds>;
};

export type AtlasSceneMetrics = {
  areaAcres: number;
  widthM: number;
  heightM: number;
  longestSpanM: number;
  precinctCount: number;
  buildingCount: number;
  routeFeatureId?: string;
  routeLengthM: number;
};

export type AtlasPresentation = {
  profile: AtlasScale;
  metrics: AtlasSceneMetrics;
  backdrop: {
    layout: AtlasBackdropLayout;
    context: "inline" | "locator" | "locator-route";
  };
  mainViews: string[];
  camera: {
    overviewRangeM: number;
    focusRangeM: number;
    roadRangeM: number;
    mobileScale: number;
  };
  labels: {
    maximumVisible: number;
    building: "none" | "cluster" | "texture";
  };
  route: {
    enabled: boolean;
    featureId?: string;
  };
};

export const DEFAULT_ATLAS_THRESHOLDS: AtlasPresentationThresholds = Object.freeze({
  estateAreaAcres: 20,
  estateLongestSpanM: 700,
  estatePrecinctCount: 2,
  townshipAreaAcres: 80,
  townshipLongestSpanM: 1_200,
  townshipPrecinctCount: 4,
  minimumRouteLengthM: 250,
});

type ProfilePresentation = Omit<AtlasPresentation, "metrics" | "profile" | "route">;

const PROFILE_PRESENTATION: Record<AtlasScale, ProfilePresentation> = {
  society: {
    backdrop: { layout: "single", context: "inline" },
    mainViews: ["arrival", "society", "neighborhood"],
    camera: { overviewRangeM: 1_050, focusRangeM: 550, roadRangeM: 430, mobileScale: 1.25 },
    labels: { maximumVisible: 8, building: "none" },
  },
  estate: {
    backdrop: { layout: "split-context", context: "locator" },
    mainViews: ["overview", "corridor", "place", "context"],
    camera: { overviewRangeM: 1_750, focusRangeM: 700, roadRangeM: 500, mobileScale: 1.3 },
    labels: { maximumVisible: 12, building: "cluster" },
  },
  township: {
    backdrop: { layout: "split-context", context: "locator-route" },
    mainViews: ["overview", "corridor", "precinct", "context"],
    camera: { overviewRangeM: 2_900, focusRangeM: 850, roadRangeM: 285, mobileScale: 1.4 },
    labels: { maximumVisible: 14, building: "texture" },
  },
};

export function resolveAtlasPresentation(
  scene: AtlasSceneInput,
  config: AtlasPresentationConfig = {},
): AtlasPresentation {
  const thresholds = { ...DEFAULT_ATLAS_THRESHOLDS, ...config.thresholds };
  const metrics = atlasSceneMetrics(scene, config);
  const profile = config.profile && config.profile !== "auto"
    ? config.profile
    : classifyAtlasScale(metrics, thresholds);
  const presentation = PROFILE_PRESENTATION[profile];
  const routeEnabled = metrics.routeLengthM >= thresholds.minimumRouteLengthM;

  return {
    profile,
    metrics,
    backdrop: { ...presentation.backdrop },
    mainViews: [...presentation.mainViews],
    camera: { ...presentation.camera },
    labels: { ...presentation.labels },
    route: {
      enabled: routeEnabled,
      featureId: routeEnabled ? metrics.routeFeatureId : undefined,
    },
  };
}

export function atlasSceneMetrics(
  scene: AtlasSceneInput,
  config: AtlasPresentationConfig = {},
): AtlasSceneMetrics {
  const routeLayers = configuredLayerIds(
    scene,
    config.routeLayerIds,
    ["terrain_corridor", "corridor", "internal_route"],
  );
  const precinctLayers = configuredLayerIds(scene, config.precinctLayerIds, ["precinct"]);
  const buildingLayers = configuredLayerIds(scene, config.buildingLayerIds, ["building"]);
  const route = longestLineFeature(scene.features.filter((feature) => routeLayers.has(feature.layerId)));
  const bounds = sceneBounds(scene);
  const widthM = bounds ? distanceM([bounds.west, bounds.south], [bounds.east, bounds.south]) : 0;
  const heightM = bounds ? distanceM([bounds.west, bounds.south], [bounds.west, bounds.north]) : 0;

  return {
    areaAcres: polygonAreaSqM(scene.anchor.boundary?.geometry) / 4_046.8564224,
    widthM,
    heightM,
    longestSpanM: Math.max(widthM, heightM),
    precinctCount: scene.features.filter((feature) => precinctLayers.has(feature.layerId)).length,
    buildingCount: scene.features.filter((feature) => buildingLayers.has(feature.layerId)).length,
    routeFeatureId: route?.feature.id,
    routeLengthM: route?.lengthM ?? 0,
  };
}

export function classifyAtlasScale(
  metrics: AtlasSceneMetrics,
  thresholds: AtlasPresentationThresholds = DEFAULT_ATLAS_THRESHOLDS,
): AtlasScale {
  if (
    metrics.areaAcres >= thresholds.townshipAreaAcres
    || metrics.longestSpanM >= thresholds.townshipLongestSpanM
    || metrics.precinctCount >= thresholds.townshipPrecinctCount
  ) return "township";
  if (
    metrics.areaAcres >= thresholds.estateAreaAcres
    || metrics.longestSpanM >= thresholds.estateLongestSpanM
    || metrics.precinctCount >= thresholds.estatePrecinctCount
  ) return "estate";
  return "society";
}

function configuredLayerIds(
  scene: AtlasSceneInput,
  explicitIds: string[] | undefined,
  renderKinds: string[],
): Set<string> {
  if (explicitIds) return new Set(explicitIds);
  return new Set(scene.layers
    .filter((layer) => renderKinds.includes(layer.renderKind))
    .map((layer) => layer.id));
}

function longestLineFeature(
  features: SceneFeature[],
): { feature: SceneFeature; lengthM: number } | null {
  return features.reduce<{ feature: SceneFeature; lengthM: number } | null>((longest, feature) => {
    if (feature.geometry.type !== "LineString") return longest;
    const lengthM = lineLengthM(feature.geometry.coordinates);
    return !longest || lengthM > longest.lengthM ? { feature, lengthM } : longest;
  }, null);
}

function sceneBounds(scene: AtlasSceneInput) {
  if (scene.viewport.bounds) return scene.viewport.bounds;
  const coordinates = [
    ...geometryCoordinates(scene.anchor.geometry),
    ...geometryCoordinates(scene.anchor.boundary?.geometry),
    ...scene.features.flatMap((feature) => geometryCoordinates(feature.geometry)),
  ];
  if (coordinates.length === 0) return null;
  const longitudes = coordinates.map(([longitude]) => longitude);
  const latitudes = coordinates.map(([, latitude]) => latitude);
  return {
    west: Math.min(...longitudes),
    south: Math.min(...latitudes),
    east: Math.max(...longitudes),
    north: Math.max(...latitudes),
  };
}

function geometryCoordinates(geometry?: SceneGeometry): [number, number][] {
  if (!geometry) return [];
  if (geometry.type === "Point") return [geometry.coordinates];
  if (geometry.type === "LineString") return geometry.coordinates;
  if (geometry.type === "Polygon") return geometry.coordinates.flat();
  return geometry.coordinates.flat(2);
}

function lineLengthM(coordinates: [number, number][]): number {
  return coordinates.slice(1).reduce(
    (total, point, index) => total + distanceM(coordinates[index], point),
    0,
  );
}

function polygonAreaSqM(geometry?: SceneGeometry): number {
  if (!geometry) return 0;
  if (geometry.type === "Polygon") return ringAreaSqM(geometry.coordinates[0] ?? []);
  if (geometry.type === "MultiPolygon") {
    return geometry.coordinates.reduce((total, polygon) => total + ringAreaSqM(polygon[0] ?? []), 0);
  }
  return 0;
}

function ringAreaSqM(ring: [number, number][]): number {
  if (ring.length < 3) return 0;
  const latitude = ring.reduce((total, [, value]) => total + value, 0) / ring.length;
  const longitudeScale = 111_320 * Math.cos(latitude * Math.PI / 180);
  const latitudeScale = 110_570;
  const twiceArea = ring.reduce((total, point, index) => {
    const next = ring[(index + 1) % ring.length];
    return total
      + point[0] * longitudeScale * next[1] * latitudeScale
      - next[0] * longitudeScale * point[1] * latitudeScale;
  }, 0);
  return Math.abs(twiceArea) / 2;
}

function distanceM(left: [number, number], right: [number, number]): number {
  const meanLatitude = (left[1] + right[1]) * Math.PI / 360;
  const longitudeM = (right[0] - left[0]) * 111_320 * Math.cos(meanLatitude);
  const latitudeM = (right[1] - left[1]) * 110_570;
  return Math.hypot(longitudeM, latitudeM);
}
