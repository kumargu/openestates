# Day 23: Single Source of Truth — Data Consistency Across All Surfaces

## 1. The Problem

The product's core promise is **transparency and trust**. But right now, the same property can show different scores, labels, and data depending on which page the user is on — because data flows through multiple inconsistent paths.

A user searches "3 BHK in Whitefield", sees a "Strong match" label on a card (computed by frontend `search.ts`), clicks into it, and sees different scores on the detail page (recomputed by `compare.ts` using different logic). The shortlist compare page runs yet another scoring path. If the backend is down, some pages show fabricated sample data with random market activity numbers presented as real. Other pages silently fail.

**This is an architecture bug, not a feature bug.** The fix requires making the backend the single authority for all scores, labels, and enrichment — and making the frontend a pure rendering layer that never invents data.

---

## 2. Current State — Where Data Comes From

### 2.1 Frontend Data Sources (the problem)

| Source | Used By | Problem |
|--------|---------|---------|
| Backend API | All pages (primary) | Good — this is the authority |
| `sample-data.ts` | ResultsPageA, ShortlistPage (fallback) | 4 hardcoded properties with fake IDs ("sample_1"), stale data |
| `societies_ranked.json` | SocietySearchPage (fallback) | Static file, may not exist |
| `compare.ts` | PropertyPage, ShortlistPage | Frontend recomputes value, commute, society, greenery, risk, resale, market themes |
| `market.ts` | PropertyPage | **Generates random numbers** for `saves_last_7d` when backend doesn't provide it |
| `search.ts` | ResultsPageA | Reimplements intent parsing + filtering + match scoring when API search fails |
| `AREA_SIGNALS` (HomePage) | HomePage | Hardcoded area insights, not from API |
| `AREA_GROUPS` (ResultsPageA) | ResultsPageA | Hardcoded area aliases, not synced with backend |

### 2.2 Backend Inconsistencies

| Route | Enrichment Applied | Problem |
|-------|-------------------|---------|
| `GET /api/properties` | PropertyCard shape + KG google_rating | Lightweight, no themes/scores |
| `GET /api/properties/:id` | Full enrichment: society from KG, area from KG | Different data than card view |
| `GET /api/search` | PropertyCard + match_score + knowledge_context | Third enrichment path |
| `GET /api/areas` | AreaListItem (minimal) | Different shape than area in property detail |

A property returned by `/api/search` has a `match_label` computed by the backend. The same property on the detail page has themes computed by the frontend's `compare.ts`. These can contradict each other.

### 2.3 Data Ingestion Paths (acknowledged, not fixed today)

There are two ways enriched data enters the backend — this is a known inconsistency to address in a future day:

| Path | How | Where it writes |
|------|-----|-----------------|
| Live Discovery (`discover.py`) | Writes node JSON files directly to `data/knowledge/nodes/` on disk | Disk only — backend must restart or hot-reload to see changes |
| Skills via GraphClient | HTTP POST to `/api/knowledge/nodes/{id}/facts` | Backend in-memory KG + persists to disk |

Both paths work, but they're not unified. The GraphClient HTTP path is the better long-term design (backend stays authoritative). For now this is acceptable — today's focus is the serving layer, not the ingestion layer.

---

## 3. The Fix — Design Principles

1. **Backend owns all computation.** Scores, labels, themes, tradeoffs, market data — all computed server-side.
2. **Frontend renders, never computes.** If data isn't in the API response, it doesn't exist. No random fallbacks.
3. **One enrichment path.** Every route that returns a property runs the same enrichment function.
4. **Honest error states.** If the backend is down, say so. Don't show fake data.
5. **Progressive detail.** Card view is a subset of detail view — never contradicts it.
6. **KG facts first.** When the knowledge graph has pre-scored facts from skills (e.g., `score_maintenance_quality`, `score_family_friendly`, `overall_score`), use them directly. Only fall back to hardcoded computation when KG scores are missing.

---

## 4. Scope — Day 23 vs Day 24

