import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/inventory",
  outputDir: "./test-results/inventory",
  workers: 1,
  timeout: 30_000,
  use: {
    baseURL: "http://127.0.0.1:5191",
    viewport: { width: 1440, height: 1000 },
    reducedMotion: "reduce",
    hasTouch: true,
    screenshot: "only-on-failure",
    launchOptions: {
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
      args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
    },
  },
  webServer: [
    { command: "npm run dev:inventory-api", url: "http://127.0.0.1:4019/api/inventory/homes", reuseExistingServer: !process.env.CI, timeout: 180_000 },
    { command: "npm run dev:inventory-scenarios", url: "http://127.0.0.1:5191", reuseExistingServer: !process.env.CI },
    { command: "npm run dev:inventory", url: "http://127.0.0.1:5192", reuseExistingServer: !process.env.CI },
  ],
});
