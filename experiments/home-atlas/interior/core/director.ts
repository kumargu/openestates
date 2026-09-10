import type {
  HomePlan,
  Room,
  TourPolicy,
  Vec2,
  ViewMoment,
  Wall,
} from "./types.ts";
import {
  add,
  angleDelta,
  bounds,
  contains,
  distance,
  heading,
  interiorPoint,
  mix,
  mul,
  roomDimensions,
  sub,
} from "./geometry.ts";
import type { Navigator } from "./navigation.ts";

/** Visibility is distinct from walkability: a camera can look across space it cannot occupy. */
export function rayDepth(room: Room, point: Vec2, yaw: number): number {
  const dir: Vec2 = [Math.sin(yaw), Math.cos(yaw)];
  let nearest = Infinity;
  for (let i = 0; i < room.polygon.length; i++) {
    const a = room.polygon[i],
      edge = sub(room.polygon[(i + 1) % room.polygon.length], a),
      v = sub(a, point);
    const cross = dir[0] * edge[1] - dir[1] * edge[0];
    if (Math.abs(cross) < 1e-8) continue;
    const t = (v[0] * edge[1] - v[1] * edge[0]) / cross,
      u = (v[0] * dir[1] - v[1] * dir[0]) / cross;
    if (t > 1e-5 && u >= 0 && u <= 1) nearest = Math.min(nearest, t);
  }
  return Number.isFinite(nearest) ? nearest : 0;
}

export function doorwayCenter(wall: Wall): Vec2 {
  const o = wall.opening!;
  return mix(wall.a, wall.b, (o.start + o.end) / 2);
}

/** Room-specific shots emerge from visibility and approach geometry, never named camera poses. */
export function directRoom(
  plan: HomePlan,
  room: Room,
  entry: Wall,
  nav: Navigator,
  policy: TourPolicy,
): ViewMoment[] {
  const door = doorwayCenter(entry),
    center = interiorPoint(room.polygon, nav.safe),
    b = bounds(room.polygon);
  const candidates: Vec2[] = [center];
  const step = Math.max(policy.viewSampleM, b.span / 18);
  for (let x = b.min[0] + step / 2; x < b.max[0]; x += step)
    for (let z = b.min[1] + step / 2; z < b.max[1]; z += step) {
      const p: Vec2 = [x, z];
      if (contains(p, room.polygon) && nav.safe(p)) candidates.push(p);
    }
  const visible = (a: Vec2, c: Vec2) => {
    const n = Math.ceil(distance(a, c) / 0.08);
    for (let i = 1; i < n; i++)
      if (!contains(mix(a, c, i / n), room.polygon)) return false;
    return true;
  };
  // Prefer useful diagonal views from the entrance side, with both side rays remaining in the room.
  const score = (p: Vec2, yaw: number) => {
    const depths = [-0.32, 0, 0.32].map((a) => rayDepth(room, p, yaw + a));
    return Math.min(...depths) * 1.8 + depths[1] * 0.4;
  };
  const bestHeading = (p: Vec2) => {
    const base = heading(p, center);
    let yaw = base,
      value = -Infinity;
    for (let i = 0; i < 48; i++) {
      const h = base + (i * Math.PI) / 24,
        s = score(p, h);
      if (s > value) {
        value = s;
        yaw = h;
      }
    }
    return { yaw, value };
  };
  let reveal = center,
    revealYaw = bestHeading(center).yaw,
    best = -Infinity;
  for (const p of candidates) {
    // Require the entry and the initial position to share an unobstructed room interval.
    if (!visible(mix(door, center, 0.02), p)) continue;
    const { yaw, value } = bestHeading(p),
      d = distance(p, door);
    const rank = value - 1.6 * d - 2 * Math.max(0, 0.75 - d);
    if (rank > best) {
      best = rank;
      reveal = p;
      revealYaw = yaw;
    }
  }
  const make = (
    purpose: ViewMoment["purpose"],
    p: Vec2,
    yaw: number,
  ): ViewMoment => {
    const depthM = rayDepth(room, p, yaw);
    return {
      purpose,
      point: p,
      target: add(p, mul([Math.sin(yaw), Math.cos(yaw)], depthM * 0.72)),
      heading: yaw,
      pitch: -0.14,
      holdSeconds: policy.viewHoldSeconds,
      depthM,
    };
  };
  const shots = [make("reveal", reveal, revealYaw)];
  // Look along the room's long dimension from its end, rather than across a narrow balcony.
  const dims = roomDimensions(room.polygon, center),
    axis = heading(dims[0].a, dims[0].b);
  let lengthShot: ViewMoment | undefined,
    lengthScore = -Infinity;
  for (const p of candidates)
    for (const yaw of [axis, axis + Math.PI]) {
      const depth = rayDepth(room, p, yaw),
        s = score(p, yaw) - 0.45 * distance(p, reveal);
      if (
        depth < policy.minViewDepthM ||
        distance(p, reveal) > Math.max(1.8, b.span * 0.55)
      )
        continue;
      if (!visible(reveal, p) || !nav.clear(reveal, p)) continue;
      if (s > lengthScore) {
        lengthScore = s;
        lengthShot = make("length", p, yaw);
      }
    }
  if (
    lengthShot &&
    (distance(reveal, lengthShot.point) > 0.45 ||
      Math.abs(angleDelta(revealYaw, lengthShot.heading)) > 0.28)
  )
    shots.push(lengthShot);
  // A final glance connects the room back to its door. Omit it if it would stare into a nearby wall.
  const last = shots[shots.length - 1],
    back = heading(last.point, door),
    backDepth = rayDepth(room, last.point, back);
  if (
    distance(last.point, door) > policy.minViewDepthM &&
    visible(last.point, mix(door, last.point, 0.02)) &&
    Math.abs(angleDelta(last.heading, back)) > 0.55 &&
    backDepth > policy.minViewDepthM
  ) {
    shots.push(make("connection", last.point, back));
  }
  // Small rooms still get their own full inspection; no timed drive-through.
  if (shots.length === 1) shots[0].holdSeconds = policy.inspectSeconds;
  return shots;
}

/** Round only corners whose entire replacement arc is collision-free. Retain tight corners for turn-in-place. */
export function roundRoute(
  path: readonly Vec2[],
  nav: Navigator,
  radius: number,
): Vec2[] {
  if (path.length < 3) return [...path];
  const result: Vec2[] = [path[0]];
  for (let i = 1; i < path.length - 1; i++) {
    const a = path[i - 1],
      b = path[i],
      c = path[i + 1],
      ab = distance(a, b),
      bc = distance(b, c);
    const r = Math.min(radius, ab * 0.3, bc * 0.3);
    if (r < 0.04) {
      result.push(b);
      continue;
    }
    const enter = mix(b, a, r / ab),
      leave = mix(b, c, r / bc),
      arc: Vec2[] = [];
    for (let j = 0; j <= 12; j++) {
      const t = j / 12;
      arc.push(mix(mix(enter, b, t), mix(b, leave, t), t));
    }
    if (
      nav.clear(result[result.length - 1], enter) &&
      arc.slice(1).every((p, j) => nav.clear(arc[j], p))
    )
      result.push(...arc);
    else result.push(b);
  }
  result.push(path[path.length - 1]);
  return result.filter((p, i) => !i || distance(p, result[i - 1]) > 1e-6);
}
