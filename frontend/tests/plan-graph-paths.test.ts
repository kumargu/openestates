import assert from "node:assert/strict";
import test from "node:test";
import {
  buildWealthGapAreas,
  linePathForValues,
  type WealthGapPoint,
} from "../src/features/home-plan/planGraphPaths.ts";

const scale = {
  x: (year: number) => year * 10,
  y: (value: number) => 100 - value,
};

test("wealth gap areas split at buy/rent crossover", () => {
  const points: WealthGapPoint[] = [
    { year: 0, buyNetWorth: 0, rentNetWorth: 10 },
    { year: 1, buyNetWorth: 20, rentNetWorth: 0 },
  ];

  const areas = buildWealthGapAreas(points, scale);

  assert.deepEqual(areas.map((area) => area.leader), ["rent", "buy"]);
  assert.match(areas[0].path, /^M0\.0,100\.0 L3\.3,93\.3 L3\.3,93\.3 L0\.0,90\.0 Z$/);
  assert.match(areas[1].path, /^M3\.3,93\.3 L10\.0,80\.0 L10\.0,100\.0 L3\.3,93\.3 Z$/);
});

test("wealth gap areas merge contiguous years with the same leader", () => {
  const points: WealthGapPoint[] = [
    { year: 0, buyNetWorth: 10, rentNetWorth: 0 },
    { year: 1, buyNetWorth: 20, rentNetWorth: 5 },
    { year: 2, buyNetWorth: 30, rentNetWorth: 8 },
  ];

  const areas = buildWealthGapAreas(points, scale);

  assert.equal(areas.length, 1);
  assert.equal(areas[0].leader, "buy");
  assert.equal(areas[0].path, "M0.0,90.0 L10.0,80.0 L20.0,70.0 L20.0,92.0 L10.0,95.0 L0.0,100.0 Z");
});

test("line path uses the same scale callbacks as area paths", () => {
  assert.equal(
    linePathForValues([10, 20, 30], scale.x, scale.y),
    "M0.0,90.0 L10.0,80.0 L20.0,70.0",
  );
});
