import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";
import { PUBLIC_BRAND_NAME } from "../src/lib/brand.ts";

const frontendRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(frontendRoot, "..");

async function sourceFilesUnder(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(entries.map(async (entry) => {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) return sourceFilesUnder(path);
    return /\.(?:ts|tsx)$/.test(entry.name) ? [path] : [];
  }));
  return files.flat();
}

test("public product surfaces use the 80feet brand", async () => {
  assert.equal(PUBLIC_BRAND_NAME, "80feet");

  const files = [
    resolve(frontendRoot, "index.html"),
    resolve(frontendRoot, "public/favicon.svg"),
    resolve(repositoryRoot, "app/config/dag/search_guardrails.json"),
    resolve(repositoryRoot, "backend/src/routes/property_map.rs"),
    ...await sourceFilesUnder(resolve(frontendRoot, "src")),
  ];

  for (const path of files) {
    const source = await readFile(path, "utf8");
    assert.doesNotMatch(source, /\bOpen[ ]?Estates\b/, path);
  }
});

test("static browser identity names 80feet", async () => {
  const [html, favicon] = await Promise.all([
    readFile(resolve(frontendRoot, "index.html"), "utf8"),
    readFile(resolve(frontendRoot, "public/favicon.svg"), "utf8"),
  ]);

  assert.match(html, /<title>80feet<\/title>/);
  assert.match(favicon, /aria-label="80feet"/);
});
