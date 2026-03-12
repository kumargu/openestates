# Day 42: Performance — Route Splitting + Image Lazy Loading

## Goal
Split 415KB monolithic bundle into lazy-loaded route chunks + add native image lazy loading. Target ~40-50% initial load reduction.

## Product Reason
Users browse on mid-range phones over spotty connections. A single 415KB bundle means downloading all 7 pages before seeing anything. Route splitting + lazy images directly improve first impression.

## Deliverables

### 1. Lazy route loading in main.tsx
- Convert all 7 page imports to `React.lazy()`
- Wrap Routes in `<Suspense>` with minimal loading fallback
- Inside existing ErrorBoundary

### 2. ImageWithFallback: add `loading="lazy"` default
- Accept optional `loading` prop (default "lazy")
- Covers all property cards, compare panels, side panels

### 3. Hero images: `loading="eager"`
- PropertyPage hero: eager (above the fold)
- All other images: lazy (default)

### 4. Raw img tags: add loading="lazy"
- SocietySearchPage, SocietyDetailPage raw `<img>` tags

### 5. Verify bundle splitting
- npm run build → multiple chunks in dist/assets/
- Main entry chunk smaller than 415KB

## Constraints
- No new dependencies (React.lazy + Suspense built-in, loading="lazy" native HTML)
- No vite.config changes needed
- No backend changes
- ErrorBoundary outside Suspense (catch chunk load failures)

## Success Criteria
1. Multiple JS chunks in build output
2. Below-fold images load on scroll only
3. Hero images load immediately
4. No visual regressions
5. `npm run build` passes
