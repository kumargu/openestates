# Day 79: Data Quality Scoring + RERA Expansion to 100 Societies

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 7 of 14. Phase 2: Scale to 100 — Day 1 of 5.

## Day 78 Grade
**A.** Phase 1 complete. Dead code deleted, stale references swept, seed_root removed, per-node save_node() for crash safety, `data/seed/` deleted. 72 tests pass.

## Feedback Disposition

### Day 76 Builder Feedback (still open):
- **KG area nodes lack median_price_per_sqft, price_range_per_sqft, sample_size** — Accepted as data gap. Phase 3 (Days 84-86) market pricing enrichment will fill these.
- **KG area nodes missing reddit_decision_drivers, reddit_concerns fact keys** — Accepted. learn_area skill needs extension; not Day 79 scope.
- **22/70 societies missing year_built and total_units** — Accepted. RERA detail pages provide total_units via `rera_total_units` for 37/59 RERA nodes. The 22 gaps are in the 17 RERA nodes without detail data + discovered nodes.

### Day 76 Verifier (cosmetic, still open):
- **backend/src/models/marketplace.rs:7 still references seed/bootstrap data in doc comment** — Fix today as warmup.
- **backend/src/knowledge/store.rs:211 doc comment still mentions bootstrap** — Fix today as warmup.

## Goal

Two deliverables that set up Phase 2 properly:

1. **Data quality scoring script** — a new Python script `pipeline/scripts/data_quality.py` that produces a structured quality report for all society nodes. This answers "what does good enough look like?" before we scale from 70 to 100. It measures completeness tiers, staleness, consistency, and produces a machine-readable report with per-node scores and an aggregate summary.

2. **RERA expansion to 100 societies** — seed ~30 new societies from underrepresented Bengaluru corridors using `seed_from_rera.py`. Enhance with builder-based seeding for more reliable area discovery.

## Product Reason

Scaling from 70 to 100 without quality guardrails means we might add 30 thin nodes that degrade search quality. Data quality scoring gives us a baseline and a repeatable check. Geographic diversity ensures the platform covers all major Bengaluru corridors, not just the East-South axis.

## Deliverables

### D1: Fix stale doc comments (warmup)

Update two doc comments that still reference seed/bootstrap:
- `backend/src/models/marketplace.rs` line 7
- `backend/src/knowledge/store.rs` line 211

Run `cargo test` after. Expected: 72 tests pass.

### D2: Data quality scoring script

**File:** `pipeline/scripts/data_quality.py`

Quality dimensions:
1. **Identity completeness** (0-1): Has name, area, city, builder_name?
2. **RERA completeness** (0-1): Has core RERA facts? Score based on fraction of 16 key RERA facts present.
3. **Enrichment depth** (0-1): Has Reddit, Google, pricing, embeddings?
4. **Fact freshness** (0-1): Average age of facts vs. staleness thresholds.
5. **Consistency** (0-1): Area node exists and matches, builder node exists and matches, edges present.

Tier classification: A (0.8+), B (0.6-0.8), C (0.4-0.6), D (<0.4).

Output: `data/validation/data_quality_report.json` + stdout summary table.

### D3: Enhance seed_from_rera.py with builder-based seeding

Add `--builder` mode that filters RERA listing by promoter name, fetches detail pages for address, infers area. Expand `_infer_area_from_address()` to include missing corridors.

### D4: Fix "unknown" area nodes

7 RERA-rooted societies have `area: "unknown"`. Parse their `rera_project_address` fact to infer area.

### D5: Seed ~30 new societies from RERA

Target underrepresented builders: Puravankara, Salarpuria, Mantri, Embassy, Shriram, DNR, Arvind, Mahindra, Tata Housing. Aim for 5+ previously uncovered corridors.

### D6: Run data quality report on expanded dataset

Run `data_quality.py` on full ~100-node dataset. Save report.

### D7: Tests

- `cargo test` >= 72 tests after D1
- `data_quality.py` produces valid report
- Total society count >= 95

## Constraints

- Do NOT run enrichment skills on new nodes today (Days 80-83 scope)
- Do NOT modify Rust backend
- Rate limit RERA portal: max 1 req/sec for detail pages
- Keep seed_from_rera.py backward compatible

## Success Criteria

1. `pipeline/scripts/data_quality.py` exists and produces a structured report
2. Data quality report shows tier distribution for all societies
3. `seed_from_rera.py` supports `--builder` mode
4. 7 "unknown" area nodes resolved where possible
5. Total society nodes >= 95
6. New societies cover at least 5 previously uncovered corridors
7. `data/validation/data_quality_report.json` committed with full report
8. `cargo test` passes with >= 72 tests
