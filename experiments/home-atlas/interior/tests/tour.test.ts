import test from "node:test";
import assert from "node:assert/strict";
import { prepareHome } from "../core/tour.ts";
import { HomeTour } from "../core/playback.ts";
import { Navigator } from "../core/navigation.ts";
import { contains, distance, mix, roomDimensions } from "../core/geometry.ts";
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
  while (tour.playing && ticks++ < 10000) tour.tick(1 / 60);
  assert.equal(tour.frame.phase, "complete");
  assert.ok(ticks / 60 < 40);
});
