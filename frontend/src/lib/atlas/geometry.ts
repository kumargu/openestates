import type { AtlasPoint } from "./types.ts";

const METRES_PER_LATITUDE_DEGREE = 111_320;

export function distanceMetres(a: AtlasPoint, b: AtlasPoint): number {
  const latitudeMetres = (b.lat - a.lat) * METRES_PER_LATITUDE_DEGREE;
  const longitudeScale = METRES_PER_LATITUDE_DEGREE * Math.cos(((a.lat + b.lat) * Math.PI) / 360);
  const longitudeMetres = (b.lng - a.lng) * longitudeScale;
  return Math.hypot(latitudeMetres, longitudeMetres);
}
