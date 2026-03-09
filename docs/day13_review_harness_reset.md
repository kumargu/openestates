# Day 13 Review Harness Reset

## What happened

Days 10, 11, and 12 all encountered the same pattern: the integrated review pipeline (page capture + journey + smoke tests) produced a Playwright sync/async runtime conflict. The error appeared as a `RuntimeError` when sync Playwright helpers were inadvertently called inside an async execution path.

Each day attempted incremental fixes within the existing orchestration layer (`review_capture.py`, `review_journey.py`). By Day 12, the scripts had been rewritten to use fully async Playwright, but the broader orchestration still carried complexity from the original integrated pipeline.

The repeated failure meant we could not reliably answer: **"What does the deployed product actually render?"**

## Why a standalone harness was introduced

Rather than continuing to patch the integrated pipeline, Day 13 introduced a deliberate simplification:

- **New standalone scripts** that do not depend on the broader agent/review orchestration
- **Minimal imports** — only `asyncio`, `json`, `pathlib`, and `playwright.async_api`
- **No shared wrapper code** from the larger pipeline
- **Direct invocation** via command line with explicit `--base-url` and `--output-dir` flags

This reduces the surface area where the sync/async conflict can appear to essentially zero.

## What the standalone harness captures

### Page capture (`pipeline/capture_deployed_pages.py`)

Captures 5 target pages:

| Page | Path |
|---|---|
| Landing | `/` |
| Results | `/results` |
| Property (valid) | `/property/<resolved-id>` |
| Property (invalid) | `/property/does-not-exist` |
| Shortlist | `/shortlist` |

The valid property ID is resolved dynamically from `GET /api/properties` (first result).

For each page, saves:
- `<name>.png` — full-page screenshot
- `<name>.txt` — rendered body text (first 8000 chars)
- `<name>.status.json` — structured status metadata

Status values: `ok`, `product_fallback`, `tooling_error`, `navigation_error`, `render_error`

### Journey script (`pipeline/journey_property_to_shortlist.py`)

Verifies the core user flow:
1. Open `/results`
2. Navigate to a property detail page
3. Save the property
4. Save a second property (so compare works)
5. Open `/shortlist` and verify compare surfaces appear

Saves `journey_report.json` and `journey_shortlist.png`.

### Smoke tests (`pipeline/smoke_test_api.py`)

Expanded backend API tests:
- Health check
- Property listing (non-empty)
- Property detail (valid ID) with response shape verification:
  - Conviction fields (days_on_market, greenery_score, etc.)
  - Transparency fields (scores, risks)
  - Render fields (what frontend needs to display)
  - Society and area sub-object fields
- Property detail (invalid ID) → 404
- Area listing
- Area detail (valid ID) with field checks
- Shortlist placeholder

## Where artifacts are stored

```
pipeline/feedback/day13/
  landing.png / .txt / .status.json
  results.png / .txt / .status.json
  property_valid.png / .txt / .status.json
  property_invalid.png / .txt / .status.json
  shortlist.png / .txt / .status.json
  capture_summary.json
  journey_report.json
  journey_shortlist.png
  smoke_test_report.json
```

## Shortlist ownership

Shortlist is frontend-local, stored in browser `localStorage` under key `openestates_shortlist`. The backend `/api/shortlist` endpoint returns an empty placeholder. Save/shortlist verification depends on preserving browser context within a single Playwright session. No backend persistence is assumed.

## What remains unknown

If any pages still fail after running the standalone harness:

- **`tooling_error`** → Playwright installation or environment issue (not a product bug)
- **`navigation_error`** → Server unreachable or routing misconfiguration
- **`render_error`** → React app crash, SPA hydration failure, or blank page
- **`product_fallback`** → Intentional empty/not-found state (expected for invalid property, empty shortlist)

## What Day 14 should do

**If rendering is proven** (all captures show `ok` or `product_fallback`):
- Return to product UX work
- Focus on real deployed review of transparency surfaces
- Use the captured artifacts to assess visual quality

**If failures remain**:
- The standalone harness provides exact classification
- Focus repair on the specific failure type (tooling vs navigation vs render)
- Do not add product breadth until the failure is resolved
