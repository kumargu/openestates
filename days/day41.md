# Day 41: Error Boundary + Consistent Error States + Offline Toast

## Goal
Add app-level error boundary, standardize error states across all pages, add offline detection.

## Product Reason
For a "trust and transparency" product, silent failures are poison. An unhandled exception gives a white screen. HomePage silently swallows API errors. Error UIs are inconsistent across pages. Fix all of these.

## Deliverables

### 1. ErrorBoundary component (`components/ErrorBoundary.tsx`)
- Class component catching render errors
- Graceful fallback: "Something went wrong" + Retry + Return home
- Styled consistent with PageState

### 2. Wire ErrorBoundary in main.tsx
- Wrap Routes (keep Nav outside so users can navigate away)

### 3. HomePage error banner
- When loadError: subtle amber banner "Market data temporarily unavailable. Search still works."
- Non-blocking, with Retry button

### 4. Standardize error states
- ResultsPageA, SocietySearchPage, ShortlistPage: replace inline error HTML with PageState component
- Add `context="society"` to PageState

### 5. OfflineToast (`components/OfflineToast.tsx`)
- Listen to online/offline events
- Fixed-bottom toast when offline, auto-dismiss on reconnect

## Constraints
- No external error tracking, no toast library
- No backend changes
- ErrorBoundary catches render errors only (async errors handled by per-page try/catch)

## Success Criteria
1. Throwing in any page component shows ErrorBoundary fallback, not white screen
2. HomePage shows banner on API failure, search still works
3. All error states use PageState for consistency
4. Offline toast appears when disconnected
5. `npm run build` passes
