# days/day12.md

# OpenEstates v2

## Day 12 – Restore Trust in the Build Loop and Ship the First Verifiable Transparency Journey

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- days/day10.md

- days/day11.md

- latest Day 11 implementation notes

- latest Day 10 and Day 11 customer journey reviews

- current pipeline/ review and deploy scripts

- current frontend routes and save/shortlist implementation

- current backend routes for /api/properties, /api/properties/:id, /api/areas, /api/health

Day 10 and Day 11 surfaced an important truth:

the product may be progressing, but our ability to verify it is still broken.

That means Day 12 should not pretend the next priority is more product breadth.

The correct priority is:

- fix the render-review loop completely

- verify the key user journey in the deployed product

- make any minimal UI or contract adjustments needed so the journey is visibly reviewable

- only then lock in the next product move

Day 12 is therefore a verification-first product day.

It is still product work, because without a trustworthy review loop we cannot honestly say OpenEstates is improving.

## 1. Goal

The goal of Day 12 is to make the first full OpenEstates transparency journey deployed, visible, reviewable, and trustworthy.

By the end of Day 12, we should be able to reliably validate this live journey:

- user lands on homepage

- user goes to results

- user opens a property detail page

- user sees transparent conviction sections

- user saves a property

- user opens shortlist

- user sees compare-oriented state

This day is successful only if all of the following are true:

- the Playwright review loop is actually fixed

- page captures succeed on deployed URLs

- screenshots and rendered text are saved

- property detail and shortlist surfaces are visibly reviewable

- save → shortlist journey is verifiable in browser automation

- product fallback states are clearly separated from tooling failures

This is the first day where trust in the iteration machine is the primary deliverable.

## 2. Product Reason

OpenEstates is a transparency-first product.

That principle applies not only to the user experience, but also to how we build it.

Right now the biggest risk is not that the product lacks one more widget.

The biggest risk is that we cannot confidently answer:

- what actually rendered in production?

- did the compare flow work?

- did the save action succeed?

- did the conviction sections really appear?

- is a failure a product bug or a review-pipeline bug?

When the review loop is broken, product learning becomes noisy and misleading.

That is dangerous because OpenEstates is being shaped through a rapid build → deploy → inspect loop.

If inspection is broken, everything downstream gets weaker:

- prioritization

- quality judgment

- carry-over planning

- UX feedback

- trust in what was shipped

Day 12 matters because it restores product observability.

There is also a second reason.

We now have enough product surface area that a real opinion can form if the live pages are visible:

- homepage promise

- results explanations

- property conviction page

- shortlist compare workspace

That is enough to assess whether OpenEstates is starting to feel differentiated.

So Day 12 is not “just fix tooling.”

It is:

make the current product visible enough to judge whether transparency is actually landing.

## 3. Deliverables

By the end of Day 12, the implementation should produce the following concrete outcomes.

### 3.1 Fully repaired deployed-page review pipeline

The review pipeline must no longer fail with the Playwright Sync-vs-Async runtime error.

It must successfully capture deployed pages using a consistent browser automation path.

For each target page, the pipeline must produce:

- screenshot

- rendered text

- structured status JSON

This is non-negotiable.

### 3.2 Clear capture-status classification

Each page capture must now result in one explicit status, such as:

- ok

- tooling_error

- navigation_error

- render_error

- product_fallback

This classification must be saved as a machine-readable artifact per page.

We should never again confuse:

- “our Playwright harness broke”
 with

- “the product showed an error state”

### 3.3 Expanded capture targets for the real product journey

The deployed review must capture at least:

- /

- /results

- /property/<valid-id>

- /property/<invalid-id>

- /shortlist

If possible, also capture a results page after a seeded shortlist/browser-state step, but that is optional.

A valid property ID must come from the seed dataset or API response, not hardcoded blindly.

### 3.4 Deterministic end-to-end browser journey script

Create or refine one deterministic browser scenario that verifies:

- open results

- open property detail

- save property

- open shortlist

- confirm shortlist reflects saved property

- confirm compare-oriented state is visible when applicable

This can be a separate journey script from the page-capture script, but it must exist and run cleanly.

### 3.5 Explicit verification of conviction surfaces on the live property page

The deployed property detail page must visibly contain, in rendered text, the following sections or equivalent headings:

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

These should be stable text anchors so both humans and automation can verify them.

### 3.6 Explicit verification of compare surfaces on the live shortlist page

