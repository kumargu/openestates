import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { journeyFixture, retainedFixture } from "../../tests/fixtures/search-journey.ts";
import { getFixtureResponse } from "../../src/lib/dev-fixtures.ts";
import { atlasFixtureId } from "../../src/lib/dev-atlas-fixtures.ts";
import journeyPolicy from "../../../app/config/ui/search-journey.json" with { type: "json" };

function openAtlasFromFirstResult(envelope: ReturnType<typeof journeyFixture>) {
  if (envelope.active.results.kind !== "current") return envelope;
  const first = envelope.active.results.resultSets[0]?.results[0];
  if (!first) return envelope;
  const previousId = first.id;
  first.id = atlasFixtureId;
  first.image = "/landing/tiles/03-map-evidence-960.webp";
  envelope.active.results.orderedResultIds = envelope.active.results.orderedResultIds
    .map((id) => id === previousId ? atlasFixtureId : id);
  return envelope;
}

async function mockJourneyApi(page: Page, staleProof = false) {
  const calls: Array<{ path: string; body: Record<string, unknown> }> = [];
  const initial = openAtlasFromFirstResult(journeyFixture());
  const updated = openAtlasFromFirstResult(journeyFixture("revision-2", "3 BHK, under ₹2.5Cr"));
  updated.attempt = { kind: "revision", outcome: "activated" };
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const body = route.request().postDataJSON() ?? {};
    calls.push({ path, body });
    if (path === "/api/search") return route.fulfill({ json: initial });
    if (path === "/api/search/revisions") return route.fulfill({ json: body.utterance === "Only 4BHK" ? retainedFixture(initial) : updated });
    if (path === "/api/search/resume") {
      const envelope = structuredClone(body.parentToken === "signed:revision-2" ? updated : initial);
      envelope.attempt = { kind: "resume", outcome: "resumed" };
      return route.fulfill({ json: envelope });
    }
    if (path === "/api/search/proofs/resolve") {
      if (staleProof) return route.fulfill({ status: 409, json: { code: "stale_proof" } });
      return route.fulfill({ json: {
        propertyId: body.propertyId, factKey: "project_size", value: { type: "Text", data: "Exact receipt value" },
        destination: { surfaceId: "project_facts", kind: "section", targetId: "property-search-match" },
        sourceObservations: [{ observationId: "exact", sourceUrl: "https://example.test/exact-receipt" }],
      } });
    }
    if (path === "/api/properties/surfaces/batch") {
      const propertyIds = Array.isArray(body.propertyIds) ? body.propertyIds as string[] : [];
      return route.fulfill({ json: { contractVersion: 1, items: propertyIds.map((propertyId) => ({
        contractVersion: 1,
        propertyId,
        missing: [],
        scenes: [{ surfaceId: "arrival_story", fillRate: { value: 0.9 } }],
      })) } });
    }
    if (path === "/api/discovery") {
      const discovery = structuredClone(getFixtureResponse(path)) as {
        shelves: Array<{ cards: Array<{ property: { id: string; image?: string } }> }>;
      };
      const featured = discovery.shelves[0]?.cards[0]?.property;
      if (featured) {
        featured.id = atlasFixtureId;
        featured.image = "/landing/tiles/03-map-evidence-960.webp";
      }
      return route.fulfill({ json: discovery });
    }
    if (path === "/api/properties") {
      const catalog = structuredClone(getFixtureResponse(path)) as Array<{ id: string }>;
      catalog[0].id = atlasFixtureId;
      (catalog[0] as { hero_image?: string }).hero_image = "/landing/tiles/03-map-evidence-960.webp";
      return route.fulfill({ json: catalog });
    }
    const fixture = getFixtureResponse(path);
    return fixture ? route.fulfill({ json: fixture }) : route.fulfill({ status: 404, json: { error: "No fixture" } });
  });
  return calls;
}

