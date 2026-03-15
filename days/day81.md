# Day 81: Close Enrichment Gap — learn_society Fix, Skill Registration, Name Normalization

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 9 of 14. Phase 2: Scale to 100 — Day 3 of 5.

## Day 80 Grade
**A.** Freshness calibration (0.35→1.0). Batch enrichment 2517 facts across 100 societies. Added `_persist_facts_locally` fallback. Tier A: 30→84, zero-enrichment: 43→0, overall avg: 0.711→0.875. 72+3 tests pass.

## Feedback Disposition

### Day 80 Builder:
- **Claude API returns 400 errors for learn_society** — FIX. Diagnose API key / model param issue. Add Gemini fallback so learn_society can run even if Claude is down.
- **_persist_facts_locally only keeps one fact per key** — ACCEPT. Latest fact per key is correct behavior. Intentional.
- **fetch_market_pricing and fetch_images not registered in SKILL_REGISTRY** — FIX. Register both.

### Day 80 Verifier:
- **52 societies lack summary fact (learn_society didn't run)** — FIX. Root-cause 400 errors, then batch-run learn_society on all 52.
- **Freshness 1.0 everywhere = zero differentiation** — ACCEPT. Will degrade naturally.
- **Normalize society names to title case** — FIX. 18 societies have ALL-UPPERCASE names.
- **Register fetch_market_pricing and fetch_images** — FIX. (Same as builder feedback.)
- **Add staleness projection to data quality report** — DEFER. Not on critical path.

## Goal

Close the enrichment gap: fix learn_society API errors, register missing skills, normalize names, and batch-run enrichment on 52 under-enriched societies. Move Tier A from 84 to 95+ and enrichment average from 0.668 to 0.80+.

## Product Reason

100 RERA-rooted societies mean nothing if half lack summaries and context. Users landing on a society page with only RERA data get no value. Closing the enrichment gap turns raw data into product-ready content.

## Deliverables

### D1: Diagnose and fix learn_society API errors
**File:** `pipeline/skills/learn_society.py`
- Investigate the 400 error: check model parameter, API key validity, request payload size.
- Add Gemini 2.5 Flash as primary provider (matching score_society pattern).
- Add explicit error logging on failure.

### D2: Register missing skills in SKILL_REGISTRY
**File:** `pipeline/enrich.py`
- Add `fetch_market_pricing` to SKILL_REGISTRY.
- Add `fetch_images` to SKILL_REGISTRY.
- Verify both callable from batch runner.

### D3: Normalize uppercase society names
**Files:** `pipeline/skills/seed_from_rera.py`, `pipeline/scripts/normalize_rera.py`
- In `seed_from_rera.py`: add `.title()` normalization on ingestion.
- Write one-time fixup in `normalize_rera.py` to title-case existing uppercase names.
- Run fixup. Expect ~18 corrected.

### D4: Batch-run learn_society on 52 missing societies
**File:** `pipeline/enrich.py`
- After fixing API, run learn_society targeting 52 societies lacking summaries.
- Rate limit appropriately.
- Target: 45+ of 52 get summary facts.

### D5: Run data quality report
- Re-run after enrichment. Capture before/after metrics.

### D6: Tests
- `cargo test` >= 75 tests pass.
- Integration tests pass.

## Constraints
- No Rust changes today. Pure pipeline/data day.
- Gemini free tier rate limits apply. Batch with delays.
- Do not change `_persist_facts_locally` behavior.
- All 75 existing backend tests must pass.

## Success Criteria
1. learn_society runs without 400 errors on >= 45 of 52 missing societies
2. `fetch_market_pricing` and `fetch_images` callable from SKILL_REGISTRY
3. Zero ALL-UPPERCASE society names remain
4. Tier A count: 84 → 95+
5. Enrichment average: 0.668 → 0.80+
6. Overall quality average: 0.875 → 0.90+
7. 75+ backend tests pass
