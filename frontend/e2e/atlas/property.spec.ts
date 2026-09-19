import { expect, test } from "@playwright/test";

// Only backend responses are fixtures. This test deliberately requires real
// Google Maps, an authorized key, and a WebGL-capable browser. No fake canvas.
test("property page: society, metro focus, nearby, aerial road, Street View exit", async ({
  page,
}) => {
  test.setTimeout(120_000);
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
  await expect(map).toHaveAttribute("data-google-initialized", "true", {
    timeout: 30000,
  });
  await map.evaluate((element) => { element.dataset.canvasInstance = "persistent-map"; });
  await page.emulateMedia({ reducedMotion: "reduce" });
  const initialRange = await map.evaluate((element) => (element as HTMLElement & { range: number }).range);
  await arrival.getByRole("button", { name: "Zoom in", exact: true }).click();
  await expect.poll(() => map.evaluate((element) => (element as HTMLElement & { range: number }).range)).toBeLessThan(initialRange);
  await arrival.getByRole("button", { name: "Top view", exact: true }).click();
  await expect.poll(() => map.evaluate((element) => (element as HTMLElement & { tilt: number }).tilt)).toBe(12);
  await arrival.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(map).toHaveAttribute("data-canvas-instance", "persistent-map");
  await arrival.getByRole("button", { name: "Close panel", exact: true }).click();
  await arrival.getByRole("button", { name: "Home", exact: true }).click();
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await arrival.getByRole("button", { name: "Metro", exact: true }).click();
  await expect(
    arrival.getByRole("button", { name: "Show together", exact: true }),
  ).toBeVisible();
  expect(await arrival.locator("gmp-polyline-3d-interactive").count()).toBeGreaterThan(0);
  await expect(map).toHaveAttribute('data-atlas-marker-count', '4');
  await arrival.locator(".property-atlas__place-list > div > button").nth(1).click();
  await expect(
    arrival.locator(".property-atlas__place-list > div > button").nth(1),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(map).toHaveAttribute('data-atlas-depth', 'inspect');
  await expect(map).toHaveAttribute('data-atlas-marker-count', '4');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(1);
  await expect(map.locator(':scope > gmp-marker-3d-interactive')).toHaveCount(4);
  await expect(map.locator('gmp-marker-3d-interactive[title="Prestige Waterford"]'))
    .not.toHaveAttribute('label', /.+/);
  await expect(map.locator(':scope > gmp-marker-3d-interactive[label]')).toHaveCount(1);
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled');
  const pairDistance = Number(await map.getAttribute('data-atlas-pair-distance'));
  const pairRange = Number(await map.getAttribute('data-atlas-camera-target-range'));
  expect(pairDistance).toBeGreaterThan(0);
  expect(pairRange).toBeGreaterThan(0);
  expect(pairRange).toBeLessThan(Number.POSITIVE_INFINITY);
  await expect(map.locator(':scope > gmp-marker-3d-interactive').first())
    .toHaveAttribute('altitude-mode', 'relative-to-ground');
  await expect(arrival.getByRole('button', {name:'Replay view',exact:true})).toHaveCount(0);
  await arrival.locator('.property-atlas__place-list > div > button[aria-pressed="true"]').click();
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled', {timeout: 10_000});
  await arrival.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(arrival.locator(".property-atlas__place-list > div > button")).toHaveCount(2);
  await expect(map).toHaveAttribute('data-atlas-depth', 'overview');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(0);
  for (const name of await arrival.locator('.property-atlas__place-list strong').allTextContents()) {
    await expect(map.getByLabel(name, {exact: true})).toHaveCount(1);
  }
  await arrival.locator('.property-atlas__place-list > div > button').first().click();
  await expect(map).toHaveAttribute('data-atlas-depth', 'inspect');
  await expect(map).toHaveAttribute('data-atlas-marker-count', '3');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(1);
  await page.waitForTimeout(1300);
  await arrival.getByRole('button', {name: 'Tour schools', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-camera-owner', 'nearby');
  await expect(map).toHaveAttribute('data-atlas-scene', 'tour:nearby:school:overview');
  await expect(map).toHaveAttribute('data-atlas-depth', 'pair', {timeout: 10_000});
  await expect(map).toHaveAttribute('data-atlas-scene', /:pair$/);
  await arrival.getByRole('button', {name: 'Pause tour', exact: true}).click();
  const pausedNearbyCamera = await map.evaluate((element) => JSON.stringify({
    center: (element as HTMLElement & {center: unknown}).center,
    heading: (element as HTMLElement & {heading: number}).heading,
    range: (element as HTMLElement & {range: number}).range,
  }));
  await page.waitForTimeout(400);
  expect(await map.evaluate((element) => JSON.stringify({
    center: (element as HTMLElement & {center: unknown}).center,
    heading: (element as HTMLElement & {heading: number}).heading,
    range: (element as HTMLElement & {range: number}).range,
  }))).toBe(pausedNearbyCamera);
  await arrival.getByRole('button', {name: 'Resume tour', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-depth', 'inspect', {timeout: 10_000});
  await arrival.locator('.property-atlas__place-list > div > button.is-active').click();
  await page.setViewportSize({width: 390, height: 844});
  await page.waitForTimeout(1300);
  await page.setViewportSize({width: 1440, height: 1000});
  await arrival
    .getByRole("button", { name: "Approach road", exact: true })
    .click();
  await expect(arrival.getByLabel("Road tour speed")).toBeVisible({
    timeout: 15000,
  });
  await expect(arrival.getByLabel("Road tour speed")).toHaveValue("2");
  await arrival.getByLabel("Road tour speed").focus();
  await arrival.getByLabel("Road tour speed").press("End");
  await expect(arrival.getByLabel("Road tour speed")).toHaveValue("4");
  await arrival.getByLabel("Road tour speed").press("Home");
  await expect(arrival.getByLabel("Road tour speed")).toHaveValue("0.5");
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
  expect(errors).toEqual([]);
});