test("active search keeps discovery and resume keeps bounded local state", async ({ page }) => {
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await expect(page.locator(".landing-featured__results")).toBeVisible();
  await expect(page.getByRole("heading", { name: "More to explore — Other areas", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Best reviewed societies", exact: true })).toHaveCount(0);
  expect(calls.filter((call) => call.path === "/api/properties")).toHaveLength(0);
  const initialBytes = await page.evaluate(() =>
    localStorage.getItem("openestates:search-journeys:v1")?.length ?? 0);
  expect(initialBytes).toBeLessThan(journeyPolicy.persistenceByteLimit);

  await page.reload();
  await expect(page.locator(".landing-featured__results")).toBeVisible();
  const resumedBytes = await page.evaluate(() =>
    localStorage.getItem("openestates:search-journeys:v1")?.length ?? 0);
  expect(resumedBytes - initialBytes).toBeLessThan(10_000);
  expect(calls.filter((call) => call.path === "/api/search/resume")).toHaveLength(1);
  expect(calls.filter((call) => call.path === "/api/properties")).toHaveLength(0);
});

test("landing keeps exact branches ahead of contextual rails and product evidence", async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await mockJourneyApi(page);
  const envelope = openAtlasFromFirstResult(journeyFixture());
  if (envelope.active.results.kind !== "current") throw new Error("fixture must be current");
  const alternative = structuredClone(envelope.active.results.resultSets[0].results[0]);
  alternative.id = "fixture-branch-alternative";
  envelope.active.results.resultSets.push({
    branchId: "branch-2",
    label: "Another way to fit",
    results: [alternative],
  });
  envelope.active.results.orderedResultIds.push(alternative.id);
  await page.route("**/api/search?*", (route) => route.fulfill({ json: envelope }));

  await page.goto("/?q=3BHK");
  const continued = page.getByRole("heading", { name: "Whitefield", exact: true });
  const compare = page.getByRole("heading", { name: "Another way to fit" });
  const contextual = page.getByRole("heading", { name: "More to explore — Other areas", exact: true });
  await expect(continued).toBeVisible();
  await expect(compare).toBeVisible();
  await expect(contextual).toBeVisible();
  await expect(page.locator(".landing-journey-section").first()).toHaveClass(/is-revealed/);
  await page.screenshot({ path: testInfo.outputPath("search-rails-resting.png") });
  const firstResultRail = page.locator(".landing-featured__results .landing-stage__featured").first();
  const firstCard = firstResultRail.locator(".landing-stage__feature-card").first();
  const secondCard = firstResultRail.locator(".landing-stage__feature-card").nth(1);
  const media = firstCard.locator(".catalog-card__media");
  const restingMedia = await media.boundingBox();
  const restingFlexBasis = await firstCard.evaluate((element) => getComputedStyle(element).flexBasis);
  const restingSecondFlexBasis = await secondCard.evaluate((element) => getComputedStyle(element).flexBasis);
  if (testInfo.project.name === "desktop") {
    await firstCard.hover();
  } else {
    await firstCard.locator("a").first().focus();
  }
  await expect(firstCard).toHaveClass(/is-active/);
  await expect.poll(async () => (await media.boundingBox())?.width ?? 0).toBeGreaterThan(restingMedia!.width);
  const activeMedia = await media.boundingBox();
  const activeFlexBasis = await firstCard.evaluate((element) => getComputedStyle(element).flexBasis);
  const activeSecondFlexBasis = await secondCard.evaluate((element) => getComputedStyle(element).flexBasis);
  expect(activeMedia!.y).toBeLessThan(restingMedia!.y);
  // The active card grows visually without moving its neighbours.
  expect(activeFlexBasis).toBe(restingFlexBasis);
  expect(activeSecondFlexBasis).toBe(restingSecondFlexBasis);
  await page.screenshot({ path: testInfo.outputPath("search-rails-interaction.png") });
  const productStory = page.getByRole("region", { name: "About us", exact: true });
  const productRows = productStory.locator(".landing-product-story__row");
  const productMarker = productStory.locator(".landing-product-story__marker");
  await productMarker.scrollIntoViewIfNeeded();
  await expect(productMarker).toHaveClass(/is-revealed/);
  await expect(productMarker).toHaveCSS("opacity", "1");
  await expect(productRows).toHaveCount(2);
  await expect(productRows.nth(1)).toHaveClass(/landing-product-story__row--reverse/);
  await expect(page.getByRole("heading", { name: "See the receipts", exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("rail-product-boundary.png") });
  for (const row of await productRows.all()) {
    await row.scrollIntoViewIfNeeded();
    await expect(row).toHaveClass(/is-revealed/);
    await expect(row).toHaveCSS("opacity", "1");
  }
  await productStory.screenshot({ path: testInfo.outputPath("product-section.png") });
  const catalogBounds = await page.locator(".landing-featured").boundingBox();
  const productRowsBounds = await productStory.locator(".landing-product-story__rows").boundingBox();
  expect(productRowsBounds?.x).toBe(catalogBounds?.x);
  expect(productRowsBounds?.width).toBe(catalogBounds?.width);
  const landingOrder = await page.locator(".landing-stage h2, .landing-product-story").evaluateAll((nodes) => (
    nodes.map((node) => node.classList.contains("landing-product-story")
      ? "product-story"
      : node.textContent?.trim())
  ));
  expect(landingOrder.indexOf("Whitefield")).toBeLessThan(landingOrder.indexOf("Another way to fit"));
  expect(landingOrder.indexOf("Another way to fit")).toBeLessThan(landingOrder.indexOf("More to explore — Other areas"));
  expect(landingOrder.indexOf("More to explore — Other areas")).toBeLessThan(landingOrder.indexOf("product-story"));

  if (testInfo.project.name === "desktop") {
    await page.setViewportSize({ width: 1024, height: 1000 });
    await productStory.screenshot({ path: testInfo.outputPath("product-section-tablet.png") });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.setViewportSize({ width: 1440, height: 1000 });
  }

  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.reload();
  await contextual.scrollIntoViewIfNeeded();
  await expect(contextual).toBeVisible();
  await productStory.scrollIntoViewIfNeeded();
  await expect(productRows.first()).toHaveCSS("opacity", "1");
  await expect(productRows.first()).toHaveCSS("transition-duration", "0s");
  await page.screenshot({ path: testInfo.outputPath("search-rails-reduced-motion.png") });

  if (testInfo.project.name === "mobile") {
    const link = firstCard.locator("a").first();
    await link.tap();
    await expect(page).toHaveURL(/\/property\/.*context=revision-1/);
    await page.goBack();
  }

  expect(errors).toEqual([]);
});

test("search, rejected edit, accepted edit, refresh and Back preserve one journey", async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" })).toBeVisible();
  await expect(page.locator(".landing-featured__results")).toBeVisible();
  await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Only 4BHK");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByText("No homes match that change. Your previous search is unchanged.")).toBeVisible();
  await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
  expect(calls.filter((call) => call.path.endsWith("/revisions")).at(-1)?.body.target).toBeUndefined();
  await page.reload();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("search.png") });
  await page.getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(page.getByLabel("Describe the life you want")).toBeVisible();
  await expect(page.locator(".home-journey")).toHaveCount(0);
  expect(errors).toEqual([]);
});

