import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("RERA document rows open their official files", async () => {
  const reportPage = await readFile(
    new URL("../src/pages/ReraReportPage.tsx", import.meta.url),
    "utf8",
  );

  assert.match(
    reportPage,
    /<a href=\{document\.url\} target="_blank" rel="noreferrer">\{document\.label\}<\/a>/,
  );
});
