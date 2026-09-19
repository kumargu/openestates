import test from "node:test";
import assert from "node:assert/strict";
import { prepareHome } from "../core/tour.ts";
import { HomeTour } from "../core/playback.ts";
import { Navigator } from "../core/navigation.ts";
import {
  angleDelta,
  contains,
  distance,
  heading,
  mix,
  roomDimensions,
} from "../core/geometry.ts";
import { rayDepth } from "../core/director.ts";
import type { Vec2 } from "../core/types.ts";
import { waterford } from "../fixtures/waterford.ts";
import { configuration } from "../fixtures/configurations.ts";

test("brochure apartment: every stop and route is safe, attached rooms remain connected", () => {
  const home = prepareHome(waterford),
    nav = new Navigator(home.plan, home.policy);
  assert.equal(home.stops.length, 13);
  assert.equal(new Set(home.stops.map((s) => s.roomId)).size, 13);
  for (const route of home.routes)
    for (let i = 1; i < route.length; i++)
      assert.ok(nav.clear(route[i - 1], route[i]));
  for (const [child, parent] of [
    ["bath1", "bed1"],
    ["bath2", "bed2"],
    ["bath3", "bed3"],
    ["balcony2", "bed1"],
  ]) {
    const index = home.stops.findIndex((s) => s.roomId === child);
    assert.ok(index > home.stops.findIndex((s) => s.roomId === parent));
    const poly = waterford.rooms.find((r) => r.id === parent)!.polygon;
    const path = home.routes[index];
    assert.ok(
      path
        .slice(1)
        .some((p, i) =>
          Array.from({ length: 101 }, (_, j) => mix(path[i], p, j / 100)).some(
            (q) => contains(q, poly),
          ),
        ),
      "Attached space is reached through its parent",
    );
  }
});
test("same preparation supports studio, 1, 2 and 4 bedrooms, rotated coordinates", () => {
  for (const n of [0, 1, 2, 4] as const)
    for (const rotation of [0, 0.41]) {
      const h = prepareHome(configuration(n, rotation));
      assert.equal(h.stops.length, n + 1);
      assert.ok(
        h.stops.every((s) =>
          s.dimensions.every((d) => d.metres > 0 && Number.isFinite(d.metres)),
        ),
      );
    }
});
test("concave room measurements stay in their connected interior, including rotated layouts", () => {
  const polygon: Vec2[] = [
    [0, 0],
    [6, 0],
    [6, 2],
    [2, 2],
    [2, 6],
    [0, 6],
  ];
  for (const theta of [0, 0.63]) {
    const turn = ([x, y]: Vec2): Vec2 => [
      x * Math.cos(theta) - y * Math.sin(theta),
      x * Math.sin(theta) + y * Math.cos(theta),
    ];
    const p = polygon.map(turn),
      dims = roomDimensions(p, turn([1, 3]));
    for (const d of dims)
      for (let i = 0; i <= 100; i++)
        assert.ok(contains(mix(d.a, d.b, i / 100), p));
    assert.ok(dims.some((d) => Math.abs(d.metres - 2) < 0.001));
  }
});
test("rejects disconnected rooms, blocked doors and over-budget navigation", () => {
  const plan = configuration(1);
  assert.throws(
    () =>
      prepareHome({
        ...plan,
        walls: plan.walls.map((w) =>
          w.id === "door-0" ? { ...w, opening: undefined } : w,
        ),
      }),
    /Disconnected/,
  );
  assert.throws(
    () =>
      prepareHome({
        ...plan,
        walls: plan.walls.map((w) =>
          w.id === "door-0"
            ? { ...w, opening: { ...w.opening!, start: 0.49, end: 0.51 } }
            : w,
        ),
      }),
    /No continuous|No safe/,
  );
  assert.throws(() => prepareHome(waterford, { maxGridNodes: 10 }), /budget/);
});
test("arrival precedes inspection; pauses freeze time and pose; manual movement resumes from actual position", () => {
  const h = prepareHome(configuration(1)),
    tour = new HomeTour(h);
  tour.play();
  let ticks = 0;
  while (tour.frame.phase !== "inspecting" && ticks++ < 10000)
    tour.tick(1 / 60);
  assert.ok(ticks < 10000);
  assert.ok(distance(tour.frame.position, h.stops[0].point) < 0.001);
  tour.pause();
  const before = { ...tour.frame };
  for (let i = 0; i < 600; i++) tour.tick(1 / 60);
  assert.deepEqual(tour.frame, before);
  tour.move(0.5, 0.3, 0.05);
  const manual = tour.frame.position;
  tour.play();
  tour.tick(1 / 60);
  assert.ok(
    distance(manual, tour.frame.position) <= h.policy.walkMps / 60 + 0.001,
  );
});
test("complete tour at 30/60 fps stays collision-safe and takes comparable time", () => {
  const h = prepareHome(configuration(1)),
    times: number[] = [];
  for (const fps of [30, 60]) {
    const tour = new HomeTour(h);
    tour.play();
    let ticks = 0;
    while (tour.playing && ticks++ < fps * 180) {
      const before = tour.frame.position;
      tour.tick(1 / fps);
      assert.ok(tour.navigator.clear(before, tour.frame.position));
    }
    assert.equal(tour.frame.phase, "complete");
    assert.equal(tour.frame.progress, 1);
    times.push(ticks / fps);
  }
  assert.ok(Math.abs(times[0] - times[1]) < 0.5);
});
test("reduced motion reaches every stop without animated travel", () => {
  const h = prepareHome(configuration(1)),
    tour = new HomeTour(h);
  tour.reducedMotion = true;
  tour.play();
  let ticks = 0;
  const poses = [
    h.entrance,
    ...h.stops.flatMap((s) => s.views.map((v) => v.point)),
  ];
  while (tour.playing && ticks++ < 10000) {
    tour.tick(1 / 60);
    assert.ok(
      poses.some((p) => distance(p, tour.frame.position) < 0.001),
      "Reduced motion must not animate intermediate travel",
    );
  }
  assert.equal(tour.frame.phase, "complete");
  const expected =
    h.policy.entranceSeconds +
    h.stops
      .flatMap((s) => s.views)
      .reduce((n, v) => n + v.holdSeconds + h.policy.settleSeconds, 0);
  assert.ok(
    Math.abs(ticks / 60 - expected) < 1,
    "Only planned holds and settling should consume time",
  );
});