for (const stale of [false, true]) {
  test(`property proof ${stale ? "expiry keeps the home usable" : "opens the exact source"}`, async ({ page }) => {
    const calls = await mockJourneyApi(page, stale);
    await page.goto("/?q=3BHK");
    await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
    await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
    await page.getByRole("button", { name: "Apply change", exact: true }).click();
    await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
    await expect.poll(() => page.evaluate(() => {
      const saved = JSON.parse(localStorage.getItem("openestates:search-journeys:v1") ?? "[]");
      return saved.some((entry: { response?: { journey?: { active?: { revision?: { id?: string } } } } }) =>
        entry.response?.journey?.active?.revision?.id === "revision-2");
    })).toBe(true);
    const result = page.locator('.landing-featured__results a[href^="/property/"]').first();
    await expect(result).toBeVisible();
    const href = await result.getAttribute("href");
    expect(new URL(href!, page.url()).searchParams.get("proofToken")).toBe("signed:exact-receipt");
    expect(new URL(href!, page.url()).searchParams.has("focus")).toBe(false);
    await result.click();
    await expect(page.locator("#property-atlas")).toBeVisible();
    const backToResults = page.getByRole("navigation", { name: "Property navigation" })
      .getByRole("link", { name: "Back to results" });
    await expect(backToResults).toBeVisible();
    await expect(backToResults).toHaveAttribute("href", /journey=revision-2/);
    if (stale) {
      await expect(page.getByText("This search receipt is no longer available. You can still explore the home.")).toBeVisible();
      await expect(page.locator("#property-atlas")).toBeVisible();
    } else {
      await expect(page.locator("#property-search-match")).toContainText("Exact receipt value");
      await expect(page.locator("#property-search-match a")).toHaveAttribute("href", "https://example.test/exact-receipt");
    }
    expect(calls.find((call) => call.path.endsWith("/resolve"))?.body.proofToken).toBe("signed:exact-receipt");
    await expect.poll(() => page.evaluate(() => {
      const saved = JSON.parse(localStorage.getItem("openestates:search-journeys:v1") ?? "[]");
      return saved.find((entry: { response?: { journey?: { active?: { revision?: { id?: string } } } } }) =>
        entry.response?.journey?.active?.revision?.id === "revision-2")?.selectedId;
    })).toBe(atlasFixtureId);
    await page.goBack();
    await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" }).click();
    await page.getByLabel("Change search", { exact: true }).fill("Only 4BHK");
    await page.getByRole("button", { name: "Apply change", exact: true }).click();
    await expect.poll(() => calls.filter((call) => call.path.endsWith("/revisions")).at(-1)?.body.selectedPropertyId).toBeTruthy();
  });
}

test("retry preserves the mutation identity without exposing condition controls", async ({ page }) => {
  await mockJourneyApi(page);
  const mutations: Array<Record<string, unknown>> = [];
  await page.route("**/api/search/revisions", async (route) => {
    mutations.push(route.request().postDataJSON());
    return mutations.length === 1
      ? route.fulfill({ status: 500, json: { error: "temporary failure" } })
      : route.fulfill({ json: journeyFixture("revision-2", "3 BHK, under ₹2.5Cr") });
  });
  await page.goto("/?q=3BHK");
  await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
  await expect(page.getByLabel("Condition to change")).toHaveCount(0);
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Your search is unchanged");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
  expect(mutations).toHaveLength(2);
  expect(mutations[1]).toEqual(mutations[0]);
});

