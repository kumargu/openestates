# Day 33: Mobile-First Responsive Pass — Core Buyer Flow

## Goal

Make Home, Search Results, and Property Detail work well on mobile (320-480px) and tablets (481-768px).

## Product Reason

Property search in India is overwhelmingly mobile. The current codebase has almost no mobile CSS — nav is a horizontal bar that cramps on small screens, search form overflows on 360px, hero image dominates mobile viewport. Without this pass, the Vercel deploy is unusable for most users.

## Deliverables

### 1. Mobile Nav — Hamburger + Slide Drawer
- Below 600px: hide NavLinks, show hamburger icon
- Drawer slides in from right with stacked links + backdrop
- Logo always visible
- Pure CSS/SVG — no library

### 2. HomePage Mobile Fixes
- Search form: stack button below input on mobile
- Stats row: reduce gap, allow wrapping
- Trending strip: horizontal scroll with scroll-snap
- Micro-market cards: reduce minmax for mobile

### 3. ResultsPageA Mobile Fixes
- Results grid: CSS class instead of inline, single column below 600px
- Search refinement bar: stack on mobile
- Area signal chips: tap-friendly on mobile

### 4. PropertyPage Mobile Fixes
- Hero image: responsive height `clamp(180px, 40vw, 360px)`
- Section card padding: reduce on mobile
- Already has sidebar stacking at 900px — good

### 5. Global Mobile Utilities in index.css
- Media queries at 480px, 600px breakpoints
- Section card, page container padding adjustments
- Mobile scroll utility class

## Constraints
- Do NOT touch SocietySearchPage, SocietyDetailPage, ShortlistPage (Day 34+)
- Do NOT add npm dependencies
- Do NOT change backend code
- Replace inline styles with CSS classes where needed for responsive overrides
- This is responsive adaptation, NOT redesign

## Success Criteria
1. No horizontal scrollbar at 360px viewport
2. Search input/button usable at 360px
3. Nav accessible via hamburger at 360px
4. Property cards single column on phone, readable
5. PropertyPage hero ≤50% of mobile viewport
6. Interactive elements ≥44px tap targets
7. Trending strip scrollable, not clipped
8. `npm run build` passes
9. No desktop regressions (>1024px)
