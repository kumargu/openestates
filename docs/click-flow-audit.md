# Click Flow Audit

Audited 2026-03-12. All 5 pages, every interactive element documented.

## Routing Structure (App.tsx / main.tsx)

| Route | Component | Notes |
|-------|-----------|-------|
| `/` | HomePage | Home / hero |
| `/results` | ResultsPageA | Property search results |
| `/property/:id` | PropertyPage | Property detail |
| `/societies` | SocietySearchPage | Society search + ranked cards |
| `/shortlist` | ShortlistPage | Saved properties compare |

No 404 / catch-all route defined. Unmatched routes render blank below the nav.

---

## Global Nav (main.tsx — Nav component)

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 1 | "OpenEstates" logo link | `/` (home) | Navigates to `/` | OK |
| 2 | "Properties" nav link | `/results` | Navigates to `/results` | OK |
| 3 | "Societies" nav link | `/societies` | Navigates to `/societies` | OK |
| 4 | "Shortlist" nav link | `/shortlist` | Navigates to `/shortlist` | OK |

No footer navigation on any page.

---

## HomePage

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 5 | Search bar submit (button "Search") | `/results?q=...` | Navigates to `/results?q={query}`, stores query in sessionStorage | OK |
| 6 | Search bar empty submit | `/results` | Navigates to `/results` (no query param) | OK |
| 7 | Popular search chips (x4) | `/results?q=...` | Navigates with query, stores in sessionStorage | OK |
| 8 | Trending strip buttons (up to 4) | `/results?q=...` | Navigates with trend's searchQuery | OK |
| 9 | BHK breakdown buttons (Market Pulse) | `/results?q={N}BHK` | Navigates to results filtered by BHK | OK |
| 10 | Top builder buttons (Market Pulse) | `/results?q={builder}` | Navigates to results filtered by builder name | OK |
| 11 | Featured property card (Link) | `/property/{id}` | Navigates to property detail page | OK |
| 12 | Micro-market cards (MicroMarketCard) | `/results?q={area}` | Navigates to results filtered by area | OK |

---

## ResultsPageA

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 13 | Inline search bar submit | `/results?q=...` | Updates search params, triggers backend search | OK |
| 14 | Area filter clear (X on tag) | `/results` or `/results?q=...` | Removes area filter, preserves query if present | OK |
| 15 | Property card (Link wrapper) | `/property/{id}` | Navigates to property detail | OK |
| 16 | Property card "Subscribe" button | Toggle shortlist (localStorage) | Toggles bookmark state, updates icon | OK |
| 17 | Property card "Quick view" button | Open side panel | Opens PropertySidePanel overlay | OK |
| 18 | Property card compare toggle (+) | Add to compare tray | Adds to compareIds (max 3), shows CompareBar | OK |
| 19 | Match explanation "+N more reasons" | Expand match reasons | Expands inline, prevents Link navigation | OK |
| 20 | Error state "Retry" button | Reload page | Calls `window.location.reload()` | OK |
| 21 | Error state "Return home" button | `/` | Navigates to home | OK |
| 22 | Intent chips (area, BHK, budget, prefs) | Display only | Non-interactive tags, no click handler | OK |

### PropertySidePanel (overlay from ResultsPageA)

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 23 | Close button (X) | Close panel | Closes with animation | OK |
| 24 | Backdrop click | Close panel | Closes panel | OK |
| 25 | Escape key | Close panel | Closes panel | OK |
| 26 | "Subscribe" button | Toggle shortlist | Toggles shortlist state | OK |
| 27 | "Compare" button | Add to compare | Adds property to compare set | OK |
| 28 | "Full details" link | `/property/{id}` | Navigates to property detail page | OK |

### CompareBar (floating tray from ResultsPageA)

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 29 | Remove chip (X) | Remove from compare | Removes property from compareIds | OK |
| 30 | "Compare (N)" CTA button | Open ComparePanel | Opens side-by-side comparison overlay | OK |
| 31 | "Clear" button | Clear all compare selections | Empties compareIds, closes panel | OK |

