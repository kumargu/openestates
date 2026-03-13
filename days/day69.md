# Day 69: Wire Trust Badges into Product Surfaces End-to-End

## Sprint Position
Sprint 3 (RERA Data Foundation & Trust Model), Day 11 of 14. 4 days remaining.

## Goal

Complete the trust visualization story by wiring all 5 trust badge components into every product surface where property data appears, and add a dedicated Data Provenance section to the property detail page.

## Product Reason

Sprint 3's promise is "make trust visible." The badge components exist, the backend delivers root_source, project_status, builder_delivery_display, data_freshness, and confidence_score through the API. But the integration is incomplete: PropertySidePanel shows zero trust badges, ComparePanel has no trust row, PropertyPage lacks a ConfidenceMeter, and there is no consolidated trust section on the detail page. This is the last chance to prove the end-to-end trust story before Sprint 3 closes.

## Feedback Resolution

1. **Manual source_type with confidence 0.5-0.7 for area facts** — Accepted. No change needed. Shows as "Moderate" in ConfidenceMeter, which is honest.
2. **Edge count targets were wrong (assumed 85, actual 70)** — Acknowledged. All 70 have edges.
3. **11 societies lack project_status (no RERA dates)** — Accepted. ProjectStatusTag handles null gracefully.
4. **Commit all untracked files early** — Do this first as prerequisite.
5. **Automate area enrichment as skill** — Defer to Sprint 4.
6. **5 orphan builder nodes** — Still Sprint 4.

## Deliverables

### 1. Backend: Add confidence_score to PropertyDetailResponse

**Files:** `backend/src/routes/properties.rs`, `backend/src/search/text.rs`

Extract the `compute_confidence_score` logic so it can be called from the property detail route. Add `confidence_score` field to `PropertyDetailResponse`.

### 2. Frontend types: Add confidence_score to PropertyDetailResponse

**File:** `frontend/src/lib/types.ts`

Add `confidence_score?: ConfidenceScore` to the detail response type.

### 3. PropertySidePanel: Add trust badges

**File:** `frontend/src/components/PropertySidePanel.tsx`

Add TrustBadge (root_source), ProjectStatusTag, BuilderTrustBadge (compact), DataFreshnessBadge (compact).

### 4. ComparePanel: Add trust row

**File:** `frontend/src/components/ComparePanel.tsx`

Add "Data Trust" comparison row with TrustBadge, ProjectStatusTag, BuilderTrustBadge per property.

### 5. PropertyPage: Add ConfidenceMeter + Data Provenance section

**File:** `frontend/src/pages/PropertyPage.tsx`

- Import and render ConfidenceMeter next to existing trust badges
- Add "Data Provenance" section-card in sidebar consolidating: Data Source, Project Status, Builder Track Record, Data Freshness, Data Confidence, RERA Number

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/routes/properties.rs` | Add confidence_score to PropertyDetailResponse |
| `backend/src/search/text.rs` | Extract compute_confidence_score for reuse |
| `frontend/src/lib/types.ts` | Add confidence_score to detail response type |
| `frontend/src/components/PropertySidePanel.tsx` | Add 4 trust badge components |
| `frontend/src/components/ComparePanel.tsx` | Add trust comparison row |
| `frontend/src/pages/PropertyPage.tsx` | Add ConfidenceMeter + Data Provenance section |

## Success Criteria

1. PropertyDetailResponse includes confidence_score from backend
2. PropertySidePanel renders TrustBadge, ProjectStatusTag, BuilderTrustBadge, DataFreshnessBadge
3. ComparePanel has "Data Trust" row with per-property trust badges
4. PropertyPage has ConfidenceMeter and "Data Provenance" section-card
5. `cargo check` passes
6. `cargo test` passes (44+ tests)
7. `npm run build` passes
8. No regressions in existing trust badge rendering on ResultsPageA

## Deferred Items

| Item | Reason |
|------|--------|
| Automating area enrichment as skill | Sprint 4 — requires LLM integration |
| 5 orphan builder nodes (duplicates) | Sprint 4 — requires dedup strategy |
| 11 Discovered societies without project_status | No RERA dates available |
| Trust badges on homepage featured properties | Homepage redesign Sprint 4+ |
