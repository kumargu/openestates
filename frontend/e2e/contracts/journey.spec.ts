import { expect, test } from "@playwright/test";

const api = "http://127.0.0.1:4016";

test("materialized evidence survives search, detail, selected-home reads and return", async ({ page, request }, info) => {
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
  await page.screenshot({ path: info.outputPath("search-resting.png") });
  const selected = await (await request.get(`${api}/api/properties/${links[0].split("/").pop()}`)).json();
  const selectedPrice = `₹${(selected.property.price / 10_000_000).toFixed(2)}Cr`;
  await cards.first().click();
  await expect(page.locator("details.property-search-match")).toBeVisible();
  await expect(page.getByText(/receipt.*expired/i)).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("receipt-resting.png") });
  await page.locator("details.property-search-match summary").click();
  await expect(page.locator("details.property-search-match a").first()).toHaveAttribute("href", /^https:\/\/listings\.example\//);
  await page.screenshot({ path: info.outputPath("receipt-open.png") });
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
  await page.screenshot({ path: info.outputPath("compare-resting.png") });
  await page.getByRole("link", { name: /Back to results/i }).click();
  await expect.poll(() => cards.evaluateAll((elements) => elements.map((element) => new URL((element as HTMLAnchorElement).href).pathname))).toEqual(links);
  expect(reads.filter((path) => path === "/api/properties")).toEqual([]);
  expect(errors).toEqual([]);

  const search = await (await request.get(`${api}/api/search?q=3BHK`)).json();
  const ids = search.active.results.orderedResultIds;
  const snapshotIdentity = search.runtimeVersion.snapshotIdentity;
  const batch = await request.post(`${api}/api/properties/batch`, { data: { propertyIds: [ids[1], ids[0]], snapshotIdentity } });
  expect(batch.ok()).toBeTruthy();
  expect((await batch.json()).items.map((item: { id: string }) => item.id)).toEqual([ids[1], ids[0]]);
  const stale = await request.get(`${api}/api/properties/${ids[0]}?snapshotIdentity=retired`);
  expect(stale.status()).toBe(409);
  expect((await stale.json()).error).toBe("stale_snapshot");
});
