import { expect, test, type Page } from "@playwright/test";

const property = "http://127.0.0.1:5192/property/discovered-prestige-waterford-3bhk";
const drawer = (page: Page) => page.locator("#property-atlas-panel");
const price = (page: Page) => page.locator(".property-atlas__identity-detail > button");

test("real property page shares one panel across price, photos, places and notes", async ({ page }, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${property}?scenario=four-ads`);
  await expect(price(page)).toHaveText("Compare 4 asking prices↗");
  await expect(page.locator(".workspace-sidebar")).toBeVisible();
  await expect(page.locator(".property-atlas__identity")).toContainText("Floor 14");
  await expect(page.locator(".property-atlas__identity")).not.toContainText("₹3.45");
  await page.screenshot({ path: info.outputPath("integrated-desktop-rest.png") });
  const before = await page.evaluate(() => ({ url: location.href, y: scrollY }));
  await price(page).focus(); await price(page).press("Enter");
  await expect(drawer(page)).toBeFocused();
  await expect(drawer(page).locator(".inventory-ad")).toHaveCount(4);
  await page.screenshot({ path: info.outputPath("integrated-desktop-prices.png") });
  await drawer(page).locator("summary").filter({ hasText: "NoBroker" }).click();
  await expect(drawer(page).getByText("₹2,58,00,000", { exact: true })).toBeVisible();
  await page.screenshot({ path: info.outputPath("integrated-desktop-receipt.png") });
  await page.keyboard.press("Escape");
  await expect(price(page)).toBeFocused();
  expect(await page.evaluate(() => ({ url: location.href, y: scrollY }))).toEqual(before);

  await price(page).click();
  await page.getByRole("button", { name: "Photos", exact: true }).click();
  await expect(drawer(page)).toHaveAttribute("aria-label", "Photos");
  await expect(page.locator(".inventory-ad")).toHaveCount(0);
  await page.getByRole("button", { name: "Next photo" }).click();
  await expect(drawer(page)).toContainText("2 / 5");
  await page.screenshot({ path: info.outputPath("integrated-desktop-photos.png") });

  await page.getByRole("button", { name: "Schools", exact: true }).click();
  await expect(drawer(page)).toHaveAttribute("aria-label", "Nearby places");
  await expect(page.locator("#property-atlas-panel")).toHaveCount(1);
  await price(page).click();
  await page.getByRole("button", { name: "Add note", exact: true }).click();
  await expect(drawer(page)).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Note", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Add note", exact: true })).toBeFocused();
  await page.getByRole("button", { name: "Save for later", exact: true }).click();
  await expect(page.getByRole("button", { name: "Remove from shortlist", exact: true })).toHaveAttribute("aria-pressed", "true");

  await price(page).click();
  await page.getByRole("button", { name: "Reviews", exact: true }).click();
  await expect(drawer(page)).toHaveCount(0);
  await expect(page.locator("#resident-voice")).toBeFocused();
  await page.getByRole("button", { name: "Back to map", exact: true }).click();
  await expect(page.locator("#property-atlas")).toBeFocused();
  expect(errors).toEqual([]);
});

test("mobile receipts fit above navigation with independent scroll and focus return", async ({ page }, info) => {
  for (const [width, height] of [[390, 844], [320, 667], [768, 1024]]) {
    await page.setViewportSize({ width, height });
    await page.goto(property);
    await expect(price(page)).toBeVisible();
    const identity = await page.locator(".property-atlas__identity").boundingBox();
    const actions = await page.locator(".property-atlas__actions").boundingBox();
    const dock = await page.locator(".property-atlas__dock").boundingBox();
    expect(actions!.y).toBeGreaterThanOrEqual(identity!.y + identity!.height);
    expect(dock!.y).toBeGreaterThanOrEqual(actions!.y + actions!.height);
    await page.screenshot({ path: info.outputPath(`integrated-${width}-rest.png`) });
    const origin = await page.evaluate(() => scrollY);
    await price(page).tap();
    await expect(drawer(page)).toBeVisible();
    const rect = await drawer(page).boundingBox();
    expect(rect!.x).toBeGreaterThanOrEqual(0);
    expect(rect!.x + rect!.width).toBeLessThanOrEqual(width);
    expect(rect!.y + rect!.height).toBeLessThanOrEqual(height - 69);
    expect(rect!.height).toBeGreaterThan(200);
    expect(await drawer(page).evaluate(el => getComputedStyle(el).animationName)).toBe("none");
    await page.screenshot({ path: info.outputPath(`integrated-${width}-prices.png`) });
    await drawer(page).locator("summary").filter({ hasText: "NoBroker" }).click();
    await expect(drawer(page).getByText("₹2,58,00,000", { exact: true })).toBeVisible();
    expect(await page.evaluate(() => scrollY)).toBe(origin);
    await page.getByRole("button", { name: "Close panel", exact: true }).click();
    await expect(price(page)).toBeFocused();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
    await page.getByRole("button", { name: "Photos", exact: true }).tap();
    await expect(drawer(page)).toHaveAttribute("aria-label", "Photos");
    await page.getByRole("button", { name: "Next photo" }).click();
    await expect(drawer(page)).toContainText("2 / 5");
    await page.screenshot({ path: info.outputPath(`integrated-${width}-photos.png`) });
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Photos", exact: true })).toBeFocused();
  }
});

test("integrated scenario switching retains exclusions, same-ad history and missing asks", async ({ page }, info) => {
  await page.goto(property);
  await expect(price(page)).toBeVisible();
  for (const scenario of ["uncertain", "reduction", "withdrawn", "long-source", "comparables", "one-active", "single", "sparse", "owner-broker"]) {
    await page.getByRole("combobox", { name: "Scenario" }).selectOption(scenario);
    await expect(page).toHaveURL(new RegExp(`scenario=${scenario}`));
    await expect(price(page)).toBeVisible();
    if (scenario === "reduction") await expect(price(page)).toContainText("₹7 L lower");
    if (scenario === "one-active") await expect(price(page)).toContainText("1 of 4 advertisements active");
    await price(page).click();
    if (scenario === "uncertain") {
      await expect(page.locator(".inventory-integrated__ask")).toContainText("₹2.58 Cr");
      await expect(drawer(page)).toContainText("Excluded from this home’s asking price");
      await expect(drawer(page)).toContainText("16th floor");
      await page.screenshot({ path: info.outputPath("integrated-conflict.png") });
    }
    if (scenario === "reduction") await expect(drawer(page).locator(".inventory-history")).toContainText("₹2.65 Cr");
    if (["withdrawn", "sparse"].includes(scenario)) await expect(page.locator(".inventory-integrated__ask strong")).toHaveCount(0);
    if (scenario === "long-source") expect(await drawer(page).evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
    if (scenario === "one-active") await expect(drawer(page)).toContainText("No longer listed");
    if (scenario === "comparables") {
      await drawer(page).locator(".inventory-more > summary").filter({ hasText: "Registered sales" }).click();
      await expect(drawer(page)).toContainText("₹2.43 Cr");
    }
    await page.keyboard.press("Escape");
  }
});

test("integrated API recovery refreshes the snapshot without showing a legacy price", async ({ page }) => {
  await page.route("**/api/inventory/homes/four-ads?*", route => route.fulfill({ status: 409, json: { code: "snapshot_mismatch" } }));
  await page.goto(property);
  await expect(page.getByRole("alert")).toContainText("observations changed");
  await expect(page.locator(".inventory-integrated__ask")).toHaveCount(0);
  await page.unroute("**/api/inventory/homes/four-ads?*");
  const catalog = page.waitForResponse(response => response.url().endsWith("/api/inventory/homes"));
  await page.getByRole("button", { name: "Try again" }).click();
  await catalog;
  await expect(price(page)).toBeVisible();
  await expect(page.locator(".inventory-integrated__ask")).toContainText("₹2.58–2.72 Cr");
});

test("without an atlas context the photographic page keeps price beside identity", async ({ page }) => {
  await page.route(/\/api\/properties\/[^/]+\/context(?:\?|$)/, route => route.fulfill({ status: 503, body: "Unavailable" }));
  await page.goto(property);
  const priceSection = page.getByRole("region", { name: "Asking price", exact: true });
  await expect(priceSection).toContainText("₹2.58–2.72 Cr");
  await expect(page.locator("#property-atlas")).toHaveCount(0);
  const section = await priceSection.boundingBox();
  const media = await page.locator(".property-filmstrip").first().boundingBox();
  expect(section!.y).toBeLessThan(media!.y);
  await priceSection.locator("details > summary").first().click();
  await expect(priceSection.locator(".inventory-ad")).toHaveCount(4);
});
