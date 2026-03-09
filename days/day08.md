# Day 8 – Frontend–Backend Contract Stabilization and First Reliable Browser Flow

Before starting today, read:

- `CLAUDE.md`
- `LEARNING.md`
- `docs/openestates_v2_surfaces_and_data.md`
- `docs/day06_data_note.md`
- `days/day07.md`

Also inspect what was completed in Day 7:

- backend routes
- API response shapes
- current folder structure
- smoke test failures
- any cleanup notes or implementation deviations

Day 7 moved OpenEstates from static data files into a real backend boundary. That was the right architectural step.

However, the Day 8 review showed that the system is not yet stable enough at the contract layer:

- `/api/health` returned 404
- `/api/areas` returned 404
- API expectations and smoke tests were not fully aligned
- results payloads may contain too many transparency tags
- hero image handling needs graceful frontend behavior

So Day 8 should not jump straight into visual polish.

The correct next step is to make the browser flow reliable, inspectable, and contract-safe so that later UI work sits on a stable base.

## 1. Goal

The goal of Day 8 is to stabilize the frontend–backend contract and deliver the first reliable localhost browser flow across the key surfaces:

- homepage
- results page
- property detail page
- shortlist page

By the end of Day 8 we should have:

- backend endpoints aligned with actual frontend needs
- frontend pages successfully fetching and rendering backend data
- loading, empty, and error states implemented
- graceful handling of placeholder images
- a small but real browser journey that works without route confusion or broken assumptions

This day is about reliability over polish.

We are not yet optimizing the product for visual sophistication or ranking depth.

## 2. Product Reason

The product promise is not just "show listings." It is:

- clarity
- trust
- seamless movement from one step to the next

A broken route, missing endpoint, or mismatched response shape is not just a technical bug. It is a product bug because it interrupts trust and creates friction.

Day 8 matters because it fixes the layer where trust is first earned:

- the app loads
- the app responds consistently
- the pages don't silently fail
- the user can move from discovery to detail without confusion

Before we build richer results cards or transparency widgets, we need confidence that:

- all critical routes exist
- the frontend knows exactly what it will receive
- the backend returns exactly what the frontend expects

This is the foundation for a seamless customer journey.

## 3. Deliverables

By the end of Day 8, the implementation should produce the following concrete outcomes.

### 3.1 Backend route and contract fixes

Backend should support and document these endpoints clearly:

```
GET /
GET /api/health
GET /api/properties
GET /api/properties/:id
GET /api/areas
GET /api/areas/:id
GET /api/shortlist
```

Notes:

- `GET /api/health` should exist explicitly
- `GET /api/areas` should exist explicitly as an area listing endpoint
- `GET /api/areas/:id` should continue to serve full area detail
- route behavior should match smoke tests and frontend usage exactly

### 3.2 Frontend page shells wired to live backend data

The frontend should render the four page shells using live backend responses:

```
/
/results
/property/:id
/shortlist
```

Each page does not need final styling, but it must be:

- navigable
- data-driven
- reliable
- readable enough for product review

### 3.3 Loading, empty, and error states

Each page that fetches data must show explicit states for:

- loading
- request failure
- no data / empty data
- not found

Examples:

- Results page should not stay blank if fetch fails
- Property page should show a clear "property not found" state
- Area-dependent widgets should fail gracefully if area data is missing
- Shortlist page should show an empty-state message if shortlist is empty

### 3.4 Frontend image fallback behavior

Hero images currently use placeholder URLs.

Frontend must implement a small image fallback strategy so broken or placeholder images do not degrade the page badly.

At minimum:

- failed image loads should fall back to a simple placeholder block or placeholder asset
- results cards should remain visually stable even when the image is unavailable
- property detail page should not collapse due to image load failures

### 3.5 Results payload cleanup

The results endpoint currently appears to expose too many transparency tags.

Day 8 should limit results card tags to a maximum of 3 primary tags.

These should be the most decision-useful tags for the card, not all available tags.

