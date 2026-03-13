# Day 51: Seller Registration Steps 5-7 — Documents, Photos, Society Info

## Goal

Complete the 7-step seller registration wizard. Backend: add Step5Payload (documents), Step6Payload (photos), Step7Payload (society info) with validation. Frontend: add Step5Form, Step6Form, Step7Form, replace the "coming soon" placeholder with working forms, add a completion summary screen, and add a "List your property" nav link.

## Product Reason

Sprint 2, Day 7 of 14. Steps 1-4 shipped on Days 49-50. Today completes the full registration journey — after this, a seller can register end-to-end. This unblocks Day 52+ work: converting completed registrations into live listings, verification flows, and the marketplace connection loop. A complete registration also gives us the completeness_pct signal that feeds into search ranking (higher completeness = higher visibility).

## Sprint Context

Sprint 2: Seller-Buyer Connection. Days 45-48 built the connection loop (interest, dashboard). Days 49-50 built Steps 1-4 of registration. Today finishes Steps 5-7. Tomorrow (Day 52) is the mid-sprint review day.

## Feedback Responses (Previous Days)

### Day 49 feedback items:
- **No nav link to /register** — FIXING TODAY. Adding "List Property" CTA in the main nav.
- **No skip button** — ACCEPTED RISK. Steps are already optional-heavy (most fields optional). Skip would add UX confusion for minimal gain. Will revisit if user testing shows friction.
- **No authentication** — ACCEPTED RISK. Sprint 2 scope is the flow, not auth. Auth is Sprint 3+.
- **Step 2 maxLength 550 vs validation 500** — Minor inconsistency, not blocking. The buffer prevents frustrating edge cases where the user types exactly 500 chars. Keeping as-is.

### Day 48 feedback items:
- **SellerInfoCard depends on react-router Link** — Acceptable coupling for now. The component lives in a page file, not a shared library.

---

## Deliverables

### 1. Backend: Steps 5, 6, 7 payloads and validation

**File: `backend/src/routes/registration.rs`** — MODIFY

Add `Step5Payload`, `Step6Payload`, `Step7Payload` structs:

```rust
struct Step5Payload {
    has_sale_deed: Option<bool>,
    has_khata: Option<bool>,
    has_ec: Option<bool>,
    has_rera_registration: Option<bool>,
    rera_number: Option<String>,     // optional, max 50 chars
}

struct Step6Payload {
    photo_count: Option<u8>,         // how many photos they plan to upload (0-20)
    has_floor_plan: Option<bool>,
    video_tour_url: Option<String>,  // optional URL, max 500 chars
}

struct Step7Payload {
    society_name: Option<String>,    // optional, max 100 chars
    total_units: Option<u32>,        // optional, 1-10000
    amenities: Option<Vec<String>>,  // optional list of amenity strings
    additional_notes: Option<String>, // optional, max 500 chars
}
```

Add match arms for steps 5, 6, 7 in `update_registration_step`.
Update the catch-all to say "step must be 1-7".

### 2. Frontend: Types

**File: `frontend/src/lib/types.ts`** — MODIFY

Add `Step5Payload`, `Step6Payload`, `Step7Payload` types.

### 3. Frontend: Step5Form (Documents)

**File: `frontend/src/pages/SellerRegistrationPage.tsx`** — MODIFY

- Checkboxes for: sale_deed, khata, ec, rera_registration
- Conditional text input for RERA number when rera_registration is checked
- Trust indicator: show how many documents are checked with encouraging message

### 4. Frontend: Step6Form (Photos)

Same file. Simple form:
- Dropdown for photo_count (0, 1-5, 6-10, 11-20)
- Checkbox for has_floor_plan
- Text input for video_tour_url (optional)
- Note: actual photo upload is future work — this captures intent/metadata

### 5. Frontend: Step7Form (Society Info)

Same file:
- Text input for society_name (autocomplete from known societies is future)
- Number input for total_units
- Amenity chips: pool, gym, clubhouse, park, security, power_backup, parking, lift
- Textarea for additional_notes

### 6. Frontend: Completion Screen

Replace ComingNextPlaceholder with a RegistrationComplete component when current_step >= 7:
- Congratulations message
- Completeness percentage
- "What happens next" explanation
- Link back to seller dashboard

### 7. Frontend: Nav link to /register

**File: `frontend/src/main.tsx`** — MODIFY (or wherever the Nav component lives)

Add "List Property" link in the main navigation, visible to all users.

---

## Files to Modify

- `backend/src/routes/registration.rs` — add Step5-7 payloads, validation, match arms
- `frontend/src/lib/types.ts` — add Step5-7 payload types
- `frontend/src/pages/SellerRegistrationPage.tsx` — add Step5Form, Step6Form, Step7Form, RegistrationComplete, remove ComingNextPlaceholder
- `frontend/src/main.tsx` — add "List Property" nav link

## Files NOT Changed

- `backend/src/models/registration.rs` — documents, photos, society_info fields already exist as Option<serde_json::Value>
- `backend/src/main.rs` — routes already registered
- `frontend/src/lib/api.ts` — updateRegistrationStep already works for any step number

---

## Constraints

- `cargo check` — zero warnings
- `npm run build` — passes
- No new crate or npm dependencies
- Reuse existing patterns from Steps 1-4 implementation
- Keep Step 5-7 forms simple — these are metadata/intent capture, not full document/photo upload

## Validation Rules

**Step 5 (Documents):**
- All fields optional (booleans default to false/null)
- `rera_number`: if provided, max 50 chars, alphanumeric + hyphens only

**Step 6 (Photos):**
- `photo_count`: optional, 0-20
- `video_tour_url`: optional, max 500 chars

**Step 7 (Society Info):**
- `society_name`: optional, max 100 chars
- `total_units`: optional, 1-10000
- `amenities`: optional, max 20 items, each max 50 chars
- `additional_notes`: optional, max 500 chars

---

## Success Criteria

1. `cargo check` — zero warnings
2. `npm run build` — passes
3. `PUT /api/registrations/{id}/step/5` with valid documents returns 200 with updated completeness
4. `PUT /api/registrations/{id}/step/6` with valid photos returns 200 with updated completeness
5. `PUT /api/registrations/{id}/step/7` with valid society info returns 200 with `current_step: 7, completeness_pct: 100`
6. Step 5 shows document checkboxes with conditional RERA number field
7. Step 6 shows photo metadata form
8. Step 7 shows society info with amenity chips
9. Completing Step 7 shows a congratulations/completion screen
10. ProgressBar shows all 7 steps as active (no more "soon" labels)
11. "List Property" link visible in main navigation
12. Refreshing the page resumes from last completed step (including steps 5-7)
