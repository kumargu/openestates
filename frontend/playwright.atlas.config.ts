import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/atlas",
  outputDir: "./test-results/atlas",
  timeout: 60000,
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:5173",
    viewport: { width: 1440, height: 1000 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev:atlas -- --host 127.0.0.1 --port 5173",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: !process.env.CI,
    env: { VITE_USE_FIXTURE_API: "true" },
  },
});
