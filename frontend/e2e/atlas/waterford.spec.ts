import { expect, test } from "@playwright/test";

// Archived renderer fixture. Source admission and receipt completeness use the materialized API journey.
test("captured facts, reviews and photos remain accessible without Google", async ({ page }, testInfo) => {
  await page.route("https://maps.googleapis.com/**", route => route.abort());
  await page.goto("/property/discovered-prestige-waterford-3bhk");
  const canvas = page.locator(".property-arrival-map--atlas");
  await expect(canvas.getByRole("button", { name: "Retry map" })).toBeVisible();
  const facts = page.getByRole("region", { name: "Property information", exact: true });
  await expect(facts.getByRole("heading", { name: "Market trail", exact: true })).toBeVisible();
  await expect(facts.getByRole("link", { name: "Official record", exact: true }))
    .toHaveAttribute("href", "/property/discovered-prestige-waterford-3bhk/rera");
  await expect(facts.locator("summary")).toHaveCount(0);
  await facts.screenshot({ path: testInfo.outputPath("market-trail.png") });
  const recordLinks = await facts.getByRole("navigation", { name: "Property reports" })
    .locator("a").evaluateAll(links => links.map(link => link.getAttribute("href")));
  expect(recordLinks.length).toBeGreaterThan(0);
  expect(new Set(recordLinks).size).toBe(recordLinks.length);

  await canvas.getByRole("button", { name: "Reviews", exact: true }).click();
  const reviews = page.locator("#resident-voice");
  await expect(reviews).toBeFocused();
  const review = reviews.locator("details").first();
  await review.locator("summary").click();
  await expect(review).toHaveAttribute("open", "");
  await expect(review.locator(".property-review-card__full")).toBeVisible();
  await reviews.getByRole("button", { name: "Back to map", exact: true }).click();
  await expect(canvas).toBeFocused();

  await canvas.getByRole("button", { name: "Photos", exact: true }).click();
  const photos = page.getByRole("complementary", { name: "Photos", exact: true });
  await expect(photos.getByText("1 / 5")).toBeVisible();
  await photos.getByRole("button", { name: "Next photo" }).click();
  await expect(photos.getByText("2 / 5")).toBeVisible();
  await photos.getByRole("button", { name: "Photo 5", exact: true }).click();
  await expect(photos.getByText("5 / 5")).toBeVisible();
  await expect(page.locator(".workspace-sidebar")).toBeVisible();
  await photos.getByRole("button", { name: "Close panel" }).click();
  await expect(canvas.getByRole("button", { name: "Photos", exact: true })).toBeFocused();
});

test("missing entrance and road never creates an arrival surface", async ({ page }) => {
  // Remove geometry from the existing contract fixture, without inventing a second society.
  await page.route("**/api/properties/*/context", async route => {
    const response = await route.fetch();
    const scene = await response.json();
    scene.anchor.geometry = null;
    scene.anchor.geometrySource = null;
    scene.features = scene.features.filter((feature: { fact: { factKey: string } }) =>
      feature.fact.factKey !== "society.entrance_entity" && feature.fact.factKey !== "approach_road");
    await route.fulfill({ response, json: scene });
  });
  await page.route("https://maps.googleapis.com/**", route => route.abort());
  await page.goto("/property/discovered-prestige-waterford-3bhk");
  await expect(page.getByRole("region", { name: "The way in." })).toHaveCount(0);
});

test('Street View keeps the complete home arrival regardless of aerial progress', async ({page}) => {
  test.setTimeout(120_000);
  await page.goto('/property/discovered-prestige-waterford-3bhk');
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await map.evaluate(element => { element.dataset.streetRestoreInstance = 'same-aerial'; });
  // Observe real Google lookup requests; keep its service and panoramas intact.
  await page.evaluate(async () => {
    type Request = {location: {lat: number; lng: number}};
    const browser = window as unknown as {
      google: {maps: {importLibrary(name: string): Promise<{StreetViewService: {prototype: {getPanorama(request: Request): Promise<unknown>}}}>}};
      streetLookups: Request['location'][];
    };
    browser.streetLookups = [];
    const library = await browser.google.maps.importLibrary('streetView');
    const original = library.StreetViewService.prototype.getPanorama;
    library.StreetViewService.prototype.getPanorama = function(request: Request) {
      browser.streetLookups.push({...request.location});
      return original.call(this, request);
    };
  });
  const lookups = () => page.evaluate(() => (window as unknown as {streetLookups: {lat: number; lng: number}[]}).streetLookups);
  await page.getByRole('button', {name: 'Approach road', exact: true}).click();
  await page.getByRole('button', {name: 'Pause road tour', exact: true}).click();
  await page.getByRole('button', {name: 'Street View', exact: true}).click();
  await expect(page.locator('[data-map-renderer="google-street-view"]')).toBeVisible({timeout: 30_000});
  const opening = await lookups();
  expect(opening.length).toBeGreaterThan(2);
  await page.getByRole('button', {name: 'Pause road tour', exact: true}).click();
  await page.getByRole('button', {name: 'Back to aerial', exact: true}).click();
  await expect(map).toHaveAttribute('data-street-restore-instance', 'same-aerial');
  await page.getByRole('button', {name: 'Replay road tour', exact: true}).click();
  await page.getByLabel('Road tour speed').focus();
  await page.getByLabel('Road tour speed').press('End');
  await expect.poll(() => map.getAttribute('data-atlas-road-distance').then(Number), {timeout: 30_000}).toBeGreaterThan(150);
  await page.getByRole('button', {name: 'Pause road tour', exact: true}).click();
  await map.dispatchEvent('pointerdown'); // Manual aerial interruption must not disable an explicit Street View request.
  await page.evaluate(() => { (window as unknown as {streetLookups: unknown[]}).streetLookups = []; });
  await page.getByRole('button', {name: 'Street View', exact: true}).click();
  await expect.poll(async () => (await lookups()).length).toBeGreaterThan(0);
  expect(await lookups()).toEqual(opening);
  await expect(page.locator('[data-map-renderer="google-street-view"]')).toBeVisible({timeout: 30_000});
  await expect(page.getByRole('button', {name: 'Pause road tour', exact: true})).toBeVisible();
  await page.getByRole('button', {name: 'Back to aerial', exact: true}).click();
  await expect(map).toHaveAttribute('data-street-restore-instance', 'same-aerial');
});
