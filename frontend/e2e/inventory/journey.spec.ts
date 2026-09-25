import { expect, test } from "@playwright/test";

test("search → evidence → receipt → saved home preserves identity, context and scroll", async ({ page }, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/?scenario=four-ads");
  const tile = page.locator(".inventory-tile");
  await expect(tile).toHaveCount(1);
  await expect(tile).toContainText("4 advertisements");
  await tile.getByRole("link").click();
  await expect(page.getByRole("heading", { name: "Prestige Waterford", exact: true })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Homes from your search" })).toContainText("3 BHK in Whitefield near Manipal Hospital");
  await expect(page.locator(".inventory-price__amount")).toHaveText("₹2.58–2.72 Cr");
  await page.screenshot({ path: info.outputPath("desktop-rest.png") });
  const trigger = page.getByRole("button", { name: /4 advertisements/ });
  const origin = await page.evaluate(() => ({ url: location.href, scroll: scrollY }));
  await trigger.focus(); await trigger.press("Enter");
  const layer = page.getByRole("region", { name: "Asking price details" });
  await expect(layer).toBeVisible();
  await expect(layer.getByRole("button", { name: "Close asking price details" })).toBeFocused();
  await expect(layer.locator(".inventory-ad")).toHaveCount(4);
  await expect(layer.locator(".inventory-history")).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("desktop-evidence.png") });
  await layer.locator("summary").filter({ hasText: "NoBroker" }).click();
  await expect(layer.getByText("₹2,58,00,000", { exact: true })).toBeVisible();
  await layer.locator('.inventory-ad[open]').getByText("Receipt reference", { exact: true }).click();
  await expect(layer.getByText("four-owner", { exact: true }).first()).toBeVisible();
  await page.screenshot({ path: info.outputPath("desktop-receipt.png") });
  await page.keyboard.press("Escape");
  await expect(layer).not.toBeVisible(); await expect(trigger).toBeFocused();
  expect(await page.evaluate(() => ({ url: location.href, scroll: scrollY }))).toEqual(origin);
  await page.getByRole("button", { name: "Save home", exact: true }).click();
  await page.getByRole("link", { name: "Saved homes 1", exact: true }).click();
  await expect(page.locator(".inventory-tile")).toHaveCount(1);
  await page.locator(".inventory-tile").getByRole("link").click();
  await expect(page.locator(".inventory-price__amount")).toHaveText("₹2.58–2.72 Cr");
  await expect(page.getByRole("button", { name: "Saved", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("link", { name: "← Results", exact: true }).click();
  await expect(page).toHaveURL(/scenario=four-ads/);
  expect(errors).toEqual([]);
});

test("all fixture states retain negative evidence and separate comparison types", async ({ page }, info) => {
  for (const [id, clue] of [
    ["single", "View advertisement"], ["uncertain", "Possibly the same home"],
    ["one-active", "Only 1 of 4"], ["withdrawn", "No advertisement confirmed active"],
    ["reduction", "₹7 L lower"], ["owner-broker", "2 advertisements"],
    ["comparables", "View advertisement"], ["sparse", "No advertisement confirmed active"],
    ["long-source", "View advertisement"],
  ]) {
    await page.goto(`/property/${id}?context=inventory-preview&qf=q148`);
    const trigger = page.locator(".inventory-price__clue");
    await expect(trigger).toContainText(clue);
    if (id === "uncertain") {
      await expect(page.locator(".inventory-price__amount")).toHaveText("₹2.58 Cr");
      await expect(page.locator(".inventory-price__caveat")).toContainText("16th floor");
    }
    if (["withdrawn", "sparse"].includes(id)) await expect(page.locator(".inventory-price__amount")).toHaveCount(0);
    await trigger.click();
    const layer = page.getByRole("region", { name: "Asking price details" });
    if (id === "reduction") {
      await expect(layer.getByRole("region", { name: "Price history for this advertisement" })).toContainText("₹2.65 Cr");
      await page.screenshot({ path: info.outputPath("price-change.png") });
    }
    if (id === "one-active") {
      await expect(layer).toContainText("No longer listed"); await expect(layer).toContainText("Not reconfirmed");
    }
    if (id === "comparables") {
      await layer.locator(".inventory-more > summary").filter({ hasText: "Other homes in this society" }).click();
      await expect(layer).toContainText("Floor 18");
      await layer.locator(".inventory-more > summary").filter({ hasText: "Registered sales" }).click();
      await expect(layer).toContainText("₹2.43 Cr");
      await expect(layer).not.toContainText("₹1.9 Cr");
    }
    if (id === "sparse") {
      await expect(layer.locator(".inventory-more")).toHaveCount(0);
      await expect(layer).not.toContainText("₹0");
    }
    await page.screenshot({ path: info.outputPath(`${id}.png`) });
    await page.keyboard.press("Escape"); await expect(trigger).toBeFocused();
  }
});

test("mobile, touch, reduced motion, long labels and outside dismissal", async ({ page }, info) => {
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto("/property/four-ads?context=inventory-preview&qf=q148");
    const trigger = page.locator(".inventory-price__clue");
    await expect(trigger).toBeVisible();
    await page.screenshot({ path: info.outputPath(`mobile-${width}-rest.png`) });
    const scroll = await page.evaluate(() => scrollY);
    await trigger.tap();
    const layer = page.getByRole("region", { name: "Asking price details" });
    await expect(layer).toBeVisible();
    const rect = await layer.boundingBox();
    expect(rect!.x).toBeGreaterThanOrEqual(0); expect(rect!.x + rect!.width).toBeLessThanOrEqual(width);
    expect(rect!.y + rect!.height).toBeLessThanOrEqual(844);
    expect(await layer.evaluate(el => getComputedStyle(el).animationName)).toBe("none");
    await layer.locator("summary").filter({ hasText: "NoBroker" }).click();
    await expect(layer.getByText("₹2,58,00,000", { exact: true })).toBeVisible();
    await page.screenshot({ path: info.outputPath(`mobile-${width}-receipt.png`) });
    await layer.getByRole("button", { name: "Close asking price details" }).click();
    await expect(trigger).toBeFocused(); expect(await page.evaluate(() => scrollY)).toBe(scroll);
    await trigger.click(); await page.getByRole("heading", { name: "Prestige Waterford" }).click();
    await expect(layer).not.toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  }
  await page.goto("/property/long-source"); await page.locator(".inventory-price__clue").click();
  const layer = page.getByRole("region", { name: "Asking price details" });
  await expect(layer).toContainText("Whitefield Homeowners’ Cooperative");
  expect(await layer.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath("mobile-long-source.png") });
});

