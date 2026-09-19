import {
  selectPrimaryAtlasRoute,
  type AtlasRoadDirection,
} from "./atlas/journey.ts";
import { buildContextLines } from "./atlas/contextLines.ts";
import type { AtlasContextLineStyle } from "./atlas/contextLines.ts";
import type { MapOverlayLine } from "./types.ts";

/** Canonical transit labels are emitted as `{CSS color} Line` by the API. */
export function lineColorFromName(name: string, fallback: string): string {
  return /^([a-z]+) line$/i.exec(name.trim())?.[1].toLowerCase() ?? fallback;
}

/** An aerial inspection of mapped alignment, never a claimed driving route. */
export function arrivalAtlasRoute(
  lines: MapOverlayLine[],
  direction: AtlasRoadDirection = "as-mapped",
) {
  const valid = lines.filter(
    (line) =>
      line.coordinates.length > 1 &&
      line.coordinates.every(
        ([lng, lat]) =>
          Number.isFinite(lat) &&
          Number.isFinite(lng) &&
          Math.abs(lat) <= 90 &&
          Math.abs(lng) <= 180,
      ),
  );
  if (!valid.length) return null;
  const route = selectPrimaryAtlasRoute(
    valid.map((line) => ({
      type: "LineString" as const,
      coordinates: line.coordinates,
    })),
    { direction },
  );
  return route.lengthM > 0 ? route : null;
}

/** Keep API-scoped segments separate; do not silently radius-filter search proof. */
export function arrivalAtlasContextLines(
  lines: MapOverlayLine[],
  style: AtlasContextLineStyle | ((line: MapOverlayLine) => AtlasContextLineStyle),
) {
  const seen = new Set<string>();
  return lines.flatMap((line) => {
    if (seen.has(line.id) || line.coordinates.length < 2 || !line.coordinates.every(
      ([lng, lat]) => Number.isFinite(lat) && Number.isFinite(lng),
    )) return [];
    seen.add(line.id);
    const [lng, lat] = line.coordinates[0];
    return buildContextLines({
      origin: { lat, lng },
      maximumDistanceM: Number.POSITIVE_INFINITY,
      style: typeof style === 'function' ? style(line) : style,
      segments: [{
        id: line.id,
        path: line.coordinates.map(([pointLng, pointLat]) => ({ lat: pointLat, lng: pointLng })),
      }],
    });
  });
}
