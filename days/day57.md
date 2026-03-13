# Day 57: Sprint 2 Hardening — Bug Fixes, Test Coverage, Polish

## Goal

Harden the seller-buyer connection pipeline by fixing known bugs from feedback, adding test coverage for critical matching logic, and polishing the end-to-end flow for Sprint 2 wrap-up readiness.

## Product Reason

Day 58 is the final day of Sprint 2. Day 57 must close all known bugs and add defensive tests so the sprint ships clean. The fixes are small individually but compound into a more trustworthy system: correct ranking behavior for seller properties, defensive UI rendering, accurate interest timestamps, and test-backed matching logic.

## Sprint Context

Day 13 of 14 in Sprint 2 (Days 45-58). All features shipped. Day 56 completed polish. Day 57 is hardening — fix bugs, add tests, verify end-to-end.

## Previous Day Feedback Decisions

- **Day 54: Completeness boost is fixed (+0.05/+0.02) regardless of raw score** — FIXING TODAY with multiplicative boost.
- **Day 54: sellers.clone() in get_property** — FIXING TODAY with scoped read-lock extraction.
- **Day 55: Trust bar stats on /sell page** — DEFER. Keep static for Sprint 2.
- **Day 55: /register redirect to /sell** — KEEP BOTH. /sell is marketing landing, /register is direct action.
- **Day 55: AREA_ALIASES is Bengaluru-specific** — DEFER. Add code comment noting limitation.
- **Day 55: No integration test for area extraction** — ADDING TODAY.
- **Day 56: Sparkline for zero-interest properties** — Already handled (checks interest_count > 0).
- **Day 56: Sparkline div-by-zero** — FIXING TODAY.
- **Day 56: No test harness for fuzzy match** — ADDING TODAY.
- **Day 56: 30-day timeline empty for seed data** — ACCEPT. By design.
- **Day 56: last_timestamp is file-order, not chronological** — FIXING TODAY.

## Deliverables

### 1. Fix: Completeness boost — multiplicative instead of additive

**File:** `backend/src/search/text.rs`

Change from additive (+0.05/+0.02) to multiplicative (*1.05/*1.02). A property with raw score 0.10 gets boosted to 0.105, not 0.15.

### 2. Fix: Avoid `sellers.clone()` in `get_property`

**File:** `backend/src/routes/properties.rs`

Replace full clone of sellers vector with scoped read-lock that extracts only the matching SellerSummary.

### 3. Fix: Sparkline div-by-zero guard

**File:** `frontend/src/pages/SellerDashboardPage.tsx`

Use `Math.max(counts.length - 1, 1)` as divisor.

### 4. Fix: `build_interest_timeline` — track chronologically latest timestamp

**File:** `backend/src/routes/sellers.rs`

Compare timestamps and keep the maximum instead of overwriting with last line.

### 5. Add: Unit tests for fuzzy society match scoring

**File:** `backend/src/routes/registration.rs`

Extract fuzzy match logic into standalone function. Add tests:
- Exact match returns correct society
- Substring prefers longest match ("Prestige Lakeside" → "Prestige Lakeside Habitat", not "The Prestige City")
- No match returns None

### 6. Add: Unit tests for area extraction from prompt text

**File:** `backend/src/routes/registration.rs`

Add tests for `extract_area_from_text`:
- "Beautiful 3BHK near ITPL Whitefield" → "Whitefield"
- "Corner flat in Sarjapur Road" → "Sarjapur Road"
- "Beautiful flat with sunrise views" → None

### 7. Add: AREA_ALIASES city-scope comment

**File:** `backend/src/search/intent.rs`

Add comment noting Bengaluru-specificity and multi-city restructuring need.

### 8. E2E smoke test (manual)

Verify: create registration → fill steps → publish → search discovery → dashboard → interest → dashboard reflects interest.

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/search/text.rs` | Multiplicative completeness boost |
| `backend/src/routes/properties.rs` | Eliminate sellers.clone() |
| `backend/src/routes/sellers.rs` | Fix last_timestamp tracking |
| `backend/src/routes/registration.rs` | Extract fuzzy_match_society, add tests |
| `backend/src/search/intent.rs` | Add city-scope comment |
| `frontend/src/pages/SellerDashboardPage.tsx` | Sparkline div-by-zero guard |

## Success Criteria

1. `cargo check` passes
2. `cargo test` passes (including new fuzzy match + area extraction tests)
3. `npm run build` passes
4. Sparkline renders correctly for 1-entry timeline (no NaN/Infinity)
5. Completeness boost: property with raw score 0.10 and 70% completeness gets ~0.105, not 0.15
6. Fuzzy match test: "Prestige Lakeside" matches "Prestige Lakeside Habitat", not "The Prestige City"
7. Area extraction test: "Beautiful 3BHK near ITPL Whitefield" extracts "Whitefield"
8. `get_property` no longer clones full sellers vector
9. `last_timestamp` reflects chronologically latest interest, not last line
