#!/usr/bin/env node
let chromium;
try {
  ({ chromium } = require("playwright"));
} catch {
  ({ chromium } = require("../frontend/node_modules/playwright"));
}

const BASE_URL = (process.env.OPENESTATES_WEB_BASE || "http://127.0.0.1:5173").replace(/\/$/, "");
const QUERY = process.env.SEARCH_UI_QUERY || "3BHK Whitefield under 2Cr";
const REPEATS = Number.parseInt(process.env.SEARCH_UI_REPEATS || "4", 10);
const WARM_MEDIAN_THRESHOLD_MS = Number.parseInt(
  process.env.SEARCH_UI_WARM_MEDIAN_THRESHOLD_MS || "650",
  10,
);
const COLD_THRESHOLD_MS = Number.parseInt(
  process.env.SEARCH_UI_COLD_THRESHOLD_MS || "1200",
  10,
);
const EXPECTED_TEXT = ["Prestige Park Grove", "Godrej Splendour"];

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid];
}

async function runOnce(browser, index) {
  const page = await browser.newPage({ viewport: { width: 1365, height: 900 } });
  const events = [];
  page.on("request", (request) => {
    if (request.url().includes("/api/")) {
      events.push(["request", Date.now(), request.method(), request.url()]);
    }
  });
  page.on("response", (response) => {
    if (response.url().includes("/api/")) {
      events.push(["response", Date.now(), response.status(), response.url()]);
    }
  });

  await page.goto(`${BASE_URL}/?_latency_run=${Date.now()}-${index}`, {
    waitUntil: "domcontentloaded",
    timeout: 30000,
  });
  const input = page.locator(".home-search-input, input[type=\"text\"]").first();
  await input.waitFor({ timeout: 10000 });
  await input.fill(QUERY);

  const startedAt = Date.now();
  await input.press("Enter");
  await page.waitForFunction(
    (expected) => expected.some((text) => document.body.innerText.includes(text)),
    EXPECTED_TEXT,
    { timeout: 15000 },
  );
  const visibleMs = Date.now() - startedAt;
  const searchRequests = events.filter(
    (event) => event[0] === "request" && String(event[3]).includes("/api/search?"),
  ).length;
  const cardCount = await page.locator(".catalog-card, article").count();
  await page.close();

  return { visibleMs, searchRequests, cardCount, events };
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const runs = [];
  try {
    for (let index = 0; index < REPEATS; index += 1) {
      runs.push(await runOnce(browser, index));
    }
  } finally {
    await browser.close();
  }

  const coldMs = runs[0]?.visibleMs ?? 0;
  const warmRuns = runs.slice(1);
  const warmMedianMs = median(warmRuns.map((run) => run.visibleMs));
  const duplicateSearchRuns = runs.filter((run) => run.searchRequests > 1);
  const emptyRuns = runs.filter((run) => run.cardCount === 0);

  for (const [index, run] of runs.entries()) {
    console.log(
      `search_ui_latency run=${index + 1} visible_ms=${run.visibleMs} ` +
        `search_requests=${run.searchRequests} cards=${run.cardCount}`,
    );
  }

  const failures = [];
  if (coldMs > COLD_THRESHOLD_MS) {
    failures.push(`cold visible latency ${coldMs}ms exceeded ${COLD_THRESHOLD_MS}ms`);
  }
  if (warmMedianMs > WARM_MEDIAN_THRESHOLD_MS) {
    failures.push(
      `warm median latency ${warmMedianMs}ms exceeded ${WARM_MEDIAN_THRESHOLD_MS}ms`,
    );
  }
  if (duplicateSearchRuns.length > 0) {
    failures.push(`${duplicateSearchRuns.length} run(s) made duplicate search requests`);
  }
  if (emptyRuns.length > 0) {
    failures.push(`${emptyRuns.length} run(s) rendered no result cards`);
  }

  if (failures.length > 0) {
    console.error("\nSearch UI latency smoke failed:");
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }

  console.log(
    `PASS search UI latency cold_ms=${coldMs} warm_median_ms=${warmMedianMs} repeats=${REPEATS}`,
  );
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
