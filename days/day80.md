# Day 80: Mid-Sprint Review + Batch Enrichment of Under-Enriched Societies

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 8 of 14. Phase 2: Scale to 100 — Day 2 of 5. **MID-SPRINT REVIEW.**

## Mid-Sprint Review

1. **On track.** Phase 0 (validation) and Phase 1 (cleanup) completed ahead of schedule. Phase 2 scaling hit 100 societies on Day 79 (Day 1 of 5). The enrichment gap (43 zero-enrichment nodes, 51 nodes with enrichment score <= 0.2) is the single biggest quality debt.
2. **No pivot needed.** The remaining days (80-83) should execute enrichment as planned. Phase 3 (market pricing, Days 84-86) is well-served by `fetch_market_pricing.py` which already exists.
3. **Scope cuts: none required.** The enrichment pipeline (`pipeline/enrich.py`) already detects 480 work items across 99 societies. Budget cap and checkpointing make incremental execution safe. Freshness calibration is a 20-minute fix, not a day's work.
4. **Vision.md alignment:** Sprint 4 target is "100 RERA-rooted societies with rich data." We have 100 RERA-rooted societies but only 30 have rich data. Days 80-83 must close this gap or we enter Sprint 5 (search intelligence) with thin data backing half the graph.
5. **Accumulated tradeoffs:** (a) Freshness scoring gives 0.3 to 93/100 nodes — calibration issue, not real staleness. (b) `fetch_market_pricing` and `fetch_images` not registered in enrichment engine. (c) Integration tests pass all 3 tests (Day 78 concern resolved).

## Day 79 Grade
**A.** Expanded from 70 to 100 societies. Built `data_quality.py` with 5-dimension scoring. Added `--builder` mode to `seed_from_rera.py`. Resolved 7 unknown-area nodes. Tier A:30 B:66 C:4 D:0. 72 tests pass.

## Feedback Disposition

### Day 79 Builder:
- **Empty RERA addresses / manual area mappings** — DECISION: No. Edge-based inference at 0.7 confidence is acceptable. Manual mappings create maintenance burden.
- **Freshness scoring gives 0.3 to most nodes** — Fix today (D1). Calibrate thresholds.
- **Salarpuria, Mantri, DNR zero usable RERA addresses** — Accepted as data gap.
- **30 new societies missing enrichment** — Primary deliverable today (D2/D3).

### Day 79 Verifier:
- **Freshness calibration** — Fix today (D1).
- **4 Tier C societies** — Accept. Likely noise nodes from RERA.
- **Whitefield 13 societies** — Acceptable saturation. No action needed.

### Day 78 Builder (closing):
- **Integration test compilation errors** — RESOLVED. All 3 tests pass.
- **save_graph() migration path** — Deferred to Phase 3.
- **routes/societies.rs references data/intelligence/** — Stale feedback, no such file exists. Closed.

### Day 77 Builder (closing):
- **KG property nodes only have 7-8 fact keys** — Sprint 5 scope.
- **No images/hero_image in KG property nodes** — Sprint 5 scope.

## Goal

Two deliverables: (1) calibrate freshness scoring so the data quality report reflects reality, and (2) run batch enrichment on the 43-51 under-enriched societies using the existing enrichment engine.

## Product Reason

Half the knowledge graph has RERA skeleton data but zero community intelligence (no Reddit, no Google reviews, no embeddings). Search results for these societies show no preference coverage, no "why this matches" explanations, and low confidence scores. Enrichment is the difference between a thin listing and a property page worth sharing.

## Deliverables

### D1: Calibrate freshness scoring in data_quality.py

**File:** `pipeline/scripts/data_quality.py`

Problem: `score_freshness()` returns 0.3 for nodes where facts lack `created_at` timestamps. RERA facts may not populate this field consistently. Fix: use node-level `updated_at` as fallback, or adjust the no-timestamp penalty from 0.3 to a more reasonable value.

After fixing, re-run and confirm freshness average rises from 0.349 to >= 0.65.

### D2: Run batch enrichment on under-enriched societies

Execute enrichment in priority order with budget cap:

1. **Free tier first** — `search_reddit` + RERA refresh for ~45 nodes
2. **Cheap tier** — `fetch_google_reviews` + `learn_society` + `embed_entity`
3. **Market pricing** — `fetch_market_pricing` on high-priority nodes
4. **Images** — `fetch_images` for societies missing photos

Budget cap: $1.50 total across all tiers.

### D3: Re-run data quality report and verify improvement

After enrichment completes, re-run `pipeline/scripts/data_quality.py` and verify:
- Enrichment average: 0.4 → 0.6+
- Freshness average: 0.349 → 0.65+
- Zero-enrichment societies: 43 → <= 10
- Overall average: 0.711 → 0.74+

### D4: Tests

- `cargo test` >= 72 tests pass
- Integration tests pass all 3
- Data quality report produces valid output with improved scores

## Constraints

- Do NOT modify any Rust backend code today
- Budget cap enrichment at $1.50 total
- Do NOT delete existing nodes or facts
- If a skill pool enters backoff (3 consecutive failures), stop that pool

## Success Criteria

1. Freshness scoring calibrated: average freshness >= 0.65
2. Enrichment engine ran on >= 40 societies
3. Zero-enrichment society count drops from 43 to <= 10
4. Overall data quality average improves from 0.711 to >= 0.74
5. `cargo test` passes >= 72 tests
6. Updated data quality report saved
