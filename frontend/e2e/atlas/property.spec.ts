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
  await expect(map).toHaveAttribute("data-google-initialized", "true", {
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
  await expect(map).toHaveAttribute('data-atlas-depth', 'pair');
  await expect(map).toHaveAttribute('data-atlas-marker-count', '2');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(1);
  await expect(map.locator(':scope > gmp-marker-3d-interactive')).toHaveCount(2);
  await expect(map.locator('gmp-marker-3d-interactive[title="Prestige Waterford"]'))
    .not.toHaveAttribute('label', /.+/);
  await expect(map.locator(':scope > gmp-marker-3d-interactive[label]')).toHaveCount(0);
  const pairDistance = Number(await map.getAttribute('data-atlas-pair-distance'));
  const pairRange = Number(await map.getAttribute('data-atlas-camera-target-range'));
  expect(pairDistance).toBeGreaterThan(0);
  expect(pairRange).toBeGreaterThanOrEqual(950);
  expect(pairRange).toBeLessThan(Number.POSITIVE_INFINITY);
  await expect(map.locator(':scope > gmp-marker-3d-interactive').first())
    .toHaveAttribute('altitude-mode', 'relative-to-ground');
  await arrival.getByRole('button', {name:'Look closer',exact:true}).click();
  await expect(map).toHaveAttribute('data-atlas-depth', 'inspect');
  await arrival.getByRole('button', {name:'With home',exact:true}).click();
  await expect(map).toHaveAttribute('data-atlas-depth', 'pair');
  await arrival.screenshot({ path: testInfo.outputPath("metro-focus.png") });
  await arrival.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(arrival.locator(".property-atlas__place-list button")).toHaveCount(2);
  await expect(map).toHaveAttribute('data-atlas-depth', 'overview');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(0);
  for (const name of await arrival.locator('.property-atlas__place-list strong').allTextContents()) {
    await expect(map.getByLabel(name, {exact: true})).toHaveCount(0);
  }
  await arrival.screenshot({path:testInfo.outputPath('schools-together.png')});
  await arrival.locator('.property-atlas__place-list button').first().click();
  await expect(map).toHaveAttribute('data-atlas-depth', 'pair');
  await expect(map).toHaveAttribute('data-atlas-marker-count', '2');
  await expect(map.locator('[data-atlas-relationship="true"]')).toHaveCount(1);
  await page.waitForTimeout(1300);
  await arrival.screenshot({path:testInfo.outputPath('school-with-home.png')});
  await arrival.getByRole('button', {name: 'Tour schools', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-camera-owner', 'nearby');
  await expect(map).toHaveAttribute('data-atlas-scene', 'nearby:school:overview');
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
  await arrival.locator('.property-atlas__place-list button.is-active').click();
  await page.setViewportSize({width: 390, height: 844});
  await page.waitForTimeout(1300);
  await arrival.screenshot({path: testInfo.outputPath('mobile-nearby.png')});
  await page.setViewportSize({width: 1440, height: 1000});
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