The deployed shortlist page must visibly contain either:

- real compare content with saved properties
 or

- a clearly intentional compare-ready empty state

At minimum the page should make the compare purpose legible.

Preferred headings or anchors:

- Quick Compare

- Decision Themes

- Best For

If compare content depends on saved items, then the browser journey must seed or create those items first.

### 3.7 Smoke-test expansion for deeper product contracts

The smoke-test layer must be expanded beyond homepage health.

Add explicit checks for:

- GET /api/properties/:id with a valid id

- GET /api/properties/:id with an invalid id

- response shape presence for detail-page-supporting fields

- any shortlist or compare-supporting contract if backend participates

If shortlist remains frontend-local, document that clearly in test notes.

### 3.8 Review-pipeline note and artifact conventions

Create or update a note:

Plain textdocs/day12_review_loop_contract.md
This note should explain:

- what was broken before

- what is fixed now

- what capture statuses exist

- which pages are captured by default

- where screenshots/text/status artifacts are saved

- how the deterministic journey script works

- what remains out of scope

### 3.9 Minimal UI hardening only where needed for verifiability

You may make small UI adjustments if needed so the review loop can verify real product surfaces.

Allowed examples:

- add missing stable section headings

- make save-button state text clearer

- make shortlist headings explicit

- ensure compare-ready empty state contains visible compare language

- ensure greenery/open-space appears in text if already intended

Do not do a broad redesign.

## 4. Technical Guidance

### 4.1 Make the Playwright path internally consistent

The current failure strongly suggests mixed API usage.

Fix the review harness so it uses one coherent model.

Preferred approach:

- use Playwright Async API end to end inside any async execution context

- avoid mixing sync browser calls with async orchestration

- keep one clean review entrypoint for deployed-page capture

The important outcome is consistency, not framework cleverness.

If the pipeline already has async orchestration, the cleanest shape is:

PythonRunasync with async_playwright() as p:
    browser = await p.chromium.launch(...)
    page = await browser.new_page(...)
    await page.goto(url, wait_until="networkidle")
    text = await page.text_content("body")
    await page.screenshot(path=...)
Do not leave any hidden sync call path inside this flow.

### 4.2 Add a page-capture artifact model

For each page capture, save a small JSON file like:

JSON{
  "page_name": "results",
  "url": "https://...",
  "capture_status": "ok",
  "http_status": 200,
  "text_length": 1834,
  "has_screenshot": true,
  "has_rendered_text": true,
  "captured_at": "2026-03-09T12:34:56Z"
}
This becomes the ground truth for review reliability.

### 4.3 Save artifacts in a deterministic structure

Use a clean artifact layout such as:

Plain textpipeline/
  feedback/
    day12/
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
      journey.status.json
Do not bury these in ad hoc temp folders.

### 4.4 Add a render preflight check before text extraction

Before extracting content, verify:

- navigation completed

- DOM exists

- there is body content

- there is no obvious tooling/runtime crash overlay

- screenshot can be taken

If preflight fails, classify the failure and stop cleanly.

Do not continue into misleading text extraction.

### 4.5 Use a real seeded property ID for valid detail capture

Do not guess the property id.

Use one of these approaches:

- parse the first property ID from /api/properties

- read a known valid seed property ID from the dataset

- keep a tiny helper that resolves a valid detail target dynamically

This removes fragile assumptions from the review loop.

### 4.6 Add an invalid detail capture target deliberately

Also capture:

Plain text/property/does-not-exist
or an equivalent invalid ID.

This is important because not-found states are part of trust and reliability.

The invalid-property page should be visibly intentional, not blank or broken.

### 4.7 Create a deterministic browser journey script

In addition to page capture, create one deterministic journey script that does something like:

Plain text1. Open /results
2. Click first property's View Details link
3. On property page, click Save
4. Open /shortlist
5. Confirm saved property appears
6. Confirm compare-oriented section text is present
If clicking is too brittle in deployed review, use stable selectors or test ids.

This is acceptable and encouraged.

Do not build a heavy E2E framework.

One trustworthy script is enough.

### 4.8 Add stable selectors or text anchors where helpful

If current UI elements are hard to automate or verify, add small test-friendly hooks.

Acceptable additions:

- data-testid="save-button"

- data-testid="quick-compare-section"

- data-testid="market-activity-section"

Or stable visible headings like:

- Why this property for you

- Quick Compare

- Decision Themes

