# days/day15.md

# OpenEstates v2

## Day 15 – Prove the App Renders Without the Review Tool, Then Lock In Product-Owned Offline and Fallback States

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- days/day13.md

- days/day14.md

- latest Day 13 and Day 14 implementation notes

- latest Day 10 through Day 14 customer journey reviews

- current review harness scripts

- current frontend route files and page-state components

- current smoke test scripts

- any current artifact folders from Day 13 and Day 14

- any current notes on how Vercel deploys the frontend and how the review harness is invoked

Day 10 through Day 14 have repeated the same lesson:

- backend health is stable

- the captured deployed experience is still showing a Playwright runtime error

- the team still does not have trustworthy proof of what the actual frontend renders

- intentional fallback UX is still unverified in the live deployed product

That means Day 15 should stop mixing too many goals together.

The next correct move is to answer two questions in strict order:

- Is the deployed frontend itself rendering correctly when viewed outside the review harness?

- If backend data is unavailable, does the frontend render calm, product-owned fallback states?

If we do not answer question 1 first, we will keep confusing:

- harness failure

- deployment/runtime configuration failure

- real product fallback behavior

Day 15 is therefore a truth-finding and product-hardening day, not a feature-breadth day.

We are not moving to richer search, deeper ranking, or more compare sophistication until we can prove that:

- the app renders as an app

- fallback states are product-owned

- the review loop is measuring reality instead of leaking its own internals

## 1. Goal

The goal of Day 15 is to establish ground-truth rendering evidence for the deployed frontend and then harden the user-facing fallback journey around that truth.

By the end of Day 15, we should have:

- proof of whether the deployed frontend renders correctly outside the failing headless-review path

- a clean separation between: 
  review harness failure

- deployed frontend runtime failure

- successful product render

- intentional product fallback

- intentional, product-owned fallback states for: 
  results

- property detail

- shortlist

- a smaller, stricter review path that only judges the product after basic render truth is established

- a clear Day 16 handoff: 
  either return to product UX depth because rendering truth is proven

- or continue deployment/runtime debugging with the exact remaining failure isolated

This day is successful only if we can say, with evidence:

- the app renders and the harness is the problem
 or

- the deployed frontend/runtime is broken here
 or

- the app renders fallback UX intentionally and predictably

Anything less is still ambiguity.

## 2. Product Reason

OpenEstates cannot keep treating “the review pipeline failed” as a generic blocker.

At this point there are two distinct products we are accidentally conflating:

- the actual OpenEstates frontend

- the tooling that tries to inspect it

That is dangerous because it prevents honest product judgment.

### 2.1 The user experience is currently unjudgeable

The visible captured text is still a Playwright runtime message, not OpenEstates UI.

That means we still cannot answer basic product questions like:

- does the homepage show a real search entry point?

- does the results page show product-owned fallback UX?

- does shortlist look compare-oriented even when empty?

- is navigation visible and calm?

Until we can see the frontend render independently of the harness, all product review remains noisy.

### 2.2 Fallback UX is now part of the actual product promise

Because the backend is still localhost-bound in deployed environments, fallback states are no longer “temporary inconvenience” states.

They are part of the product surface.

If live data is unavailable, OpenEstates should still feel:

- calm

- premium

- navigable

- trustworthy

- intentional

This is directly aligned with the transparency promise.

### 2.3 We need one layer of truth before any more sophistication

The next right move is not:

- more ranking logic

- more compare intelligence

- more widgets

The next right move is:

- prove the app renders

- make fallback UX intentional

- tighten the review loop only after that proof exists

That is the fastest route back to meaningful product progress.

## 3. Deliverables

By the end of Day 15, the implementation should produce the following concrete outcomes.

### 3.1 A render-truth check that does not depend on Playwright page inspection alone

Create a small verification path that answers:

- does the deployed homepage HTML/JS boot into the real app shell?

- do route pages render the app shell?

- are we seeing OpenEstates content or a tool/runtime artifact?

