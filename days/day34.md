# Day 34: Mobile Responsive — Remaining Pages + Footer Nav

## Goal
Complete mobile responsive pass across ALL pages: SocietySearchPage, SocietyDetailPage, ShortlistPage. Add site-wide footer nav.

## Product Reason
Day 33 made 3 core pages responsive. 3 pages remain — any user tapping into a society or shortlist on phone hits a broken layout. This closes the gap.

## Deliverables

### 1. SocietySearchPage mobile
- CSS class `society-search-bar` — stack at 600px
- CSS class `society-card` — responsive hero height, tighter padding
- Area context grid — single column at 480px

### 2. SocietyDetailPage mobile
- Hero image: `clamp(180px, 40vw, 360px)` via CSS class
- Photos grid: 2-column at 480px
- Hero badge overlays: smaller font on mobile
- Sidebar collapse already works via existing `.property-layout`

### 3. ShortlistPage mobile
- Compare tables: smooth horizontal scroll with `mobile-scroll-x`
- Cards grid: single column at 600px
- Empty state: tighter padding on mobile

### 4. Footer nav
- Simple footer below Routes in main.tsx (not on HomePage)
- 3 nav links + wordmark + tagline
- Stack vertically on mobile

## Constraints
- Same breakpoints as Day 33: 600px, 480px
- No new dependencies, no backend changes
- Extract inline styles to CSS classes only where needed

## Success Criteria
1. SocietySearchPage search bar stacks at 600px, cards clean at 375px
2. SocietyDetailPage hero scales, photos don't overflow
3. ShortlistPage tables scrollable, cards single-column at 600px
4. Footer visible on all pages except home
5. No horizontal overflow at 320px on any page
6. Desktop layouts unchanged
7. `npm run build` passes