test("missing saved state offers recovery without rerunning a different search", async ({ page }) => {
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK&journey=missing-revision");
  await expect(page.getByRole("alert")).toContainText("Start a new search");
  await expect(page.getByLabel("Loading matching homes")).toHaveCount(0);
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(0);
  await page.getByRole("button", { name: "New search", exact: true }).click();
  await expect(page).toHaveURL("/");
  await expect(page.getByRole("alert")).toHaveCount(0);
  const searchInput = page.getByLabel("Describe the life you want");
  await searchInput.fill("3BHK");
  await expect(searchInput).toHaveValue("3BHK");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" })).toBeVisible();
});

test("Undo restores intent after refresh and an unrelated history entry", async ({ page }) => {
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByRole("button", { name: "Undo", exact: true })).toBeVisible();
  const savedUrl = page.url();
  // Resume can reissue a revision ID; that is not a user edit to undo.
  await page.route("**/api/search/resume", async (route) => {
    const body = route.request().postDataJSON();
    const updated = body.parentToken === "signed:revision-2";
    const envelope = openAtlasFromFirstResult(journeyFixture(updated ? "revision-2" : "revision-1",
      updated ? "3 BHK, under ₹2.5Cr" : "3 BHK, under ₹2.4Cr"));
    envelope.active.revision.id += "-resumed";
    envelope.attempt = { kind: "resume", outcome: "resumed" };
    return route.fulfill({ json: envelope });
  });
  await page.goto("/workspace");
  await page.goto(savedUrl);
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
  await expect(page).toHaveURL(/journey=revision-2-resumed/);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Undo", exact: true })).toHaveCount(0);
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(1);
});

test("a copied link restores the latest signed search in a fresh browser context", async ({ page, browser }) => {
  await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.4Cr" }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
  const copiedUrl = page.url();
  const fresh = await browser.newContext();
  try {
    const recipient = await fresh.newPage();
    const calls = await mockJourneyApi(recipient);
    await recipient.goto(copiedUrl);
    await expect(recipient.getByRole("button", { name: "Change search. Current search: 3 BHK, under ₹2.5Cr" })).toBeVisible();
    expect(calls.find((call) => call.path === "/api/search/resume")?.body.parentToken).toBe("signed:revision-2");
    expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(0);
    await expect(recipient.getByRole("button", { name: "Undo", exact: true })).toHaveCount(0);
    await recipient.getByRole("button", { name: "Search homes", exact: true }).click();
    await expect(recipient).toHaveURL(copiedUrl);
    await recipient.getByRole("button", { name: "Clear search", exact: true }).click();
    expect(new URL(recipient.url()).hash).toBe("");
  } finally { await fresh.close(); }
});