This must be separate from the current headless review harness.

Suggested files:

Plain textpipeline/check_render_truth.py
pipeline/render_truth.summary.json
This check should be deliberately narrow and should verify at least:

- the deployed frontend responds on /

- the HTML includes expected app-shell markers

- the browser console or page text does not immediately collapse into the known Playwright runtime failure

- the root layout contains recognizable OpenEstates-owned text or container markers

This is not yet a full UX review.

It is a truth check.

### 3.2 A reduced “app-shell first” review gate

Refine the review gate so it does not attempt full UX judgment before app-shell render is proven.

Suggested top-level sequence:

Plain text1. API smoke checks
2. Render-truth check
3. Page capture only if render-truth passes
4. Journey script only if page capture is meaningful
5. Aggregate summary
Suggested file:

Plain textpipeline/run_review_gate.py
If render-truth fails, the gate must stop early and classify the failure clearly.

### 3.3 Expanded smoke tests for missing critical evidence

The current smoke evidence is still too shallow.

Add and record explicit smoke coverage for:

- GET /api/properties/:id with one valid property ID

- GET /api/properties/:id with one invalid property ID

- GET /api/areas/:id with one valid area ID

- response-shape checks for detail-page-supporting fields

- response-shape checks for area-card-supporting fields

Suggested file:

Plain textpipeline/smoke_test_api.py
The output must be written in a machine-readable form, not only printed.

### 3.4 Intentional product-owned fallback UX on key pages

Regardless of whether the harness is fixed, the frontend must now visibly own the non-happy path.

Implement or refine calm, product-owned fallback states for:

- /results

- /property/:id

- /shortlist

These should be real UI states, not raw error text.

Each fallback state must include:

- a clear heading

- one-sentence explanation

- one recovery action

- one navigation path

Suggested copy direction:

#### Results fallback

- Heading: Results temporarily unavailable

- Copy: “We couldn’t load live property data right now, but you can still continue exploring Bengaluru areas.”

- Actions: 
  Browse areas

- Return home

#### Property fallback

- Heading: Property details unavailable

- Copy: “This property page could not be loaded right now. You can go back to results or continue browsing other areas.”

- Actions: 
  Back to results

- Browse areas

#### Invalid property state

- Heading: Property not found

- Copy: “This listing may no longer be available or the link may be incorrect.”

- Actions: 
  Browse properties

- Return home

#### Shortlist fallback / empty state

- Heading: Compare saved homes

- Copy: “Your shortlist is stored on this device. Save a few homes to compare value, commute, openness, and risk side by side.”

- Actions: 
  Browse properties

- Return home

### 3.5 Stable visible fallback anchors for automation and UX clarity

Add stable visible headings so both users and automation can recognize the intended page state.

Required anchors:

#### Home

- Describe what you're looking for

#### Results

- Results temporarily unavailable
 or

- Why this property

#### Property

- Property details unavailable
 or

- Property not found
 or

- Why this property for you

#### Shortlist

- Compare saved homes
 or

- Quick Compare

These headings should be actual visible text, not only hidden selectors.

### 3.6 Artifact set for render-truth and product-state verification

For each key route, save:

- screenshot

- rendered text

- status JSON

But now also save:

- render-truth JSON

- aggregate review summary JSON

Suggested artifact layout:

Plain textpipeline/
  feedback/
    day15/
      render_truth.summary.json
      review.summary.json

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

### 3.7 Fixed page-status model must be implemented and exposed

Continue using the fixed page-status model, but now make sure it is actually produced and surfaced in artifacts.

Allowed values:

- ok

- tooling_error

- navigation_error

- render_error

- product_fallback

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

### 3.8 Deterministic journey script should become fallback-aware, not vague

Refine the journey script so it can classify one of these outcomes explicitly:

- full happy path worked

- results page rendered product fallback

- property page rendered product fallback

- shortlist rendered compare-ready empty state

- tooling failed before journey could begin

