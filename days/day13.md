# days/day13.md

# OpenEstates v2

## Day 13 – Isolate the Review Harness, Prove Live Rendering, and Unblock Real Product Judgment

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- days/day11.md

- days/day12.md

- latest Day 11 and Day 12 implementation notes

- latest Day 10, Day 11, and Day 12 customer journey reviews

- current review/capture pipeline code

- current deploy scripts

- current frontend routes and page components

- current smoke tests

- any notes on how the headless review is currently invoked

Day 10, Day 11, and Day 12 all revealed the same blocker:

the product may be shipping, but the review harness is still leaking an internal Playwright runtime failure instead of showing actual rendered product pages.

That means we should stop pretending this is a normal “build the next feature” moment.

The next correct move is to treat the review harness as a broken subsystem that needs to be isolated, simplified, and proven. Until that happens, product judgment remains unreliable.

Day 13 is therefore a verification and recovery day, not a breadth day.

We are not moving to more ranking depth, more compare sophistication, or more UI richness until we can answer one basic question with confidence:

What does the deployed product actually render?

## 1. Goal

The goal of Day 13 is to make the deployed OpenEstates frontend visibly capturable and reviewable through a simplified, trustworthy review path.

By the end of Day 13, we should have:

- a minimal, standalone deployed-page capture script that works independently of the broader pipeline

- proof that deployed pages can be opened and rendered without the Playwright Sync/Async failure

- saved artifacts for the real product surfaces: 
  landing page

- results page

- valid property detail page

- invalid property page

- shortlist page

- explicit classification of whether any remaining failures are: 
  tooling failures

- navigation failures

- product-render failures

- expected product fallback states

- a clean handoff point so Day 14 can either: 
  return to product UX work if rendering is proven, or

- continue infrastructure repair with high confidence about where the failure lives

This day is successful only if the team can finally inspect real rendered output from the deployed site or conclusively isolate why not.

## 2. Product Reason

OpenEstates is now beyond the stage where API uptime alone means meaningful progress.

The product is defined by what users can see and do:

- can they start from a calm homepage?

- can they understand results?

- can they reach conviction on a property page?

- can they save and compare homes?

Right now we cannot answer those questions because the review loop is still broken.

That creates two major risks.

### 2.1 We may be mistaking implementation activity for product progress

The backend looks stable.

Some frontend work may also be landing.

But if the review harness fails before rendering, we cannot distinguish:

- product working but review tooling broken
 from

- product actually broken in production

That ambiguity is now the biggest blocker.

### 2.2 We are spending too many days on a noisy loop

Three consecutive review cycles have shown the same runtime error. That means the current response should not be “add one more check and try again.” The right response is to reduce complexity and create a smaller trusted path.

Day 13 matters because it restores disciplined learning:

- first prove rendering

- then inspect product surfaces

- then resume product iteration

That is the fastest path back to meaningful progress.

## 3. Deliverables

By the end of Day 13, the implementation should produce the following concrete outcomes.

### 3.1 A standalone minimal review harness

Create a small, explicit deployed-page capture script whose only job is:

- launch browser

- open URL

- wait for render

- save screenshot

- save rendered text

- save status JSON

This script must not depend on the rest of the larger agent/review orchestration if that orchestration is currently suspected to be the source of the Sync/Async conflict.

Suggested file:

Plain textpipeline/capture_deployed_pages.py
This should become the new source of truth for “can we render deployed pages?”

### 3.2 A reduced capture target set with real product pages

The standalone harness must capture at least:

Plain text/
 /results
 /property/<valid-id>
 /property/<invalid-id>
 /shortlist
The valid property ID must be resolved from a real source, not guessed.

Acceptable approaches:

- read the first property ID from /api/properties

- read from seed data

- keep a tiny helper that resolves a valid property ID dynamically

### 3.3 Artifact triplet per page

For every captured page, save:

- screenshot

- rendered text

- status JSON

Required output shape:

Plain textpipeline/
  feedback/
    day13/
      landing.png
      landing.txt
      landing.status.json
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

### 3.4 Explicit status model for page capture

Each page must produce one explicit status value:

- ok

- tooling_error

