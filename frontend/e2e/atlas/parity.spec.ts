import { expect, test } from "@playwright/test";
import policy from "../../src/lib/atlasPolicy.ts";

// Bounded numeric checks replace the superseded four-stage film and capture loop.
test("native Home orbit, road progress and nearby tours share one map", async ({ page }) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const atlas = page.locator(".property-arrival-map--atlas");
  const map = atlas.locator("gmp-map-3d");
  await expect(map).toHaveAttribute("data-google-initialized", "true", { timeout: 30_000 });
  await map.evaluate(element => { element.dataset.atlasInstance = "persistent-map"; });
  await expect(map).toHaveAttribute("data-atlas-home-phase", "orbit", { timeout: 15_000 });
  const heading = () => map.evaluate(element => (element as HTMLElement & { heading: number }).heading);
  const initialHeading = await heading();
  await expect.poll(async () => Math.abs(((await heading() - initialHeading + 540) % 360) - 180),
    { timeout: 10_000 }).toBeGreaterThan(1);
  await atlas.getByRole("button", { name: /Pause society/ }).click();
  const pausedHeading = await heading();
  await page.waitForTimeout(500);
  expect(await heading()).toBeCloseTo(pausedHeading, 1);
  await atlas.getByRole("button", { name: /Resume society/ }).click();
  await expect.poll(async () => Math.abs(((await heading() - pausedHeading + 540) % 360) - 180),
    { timeout: 10_000 }).toBeGreaterThan(1);

  await atlas.getByRole("button", { name: "Approach road", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-scene", "road:flight", { timeout: 15_000 });
  await expect(atlas.getByLabel("Road tour speed")).toHaveValue(String(policy.road.defaultRate));
  const distance = () => map.getAttribute("data-atlas-road-distance").then(Number);
  const initialDistance = await distance();
  await expect.poll(distance, { timeout: 4000 }).toBeGreaterThan(initialDistance);
  const advancedDistance = await distance();
  const length = Number(await map.getAttribute("data-atlas-road-length"));
  expect(advancedDistance).toBeLessThanOrEqual(length);

  for (const category of ["Schools", "Metro", "Lakes"]) {
    await atlas.getByRole("button", { name: category, exact: true }).click();
    await expect(map).toHaveAttribute("data-atlas-visibility", "category");
    const drawer = atlas.locator(".property-atlas__drawer");
    const count = await drawer.locator(".property-atlas__place-list > div > button").count();
    await drawer.getByRole("button", { name: new RegExp(`^Tour ${category.toLowerCase()}`) }).click();
    await expect(map).toHaveAttribute("data-atlas-depth", "pair", { timeout: 10_000 });
    await expect(map).toHaveAttribute("data-atlas-marker-count", String(count + 1));
    await expect(map.locator(":scope > gmp-marker-3d-interactive[label]")).toHaveCount(1);
    await expect(map).toHaveAttribute("data-atlas-depth", "inspect", { timeout: 10_000 });
    expect(Number(await map.getAttribute("data-atlas-camera-target-range"))).toBeGreaterThan(0);
    await drawer.getByRole("button", { name: "End tour", exact: true }).click();
    await expect(map).toHaveAttribute("data-atlas-instance", "persistent-map");
  }
  await atlas.getByRole("button", { name: "Home", exact: true }).click();
  await expect(map).toHaveAttribute("data-atlas-home-phase", "orbit", { timeout: 15_000 });
  await expect(map).toHaveAttribute("data-atlas-instance", "persistent-map");
  expect(errors).toEqual([]);
});
