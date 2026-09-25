import { expect, test, type Page, type TestInfo } from "@playwright/test";

async function capture(page: Page, info: TestInfo, name: string) {
  await page.evaluate(async () => {
    const animations = document.getAnimations().filter(animation =>
      animation.effect?.getComputedTiming().iterations !== Infinity);
    await Promise.all(animations.map(animation => animation.finished.catch(() => undefined)));
  });
  await page.screenshot({ path: info.outputPath(name) });
}

const api = "http://127.0.0.1:4016";

test("materialized evidence survives search, detail, selected-home reads and return", async ({ page, request }, info) => {
  expect((await request.post("http://127.0.0.1:4017/snapshot/original")).ok()).toBeTruthy();
  const errors: string[] = [];
  const reads: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => reads.push(new URL(request.url()).pathname));
  // External rendering is outside this contract. All application API requests
  // reach the production router over source-materialized Parquet.
  await page.route("https://maps.googleapis.com/**", (route) => route.abort());
  await page.goto("/?q=3BHK");
  const cards = page.locator(".landing-featured__results .catalog-card__link");
  await expect(cards.first()).toBeVisible();
  const links = await cards.evaluateAll((elements) => elements.map((element) => new URL((element as HTMLAnchorElement).href).pathname));
  await capture(page, info, "search-resting.png");
  const selected = await (await request.get(`${api}/api/properties/${links[0].split("/").pop()}`)).json();
  const selectedPrice = `₹${(selected.property.price / 10_000_000).toFixed(2)}Cr`;
  const detailHref = await cards.first().getAttribute("href");
  await cards.first().click();
  await expect(page.locator("details.property-search-match")).toBeVisible();
  await expect(page.getByText(/receipt.*expired/i)).toHaveCount(0);
  await capture(page, info, "receipt-resting.png");
  await page.locator("details.property-search-match summary").click();
  await expect(page.locator("details.property-search-match a").first()).toHaveAttribute("href", /^https:\/\/listings\.example\//);
  await capture(page, info, "receipt-open.png");
  await page.getByRole("link", { name: "EMI Plan", exact: true }).click();
  await expect(page.getByRole("region", { name: "Loan repayment assumptions" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toContainText(selectedPrice);
  await page.getByRole("link", { name: "RERA", exact: true }).click();
  await page.locator("summary").filter({ hasText: "Plans" }).click();
  const document = page.getByRole("link", { name: "Sanctioned plan", exact: true });
  await expect(document).toHaveAttribute("href", "https://rera.example/documents/sanctioned-plan.pdf");
  await expect(document).toHaveAttribute("target", "_blank");
  await page.getByRole("link", { name: /Back to results/i }).click();
  await expect(cards.first()).toBeVisible();
  await expect.poll(() => cards.evaluateAll((elements) => elements.map((element) => new URL((element as HTMLAnchorElement).href).pathname))).toEqual(links);
  const resultRail = page.locator(".landing-featured__results");
  const homeNames = await cards.locator("h3").allTextContents();
  for (const name of homeNames.slice(0, 2)) {
    await resultRail.getByRole("button", { name: `Save ${name} for later`, exact: true }).click();
    await expect(resultRail.getByRole("button", { name: `Remove ${name} from shortlist`, exact: true })).toBeVisible();
  }
  await page.getByRole("link", { name: "Workspace", exact: true }).click();
  const selection = page.locator(".notion-compare-check input");
  await expect(selection).toHaveCount(2);
  await selection.nth(0).check();
  await selection.nth(1).check();
  await page.getByRole("navigation", { name: "Workspace view" }).getByRole("link", { name: "Compare", exact: true }).click();
  const comparedHomes = page.locator(".compare-editorial__home");
  await expect(comparedHomes).toHaveCount(2);
  await expect(comparedHomes.first()).toContainText("super built-up");
  const comparedLinks = await comparedHomes.locator("a").evaluateAll((elements) => elements.map((element) => new URL((element as HTMLAnchorElement).href).pathname));
  expect(new Set(comparedLinks)).toEqual(new Set(links.slice(0, 2)));
  await capture(page, info, "compare-resting.png");
  await page.getByRole("link", { name: /Back to results/i }).click();
  await expect.poll(() => cards.evaluateAll((elements) => elements.map((element) => new URL((element as HTMLAnchorElement).href).pathname))).toEqual(links);
  expect(reads.filter((path) => path === "/api/properties")).toEqual([]);
  expect(errors).toEqual([]);

  const search = await (await request.get(`${api}/api/search?q=3BHK`)).json();
  const ids = search.active.results.orderedResultIds;
  const snapshotIdentity = search.runtimeVersion.snapshotIdentity;
  await expect(async () => {
    const batch = await request.post(`${api}/api/properties/batch`, { data: { propertyIds: [ids[1], ids[0]], snapshotIdentity } });
    expect(batch.ok(), `${batch.status()}: ${await batch.text()}`).toBeTruthy();
    expect((await batch.json()).items.map((item: { id: string }) => item.id)).toEqual([ids[1], ids[0]]);
  }).toPass({ intervals: [250, 500], timeout: 5000 });
  const stale = await request.get(`${api}/api/properties/${ids[0]}?snapshotIdentity=retired`);
  expect(stale.status()).toBe(409);
  expect((await stale.json()).error).toBe("stale_snapshot");

  expect((await request.post("http://127.0.0.1:4017/snapshot/updated")).ok()).toBeTruthy();
  await page.goto(detailHref!);
  await expect(page.getByRole("heading", { name: "This evidence changed since your search." })).toBeVisible();
  await expect(page.locator("details.property-search-match")).toHaveCount(0);
  await capture(page, info, "snapshot-changed.png");
  await page.getByRole("link", { name: "Search again", exact: true }).click();
  await expect(cards.first()).toBeVisible();
  const updatedSearch = await (await request.get(`${api}/api/search?q=3BHK`)).json();
  expect(updatedSearch.runtimeVersion.snapshotIdentity).not.toBe(snapshotIdentity);
  await cards.first().click();
  await expect(page.locator("details.property-search-match")).toBeVisible();
  await expect(page.getByText("This evidence changed since your search.", { exact: true })).toHaveCount(0);
});
