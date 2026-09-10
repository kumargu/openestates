import {
  DEFAULT_POLICY,
  type HomePlan,
  type PreparedHome,
  type TourPolicy,
  type TourStop,
} from "./types.ts";
import {
  area,
  bounds,
  distance,
  heading,
  interiorPoint,
  roomDimensions,
} from "./geometry.ts";
import { Navigator } from "./navigation.ts";
import { directRoom, roundRoute } from "./director.ts";

export function validatePlan(plan: HomePlan) {
  if (plan.version !== 1 || plan.units !== "metres")
    throw new Error("Expected HomePlan version 1 in metres.");
  const ids = new Set(plan.rooms.map((r) => r.id));
  if (!ids.size || ids.has("") || ids.size !== plan.rooms.length)
    throw new Error("Room IDs must be unique and nonempty.");
  for (const room of plan.rooms) {
    if (
      room.polygon.length < 3 ||
      room.polygon.some((p) => p.length !== 2 || !p.every(Number.isFinite)) ||
      area(room.polygon) < 0.1
    )
      throw new Error(`Invalid room polygon: ${room.id}`);
  }
  if (new Set(plan.walls.map((w) => w.id)).size !== plan.walls.length)
    throw new Error("Wall IDs must be unique.");
  for (const wall of plan.walls) {
    if (
      !wall.id ||
      distance(wall.a, wall.b) < 0.001 ||
      (wall.heightM !== undefined &&
        !(Number.isFinite(wall.heightM) && wall.heightM > 0))
    )
      throw new Error("Invalid wall dimensions.");
    if (![...wall.a, ...wall.b].every(Number.isFinite))
      throw new Error("Non-finite wall geometry.");
    const o = wall.opening;
    if (o && o.kind !== "window" && !o.connects)
      throw new Error(
        `Traversable opening needs room connectivity: ${wall.id}`,
      );
    if (
      o &&
      ((o.sillM !== undefined && !(Number.isFinite(o.sillM) && o.sillM >= 0)) ||
        (o.headM !== undefined &&
          !(Number.isFinite(o.headM) && o.headM > (o.sillM ?? 0))))
    )
      throw new Error(`Invalid opening height: ${wall.id}`);
    if (
      o &&
      (!(o.start >= 0 && o.end <= 1 && o.start < o.end) ||
        o.connects?.some((id) => id !== null && !ids.has(id)))
    )
      throw new Error(`Invalid opening: ${wall.id}`);
  }
  if (
    !(
      Number.isFinite(plan.architecture.ceilingM) &&
      Number.isFinite(plan.architecture.wallM) &&
      plan.architecture.ceilingM > 1.8 &&
      plan.architecture.wallM > 0
    )
  )
    throw new Error("Invalid architectural dimensions.");
}
export function prepareHome(
  plan: HomePlan,
  overrides: Partial<TourPolicy> = {},
): PreparedHome {
  validatePlan(plan);
  const policy = { ...DEFAULT_POLICY, ...overrides };
  if (
    ![
      policy.gridM,
      policy.walkMps,
      policy.clearanceM,
      policy.maxGridNodes,
      policy.eyeM,
      policy.turnRadiansPerSecond,
      policy.settleSeconds,
      policy.inspectSeconds,
      policy.entranceSeconds,
      policy.viewHoldSeconds,
      policy.doorwayMps,
      policy.accelerationMps2,
      policy.cornerRadiusM,
      policy.maxMovingYawError,
      policy.viewSampleM,
      policy.minViewDepthM,
    ].every((n) => Number.isFinite(n) && n > 0) ||
    policy.eyeM >= plan.architecture.ceilingM
  )
    throw new Error("Invalid tour policy.");
  const nav = new Navigator(plan, policy),
    graph = new Map(plan.rooms.map((r) => [r.id, [] as string[]]));
  for (const wall of plan.walls) {
    const c = wall.opening?.connects;
    if (c?.[1] && wall.opening?.kind !== "window") {
      graph.get(c[0])!.push(c[1]);
      graph.get(c[1])!.push(c[0]);
    }
  }
  const root = plan.walls.find((w) => w.id === plan.entranceWallId)!.opening!
    .connects![0];
  const visited = new Set<string>(),
    order: string[] = [];
  const entries = new Map<string, import("./types.ts").Wall>();
  const visit = (id: string, entry: import("./types.ts").Wall) => {
    if (visited.has(id)) return;
    entries.set(id, entry);
    visited.add(id);
    order.push(id);
    const neighbors = [...graph.get(id)!].sort((a, b) => {
      const ra = plan.rooms.find((r) => r.id === a)!,
        rb = plan.rooms.find((r) => r.id === b)!;
      return (
        policy.roomPriority[ra.kind] - policy.roomPriority[rb.kind] ||
        a.localeCompare(b)
      );
    });
    for (const next of neighbors)
      visit(
        next,
        plan.walls.find(
          (w) =>
            w.opening?.kind !== "window" &&
            w.opening?.connects?.includes(id) &&
            w.opening?.connects?.includes(next),
        )!,
      );
  };
  visit(root, plan.walls.find((w) => w.id === plan.entranceWallId)!);
  if (visited.size !== plan.rooms.length)
    throw new Error(
      `Disconnected rooms: ${plan.rooms
        .filter((r) => !visited.has(r.id))
        .map((r) => r.name)
        .join(", ")}`,
    );
  const stops: TourStop[] = order.map((roomId) => {
    const room = plan.rooms.find((r) => r.id === roomId)!;
    const center = interiorPoint(room.polygon, nav.safe),
      dimensions = roomDimensions(room.polygon, center);
    const views = directRoom(plan, room, entries.get(roomId)!, nav, policy);
    return {
      roomId,
      point: views[0].point,
      dimensions,
      heading: views[0].heading,
      views,
    };
  });
  const entrance = nav.entrance.outside;
  const routes = stops.map((stop, i) => {
    try {
      const previous = i ? stops[i - 1].views.at(-1)!.point : entrance;
      return roundRoute(
        nav.route(previous, stop.point),
        nav,
        policy.cornerRadiusM,
      );
    } catch (error) {
      throw new Error(`Route to ${stop.roomId}: ${(error as Error).message}`);
    }
  });
  return {
    plan,
    policy,
    entrance,
    entranceHeading: heading(entrance, nav.entrance.inside),
    stops,
    routes,
    bounds: bounds(plan.rooms.flatMap((r) => [...r.polygon])),
  };
}
