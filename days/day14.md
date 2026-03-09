# days/day14.md

# OpenEstates v2

## Day 14 – Separate Tooling Failure from Product Reality, Ship Intentional Fallback UX, and Establish a Hard Review Gate

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- days/day12.md

- days/day13.md

- latest Day 12 and Day 13 implementation notes

- latest Day 10, Day 11, Day 12, and Day 13 customer journey reviews

- current review harness scripts

- current frontend routes and fallback states

- current smoke tests

- any artifact folders produced by Day 12 or Day 13

- any current notes on how the capture harness is invoked in CI or locally

Day 10 through Day 13 have now shown the same pattern too many times:

- backend health is stable

- the deployed customer journey is still not actually reviewable

- the visible output from review is still an internal Playwright/runtime failure

- we are still unable to tell, with confidence, what the real product renders

That means Day 14 should not be another “try to fix the harness and hope.”
It should create a hard separation between:

- review-tool failure

- backend-unavailable product fallback

- successful product rendering

And it should make those states visible, explicit, and impossible to confuse.

Day 14 is therefore not about adding more feature breadth.
It is about restoring truthfulness in the build-review loop and ensuring that, even when backend data is unavailable, the user sees a calm, intentional OpenEstates-owned experience instead of something that feels broken.

## 1. Goal

The goal of Day 14 is to establish a hard, trustworthy review gate for OpenEstates and make all non-happy-path states explicitly product-owned.

By the end of Day 14, we should have:

- a review harness that can clearly prove one of three outcomes for every page: 
  tooling failed

- product rendered with intentional fallback state

- product rendered normally

- a deployed frontend that shows intentional, premium, calm fallback states when backend data is unavailable

- per-page review artifacts that make it impossible to confuse internal automation failure with user-facing UX

- a hard pre-Day-15 gate so no one claims product progress unless deployed rendering is actually verified

This day is successful only if the team can answer, with evidence:

- did the review harness fail?

- or did OpenEstates render a designed fallback?

- or did the full page render successfully?

That clarity is now more important than adding one more widget.

## 2. Product Reason

OpenEstates is a transparency-first product.

That principle now applies at two levels:

### 2.1 User-facing transparency

A user should never wonder:

- why a page is empty

- whether data failed to load

- whether the product is broken

- what they should do next

If the backend is unavailable, OpenEstates should still feel:

- calm

- intentional

- trustworthy

- navigable

Right now the captured experience does not meet that bar. It reads like leaked internal tooling.

### 2.2 Team-facing transparency

The team should never wonder:

- is the harness broken?

- is the deployed page broken?

- is the fallback state intentional?

- did the user journey actually render?

Three-plus consecutive days of ambiguity is now itself a product-development failure.

So Day 14 matters because it restores truth in the iteration loop:

- tooling failure must look like tooling failure

- product fallback must look like product fallback

- successful render must be evidenced, not assumed

Only then can we return to feature depth with confidence.

## 3. Deliverables

By the end of Day 14, the implementation should produce the following concrete outcomes.

### 3.1 A hard review-gate script with explicit pass/fail criteria

Create or refine one top-level review command that does all of the following in order:

- runs deeper API smoke tests

- runs deployed page capture

- runs one deterministic journey scenario

- writes a machine-readable summary

- exits clearly as: 
  review_passed

- review_failed_tooling

- review_failed_product

- review_failed_mixed

Suggested file:

Plain textpipeline/run_review_gate.py
This should become the single “truth entrypoint” for deployed review.

### 3.2 Stronger per-page status model

Every reviewed page must produce an explicit status from this fixed set:

- ok

- tooling_error

- navigation_error

- render_error

- product_fallback

Each page status file must also include evidence fields so the classification is auditable.

Minimum shape:

JSON{
  "page_name": "results",
  "url": "https://frontend-...vercel.app/results",
  "capture_status": "product_fallback",
  "http_status": 200,
  "text_length": 932,
  "has_screenshot": true,
  "has_rendered_text": true,
  "contains_tooling_error_signature": false,
  "contains_product_fallback_heading": true,
  "captured_at": "2026-03-09T12:34:56Z"
}

### 3.3 Product-owned fallback UX for all key data pages

The frontend must now render intentional fallback states on the deployed product for at least:

- /results

- /property/:id

- /shortlist

These fallback states should be visibly OpenEstates-owned and calm.

They should not be raw exception text.

At minimum each fallback state should include:

- a clear heading

- a short explanation in product language

- one next-step action

- one navigation recovery path

Example patterns:

