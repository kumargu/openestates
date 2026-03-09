# Day 15: Render Truth and Fallback Contract

## What "Render Truth" Means

Render truth is a verification checkpoint that answers one question:

**Did the OpenEstates app shell actually render?**

Before Day 15, the review pipeline tried to do too much at once — checking UX quality, page capture, and journey flow all in one pass. When Playwright or the deployment had issues, the entire review became ambiguous: was the *app* broken, or was the *tooling* broken?

Render truth separates these concerns by establishing ground truth *before* deeper review begins.

## How Render Truth Is Checked

### Phase 1: HTTP App-Shell Verification (no browser needed)

For each route (`/`, `/results`, `/property/:id`, `/shortlist`):

1. HTTP GET the URL
2. Verify HTTP 200 response
3. Check raw HTML for app-shell markers:
   - `<div id="root"` (React mount point)
   - `<script` tags (JS bundles present)
4. Check for OpenEstates ownership markers (e.g., "openestates", "property", "bengaluru")
5. Check for known tooling error signatures (e.g., "Playwright Sync API", "Traceback", "RuntimeError")

### Phase 2: Hydration Verification (optional, uses Playwright)

If `--with-hydration` is passed:

1. Launch headless browser
2. Navigate to each route
3. Wait for network idle + 3s hydration delay
4. Verify `#root` has substantial text content (>20 chars)
5. Report hydration status per route

### Output

`pipeline/feedback/day15/render_truth.summary.json`

## Review Gate Sequence (Day 15)

The review gate now runs four layers in strict order:

```
1. API smoke tests          → are contracts alive?
2. Render-truth check       → does the app shell render?
   └── GATE: if this fails, stop. Deeper review is skipped.
3. Page capture             → what rendered on each page?
4. Journey verification     → what user path worked?
5. Aggregate summary        → overall verdict
```

If render truth fails, the gate produces a clear `review_failed_render_truth` verdict and does not proceed to capture or journey. This prevents wasting time on UX review when the app itself isn't booting.

## Page Status Model

Every captured page produces a status JSON with one of these values:

| Status | Meaning |
|---|---|
| `ok` | Page rendered expected product content |
| `tooling_error` | Known tooling/automation signature detected (Playwright, Traceback, etc.) |
| `navigation_error` | Could not navigate to the URL |
| `render_error` | App shell missing, crash overlay, or empty body |
| `product_fallback` | App rendered intentional fallback UX (backend unavailable) |

### Status JSON Shape

```json
{
  "page_name": "results",
  "url": "http://localhost:5173/results",
  "capture_status": "product_fallback",
  "http_status": 200,
  "text_length": 932,
  "has_screenshot": true,
  "has_rendered_text": true,
  "contains_tooling_error_signature": false,
  "contains_product_fallback_heading": true,
  "captured_at": "2026-03-09T12:34:56Z"
}
```

## Known Tooling Error Signatures

If any of these appear in rendered text, the page is classified as `tooling_error` (never as product content):

- `playwright sync api`
- `traceback (most recent`
- `runtimeerror`
- `econnrefused`
- `unhandledrejection`
- `module not found`
- `syntaxerror`
- `typeerror:`
- `cannot read properties of`

## Expected Fallback Headings Per Page

### Home (`/`)
- Visible heading: **"Describe what you're looking for"**

### Results (`/results`)
- Happy path heading: **"Properties"** with result cards
- Fallback heading: **"Results temporarily unavailable"**
- Fallback copy: "We couldn't load live property data right now, but you can still continue exploring Bengaluru areas."
- Actions: Browse areas | Return home

### Property (`/property/:id`)
- Happy path heading: property title + **"Why this property for you"**
- Error fallback heading: **"Property details unavailable"**
- Error fallback copy: "This property page could not be loaded right now. You can go back to results or continue browsing other areas."
- Actions: Back to results | Browse areas
- Not-found heading: **"Property not found"**
- Not-found copy: "This listing may no longer be available or the link may be incorrect."
- Actions: Browse properties | Return home

### Shortlist (`/shortlist`)
- Happy path heading: **"Compare saved homes"** + **"Quick compare"**
- Empty state heading: **"Compare saved homes"** (page title) + **"Your shortlist is empty"**
- Empty state copy: "Save a few homes to compare value, commute, openness, and risk side by side."
- Ownership note: "Saved homes stay on this browser for now."
- Actions: Browse properties | Return home

## Artifacts Generated

```
pipeline/feedback/day15/
  render_truth.summary.json      ← render truth results
  review.summary.json            ← aggregate review verdict
  smoke_test_report.json         ← API smoke test results

  landing.png                    ← screenshot
  landing.txt                    ← rendered text
  landing.status.json            ← page status

  results.png
  results.txt
  results.status.json

  property_valid.png
  property_valid.txt
  property_valid.status.json

  property_invalid.png
  property_invalid.txt
  property_invalid.status.json

  shortlist.png
  shortlist.txt
  shortlist.status.json

  journey_report.json            ← journey step results
  journey_shortlist.png          ← shortlist screenshot from journey
```

## What Must Pass Before Day 16

Day 16 can return to product breadth (richer search, deeper ranking, more compare sophistication) **only if**:

1. Render truth passes — app shell verified on all routes
2. No tooling errors in page captures
3. All pages render either `ok` or `product_fallback` (never `render_error` or `tooling_error`)
4. Journey classifies its outcome precisely (not vaguely)
5. Fallback states are intentional, product-owned, and match the headings above

## What Remains Unknown If Render Truth Still Fails

If render truth fails, Day 16 must investigate:

- Is the Vite build producing valid HTML/JS bundles?
- Is the deployment platform (Vercel/local) serving files correctly?
- Is there a runtime JS error preventing React from mounting?
- Is CORS or API configuration blocking initial render?

The render truth check gives enough signal to narrow the investigation to one of these categories.

## Shortlist Ownership

Shortlist state is stored in browser `localStorage`. The backend `/api/shortlist` endpoint is a placeholder.

This is made explicit in the UI: "Saved homes stay on this browser for now."

This is both honest and consistent with the transparency promise.
