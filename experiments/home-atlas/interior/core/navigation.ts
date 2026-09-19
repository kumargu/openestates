import type { HomePlan, TourPolicy, Vec2, Wall } from "./types.ts";
import {
  add,
  bounds,
  contains,
  distance,
  mix,
  mul,
  pointSegmentDistance,
  sub,
} from "./geometry.ts";

export function solidWallSegments(wall: Wall): [Vec2, Vec2][] {
  const o = wall.opening;
  const parts =
    o && o.kind !== "window"
      ? [
          [0, o.start],
          [o.end, 1],
        ]
      : [[0, 1]];
  return parts
    .filter(([a, b]) => b - a > 1e-6)
    .map(([a, b]) => [mix(wall.a, wall.b, a), mix(wall.a, wall.b, b)]);
}
export function entrancePoint(plan: HomePlan) {
  const wall = plan.walls.find((w) => w.id === plan.entranceWallId);
  if (
    !wall?.opening?.connects ||
    wall.opening.kind === "window" ||
    wall.opening.connects[1] !== null
  )
    throw new Error(
      "The main entrance must explicitly connect a room to the exterior.",
    );
  const center = mix(
    wall.a,
    wall.b,
    (wall.opening.start + wall.opening.end) / 2,
  );
  const direction = sub(wall.b, wall.a),
    normal: Vec2 = [-direction[1], direction[0]];
  const unit = mul(normal, 1 / Math.hypot(...normal));
  const room = plan.rooms.find((r) => r.id === wall.opening!.connects![0])!;
  const inward = contains(add(center, mul(unit, 0.25)), room.polygon)
    ? unit
    : mul(unit, -1);
  return {
    outside: add(center, mul(inward, -0.65)),
    inside: add(center, mul(inward, 0.45)),
    center,
    inward,
  };
}
export class Navigator {
  readonly plan: HomePlan;
  readonly policy: TourPolicy;
  readonly walls: [Vec2, Vec2][];
  readonly entrance: ReturnType<typeof entrancePoint>;
  constructor(plan: HomePlan, policy: TourPolicy) {
    this.plan = plan;
    this.policy = policy;
    this.walls = plan.walls.flatMap(solidWallSegments);
    this.entrance = entrancePoint(plan);
  }
  safe = (p: Vec2): boolean => {
    const onEntrance =
      pointSegmentDistance(p, this.entrance.outside, this.entrance.inside) <
      this.policy.clearanceM + 0.08;
    if (!onEntrance && !this.plan.rooms.some((r) => contains(p, r.polygon)))
      return false;
    if (
      this.walls.some(
        ([a, b]) =>
          pointSegmentDistance(p, a, b) <
          this.policy.clearanceM + this.plan.architecture.wallM / 2,
      )
    )
      return false;
    // Keep a human-sized footprint away from un-walled polygon boundaries too.
    return (
      onEntrance ||
      [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ].every((d) => {
        const q = add(p, mul(d as unknown as Vec2, this.policy.clearanceM));
        return this.plan.rooms.some((r) => contains(q, r.polygon));
      })
    );
  };
  clear(a: Vec2, b: Vec2): boolean {
    const n = Math.max(
      1,
      Math.ceil(distance(a, b) / Math.min(0.045, this.policy.gridM / 3)),
    );
    for (let i = 0; i <= n; i++) if (!this.safe(mix(a, b, i / n))) return false;
    return true;
  }
  route(start: Vec2, end: Vec2): Vec2[] {
    if (!this.safe(start) || !this.safe(end))
      throw new Error("The route endpoint is not walkable.");
    if (this.clear(start, end)) return [start, end];
    const step = this.policy.gridM,
      b = bounds([
        ...this.plan.rooms.flatMap((r) => [...r.polygon]),
        this.entrance.outside,
      ]);
    const origin: Vec2 = [b.min[0] - step, b.min[1] - step];
    const cols = Math.ceil((b.max[0] - origin[0]) / step) + 2,
      rows = Math.ceil((b.max[1] - origin[1]) / step) + 2;
    if (cols * rows > this.policy.maxGridNodes)
      throw new Error("Plan exceeds the configured navigation grid budget.");
    const point = (id: number): Vec2 => [
      origin[0] + (id % cols) * step,
      origin[1] + Math.floor(id / cols) * step,
    ];
    const nearest = (p: Vec2) => {
      let best = -1,
        score = Infinity;
      const x = Math.round((p[0] - origin[0]) / step),
        z = Math.round((p[1] - origin[1]) / step);
      for (let dz = -3; dz <= 3; dz++)
        for (let dx = -3; dx <= 3; dx++) {
          if (x + dx < 0 || x + dx >= cols || z + dz < 0 || z + dz >= rows)
            continue;
          const id = (z + dz) * cols + x + dx,
            q = point(id),
            d = distance(p, q);
          if (d < score && this.clear(p, q)) {
            best = id;
            score = d;
          }
        }
      if (best < 0)
        throw new Error("No safe connection to the navigation grid.");
      return best;
    };
    const source = nearest(start),
      target = nearest(end),
      prev = new Int32Array(cols * rows).fill(-2);
    const valid = new Int8Array(cols * rows),
      queue = [source];
    prev[source] = -1;
    const isSafe = (id: number) => {
      if (!valid[id]) valid[id] = this.safe(point(id)) ? 1 : -1;
      return valid[id] === 1;
    };
    for (let head = 0; head < queue.length && prev[target] === -2; head++) {
      const id = queue[head],
        x = id % cols,
        z = Math.floor(id / cols);
      for (const [dx, dz] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        if (x + dx < 0 || x + dx >= cols || z + dz < 0 || z + dz >= rows)
          continue;
        const next = id + dx + dz * cols;
        if (
          prev[next] === -2 &&
          isSafe(next) &&
          this.clear(point(id), point(next))
        ) {
          prev[next] = id;
          queue.push(next);
        }
      }
    }
    if (prev[target] === -2)
      throw new Error(
        "No continuous doorway route. Review the source plan; teleportation is disabled.",
      );
    const path: Vec2[] = [end];
    for (let id = target; id !== -1; id = prev[id]) path.push(point(id));
    path.push(start);
    path.reverse();
    const smooth: Vec2[] = [start];
    for (let i = 0; i < path.length - 1; ) {
      let next = path.length - 1;
      while (next > i + 1 && !this.clear(path[i], path[next])) next--;
      smooth.push(path[next]);
      i = next;
    }
    return smooth;
  }
}