- “We couldn’t load live property data right now”

- “Browse Bengaluru areas instead”

- “Return to homepage”

- “Your shortlist is stored locally on this device”

### 3.4 Stable visible headings for all key fallback states

Add stable headings that make product fallback recognizable in captured text.

Suggested fallback anchors:

#### Results page fallback

- Results temporarily unavailable

#### Property page fallback

- Property details unavailable

- or Property not found

#### Shortlist page fallback

- Your shortlist

- Compare saved homes

- or No saved homes to compare yet

These headings should be visible to both users and automation.

### 3.5 Artifact triplet plus review summary

For each reviewed page, save:

- screenshot

- rendered text

- status JSON

And also save one aggregate review summary:

Plain textpipeline/feedback/day14/review.summary.json
Suggested summary shape:

JSON{
  "review_status": "review_failed_tooling",
  "pages": [
    {
      "page_name": "landing",
      "capture_status": "tooling_error"
    },
    {
      "page_name": "results",
      "capture_status": "product_fallback"
    }
  ],
  "journey_status": "not_run",
  "smoke_test_status": "partial",
  "captured_at": "2026-03-09T12:34:56Z"
}

### 3.6 Expanded smoke tests for detail and area contracts

Add explicit smoke coverage for:

- GET /api/properties/:id with a valid property ID

- GET /api/properties/:id with an invalid property ID

- GET /api/areas/:id with a valid area ID

- response-shape checks for fields needed by: 
  property conviction sections

- shortlist compare sections

- area cards

This is non-negotiable now.

### 3.7 Deterministic journey script with explicit fallback-aware assertions

Refine the browser journey script so it can distinguish:

- true save → shortlist success

- compare-ready empty state

- missing UI

- tooling failure

It must verify one of these intentionally, not just fail vaguely.

Suggested file:

Plain textpipeline/journey_property_to_shortlist.py
Required flow target:

Plain textresults -> property detail -> save -> shortlist
If deployed backend unavailability prevents full happy-path journey, the script must at least prove:

- results shows product-owned fallback, not tooling leakage

- shortlist shows product-owned compare-ready or empty-state UX

### 3.8 Review gate note

Create a note documenting the Day 14 hardening of the review loop.

Suggested file:

Plain textdocs/day14_review_gate_contract.md
This note should explain:

- what statuses exist

- what counts as tooling failure vs product fallback

- what pages are reviewed by default

- what the fallback headings are

- what the review gate must pass before Day 15 can move back to product breadth

- what remains known/unknown after Day 14

### 3.9 Minimal UI hardening for landing page clarity

If rendering is proven again, the landing page should clearly expose the intended entry action.

At minimum, ensure visible text presence for:

- a natural-language search input or search prompt

- one calm primary CTA

- one area-browsing entry point

This is a smaller task than the review/fallback work, but it is worth tightening if time remains and only after the review gate is solid.

## 4. Technical Guidance

### 4.1 Stop treating raw error text as acceptable capture output

The current visible output is not an acceptable product state.

For Day 14, the harness must explicitly detect known tooling failure signatures such as:

- Playwright Sync API inside the asyncio loop

- similar runtime exception overlays or raw Python exception text

If those appear in rendered text, classify as:

- tooling_error

Do not let such pages be mistaken for product fallback.

### 4.2 Add text-signature checks for tooling vs product-owned fallback

The review harness should check for known strings.

#### Tooling failure signatures

Examples:

Plain textPlaywright Sync API inside the asyncio loop
Traceback
RuntimeError

#### Product fallback signatures

Examples:

Plain textResults temporarily unavailable
Property details unavailable
Compare saved homes
Return to homepage
Browse Bengaluru areas
This gives the status model teeth.

### 4.3 Make the deployed page capture path as small as possible

Do not re-entangle Day 14 with large agent orchestration.

Use a minimal, direct capture path:

PythonRunasync with async_playwright() as p:
    browser = await p.chromium.launch(headless=True)
    page = await browser.new_page()
    await page.goto(url, wait_until="networkidle")
    text = await page.text_content("body")
    await page.screenshot(path=...)
No sync Playwright calls anywhere in this execution path.

### 4.4 Keep review gate layers separate

The review gate should collect three distinct evidence types:

#### A. API smoke evidence

Answers:

- are contracts alive?

- do detail endpoints behave correctly?

#### B. Page capture evidence

Answers:

- what rendered?

- was it tooling failure, fallback, or full content?

#### C. Journey evidence

Answers:

- what user path actually worked?

Do not collapse these into one vague output.

