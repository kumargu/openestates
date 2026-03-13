# Day 70: Git Commit, Test Coverage, and Sprint 3 Hardening

## Sprint Position
Sprint 3 (RERA Data Foundation & Trust Model), Day 12 of 14. 2 days remaining after today.

## Day 69 Grade
Day 69 delivered exactly what was promised: all 5 trust badge components wired into every surface, confidence_score on PropertyDetailResponse, and Data Provenance sidebar. Both builds clean, 44 tests pass. Solid execution.

## Feedback Resolution (Day 67)

1. **Fuzzy match threshold 0.5 Jaccard never exercised** — Add unit test for fuzzy path today. All 70 societies match via direct slug, but code path should be tested.
2. **graph_area_match penalty -1.0** — Implemented and accepted. No change needed.
3. **Area node claims via SocietyInArea edges** — Working as designed. No change.
4. **Marathahalli--Whitefield Road slugification fragile** — Fixed in day 68 (em-dash normalization).
5. **Untracked society nodes lack edges** — Fixed. All 15 have SocietyInArea and BuiltBy edges.
6. **No area nodes for JP Nagar, Devanahalli, Sarjapur** — Fixed in day 68 with 13 facts each.
7. **Substring containment false positives for short names** — Fixed in day 68 (>= 4 chars guard).
8. **Add test for graph_area_match edge cases** — Day 70 scope.

## Goal

Commit all Sprint 3 work (53 untracked + 250 modified files), add test coverage for graph_area_match and compute_confidence, and resolve the 5 orphan builder duplicates.

## Product Reason

Sprint 3 has 12 days of uncommitted work since Sprint 2 tag (3c0fe8f). A clean checkout would lose everything: RERA nodes, trust badges, edge backfills, confidence scoring, 15 new societies, enriched areas. The commit is the most important deliverable.

Test gaps for `graph_area_match` and `compute_confidence` are the two most critical untested search paths, flagged in day 67 feedback.

## Deliverables

### 1. Commit all Sprint 3 work (FIRST)
Create 2-3 logically grouped commits:
1. **Data + pipeline**: `data/knowledge/`, `pipeline/skills/*.py`, `pipeline/agent.py`, `pipeline/sprint_agent.py`
2. **Backend + frontend**: `backend/src/`, `frontend/src/`
3. **Docs**: `days/*.md`, `docs/vision.md`, `.claude/skills/`, `run-sprint.sh`

### 2. Add Rust tests for `graph_area_match`
**File:** `backend/src/search/text.rs`

Tests:
- Exact match: society with SocietyInArea edge, intent matches area
- Substring containment: area "Sarjapur Road", intent "Sarjapur" — matches (>= 4 chars)
- Short name guard: area "JP" (< 4 chars) — blocked from substring match
- No edge: society without SocietyInArea edge — returns false
- No graph: graph = None — returns false

### 3. Add Rust tests for `compute_confidence`
**File:** `backend/src/search/text.rs`

Tests:
- RERA source with many facts → "High" label
- Discovered source with few facts → "Low" label
- Threshold calibration: node at FACT_COVERAGE_THRESHOLD → coverage = 1.0

### 4. Resolve 5 orphan builder duplicates
Add `canonical_builder` fact to each orphan pointing to canonical ID. Add BuiltBy edges from orphan-referencing societies to canonical builders.

Orphans:
- `casagrand-builder-private-limited` → `casagrand`
- `century-real-estate-holdings-private-limited` → `century-real-estate`
- `dnr-corp` → `dnr-corporation`
- `sumadhura-constructions` → `sumadhura-infracon-pvt-ltd`
- `sumadhura-group` → `sumadhura-infracon-pvt-ltd`

### 5. Add fuzzy match tests to Python backfill script
**File:** `pipeline/skills/backfill_located_in_edges.py`

Test slugify em-dash normalization and fuzzy_match_area with matching/non-matching inputs.

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/search/text.rs` | Add test module for graph_area_match and compute_confidence |
| `data/knowledge/edges.json` | Add canonical builder redirect edges |
| `data/knowledge/nodes/builder/casagrand-builder-private-limited.json` | Add canonical_builder fact |
| `data/knowledge/nodes/builder/century-real-estate-holdings-private-limited.json` | Add canonical_builder fact |
| `data/knowledge/nodes/builder/dnr-corp.json` | Add canonical_builder fact |
| `data/knowledge/nodes/builder/sumadhura-constructions.json` | Add canonical_builder fact |
| `data/knowledge/nodes/builder/sumadhura-group.json` | Add canonical_builder fact |
| `pipeline/skills/backfill_located_in_edges.py` | Add slugify + fuzzy match tests |

## Success Criteria

1. All Sprint 3 work committed (zero untracked knowledge/component/skill files)
2. At least 4 new Rust tests for graph_area_match
3. At least 3 new Rust tests for compute_confidence
4. 5 orphan builder nodes have canonical_builder facts
5. `cargo test` passes with 50+ tests (was 44)
6. `npm run build` succeeds
7. Python slugify/fuzzy tests pass
8. `git status` shows clean working tree (or only intentionally uncommitted files)

## Deferred Items (Sprint 4)

| Item | Reason |
|------|--------|
| Delete orphan builder node files | Wait for canonical redirect validation |
| Set-based edge index | Only 140 edges; linear scan fine |
| Integration tests for full pipeline | Sprint 4 validation gate |
