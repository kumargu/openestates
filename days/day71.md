# Day 71: Confidence Scoring Fix, Feedback Polish, Sprint 3 Wrap

## Sprint Position
Sprint 3 (RERA Data Foundation & Trust Model), Day 13 of 14. 1 day remaining after today (Day 72 is final).

## Day 70 Grade
Excellent. Committed all Sprint 3 work across 3 structured commits (305 files). Added 15 new Rust tests (9 graph_area_match + 6 confidence, total 59). Resolved 5 orphan builder duplicates with canonical_builder facts. Added 14 Python fuzzy match tests. Clean working tree. Both builds pass.

## Goal

Resolve all accumulated feedback items from Days 69-70, focusing on confidence scoring accuracy and UI polish. No new features. This is hardening day.

## Product Reason

Sprint 3 ends tomorrow. The trust model is functionally complete but has calibration and display issues that undermine the transparency promise:
- Confidence scores on property detail pages are artificially low because `graph_driven_pct=0.0` is hardcoded
- Discovered nodes with few facts score "Moderate" when they should score "Low" because freshness ~1.0 inflates the overall
- Orphan builder canonical_builder facts exist but the Rust backend ignores them at query time
- Minor UI gaps: RERA portal URL not linked, trust badges slightly redundant between header and sidebar

Fixing these before Sprint 4 prevents carrying bad calibration into the expanded dataset.

## Deliverables

### 1. Fix compute_confidence for detail route
Add `compute_confidence_for_detail(graph, society_id)` in `backend/src/search/text.rs` that replaces `match_quality` (graph_driven_pct) with `fact_source_quality` (average of fact confidence scores). Update `properties.rs` to call it instead of `compute_confidence(..., 0.0)`.

**Files:** `backend/src/search/text.rs`, `backend/src/routes/properties.rs`

### 2. Fix freshness inflation for discovered/new nodes
Add minimum age gate: if all facts share the same `learned_at` timestamp (within 1 second), cap freshness at 0.5. Distinguishes "freshly enriched" from "just created with seed data."

**Files:** `backend/src/search/text.rs`

### 3. Resolve canonical_builder at query time
In `extract_builder_trust`, after finding builder via BuiltBy edge, check for `canonical_builder` fact. If present, follow to canonical builder node and read delivery facts from there.

**Files:** `backend/src/routes/enrichment.rs`

### 4. Add RERA portal URL link in Data Provenance sidebar
Add "Verify on RERA" link when `rera_portal_url` is available.

**Files:** `frontend/src/pages/PropertyPage.tsx`

### 5. Deduplicate trust badges between header and sidebar
Remove TrustBadge and DataFreshnessBadge from header. Keep ConfidenceMeter as compact summary. Data Provenance sidebar is the authoritative location.

**Files:** `frontend/src/pages/PropertyPage.tsx`

### 6. Add tests for new confidence functions
- `test_confidence_detail_uses_fact_quality`
- `test_freshness_capped_for_bulk_created_nodes`
- `test_freshness_not_capped_after_enrichment`
- `test_canonical_builder_resolution`

**Files:** `backend/src/search/text.rs`, `backend/src/routes/enrichment.rs`

## Success Criteria

1. Property detail page confidence uses fact-quality-based scoring (not `graph_driven_pct=0.0`)
2. Newly discovered nodes with 2 facts and same-timestamp creation score "Low" not "Moderate"
3. Societies linked to orphan builders show canonical builder delivery data
4. RERA portal URL is clickable in Data Provenance sidebar
5. Trust badges appear only in Data Provenance sidebar, not duplicated in header
6. 4+ new Rust tests pass (total 63+)
7. `cargo test` passes, `npm run build` succeeds
