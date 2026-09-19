import type { AtlasFeature, AtlasPoint } from "./types.ts";

const METRES_PER_LATITUDE_DEGREE = 111_320;
const COMPLETE_CIRCLE_RADIANS = Math.PI * 2;

export function distanceMetres(a: AtlasPoint, b: AtlasPoint): number {
  const latitudeMetres = (b.lat - a.lat) * METRES_PER_LATITUDE_DEGREE;
  const longitudeScale = METRES_PER_LATITUDE_DEGREE * Math.cos(((a.lat + b.lat) * Math.PI) / 360);
  const longitudeMetres = (b.lng - a.lng) * longitudeScale;
  return Math.hypot(latitudeMetres, longitudeMetres);
}

export function featurePoints(feature: AtlasFeature): readonly AtlasPoint[] {
  return feature.boundary ?? feature.segments?.flatMap((segment) => segment.path) ?? [feature.position];
}

export function footprintCircle(feature: AtlasFeature, radiusM: number, pointCount = 48): readonly AtlasPoint[] {
  if (pointCount < 3) throw new Error("A footprint needs at least three points");
  const points = Array.from({ length: pointCount + 1 }, (_, index) => {
    const angle = (index / pointCount) * COMPLETE_CIRCLE_RADIANS;
    return {
      lat: feature.position.lat + (Math.sin(angle) * radiusM) / METRES_PER_LATITUDE_DEGREE,
      lng: feature.position.lng +
        (Math.cos(angle) * radiusM) /
          (METRES_PER_LATITUDE_DEGREE * Math.cos((feature.position.lat * Math.PI) / 180)),
    };
  });
  return points;
}

export type SegmentAnchor = Readonly<AtlasPoint & {
  heading: number;
  distanceM: number;
  segmentId: string;
}>;

export function nearestSegmentAnchor(origin: AtlasPoint, feature: AtlasFeature): SegmentAnchor | undefined {
  const longitudeScale = METRES_PER_LATITUDE_DEGREE * Math.cos((origin.lat * Math.PI) / 180);
  let nearest: SegmentAnchor | undefined;

  for (const segment of feature.segments ?? []) {
    for (let index = 1; index < segment.path.length; index += 1) {
      const start = segment.path[index - 1];
      const end = segment.path[index];
      const startX = (start.lng - origin.lng) * longitudeScale;
      const startY = (start.lat - origin.lat) * METRES_PER_LATITUDE_DEGREE;
      const deltaX = (end.lng - start.lng) * longitudeScale;
      const deltaY = (end.lat - start.lat) * METRES_PER_LATITUDE_DEGREE;
      const lengthSquared = deltaX ** 2 + deltaY ** 2;
      if (lengthSquared === 0) continue;

      const fraction = Math.max(0, Math.min(1, -(startX * deltaX + startY * deltaY) / lengthSquared));
      const distanceM = Math.hypot(startX + fraction * deltaX, startY + fraction * deltaY);
      if (nearest && nearest.distanceM <= distanceM) continue;

      nearest = {
        lat: start.lat + (end.lat - start.lat) * fraction,
        lng: start.lng + (end.lng - start.lng) * fraction,
        heading: (Math.atan2(deltaX, deltaY) * 180) / Math.PI,
        distanceM,
        segmentId: segment.id,
      };
    }
  }

  return nearest;
}