Suggested file:

Plain textpipeline/journey_property_to_shortlist.py
The required target journey remains:

Plain textresults -> property detail -> save -> shortlist
But for Day 15, the critical requirement is:

- it must classify the blocker precisely rather than fail vaguely

### 3.9 One review-contract note that reflects actual Day 15 truth

Create:

Plain textdocs/day15_render_truth_and_fallback_contract.md
This note should explain:

- what “render truth” means

- how it is checked before deeper review

- what statuses exist

- what fallback headings are expected on each page

- what artifacts are generated

- what must pass before Day 16 can return to product breadth

- what remains unknown if render-truth still fails

### 3.10 Landing page minimum clarity pass, only after render truth is proven

If time remains after the render-truth and fallback work is solid, make one small landing-page improvement:

- ensure the page visibly exposes: 
  a natural-language search prompt

- one clear primary CTA

- one area-browsing entry path

This is not a redesign.

It is a minimum clarity pass so that once rendering becomes reviewable, the homepage entry point is legible.

## 4. Technical Guidance

### 4.1 Stop assuming the captured runtime error is a product page

Day 15 should explicitly treat the repeated Playwright runtime text as a known tooling signature, not as ambiguous page output.

Known signatures should include checks for strings like:

Plain textPlaywright Sync API inside the asyncio loop
Please use the Async API instead
Traceback
RuntimeError
If those appear in captured text, classify as:

- tooling_error

Do not allow such pages to be counted as product fallback or product render.

### 4.2 Add a render-truth layer before page review

The key architecture change for Day 15 is sequencing.

Before full page capture, verify:

- the deployed app shell is booting

- the browser is not immediately showing known tooling failure text

- the page contains recognizable OpenEstates-owned text or app-shell markers

This can be done with a smaller script than the full review harness.

The point is to answer:

- did the app render at all?

before asking:

- was the UX good?

### 4.3 Use Playwright Async API consistently in any remaining browser path

Any browser automation that remains in Day 15 must use Async Playwright only.

Recommended structure:

PythonRunasync with async_playwright() as p:
    browser = await p.chromium.launch(headless=True)
    page = await browser.new_page()
    await page.goto(url, wait_until="networkidle")
    text = await page.text_content("body")
    await page.screenshot(path=...)
    await browser.close()
Do not reuse any wrapper that might still hide sync calls in an async path.

### 4.4 Dynamic valid-ID resolution is required for all deeper checks

Do not hardcode valid IDs.

Recommended sequence:

Plain text1. Fetch /api/properties
2. Select first property id
3. Fetch /api/areas
4. Select first area id
5. Use those ids in smoke tests and capture targets
This should be part of the review scripts, not a manual assumption.

### 4.5 Centralize page-state rendering if still fragmented

If fallback UI is still duplicated or ad hoc, centralize it now.

Suggested component:

Plain textfrontend/src/components/PageState.tsx
Support at least:

- loading

- error

- empty

- not_found

- backend_unavailable

Each variant should support:

- heading

- description

- primary action

- secondary navigation action

This will help results, property, and shortlist stay visually consistent and trustworthy.

### 4.6 Suggested fallback copy must remain product-owned, not engineering-owned

Use copy like:

Plain textWe couldn’t load live property data right now.
You can still browse Bengaluru areas and continue exploring.
Saved homes stay on this browser for now.
Avoid copy like:

Plain textFetch failed
Unexpected error
ECONNREFUSED
Unhandled exception

### 4.7 Keep three evidence layers separate in outputs

The scripts must continue separating:

#### API smoke evidence

Answers:

- are contracts alive?

#### Render-truth evidence

Answers:

- did the app shell render?

#### Product capture / journey evidence

Answers:

- what user-facing state appeared?

- what path worked or failed?

Do not collapse all three into a single vague success/failure line.

### 4.8 Make shortlist ownership explicit in both UI and docs

If shortlist still lives in browser-local state, say so clearly.