### 4.5 Dynamic valid-ID resolution is required

Do not hardcode valid property IDs or area IDs.

Recommended approach:

- fetch /api/properties

- select the first valid property id

- fetch /api/areas

- select the first valid area id

Then use those for smoke tests and capture targets.

### 4.6 Implement intentional page-state components if not already present

If fallback UI is scattered or ad hoc, centralize it.

Suggested frontend component:

Plain textfrontend/src/components/PageState.tsx
Support at least these variants:

- loading

- error

- empty

- not_found

- backend_unavailable

This helps all pages stay calm and consistent.

### 4.7 Suggested fallback copy posture

Use product-owned language, not engineering language.

#### Good

- “We couldn’t load live property data right now.”

- “You can still browse Bengaluru areas and continue exploring.”

- “Your shortlist lives on this device.”

#### Bad

- “Fetch failed”

- “ECONNREFUSED”

- “Unexpected error”

- raw stack traces or runtime text

### 4.8 Make shortlist state explicit

If shortlist is still frontend-local, say so clearly in UI and docs.

Example UI support text:

- “Saved homes stay on this browser for now.”

This is both honest and useful.

### 4.9 Add stable user-facing anchors for key core sections

Where possible, rely on visible headings instead of only hidden test ids.

#### Landing page

- Describe what you're looking for

#### Results page

- Why this property

- or fallback heading if unavailable

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

- or compare-ready empty-state heading

These improve both UX and automated verification.

### 4.10 Suggested file touch points

These are indicative, not mandatory:

Plain textpipeline/
  run_review_gate.py
  capture_deployed_pages.py
  journey_property_to_shortlist.py
  smoke_test_api.py

frontend/
  src/
    components/
      PageState.tsx
    pages/
      ResultsPage.tsx
      PropertyPage.tsx
      ShortlistPage.tsx
      HomePage.tsx

docs/
  day14_review_gate_contract.md
Reuse existing files where clean, but prefer clarity over clever reuse.

## 5. Constraints

Do not build today:

- ranking engine improvements

- richer compare logic beyond fallback/verifiability needs

- search UX redesign beyond landing-page clarity if time remains

- map view

- authentication

- server-side shortlist persistence

- offers/bids

- AI explanation expansion

- database migration

- major visual redesign

Day 14 must remain focused on:

- separating tooling failure from product reality

- shipping intentional fallback UX

- strengthening the review gate

- proving which product states are real in deployment

- making future product review trustworthy again

Do not let today become another mixed feature sprint.

## 6. Success Criteria

Day 14 is successful if all of the following are true:

- the review gate script exists and runs as the main deployed-review entrypoint

- per-page statuses use the fixed model: 
  ok

- tooling_error

- navigation_error

- render_error

- product_fallback

- screenshot, rendered text, and status JSON are saved for: 
  homepage

- results

- valid property page

- invalid property page

- shortlist

- valid and invalid GET /api/properties/:id smoke tests are recorded explicitly

- valid GET /api/areas/:id smoke coverage is added if the endpoint exists

- the harness can explicitly detect leaked tooling errors in captured text

- data pages render intentional product-owned fallback copy when backend data is unavailable

- captured output no longer confuses tooling failure with product fallback

- shortlist page visibly reads as compare-oriented, even in empty or fallback mode

- a review summary JSON is produced for the whole run

- docs/day14_review_gate_contract.md is written

- by the end of the day, the team can clearly state: 
  the deployed product is reviewable, or

- the exact remaining failure class is isolated and evidenced

That is the threshold for Day 15.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 14 adds intentional fallback UX as a first-class product requirement

Previously, the focus was mostly on fixing the review harness. Day 14 adds a second requirement: even if backend data is unavailable, the user-facing experience must still feel calm and product-owned.

Why:

- users should never see chaos just because a dependency is unavailable

- OpenEstates must remain trustworthy in non-happy-path states

- fallback UX is part of the transparency promise

### Decision 2: “Review gate passed” is now the prerequisite for returning to product breadth

We are explicitly not moving back to more ambitious UX depth until the review gate produces trustworthy evidence.

Why:

- three-plus consecutive ambiguous days are enough

- more features without truthful inspection only create false progress

- once the gate is solid, prioritization becomes much sharper

### Decision 3: Tooling failure and product fallback must be impossible to confuse

This is now a hard architecture rule for review.

Why:

- current ambiguity has blocked real product judgment for multiple days

- the team needs binary clarity on whether the issue is internal tooling or the deployed UX

- this increases trust in both planning and execution