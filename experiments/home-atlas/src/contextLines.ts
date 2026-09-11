import { distanceMetres } from "./geometry.ts";
import type { AtlasPoint, AtlasSegment } from "./types.ts";

export type AtlasContextLineStyle = Readonly<{
  strokeColor: string;
  strokeWidth: number;
  altitudeMode: "clamp_to_ground";
  drawsOccludedSegments: boolean;
}>;

export type AtlasContextLine = Readonly<{
  id: string;
  path: readonly AtlasPoint[];
  style: AtlasContextLineStyle;
}>;

/**
 * Produces renderer-neutral line parts without joining separate OSM ways.
 * A Google 3D adapter can map each result to one Polyline3DElement.
 */
export function buildContextLines(input: Readonly<{
  segments: readonly AtlasSegment[];
  origin: AtlasPoint;
  maximumDistanceM: number;
  style: AtlasContextLineStyle;
}>): readonly AtlasContextLine[] {
  if (input.maximumDistanceM <= 0) throw new Error("Context distance must be positive");

  const lines: AtlasContextLine[] = [];
  const seen = new Set<string>();

  for (const segment of input.segments) {
    if (seen.has(segment.id)) continue;
    seen.add(segment.id);

    let part: AtlasPoint[] = [];
    let partIndex = 0;
    const flush = () => {
      if (part.length > 1) {
        lines.push({
          id: `${segment.id}:${partIndex}`,
          path: part,
          style: input.style,
        });
        partIndex += 1;
      }
      part = [];
    };

    for (const point of segment.path) {
      if (distanceMetres(input.origin, point) <= input.maximumDistanceM) part.push(point);
      else flush();
    }
    flush();
  }

  return lines;
}