Recommended support text on shortlist page:

Plain textSaved homes stay on this browser for now.
And document the same in the Day 15 note.

This is both honest and helpful.

### 4.9 Visible anchors are preferred over hidden selectors

Use visible headings first whenever possible.

Preferred anchors:

#### Home

- Describe what you're looking for

#### Results

- Results temporarily unavailable

- Why this property

#### Property

- Property details unavailable

- Property not found

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

#### Shortlist

- Compare saved homes

- Quick Compare

- Decision Themes

- Best For

Hidden test ids are acceptable only where interaction automation truly needs them.

### 4.10 Suggested file touch points

These are indicative, not mandatory:

Plain textpipeline/
  check_render_truth.py
  run_review_gate.py
  capture_deployed_pages.py
  journey_property_to_shortlist.py
  smoke_test_api.py

frontend/
  src/
    components/
      PageState.tsx
    pages/
      HomePage.tsx
      ResultsPage.tsx
      PropertyPage.tsx
      ShortlistPage.tsx

docs/
  day15_render_truth_and_fallback_contract.md
Prefer a smaller, clearer set of files over spreading logic across too many wrappers.

## 5. Constraints

Do not build today:

- ranking engine improvements

- richer compare logic beyond fallback and verifiability needs

- search UX redesign beyond basic homepage clarity if time remains

- map view

- authentication

- server-side shortlist persistence

- offers or bids

- AI explanation expansion

- database migration

- major visual redesign

Day 15 must remain focused on:

- proving whether the app itself renders

- separating harness failure from product reality

- shipping intentional product-owned fallback states

- strengthening the review sequence

- making Day 16 planning evidence-based again

Do not let today become another mixed feature sprint.

## 6. Success Criteria

Day 15 is successful if all of the following are true:

- a render-truth script exists and runs before deeper review

- the review gate now sequences: 
  smoke tests

- render-truth

- page capture

- journey

- valid and invalid GET /api/properties/:id smoke results are recorded explicitly

- valid GET /api/areas/:id smoke coverage is added and recorded

- screenshot, rendered text, and status JSON are saved for: 
  homepage

- results

- valid property page

- invalid property page

- shortlist

- the fixed status model is actually produced in artifacts: 
  ok

- tooling_error

- navigation_error

- render_error

- product_fallback

- results, property, and shortlist pages show intentional product-owned fallback copy when data is unavailable

- the harness can explicitly distinguish known tooling signatures from product fallback headings

- the journey script classifies blockers precisely rather than failing vaguely

- pipeline/feedback/day15/render_truth.summary.json exists

- pipeline/feedback/day15/review.summary.json exists

- docs/day15_render_truth_and_fallback_contract.md is written

- by end of day, the team can clearly state one of: 
  the deployed app shell renders and review can resume

- the deployed frontend/runtime is failing in this exact way

- the app renders product-owned fallback states and those are now verifiable

That clarity is the threshold for Day 16.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 15 introduces “render truth” as a separate checkpoint before UX review

Previously, review steps were trying to do too much at once.

Why:

- we still do not know whether the deployed app itself is rendering

- UX review is meaningless if app-shell render is not proven first

- separating render truth from UX judgment reduces ambiguity sharply

This is a sequencing correction, not a product pivot.

### Decision 2: Product-owned fallback UX is now treated as a core shipped surface, not a temporary placeholder

Because deployed data pages are still likely to face backend unavailability, fallback UX is now part of the actual product experience.

Why:

- users still need a calm, trustworthy experience on non-happy paths

- transparency includes explaining failure states clearly

- fallback quality now directly affects trust in the product

### Decision 3: Day 16 should only return to feature breadth if Day 15 proves app-shell rendering truth

We are explicitly adding a higher bar before resuming richer product work.

Why:

- multiple days of ambiguous review have already cost enough iteration quality

- more feature work without ground-truth rendering evidence would create false progress

- once rendering truth is proven, prioritization becomes much more reliable again