test("failed and mismatched API responses never leak a different price", async ({ page }) => {
  await page.route("**/api/inventory/homes/four-ads?*", route => route.fulfill({ status: 503, body: "Unavailable" }));
  await page.goto("/property/four-ads");
  await expect(page.getByRole("alert")).toContainText("couldn’t be loaded");
  await expect(page.locator(".inventory-price__amount")).toHaveCount(0);
  await page.unroute("**/api/inventory/homes/four-ads?*");
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.locator(".inventory-price__amount")).toHaveText("₹2.58–2.72 Cr");
  await page.route("**/api/inventory/homes/single?*", async route => {
    const response = await route.fetch(); const body = await response.json(); body.snapshot_id = "another-snapshot";
    await route.fulfill({ response, json: body });
  });
  await page.goto("/property/single");
  await expect(page.getByRole("alert")).toContainText("don’t match this home");
  await expect(page.locator(".inventory-price__amount")).toHaveCount(0);
  await page.unroute("**/api/inventory/homes/single?*");
  await page.route("**/api/inventory/homes/single?*", route => route.fulfill({ status: 409, json: { code: "snapshot_mismatch" } }));
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByRole("alert")).toContainText("observations changed");
  await page.unroute("**/api/inventory/homes/single?*");
  const refresh = page.waitForResponse(response => response.url().endsWith("/api/inventory/homes"));
  await page.getByRole("link", { name: "Return to results", exact: true }).click();
  await refresh;
  await page.locator(".inventory-tile").getByRole("link").click();
  await expect(page.locator(".inventory-price__amount")).toHaveText("₹2.58 Cr");
});
