import { defineConfig } from "@playwright/test";

const runId = process.env.ATLAS_RUN_ID
  ?? new Date().toISOString().replace(/[:.]/g, "-");

export default defineConfig({
  testDir: "./e2e/atlas",
  outputDir: `./test-results/atlas-parity/${runId}`,
  timeout: 60000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:5173",
    viewport: { width: 1440, height: 1000 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "on",
    channel: "chrome",
  },
  webServer: {
    command: "npm run dev:atlas -- --host 127.0.0.1 --port 5173",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: !process.env.CI,
    env: { VITE_USE_FIXTURE_API: "true" },
  },
});
