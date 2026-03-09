# days/day11.md

# OpenEstates v2

## Day 11 – Fix the Review Loop, Verify Conviction Surfaces, and Harden the Property → Compare Journey

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- days/day09.md

- days/day10.md

- latest Day 10 feedback

- latest Day 10 customer journey review

- current pipeline/deploy scripts

- current Playwright capture implementation

- any Day 10 implementation notes, especially around: 
  property detail sections

- shortlist compare flow

- local save state

- compare themes

- market activity and greenery signals

Day 10 aimed to make OpenEstates feel like a decision platform.

However, the most important outcome from the latest review is not about UI polish. It is this:

the render-review loop failed.

The deployed product could not be reviewed because the Playwright capture pipeline broke with a Sync API vs Async API error. That means we currently cannot trust the product feedback loop, and without that loop we cannot confidently judge whether Day 10 actually landed.

So Day 11 should do two things, in this order:

- repair the deployed review pipeline so the product can be rendered and judged again

- verify and harden the actual Day 10 conviction surfaces: 
  property detail page

- save flow

- shortlist compare workspace

This is a product day and an infrastructure day at the same time.
The review loop is now part of the product-building machine and must be treated as critical infrastructure.

## 1. Goal

The goal of Day 11 is to restore a trustworthy build → deploy → capture → review loop and use it to validate the most important user journey in the product so far:

results → property detail → save → shortlist → compare

By the end of Day 11, we should have:

- a fixed Playwright/browser capture pipeline that successfully renders deployed pages

- preflight checks so tooling failures are caught before product review begins

- screenshots and rendered text capture working again

- explicit verification of the property detail page and shortlist compare page

- end-to-end validation that the conviction loop works in the live deployed product

This day is successful only if both are true:

- the review system works again

- the product surfaces it reviews are real and verifiable

## 2. Product Reason

OpenEstates is now at the stage where the main risk is no longer “can we build pages?”
The main risk is:

are we actually seeing the real product clearly enough to improve it day by day?

If the review pipeline is broken:

- product judgments become guesswork

- regressions slip through

- user journey reviews lose meaning

- the team can confuse implementation progress with actual product progress

That is dangerous.

The review loop is not just tooling.
It is the mechanism by which OpenEstates stays honest.

There is also a second reason Day 11 matters.

Day 10 was supposed to make OpenEstates feel differentiated through:

- property conviction surfaces

- compare workspace

- Hinge-style match framing

- Robinhood-style market visibility

- greenery/open-space as a real theme

But we still do not know whether those surfaces truly appeared in the deployed UI because the browser capture failed.

So Day 11 is about restoring observability and proving the actual product experience, not adding a lot of new feature surface area.

## 3. Deliverables

By the end of Day 11, the implementation should produce the following concrete outcomes.

### 3.1 Fix the Playwright capture pipeline

The deployed page capture system must no longer fail with:

- It looks like you are using Playwright Sync API inside the asyncio loop

The pipeline should successfully:

- open deployed pages

- wait for them to render

- capture rendered text

- capture screenshots

- save outputs for review

### 3.2 Add capture preflight and failure classification

Before extracting text, the pipeline should run a simple preflight check and classify failures cleanly.

At minimum distinguish between:

- tooling failure (Playwright/runtime error)

- page navigation failure

- backend/product fallback state

- successful render

This prevents internal tooling errors from being mistaken for product UX.

### 3.3 Re-run live journey capture for the key pages

After the pipeline fix, capture and save outputs for at least:

- landing page

- results page

- one property detail page

- shortlist page

If possible, also capture:

- one invalid property page (/property/:id bad id) to verify not-found state

### 3.4 Verify the Day 10 conviction surfaces in the live product

The deployed property detail page must visibly render the sections Day 10 aimed to build.

At minimum verify presence of:

- Property Summary

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

- Save / Compare actions

The deployed shortlist page must visibly render:

- saved properties or a compare-ready state

