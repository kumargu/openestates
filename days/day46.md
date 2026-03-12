# Day 46: "I'm Interested" Button and Seller Visibility on Property Pages

## Goal
Make the buyer-seller connection flow visible in the product. Add the "I'm Interested" button to property detail pages, display seller info and property prompts where a seller is linked, and show live interest counts as social proof.

## Product Reason
Day 45 built the invisible plumbing — seller data model, interest events, API endpoints. None of it is surfaced to users yet. The fastest path to product value is making the connection flow visible: a buyer sees a property, clicks "I'm Interested", and sees that others have too. For seller-linked properties, the seller's name, verification badge, and property prompt add trust. This is Sprint 2's first user-facing deliverable and the seed of the "Connection is the revenue event" thesis.

---

## Feedback Responses (Day 45 Items)

**1. Completeness 42% minimum (not 30%)**
Decision: Accept 42% as minimum. 7 boolean fields with integer division make 30% impossible (2/7=28%, 3/7=42%). 42% still represents a meaningfully incomplete profile. Update vision.md wording.

**2. Interest ID collision risk (subsec_nanos % 10000)**
Decision: Replace with AtomicU64 counter in AppState. ID format: `{property_id}-{timestamp_millis}-{counter}`. Zero new dependencies, zero collision risk.

**3. Interest count reads JSONL on every GET**
Decision: Accept for now. Single-digit interests per property = negligible file reads. Add comment noting scaling path.

**4. No auth on POST /api/interests**
Decision: Accept for now, but add simple global rate limiting (max 60/minute). Full auth is a later Sprint 2 concern.

**5. Two sellers can claim same property**
Decision: Intentional for now. When displaying seller info, pick first verified, then highest completeness. TODO for Day 50+ to handle disputed claims.

**6. Dummy email/phone data**
Decision: Not actionable. Seed data for development. Real data flows in with seller registration later.

---

## Deliverables

### 1. Interest ID collision fix (`backend/src/routes/interests.rs`)
- Add `AtomicU64` interest counter to `AppState` in `backend/src/state.rs`
- Replace `rand_4_digits()` with `state.interest_counter.fetch_add(1, Ordering::Relaxed)`
- Interest ID format: `{property_id}-{timestamp_millis}-{counter}`

### 2. Basic rate limiting for interests (`backend/src/routes/interests.rs`)
- Add `interest_rate_limiter: RwLock<(Instant, u32)>` to AppState
- Before writing interest, check if count exceeds 60/minute. If so, return 429.
- Simple global limiter, not per-IP

### 3. Seller info on PropertyDetailResponse (`backend/src/routes/properties.rs`)
- Add `seller: Option<SellerSummary>` to the property detail JSON response
- `SellerSummary`: `{ name, verified, completeness_pct, property_prompt: Option<String>, documents_provided: Vec<String> }`
- Look up seller by matching `seller_id` or `property_ids`. Pick first verified, then highest completeness.
- Do NOT expose seller email/phone to buyers

### 4. Interest count on PropertyDetailResponse (`backend/src/routes/properties.rs`)
- Add `interest_count: u32` to the property detail response
- Read from JSONL file at response time

### 5. Frontend types update (`frontend/src/lib/types.ts`)
- Add `SellerSummary` type
- Add `seller?: SellerSummary` and `interest_count?: number` to `PropertyDetailResponse`

### 6. "I'm Interested" button on PropertyPage (`frontend/src/pages/PropertyPage.tsx`)
- States: idle → submitting → success → already_expressed
- Calls `expressInterest({ property_id })` from api.ts
- Store expressed interest in localStorage to prevent duplicates
- Show interest count: "{N} buyers interested"

### 7. Seller info display on PropertyPage (`frontend/src/pages/PropertyPage.tsx`)
- SellerInfoCard in sidebar: seller name, verified badge, completeness bar
- If property_prompt exists, show as quoted block
- If documents_provided, show document badges
- If no seller linked, show existing "Claim this property" section

---

## Technical Guidance
- `SellerSummary` lives in `backend/src/models/seller.rs`
- In `get_property_detail`, look up seller by iterating `state.sellers` matching property ID
- Rate limiter: `(Instant, u32)` tuple behind `RwLock`, reset every 60 seconds
- InterestButton follows same state machine pattern as ClaimSection
- Use localStorage for deduplication (no auth needed)
- SellerInfoCard follows TransparencyScoreTile styling

## Constraints
- `cargo check` and `cargo clippy -- -D warnings` — zero warnings
- `npm run build` — passes
- No new crate dependencies
- No new frontend pages — only modifications to PropertyPage
- Seller email/phone must NOT be in buyer-facing APIs
- All new components must work on mobile

## Success Criteria
1. `cargo check` — zero warnings
2. `cargo clippy -- -D warnings` — zero warnings
3. `npm run build` — passes
4. Property detail page shows "I'm Interested" button in sidebar
5. Clicking "I'm Interested" calls POST /api/interests and shows success state
6. Revisiting page shows "Interest sent" (localStorage check)
7. Interest count displays below button
8. Properties with linked seller show SellerInfoCard with name + verified badge
9. Properties with seller property_prompt show it as quoted block
10. Properties without seller show "Claim this property" (unchanged)
11. Interest IDs use atomic counter (no collision risk)
12. POST /api/interests returns 429 if rate limit exceeded
13. Seller email/phone NOT present in PropertyDetailResponse