Prefer visible user-facing anchors when possible, because they also improve clarity for real users.

### 4.9 Expand smoke tests to deeper contracts

Add smoke coverage for:

Plain textGET /api/properties/:id     -> valid id returns 200
GET /api/properties/:id     -> invalid id returns 404
If the property detail page depends on fields such as market activity or compare themes, verify the response shape contains what the frontend expects.

Do not assume the page is correct just because /api/properties works.

### 4.10 Clarify shortlist ownership if still frontend-local

If shortlist state still lives entirely in localStorage, document that explicitly.

The Day 12 notes should make clear:

- shortlist is frontend-local

- browser automation must preserve or set that state

- no backend shortlist persistence is assumed yet

This matters because otherwise future reviewers may misread test failures.

### 4.11 Minimal UI hardening tasks allowed today

Only make UI changes that improve verifiability or obvious customer trust.

Examples that are in scope:

- add missing section headings to property detail page

- add missing compare headings to shortlist page

- improve save-button text from ambiguous icon-only behavior to visible state text

- make compare empty state explicitly about comparing saved homes

- ensure greenery/open-space text is visible if that theme was already intended

Examples that are out of scope today:

- redesign landing page

- add maps

- add new ranking engine logic

- implement real bidding backend

- add account/auth flows

- rewrite the entire compare experience

### 4.12 Suggested file touch points

These are indicative only:

Plain textpipeline/
  review_capture.py
  review_playwright.py
  agent.py

frontend/
  src/
    pages/
      PropertyPage.tsx
      ShortlistPage.tsx
    components/
      SaveButton.tsx
      CompareThemeSection.tsx
      MarketActivityWidget.tsx
      TradeoffsWidget.tsx

backend/
  src/
    routes/
      properties.rs

docs/
  day12_review_loop_contract.md
Reuse and refine existing files where possible.

Do not duplicate review logic.

## 5. Constraints

Do not build today:

- real bidding or offer engine

- transaction workflow

- authentication

- server-side shortlist persistence unless nearly done already

- broad review/data enrichment

- AI explanation expansion

- map view

- database migration

- major ranking-engine rewrite

- broad landing-page redesign

Day 12 must remain focused on:

- fixing the review loop for real

- proving deployed page capture works

- validating the key property → save → shortlist journey

- hardening verifiability of current transparency surfaces

- making current product progress inspectable and trustworthy

Do not let today turn into a general feature sprint.

## 6. Success Criteria

Day 12 is successful if all of the following are true:

- the Playwright Sync/Async error is gone

- deployed page capture succeeds for the default review set

- screenshot, text, and status artifacts are saved per page

- capture statuses clearly distinguish tooling failures from product states

- a valid property detail page is captured and reviewable

- an invalid property page is captured and reviewable

- the property detail page visibly includes: 
  Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

- the shortlist page is captured and visibly compare-oriented, or shows a clearly intentional compare-ready empty state

- a deterministic browser journey verifies: 
  results → property detail → save → shortlist

- smoke tests now include valid and invalid /api/properties/:id

- a review-loop contract note is written and checked in

If these conditions are met, Day 12 will have accomplished something critical:

OpenEstates will again be a product we can see clearly, judge honestly, and improve with confidence.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 12 prioritizes verification over new feature breadth

We are deliberately not moving on to more product scope until the current product can be reviewed reliably.

Why:

- the review loop is still the biggest blocker

- adding more features without trustworthy inspection creates false progress

- product quality now depends on observability as much as implementation

This is a sequencing correction, not a product pivot.

### Decision 2: Review artifacts are now part of the product-development contract

Screenshots, rendered text, and status JSON are no longer optional debugging extras.

They are now part of the expected daily output.

Why:

- OpenEstates is being shaped through deployed product review

- without stable artifacts, the feedback loop is too fragile

- artifact-backed review reduces ambiguity and speeds better planning

### Decision 3: Stable visible section headings are now required for core transparency surfaces

Core property and compare sections should have stable visible headings, not only visual grouping.

Why:

- this improves user comprehension

- this improves automated verification

- it prevents important transparency surfaces from becoming hard to detect or accidentally removed

### Decision 4: Day 10 and Day 11 are not considered fully landed until the deployed journey is visibly verified

Implementation and backend health are not enough anymore.

Why:

- OpenEstates is judged by the rendered product experience

- the key question is what users can actually see and do

- verification is now a first-class definition of done for major UI surfaces