- Quick Compare

- Decision Themes

- Best For summary or equivalent

- greenery/open-space as a visible compare theme if implemented

### 3.5 Harden the end-to-end save → shortlist → compare journey

The product flow must be manually and/or automatically verifiable:

- save from results

- save from property detail

- open shortlist

- compare saved properties

- remove from shortlist if implemented

This should work in the deployed product, not only locally.

### 3.6 Improve smoke tests and review artifacts

Expand the smoke/capture flow so Day 12 can trust it.

At minimum include:

- /api/properties/:id valid and invalid

- screenshot capture artifact

- rendered text artifact

- a short machine-readable capture summary

### 3.7 Write a Day 11 review-pipeline note

Create a short note documenting what was fixed in the review loop.

Suggested file:

- docs/day11_review_pipeline_note.md

This should explain:

- what was broken

- what changed

- how failures are classified now

- what pages are captured by default

- where artifacts are saved

## 4. Technical Guidance

### 4.1 Fix the Playwright Sync/Async issue correctly

The reported failure strongly suggests the current review code is using the Sync Playwright API inside an asyncio loop.

Fix this by making the review/capture path consistent.

Choose one of these approaches:

- Preferred: move the capture flow fully to Playwright Async API

- or isolate the Sync API in a clearly non-async execution path

Do not leave mixed usage.

If the pipeline already uses async orchestration elsewhere, the cleanest fix is usually:

- async_playwright

- async browser launch

- async page navigation

- async content extraction

- async screenshot save

The important thing is not which API you choose.
The important thing is that the pipeline becomes internally consistent.

### 4.2 Add a render preflight check

Before attempting text extraction, verify:

- page navigation succeeded

- DOM is present

- body text is non-empty or intentionally empty

- page did not throw a tooling/runtime error

At minimum store a status object like:

JSON{
  "page": "/results",
  "capture_status": "ok | tooling_error | navigation_error | render_error",
  "http_status": 200,
  "text_length": 1234,
  "screenshot_saved": true
}
This should be saved as a small JSON artifact per run.

### 4.3 Capture both screenshot and text

Do not rely only on rendered text.

For each reviewed page, save:

- screenshot

- rendered text

- capture status JSON

Suggested artifact layout:

Plain textpipeline/
  feedback/
    day11/
      landing.png
      landing.txt
      landing.status.json
      results.png
      results.txt
      results.status.json
      property_prop_w_001.png
      property_prop_w_001.txt
      property_prop_w_001.status.json
      shortlist.png
      shortlist.txt
      shortlist.status.json
This will make debugging and product review much more trustworthy.

### 4.4 Add page-specific capture targets

Today’s review needs more than landing/results/shortlist.

Explicitly add capture targets for:

- /

- /results

- /property/<valid-id>

- /property/<invalid-id>

- /shortlist

Use a real seeded property ID for the valid property capture.

This is critical because Day 10 was centered on the property detail page.

### 4.5 Add a small browser-journey script for conviction flow

Create a thin, deterministic browser script that can do the following:

- open homepage

- go to results

- open a property detail page

- trigger save action if possible

- open shortlist

- verify at least one saved property appears

This can be a separate Playwright scenario from the page-capture flow.

Do not overbuild end-to-end test infrastructure.
One clear deterministic scenario is enough.

### 4.6 Verify localStorage-dependent flows in deployed context

If shortlist save uses localStorage, ensure the Playwright journey script supports it correctly.

Expected checks:

- save action changes button state or UI state

- shortlist page reflects saved item(s)

- compare workspace shows more than an empty message when enough items are saved

If save actions are currently too hard to automate in deployed review, minimally:

- expose a test-friendly deterministic path

- or seed shortlist state in the browser context for review scripts

But be explicit if you do this.
Do not let the review pipeline rely on hidden assumptions.

### 4.7 Expand backend/API smoke tests to cover conviction surfaces

The current smoke tests are too shallow for the product stage we are in.