Examples of acceptable results-card tags:

- Below area median
- Ready to move
- Low litigation risk

Do not overload the card.

### 3.6 API contract note

Create a short technical note documenting the stabilized contract.

Suggested file: `docs/day08_contract_note.md`

This note should capture:

- final endpoints exposed
- which routes are list routes vs detail routes
- what each response shape is meant to power in the frontend
- any Day 7 ambiguity that was resolved

This will reduce churn on Day 9 and Day 10.

### 3.7 Suggested file structure after Day 8

Keep this lightweight. A reasonable target shape is:

```
frontend/
  src/
    pages/
      HomePage.tsx
      ResultsPage.tsx
      PropertyPage.tsx
      ShortlistPage.tsx

    components/
      PropertyCard.tsx
      PageState.tsx
      ImageWithFallback.tsx
      AreaCard.tsx

    lib/
      api.ts
      types.ts

backend/
  src/
    routes/
      health.rs
      properties.rs
      areas.rs
      shortlist.rs
```

This is illustrative. Small variations are fine if the architecture stays explicit.

## 4. Technical Guidance

### 4.1 Backend contract stabilization

Add a dedicated health endpoint.

Expected response:

```json
{
  "service": "openestates-api",
  "status": "ok"
}
```

This should be returned by `GET /api/health`. Do not rely only on `/` for health.

### 4.2 Areas API clarification

The Day 7 plan defined `GET /api/areas/:id`, but smoke tests attempted `GET /api/areas`.

That ambiguity should be resolved now by supporting both.

#### GET /api/areas

Should return a lightweight list for homepage cards or browsing.

Example:

```json
[
  {
    "id": "whitefield",
    "name": "Whitefield",
    "median_price_per_sqft": 9200,
    "trend_direction": "up",
    "primary_signal": "Metro nearby"
  }
]
```

#### GET /api/areas/:id

Should return full area detail.

Example:

```json
{
  "id": "whitefield",
  "name": "Whitefield",
  "median_price_per_sqft": 9200,
  "trend_direction": "up",
  "trend_summary": "Prices have moved up steadily in the last 6 months.",
  "metro_access_summary": "Good metro access for most micro-pockets.",
  "traffic_summary": "Commute congestion remains a caution during peak hours.",
  "waterlogging_summary": "Localized waterlogging risk in some stretches.",
  "livability_summary": "Popular with tech corridor buyers seeking established societies."
}
```

### 4.3 Explicit frontend types

Create or refine `frontend/src/lib/types.ts`.

Do not let pages infer shapes ad hoc.

At minimum define types for:

- `PropertyCard`
- `PropertyDetailResponse`
- `AreaListItem`
- `AreaDetail`
- `ShortlistResponse`
- `ApiError`

This keeps Day 9 and Day 10 safer.

Example:

```typescript
export type PropertyCard = {
  id: string;
  title: string;
  area: string;
  price: number;
  price_per_sqft: number;
  bhk: number;
  sqft: number;
  society_name: string;
  hero_image: string | null;
  transparency_tags: string[];
};
```

### 4.4 API helper cleanup

`frontend/src/lib/api.ts` should become the single source of fetch behavior.

Add:

- typed return values
- consistent error throwing
- one base URL
- no duplicate fetch logic inside page components

Suggested functions:

```typescript
getHealth()
getProperties()
getProperty(id)
getAreas()
getArea(id)
getShortlist()
```

### 4.5 Shared page-state component

Create a tiny shared component for non-happy-path rendering.

Suggested component: `PageState.tsx`

It should support simple variants:

- loading
- error
- empty
- not_found

This avoids every page implementing ad hoc status UI.

### 4.6 Results page expectations

The results page should:

- fetch `GET /api/properties`
- render a list of property cards
- display max 3 transparency tags
- show an image fallback if image fails
- allow clicking into `/property/:id`

This is still a shell, not the final polished results experience.

Do not overbuild filters yet.

### 4.7 Property page expectations

The property detail page should:

