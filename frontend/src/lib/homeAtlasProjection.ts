import { selectPrimaryAtlasRoute } from "../../../experiments/home-atlas/src/journey.ts";
import { buildContextLines } from "../../../experiments/home-atlas/src/contextLines.ts";
import type { AtlasContextLineStyle } from "../../../experiments/home-atlas/src/contextLines.ts";
import type { MapOverlayLine } from "./types.ts";

/** An aerial inspection of mapped alignment, never a claimed driving route. */
export function arrivalAtlasRoute(lines: MapOverlayLine[]) {
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
  );
  return route.lengthM > 0 ? route : null;
}

/** Keep API-scoped segments separate; do not silently radius-filter search proof. */
export function arrivalAtlasContextLines(
  lines: MapOverlayLine[],
  style: AtlasContextLineStyle,
) {
  const origin = lines[0]?.coordinates[0];
  if (!origin) return [];
  return buildContextLines({
    origin: { lat: origin[1], lng: origin[0] },
    maximumDistanceM: Number.POSITIVE_INFINITY,
    style,
    segments: lines
      .filter((line) =>
        line.coordinates.every(
          ([lng, lat]) => Number.isFinite(lat) && Number.isFinite(lng),
        ),
      )
      .map((line) => ({
        id: line.id,
        path: line.coordinates.map(([lng, lat]) => ({ lat, lng })),
      })),
  });
}
