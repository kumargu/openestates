# Day 52: Mid-Sprint Review + Registration-to-Seller Conversion

## Mid-Sprint Assessment (Day 8 of 14, Sprint 2)

1. **Connection infrastructure is solid.** Interest flow, dashboard, and the I'm Interested button work end-to-end with real data.
2. **Registration wizard is feature-complete but dead-ended.** All 7 steps work, validation is thorough, but completed registrations produce nothing visible.
3. **The missing bridge is the most important remaining work.** Without draft-to-seller conversion, the entire registration flow is theater — it collects data that goes nowhere.
4. **Trust & Verification is deprioritized.** With 6 days left, the animated landing page and document verification UI should yield to making the core loop functional.
5. **Remaining scope: (a) conversion endpoint, (b) linking registration to dashboard, (c) making seller-registered properties findable.** These three make the sprint's core promise real.

## Goal

Build the registration-to-seller conversion bridge: a backend endpoint that publishes a completed registration draft as a real Seller + Property, and update the frontend completion screen to link to the new seller's dashboard.

## Product Reason

Without this bridge, Sprint 2's seller registration is a dead end. The sprint promise is "connect buyers and sellers" — that requires sellers to actually exist in the system after registering. This is the single highest-leverage piece of work remaining in the sprint.

## Sprint Context

Sprint 2: Seller-Buyer Connection. Days 45-48 built the connection loop. Days 49-51 built the full 7-step registration wizard. Today closes the gap: completed registrations become real sellers with real listings.

## Feedback Responses (Previous Days)

### Day 51 feedback items:
- **RegistrationComplete links to `/` since registrations don't map to seller IDs** — FIXING TODAY. The publish endpoint will return a seller ID, and the completion screen will link to `/seller/{id}`.
- **Steps 5-7 capture metadata/intent, not actual uploads** — ACCEPTED. Photo/document upload is future work (needs S3).
- **Amenities stored as `Vec<String>` not enums** — ACCEPTED. Normalize later when canonical set is known.
- **`validate_step5` allows `/` in `rera_number`** — CORRECT. Real RERA numbers contain slashes.

## Deliverables

### 1. Backend: Publish registration endpoint

**File: `backend/src/routes/registration.rs`** — MODIFY

Add `POST /api/registrations/{id}/publish` handler:

- Load draft from disk, validate `current_step >= 4` (minimum viable)
- Generate seller ID: `seller-{timestamp}-{counter}` and property ID: `prop-reg-{timestamp}-{counter}`
- Convert draft fields into `Seller` struct (name, email, phone, property_prompt, completeness fields, `verified: false`)
- Convert draft fields into `Property` struct (type, BHK, area, price from steps 3-4, title auto-generated)
- Append to `data/sellers/sellers.json` and `data/seed/properties.json` (atomic writes)
- Insert into in-memory `AppState.sellers` and `AppState.properties` via `RwLock`
- Return `{ seller_id, property_id, dashboard_url }`
- Idempotency: draft gets `published_seller_id` field, publishing twice returns error

### 2. Backend: Wire the new route

**File: `backend/src/main.rs`** — MODIFY

Add route: `.route("/api/registrations/{id}/publish", post(routes::registration::publish_registration))`

### 3. Backend: Make sellers mutable

**File: `backend/src/state.rs`** — MODIFY

Change `pub sellers: Vec<Seller>` to `pub sellers: RwLock<Vec<Seller>>` (same pattern as properties).

### 4. Backend: Update all seller read sites

**Files: `backend/src/routes/sellers.rs`, `backend/src/data_loader.rs`** — MODIFY

Update `state.sellers.iter()` → `state.sellers.read().await.iter()`.

### 5. Frontend: API function for publish

**File: `frontend/src/lib/api.ts`** — MODIFY

Add `publishRegistration(draftId: string)` → `POST /api/registrations/{draftId}/publish`.

### 6. Frontend: Update RegistrationComplete

**File: `frontend/src/pages/SellerRegistrationPage.tsx`** — MODIFY

- Add "Publish My Listing" button that calls `publishRegistration`
- On success: show seller ID, link to `/seller/{seller_id}`
- On error: show error with retry
- Clear localStorage after successful publish
- Change messaging to "Your listing is live!"

### 7. Frontend: Types for publish response

**File: `frontend/src/lib/types.ts`** — MODIFY

Add `PublishResult` type: `{ seller_id: string; property_id: string; dashboard_url: string }`.

## Success Criteria

1. `cargo check` — zero warnings
2. `npm run build` — passes
3. `POST /api/registrations/{id}/publish` returns `{ seller_id, property_id, dashboard_url }` with 200
4. New seller appears in `GET /api/sellers`
5. New property appears in `GET /api/properties` with `seller_id` set
6. Seller dashboard at `/seller/{seller_id}` shows the property
7. RegistrationComplete screen has publish button → creates seller → links to dashboard
8. Publishing same draft twice returns error (idempotency)
9. Publishing draft with `current_step < 4` returns 400
10. Property page for new property shows SellerInfoCard

## Remaining Sprint Scope (Days 53-58)

- **Day 53:** Seller-registered properties in search results. Full journey test.
- **Day 54:** Progressive trust indicators, completeness affects search rank.
- **Day 55:** Seller can edit listing from dashboard.
- **Day 56:** Expand seed sellers, polish dashboard UI.
- **Day 57:** End-to-end journey test.
- **Day 58:** Sprint review, polish, bug fixes, update docs.
