import { distanceMetres, featurePoints, nearestSegmentAnchor } from "./geometry.ts";
import type { AtlasCamera, AtlasCameraContext, AtlasCameraTuning, AtlasFeature } from "./types.ts";

export const DEFAULT_CAMERA_TUNING: AtlasCameraTuning = Object.freeze({
  mobileBreakpointPx: 700,
  mobilePlaceScale: 1.35,
  mobileGroupScale: 1.3,
  maxContextDistanceM: 4_200,
  minimumGroupRangeM: 1_000,
  groupRangeMultiplier: 3.8,
  minimumPairRangeM: 950,
  pairRangeMultiplier: 2.15,
  cameraAltitudeOffsetM: 20,
});

function tuning(context: AtlasCameraContext): AtlasCameraTuning {
  return { ...DEFAULT_CAMERA_TUNING, ...context.tuning };
}

function placeScale(context: AtlasCameraContext): number {
  const values = tuning(context);
  return context.viewportWidthPx < values.mobileBreakpointPx ? values.mobilePlaceScale : 1;
}

function elevation(feature: AtlasFeature, context: AtlasCameraContext): number {
  return (feature.elevationM ?? context.defaultElevationM) + tuning(context).cameraAltitudeOffsetM;
}

export function placeCamera(
  context: AtlasCameraContext,
  feature: AtlasFeature,
  options: Readonly<{ heading?: number; range?: number; tilt?: number }> = {},
): AtlasCamera {
  return {
    center: { ...feature.position, altitude: elevation(feature, context) },
    heading: options.heading ?? 225,
    range: (options.range ?? 550) * placeScale(context),
    tilt: options.tilt ?? 60,
  };
}

export function pairCamera(context: AtlasCameraContext, feature: AtlasFeature): AtlasCamera {
  const values = tuning(context);
  return {
    center: {
      lat: (context.home.position.lat + feature.position.lat) / 2,
      lng: (context.home.position.lng + feature.position.lng) / 2,
      altitude: (elevation(context.home, context) + elevation(feature, context)) / 2,
    },
    heading: 0,
    tilt: 26,
    range: Math.max(values.minimumPairRangeM, feature.evidence.distance.metres * values.pairRangeMultiplier) * placeScale(context),
  };
}

export function groupCamera(context: AtlasCameraContext, features: readonly AtlasFeature[]): AtlasCamera {
  const values = tuning(context);
  const localPoints = [context.home.position, ...features.flatMap(featurePoints)].filter(
    (point) => distanceMetres(context.home.position, point) < values.maxContextDistanceM,
  );
  const points = localPoints.length > 0 ? localPoints : [context.home.position];
  const latitudes = points.map((point) => point.lat);
  const longitudes = points.map((point) => point.lng);
  const lat = (Math.min(...latitudes) + Math.max(...latitudes)) / 2;
  const lng = (Math.min(...longitudes) + Math.max(...longitudes)) / 2;
  const radiusM = Math.max(0, ...points.map((point) => distanceMetres({ lat, lng }, point)));
  const averageElevationM = features.length > 0
    ? features.reduce((total, feature) => total + (feature.elevationM ?? context.defaultElevationM), 0) / features.length
    : context.home.elevationM ?? context.defaultElevationM;
  const scale = context.viewportWidthPx < values.mobileBreakpointPx ? values.mobileGroupScale : 1;
  return {
    center: { lat, lng, altitude: averageElevationM + values.cameraAltitudeOffsetM },
    heading: 0,
    tilt: 25,
    range: Math.max(values.minimumGroupRangeM, radiusM * values.groupRangeMultiplier) * scale,
  };
}

export function featureCamera(context: AtlasCameraContext, feature: AtlasFeature): AtlasCamera {
  if (feature.boundary) return { ...groupCamera(context, [feature]), heading: 210, tilt: 30 };
  if (feature.segments) return { ...groupCamera(context, [feature]), heading: 25, tilt: 50 };
  return placeCamera(context, feature);
}

export function segmentCamera(context: AtlasCameraContext, feature: AtlasFeature): AtlasCamera {
  const anchor = nearestSegmentAnchor(context.home.position, feature);
  if (!anchor) return featureCamera(context, feature);
  return placeCamera(
    context,
    { ...feature, position: { lat: anchor.lat, lng: anchor.lng } },
    { heading: anchor.heading, range: 430, tilt: 58 },
  );
}