### ComparePanel (overlay from ResultsPageA)

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 32 | Close button (X) | Close panel | Closes with animation | OK |
| 33 | Backdrop click | Close panel | Closes panel | OK |
| 34 | Remove property button (X on header) | Remove from compare | Removes, closes panel if < 2 remain | OK |
| 35 | "Full details" link per property | `/property/{id}` | Navigates to property detail page | OK |

---

## PropertyPage

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 36 | "Back to results" link | `/results` | Navigates to `/results` (hardcoded) | ISSUE |
| 37 | "Save to shortlist" sidebar button | Toggle shortlist | Toggles shortlist localStorage state | OK |
| 38 | Similar property cards (Link) | `/property/{id}` | Navigates to another property detail | OK |

**ISSUE #36:** "Back to results" always links to `/results` without preserving the search query. If the user searched "3BHK Whitefield" and clicked a result, going back loses the query context. Should ideally be `/results?q={previousQuery}` or use browser history.

---

## SocietySearchPage

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 39 | Search bar submit | `/societies?q=...` | Updates search params, triggers society search | OK |
| 40 | Society card body | No link destination | Cards are non-clickable divs (`cursor: default`) | DEAD END |
| 41 | "More details" / "Show less" toggle | Expand/collapse card | Toggles expanded state inline | OK |
| 42 | Error state "Retry" button | Retry search | Re-fetches results | OK |
| 43 | Error state "Return home" button | `/` | Navigates to home | OK |

**DEAD END #40:** Society cards have no clickthrough to a society detail page. There is no `/society/:slug` route. Cards show rank, scores, signals, and resident quotes, but the user cannot navigate deeper. This is a significant gap in the society discovery flow.

---

## ShortlistPage

| # | Element | Expected Destination | Actual Behavior | Status |
|---|---------|---------------------|-----------------|--------|
| 44 | "Browse properties" CTA (empty state) | `/results` | Navigates to results page | OK |
| 45 | Quick compare table property name links | `/property/{id}` | Navigates to property detail | OK |
| 46 | "Remove" button per property in table | Remove from shortlist | Toggles shortlist, refreshes list | OK |
| 47 | "Best for" links (e.g. "Best for value") | `/property/{id}` | Navigates to the winning property | OK |
| 48 | Saved property cards (PropertyCard) | `/property/{id}` | Navigates to property detail via Link wrapper | OK |
| 49 | PropertyCard "Save" / heart button | Toggle shortlist | Toggles shortlist state | OK |
| 50 | PropertyCard "Quick view" expand toggle | Expand inline detail | Fetches detail on first expand, toggles view | OK |
| 51 | PropertyCard quick-view "Save to shortlist" | Toggle shortlist | Toggles shortlist state | OK |
| 52 | PropertyCard quick-view "Full details" link | `/property/{id}` | Navigates to property detail | OK |
| 53 | Error state "Retry" button | Reload page | Calls `window.location.reload()` | OK |
| 54 | Error state "Return home" button | `/` | Navigates to home | OK |

---

## Summary of Issues

| # | Issue | Severity | Page | Details |
|---|-------|----------|------|---------|
| 1 | No 404 catch-all route | Medium | All | Unmatched URLs render blank page with nav |
| 2 | "Back to results" loses search context | Medium | PropertyPage | Always goes to `/results` without query params |
| 3 | Society cards have no clickthrough | High | SocietySearchPage | No `/society/:slug` route exists; cards are dead ends |
| 4 | No footer navigation | Low | All | No footer on any page |
| 5 | API_BASE hardcoded to localhost:4000 | Blocker (prod) | api.ts | `const API_BASE = "http://localhost:4000"` — all API calls fail in production |
| 6 | No society detail page | High | N/A | Route does not exist; no component for `/society/:slug` |

### Stats

- **Total interactive elements audited:** 54
- **Working correctly:** 50
- **Issues found:** 4 unique issues (affecting elements #36, #40)
- **Production blocker:** 1 (API_BASE hardcoded)
