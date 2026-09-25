import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";

const repo = fileURLToPath(new URL("../../../", import.meta.url));
const manifest = resolve(repo, "backend/prototypes/inventory-truth/Cargo.toml");
const environment = { ...process.env, CARGO_REGISTRIES_CRATES_IO_PROTOCOL: "git" };
const build = spawnSync("cargo", ["build", "--quiet", "--manifest-path", manifest], { cwd: repo, env: environment, stdio: "inherit" });
if (build.status !== 0) process.exit(build.status ?? 1);
const binary = resolve(repo, "backend/prototypes/inventory-truth/target/debug/inventory-truth-preview");
const directory = join(await mkdtemp(join(tmpdir(), "openestates-inventory-")), "snapshot");
const materialize = spawnSync(binary, ["materialize", resolve(repo, "backend/prototypes/inventory-truth/fixtures/scenarios.json"), resolve(repo, "app/config/dag/inventory_preview_policy.json"), directory], { stdio: "inherit" });
if (materialize.status !== 0) process.exit(materialize.status ?? 1);
const server = spawn(binary, ["serve", directory, "4019"], { stdio: "inherit" });
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.kill(signal));
server.on("exit", code => process.exit(code ?? 0));
// Keep the immutable temporary snapshot for inspection after the preview exits.