- fetch `GET /api/properties/:id`
- render core property facts
- render joined society and area information
- handle missing property with a clear not-found state
- gracefully render if one secondary section is partially missing

This is important: the page should degrade gracefully rather than fail hard.

### 4.8 Homepage expectations

Homepage should fetch area list data from `GET /api/areas`.

It should show:

- a simple search input shell
- a few area cards (area name, one signal, one price/trend datapoint)

This keeps the product aligned with the Day 5 blueprint without forcing full search logic yet.

### 4.9 Shortlist page expectations

Shortlist page should fetch `GET /api/shortlist` and then optionally resolve property IDs against existing property data for display.

For Day 8, it is acceptable to:

- show shortlist IDs resolved into simple cards if available
- or show a clean placeholder shortlist shell if resolution is not yet implemented

But the page must not feel broken.

### 4.10 Logging and smoke-test friendliness

Add small development logging for backend routes so missing-route problems are easier to detect.

At minimum:

- log route hits in development
- log startup route registration summary if easy
- keep this lightweight and removable later

This is not about observability systems. It is about fast local diagnosis.

### 4.11 Acceptance test checklist to run before marking Day 8 complete

These should all succeed:

```
GET /                       -> 200
GET /api/health             -> 200
GET /api/properties         -> 200
GET /api/properties/:id     -> 200 for valid id, 404 for invalid id
GET /api/areas              -> 200
GET /api/areas/:id          -> 200 for valid id, 404 for invalid id
GET /api/shortlist          -> 200
```

And in browser:

- home page loads
- results page loads
- clicking a property card navigates correctly
- property detail page renders for a valid id
- invalid property id shows not-found state
- shortlist page renders some state, not a blank screen
- broken image does not break layout

## 5. Constraints

Do not implement today:

- ranking engine
- contextual search parsing
- advanced filters
- pagination
- database persistence
- shortlist mutations
- compare table
- visual polish pass
- Google review integration
- review summarization
- AI explanation generation
- backend caching

Day 8 must remain focused on:

- route correctness
- contract stability
- browser reliability
- page-state handling
- graceful fallbacks

Do not let the day expand into "build the whole product."

## 6. Success Criteria

Day 8 is successful if all of the following are true:

- backend exposes all required endpoints cleanly
- `/api/health` exists and works
- `/api/areas` exists and works
- frontend pages fetch from backend without contract confusion
- all four key pages render a meaningful shell in browser
- loading, empty, error, and not-found states are visible and intentional
- results cards show at most 3 transparency tags
- hero image failures are handled gracefully
- a contract note is written to document what was stabilized

If Day 8 is successful, OpenEstates will have crossed from "some backend plus some pages" to "a reliable, reviewable product skeleton with a usable browser journey."

That is the right foundation for Day 9.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 8 should prioritize contract stabilization before UI richness

This is a deliberate sequencing decision.

Originally, Day 8 naturally points toward frontend page shells and navigation. That remains true. But based on the Day 8 feedback, the more urgent product need is to stabilize:

- route behavior
- response shapes
- error handling
- browser reliability

Why:

- broken contracts destroy trust faster than plain styling
- later UI work becomes expensive if contracts are still moving
- the seamless customer journey starts with pages that work predictably

This is not a change in product direction. It is a correction in implementation order.

### Decision 2: Support both GET /api/areas and GET /api/areas/:id

The earlier plan only explicitly required detail lookup by id. The review exposed that both humans and smoke tests naturally expect an area-list endpoint.

Why:

- homepage area cards need a lightweight list endpoint
- smoke tests reasonably probed `/api/areas`
- adding the list route now reduces confusion later

This is a small evolution in API surface, not a strategy change.

### Decision 3: Results cards should cap transparency tags at 3

The backend appears to expose too many tags in results responses.

Why:

- transparency is not the same as dumping every signal
- too many tags reduce scan-ability
- results cards should surface only the highest-value reasons

This keeps the product aligned with the "high-signal, calm, premium" UI direction.

End of Day 8.
