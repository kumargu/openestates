import type { Dimension, Vec2 } from "./types.ts";

export const add = (a: Vec2, b: Vec2): Vec2 => [a[0] + b[0], a[1] + b[1]];
export const sub = (a: Vec2, b: Vec2): Vec2 => [a[0] - b[0], a[1] - b[1]];
export const mul = (a: Vec2, n: number): Vec2 => [a[0] * n, a[1] * n];
export const distance = (a: Vec2, b: Vec2) =>
  Math.hypot(a[0] - b[0], a[1] - b[1]);
export const mix = (a: Vec2, b: Vec2, t: number): Vec2 =>
  add(a, mul(sub(b, a), t));
export const clamp = (n: number, a: number, b: number) =>
  Math.max(a, Math.min(b, n));
export const heading = (a: Vec2, b: Vec2) =>
  Math.atan2(b[0] - a[0], b[1] - a[1]);
export const angleDelta = (a: number, b: number) =>
  Math.atan2(Math.sin(b - a), Math.cos(b - a));
export const turnToward = (a: number, b: number, step: number) =>
  a + clamp(angleDelta(a, b), -step, step);
export function bounds(points: readonly Vec2[]) {
  const min: Vec2 = [
    Math.min(...points.map((p) => p[0])),
    Math.min(...points.map((p) => p[1])),
  ];
  const max: Vec2 = [
    Math.max(...points.map((p) => p[0])),
    Math.max(...points.map((p) => p[1])),
  ];
  return {
    min,
    max,
    center: mix(min, max, 0.5),
    span: Math.max(max[0] - min[0], max[1] - min[1]),
  };
}
export function pointSegmentDistance(p: Vec2, a: Vec2, b: Vec2) {
  const d = sub(b, a),
    v = sub(p, a),
    length2 = d[0] ** 2 + d[1] ** 2;
  return distance(
    p,
    mix(a, b, length2 ? clamp((v[0] * d[0] + v[1] * d[1]) / length2, 0, 1) : 0),
  );
}
export function contains(p: Vec2, polygon: readonly Vec2[]) {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i],
      b = polygon[j];
    if (pointSegmentDistance(p, a, b) < 1e-7) return true;
    if (
      a[1] > p[1] !== b[1] > p[1] &&
      p[0] < ((b[0] - a[0]) * (p[1] - a[1])) / (b[1] - a[1]) + a[0]
    )
      inside = !inside;
  }
  return inside;
}
export function area(polygon: readonly Vec2[]) {
  return (
    Math.abs(
      polygon.reduce((sum, a, i) => {
        const b = polygon[(i + 1) % polygon.length];
        return sum + a[0] * b[1] - b[0] * a[1];
      }, 0),
    ) / 2
  );
}
/** A deterministic point of good wall clearance; unlike a centroid it stays inside concave rooms. */
export function interiorPoint(
  polygon: readonly Vec2[],
  safe: (p: Vec2) => boolean,
) {
  const b = bounds(polygon),
    step = Math.max(0.12, b.span / 45);
  let best: Vec2 | undefined,
    score = -Infinity;
  for (let x = b.min[0] + step / 2; x < b.max[0]; x += step)
    for (let z = b.min[1] + step / 2; z < b.max[1]; z += step) {
      const p: Vec2 = [x, z];
      if (!contains(p, polygon) || !safe(p)) continue;
      const clearance = Math.min(
        ...polygon.map((a, i) =>
          pointSegmentDistance(p, a, polygon[(i + 1) % polygon.length]),
        ),
      );
      const candidate = clearance - 0.08 * distance(p, b.center);
      if (candidate > score) {
        best = p;
        score = candidate;
      }
    }
  if (!best)
    throw new Error(
      "A room has no navigable interior. Check its polygon and openings.",
    );
  return best;
}
/** Clips a line to the connected polygon interval containing the viewpoint. No bounding-box dimensions. */
function chord(polygon: readonly Vec2[], p: Vec2, axis: Vec2): [Vec2, Vec2] {
  const intersections: number[] = [];
  for (let i = 0; i < polygon.length; i++) {
    const a = polygon[i],
      edge = sub(polygon[(i + 1) % polygon.length], a),
      v = sub(a, p);
    const cross = axis[0] * edge[1] - axis[1] * edge[0];
    if (Math.abs(cross) < 1e-9) continue;
    const t = (v[0] * edge[1] - v[1] * edge[0]) / cross;
    const u = (v[0] * axis[1] - v[1] * axis[0]) / cross;
    if (u >= -1e-8 && u <= 1 + 1e-8) intersections.push(t);
  }
  const lower = Math.max(...intersections.filter((t) => t <= 1e-7));
  const upper = Math.min(...intersections.filter((t) => t >= -1e-7));
  return [
    add(p, mul(axis, Number.isFinite(lower) ? lower : 0)),
    add(p, mul(axis, Number.isFinite(upper) ? upper : 0)),
  ];
}
export function roomDimensions(
  polygon: readonly Vec2[],
  point: Vec2,
): Dimension[] {
  const longest = polygon
    .map((a, i) => sub(polygon[(i + 1) % polygon.length], a))
    .sort((a, b) => Math.hypot(...b) - Math.hypot(...a))[0];
  const axis = mul(longest, 1 / Math.hypot(...longest));
  const lines = [
    chord(polygon, point, axis),
    chord(polygon, point, [-axis[1], axis[0]]),
  ].sort((a, b) => distance(...b) - distance(...a));
  return lines.map(([a, b], i) => ({
    a,
    b,
    metres: distance(a, b),
    label: i === 0 ? "Length" : "Width",
  }));
}
export const pathLength = (path: readonly Vec2[]) =>
  path.slice(1).reduce((sum, p, i) => sum + distance(path[i], p), 0);
export function alongPath(path: readonly Vec2[], metres: number): Vec2 {
  for (let i = 1; i < path.length; i++) {
    const length = distance(path[i - 1], path[i]);
    if (metres <= length)
      return mix(path[i - 1], path[i], length ? metres / length : 0);
    metres -= length;
  }
  return path[path.length - 1];
}