test("full apartment: every view is held still, turns precede movement, and all views remain reachable", () => {
  const h = prepareHome(waterford),
    tour = new HomeTour(h);
  tour.play();
  const holds = new Map<string, number>();
  let ticks = 0;
  while (tour.playing && ticks++ < 60 * 900) {
    const before = { ...tour.frame };
    const f = tour.tick(1 / 60);
    assert.ok(
      tour.navigator.clear(before.position, f.position),
      "Every frame remains collision-free",
    );
    const moved = distance(before.position, f.position);
    assert.ok(
      moved <= h.policy.walkMps / 60 + 0.006,
      "No hidden position jump",
    );
    if (moved > 0.00001)
      assert.ok(
        Math.abs(angleDelta(f.heading, heading(before.position, f.position))) <=
          h.policy.maxMovingYawError + 0.05,
        "No sideways travel into doorways",
      );
    if (before.phase === "inspecting" && f.phase === "inspecting") {
      assert.equal(moved, 0);
      assert.equal(before.heading, f.heading);
      const key = f.stopIndex + ":" + f.viewIndex;
      holds.set(key, (holds.get(key) ?? 0) + 1 / 60);
    }
  }
  assert.equal(tour.frame.phase, "complete");
  h.stops.forEach((s, i) =>
    s.views.forEach((v, j) =>
      assert.ok(
        (holds.get(i + ":" + j) ?? 0) >= v.holdSeconds - 0.08,
        `${s.roomId}/${v.purpose} got its viewing time`,
      ),
    ),
  );
});

test("view director looks into room depth; narrow balconies are viewed along their length", () => {
  for (const plan of [waterford, configuration(1, 0.57)]) {
    const h = prepareHome(plan);
    for (const s of h.stops) {
      const room = plan.rooms.find((r) => r.id === s.roomId)!;
      for (const v of s.views) {
        assert.ok(contains(v.target, room.polygon));
        assert.ok(
          v.depthM >= h.policy.minViewDepthM,
          `${room.name} ${v.purpose}: ${v.depthM}m`,
        );
      }
      if (room.kind === "balcony")
        assert.ok(
          s.views.some(
            (v) =>
              rayDepth(room, v.point, v.heading) >
              s.dimensions[0].metres * 0.65,
          ),
          "Show the usable balcony length",
        );
    }
  }
});

test("looking around during a pause resumes through a bounded turn, and faster pace preserves inspection duration", () => {
  const h = prepareHome(configuration(0)),
    tour = new HomeTour(h);
  tour.play();
  let ticks = 0;
  while (tour.frame.phase !== "inspecting" && ticks++ < 10000)
    tour.tick(1 / 60);
  tour.look(1.2, 0.2);
  const pose = { ...tour.frame };
  tour.play();
  tour.tick(1 / 60);
  assert.ok(
    Math.abs(angleDelta(pose.heading, tour.frame.heading)) <=
      h.policy.turnRadiansPerSecond / 60 + 0.001,
  );
  assert.deepEqual(tour.frame.position, pose.position);
  for (const rate of [0.65, 1.25]) {
    const t = new HomeTour(h);
    t.rate = rate;
    t.play();
    let hold = 0,
      n = 0;
    while (t.playing && n++ < 20000) {
      const before = t.frame.phase;
      t.tick(1 / 60);
      if (before === "inspecting" && t.frame.phase === "inspecting")
        hold += 1 / 60;
    }
    assert.ok(
      Math.abs(
        hold -
          h.stops
            .flatMap((s) => s.views)
            .reduce((a, v) => a + v.holdSeconds, 0),
      ) < 0.15,
    );
  }
});