This work is split across two days:

| Day | Scope | Value |
|-----|-------|-------|
| **Day 23 (today)** | Phase A (kill fallbacks) + Phase B (centralize enrichment) + Phase C (simplify frontend) | Stop lying to users. One enrichment path. Frontend becomes a renderer. |
| **Day 24** | Phase D (move theme computation to backend, KG-facts-first) | Backend computes all themes/tradeoffs. Frontend deletes compare.ts/market.ts entirely. |

Day 23 alone is high-value: it eliminates fake data, unifies enrichment, and removes the most harmful frontend fallbacks. Day 24 completes the architecture by moving scoring server-side.

---

## 5. Implementation Plan (Day 23)

### Phase A: Kill Stale Fallbacks (30 min)

**Goal:** Stop showing fake/stale data immediately. This is the most harmful issue.

#### A.1 Delete `sample-data.ts`

Remove `frontend/src/lib/sample-data.ts` entirely. It contains 4 hardcoded properties with IDs that don't match any real data.

#### A.2 Delete `societies_ranked.json` fallback

Remove the fetch fallback in `SocietySearchPage.tsx` that tries to load a static JSON file.

#### A.3 Unified error state for all pages

Every page that fetches data should have the same error behavior:
- Show a clean "Could not load data" state (use existing `PageState` component)
- No fake data, no sample data, no random numbers
- Optional "Retry" button

Pages to update:
- `HomePage.tsx` — currently silently fails, should show error state
- `ResultsPageA.tsx` — currently falls back to sample data, should show error
- `ShortlistPage.tsx` — currently falls back to sample data, should show error
- `SocietySearchPage.tsx` — currently tries static JSON, should show error

#### A.4 Kill random market data

In `market.ts`, the `computeMarketActivity()` function generates random `saves_last_7d` when the backend doesn't provide it. Remove the random fallback — if the data isn't there, don't show the widget.

**Exception: engagement stats like `saves_last_7d`, `offers_last_7d`, `interest_level` are legitimately not built yet** (no real save/offer tracking exists). These are fine to omit entirely from the UI for now. Don't fake them, don't show placeholder numbers — just hide the widget until we have real engagement tracking. This is an honest "not built yet", not a data consistency bug.

**PAUSE. Verify: all pages show clean error states when backend is down. No fake data anywhere. Frontend builds cleanly.**

---

### Phase B: Centralize Backend Enrichment (1-2 hours)

**Goal:** One function enriches a property the same way everywhere.

#### B.1 Create shared enrichment functions

In `backend/src/routes/`, create a shared module (or add to an existing helpers file):

```rust
/// Enrich a PropertyCard with KG data — used by ALL routes
fn enrich_property_card(
    property: &Property,
    societies: &[Society],
    graph: &KnowledgeGraph,
) -> PropertyCard { ... }

/// Enrich a Society with KG facts — used by property detail AND search
fn enrich_society(
    society: &Society,
    graph: &KnowledgeGraph,
) -> Society { ... }

/// Enrich an AreaProfile with KG facts — used by property detail AND area routes
fn enrich_area(
    area: &AreaProfile,
    graph: &KnowledgeGraph,
) -> AreaProfile { ... }
```

**KG-facts-first principle:** These functions should check the knowledge graph for pre-scored facts before computing anything. The skills already produce facts like:
- `score_maintenance_quality` (from `score_society` skill)
- `score_family_friendly` (from `score_society` skill)
- `overall_score` (from `score_society` skill)
- `one_line_verdict` (from `score_society` skill)
- `top_signals`, `top_cautions` (from `score_society` skill)
- `google_rating`, `google_review_count` (from `fetch_google_reviews` skill)

If the KG has these, use them directly. Only fall back to seed data scores when KG facts are absent.

#### B.2 Update all routes to use shared enrichment

