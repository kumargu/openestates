# Day 55: Area Extraction Fix + Seller Landing Page

## Previous Day Feedback Decisions

- **Day 53 mentioned area from society_info or property_prompt should populate area** — Day 53 implemented the Step 7 area field and society fuzzy matching in `publish_registration`. However, properties where the seller skips step 7 (or provides no area/society) still have empty area fields and are invisible to area searches. **FIXING TODAY**.
- **Day 54 added prompt matching and trust indicators** — These work only if the property appears in search results at all. The area extraction gap means seller properties without area are still excluded from area-filtered searches.
- **Animated seller landing page still not built** — Sprint vision explicitly lists this. With 4 days remaining, today is the last reasonable day to build it.

## Goal

Fix the area extraction gap so seller-registered properties are findable in area-based searches, and build a dedicated seller landing page (`/sell`) with animated value propositions that funnels sellers into the registration wizard.

## Product Reason

Two problems, both blocking Sprint 2's core promise:

1. **Findability**: A seller who fills out "East facing flat near Prestige Forum Whitefield" in their property prompt but skips or partially fills step 7 produces a property with empty `area`. That property is invisible to any buyer searching "Whitefield". The connection loop breaks silently.

2. **Seller acquisition**: The registration wizard is functional but has no marketing surface. A dedicated landing page with value propositions converts visitors into registrants. The vision doc explicitly calls for "animated seller landing page with value propositions."

## Sprint Context

Sprint 2: Seller-Buyer Connection. Day 11 of 14. Registration (days 49-51), publish (day 52), search visibility (day 53), prompt matching + trust badges (day 54) are done. Today fixes the last findability gap and builds the seller acquisition surface. 3 days remain after today for polish and additional seed data.

## Deliverables

### 1. Backend: Extract area from property_prompt at publish time (fallback chain enhancement)

**File: `backend/src/routes/registration.rs`** — MODIFY `publish_registration`

The current area resolution fallback chain is:
1. Society matched via step 7 → inherit society's area
2. Seller provided area in step 7 → use directly
3. Neither → empty (property invisible to area search)

Add a third fallback before the empty case: **extract area from property_prompt text** using the same AREA_ALIASES table used by the search intent parser.

```
Fallback chain (updated):
1. Society matched via step 7 → inherit society's area + area_id
2. Seller provided area in step 7 → use directly, match area_id
3. Property prompt mentions a known area → extract and use it
4. Neither → empty (appears only in browse/keyword, not area search)
```

Implementation: Create a helper function `extract_area_from_text(text: &str, areas: &[AreaProfile]) -> Option<(String, String)>` that:
- Lowercases the text
- Scans against `AREA_ALIASES` (imported from `search::intent`) for the longest match
- Returns `(area_name, area_id)` if found
- Falls back to checking against `state.areas` names for direct substring match

### 2. Backend: Also extract area for already-published properties with empty area

After building the Property struct, if `resolved_area` is still empty but `description_summary` (which contains property_prompt) is non-empty, run the area extraction on it. This is a safety net.

### 3. Frontend: Seller landing page at /sell

**File: `frontend/src/pages/SellerLandingPage.tsx`** — CREATE

A dedicated marketing page that sells the value of listing on OpenEstates. Design:

**Hero section (animated)**:
- Headline: "List your property where buyers trust the data"
- Subtitle with rotating text: cycling through "transparent pricing", "verified documents", "genuine buyers", "honest rankings"
- Primary CTA button: "List Your Property" → navigates to /register
- Secondary text: "Free to list. No hidden charges."

**Value propositions section (3 cards, fade-in on scroll)**:
- Card 1: "Transparency that sells" — "Buyers see your documents, verification status, and honest pricing. Complete profiles rank higher in search results."
- Card 2: "Reach serious buyers" — "Every buyer who clicks 'I'm Interested' is actively searching. No brokers, no spam calls."
- Card 3: "Your listing, your control" — "7-step registration at your pace. Skip steps, come back later. Publish when ready."

**How it works section (numbered steps)**:
1. "Register in 5 minutes" — basic info + property description
2. "Add details at your pace" — pricing, documents, photos (all optional, improve ranking)
3. "Get discovered" — property appears in buyer searches with trust indicators
4. "Track interest" — see who's interested on your seller dashboard

**Trust bar**:
- "X sellers listed" / "Y active buyers" / "Z interests expressed" (hardcoded stats for now)

**Bottom CTA**: "Start listing now" button → /register

Styling: Use the same warm color palette (#c96b4f accent, #fdf9f7 background). Use `IntersectionObserver` for scroll-triggered fade-in animations (same pattern as HomePage's `useOnScreen` hook).

### 4. Frontend: Wire routing and navigation

**File: `frontend/src/main.tsx`** — MODIFY

- Add lazy import for SellerLandingPage
- Add route: `<Route path="/sell" element={<SellerLandingPage />} />`
- Update NAV_ITEMS: change "List Property" link from `/register` to `/sell`

### 5. Seed data: Add 5 more sellers (total 10)

**File: `data/sellers/sellers.json`** — MODIFY

Add 5 new sellers with varying completeness to meet the sprint vision target of "10 sellers with varying completeness (30%–100%)":
- seller-006 through seller-010
- Mix of completeness levels: ~28%, ~57%, 100%, verified with RERA, documents but no photos

## Technical Guidance

- The area extraction helper should reuse `AREA_ALIASES` from `backend/src/search/intent.rs`.
- For the seller landing page, follow the existing animation patterns in `frontend/src/pages/HomePage.tsx`: the `useOnScreen` hook, `RotatingText` component, and `IntersectionObserver`-based fade-in.
- Keep the landing page as a single-file component.
- For additional seller seed data, follow the exact JSON structure of existing sellers.

## Constraints

- No new API endpoints needed for the landing page (static content)
- No new backend dependencies
- Do NOT modify the search algorithm — only modify the publish flow
- Do NOT add city selection or area autocomplete to registration
- Area extraction is a publish-time enhancement only

## Success Criteria

1. `cargo check` passes
2. `npm run build` passes
3. A seller who writes "3BHK apartment in Whitefield with park views" as their property prompt and skips step 7 produces a property with `area: "Whitefield"` after publishing
4. That property appears when a buyer searches "Whitefield"
5. `/sell` renders the seller landing page with animated hero, value props, and CTA linking to `/register`
6. Navigation "List Property" link goes to `/sell` instead of `/register`
7. `data/sellers/sellers.json` contains 10 sellers with completeness ranging from ~28% to 100%