test("returning home resumes the latest intent while Clear starts a new journey", async ({ page }, testInfo) => {
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await page.getByRole("button", { name: /Change search. Current search/ }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await expect(page.getByRole("button", { name: /Current search: 3 BHK, under ₹2.5Cr/ })).toBeVisible();
  await page.goto("/");
  await expect(page.getByRole("button", { name: /Current search: 3 BHK, under ₹2.5Cr/ })).toBeVisible();
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(1);
  await page.screenshot({ path: testInfo.outputPath("continued-search.png") });
  await page.getByText("History", { exact: true }).click();
  await expect(page.getByRole("button", { name: "3 BHK, under ₹2.4Cr", exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("search-history.png") });
  await page.getByRole("button", { name: "3 BHK, under ₹2.4Cr", exact: true }).press("Escape");
  await expect(page.getByRole("button", { name: "3 BHK, under ₹2.4Cr", exact: true })).toBeHidden();
  await page.getByText("History", { exact: true }).click();
  const restoredTokens: string[] = [];
  await page.route("**/api/search/resume", (route) => {
    restoredTokens.push(route.request().postDataJSON().parentToken);
    return restoredTokens.length === 1
      ? route.fulfill({ status: 503, json: { error: "temporary failure" } })
      : route.fulfill({ json: openAtlasFromFirstResult(journeyFixture()) });
  });
  await page.getByRole("button", { name: "3 BHK, under ₹2.4Cr", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("could not be restored");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByRole("button", { name: /Current search: 3 BHK, under ₹2.4Cr/ })).toBeVisible();
  expect(restoredTokens).toEqual(["signed:revision-1", "signed:revision-1"]);
  await page.getByRole("button", { name: "Clear search", exact: true }).click();
  await page.reload();
  await expect(page).toHaveURL("/");
  await expect(page.getByLabel("Describe the life you want")).toBeVisible();
});

test("ambiguous edits disclose targets only when needed and submit the signed identity", async ({ page }, testInfo) => {
  await mockJourneyApi(page);
  const edits: Array<Record<string, unknown>> = [];
  await page.route("**/api/search/revisions", async (route) => {
    const body = route.request().postDataJSON();
    edits.push(body);
    if (body.target && edits.length === 2) return route.fulfill({ status: 503, json: { error: "temporary failure" } });
    const envelope = journeyFixture(body.target ? "revision-2" : "clarification");
    envelope.active.latestUtterance = "Under 2.5Cr";
    envelope.attempt = body.target ? { kind: "revision", outcome: "activated" } : {
      kind: "revision", outcome: "clarificationRequired",
      clarification: { code: "clarificationRequired", message: "Choose the condition." },
    };
    return route.fulfill({ json: envelope });
  });
  await page.goto("/?q=3BHK");
  await expect(page.getByLabel("Which part should change?")).toHaveCount(0);
  await page.getByRole("button", { name: /Change search. Current search/ }).click();
  await page.getByLabel("Change search", { exact: true }).fill("Under 2.5Cr");
  await page.getByRole("button", { name: "Apply change", exact: true }).click();
  await page.getByLabel("Which part should change?").selectOption("0");
  await expect(page.getByRole("option", { name: "Price ≤ ₹2.4Cr", exact: true })).toHaveCount(1);
  await page.screenshot({ path: testInfo.outputPath("search-clarification.png") });
  await page.getByRole("button", { name: "Apply to this condition" }).click();
  await expect(page.getByRole("alert")).toContainText("Your search is unchanged");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByLabel("Which part should change?")).toHaveCount(0);
  expect(edits[2]).toEqual(edits[1]);
  expect(edits[1].utterance).toBe("Under 2.5Cr");
  expect(edits[1].parentToken).toBe("signed:clarification");
  expect(edits[1].target).toEqual({ kind: "predicate", branchId: "branch-1", predicateId: "budget" });
});

test("catalog refresh keeps backend order without client-side rebucketing", async ({ page }, testInfo) => {
  await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  const updated = openAtlasFromFirstResult(journeyFixture());
  updated.attempt = { kind: "resume", outcome: "resumed", catalogRebased: true,
    catalogDelta: { added: [atlasFixtureId], removed: [], retained: [], moved: [] } };
  await page.route("**/api/search/resume", (route) => route.fulfill({ json: updated }));
  await page.reload();
  await expect(page.getByRole("heading", { name: "New since you looked" })).toHaveCount(0);
  await expect(page.locator('.landing-featured__results .landing-stage__feature-card a[href^="/property/"]').first()).toBeVisible();
  const cardHrefs = await page.locator('.landing-featured__results .landing-stage__feature-card a[href^="/property/"]').evaluateAll(
    (links) => links.map((link) => (link as HTMLAnchorElement).pathname),
  );
  expect(new Set(cardHrefs).size).toBe(cardHrefs.length);
  expect(cardHrefs[0]).toContain(atlasFixtureId);
  await page.screenshot({ path: testInfo.outputPath("catalog-additions.png") });
});

test("a property link rebuilds its journey after session storage is lost", async ({ page }) => {
  await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  const first = page.locator('.landing-featured__results a[href^="/property/"]').first();
  await expect(first).toBeVisible();
  const href = await first.getAttribute("href");
  await page.evaluate(() => sessionStorage.clear());
  await page.goto(href!);
  const back = page.getByRole("navigation", { name: "Property navigation" }).getByRole("link", { name: "Back to results" });
  await expect(back).toHaveAttribute("href", /journey=revision-1/);
  await back.click();
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
});

test("workspace and comparison carry the same journey back to search", async ({ page }) => {
  const calls = await mockJourneyApi(page);
  await page.goto("/?q=3BHK");
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  const workspace = page.getByRole("navigation", { name: "Footer" }).getByRole("link", { name: "Workspace" });
  await expect(workspace).toHaveAttribute("href", /context=revision-1/);
  await workspace.click();
  const compare = page.getByRole("navigation", { name: "Workspace view" }).getByRole("link", { name: "Compare", exact: true });
  await expect(compare).toHaveAttribute("href", /context=revision-1/);
  await compare.click();
  await page.evaluate(() => sessionStorage.clear());
  await page.reload();
  const notes = page.getByRole("navigation", { name: "Workspace view" }).getByRole("link", { name: "Notes", exact: true });
  await expect(notes).toHaveAttribute("href", /context=revision-1/);
  await notes.click();
  await page.getByRole("link", { name: "Explore", exact: true }).click();
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(1);
  expect(calls.filter((call) => call.path === "/api/search/resume").at(-1)?.body.parentToken).toBe("signed:revision-1");
});

test("config-ranked discovery shelves lead into the same search journey", async ({ page }, testInfo) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  const calls = await mockJourneyApi(page);
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Explore Whitefield", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Metro within reach", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Schools nearby", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Room to breathe", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Places people rate highly", exact: true })).toBeVisible();
  const firstShelf = page.locator(".landing-catalog__shelf").first();
  const firstShelfRail = firstShelf.locator(".landing-stage__featured");
  const firstShelfCards = firstShelf.locator(".landing-stage__feature-card:not(.landing-stage__feature-card--spacer)");
  const railBounds = await firstShelfRail.boundingBox();
  const firstCardBounds = await firstShelfCards.first().boundingBox();
  const lastCardBounds = await firstShelfCards.last().boundingBox();
  expect(firstCardBounds!.width).toBeGreaterThan(testInfo.project.name === "desktop" ? 250 : 300);
  if (testInfo.project.name === "desktop") {
    expect(Math.abs(
      lastCardBounds!.x + lastCardBounds!.width - railBounds!.x - railBounds!.width,
    )).toBeLessThan(1);
  } else {
    expect(firstCardBounds!.width).toBeCloseTo(railBounds!.width, 1);
  }
  await expect(firstShelf.locator(".landing-stage__feature-card--spacer")).toHaveCount(0);
  await firstShelf.scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath("discovery.png") });
  const productStory = page.getByRole("region", { name: "About us", exact: true });
  await productStory.scrollIntoViewIfNeeded();
  await expect(productStory).toBeVisible();
  await expect(page.getByRole("heading", { name: "About us", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Why 80feet", exact: true })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "See the receipts", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Compare before deciding", exact: true })).toBeVisible();
  await expect(page.getByLabel("Ways to browse")).toHaveCount(0);
  expect(calls.filter((call) => call.path === "/api/discovery")).toHaveLength(1);
  expect(calls.filter((call) => call.path === "/api/properties")).toHaveLength(0);
  await page.getByRole("button", { name: "Quiet near schools", exact: true }).click();
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(1);
  await expect(page.getByRole("heading", { name: "Explore Whitefield", exact: true })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "More to explore — Other areas", exact: true })).toBeVisible();
  await productStory.scrollIntoViewIfNeeded();
  await expect(page.getByRole("heading", { name: "See the receipts", exact: true })).toBeVisible();
  await expect(productStory).toBeVisible();
  await expect(page.locator(".landing-product-story__row a").first()).toHaveAttribute("href", /context=revision-1/);
  await expect(page.locator(".landing-product-story__row a").last()).toHaveAttribute("href", /context=revision-1/);
});

