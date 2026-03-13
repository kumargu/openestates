# Day 68: Data Quality Hardening and Feedback Resolution

## Mid-Sprint Status (Sprint 3, Day 10 of 14)

**Coverage snapshot after day 67:**
- 70 BuiltBy edges (complete)
- 32 builders with delivery_rate (complete)
- 70 SocietyInArea edges (complete — was 22, now 70, zero unmatched)
- 59/70 societies with project_status (11 missing are Discovered-root with no RERA dates)
- 15 untracked society nodes (RERA-seeded, not yet committed)
- 12 untracked builder nodes (RERA-seeded, not yet committed)
- 3 area nodes (devanahalli, jp-nagar, sarjapur) with only 2 facts each (name + city)
- 5 orphan builder nodes (duplicates — no BuiltBy edges point to them)
- 5 untracked frontend components (BuilderTrustBadge, ConfidenceMeter, DataFreshnessBadge, ProjectStatusTag, TrustBadge)

## Feedback Resolution

### Day 67 Feedback

1. **Fuzzy match threshold 0.5 Jaccard never exercised** — Accept. All 70 SocietyInArea matches were direct slug hits. The fuzzy path exists as a safety net for future nodes. No change needed.

2. **graph_area_match penalty -1.0 tradeoff** — Accept. The penalty hierarchy (exact=0.0, graph=-1.0, nearby=-2.0) correctly captures confidence gradient.

3. **Area node claims included for all matched societies** — Accept. Working as designed with dedup_by handling.

4. **Marathahalli--Whitefield Road slugification fragile with em-dash** — **Fix today.** Normalize em/en-dashes in slugify, with backward-compatible fallback.

5. **Untracked society nodes don't have SocietyInArea edges yet** — **Fix today.** Commit and re-run backfill scripts.

6. **No area nodes for JP Nagar, Devanahalli, or Sarjapur have substantive facts** — **Fix today.** Enrich these 3 area nodes.

### Day 65 Feedback (still open)

1. **fact_count/15 coverage formula uses arbitrary threshold** — **Fix today.** Raise FACT_COVERAGE_THRESHOLD from 10 to 25 (~p25 of enriched societies).

2. **DataFreshnessBadge and ConfidenceMeter are untracked (git)** — **Fix today.** Commit all 5 untracked frontend components.

### Day 66 Feedback (still open)

1. **5 orphan builder nodes (duplicates)** — Defer to Sprint 4. Requires builder dedup strategy.

2. **Jaccard threshold 0.4 in backfill_built_by_edges may be too permissive** — Accept the risk. Zero false positives with 70 edges.

## Goal

Data quality hardening: fix calibration issues and data gaps from days 65-67. Commit all untracked files. Enrich 3 sparse area nodes. Harden substring matching. Recalibrate confidence formula. No new features.

## Product Reason

The trust UI surfaces are only as good as the data behind them. The ConfidenceMeter showing "High" for a society with 11 facts and "High" for one with 49 facts reduces trust in the meter itself. Sparse area nodes mean some search results lack area context. Untracked files mean the next clean checkout breaks. This day makes the existing system honest.

## Deliverables

### 1. Recalibrate FACT_COVERAGE_THRESHOLD from 10 to 25

**File:** `backend/src/search/text.rs`

Change the threshold constant and update the comment to note calibration basis (~p25 of enriched societies, median=49).

### 2. Harden graph_area_match substring matching

**File:** `backend/src/search/text.rs`

Add minimum length guard (>= 4 chars) for substring containment to prevent false positives with short area names. Exact matches always accepted regardless of length.

### 3. Normalize em-dash in slugify function

**File:** `pipeline/skills/backfill_located_in_edges.py`

Normalize em-dashes (U+2014) and en-dashes (U+2013) to regular hyphens before slugifying. Use as secondary lookup to preserve backward compatibility with existing node filenames.

### 4. Enrich 3 sparse area nodes

Enrich devanahalli, jp-nagar, sarjapur area nodes with substantive facts (metro_status, traffic_reality, waterlogging_risk, price_trend, livability_summary, area_vibe, school_quality, etc.) so they match the 13-fact standard of other area nodes.

### 5. Re-run backfill scripts on expanded node set

After committing the 15 new society nodes, re-run edge backfill scripts so these societies get their SocietyInArea and BuiltBy edges, project_status classifications, and builder delivery rates.

### 6. Commit all untracked files

36 untracked files: 15 society nodes, 12 builder nodes, 3 area nodes, 5 frontend components, 1 pipeline skill.

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/search/text.rs` | Raise FACT_COVERAGE_THRESHOLD to 25; harden graph_area_match substring guard |
| `pipeline/skills/backfill_located_in_edges.py` | Normalize em/en-dash in slugify |

## Constraints

- No new frontend components or pages
- No new API endpoints
- No new features — data quality and calibration only
- All currently passing tests must continue to pass

## Success Criteria

1. `FACT_COVERAGE_THRESHOLD` is 25 in code
2. `graph_area_match` has minimum length guard for substring matching
3. `backfill_located_in_edges.py` handles em/en-dash gracefully
4. 3 sparse area nodes (devanahalli, jp-nagar, sarjapur) enriched with substantive facts
5. SocietyInArea edges >= 82 (was 70)
6. BuiltBy edges >= 80 (was 70)
7. `cargo check` passes
8. `cargo test` passes (44+ tests)
9. `npm run build` succeeds
10. Zero untracked knowledge/component files in `git status`

## Deferred Items (Sprint 4)

| Item | Reason |
|------|--------|
| 5 orphan builder nodes (duplicates) | Requires builder dedup strategy |
| Set-based edge index for O(1) lookups | Only 140 edges; linear scan is fine |
| Fuzzy match test cases for backfill scripts | 100% direct-match success, low risk |
| 11 Discovered societies without project_status | No RERA dates available |
