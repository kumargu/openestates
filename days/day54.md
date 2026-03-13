# Day 54: Property Prompt Matching in Search + Trust Indicators

## Previous Day Feedback Decisions

- **RegistrationComplete links to / (home)** — Acceptable for now. When we have auth (Sprint 4+), the publish response already returns `dashboard_url` with the seller ID. The frontend just needs to use it. No change needed today.
- **No actual file upload for photos/documents** — Correct. S3 isn't built. Step 5/6 capture metadata only. No change needed.
- **Amenities as free-text Vec<String>** — Fine. We'll normalize later when we have enough seller data to know canonical set.
- **validate_step5 allows / in rera_number** — Acceptable, real RERA numbers contain slashes.

## Goal

Make seller property prompts boost search relevance, and show trust indicators (document badges, completeness) on property cards so buyers can distinguish verified sellers from unverified ones at a glance.

## Product Reason

Sprint 2 is about connection. Two things drive connection: **findability** (seller descriptions matching buyer intent) and **trust** (visible verification status). Today builds both.

## Sprint Context

Sprint 2: Seller-Buyer Connection. Day 10 of 14. Registration (days 49-51), publish (day 52), search visibility (day 53) are done. Today adds the matching signal (prompts) and trust signal (badges) that make seller listings competitive with seed data.

## Deliverables

### 1. Backend: Include seller prompt in search text matching

**File: `backend/src/routes/properties.rs`** — MODIFY search handler

- When building search text for a property, include `description_summary` (which contains the seller's `property_prompt` for registered properties) in the text that gets matched against the search query
- This means "east facing corner flat with sunrise views" in a seller prompt will match buyer search "east facing flat"
- No new fields needed — `description_summary` is already populated from `property_prompt` at publish time

### 2. Backend: Add seller trust fields to PropertyCard

**File: `backend/src/models/mod.rs`** — MODIFY PropertyCard

- Add `seller_completeness_pct: Option<u32>` — populated when property has a seller
- Add `documents_provided: Vec<String>` — populated from seller's documents_provided
- Add `seller_verified: Option<bool>` — populated from seller's verified flag

**File: `backend/src/routes/enrichment.rs`** (or wherever `enrich_property_card` lives) — MODIFY

- Look up seller by property's `seller_id`, populate the new card fields

### 3. Backend: Completeness boost in search ranking

**File: `backend/src/routes/properties.rs`** — MODIFY search scoring

- Properties with seller_completeness_pct >= 70 get a small score boost (+0.05)
- Properties with seller_completeness_pct >= 42 get a smaller boost (+0.02)
- This implements "higher completeness profiles rank higher" from sprint vision

### 4. Frontend: Trust badges on PropertyCard

**File: `frontend/src/components/PropertyCard.tsx`** — MODIFY

- Show document badges (small icons) when documents_provided is non-empty: sale_deed, khata, ec, rera
- Show seller completeness as a small indicator (e.g., "85% profile" in muted text)
- Differentiate verified sellers with a green checkmark badge
- Keep it subtle — don't clutter the card

### 5. Frontend: Trust indicators on PropertySidePanel

**File: `frontend/src/components/PropertySidePanel.tsx`** — MODIFY

- Show same trust badges as PropertyCard but slightly more detailed
- Show "Seller verified" or "Verification pending" status

## Technical Guidance

- Read `.claude/skills/add-api-endpoint.md` if adding new response fields
- The `enrich_property_card` function in routes/enrichment.rs is where card enrichment happens
- PropertyCard already shows `transparency_tags` — trust badges complement these
- Keep styling consistent with existing tag styling (amber/gray for seller-registered/verification-pending from day 53)

## Constraints

- No new API endpoints needed — just enriching existing responses
- No new frontend pages — just enhancing existing components
- Don't change the search algorithm fundamentally — just add prompt text to search corpus and a small completeness boost
- Keep badge styling minimal and consistent with existing design

## Success Criteria

1. `cargo check` passes
2. `npm run build` passes
3. PropertyCard shows document badges when seller has documents
4. PropertyCard shows seller completeness indicator when seller exists
5. Search for terms in a seller's property_prompt returns that property
6. Properties with higher seller completeness rank slightly higher in search
