import { defineConfig } from "@playwright/test";

const runId = process.env.ATLAS_RUN_ID
  ?? new Date().toISOString().replace(/[:.]/g, "-");
const baseURL = process.env.ATLAS_BASE_URL ?? "http://127.0.0.1:5173";
const port = new URL(baseURL).port || "5173";

export default defineConfig({
  testDir: "./e2e/atlas",
  outputDir: `./test-results/atlas-parity/${runId}`,
  timeout: 60000,
  workers: 1,
  use: {
    baseURL,
    viewport: { width: 1440, height: 900 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "on",
    channel: "chrome",
  },
  webServer: {
    command: `npm run dev:atlas -- --host 127.0.0.1 --port ${port}`,
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    env: { VITE_USE_FIXTURE_API: "true" },
  },
});
