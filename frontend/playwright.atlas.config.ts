import { defineConfig } from "@playwright/test";

const runId = process.env.ATLAS_RUN_ID
  ?? new Date().toISOString().replace(/[:.]/g, "-");
const port = process.env.ATLAS_PORT ?? "5173";
const baseURL = `http://127.0.0.1:${port}`;
const fixtureMode = process.env.ATLAS_FIXTURE_MODE === "waterford" ? "waterford" : "atlas";

export default defineConfig({
  testDir: "./e2e/atlas",
  testMatch: fixtureMode === "waterford" ? /(?:waterford|immersive)\.spec\.ts/ : /(?:canvas|property|parity)\.spec\.ts/,
  outputDir: `./test-results/atlas-parity/${runId}`,
  timeout: 60000,
  workers: 1,
  use: {
    baseURL,
    viewport: { width: 1440, height: 1000 },
    screenshot: "off",
    trace: "off",
    video: "off",
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
      args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
    },
  },
  webServer: {
    command: `npm run dev:${fixtureMode} -- --host 127.0.0.1 --port ${port}`,
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    env: { VITE_USE_FIXTURE_API: fixtureMode === "atlas" ? "true" : "false" },
  },
});
