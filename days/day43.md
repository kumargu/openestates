# Day 43: Accessibility Foundations

## Goal
Add foundational accessibility: skip-to-content, focus-visible styles, scroll-to-top on navigation, keyboard-accessible drawer, ARIA labels, aria-live for search results.

## Product Reason
No :focus-visible styles in 2900+ lines of CSS — keyboard users can't see where focus is. No skip link, no landmarks, no focus management on route changes. For a trust-first product, accessibility is non-negotiable.

## Deliverables

### 1. .sr-only utility + :focus-visible global styles (index.css)
- `.sr-only` class for screen-reader-only content
- `:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }`
- `:focus:not(:focus-visible) { outline: none; }` for mouse users

### 2. Skip-to-content + `<main>` landmark (main.tsx)
- Skip link as first child in BrowserRouter
- Wrap route content in `<main id="main-content" tabIndex={-1}>`

### 3. FocusOnNavigate + scroll-to-top (main.tsx)
- Component using useLocation: scrollTo(0,0) + focus main on pathname change

### 4. Mobile drawer keyboard accessibility (main.tsx)
- Escape key closes drawer
- Focus trapped: close button focused on open
- Focus returns to hamburger on close
- body scroll locked when open
- aria-modal, role="dialog" when open

### 5. ARIA labels on key elements
- HomePage: search form + input aria-labels
- ResultsPageA: aria-live polite region for result count
- Shortlist/save buttons: descriptive aria-labels

## Constraints
- No new dependencies
- No backend changes
- Don't add tabindex="0" to non-interactive elements
- Don't refactor inline styles to CSS

## Success Criteria
1. Tab through HomePage — focus ring visible on every interactive element
2. Skip link visible on first Tab, jumps to main content
3. Route change scrolls to top and focuses main
4. Escape closes mobile drawer, focus returns to hamburger
5. Screen reader announces result count on search
6. `npm run build` passes
