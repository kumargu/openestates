# Day 50: Seller Registration Steps 3-4 — Property Details & Pricing

## Goal

Extend the seller registration wizard with Steps 3 (Property Details) and 4 (Pricing). Backend: add typed payload structs and validation for steps 3-4 in the existing update_registration_step handler. Frontend: add Step3Form (property type, BHK, area, floor, facing, furnishing, age) and Step4Form (asking price, maintenance, possession status) to the wizard. Update the ProgressBar to mark steps 1-4 as active.

## Product Reason

Sprint 2, Day 6 of 14. Steps 1-2 (Basic Info + Property Prompt) shipped on Day 49. Steps 3-4 add the structured data that powers property cards, search matching, and comparison — these are the fields buyers actually filter and compare on. Without them, a registered seller's property can't appear meaningfully in search results.

## Sprint Context

Sprint 2: Seller-Buyer Connection. Days 45-48 built the connection loop (interest, dashboard). Day 49 started the 7-step registration journey. Today completes the structured data steps. Days 51-52 will handle Steps 5-7 (Documents, Photos, Society).

## Feedback Responses (Day 49 Items)

Day 49 had no unresolved failures — backend compiles clean, frontend builds clean, all success criteria passed.

No builder feedback or verifier observations were flagged as needing planner decisions.

---

## Deliverables

### 1. Backend: Step 3 + Step 4 payloads and validation

**File: `backend/src/routes/registration.rs`** — MODIFY

Add `Step3Payload` and `Step4Payload` structs:

```rust
struct Step3Payload {
    property_type: String,     // "apartment" | "villa" | "plot" | "independent_house"
    bhk: Option<u8>,           // 1-6, required for apartment/villa
    carpet_area_sqft: Option<u32>, // optional, if provided must be 100-50000
    floor: Option<u8>,         // optional, 0-99
    total_floors: Option<u8>,  // optional, 1-99
    facing: Option<String>,    // optional: "north" | "south" | "east" | "west" | "north_east" | "north_west" | "south_east" | "south_west"
    furnishing: Option<String>, // optional: "furnished" | "semi_furnished" | "unfurnished"
    age_years: Option<u8>,     // optional, 0-99
}

struct Step4Payload {
    asking_price: u64,         // required, in INR, min 100000 (1 lakh)
    price_negotiable: Option<bool>,
    maintenance_monthly: Option<u32>, // optional, in INR
    possession_status: Option<String>, // "ready" | "under_construction" | "resale"
}
```

Add match arms for steps 3 and 4 in `update_registration_step`.
Store step 3 as `draft.property_details = Some(serde_json::to_value(&payload))`.
Store step 4 as `draft.pricing = Some(serde_json::to_value(&payload))`.

### 2. Frontend: Types

**File: `frontend/src/lib/types.ts`** — MODIFY

Add `Step3Payload` and `Step4Payload` types.

### 3. Frontend: Step3Form + Step4Form

**File: `frontend/src/pages/SellerRegistrationPage.tsx`** — MODIFY

- `Step3Form`: dropdowns for property_type, bhk, furnishing, facing. Number inputs for carpet_area, floor, total_floors, age. BHK field only shows when property_type is apartment/villa.
- `Step4Form`: number input for asking_price (formatted with commas on display), checkbox for negotiable, number input for maintenance, dropdown for possession_status.
- Update ProgressBar: mark steps 1-4 as active (remove "soon" label).
- Update `ComingNextPlaceholder` to say "Steps 5-7 coming soon" and offer "Back to Step 4".
- Wire handleStep3Save and handleStep4Save in the main component.
- Update resume logic: `Math.min(existing.current_step + 1, 5)` to cap at step 5 placeholder.

---

## Files to Modify

- `backend/src/routes/registration.rs` — add Step3Payload, Step4Payload, validation, match arms
- `frontend/src/lib/types.ts` — add Step3Payload, Step4Payload
- `frontend/src/pages/SellerRegistrationPage.tsx` — add Step3Form, Step4Form, wire saves, update ProgressBar

## Files NOT Changed

- `backend/src/models/registration.rs` — property_details and pricing fields already exist as Option<serde_json::Value>
- `backend/src/main.rs` — routes already registered
- `frontend/src/lib/api.ts` — updateRegistrationStep already works for any step number

---

## Constraints

- `cargo check` — zero warnings
- `npm run build` — passes
- No new crate or npm dependencies
- Reuse existing patterns from Step 1/2 implementation

## Validation Rules

**Step 3 (Property Details):**
- `property_type`: required, must be one of: apartment, villa, plot, independent_house
- `bhk`: required when property_type is apartment or villa, must be 1-6
- `carpet_area_sqft`: optional, if provided must be 100-50000
- `floor`: optional, 0-99
- `total_floors`: optional, 1-99
- `facing`: optional, must be valid direction
- `furnishing`: optional, must be one of: furnished, semi_furnished, unfurnished
- `age_years`: optional, 0-99

**Step 4 (Pricing):**
- `asking_price`: required, minimum 100000 (1 lakh INR)
- `price_negotiable`: optional boolean
- `maintenance_monthly`: optional, if provided max 100000
- `possession_status`: optional, must be one of: ready, under_construction, resale

---

## Success Criteria

1. `cargo check` — zero warnings
2. `npm run build` — passes
3. `PUT /api/registrations/{id}/step/3` with valid property details returns 200 with `current_step: 3, completeness_pct: 42`
4. `PUT /api/registrations/{id}/step/4` with valid pricing returns 200 with `current_step: 4, completeness_pct: 57`
5. Step 3 validation rejects missing property_type, invalid bhk for apartment
6. Step 4 validation rejects asking_price below 1 lakh
7. ProgressBar shows steps 1-4 as active, 5-7 as "soon"
8. Step 3 form shows conditional BHK field (only for apartment/villa)
9. Step 4 form shows price input with sensible formatting
10. Filling Steps 3 and 4 advances to the "coming soon" placeholder for Steps 5-7
11. Refreshing the page resumes from last completed step
