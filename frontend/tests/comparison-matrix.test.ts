import assert from "node:assert/strict";
import test from "node:test";
import { buildTaggedMatrixRows } from "../src/lib/comparisonMatrix.ts";

type Evidence = {
  id: string;
  group: string;
  primaryLabel: string;
  distance: number;
};

test("comparison tag rows stay aligned when each home receives tags in a different order", () => {
  const columns = [
    {
      key: "home-a",
      items: [
        { id: "a-metro", group: "access", primaryLabel: "metro", distance: 0.2 },
        { id: "a-school", group: "access", primaryLabel: "schools", distance: 1.1 },
      ],
    },
    {
      key: "home-b",
      items: [
        { id: "b-school", group: "access", primaryLabel: "schools", distance: 0.4 },
        { id: "b-hospital", group: "access", primaryLabel: "hospitals", distance: 0.6 },
        { id: "b-metro", group: "access", primaryLabel: "metro", distance: 0.1 },
      ],
    },
  ] satisfies Array<{ key: string; items: Evidence[] }>;

  const rows = buildTaggedMatrixRows(
    columns,
    "access",
    (left, right) => left.localeCompare(right),
    (left, right) => left.distance - right.distance,
  );

  assert.deepEqual(rows.map((row) => row.tag), ["hospitals", "metro", "schools"]);
  assert.deepEqual(
    rows.map((row) => row.cells.map((cell) => cell.items.map((item) => item.id))),
    [
      [[], ["b-hospital"]],
      [["a-metro"], ["b-metro"]],
      [["a-school"], ["b-school"]],
    ],
  );
});

test("comparison tag rows exclude evidence from other groups", () => {
  const rows = buildTaggedMatrixRows(
    [{
      key: "home-a",
      items: [
        { id: "metro", group: "access", primaryLabel: "metro", distance: 0.2 },
        { id: "water", group: "risks", primaryLabel: "water", distance: 0 },
      ],
    }],
    "access",
    (left, right) => left.localeCompare(right),
    (left, right) => left.distance - right.distance,
  );

  assert.deepEqual(rows.map((row) => row.tag), ["metro"]);
});
