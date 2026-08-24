import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("property detail uses bounded inputs and card-based comparison", async () => {
  const propertyPage = await readFile(
    new URL("../src/pages/PropertyPage.tsx", import.meta.url),
    "utf8",
  );
  const shortCompare = await readFile(
    new URL("../src/components/property/PropertyShortCompare.tsx", import.meta.url),
    "utf8",
  );

  assert.doesNotMatch(propertyPage, /\bgetProperties\b/);
  assert.doesNotMatch(shortCompare, /<table\b/);
  assert.match(shortCompare, /homes\.length !== 3/);
});