test("search and portable resume work when browser storage writes are unavailable", async ({ page }) => {
  const calls = await mockJourneyApi(page);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    Storage.prototype.setItem = () => { throw new DOMException("Storage disabled", "QuotaExceededError"); };
    Storage.prototype.removeItem = () => { throw new DOMException("Storage disabled", "SecurityError"); };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Quiet near schools", exact: true }).click();
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: /Change search. Current search/ })).toBeVisible();
  expect(calls.filter((call) => call.path === "/api/search")).toHaveLength(1);
  // StrictMode may replay the cancellable resume effect; each call must carry
  // the same checkpoint, never reconstruct a search from the visible brief.
  const resumes = calls.filter((call) => call.path === "/api/search/resume");
  expect(resumes.length).toBeGreaterThan(0);
  expect(new Set(resumes.map((call) => call.body.parentToken))).toEqual(new Set(["signed:revision-1"]));
  await page.getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(page).toHaveURL("/");
  expect(errors).toEqual([]);
});

test("native rail scrolling, resize and reduced motion keep controls synchronized", async ({ page }, testInfo) => {
  await mockJourneyApi(page);
  const envelope = journeyFixture();
  if (envelope.active.results.kind !== "current") throw new Error("fixture must be current");
  const template = envelope.active.results.resultSets[0].results[0];
  const cards = Array.from({ length: 15 }, (_, index) => ({ ...template, id: index === 0 ? atlasFixtureId : `scroll-${index}`, save_id: atlasFixtureId }));
  envelope.active.results.resultSets[0].results = cards;
  envelope.active.results.orderedResultIds = cards.map((card) => card.id);
  await page.route("**/api/search?*", (route) => route.fulfill({ json: envelope }));
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/?q=3BHK");
  const rail = page.locator(".landing-featured__results .landing-featured__rail").first();
  const scroller = rail.locator(".landing-stage__featured");
  const previous = rail.getByRole("button", { name: "Previous homes" });
  const next = rail.getByRole("button", { name: "Next homes" });
  await expect(previous).toBeDisabled();
  await expect(next).toBeEnabled();
  if (testInfo.project.name === "desktop") {
    const fullWidth = await scroller.evaluate((element) => element.clientWidth);
    await rail.getByRole("button", { name: /Save .* for later/ }).first().click();
    await expect.poll(() => scroller.evaluate((element) => element.clientWidth)).toBeLessThan(fullWidth);
    await expect(previous).toBeDisabled();
  } else {
    await scroller.scrollIntoViewIfNeeded();
    await scroller.click({ trial: true });
    const bounds = await scroller.boundingBox();
    const touch = await page.context().newCDPSession(page);
    await touch.send("Input.synthesizeScrollGesture", {
      x: bounds!.x + bounds!.width * 0.8, y: bounds!.y + 100,
      xDistance: -280, yDistance: 0, gestureSourceType: "touch",
    });
    await expect(previous).toBeEnabled();
    await page.screenshot({ path: testInfo.outputPath("touch-scroll.png") });
    await touch.detach();
  }
  // This is a native scroll event, independent of the arrow handler.
  await scroller.evaluate((element) => element.scrollTo({ left: element.scrollWidth, behavior: "instant" }));
  await expect(next).toBeDisabled();
  await expect(previous).toBeEnabled();
  await page.screenshot({ path: testInfo.outputPath("native-scroll-boundary.png") });
  await previous.click();
  await expect(next).toBeEnabled();
  await scroller.evaluate((element) => element.scrollTo({ left: 0, behavior: "instant" }));
  await expect(previous).toBeDisabled();
  const initialWidth = await scroller.evaluate((element) => element.clientWidth);
  await page.setViewportSize({ width: testInfo.project.name === "desktop" ? 1100 : 430, height: 900 });
  await expect.poll(() => scroller.evaluate((element) => element.clientWidth)).not.toBe(initialWidth);
  await expect(previous).toBeDisabled();
  await expect(next).toBeEnabled();
  await expect(page.locator(".landing-journey-section").first()).toHaveCSS("opacity", "1");
});

