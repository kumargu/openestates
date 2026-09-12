import type { AtlasCamera, AtlasPoint } from "./types.ts";

const METRES_PER_LATITUDE_DEGREE = 111_320;

export type AtlasScreenFrame = Readonly<{
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
}>;

export type AtlasFitPoint = Readonly<AtlasPoint & { heightM?: number }>;
export type AtlasScreenPoint = Readonly<{ x: number; y: number }>;

function longitudeScale(latitude: number): number {
  return METRES_PER_LATITUDE_DEGREE
    * Math.max(0.2, Math.cos(latitude * Math.PI / 180));
}

function normaliseHeading(heading: number): number {
  return ((heading % 360) + 360) % 360;
}

function localPoint(
  point: AtlasFitPoint,
  origin: AtlasPoint,
  heading: number,
  tilt: number,
): { rightM: number; verticalM: number } {
  const eastM = (point.lng - origin.lng) * longitudeScale(origin.lat);
  const northM = (point.lat - origin.lat) * METRES_PER_LATITUDE_DEGREE;
  const headingRadians = heading * Math.PI / 180;
  const tiltRadians = tilt * Math.PI / 180;
  const rightM = eastM * Math.cos(headingRadians) - northM * Math.sin(headingRadians);
  const forwardM = eastM * Math.sin(headingRadians) + northM * Math.cos(headingRadians);
  return {
    rightM,
    verticalM: -forwardM * Math.cos(tiltRadians)
      - (point.heightM ?? 0) * Math.sin(tiltRadians),
  };
}

function metresPerPixel(range: number, frame: AtlasScreenFrame, fovDegrees: number): number {
  return 2 * range * Math.tan(fovDegrees * Math.PI / 360) / frame.height;
}

export function bearingDegrees(from: AtlasPoint, to: AtlasPoint): number {
  const eastM = (to.lng - from.lng) * longitudeScale((from.lat + to.lat) / 2);
  const northM = (to.lat - from.lat) * METRES_PER_LATITUDE_DEGREE;
  if (Math.abs(eastM) < 1e-6 && Math.abs(northM) < 1e-6) return 0;
  return normaliseHeading(Math.atan2(eastM, northM) * 180 / Math.PI);
}

/** Fit ground geometry and elevated scene anchors into the clear screen rectangle. */
export function fitCameraToScreen(input: Readonly<{
  points: readonly AtlasFitPoint[];
  frame: AtlasScreenFrame;
  heading: number;
  tilt: number;
  fieldOfViewDegrees: number;
  minimumRangeM: number;
  opticalPaddingPx: number;
  altitudeM: number;
  /** Prefer this subject at the clear-frame centre, without cropping context. */
  focusPoint?: AtlasFitPoint;
}>): AtlasCamera {
  const points = input.points.filter((point) =>
    Number.isFinite(point.lat)
    && Number.isFinite(point.lng)
    && Number.isFinite(point.heightM ?? 0));
  if (points.length === 0) throw new Error("Camera fitting requires at least one finite point");
  if (!(input.frame.width > 0) || !(input.frame.height > 0)) {
    throw new Error("Camera fitting requires a positive viewport");
  }

  const origin = {
    lat: points.reduce((total, point) => total + point.lat, 0) / points.length,
    lng: points.reduce((total, point) => total + point.lng, 0) / points.length,
  };
  const heading = normaliseHeading(input.heading);
  const tilt = Math.max(0, Math.min(80, input.tilt));
  const projected = points.map((point) => localPoint(point, origin, heading, tilt));
  const minRight = Math.min(...projected.map((point) => point.rightM));
  const maxRight = Math.max(...projected.map((point) => point.rightM));
  const minVertical = Math.min(...projected.map((point) => point.verticalM));
  const maxVertical = Math.max(...projected.map((point) => point.verticalM));

  const padding = Math.max(0, input.opticalPaddingPx);
  const safeLeft = Math.min(input.frame.width, input.frame.left + padding);
  const safeRight = Math.max(0, input.frame.width - input.frame.right - padding);
  const safeTop = Math.min(input.frame.height, input.frame.top + padding);
  const safeBottom = Math.max(0, input.frame.height - input.frame.bottom - padding);
  const availableWidth = Math.max(1, safeRight - safeLeft);
  const availableHeight = Math.max(1, safeBottom - safeTop);
  const requiredMetresPerPixel = Math.max(
    (maxRight - minRight) / availableWidth,
    (maxVertical - minVertical) / availableHeight,
  );
  const fittedRange = requiredMetresPerPixel * input.frame.height
    / (2 * Math.tan(input.fieldOfViewDegrees * Math.PI / 360));
  const range = Math.max(input.minimumRangeM, fittedRange, 1);
  const scale = metresPerPixel(range, input.frame, input.fieldOfViewDegrees);

  const safeCenterX = (safeLeft + safeRight) / 2;
  const safeCenterY = (safeTop + safeBottom) / 2;
  const focus = input.focusPoint
    ? localPoint(input.focusPoint, origin, heading, tilt)
    : {rightM: (minRight + maxRight) / 2, verticalM: (minVertical + maxVertical) / 2};
  // Only spend unused framing space on the selected subject. Home, sourced
  // extents and relationship arcs remain hard containment constraints.
  const constrainedTarget = (preferred: number, min: number, max: number,
    screenStart: number, screenEnd: number, screenSize: number, screenCenter: number) => {
    const lower = max - (screenEnd - screenSize / 2) * scale;
    const upper = min - (screenStart - screenSize / 2) * scale;
    return Math.max(lower, Math.min(upper, preferred - (screenCenter - screenSize / 2) * scale));
  };
  const targetRightM = constrainedTarget(focus.rightM, minRight, maxRight,
    safeLeft, safeRight, input.frame.width, safeCenterX);
  const targetVerticalM = constrainedTarget(focus.verticalM, minVertical, maxVertical,
    safeTop, safeBottom, input.frame.height, safeCenterY);
  const headingRadians = heading * Math.PI / 180;
  const tiltRadians = tilt * Math.PI / 180;
  const targetForwardM = -targetVerticalM / Math.max(0.17, Math.cos(tiltRadians));
  const targetEastM = targetRightM * Math.cos(headingRadians)
    + targetForwardM * Math.sin(headingRadians);
  const targetNorthM = -targetRightM * Math.sin(headingRadians)
    + targetForwardM * Math.cos(headingRadians);

  return {
    center: {
      lat: origin.lat + targetNorthM / METRES_PER_LATITUDE_DEGREE,
      lng: origin.lng + targetEastM / longitudeScale(origin.lat),
      altitude: input.altitudeM,
    },
    heading,
    tilt,
    range,
  };
}

export function projectCameraPointToScreen(
  camera: AtlasCamera,
  point: AtlasFitPoint,
  frame: AtlasScreenFrame,
  fieldOfViewDegrees: number,
): AtlasScreenPoint {
  const projected = localPoint(point, camera.center, camera.heading, camera.tilt);
  const scale = metresPerPixel(camera.range, frame, fieldOfViewDegrees);
  return {
    x: frame.width / 2 + projected.rightM / scale,
    y: frame.height / 2 + projected.verticalM / scale,
  };
}
