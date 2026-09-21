import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/contracts",
  outputDir: "./test-results/contracts",
  timeout: 45000,
  workers: 1,
  projects: [
    { name: "desktop", use: { viewport: { width: 1440, height: 1000 } } },
    { name: "mobile", use: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true } },
  ],
  use: { baseURL: "http://127.0.0.1:5176", channel: "chrome", trace: "retain-on-failure" },
  webServer: [
    {
      command: "cargo test --manifest-path ../backend/Cargo.toml --config 'profile.test.package.backend.opt-level=1' --test project_enrichment_vertical_contract -- --nocapture",
      url: "http://127.0.0.1:4016/api/health",
      env: { OPENESTATES_ALLOWED_ORIGINS: "http://127.0.0.1:5176", OPENESTATES_CONTRACT_SERVER_ADDR: "127.0.0.1:4016" },
      timeout: 240000,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: "npm run dev -- --host 127.0.0.1 --port 5176 --strictPort",
      url: "http://127.0.0.1:5176",
      env: { VITE_USE_FIXTURE_API: "false", VITE_API_BASE: "http://127.0.0.1:4016", VITE_VERCEL_ENV: "test" },
      reuseExistingServer: !process.env.CI,
    },
  ],
});