- `GET /api/properties` → uses `enrich_property_card()` ← same function
- `GET /api/properties/:id` → uses `enrich_property_card()` + `enrich_society()` + `enrich_area()`
- `GET /api/search` → uses `enrich_property_card()` for each result ← same function
- `GET /api/societies/search` → uses `enrich_society()` ← same function

The key guarantee: **a property card looks identical whether it came from `/properties`, `/search`, or `/properties/:id`.**

#### B.3 Centralize slug normalization

Create a single `to_slug()` function used everywhere:

```rust
/// Canonical slug: lowercase, hyphens, no prefix
fn to_slug(id: &str) -> String {
    id.to_lowercase()
        .replace(['_', ' '], "-")
        .strip_prefix("soc-")
        .unwrap_or(&id)
        .to_string()
}
```

Replace all inline slug normalization scattered across `routes/properties.rs`, `routes/search.rs`, etc.

**PAUSE. Verify: `cargo check` passes. Hit `/api/properties` and `/api/search` — same property returns identical card data from both routes. Slug lookups work consistently.**

---

### Phase C: Simplify Frontend (1-2 hours)

**Goal:** Remove frontend fallback logic that contradicts backend data. Frontend stops computing, starts rendering.

Note: We are NOT deleting `compare.ts` or `market.ts` yet — that happens in Day 24 when the backend provides themes in the API response. Today we remove the **fallback paths** and **hardcoded data** that cause inconsistencies.

#### C.1 Remove frontend search fallback

In `ResultsPageA.tsx`, remove the fallback path that runs when the API fails:
- Remove `filterProperties()`, `computeMatch()`, `parseSearch()` usage
- Remove `AREA_GROUPS` hardcoded map
- If `/api/search` fails → show error state, not client-side filtered results

In `search.ts`, remove or deprecate the fallback functions. Keep `formatSearchSummary()` if it's purely for display formatting.

#### C.2 Remove hardcoded area data from frontend

In `HomePage.tsx`, remove the `AREA_SIGNALS` hardcoded map. Area signals should come from the `/api/areas` response. If the API doesn't return signals yet, the area cards simply don't show signal text — that's honest.

#### C.3 Remove sample data imports

After deleting `sample-data.ts` in Phase A, update all files that imported from it:
- `ResultsPageA.tsx` — remove SAMPLE_PROPERTIES import and fallback rendering
- `ShortlistPage.tsx` — remove SAMPLE_PROPERTIES import and fallback rendering

#### C.4 Stop showing fake engagement stats

In any page that uses `computeMarketActivity()` from `market.ts`:
- If the backend provides `days_on_market`, show it
- If the backend provides real engagement data, show it
- Otherwise, don't render the market activity widget at all — no random numbers

Do NOT delete `compare.ts` or `market.ts` files yet. Day 24 will do that after the backend serves themes.

#### C.5 Update TypeScript types

Add area signals to the `AreaListItem` type to prepare for API-driven signals:

```typescript
interface AreaListItem {
  id: string;
  name: string;
  median_price_per_sqft?: number;
  trend_direction?: string;
  primary_signal?: string;
  signals?: string[];  // NEW: from KG + seed data
}
```

**PAUSE. Verify: `npm run build` succeeds. No TypeScript errors. All pages render from API data only. No hardcoded area signals. No sample data. No random market numbers.**

---

### Phase D: Consistency Verification (30 min)

**Goal:** Prove that data is now consistent across all surfaces.

#### D.1 Same-property test

1. Load `/api/properties` — note the card data for property X
2. Load `/api/search?q=...` that returns property X — compare card data
3. Load `/api/properties/X` — verify card-level fields match
4. Confirm: google_rating, hero_image, transparency_tags are identical across all three

#### D.2 Error state test

1. Stop the backend
2. Load each page: HomePage, ResultsPage, PropertyPage, ShortlistPage, SocietySearchPage
3. Verify: all show clean error state, no fake data, no random numbers

#### D.3 Empty state test

1. Search for something with no results
2. Verify: clean "no results" state, no sample data fallback

**PAUSE. All tests pass. Data is consistent. The product tells the truth.**

