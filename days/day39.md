# Day 39: Search Experience Polish — Skeleton, Empty State, Recent Searches

## Goal
Make the search flow feel responsive, guided, and memorable. Fix three gaps: no loading feedback, dead-end zero results, no search memory.

## Product Reason
Search is the entry point to everything. A blank spinner or empty grid kills trust before users see a single property. Recent searches reduce friction for the iterate-compare-decide loop.

## Deliverables

### 1. Search Loading Skeleton on ResultsPage
- Replace plain "Finding properties..." with shimmer skeleton grid
- 6 ghost cards matching card-a dimensions (image + title + price + tags)
- Reuse existing `@keyframes shimmer` from index.css
- File: `frontend/src/pages/ResultsPageA.tsx`, `frontend/src/index.css`

### 2. Zero-Results Empty State with Suggestions
- When matchResults.length === 0: show "No properties match [query]"
- Suggest broadening: area-only chip, BHK-less chip
- Show popular search chips
- "Browse all properties" link
- File: `frontend/src/pages/ResultsPageA.tsx`

### 3. Recent Searches (localStorage)
- New `frontend/src/lib/recent-searches.ts`: get/add/clear, max 5, deduplicated
- HomePage: show recent chips below popular searches, "clear" button
- ResultsPageA: add search to recents on query change
- Files: `frontend/src/lib/recent-searches.ts` (new), `HomePage.tsx`, `ResultsPageA.tsx`

## Constraints
- Frontend only — no backend changes
- No new npm dependencies
- Responsive at mobile breakpoints
- Total new code under 250 lines

## Success Criteria
1. Skeleton shows within 50ms of navigation, before API response
2. Zero-result query shows suggestions instead of blank page
3. Recent searches appear on homepage after searching, clickable, clearable
4. No regressions to existing search flow
5. `npm run build` passes