test("zero exact results carry contextual homes through detail, workspace and compare", async ({ page }) => {
  await mockJourneyApi(page);
  const envelope = journeyFixture();
  if (envelope.active.results.kind !== "current") throw new Error("fixture must be current");
  envelope.active.results.resultSets = [];
  envelope.active.results.orderedResultIds = [];
  const card = envelope.active.collections[0].cards[0];
  card.id = atlasFixtureId;
  card.detail_href = `/property/${atlasFixtureId}`;
  card.save_id = atlasFixtureId;
  await page.route("**/api/search?*", (route) => route.fulfill({ json: envelope }));
  await page.route("**/api/search/resume", (route) => route.fulfill({ json: { ...envelope, attempt: { kind: "resume", outcome: "resumed" } } }));
  await page.goto("/?q=3BHK");
  const contextual = page.locator(`.landing-catalog__shelf a[href^="/property/${atlasFixtureId}"]`).first();
  await expect(contextual).toHaveAttribute("href", /context=revision-1/);
  expect(new URL((await contextual.getAttribute("href"))!, page.url()).searchParams.has("proofToken")).toBe(false);
  await contextual.click();
  await expect(page.locator("#property-atlas")).toBeVisible();
  await expect(page.locator(".property-search-strip, .property-search-panel").first()).toContainText(envelope.active.collections[0].title);
  await page.getByRole("navigation", { name: "Property navigation" }).getByRole("link", { name: "Back to results" }).click();
  await expect(page.getByRole("heading", { name: envelope.active.collections[0].title, exact: true })).toBeVisible();
  await page.getByRole("navigation", { name: "Footer" }).getByRole("link", { name: "Workspace" }).click();
  const compare = page.getByRole("navigation", { name: "Workspace view" }).getByRole("link", { name: "Compare", exact: true });
  await expect(compare).toHaveAttribute("href", /context=revision-1/);
  await compare.click();
  await page.getByRole("navigation", { name: "Buyer workspace" }).getByRole("link", { name: "Back to results", exact: true }).click();
  await expect(page.getByRole("heading", { name: envelope.active.collections[0].title, exact: true })).toBeVisible();
});

