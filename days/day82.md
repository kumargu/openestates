# Day 82: RERA Backfill + Market Pricing Blitz — Push to Tier A 95+ and Enrichment 0.80+

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 10 of 14. Phase 2: Scale to 100 — Day 4 of 5.

## Day 81 Grade
**A.** Fixed learn_society (Gemini primary+Claude fallback), registered fetch_market_pricing+fetch_images in SKILL_REGISTRY, normalized 18 uppercase names, enriched 42 new societies with learn_society (48→90 coverage). Tier A: 84→86, enrichment avg: 0.668→0.752, overall: 0.875→0.896. 75 tests pass.

## Feedback Disposition

### Day 81 Builder:
- **Enrichment score (0.668→0.752) did not hit 0.80 target** — FIX. Root cause: 87/100 societies missing pricing facts (the single largest gap). Batch-run fetch_market_pricing today.
- **FetchImagesSkill produces no SourcedFact objects** — ACCEPT. Images are on disk; engine marks fresh based on execution timestamp.
- **Claude API 400 errors persist** — ACCEPT. Gemini fallback works. Not blocking.
- **14 societies still lack learn_society synthesis** — 10 still missing summary category. FIX: re-run learn_society on those 10.
- **BaseSkill retry logic with exponential backoff can cause long hangs** — DEFER.

### Day 81 Verifier:
- **Tier A at 86 (not 95+ target)** — FIX. 11 Tier B societies have RERA=0.06 (legacy seed data with only `rera_registered` fact). Run seed_from_rera --backfill to get proper RERA data. Score gain: +0.22 per society, all 11 become Tier A. Tier A count: 86→97.
- **Enrichment average at 0.752 (not 0.80+ target)** — FIX. Pricing is missing on 87 societies (the dominant gap). Run fetch_market_pricing batch. 50+ completions pushes enrichment to 0.85+.
- **Run embed_entity on societies missing embeddings** — FIX. Only 1 society missing.
- **Consider adding learn_society sentiment keys as 6th enrichment category** — DEFER.
- **score_society not run after learn_society enrichment** — DEFER. score_society output keys are NOT in data_quality.py's ENRICHMENT_CATEGORIES. Valuable for product but not for numeric targets. Schedule for Day 83.

## Goal

Push all three metrics past their targets: Tier A from 86→95+, enrichment average from 0.752→0.80+, overall from 0.896→0.92+. Two high-leverage actions: RERA backfill on 11 legacy societies and market pricing on 87 societies.

## Product Reason

11 societies show "Discovered" root source with minimal RERA data — users see no trust signal on these. RERA backfill gives them government-verified facts and trust badges. Market pricing gives every society page concrete per-BHK price ranges that buyers need for comparison. These are the two most visible data gaps in the product.

## Deliverables

### D1: Baseline snapshot
**Command:** `python3 pipeline/scripts/data_quality.py`
- Record before metrics: Tier A count, enrichment avg, overall avg.

### D2: RERA backfill on 11 legacy societies
**File:** `pipeline/skills/seed_from_rera.py`
**Command:** `python3 pipeline/skills/seed_from_rera.py --backfill`

The 11 societies with RERA=0.06 have `root_source: "Discovered"`, only 1 rera fact (`rera_registered`), and `has_rera_dates` returns False. The backfill will:
1. Fuzzy-match each against RERA listing
2. Fetch detail page from RERA portal
3. Add ~15 RERA facts per society
4. Update `root_source` to "Rera"

**Expected result:** Each society's RERA score goes from 0.06→~0.94. All 11 move from Tier B to Tier A. Tier A count: 86→97.

**Risk:** Some societies may not fuzzy-match. Even 8/11 matching hits the 95+ target.

### D3: Batch fetch_market_pricing on 87 societies
**File:** `pipeline/skills/fetch_market_pricing.py`
**Command:** `python3 pipeline/skills/fetch_market_pricing.py --all`

87 societies are missing pricing facts. Each successful run adds `price_per_sqft`, `pricing_2bhk`/`pricing_3bhk` etc, which satisfies the "pricing" enrichment category (+0.20 enrichment score per society).

If 60 of 87 get pricing, average enrichment gain = 60 * 0.20 / 100 = 0.12. New enrichment avg: 0.752 + 0.12 = 0.872 (well past 0.80 target).

### D4: Fill remaining enrichment gaps
**File:** `pipeline/enrich.py`

Run targeted enrichment for:
1. **Google reviews (26 missing)**
2. **Summary/learn_society (10 missing)**
3. **Embeddings (1 missing)**

### D5: Final quality report
**Command:** `python3 pipeline/scripts/data_quality.py`
- Record after metrics. Compare against targets.

### D6: Tests
**Command:** `cd backend && cargo test`
- All 75+ existing tests must pass (regression check only).

## Technical Guidance

### Execution order
1. D1 (baseline) — before anything else
2. D2 (RERA backfill) — independent and fast (~5 min)
3. D3 (market pricing) — long pole (~30-60 min for 87 societies)
4. D4 (remaining gaps) — run after D3
5. D5 (final report) — after all enrichment
6. D6 (tests) — anytime

### No Rust changes needed
All work is in Python pipeline. Backend loads KG from disk at startup.

## Constraints
- No Rust code changes today. Pure pipeline/data day.
- Gemini free tier rate limits: ~15 requests/minute, 1500/day.
- Do not modify `_persist_facts_locally` behavior.
- Do not change data_quality.py scoring weights or categories.
- All 75+ existing backend tests must pass.

## Success Criteria
1. RERA backfill succeeds on >= 8 of 11 legacy societies
2. Tier A count: 86→95+ (target: 95)
3. fetch_market_pricing succeeds on >= 50 of 87 societies
4. Enrichment average: 0.752→0.80+ (target: 0.80)
5. Overall quality average: 0.896→0.92+ (target: 0.92)
6. 75+ backend tests pass
7. Before/after data quality reports saved
