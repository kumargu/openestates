# Day 11 — Review Pipeline Fix

## What was broken

The Playwright capture pipeline in `pipeline/agent.py` used the **sync Playwright API** (`sync_playwright()`) but was called from within an async context (the day agent's asyncio loop). This produced:

```
"It looks like you are using Playwright Sync API inside the asyncio loop.
Please use the Async API instead."
```

All page captures failed — landing, results, and shortlist pages could not be rendered or reviewed. This meant Day 10's conviction surfaces (property detail, shortlist compare) were never actually verified in the deployed product.

## What changed

### 1. New async capture module: `pipeline/review_capture.py`

A standalone capture pipeline built entirely on **async Playwright API**:
- `async_playwright()` → async browser launch → async page navigation → async screenshot + text extraction
- Each page produces three artifacts: `.png` screenshot, `.txt` rendered text, `.status.json` metadata
- A `capture_summary.json` is written per run with aggregate results

### 2. Failure classification

Each capture now returns a structured status:

| Status | Meaning |
|---|---|
| `ok` | Page rendered successfully, text extracted, screenshot saved |
| `tooling_error` | Playwright/runtime error (not a product issue) |
| `navigation_error` | Page could not be reached (URL/network problem) |
| `render_error` | Page loaded but SPA didn't hydrate (empty body) |

This prevents tooling failures from being mistaken for product UX problems.

### 3. Updated `agent.py` bridge

The `capture_rendered_pages()` function in `agent.py` now delegates to the async module. If called from an existing asyncio loop, it falls back to running the capture in a thread pool executor to avoid the sync/async conflict.

### 4. New conviction journey script: `pipeline/review_journey.py`

A deterministic Playwright scenario that walks the core user journey:
1. Open homepage
2. Navigate to results
3. Open a property detail page
4. Verify conviction sections are present (Why this property, Price vs median, Market activity, etc.)
5. Save the property
6. Save a second property
7. Open shortlist — verify Quick Compare, Decision Themes, Best For, Greenery theme
8. Remove from shortlist

### 5. Backend smoke tests: `pipeline/smoke_test.py`

API-level smoke tests covering:
- `GET /api/health`
- `GET /api/properties` (list)
- `GET /api/properties/:id` (valid — checks property/society/area join + conviction fields)
- `GET /api/properties/:id` (invalid — expects 404)
- `GET /api/areas`
- `GET /api/shortlist`

## Default capture targets

The capture pipeline captures these pages by default:

| Name | Path |
|---|---|
| landing | `/` |
| results | `/results` |
| property_prop_w_001 | `/property/prop_w_001` |
| property_invalid | `/property/NONEXISTENT_ID` |
| shortlist | `/shortlist` |

## Where artifacts are saved

```
pipeline/feedback/day11/
  landing.png
  landing.txt
  landing.status.json
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
  capture_summary.json
  journey_report.json
  journey_shortlist.png
  smoke_test_report.json
```

## How to run

```bash
# Backend smoke tests (requires backend on port 4000)
python3 pipeline/smoke_test.py

# Page capture (requires frontend on port 5173 or deployed URL)
python3 pipeline/review_capture.py http://localhost:5173

# Conviction journey test
python3 pipeline/review_journey.py http://localhost:5173
```
