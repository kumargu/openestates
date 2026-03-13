# Day 49: Seller Registration — Data Model, API, and First 2 Form Steps

## Goal

Build the foundation for seller registration: a `RegistrationDraft` data model persisted to disk, two new API endpoints (create draft + update step), and the first two frontend form steps (Basic Info and Property Prompt) rendered as a multi-step wizard with step-by-step saving.

## Product Reason

Days 45-48 closed the connection loop: buyers can discover sellers, express interest, and sellers can view their dashboards. But every seller in the system was seeded manually. There is no way for a new seller to register. Day 49 lays the backend and frontend foundation so a seller can begin registration, fill in basic info, write a property prompt, and resume later. This is the first half of the Hinge-style 7-step journey described in `docs/vision.md`.

## Sprint Context

Sprint 2, Day 5 of 14. Connection loop is closed. This begins the Seller Registration Journey, the next major Sprint 2 feature. Days 49-50 will cover steps 1-2 (today) and steps 3-4 (tomorrow), with steps 5-7 following in days 51-52.

---

## Design Decisions

### Why a separate `RegistrationDraft` instead of mutating `Seller`?

The existing `Seller` model represents a published, visible seller. A registration draft is an in-progress, potentially incomplete record that should not appear in buyer-facing search results or property pages until the seller explicitly publishes. Mixing draft state into the live `Seller` model would create confusion about which sellers are "real" vs "in progress."

The draft lives at `data/registrations/{draft_id}.json`. When the seller completes enough steps and publishes, the draft is converted into a `Seller` entry and appended to `data/sellers/sellers.json`.

### Why file-per-draft?

Follows the same pattern as `data/knowledge/nodes/{type}/{slug}.json` -- atomic writes via tmp+rename, no lock contention between different registrations, S3-ready prefix structure.

### Why save on each step?

Resumability is a core vision requirement. Each step POST updates the draft file on disk. If the user closes their browser and returns, they resume from where they left off.

---

## Feedback Responses (Day 48 Items)

**1. SellerInfoCard depends on react-router Link (Day 48 concern)**
Decision: Acceptable coupling. SellerInfoCard is a page-level component, not a shared library widget. No action needed.

**2. Global rate limiter not per-IP (Day 46 concern)**
Decision: Accept for now. Per-IP rate limiting deferred to Sprint 4 (Performance & Data Expansion). Current global limit is sufficient for seed-stage traffic.

**3. interest_counter resets on restart (Day 46 concern)**
Decision: Accept. Combined with ms timestamps, collision is extremely unlikely. Production would seed counter from existing IDs.

**4. Interest count is 12 (Day 48 data quality)**
Decision: Acceptable. Within the 12-15 target range.

---

## Deliverables

### 1. Backend: `RegistrationDraft` model

**File: `backend/src/models/registration.rs`** (NEW)

```rust
pub struct RegistrationDraft {
    pub id: String,                          // "draft-{timestamp}-{counter}"
    pub current_step: u8,                    // 0-7, whichever step was last completed
    pub created_at: String,
    pub updated_at: String,
    // Step 1: Basic Info
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    // Step 2: Property Prompt
    pub property_prompt: Option<String>,
    // Steps 3-7: placeholders for future days
    pub property_details: Option<serde_json::Value>,
    pub pricing: Option<serde_json::Value>,
    pub documents: Option<serde_json::Value>,
    pub photos: Option<serde_json::Value>,
    pub society_info: Option<serde_json::Value>,
}
```

Methods: `new(id)`, `completeness_pct()`.

**File: `backend/src/models/mod.rs`** -- add `pub mod registration` + re-export.

### 2. Backend: Registration API endpoints

**File: `backend/src/routes/registration.rs`** (NEW)

- `POST /api/registrations` — Create blank draft, return 201
- `PUT /api/registrations/{id}/step/{step_num}` — Update step 1 or 2, validate, persist
- `GET /api/registrations/{id}` — Load draft for resume

**File: `backend/src/routes/mod.rs`** -- add `pub mod registration`
**File: `backend/src/main.rs`** -- register routes + startup log

### 3. Frontend: Types and API

**File: `frontend/src/lib/types.ts`** -- RegistrationDraft, payload types
**File: `frontend/src/lib/api.ts`** -- `putJson` helper + `createRegistration`, `getRegistration`, `updateRegistrationStep`

### 4. Frontend: SellerRegistrationPage

**File: `frontend/src/pages/SellerRegistrationPage.tsx`** (NEW)

Multi-step wizard:
- Progress bar (7 steps, 1-2 active, 3-7 grayed)
- Step 1: Basic Info (name required, email/phone optional)
- Step 2: Property Prompt (textarea, max 500 chars)
- Save & Continue / Skip / Back buttons
- localStorage persistence for resume

**Route: `/register`** in `frontend/src/main.tsx`

---

## Files to Create
- `backend/src/models/registration.rs`
- `backend/src/routes/registration.rs`
- `frontend/src/pages/SellerRegistrationPage.tsx`

## Files to Modify
- `backend/src/models/mod.rs` -- add `pub mod registration` + re-export
- `backend/src/routes/mod.rs` -- add `pub mod registration`
- `backend/src/main.rs` -- register routes + startup log
- `frontend/src/lib/types.ts` -- add registration types
- `frontend/src/lib/api.ts` -- add putJson + registration API functions
- `frontend/src/main.tsx` -- add route for `/register`

---

## Constraints

- `cargo check` -- zero warnings
- `cargo clippy -- -D warnings` -- zero warnings
- `npm run build` -- passes
- No new crate or npm dependencies
- Rate limit: reuse existing global rate limiter for POST /api/registrations (max 30/minute)

## Validation Rules

**Step 1 (Basic Info):**
- `name`: required, non-empty after trim, max 100 chars
- `email`: optional, if provided must contain `@` and `.`
- `phone`: optional, if provided must be 10-15 digits (after stripping `+`, `-`, spaces)

**Step 2 (Property Prompt):**
- `property_prompt`: required, non-empty after trim, max 500 chars

---

## Success Criteria

1. `cargo check` -- zero warnings
2. `cargo clippy -- -D warnings` -- zero warnings
3. `npm run build` -- passes
4. `POST /api/registrations` returns 201 with new draft ID
5. `PUT /api/registrations/{id}/step/1` with `{ name: "Test" }` returns 200 with `current_step: 1, completeness_pct: 14`
6. `PUT /api/registrations/{id}/step/2` with `{ property_prompt: "Nice flat" }` returns 200 with `current_step: 2, completeness_pct: 28`
7. `GET /api/registrations/{id}` returns full draft with all saved fields
8. Draft files appear in `data/registrations/` directory
9. `/register` page renders Step 1 form
10. Filling Step 1 and clicking "Save & Continue" advances to Step 2
11. Filling Step 2 and clicking "Save & Continue" shows placeholder for steps 3-7
12. Refreshing the page resumes from last completed step (localStorage + GET draft)
13. Validation errors display inline (empty name, prompt over 500 chars)