- navigation_error

- render_error

- product_fallback

At minimum, each status JSON should include:

JSON{
  "page_name": "results",
  "url": "https://...",
  "capture_status": "ok",
  "http_status": 200,
  "text_length": 1820,
  "has_screenshot": true,
  "has_rendered_text": true,
  "captured_at": "2026-03-09T12:34:56Z"
}

### 3.5 One deterministic browser journey script

In addition to static page capture, add one small journey script that verifies the main flow as far as current product state allows:

Plain textresults -> property detail -> save -> shortlist
Suggested file:

Plain textpipeline/journey_property_to_shortlist.py
This script should:

- open /results

- open one property detail page

- click save if possible

- open /shortlist

- verify that saved property or compare-ready state appears

If save automation is brittle, it is acceptable to seed localStorage or browser state explicitly, but that must be documented clearly.

### 3.6 Expanded smoke tests for deeper contracts

Expand smoke tests beyond shallow health endpoints.

At minimum include:

- GET /api/properties/:id with a valid ID -> 200

- GET /api/properties/:id with an invalid ID -> 404

- GET /api/areas/:id with a valid ID -> 200 if available

- response-shape checks for detail-page fields needed by the frontend

This is important because the product is now detail-page and compare-page heavy.

### 3.7 Review-loop isolation note

Create a short note documenting the Day 13 reset of the review harness.

Suggested file:

Plain textdocs/day13_review_harness_reset.md
This note should explain:

- what repeated failure pattern was observed

- why a standalone harness was introduced

- what the minimal harness captures

- where artifacts are stored

- what remains unknown if any pages still fail

- what Day 14 should do depending on the outcome

### 3.8 Minimal UI hardening only if it helps verifiability

You may make small frontend changes only if they improve review clarity.

Allowed examples:

- add stable visible headings to core sections

- make save button text state visible

- make shortlist heading explicitly compare-oriented

- ensure invalid property page shows intentional not-found text

- ensure empty/fallback states are visibly product-owned

Do not do a broader redesign today.

## 4. Technical Guidance

### 4.1 Treat the existing review pipeline as suspect

Do not keep patching the current larger orchestration blindly.

For Day 13, assume the current integrated review path may itself be the problem.

That means:

- create a small new script

- keep imports minimal

- do not reuse complex wrapper code unless clearly safe

- prove page rendering in the simplest possible path first

This is a deliberate simplification move.

### 4.2 Use Playwright Async API consistently end to end

The repeated visible error strongly indicates sync Playwright usage is still happening inside an async execution path somewhere.

The standalone harness should use only the Async API.

Recommended shape:

PythonRunasync with async_playwright() as p:
    browser = await p.chromium.launch(headless=True)
    page = await browser.new_page()
    await page.goto(url, wait_until="networkidle")
    text = await page.text_content("body")
    await page.screenshot(path=...)
    await browser.close()
Do not call sync Playwright helpers anywhere in that path.

### 4.3 Keep the standalone harness runnable directly

The harness should be invokable in a single obvious way, for example:

Bashpython pipeline/capture_deployed_pages.py --base-url https://frontend-...vercel.app --output-dir pipeline/feedback/day13
Avoid hiding execution behind unrelated orchestration layers.

The goal is that anyone can run it and inspect whether the deployed pages render.

### 4.4 Add a preflight check before extraction

Before extracting text, verify:

- navigation completed

- page content exists

- screenshot can be taken

- body is present

- no obvious runtime/tooling error has replaced the app content

If preflight fails, classify cleanly and stop.

Do not continue into misleading extraction behavior.

### 4.5 Make valid property resolution dynamic

The valid detail capture target must be resolved from real data.

Recommended approach:

- Fetch /api/properties

- Read the first id

- Build /property/<id>

This keeps the harness aligned with the current dataset and avoids stale hardcoded IDs.

### 4.6 Add an intentionally invalid property capture

Also capture an invalid property page deliberately, for example:

Plain text/property/does-not-exist
This is useful for two reasons:

- it proves routing/not-found behavior

- it tests whether product fallback states are intentional-looking

### 4.7 Add stable page anchors if needed