Add smoke tests for:

- GET /api/properties/:id valid

- GET /api/properties/:id invalid

- GET /api/shortlist if it exists server-side

- any compare-supporting response shape if introduced

Even if shortlist remains frontend-local, the smoke tests should clearly document that.

### 4.8 Make property detail presence verifiable

If the Day 10 sections are difficult to detect in rendered text, ensure the UI includes stable visible section headings such as:

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

This is good for both users and automated review.

Do the same for shortlist compare sections:

- Quick Compare

- Decision Themes

- Best For

This makes the product easier to review and understand.

### 4.9 Do not expand product scope too much today

Only make UI changes today if needed to support verifiability or fix obvious Day 10 misses.

Good examples:

- add stable section headings

- improve save button feedback

- make compare sections visible

- ensure greenery/open-space text appears clearly

Bad examples for today:

- redesign whole landing page

- add maps

- add auth

- rewrite ranking engine

- add real bidding backend

Today is about making the current product visible, testable, and trustworthy.

### 4.10 Suggested file touch points

Plain textpipeline/
  agent.py
  review_capture.py
  review_playwright.py

frontend/
  src/
    pages/
      PropertyPage.tsx
      ShortlistPage.tsx
    components/
      SaveButton.tsx
      CompareThemeSection.tsx
      TradeoffsWidget.tsx

backend/
  src/
    routes/
      properties.rs

docs/
  day11_review_pipeline_note.md
This is illustrative only. Reuse existing files where it keeps the architecture cleaner.

## 5. Constraints

Do not build today:

- real bidding engine

- real transactional/offer workflow

- authentication

- server-side shortlist persistence unless already very close

- broad data enrichment or scraping

- AI summarization layer expansion

- major landing-page redesign

- map view

- database migration

- heavy ranking-engine rewrite

Day 11 must remain focused on:

- fixing the review/capture loop

- verifying deployed product rendering

- validating property detail and shortlist compare surfaces

- hardening the save → shortlist → compare journey

- making current conviction surfaces clearly visible and reviewable

Do not let this day turn into a broad feature sprint.

## 6. Success Criteria

Day 11 is successful if all of the following are true:

- the Playwright/runtime error is fixed

- deployed pages can be captured successfully again

- both screenshot and rendered text artifacts are saved for reviewed pages

- capture status clearly distinguishes tooling failure from product fallback states

- live capture includes: 
  landing page

- results page

- valid property detail page

- invalid property page

- shortlist page

- property detail page visibly shows the key Day 10 conviction sections

- shortlist page visibly shows compare-oriented sections or a clearly intentional compare-ready state

- save → shortlist → compare flow is verifiable end-to-end

- /api/properties/:id valid and invalid cases are smoke-tested

- a short review-pipeline note is written documenting the fix

If these conditions are met, Day 12 can return to product depth with confidence because the build-review loop will be trustworthy again.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 11 prioritizes review-loop integrity over adding more surface area

We are intentionally not moving straight into more product features.

Why:

- the current biggest blocker is that the deployed product cannot be reliably reviewed

- without a trustworthy review loop, product iteration quality drops sharply

- fixing observability now is higher leverage than adding another surface blindly

This is not a product pivot.
It is a sequencing correction.

### Decision 2: Verifiability is now a first-class product requirement

Stable visible headings, capture artifacts, and deterministic review scenarios are now part of the product-building discipline.

Why:

- OpenEstates is being built through a deploy-and-review loop

- if important surfaces are hard to detect or verify, iteration slows down and trust in the build process falls

- clear section labeling also improves user comprehension, not just automation

### Decision 3: Day 10’s goal is not considered complete until the deployed product is visibly reviewable

Even if implementation happened, we should not assume Day 10 landed until we can actually inspect the deployed result.

Why:

- backend health alone is not enough at this stage

- the product is defined by the user-facing conviction surfaces

- Day 11 must prove those surfaces exist in the real deployed experience