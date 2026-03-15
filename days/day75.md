# Day 75: Phase 0 Gate Close — Search Verification, Seller Matching Test, Gate Decision

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 3 of 14. Phase 0: Validation Gate — FINAL DAY.

## Day 74 Grade
**A.** Deduped 101 facts, normalized RERA, enriched market pricing on 8 societies (71 new facts). Validation improved from 178 PASS/34 WARN to 200 PASS/12 WARN. All targets exceeded.

## Feedback Addressed
- **total_units 6/10 WARN (Day 74 verifier):** Root cause: RERA skill writes `rera_total_units` but validator checks `total_units`. Fix validator to accept either key. 4/6 resolved instantly.
- **Google sentiment 3/10 (Day 74 verifier):** Run `fetch_google_reviews` on 2 missing societies (Sarang, NBR Group).
- **NBR Group sparse (Day 74 verifier):** 17 facts, discovered via Gemini, no RERA evidence. Accept as known limitation.
- **Seller matching untested (Phase 0 gate):** Verify 4 seller-property pairs via API.
- **Search verification untested (Phase 0 gate):** Build and run search verification script.

## Goal

Close Phase 0 by: (1) fixing `total_units` key mismatch in validator, (2) running Google reviews on 2 missing societies, (3) verifying search returns 10 properties, (4) verifying seller matching for 4 test cases, (5) running final validation, (6) writing Phase 0 gate summary.

## Product Reason

Phase 0 is the quality gate that proves every layer works before scaling to 100 societies. Without verifying search and seller matching, we would scale a pipeline that might not serve users.

## Deliverables

### 1. Fix validator `total_units` key mismatch
**File:** `pipeline/validation/validate_phase0.py`
Accept `rera_total_units` as equivalent to `total_units`. Eliminates 4 WARNs instantly.

### 2. Run `fetch_google_reviews` on 2 missing societies
- `sarang-by-sumadhura-phase-2`
- `nbr-group-apartments-near-wipro-sarjapur-road`

### 3. Search verification script
**File:** `pipeline/validation/verify_search.py`
- Reads validation set, constructs 2-3 NL queries per property
- Hits `GET http://localhost:4000/api/search?q=...`
- Verifies each property appears in at least 1 query result
- Reports PASS/WARN per property

### 4. Seller matching verification script
**File:** `pipeline/validation/verify_sellers.py`
- Tests 4 seller-property pairs from validation set
- Verifies seller exists, property linked, interest flow works
- Reports PASS/FAIL per test case

### 5. Final Phase 0 validation
Run `validate_phase0.py` — target: 0 FAIL, ≤5 WARN (all NBR Group).

### 6. Phase 0 Gate Summary
**File:** `pipeline/validation/phase0_gate_summary.md`
Gate decision, metrics, improvement timeline, accepted exceptions, Phase 1 readiness.

## Success Criteria

1. Validator accepts `rera_total_units` — 4 WARNs eliminated
2. Google reviews run on 2 societies — at least 1 gains facts
3. Search: all 10 properties found via at least 1 query
4. Seller: all 4 test cases pass
5. Final: 0 FAIL, ≤5 WARN (all NBR Group)
6. Gate summary written with PASS decision
7. Phase 0 CLOSED — ready for Phase 1
