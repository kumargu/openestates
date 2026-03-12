# Day 31: CI Gate + Click Flow Audit + SEO Foundation

## Goal

Establish the CI safety net and audit the buyer-facing click flow so the rest of Sprint 1 builds on verified ground.

## Product Reason

Sprint 1 is about making the buyer experience shareable, trustworthy, and mobile-ready. Before touching UI, we need: (1) a CI gate that prevents regressions across 14 days of rapid changes, and (2) a concrete inventory of every dead end and broken link across all 5 pages, so subsequent days fix real issues instead of guessing. The SEO meta tag foundation unblocks shareable property pages early.

## Deliverables

### 1. CI Gate — GitHub Actions workflow (`.github/workflows/ci.yml`)

- Trigger on push to `main` and on pull requests
- Two jobs: `backend-check` and `frontend-build`
  - `backend-check`: `cargo check` in `backend/`
  - `frontend-build`: `npm ci && npm run build` in `frontend/`
- Fail fast — if either job fails, the workflow fails
- No deployment step (Vercel handles that separately)

### 2. Click Flow Audit (`docs/click-flow-audit.md`)

Manual walkthrough of all 5 pages documenting every interactive element. Markdown table with: Page, Element, Expected Destination, Actual Behavior, Status (ok / dead-end / broken / missing).

Pages: HomePage, ResultsPageA, PropertyPage, SocietySearchPage, ShortlistPage.

Known issues to investigate:
- Society cards on SocietySearchPage may have no clickthrough
- PropertyPage "Back to results" may lose search query context
- No footer navigation on any page
- API_BASE hardcoded to localhost (blocker for production)

### 3. SEO Meta Tag Foundation

- Install `react-helmet-async`
- Wrap `<App>` in `<HelmetProvider>` in `main.tsx`
- Default `<Helmet>` with title, description, OG tags
- Dynamic `<Helmet>` on `PropertyPage.tsx` with property-specific title and OG tags

## Files to Create/Modify

- `.github/workflows/ci.yml` (new)
- `docs/click-flow-audit.md` (new)
- `frontend/package.json` (add react-helmet-async)
- `frontend/src/main.tsx` (HelmetProvider + default Helmet)
- `frontend/src/pages/PropertyPage.tsx` (dynamic Helmet)

## Constraints

- Do NOT start the mobile responsive pass (Days 32-33)
- Do NOT add structured data / JSON-LD (mid-sprint)
- Do NOT fix click-flow issues — just document them
- Do NOT change routing structure — note gaps only
- Do NOT fix API_BASE hardcoding — document as blocker

## Success Criteria

1. `cargo check` succeeds in `backend/` and `npm run build` succeeds in `frontend/`
2. `docs/click-flow-audit.md` covers all 5 pages with 25+ interactive elements audited
3. SEO meta tags visible in browser dev tools on home page and property detail page
4. No regressions — `npm run build` still passes after all changes