test("live bundle API and UI agree through two edits, proof and resume", async ({ page, request }, testInfo) => {
  test.skip(!process.env.SEARCH_LIVE_API, "Set SEARCH_LIVE_API to run against a rebuilt API");
  const bank = JSON.parse(readFileSync(new URL("../../../data/validation/search_query_bank.json", import.meta.url), "utf8"));
  const suite = bank.suites.find((suite: { id: string }) => suite.id === "proof_handoff_live");
  const scenario = bank.cases.find((scenario: { group: string }) => scenario.group === "proof_handoff_live");
  const api = process.env.SEARCH_LIVE_API!;
  const health = await (await request.get(`${api}/api/health`)).json();
  expect(health.serving_bundle_version).toBe(suite.required_serving_bundle_version);
  const initialResponse = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/search");
  await page.goto(`/?q=${encodeURIComponent(scenario.query)}`);
  let envelope = await (await initialResponse).json();
  const assertParity = async () => {
    await expect(page.getByRole("button", { name: `Change search. Current search: ${envelope.active.buyerBrief}`, exact: true })).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`journey=${envelope.active.revision.id}`));
    const cards = envelope.active.results.resultSets.flatMap((set: { results: Array<{ id: string; reasons: Array<{ showOnCard: boolean; explanation: string; proofToken: string }> }> }) => set.results);
    const links = page.locator('.landing-featured__results .catalog-card__link');
    await expect(links).toHaveCount(cards.length);
    const actual = await links.evaluateAll((nodes) => nodes.map((node) => decodeURIComponent(new URL((node as HTMLAnchorElement).href).pathname.split("/").at(-1)!)));
    expect(actual).toEqual(cards.map((card: { id: string }) => card.id));
    for (const [index, card] of cards.entries()) {
      const shown = card.reasons.find((reason: { showOnCard: boolean }) => reason.showOnCard);
      await expect(links.nth(index).locator(".catalog-card__signal")).toHaveText(shown ? [shown.explanation] : []);
      const href = new URL((await links.nth(index).getAttribute("href"))!, page.url());
      expect(href.searchParams.get("context")).toBe(envelope.active.revision.id);
      expect(href.searchParams.get("proofToken")).toBe((shown ?? card.reasons[0])?.proofToken ?? null);
    }
    for (const collection of envelope.active.collections) {
      const section = page.locator(".landing-catalog__shelf").filter({ has: page.getByRole("heading", { name: collection.title, exact: true }) });
      await expect(section.locator(".catalog-card__link")).toHaveCount(collection.cards.length);
      const ids = await section.locator(".catalog-card__link").evaluateAll((nodes) => nodes.map((node) => decodeURIComponent(new URL((node as HTMLAnchorElement).href).pathname.split("/").at(-1)!)));
      expect(ids).toEqual(collection.cards.map((card: { id: string }) => card.id));
    }
  };
  await assertParity();
  for (const utterance of ["under 2.5 Cr", "near metro"]) {
    await page.getByRole("button", { name: /Change search. Current search/ }).click();
    await page.getByLabel("Change search", { exact: true }).fill(utterance);
    const edited = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/search/revisions");
    await page.getByRole("button", { name: "Apply change", exact: true }).click();
    envelope = await (await edited).json();
    expect(envelope.attempt.outcome).toBe("activated");
    await assertParity();
  }
  await page.screenshot({ path: testInfo.outputPath("live-search.png") });
  const intent = envelope.active.intent;
  const ids = envelope.active.results.orderedResultIds;
  const first = page.locator('.landing-featured__results .catalog-card__link').first();
  expect(new URL((await first.getAttribute("href"))!, page.url()).searchParams.has("proofToken")).toBe(true);
  const propertyPath = new URL((await first.getAttribute("href"))!, page.url()).pathname;
  // StrictMode cancels the first effect request. Inspect the completed response,
  // rather than an abandoned response whose headers happened to arrive first.
  const proof = page.waitForResponse(async (response) => {
    if (new URL(response.url()).pathname !== "/api/search/proofs/resolve") return false;
    try { await response.json(); return true; } catch { return false; }
  });
  await first.click();
  const resolved = await proof;
  expect(resolved.status()).toBe(200);
  expect((await resolved.json()).targetLabel).toBe(scenario.expected.target_label);
  await expect(page.locator("#property-atlas")).toBeVisible();
  const selected = page.locator(".property-atlas__place-list .is-selected");
  await expect(selected).toContainText(scenario.expected.target_label);
  await expect(selected.getByText("Matched your search", { exact: true })).toBeVisible();
  await expect(selected.getByRole("link", { name: "Source" })).toHaveAttribute("href", /^https:\/\//);
  await selected.getByRole("link", { name: "Source" }).click({ trial: true });
  await expect(page.getByText(/This search receipt is no longer available/)).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("live-proof.png"), animations: "disabled" });
  const resumed = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/search/resume");
  await page.getByRole("navigation", { name: "Property navigation" }).getByRole("link", { name: "Back to results" }).click();
  envelope = await (await resumed).json();
  expect(envelope.active.intent).toEqual(intent);
  expect(envelope.active.results.orderedResultIds).toEqual(ids);
  await assertParity();
  await page.goto(propertyPath);
  await expect(page.locator("#property-atlas")).toBeVisible();
  await expect(page.getByText("Matched your search", { exact: true })).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath("live-property-rest.png"), animations: "disabled" });
});