If current rendered text is too ambiguous, add stable visible headings on the frontend.

Preferred anchors:

#### Property page

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

#### Shortlist page

- Quick Compare

- Decision Themes

- Best For

#### Invalid property page

- Property not found

These help both product clarity and capture verification.

### 4.8 Add test-friendly selectors only where truly needed

If journey automation is difficult, add a few explicit hooks such as:

Plain textdata-testid="save-button"
data-testid="shortlist-page"
data-testid="quick-compare-section"
data-testid="property-card-link"
Prefer visible headings first, selectors second.

### 4.9 Document shortlist ownership clearly

If shortlist is still frontend-local via localStorage, the Day 13 note must state that explicitly.

Document:

- shortlist lives in browser state

- save/shortlist verification depends on preserving browser context

- no backend persistence is assumed yet

This prevents future confusion about whether a missing shortlist item is a product bug or simply missing browser state.

### 4.10 Keep smoke tests and capture artifacts separate

Do not blend API smoke results and browser capture artifacts into one vague output.

Keep them distinct:

- smoke tests answer: “is the contract alive?”

- capture artifacts answer: “what rendered?”

- journey script answers: “what user flow worked?”

All three are needed, but they serve different purposes.

### 4.11 Suggested file touch points

These are indicative, not mandatory:

Plain textpipeline/
  capture_deployed_pages.py
  journey_property_to_shortlist.py
  smoke_test_api.py

frontend/
  src/
    pages/
      PropertyPage.tsx
      ShortlistPage.tsx
      NotFoundPage.tsx
    components/
      SaveButton.tsx
      CompareThemeSection.tsx
      PageState.tsx

docs/
  day13_review_harness_reset.md
Reuse existing files where clean, but do not force reuse if it keeps the failing complexity alive.

## 5. Constraints

Do not build today:

- ranking engine improvements

- new compare features beyond verifiability fixes

- search UX redesign

- map view

- authentication

- server-side shortlist persistence

- offer/bid workflows

- AI explanation expansion

- database migration

- broad landing-page redesign

Day 13 must remain focused on:

- isolating and fixing the review harness

- proving deployed page rendering

- validating the current live journey

- making current transparency surfaces reviewable

- turning noisy ambiguity into clear evidence

Do not let today become another mixed “tooling plus feature breadth” sprint.

## 6. Success Criteria

Day 13 is successful if all of the following are true:

- a standalone deployed-page capture script exists and runs directly

- the Playwright Sync/Async runtime error no longer appears in that standalone path

- deployed captures are produced for: 
  homepage

- results

- valid property page

- invalid property page

- shortlist

- each captured page has: 
  screenshot

- rendered text

- status JSON

- valid and invalid /api/properties/:id smoke tests are recorded explicitly

- one deterministic browser journey verifies: 
  results -> property detail -> save -> shortlist
 or clearly documents the exact remaining blocker

- the property detail page is visibly reviewable, or the failure is conclusively classified

- the shortlist page is visibly reviewable, or the failure is conclusively classified

- a Day 13 review-harness reset note is written

- by the end of the day, the team can clearly say one of two things: 
  “the deployed product is now reviewable”

- or “the remaining failure is isolated to this exact subsystem”

That clarity is the required outcome.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 13 deliberately reduces scope to isolate the failure

We are intentionally stepping back from the larger integrated review pipeline and building a smaller trusted harness.

Why:

- three consecutive days have shown the same failure pattern

- repeated partial fixes are not restoring confidence

- a smaller trusted path is now the fastest route back to real product learning

This is a debugging strategy decision, not a product pivot.

### Decision 2: “Reviewable in production” is now a hard definition-of-done requirement for major UI work

Backend health and local implementation are no longer enough for major product surfaces.

Why:

- OpenEstates is shaped through deployed review

- if a surface cannot be rendered and inspected, it is not truly landed

- this prevents false positives in progress tracking

### Decision 3: We should not add more product breadth until live rendering is proven

The next right move is not more features.

It is restoring trust in the build-review loop first.

Why:

- the current bottleneck is observability, not imagination

- more UI work without reliable review only compounds ambiguity

- once live rendering is proven, product prioritization becomes much sharper again