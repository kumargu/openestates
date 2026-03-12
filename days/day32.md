# Day 32: Fix Click Flow Dead Ends + API_BASE Production Blocker

## Goal

Eliminate the four highest-severity issues from the Day 31 click flow audit: hardcoded API_BASE, society card dead ends, "Back to results" context loss, and missing 404 route.

## Product Reason

Society cards — the primary output of society discovery — are dead ends. Users see rich ranked cards but cannot click through. Meanwhile, hardcoded `API_BASE = "http://localhost:4000"` means no API call works in production. These two issues make the product non-functional outside local dev.

## Deliverables

### 1. Fix API_BASE — environment-aware configuration
- `frontend/src/lib/api.ts` — use `import.meta.env.VITE_API_BASE || "http://localhost:4000"`
- `frontend/.env.development` — `VITE_API_BASE=http://localhost:4000`
- `frontend/.env.production` — `VITE_API_BASE=` (empty for same-origin)

### 2. Society detail page — `/society/:slug`
- `frontend/src/lib/api.ts` — add `getSociety(slug)` calling `GET /api/societies/${slug}`
- `frontend/src/pages/SocietyDetailPage.tsx` (new) — full society detail with scores, signals, photos, quotes
- `frontend/src/main.tsx` — add `/society/:slug` route
- `frontend/src/pages/SocietySearchPage.tsx` — wrap SocietyCard in `<Link>`

### 3. "Back to results" preserves context
- `frontend/src/pages/PropertyPage.tsx` — use `navigate(-1)` with fallback to `/results`

### 4. 404 catch-all route
- `frontend/src/pages/NotFoundPage.tsx` (new) — "Page not found" with links to home/search
- `frontend/src/main.tsx` — add `<Route path="*">` as last route

### 5. Nav active state for society detail
- Fix nav highlight for `/society/:slug` paths

## Constraints
- Do NOT start mobile responsive pass (Day 33+)
- Do NOT change backend Rust code — endpoint already exists
- Do NOT add footer navigation (defer)
- Keep SocietyDetailPage simple v1 — use only existing API data

## Success Criteria
1. API_BASE reads from VITE_API_BASE env var
2. Society cards clickable → navigate to `/society/{slug}`
3. `/society/{slug}` renders full detail page
4. "Back" on PropertyPage uses browser history
5. `/nonexistent-page` shows 404 page
6. Nav highlights "Societies" on society detail pages
7. `npm run build` passes
