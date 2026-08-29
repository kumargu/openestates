import assert from "node:assert/strict";
import test from "node:test";

import { semanticPalette } from "./semantic_colors.mjs";

test("semantic colors remain deterministic and collision-free", () => {
  const ids = [
    "site-boundary",
    "osm-way-1000001006",
    "osm-way-1000001428",
    "osm-way-20",
  ];
  const first = semanticPalette(ids);
  const second = semanticPalette([...ids].reverse());
  const colors = Object.values(first).map(({ hex }) => hex);

  assert.deepEqual(first, second);
  assert.equal(new Set(colors).size, ids.length);
  assert.notDeepEqual(
    first["osm-way-1000001006"],
    first["osm-way-1000001428"],
  );
});
