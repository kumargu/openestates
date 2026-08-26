import assert from "node:assert/strict";
import test from "node:test";

import { completeSettledValues } from "../src/lib/compare.ts";

test("comparison details are ready only when every property loads", () => {
  assert.deepEqual(
    completeSettledValues([
      { status: "fulfilled", value: "home-one" },
      { status: "fulfilled", value: "home-two" },
    ], 2),
    ["home-one", "home-two"],
  );
  assert.equal(
    completeSettledValues([
      { status: "fulfilled", value: "home-one" },
      { status: "rejected", reason: new Error("offline") },
    ], 2),
    null,
  );
});
