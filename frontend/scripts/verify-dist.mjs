import { readFile, readdir, stat } from "node:fs/promises";
import { resolve, relative, sep } from "node:path";

const frontendRoot = resolve(import.meta.dirname, "..");
const distRoot = resolve(frontendRoot, "dist");
const budget = JSON.parse(
  await readFile(resolve(frontendRoot, "bundle-budget.json"), "utf8"),
);

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async (entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? filesUnder(path) : [path];
  }));
  return nested.flat();
}

const files = await filesUnder(distRoot);
const records = await Promise.all(files.map(async (path) => ({
  path: relative(distRoot, path).split(sep).join("/"),
  bytes: (await stat(path)).size,
})));
const forbiddenRoots = [
  "backend/",
  "data/",
  "lake/",
  "media/",
  "pipeline/",
  "serving/",
  "societies/",
];
const forbidden = records.filter((record) =>
  forbiddenRoots.some((root) => record.path === root.slice(0, -1) || record.path.startsWith(root))
);
if (forbidden.length > 0) {
  throw new Error(`Forbidden deployment output: ${forbidden.map(({ path }) => path).join(", ")}`);
}

const totalBytes = records.reduce((sum, record) => sum + record.bytes, 0);
const jsRecords = records.filter(({ path }) => path.endsWith(".js"));
const cssRecords = records.filter(({ path }) => path.endsWith(".css"));
const jsBytes = jsRecords.reduce((sum, record) => sum + record.bytes, 0);
const cssBytes = cssRecords.reduce((sum, record) => sum + record.bytes, 0);
const largestJsBytes = Math.max(0, ...jsRecords.map(({ bytes }) => bytes));
const actual = { totalBytes, jsBytes, cssBytes, largestJsBytes };

for (const [metric, value] of Object.entries(actual)) {
  const limit = budget[metric];
  if (!Number.isFinite(limit) || limit <= 0) {
    throw new Error(`Invalid ${metric} limit in bundle-budget.json`);
  }
  if (value > limit) {
    throw new Error(`${metric} is ${value} bytes; budget is ${limit} bytes`);
  }
}

process.stdout.write(`${JSON.stringify({ files: records.length, ...actual, budget }, null, 2)}\n`);
