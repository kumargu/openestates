# Day 64: Builder Trust Frontend + Backend DRY Cleanup + ProjectStatusTag Fix

## Goal

Surface builder delivery track record in the frontend (PropertyCard, ResultsPage, PropertyPage), fix ProjectStatusTag to use machine-readable `project_status`, and DRY up duplicated RootSource mapping in Rust.

## Product Reason

Day 63 built the backend cross-node builder scoring and the `compute_builder_delivery_rate` skill, but the trust data is invisible to users outside of search match reasons. Users should see builder reliability as a visible badge — this completes Sprint 3's "make trust visible" promise for builders. The ProjectStatusTag fix ensures color-coding is data-driven, not heuristic.

## Sprint Context

Day 6 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Addressed

1. **Day 61 verifier warning: ProjectStatusTag doesn't pass status prop** — Fix: add `project_status` to PropertyDetail response, pass to component.
2. **Day 61 verifier suggestion: DRY up RootSource mapping** — Fix: add `RootSource::as_str()` method, use in both callsites.
3. **Day 61 builder concern: duplicated RootSource match arms** — Same fix as above.
4. **Day 63 deferred: Frontend builder badge/display** — This is the main deliverable today.

## Deliverables

### 1. Backend: `RootSource::as_str()` (DRY fix)

**File:** `backend/src/knowledge/node.rs`

Add `pub fn as_str(&self) -> &'static str` on `RootSource`. Update both callsites in `enrichment.rs` and `properties.rs`.

### 2. Backend: Add `project_status` to `PropertyDetail`

**File:** `backend/src/routes/properties.rs`

Add `project_status: Option<String>` to `PropertyDetail` struct. Populate from society KG node facts.

### 3. Backend: `BuilderTrust` struct + extraction

**File:** `backend/src/routes/enrichment.rs`

Add `BuilderTrust` struct with:
- `delivery_rate: Option<f64>`
- `project_count: Option<u32>`
- `delivery_display: Option<String>`
- `zero_revocations: Option<bool>`

Add `extract_builder_trust()` — traverse BuiltBy edges from society to builder node, extract facts.

Wire into `PropertyDetail` (properties.rs) and `PropertyCard` enrichment (enrichment.rs — add `builder_delivery_display: Option<String>`).

### 4. Backend: Add `builder_delivery_display` to `PropertyCard` model

**File:** `backend/src/models/property.rs`

Add `builder_delivery_display: Option<String>` field.

### 5. Frontend: Types update

**File:** `frontend/src/lib/types.ts`

- Add `builder_delivery_display?: string` to `PropertyCard`
- Add `project_status?: string` to `PropertyDetailResponse`
- Add `builder_trust?: { delivery_rate?: number; project_count?: number; delivery_display?: string; zero_revocations?: boolean }` to `PropertyDetailResponse`

### 6. Frontend: `BuilderTrustBadge` component

**New file:** `frontend/src/components/BuilderTrustBadge.tsx`

Compact pill component (follows TrustBadge pattern):
- 90-100% delivery: green badge "Builder: 100% on time"
- 60-89%: amber badge
- Below 60%: red/caution badge
- Uses `builder_delivery_display` text when available

### 7. Frontend: Wire into pages

- **PropertyCard.tsx**: Add `BuilderTrustBadge` to signals row
- **ResultsPageA.tsx**: Add `BuilderTrustBadge` to CardA signals row
- **PropertyPage.tsx**: Fix ProjectStatusTag `status` prop, add builder trust section

### 8. Run `compute_builder_delivery_rate` on all eligible builders

`python3 -m pipeline.skills.compute_builder_delivery_rate`

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/knowledge/node.rs` | Add `RootSource::as_str()` |
| `backend/src/routes/enrichment.rs` | Use `as_str()`, add `BuilderTrust`, `extract_builder_trust()`, wire into PropertyCard |
| `backend/src/routes/properties.rs` | Use `as_str()`, add `project_status` + `builder_trust` to PropertyDetail |
| `backend/src/models/property.rs` | Add `builder_delivery_display` to PropertyCard |
| `frontend/src/lib/types.ts` | Add builder + project_status fields |
| `frontend/src/components/BuilderTrustBadge.tsx` | New component |
| `frontend/src/components/PropertyCard.tsx` | Add BuilderTrustBadge |
| `frontend/src/pages/ResultsPageA.tsx` | Add BuilderTrustBadge |
| `frontend/src/pages/PropertyPage.tsx` | Fix ProjectStatusTag, add builder trust section |

## Not in Scope

- Builder deduplication (Sprint 4)
- Builder profile pages (Sprint 4)
- Running additional enrichment skills

## Success Criteria

1. `cargo test` passes (all 43+ tests)
2. `npm run build` succeeds
3. `RootSource` match arms in ONE place only (node.rs), not duplicated
4. PropertyPage ProjectStatusTag receives machine-readable `project_status` for color-coding
5. Properties with builder delivery data show BuilderTrustBadge on PropertyCard, ResultsPage, PropertyPage
6. Properties WITHOUT builder data show nothing (graceful absence)
7. `compute_builder_delivery_rate` populates all eligible builders
