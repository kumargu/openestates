# Day 48: Seller-Buyer Wiring — Dashboard Link, Error Feedback, Seed Interests

## Goal
Wire the seller dashboard into the buyer flow (link from SellerInfoCard), add error feedback to InterestButton, and seed realistic interest data so dashboards show non-zero counts. These are the three feedback items from Day 47 that close gaps in the connection loop.

## Product Reason
Days 45-47 built the seller data model, interest API, and seller dashboard page. But these pieces are disconnected: a buyer viewing a property has no way to reach the seller's dashboard, the interest button silently fails on errors, and every dashboard shows 0 interests because no interest data exists. Day 48 closes these three gaps to create a believable end-to-end connection flow for the first time.

## Sprint Context
Sprint 2, Day 4 of 14. The connection loop is nearly closed. After today, a buyer can: view property, see seller info, click through to seller dashboard, express interest (with error feedback), and the seller dashboard shows real interest counts.

---

## Feedback Responses (Day 47 Items)

**1. "Should the seller dashboard be linked from SellerInfoCard on PropertyPage?"**
Decision: YES. Add `seller_id` to `SellerSummary` and add a "View seller profile" link.

**2. "Should last_interest_at parse full JSONL or just regex timestamp?"**
Decision: Accept current approach.

**3. No auth on dashboard endpoint**
Decision: Accept for now. Auth comes later in sprint.

**4. Completeness guide next_steps are labels only**
Decision: Accept. Edit flows come later.

**5. No interest data exists yet (data quality)**
Decision: Seed realistic interest data today.

**6. Completeness steps boolean-only (data quality)**
Decision: Accept. Partial tracking adds complexity without clear product value at this stage.

---

## Deliverables

### 1. Add `id` to `SellerSummary` and link SellerInfoCard to Dashboard

**Backend:**
- `backend/src/models/seller.rs` — Add `pub id: String` to `SellerSummary`, update `to_summary()` to include it.

**Frontend:**
- `frontend/src/lib/types.ts` — Add `id: string` to `SellerSummary` type.
- `frontend/src/pages/PropertyPage.tsx` — Add "View seller profile" link in SellerInfoCard pointing to `/seller/${seller.id}`.

### 2. Add error feedback to InterestButton

**`frontend/src/pages/PropertyPage.tsx`:**
- Add `"error"` state to InterestButton status
- Show error message on failure: "Something went wrong. You can try again."
- Auto-clear after 3 seconds
- Button shows "Try Again" in error state

### 3. Seed interest data

Create 5 JSONL files in `data/interests/` for seller-linked properties with 12-15 total interest entries. Use realistic Indian names and timestamps spread across late Feb to early March 2026.

Target properties:
- `discovered-sarang-by-sumadhura-phase-2-3bhk` — 3 interests
- `discovered-total-environment-pursuit-of-a-radical-rhapsody-3bhk` — 2 interests
- `discovered-vaswani-starlight-3bhk` — 4 interests
- `discovered-prestige-lakeside-habitat-3bhk` — 1 interest
- `discovered-sumadhura-capitol-residences-3bhk` — 2 interests

---

## Files to Create
- `data/interests/discovered-sarang-by-sumadhura-phase-2-3bhk.jsonl`
- `data/interests/discovered-total-environment-pursuit-of-a-radical-rhapsody-3bhk.jsonl`
- `data/interests/discovered-vaswani-starlight-3bhk.jsonl`
- `data/interests/discovered-prestige-lakeside-habitat-3bhk.jsonl`
- `data/interests/discovered-sumadhura-capitol-residences-3bhk.jsonl`

## Files to Modify
- `backend/src/models/seller.rs` — add `id` to `SellerSummary` and `to_summary()`
- `frontend/src/lib/types.ts` — add `id` to `SellerSummary` type
- `frontend/src/pages/PropertyPage.tsx` — dashboard link + error feedback

---

## Constraints
- `cargo check` — zero warnings
- `cargo clippy -- -D warnings` — zero warnings
- `npm run build` — passes
- No new crate or npm dependencies

## Success Criteria
1. `cargo check` — zero warnings
2. `cargo clippy -- -D warnings` — zero warnings
3. `npm run build` — passes
4. `SellerSummary` includes `id` field in API responses
5. SellerInfoCard shows "View seller profile" link to `/seller/{seller_id}`
6. Clicking link navigates to working SellerDashboardPage
7. InterestButton shows error message on API failure with retry
8. Error auto-clears after 3 seconds
9. 5 JSONL files in `data/interests/` with 12-15 total interest entries
10. Seller dashboards show non-zero interest counts
11. `last_interest_at` displays meaningful dates
