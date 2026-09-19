import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("property detail uses bounded inputs", async () => {
  const propertyPage = await readFile(
    new URL("../src/pages/PropertyPage.tsx", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(propertyPage, /\bgetProperties\b/);
});
