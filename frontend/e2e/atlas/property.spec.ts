import { expect, test } from "@playwright/test";

// Only backend responses are fixtures. This test deliberately requires real
// Google Maps, an authorized key, and a WebGL-capable browser. No fake canvas.
test("property page: society, metro focus, nearby, aerial road, Street View exit", async ({
  page,
}, testInfo) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const arrival = page.locator(".property-arrival-map--atlas");
  await expect(arrival).toBeVisible();
  await expect(
    arrival.locator('[data-map-renderer="google-3d"]'),
  ).toHaveAttribute("aria-busy", "false", { timeout: 30000 });
  const map = arrival.locator("gmp-map-3d");
  await expect(map).toBeVisible();
  await expect(map).toHaveAttribute("data-google-steady", "true", {
    timeout: 30000,
  });
  await arrival.screenshot({ path: testInfo.outputPath("society.png") });
  await arrival.getByRole("button", { name: "Metro", exact: true }).click();
  await expect(
    arrival.getByRole("button", { name: "Show together", exact: true }),
  ).toBeVisible();
  await expect(arrival.locator("gmp-polyline-3d-interactive")).toHaveCount(16);
  await arrival.locator(".property-atlas__place-list button").nth(1).click();
  await expect(
    arrival.locator(".property-atlas__place-list button").nth(1),
  ).toHaveAttribute("aria-pressed", "true");
  await arrival.screenshot({ path: testInfo.outputPath("metro-focus.png") });
  await arrival.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(arrival.locator(".property-atlas__place-list button")).toHaveCount(2);
  await arrival
    .getByRole("button", { name: "Road journey", exact: true })
    .click();
  await expect(arrival.getByLabel("Road tour speed")).toBeVisible({
    timeout: 15000,
  });
  await arrival.getByLabel("Road tour speed").focus();
  await arrival.getByLabel("Road tour speed").press("End");
  await expect(arrival.getByLabel("Road tour speed")).toHaveValue("2");
  await arrival
    .getByRole("button", { name: "Pause road tour", exact: true })
    .click();
  await expect(
    arrival.getByRole("button", { name: "Resume road tour", exact: true }),
  ).toBeVisible();
  const paused = await map.evaluate((el) =>
    JSON.stringify((el as HTMLElement & { center: unknown }).center),
  );
  await page.waitForTimeout(400);
  expect(
    await map.evaluate((el) =>
      JSON.stringify((el as HTMLElement & { center: unknown }).center),
    ),
  ).toBe(paused);
  await arrival
    .getByRole("button", { name: "Resume road tour", exact: true })
    .click();
  await arrival.screenshot({ path: testInfo.outputPath("road.png") });
  await arrival
    .getByRole("button", { name: "Street View", exact: true })
    .click();
  // Exit is reachable immediately, even if panorama coverage fails or is slow.
  await expect(
    arrival.getByRole("button", { name: "Back to aerial", exact: true }),
  ).toBeVisible();
  await arrival
    .getByRole("button", { name: "Back to aerial", exact: true })
    .click();
  await expect(
    arrival.locator('[data-map-renderer="google-3d"]'),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await arrival.screenshot({ path: testInfo.outputPath("mobile-road.png") });
  expect(errors).toEqual([]);
});
