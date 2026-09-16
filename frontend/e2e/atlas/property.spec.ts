import { expect, test } from "@playwright/test";

// Backend fixture only: all map geometry/cameras use real Google 3D.
test("property explorer: named evidence, close inspection, stable tours and fullscreen", async ({ page }, testInfo) => {
  test.setTimeout(180000);
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const explorer = page.locator(".property-arrival-map--atlas");
  const map = explorer.locator("gmp-map-3d");
  await expect(map).toHaveAttribute("data-google-initialized", "true", { timeout: 30000 });
  await expect(explorer).toHaveAttribute("data-nearby-mode", "rest");
  await expect(explorer.getByRole("button", { name: "2 km", exact: true })).toBeDisabled();
  await explorer.screenshot({ path: testInfo.outputPath("home.png") });

  await explorer.getByRole("button", { name: "Schools", exact: true }).click();
  const list = explorer.locator(".property-atlas__place-list");
  await expect(list.getByRole("button")).toHaveCount(2);
  await list.getByRole("button").first().click();
  const card = explorer.getByLabel("Selected place", { exact: true });
  await expect(card).toBeFocused();
  await expect(map).toHaveAttribute("data-atlas-depth", "pair");
  await expect(map).toHaveAttribute("data-atlas-marker-count", "2");
  await expect(card.getByRole("link", { name: /Directions/ })).toBeVisible();
  await expect(card.getByRole("button", { name: "Add note" })).toBeVisible();
  await card.getByRole("button", { name: "Look closer" }).click();
  await expect(map).toHaveAttribute("data-atlas-depth", "inspect");
  await expect.poll(async () => Number(await map.getAttribute("data-atlas-camera-target-range")))
    .toBeGreaterThan(0);
  await explorer.screenshot({ path: testInfo.outputPath("school-inspection.png") });
  await card.getByRole("button", { name: "Show with home" }).click();
  await expect(map).toHaveAttribute("data-atlas-depth", "pair");

  const instance = await map.elementHandle();
  await explorer.getByRole("button", { name: /Explore in 3D/ }).click();
  await expect(explorer).toHaveAttribute("aria-modal", "true");
  expect(await explorer.evaluate(element => element.matches(":modal"))).toBe(true);
  expect(await map.evaluate((element, previous) => element === previous, instance)).toBe(true);
  await expect(explorer.getByRole("button", { name: "Schools", exact: true })).toBeVisible();
  await explorer.screenshot({ path: testInfo.outputPath("fullscreen.png") });
  await explorer.getByRole("button", { name: "Close explorer" }).press("Escape");
  await expect(explorer).not.toHaveAttribute("aria-modal", "true");
  await expect(explorer.getByRole("button", { name: /Explore in 3D/ })).toBeFocused();

  await explorer.getByRole("button", { name: "Start tour", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-scene", /:overview$/);
  await expect(map).toHaveAttribute("data-atlas-depth", "pair", { timeout: 10000 });
  await explorer.getByRole("button", { name: "Pause tour", exact: true }).click();
  const pausedBounds = await map.boundingBox();
  const camera = () => map.evaluate(element => {
    const camera = element as HTMLElement & { center: unknown; heading: number; range: number };
    return JSON.stringify({ center: camera.center, heading: camera.heading, range: camera.range });
  });
  const paused = await camera();
  await page.waitForTimeout(400);
  expect(await camera()).toBe(paused);
  await explorer.getByRole("button", { name: "Resume tour", exact: true }).click();
  expect(await map.boundingBox()).toEqual(pausedBounds);
  await explorer.getByRole("button", { name: "Next view", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-depth", "inspect", { timeout: 15000 });
  await map.dispatchEvent("pointerdown", { pointerId: 1, clientX: 120, clientY: 120, buttons: 1 });
  await map.dispatchEvent("pointermove", { pointerId: 1, clientX: 145, clientY: 130, buttons: 1 });
  await map.dispatchEvent("pointerup", { pointerId: 1, clientX: 145, clientY: 130 });
  await expect(explorer.getByRole("button", { name: "Reset view" })).toBeVisible();
  const manual = await camera();
  await page.waitForTimeout(400);
  expect(await camera()).toBe(manual);
  await explorer.getByRole("button", { name: "Reset view" }).click();
  await card.getByRole("button", { name: "Close selected place" }).click();
  await expect(explorer.getByRole("button", { name: "Schools", exact: true })).toBeFocused();

  await explorer.getByRole("button", { name: "Metro", exact: true }).click();
  await expect(explorer.locator("gmp-polyline-3d-interactive")).toHaveCount(16);
  await page.setViewportSize({ width: 390, height: 844 });
  await list.getByRole("button").first().click();
  await expect(card.getByRole("button", { name: /Look closer|Show with home/ })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await explorer.screenshot({ path: testInfo.outputPath("mobile-selection.png") });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await explorer.getByRole("button", { name: "Top view", exact: true }).click();
  await expect(explorer.getByRole("button", { name: "Top view", exact: true })).toHaveAttribute("aria-pressed", "true");
  expect(errors).toEqual([]);
});

test("approach chapter: road speed, pause and reversible Street View", async ({ page }, testInfo) => {
  test.setTimeout(90000);
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const approach = page.locator(".property-arrival-map--approach");
  await approach.scrollIntoViewIfNeeded();
  await expect(approach.locator("gmp-map-3d")).toHaveAttribute("data-google-initialized", "true", { timeout: 30000 });
  const replay = approach.getByRole("button", { name: "Replay road tour", exact: true });
  if (await replay.isVisible()) await replay.click();
  await expect(approach.getByLabel("Road tour speed")).toBeVisible();
  await approach.getByLabel("Road tour speed").focus();
  await approach.getByLabel("Road tour speed").press("End");
  await expect(approach.getByLabel("Road tour speed")).toHaveValue("2");
  await approach.getByRole("button", { name: "Pause road tour", exact: true }).click();
  await expect(approach.getByRole("button", { name: "Resume road tour", exact: true })).toBeVisible();
  await approach.screenshot({ path: testInfo.outputPath("road.png") });
  await approach.getByRole("button", { name: "Street View", exact: true }).click();
  await expect(approach.getByRole("button", { name: "Back to aerial", exact: true })).toBeVisible();
  await approach.getByRole("button", { name: "Back to aerial", exact: true }).click();
  await expect(approach.getByRole("button", { name: "Street View", exact: true })).toBeVisible();
});