---

## 6. Day 24 Preview — Move Computation to Backend (KG-facts-first)

Day 24 completes the architecture by adding backend-computed themes to API responses.

**The key principle for Day 24:** Don't re-port the hardcoded frontend logic to Rust. Instead, read pre-scored KG facts first.

```
compute_society_theme(property, society, graph):
  1. Check graph for "score_maintenance_quality", "score_family_friendly", "overall_score"
     → If found: use directly as theme score + use fact's display_template as reasoning
  2. Check graph for "google_rating", "google_review_count"
     → If found: fold into society theme
  3. Fallback: use seed data scores (maintenance_score, society_quality_score)
     → This shrinks to nothing as skills enrich more entities
```

Day 24 scope:
- Add `PropertyThemes`, `Tradeoffs`, `MarketActivity` structs to backend
- Create `backend/src/scoring/` module with KG-first theme computation
- Extend `PropertyDetailResponse` with `themes`, `tradeoffs`, `market_activity`
- Delete `frontend/src/lib/compare.ts` and `frontend/src/lib/market.ts`
- Frontend renders API-provided themes directly

---

## 7. Files Changed (Day 23)

### Deleted
- `frontend/src/lib/sample-data.ts`

### Modified (Backend)
- `backend/src/routes/properties.rs` — extract shared enrichment functions
- `backend/src/routes/search.rs` — use shared enrichment for result cards
- `backend/src/routes/societies.rs` — use shared enrichment
- `backend/src/routes/mod.rs` — add shared `to_slug()`, `enrich_property_card()`, `enrich_society()`, `enrich_area()`

### Modified (Frontend)
- `frontend/src/lib/types.ts` — add signals to AreaListItem
- `frontend/src/pages/HomePage.tsx` — remove AREA_SIGNALS, use API data
- `frontend/src/pages/ResultsPageA.tsx` — remove fallback search, AREA_GROUPS, sample data imports
- `frontend/src/pages/ShortlistPage.tsx` — remove sample data fallback
- `frontend/src/pages/SocietySearchPage.tsx` — remove static JSON fallback
- `frontend/src/lib/search.ts` — remove/deprecate fallback functions
- `frontend/src/lib/market.ts` — stop generating random engagement stats

---

## 8. What NOT to Build Today

- Backend theme computation (Day 24)
- Delete compare.ts or market.ts (Day 24 — backend must serve themes first)
- New scoring dimensions or AI-powered ranking improvements
- Database or storage changes
- New API endpoints
- Frontend redesign or new components
- Caching strategy changes
- Fix data ingestion inconsistency (discover.py vs GraphClient) — acknowledged, deferred

---

## 9. Success Criteria

- [ ] `sample-data.ts` deleted — no hardcoded fallback data in frontend
- [ ] No random number generation in frontend (`Math.random()` for data = gone)
- [ ] All pages show clean error state when backend is unavailable
- [ ] Shared `enrich_property_card()` used by `/properties`, `/search`, and `/properties/:id`
- [ ] Shared enrichment reads KG facts first, falls back to seed data
- [ ] Slug normalization uses a single `to_slug()` function everywhere
- [ ] Frontend search fallback (client-side filtering + scoring) removed
- [ ] `AREA_SIGNALS` and `AREA_GROUPS` removed from frontend
- [ ] Same property returns identical card data from `/properties` and `/search`
- [ ] `cargo check` + `cargo test` pass
- [ ] `npm run build` succeeds with zero errors
- [ ] End-to-end: search → click card → detail page shows consistent data

---

## 10. The Principle

After today, every piece of data the user sees traces to one path: **backend API → frontend render**. No side channels, no fallbacks, no client-side recomputation. The frontend becomes a rendering layer that trusts the backend completely.

Day 24 finishes the job: the backend computes themes using KG facts (from skills that already produce scores, verdicts, and signals), and the frontend deletes its local scoring code entirely.

This is what "transparency-first" means at the engineering level: **the system never lies, not even to itself.**
