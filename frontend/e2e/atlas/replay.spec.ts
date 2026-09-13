import { expect, test } from "@playwright/test";

test("loads the captured backend replay without the Rust service", async ({ page }) => {
  const failedBackendRequests: string[] = [];
  page.on("requestfailed", (request) => {
    if (request.url().includes("/api/")) failedBackendRequests.push(request.url());
  });

  await page.goto("/property/discovered-prestige-waterford-3bhk");
  await expect(page.getByRole("heading", { name: /3 BHK in PRESTIGE WATERFORD/i }))
    .toBeVisible();
  const atlas = page.locator(".property-arrival-map--atlas");
  await expect(atlas).toBeVisible();
  await expect(atlas.locator("gmp-map-3d")).toHaveAttribute(
    "data-google-initialized",
    "true",
    { timeout: 30_000 },
  );
  expect(failedBackendRequests).toEqual([]);
});
