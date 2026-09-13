import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(frontendRoot, "..");
const fixturePath = join(
  frontendRoot,
  "src/lib/fixtures/prestige-waterford-api/surface-around-this-home.json",
);

function localEnvironment() {
  const values = {};
  for (const filename of [".env.local", ".env.atlas.local", ".env.atlas"]) {
    const path = join(frontendRoot, filename);
    if (!existsSync(path)) continue;
    for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
      const match = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/);
      if (!match) continue;
      values[match[1]] = match[2].replace(/^['"]|['"]$/g, "");
    }
  }
  return values;
}

function fail(message, suggestion) {
  console.error(`Atlas lab preflight failed: ${message}`);
  if (suggestion) console.error(`Fix: ${suggestion}`);
  process.exit(1);
}

const localEnv = localEnvironment();
const mapsKey = process.env.VITE_GOOGLE_MAPS_API_KEY
  ?? localEnv.VITE_GOOGLE_MAPS_API_KEY;
if (!mapsKey || mapsKey === "REDACTED_FOR_FIXTURE") {
  fail(
    "VITE_GOOGLE_MAPS_API_KEY is not configured.",
    "Add it to frontend/.env.local (this file is gitignored).",
  );
}
if (!existsSync(fixturePath)) {
  fail("the Prestige Waterford fixtures are missing.");
}

const requestedChannel = process.env.ATLAS_BROWSER_CHANNEL?.trim();
const browserPath = chromium.executablePath();
if (!requestedChannel && !existsSync(browserPath)) {
  fail(
    "the project-pinned Chromium binary is not installed.",
    "Run PLAYWRIGHT_DOWNLOAD_CONNECTION_TIMEOUT=120000 npx playwright install chromium",
  );
}

const runId = `lab-${new Date().toISOString().replace(/[:.]/g, "-")}`;
const requestedSpec = process.argv.includes("--full")
  ? "e2e/atlas/parity.spec.ts"
  : "e2e/atlas/lab.spec.ts";
const executable = join(
  frontendRoot,
  "node_modules",
  ".bin",
  process.platform === "win32" ? "playwright.cmd" : "playwright",
);
const result = spawnSync(
  executable,
  ["test", "--config", "playwright.atlas.config.ts", requestedSpec, "--reporter=line"],
  {
    cwd: frontendRoot,
    env: {
      ...localEnv,
      ...process.env,
      ATLAS_RUN_ID: runId,
    },
    encoding: "utf8",
    stdio: "inherit",
  },
);

const outputRoot = join(frontendRoot, "test-results", "atlas-parity", runId);
if (existsSync(outputRoot)) {
  const files = walk(outputRoot);
  const screenshots = files.filter((path) => path.endsWith(".png"));
  const fixtureHash = createHash("sha256")
    .update(readFileSync(fixturePath))
    .digest("hex");
  const gitSha = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).stdout.trim();
  const manifest = {
    generatedAt: new Date().toISOString(),
    gitSha,
    fixtureSha256: fixtureHash,
    fixtureRoutes: {
      cameraLab: "/property/fixture-prestige-waterford-3bhk",
      backendReplay: "/property/discovered-prestige-waterford-3bhk",
    },
    spec: requestedSpec,
    screenshots: screenshots.map((path) => relative(outputRoot, path)),
  };
  writeFileSync(
    join(outputRoot, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  writeFileSync(join(outputRoot, "index.html"), contactSheet(manifest));
  console.log(`Atlas review: ${join(outputRoot, "index.html")}`);
}

process.exit(result.status ?? 1);

function walk(root) {
  return readdirSync(root).flatMap((name) => {
    const path = join(root, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function contactSheet(manifest) {
  const cards = manifest.screenshots.map((path) => `
    <figure>
      <img src="${escapeHtml(path)}" alt="${escapeHtml(path)}">
      <figcaption>${escapeHtml(path)}</figcaption>
    </figure>`).join("");
  return `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OpenEstates Atlas review</title>
<style>
  :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
  body { margin: 0; padding: 28px; background: #101815; color: #f1f6f3; }
  header { max-width: 760px; margin: 0 auto 24px; }
  h1 { margin: 0 0 8px; font-size: clamp(24px, 4vw, 42px); }
  p { margin: 4px 0; color: #aabbb2; }
  main { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 18px; }
  figure { margin: 0; overflow: hidden; border: 1px solid #2a3d34; border-radius: 18px; background: #17221d; }
  img { display: block; width: 100%; height: auto; background: #0b100e; }
  figcaption { padding: 12px 14px; font: 12px ui-monospace, monospace; color: #c6d6ce; }
</style>
<body>
  <header>
    <h1>Atlas visual review</h1>
    <p>${escapeHtml(manifest.generatedAt)} · ${escapeHtml(manifest.gitSha.slice(0, 12))}</p>
    <p>${escapeHtml(manifest.spec)}</p>
  </header>
  <main>${cards || "<p>No screenshots were produced.</p>"}</main>
</body>
</html>\n`;
}
