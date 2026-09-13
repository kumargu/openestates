import { expect, test, type Locator, type Page, type TestInfo } from "@playwright/test";

const CAMERA_FIXTURE_URL = "/property/fixture-prestige-waterford-3bhk";

async function waitForAtlasReady(page: Page) {
  const atlas = page.locator(".property-arrival-map--atlas");
  const map = atlas.locator("gmp-map-3d");

  await expect(atlas).toBeVisible();
  await expect(atlas.locator('[data-map-renderer="google-3d"]'))
    .toHaveAttribute("aria-busy", "false", { timeout: 30_000 });
  await expect(map).toHaveAttribute("data-google-initialized", "true", {
    timeout: 30_000,
  });
  await waitForAtlasSteady(atlas, map);

  return { atlas, map };
}

async function waitForAtlasSteady(atlas: Locator, map: Locator) {
  await map.evaluate(async (element) => {
    const mapElement = element as HTMLElement & { steady?: boolean };
    await new Promise<void>((resolve) => {
      let timeout = 0;
      const finish = (force = false) => {
        if (!force && mapElement.steady === false) return;
        window.clearTimeout(timeout);
        mapElement.removeEventListener("gmp-steadychange", onSteadyChange);
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
      };
      const onSteadyChange = () => finish();
      mapElement.addEventListener("gmp-steadychange", onSteadyChange);
      timeout = window.setTimeout(() => finish(true), 5_000);
      finish();
    });
  });
  await atlas.evaluate(async (element) => {
    const animations = element.getAnimations({ subtree: true }).filter((animation) => {
      return animation.effect?.getTiming().iterations !== Infinity;
    });
    await Promise.race([
      Promise.allSettled(animations.map((animation) => animation.finished)),
      new Promise((resolve) => window.setTimeout(resolve, 1_500)),
    ]);
  });
}

async function capture(
  atlas: Locator,
  map: Locator,
  testInfo: TestInfo,
  name: string,
) {
  await waitForAtlasSteady(atlas, map);
  await atlas.screenshot({ path: testInfo.outputPath(`${name}.png`) });
}

test("creates the Astra visual review matrix", async ({ page }, testInfo) => {
  test.setTimeout(180_000);
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.goto(CAMERA_FIXTURE_URL);
  const { atlas, map } = await waitForAtlasReady(page);
  await capture(atlas, map, testInfo, "01-home-desktop");

  await atlas.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-depth", "overview");
  await capture(atlas, map, testInfo, "02-schools-overview-desktop");

  await atlas.locator(".property-atlas__place-list button").first().click();
  await expect(map).toHaveAttribute("data-atlas-depth", "pair");
  await capture(atlas, map, testInfo, "03-school-with-home-desktop");

  await atlas.getByRole("button", { name: "Look closer", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-depth", "inspect");
  await capture(atlas, map, testInfo, "04-school-close-desktop");

  await atlas.getByRole("button", { name: "Road journey", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-scene", "road:flight", {
    timeout: 30_000,
  });
  await capture(atlas, map, testInfo, "05-road-journey-desktop");

  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(CAMERA_FIXTURE_URL);
  const mobile = await waitForAtlasReady(page);
  await capture(mobile.atlas, mobile.map, testInfo, "06-home-mobile");
  await mobile.atlas.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(mobile.map).toHaveAttribute("data-atlas-depth", "overview");
  await mobile.atlas.locator(".property-atlas__place-list button").first().click();
  await expect(mobile.map).toHaveAttribute("data-atlas-depth", "pair");
  await capture(mobile.atlas, mobile.map, testInfo, "07-school-with-home-mobile");

  await testInfo.attach("browser-console-errors", {
    body: Buffer.from(JSON.stringify(consoleErrors, null, 2)),
    contentType: "application/json",
  });
  expect(pageErrors).toEqual([]);
});
