import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

type VercelRoute = {
  src?: string;
  dest?: string;
  handle?: string;
  continue?: boolean;
  headers?: Record<string, string>;
};

const vercelConfig = JSON.parse(
  readFileSync(new URL("../vercel.json", import.meta.url), "utf8"),
) as { routes: VercelRoute[] };
const packageConfig = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
) as { engines?: { node?: string } };

test("Vercel serves real files before the SPA fallback", () => {
  const filesystemIndex = vercelConfig.routes.findIndex(({ handle }) => handle === "filesystem");
  const fallbackIndex = vercelConfig.routes.findIndex(({ dest }) => dest === "/index.html");
  assert.ok(filesystemIndex >= 0);
  assert.ok(fallbackIndex > filesystemIndex);
});

test("Vercel keeps hashed assets immutable and HTML revalidated", () => {
  const assets = vercelConfig.routes.find(({ src }) => src === "/assets/.*");
  const fallback = vercelConfig.routes.find(({ dest }) => dest === "/index.html");
  assert.equal(assets?.headers?.["Cache-Control"], "public, max-age=31536000, immutable");
  assert.equal(fallback?.headers?.["Cache-Control"], "public, max-age=0, must-revalidate");
});

test("static responses carry the launch security policy", () => {
  const globalHeaders = vercelConfig.routes.find(({ src, continue: keepMatching }) =>
    src === "/.*" && keepMatching
  )?.headers;
  assert.equal(globalHeaders?.["X-Content-Type-Options"], "nosniff");
  assert.match(globalHeaders?.["Content-Security-Policy"] ?? "", /frame-ancestors 'none'/);
  assert.match(globalHeaders?.["Content-Security-Policy"] ?? "", /https:\/\/api\.80feet\.app/);
  assert.match(globalHeaders?.["Content-Security-Policy"] ?? "", /https:\/\/tiles\.openfreemap\.org/);
});

test("Node major matches CI and Vercel", () => {
  assert.equal(packageConfig.engines?.node, "22.x");
});
