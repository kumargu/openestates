import { expect, test } from "@playwright/test";

// Layout/navigation checks use the existing dev API fixtures. They do not
// certify Google rendering; property.spec.ts and parity.spec.ts require it.
for (const viewport of [{ width: 1440, height: 1000 }, { width: 390, height: 844 }, { width: 320, height: 568 }, { width: 844, height: 390 }]) {
  test(`canvas panels remain usable at ${viewport.width} × ${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto("/property/fixture-prestige-waterford-3bhk");
    const canvas = page.locator(".property-arrival-map--atlas");
    await expect(canvas.getByRole("heading", { level: 1 })).toHaveText("Prestige Waterford");
    await expect(page.locator(".workspace-sidebar")).toBeVisible();
    await expect(page.locator("#property-atlas-panel")).toHaveCount(0);

    await canvas.getByRole("button", { name: "Schools", exact: true }).click();
    const panel = page.getByRole("complementary", { name: "Nearby places" });
    await expect(panel).toBeFocused();
    const places = panel.locator(".property-atlas__place-list > div > button");
    await expect(places).toHaveCount(2);
    await places.first().click();
    await expect(places.first()).toHaveAttribute("aria-pressed", "true");
    await panel.getByRole("button", { name: "Replay view", exact: true }).scrollIntoViewIfNeeded();
    await expect(panel.getByRole("button", { name: "Replay view", exact: true })).toBeVisible();
    await expect(places).toHaveCount(2); // Focusing one place does not delete the rest.
    await expect(panel.getByRole("button", { name: "Close panel" })).toBeInViewport({ ratio: 1 });

    const panelBox = await panel.boundingBox();
    const sceneBox = await canvas.boundingBox();
    expect(panelBox).not.toBeNull();
    expect(sceneBox).not.toBeNull();
    expect(panelBox!.x).toBeGreaterThanOrEqual(sceneBox!.x);
    expect(panelBox!.x + panelBox!.width).toBeLessThanOrEqual(sceneBox!.x + sceneBox!.width);
    expect(panelBox!.y + panelBox!.height).toBeLessThanOrEqual(sceneBox!.y + sceneBox!.height);
    await page.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
    await expect(canvas.getByRole("button", { name: "Schools", exact: true })).toBeFocused();

    await canvas.getByRole("button", { name: "Home", exact: true }).click();
    await expect(canvas.getByRole("button", { name: "Home details", exact: true })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(errors).toEqual([]);
  });
}

test("unavailable Google map keeps nearby places accessible", async ({ page }) => {
  await page.route("https://maps.googleapis.com/**", (route) => route.abort());
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const canvas = page.locator(".property-arrival-map--atlas");
  await expect(canvas.getByRole("button", { name: "Retry map" })).toBeVisible();
  await expect(canvas.getByRole("button", { name: "Zoom in" })).toBeDisabled();
  await canvas.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(canvas.locator(".property-atlas__place-list strong")).toHaveCount(2);
  await canvas.locator(".property-atlas__place-list > div > button").first().click();
  await expect(canvas.getByRole("button", { name: "Replay view", exact: true })).toBeDisabled();
  await canvas.getByRole("button", { name: "Close panel" }).click();
  await expect(canvas.getByRole("button", { name: "Schools", exact: true })).toBeFocused();
});
