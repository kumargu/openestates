import { expect, test } from "@playwright/test";

// Archived API facts; no screenshots, traces, video, or renderer mock.
test("captured facts, reviews and photos remain accessible without Google", async ({ page }) => {
  await page.route("https://maps.googleapis.com/**", route => route.abort());
  await page.goto("/property/discovered-prestige-waterford-3bhk");
  const canvas = page.locator(".property-arrival-map--atlas");
  await expect(canvas.getByRole("button", { name: "Retry map" })).toBeVisible();
  const facts = page.getByRole("region", { name: "Property information", exact: true });
  await facts.locator("summary").filter({ hasText: "Lifecycle" }).click();
  await expect(facts.locator("details[open] dd").first()).not.toBeEmpty();
  await facts.locator("summary").filter({ hasText: "Community pulse" }).click();
  await expect(facts.getByText(/Residents praise/)).toBeVisible();
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

test("missing mapped road is explicit and never creates a synthetic tour", async ({ page }) => {
  // Remove geometry from the existing contract fixture, without inventing a second society.
  await page.route("**/api/properties/*/surfaces/arrival_story", async route => {
    const response = await route.fetch();
    const scene = await response.json();
    delete scene.anchor.boundary;
    scene.features = scene.features.filter((feature: { layerId: string }) =>
      feature.layerId !== "entrance" && feature.layerId !== "approach_road");
    await route.fulfill({ response, json: scene });
  });
  await page.route("https://maps.googleapis.com/**", route => route.abort());
  await page.goto("/property/discovered-prestige-waterford-3bhk");
  const canvas = page.locator(".property-arrival-map--atlas");
  await expect(canvas.getByRole("button", { name: "Site outline", exact: true })).toBeDisabled();
  await canvas.getByRole("button", { name: "Approach road", exact: true }).click();
  await expect(canvas.getByRole("status").filter({ hasText: "Approach road not mapped" })).toBeVisible();
  await expect(canvas.getByLabel("Road tour speed")).toHaveCount(0);
});
