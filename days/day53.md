# Day 53: Seller-Published Properties in Search Results + Full Journey Test

## Day 52 Builder Feedback Decisions

- **Knowledge graph indexing at publish time?** → NO. Deferred to enrichment pipeline. Published properties have empty area/society fields — KG enrichment should come from Python skills pipeline per CLAUDE.md §17.4. Property already gets `transparency_tags: ["seller-registered", "verification-pending"]`.
- **Rate limit on publish endpoint?** → YES. Reuse the `registration_rate_limiter` check inside `publish_registration`.

## Goal

Make seller-published properties discoverable in search results and verify the complete buyer journey: search → find seller property → view detail → express interest → seller sees interest on dashboard.

## Product Reason

The seller registration flow (Days 47-52) is useless if published properties are invisible to buyers. This day closes the loop: sellers list, buyers find.

## Sprint Context

Sprint 2: Seller-Buyer Connection. Day 9 of 14. Days 45-48 built connection loop. Days 49-51 built 7-step registration wizard. Day 52 built publish endpoint. Today makes published properties findable.

## Problem Analysis

Seller-published properties have **empty `area` and `area_id` fields**. This means:
1. Area-filtered searches exclude them (empty area doesn't match any area name)
2. No-area searches (browsing) do include them via text matching
3. Society name is not linked — no society enrichment

## Deliverables

### 1. Backend: Add `area` to step 7 payload, propagate to Property on publish

**File: `backend/src/routes/registration.rs`** — MODIFY

- Add `area: Option<String>` to `Step7Payload`
- In `publish_registration`, extract area from step 7 and populate `property.area`
- Fuzzy-match `society_name` from step 7 against `state.societies` to populate `society_id`
- If matched, inherit society's `area` and `area_id`

### 2. Backend: Add publish rate limit

**File: `backend/src/routes/registration.rs`** — MODIFY

- Add `registration_rate_limiter` check at top of `publish_registration`

### 3. Frontend: Add `area` field to step 7 of registration wizard

**File: `frontend/src/pages/SellerRegistrationPage.tsx`** — MODIFY
**File: `frontend/src/lib/types.ts`** — MODIFY

- Add "Area / Locality" text input to Step 7
- Add `area?: string` to Step7Payload type

### 4. Frontend: Style seller-registered property cards distinctly

**File: Results page / PropertyCard** — MODIFY

- "seller-registered" tag → amber/orange style
- "verification-pending" tag → muted/gray style

### 5. Full Journey Smoke Test

Verify end-to-end: register → publish → search → find → express interest → seller sees interest

## Technical Guidance

### Society fuzzy matching (simple):
```rust
let matched_society = state.societies.iter().find(|s| {
    let s_lower = s.name.to_lowercase();
    let input_lower = society_name.to_lowercase();
    s_lower == input_lower || s_lower.contains(&input_lower) || input_lower.contains(&s_lower)
});
```

### Area fallback chain:
1. Society matched → use society's area
2. Seller provided area in step 7 → use that directly
3. Neither → leave empty (appears in browsing only)

### Do NOT:
- Add KG nodes at publish time
- Add embedding generation for seller properties
- Add area autocomplete/dropdown
- Modify scoring algorithm
- Add separate seller properties endpoint

## Success Criteria

1. `cargo check` — passes
2. `npm run build` — passes
3. Seller-published property with area "Whitefield" appears when searching "Whitefield"
4. Property detail page for seller-published property shows SellerInfoCard
5. Express interest on seller-published property works (returns 201)
6. Seller dashboard shows interest for that property
7. Publish endpoint rate-limited (429 on excessive calls)
8. Society fuzzy matching works (e.g., "Prestige Shantiniketan" matches existing society)
