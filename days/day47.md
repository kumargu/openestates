# Day 47: Seller Dashboard — Listings, Interest, and Profile Completeness

## Goal
Build the Seller Dashboard page where a seller can see their listed properties, buyer interest counts per property, and their profile completeness with guidance on what to complete next.

## Product Reason
Days 45-46 built the data model and buyer-facing connection flow (interest button, seller info card). But sellers have no way to see their own view of the platform. The dashboard is the seller's home — it answers "how are my listings doing?" and "what should I complete next?". Without it, there's no seller retention loop. This is the minimum viable seller experience: see listings → see interest → improve profile.

## Sprint Context
Sprint 2, Day 3 of 14. Days 45-46 delivered: seller data model, seed data (10 sellers), interest API, "I'm Interested" button, SellerInfoCard on PropertyPage, atomic interest IDs, rate limiting.

---

## Feedback Responses (Day 46 Items)

**1. Rate limiter uses RwLock not Mutex**
Decision: Accept. Negligible difference at this scale. Not worth changing.

**2. interest_counter starts at 0 on restart**
Decision: Accept. Combined with ms timestamps, collision is astronomically unlikely. Production would seed from existing data.

**3. Global rate limiter not per-IP**
Decision: Accept for now. Per-IP rate limiting requires tracking state per IP. Day 50+ can add IP-based limiting when auth exists.

**4. interest_counter resets on restart**
Decision: Same as #2. Accept.

**5. InterestButton label uses lowercase i**
Decision: Fix during build — capitalize "I'm Interested".

**6. count_lines duplicated in interests.rs and properties.rs**
Decision: Extract to a shared utility function during build.

**7. 42pct completeness minimum should be named constant**
Decision: Add `MIN_COMPLETENESS_PCT` constant in seller model.

---

## Deliverables

### 1. Seller Dashboard API (`backend/src/routes/sellers.rs`)
- New endpoint: `GET /api/sellers/{id}/dashboard`
- Response: `SellerDashboard { seller: SellerDetail, interest_summary: Vec<PropertyInterestSummary>, completeness_guide: CompletenessGuide }`
- `PropertyInterestSummary`: `{ property_id, property_title, interest_count, last_interest_at: Option<String> }`
- `CompletenessGuide`: `{ pct: u32, completed_steps: Vec<String>, next_steps: Vec<String> }`
- Reads interest JSONL files to compute per-property counts
- Generates completeness guide from seller's boolean flags

### 2. Extract shared `count_lines` utility (`backend/src/utils.rs`)
- Move duplicated `count_lines` from `interests.rs` and `properties.rs` to `backend/src/utils.rs`
- Create `utils` module, expose `count_lines` function
- Update both files to use the shared version

### 3. Frontend Seller Dashboard page (`frontend/src/pages/SellerDashboardPage.tsx`)
- Route: `/seller/:id`
- Three sections:
  1. **Profile Header** — seller name, verified badge, completeness percentage ring/bar
  2. **Listings** — cards for each linked property with title, area, price, interest count badge
  3. **Complete Your Profile** — checklist showing completed/incomplete steps with clear CTAs
- Mobile-first layout
- Uses existing design language (calm, premium, high-signal)

### 4. Frontend types + API client updates
- Add `SellerDashboard`, `PropertyInterestSummary`, `CompletenessGuide` types to `frontend/src/lib/types.ts`
- Add `fetchSellerDashboard(id)` to `frontend/src/lib/api.ts`

### 5. Wire route in `frontend/src/main.tsx`
- Add `/seller/:id` route pointing to SellerDashboardPage
- Lazy load the page component

### 6. Minor fixes from Day 46 feedback
- Capitalize "I'm Interested" button label
- Add `MIN_COMPLETENESS_PCT` constant in seller model

---

## Technical Guidance
- Reference `.claude/skills/add-api-endpoint.md` for endpoint pattern
- `SellerDashboard` endpoint composes existing data (seller detail + interest counts + completeness logic)
- Interest counts: iterate `data/interests/` JSONL files, filter by property_id, count lines
- Completeness guide: map each `has_*` boolean to a step name, split into completed/next_steps
- The 7 steps: basic_info, property_prompt, details, pricing, documents, photos, society_info
- Dashboard page should work without auth for now (seller selects their profile by ID via URL)
- No seller login/auth in this day — that's a later Sprint 2 concern

## Constraints
- `cargo check` — zero warnings
- `cargo clippy -- -D warnings` — zero warnings
- `npm run build` — passes
- No new crate dependencies
- Dashboard must be mobile-responsive
- Seller email/phone visible on own dashboard (it's their data)

## Success Criteria
1. `cargo check` — zero warnings
2. `cargo clippy -- -D warnings` — zero warnings
3. `npm run build` — passes
4. `GET /api/sellers/{id}/dashboard` returns seller info + per-property interest counts + completeness guide
5. `/seller/:id` page renders with profile header, listings section, and completeness checklist
6. Interest count badges show on each listing card
7. Completeness guide shows correct completed/incomplete steps based on seller data
8. Page is mobile-responsive
9. `count_lines` extracted to shared utility (no duplication)
10. "I'm Interested" button label is capitalized correctly
