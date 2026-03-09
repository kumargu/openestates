# Day 12 — Review Loop Contract

## What was broken before

The review pipeline had a Playwright Sync-vs-Async runtime error. When
`playwright.sync_api` calls were mixed with `asyncio` orchestration, Playwright
raised `Error: It looks like you are using Playwright Sync API inside the async
context`. This made all deployed-page captures fail silently, producing no
screenshots, no rendered text, and no usable status artifacts.

The result was that product progress could not be visually verified. Tooling
failures were indistinguishable from product bugs.

## What is fixed now

The entire review pipeline uses **Playwright Async API** end-to-end:

```python
async with async_playwright() as p:
    browser = await p.chromium.launch(headless=True)
    context = await browser.new_context(viewport={"width": 1280, "height": 900})
    page = await context.new_page()
    await page.goto(url, wait_until="networkidle")
    text = await page.inner_text("body")
    await page.screenshot(path=..., full_page=True)
```

No sync API calls exist anywhere in the pipeline. All scripts run via
`asyncio.run()` at the top level.

## Capture statuses

Each page capture produces one of these statuses:

| Status | Meaning |
|---|---|
| `ok` | Page rendered with real product content |
| `product_fallback` | Page showed an intentional fallback state (e.g. "not found", empty shortlist). This is a valid product state, not a bug. |
| `tooling_error` | The Playwright harness failed (DOM query error, selector error, etc.) |
| `navigation_error` | Page navigation failed (timeout, network error, DNS failure) |
| `render_error` | Page loaded but body had <10 chars or a Vite/React crash overlay was detected |

The key distinction: **`product_fallback` means the product is working as
designed. `tooling_error` / `navigation_error` / `render_error` mean something
in the pipeline or infrastructure broke.**

## Pages captured by default

| Page name | URL path | Expected status |
|---|---|---|
| `landing` | `/` | ok |
| `results` | `/results` | ok |
| `property_prop_w_001` | `/property/prop_w_001` | ok |
| `property_invalid` | `/property/NONEXISTENT_ID` | product_fallback |
| `shortlist` | `/shortlist` | ok or product_fallback (depends on localStorage state) |

The valid property ID (`prop_w_001`) comes from the seed dataset. It is not
hardcoded blindly — it exists in `data/seed/properties.json`.

## Artifact structure

All artifacts are saved to a deterministic directory:

```
pipeline/feedback/day12/
  landing.png                    # Full-page screenshot
  landing.txt                    # Rendered body text (first 8000 chars)
  landing.status.json            # Machine-readable capture status
  results.png
  results.txt
  results.status.json
  property_prop_w_001.png
  property_prop_w_001.txt
  property_prop_w_001.status.json
  property_invalid.png
  property_invalid.txt
  property_invalid.status.json
  shortlist.png
  shortlist.txt
  shortlist.status.json
  capture_summary.json           # Overall summary of all captures
  journey_shortlist.png          # Shortlist screenshot from journey
  journey_report.json            # Journey step-by-step results
  smoke_test_report.json         # Backend smoke test results
```

### Status JSON format (per page)

```json
{
  "page_name": "results",
  "url": "http://localhost:5173/results",
  "capture_status": "ok",
  "http_status": 200,
  "text_length": 1834,
  "has_screenshot": true,
  "has_rendered_text": true,
  "error": null,
  "captured_at": "2026-03-09T12:34:56Z"
}
```

## Deterministic journey script

`pipeline/review_journey.py` runs a deterministic browser scenario:

1. **Open homepage** — verify >50 chars rendered
2. **Navigate to results** — verify BHK/sqft/Cr text present
3. **Open property detail** (`/property/prop_w_001`) — verify 5+ conviction
   sections via text anchors and `data-testid` selectors
4. **Save property** — click save button, verify button text changes
5. **Save second property** (`prop_s_001`) — ensures compare works
6. **Open shortlist** — verify Quick Compare, Decision Themes, Best For
   sections present (text + `data-testid`)
7. **Verify compare-oriented state** — Quick Compare + at least one of
   Decision Themes/Best For must be present
8. **Remove from shortlist** — click Remove button

The journey uses `data-testid` selectors with text-based fallbacks:
- `data-testid="save-button"` for the save button
- `data-testid="quick-compare-section"` for Quick Compare
- `data-testid="decision-themes-section"` for Decision Themes
- `data-testid="best-for-section"` for Best For

## Shortlist ownership

**Shortlist state is entirely frontend-local (localStorage).**

- Key: `"openestates_shortlist"` in localStorage
- No backend shortlist persistence exists yet
- The backend `/api/shortlist` endpoint returns an empty placeholder
- Browser automation must preserve or seed localStorage to test shortlist flows
- The journey script creates shortlist state by clicking save buttons during
  the test run (same browser context)

## What remains out of scope

- Server-side shortlist persistence
- Authentication or user sessions
- Real deployment URL testing (scripts work with any base URL)
- Visual regression comparison (screenshots are captured but not diffed)
- Performance benchmarks
- Mobile viewport testing (captures use 1280x900 desktop viewport)

## Running the pipeline

```bash
# 1. Start backend (port 4000)
cd backend && cargo run

# 2. Start frontend (port 5173)
cd frontend && npm run dev

# 3. Run smoke tests against backend
python3 pipeline/smoke_test.py

# 4. Run page capture against frontend
python3 pipeline/review_capture.py http://localhost:5173

# 5. Run conviction journey
python3 pipeline/review_journey.py http://localhost:5173
```

All three scripts exit with code 1 on failure, code 0 on success.
