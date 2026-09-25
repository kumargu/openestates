import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/search",
  outputDir: "./test-results/search-journey",
  timeout: 30000,
  grepInvert: /live bundle API/,
  workers: 1,
  projects: [
    { name: "desktop", use: { viewport: { width: 1440, height: 1000 } } },
    { name: "mobile", use: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true } },
  ],
  use: { baseURL: "http://localhost:5174", channel: "chrome", screenshot: "only-on-failure", trace: "retain-on-failure" },
  webServer: {
    command: "npm run dev -- --host localhost --port 5174 --strictPort",
    url: "http://localhost:5174",
    env: { VITE_USE_FIXTURE_API: "false", VITE_API_BASE: process.env.SEARCH_LIVE_API ?? "http://localhost:5174", VITE_VERCEL_ENV: "test" },
  },
});
