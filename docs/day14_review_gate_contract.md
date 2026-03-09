# Day 14 — Review Gate Contract

## Purpose

This document defines the hard review gate established on Day 14 to restore truthfulness in the build-review loop. After Days 10-13 produced ambiguous review results where tooling failures were confused with product rendering issues, Day 14 introduces explicit classification, stable headings, and a single review entrypoint.

## Review Gate Entrypoint

```
python3 pipeline/run_review_gate.py
```

This is the single "truth entrypoint" for deployed review. It runs three evidence layers in order and produces a machine-readable verdict.

### Options

| Flag | Default | Description |
|---|---|---|
| `--frontend-url` | `http://localhost:5173` | Frontend URL to capture |
| `--api-url` | `http://localhost:4000` | Backend API URL |
| `--skip-journey` | (off) | Skip browser journey, run smoke + capture only |

## Evidence Layers

### Layer A: API Smoke Tests (`pipeline/smoke_test_api.py`)

Answers: are backend contracts alive? Do detail endpoints behave correctly?

Tests:
- `GET /api/health` → 200
- `GET /api/properties` → non-empty list with card shape
- `GET /api/properties/:id` (valid) → conviction, market, greenery, compare, transparency fields
- `GET /api/properties/:id` (invalid) → 404
- `GET /api/areas` → non-empty list with card shape
- `GET /api/areas/:id` (valid) → full signal fields
- `GET /api/areas/:id` (invalid) → 404
- Society livability fields in property detail
- Shortlist placeholder

### Layer B: Page Capture (`pipeline/capture_deployed_pages.py`)

Answers: what did the deployed product actually render?

Pages captured:
- **landing** (`/`)
- **results** (`/results`)
- **property_valid** (`/property/:id` with dynamic ID)
- **property_invalid** (`/property/does-not-exist`)
- **shortlist** (`/shortlist`)

Per-page artifacts:
- Screenshot (`.png`)
- Rendered text (`.txt`)
- Status JSON (`.status.json`)

### Layer C: Journey Verification (`pipeline/journey_property_to_shortlist.py`)

Answers: what user path actually worked?

Flow: `results → property detail → save → shortlist`

## Page Capture Status Model

Every reviewed page gets one of these statuses:

| Status | Meaning |
|---|---|
| `ok` | Page rendered successfully with real content |
| `tooling_error` | Automation/harness failure leaked into capture (e.g. Playwright runtime error, Python traceback) |
| `navigation_error` | Page failed to load (timeout, DNS, etc.) |
| `render_error` | Page loaded but SPA did not render (blank body, crash overlay) |
| `product_fallback` | Page rendered an intentional, product-owned fallback state |

### Key rule: tooling_error and product_fallback must never be confused

If captured text contains tooling error signatures, it is always classified as `tooling_error`, even if product fallback text is also present.

## Tooling Error Signatures

These strings in rendered text indicate a tooling/automation leak, not a product state:

- `Playwright Sync API`
- `Traceback (most recent`
- `RuntimeError`
- `ECONNREFUSED`
- `UnhandledRejection`
- `TypeError:`
- `Cannot read properties of`

## Product Fallback Signatures

These strings indicate an intentional, product-owned fallback state:

- `Results temporarily unavailable`
- `Property details unavailable`
- `Property not found`
- `Compare saved homes`
- `No saved homes to compare yet`
- `Return to homepage`
- `Browse Bengaluru areas`
- `We couldn't load`
- `Saved homes stay on this browser`

## Fallback Headings by Page

### Results page
- **"Results temporarily unavailable"** — shown when backend is unreachable
- Recovery actions: "Browse Bengaluru areas", "Return to homepage"

### Property page
- **"Property details unavailable"** — backend error
- **"Property not found"** — 404 / invalid ID
- Recovery actions: "Browse properties", "Return to homepage"

### Shortlist page
- **"Compare saved homes"** — page heading (both populated and empty states)
- **"No saved homes to compare yet"** — empty state
- **"Saved homes stay on this browser for now."** — local storage disclosure

### Landing page
- **"Describe what you're looking for"** — search input label

## Review Gate Verdicts

| Verdict | Exit code | Meaning |
|---|---|---|
| `review_passed` | 0 | All evidence layers green; product is reviewable |
| `review_failed_tooling` | 2 | Automation/harness issue, not a product bug |
| `review_failed_product` | 1 | Deployed rendering has issues |
| `review_failed_mixed` | 3 | Both tooling and product issues detected |

## Output Artifacts

All saved to `pipeline/feedback/day14/`:

| File | Content |
|---|---|
| `smoke_test_report.json` | API smoke test results |
| `capture_summary.json` | Page capture aggregate |
| `{page}.png` | Per-page screenshot |
| `{page}.txt` | Per-page rendered text |
| `{page}.status.json` | Per-page status with evidence |
| `journey_report.json` | Journey step results |
| `journey_shortlist.png` | Shortlist screenshot from journey |
| `review.summary.json` | Aggregate review verdict |

## Pre-Day-15 Gate

Day 15 may not begin product breadth work until:

1. `review.summary.json` exists with `review_status` field
2. The verdict is `review_passed` OR
3. The exact remaining failure class is isolated and documented

If the verdict is `review_failed_tooling`, that is not a product blocker — it means the harness needs fixing, not the product.

If the verdict is `review_failed_product`, the specific failing pages and their statuses must be addressed before new feature work.

## What Remains Known/Unknown After Day 14

### Known
- The review gate script exists and runs as the main deployed-review entrypoint
- Per-page statuses use the fixed 5-status model
- Tooling errors and product fallbacks are explicitly separated
- Fallback UX is intentional, calm, and product-owned
- Shortlist is frontend-local (localStorage) — this is disclosed to users

### Unknown (deferred)
- Vercel deployment capture (depends on deploy; gate works against localhost)
- CI integration (gate runs locally for now)
- Whether all 20 properties render correctly on detail pages (only first valid ID tested)
- Performance under load (not in scope)
