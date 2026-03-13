# Day 56: Sprint 2 Polish — Fuzzy Match Fix, Separate Rate Limiter, Dashboard Interest Timeline

## Goal

Harden the seller-buyer connection pipeline by fixing the fuzzy society match to prefer the best match, separating the publish rate limiter from registration creation, and enriching the seller dashboard with per-property interest timelines and actionable completeness recommendations.

## Product Reason

Sprint 2 is feature-complete but has known rough edges that erode trust. A seller typing "Prestige Lakeside" could get matched to "The Prestige City" because the current logic returns the first `contains()` match. A burst of registration drafts could block a seller from publishing. The dashboard shows interest count but not *when* interest happened — a timeline gives sellers a sense of momentum. These are small fixes with outsized impact on seller confidence.

## Sprint Context

Day 12 of 14 in Sprint 2 (Days 45-58). All major features shipped. Days 56-58 are polish. Both `cargo check` and `npm run build` pass clean. This plan addresses feedback items from Day 53.

## Previous Day Feedback Decisions

- **Day 53: Society fuzzy match returns first match, not best match** — FIXING TODAY with longest-match scoring.
- **Day 53: Rate limiter for publish shares budget with create** — FIXING TODAY with separate publish rate limiter.
- **Day 53: No test coverage for fuzzy matching** — Acknowledged but not adding unit tests this sprint (no test harness set up). Manual verification via success criteria.
- **Day 53: Normalize area strings** — Already addressed by Day 55's AREA_ALIASES extraction. Accepting current state.

## Deliverables

### 1. Fix society fuzzy match to prefer best (longest) match

**File:** `backend/src/routes/registration.rs`

Replace the current `.find()` with a scoring approach:
- Exact match = highest priority
- Prefer the society whose name has the longest substring overlap with input
- Break ties by string length (prefer most specific match)
- If no match scores above 0, fall through to area fallback chain

### 2. Separate publish rate limiter from registration rate limiter

**Files:**
- `backend/src/state.rs` — add `publish_rate_limiter` field
- `backend/src/routes/registration.rs` — use `state.publish_rate_limiter` in `publish_registration`
- `backend/src/main.rs` — initialize `publish_rate_limiter`

Publish limit: 10 per 60s (tighter than registration's 30 per 60s).

### 3. Enrich dashboard API with per-property interest timeline

**File:** `backend/src/routes/sellers.rs`

Add `timeline: Vec<InterestTimelineEntry>` to `PropertyInterestSummary`. Each entry: `{ date: String, count: usize }` — daily bucketed interest counts for last 30 days.

### 4. Dashboard frontend: sparkline + completeness recommendations

**File:** `frontend/src/pages/SellerDashboardPage.tsx`

- Render inline SVG sparkline for properties with interest events
- Add human-readable recommendations for each incomplete step (e.g., "Add photos to rank higher in search results")

**File:** `frontend/src/lib/types.ts`
- Add `timeline?: { date: string; count: number }[]` to PropertyInterestSummary type

## Technical Guidance

- Fuzzy match: follow the "longest match" pattern from `extract_area_from_text` (already in registration.rs)
- Sparkline: pure SVG polyline, no charting library. Brand accent `#c96b4f`. 120px wide, 20px tall.
- Timeline parsing: defensive — skip malformed JSONL lines silently
- Completeness recommendations: static map in frontend, no backend change

## Constraints

- No new npm dependencies
- No new Rust crates
- No new API endpoints
- `cargo check` and `npm run build` must pass
- Dashboard API response backward-compatible (timeline is additive)

## Success Criteria

1. Fuzzy match: publishing with society_name "Prestige Lakeside" matches "Prestige Lakeside Habitat", not another Prestige society
2. Fuzzy match: exact match "Prestige Lakeside Habitat" still works
3. Publish rate limiter is independent from registration rate limiter
4. Dashboard API returns `interest_summary[].timeline` as `[{ date, count }]`
5. Dashboard frontend shows sparkline for properties with interest
6. Completeness recommendations visible for incomplete steps
7. `cargo check` passes
8. `npm run build` passes
