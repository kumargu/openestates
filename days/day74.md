# Day 74: Fix RERA Facts, Deduplicate, Enrich Market Pricing

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 2 of 14. Phase 0: Validation Gate.

## Day 73 Grade
**B+.** Solid Phase 0 kickoff. Validation harness is clean, well-structured, runs in seconds. 178 PASS / 34 WARN / 0 FAIL is a strong baseline. 21-conflict merge handled correctly. Gap analysis identifies actionable priorities.

## Feedback Addressed
- **Market pricing (Day 73 verifier):** 100% gap — run fetch_market_pricing on all 10 validation societies today
- **RERA badge eligibility (Day 73 verifier):** Root cause is duplicate rera_registered facts (false at 0.8 + true at 1.0); dedup fixes 4/7, normalize fixes remaining
- **Duplicate fact keys (Day 73 verifier):** 25/70 societies affected, batch dedup across all nodes
- **seller_id (Day 73 verifier):** Already fixed in Day 73 — 7/10 have seller_id. Closed.
- **soc- prefix mismatch (Day 73 builder):** Accept risk. Validator strips prefix correctly. Not worth migration.
- **Freshness cap (Day 71):** Accept default. No action until RERA data >30 days old.
- **compute_confidence gdp (Day 71):** Accept risk. Validation harness monitors indirectly.
- **Orphan builder coverage (Day 71):** Accept risk. Not Phase 0 scope.
- **Zero frontend tests (Day 72):** Not Phase 0 scope. Sprint 5.
- **Staleness detection (Day 72):** Not Phase 0 scope. All RERA data <1 week old.

## Goal

Close the top 3 gaps from Phase 0 baseline: (1) deduplicate rera_registered facts across all 70 societies, (2) normalize RERA badge eligibility so the 7 broken nodes pass, (3) run market pricing enrichment on all 10 validation societies. Re-run validation and show measurable improvement.

## Product Reason

RERA trust badges are the core trust signal. 7/10 failing badge eligibility = 70% of transparency surface broken. Market pricing is the #1 user-facing gap — property pages without pricing data are useless for comparison. These two gaps are the minimum bar before Phase 0 can pass.

## Deliverables

### 1. Fact deduplication script
**File:** `pipeline/scripts/dedup_facts.py`

- Walk ALL `data/knowledge/nodes/{type}/*.json` files (not just societies)
- Group facts by `key`, when duplicates exist: keep highest confidence → most recent learned_at → highest version
- Atomic write (.tmp + rename)
- `--dry-run` support
- Target: 0 duplicate fact keys across all nodes

**Pattern:** Follow `pipeline/scripts/fix_scoring_hints.py` for batch-walk with dry-run.

### 2. RERA normalization
**File:** `pipeline/scripts/normalize_rera.py`

- Find society nodes where `rera_registered.value.data` is false/missing BUT `root_source` is "Rera"
- Confirm node has `rera_number` or `rera_ack_number` facts (safety check)
- Update to `{"type": "Bool", "data": true}` with confidence 1.0
- `--dry-run` support
- Run AFTER dedup

### 3. Market pricing enrichment
Run `fetch_market_pricing` skill on 10 validation societies.
- Needs `GOOGLE_AI_API_KEY`
- 10 societies x ~5s = ~50 seconds total
- Skill handles its own fact dedup on write

### 4. Re-run validation harness
`python3 -m pipeline.validation.validate_phase0`

**Expected improvement:**

| Gap | Day 73 | Day 74 target |
|-----|--------|---------------|
| market_pricing | 10/10 WARN | 0/10 |
| trust_badges_rera | 7/10 WARN | ≤1/10 WARN |
| provenance_duplicates | 6/10 WARN | 0/10 |
| total_units | 6/10 WARN | 6/10 (no change) |
| reviews | 3/10 WARN | 3/10 (no change) |

Target: 34 WARN → ≤22 WARN, 0 FAIL.

## Technical Guidance

**Execution order matters:**
1. `dedup_facts.py` first (removes duplicates)
2. `normalize_rera.py` second (fixes remaining false values)
3. Market pricing enrichment third (adds new facts)
4. Validation harness last (measures result)

**dedup_facts.py:** Sort by `(-confidence, -learned_at_timestamp, -version)`. Atomic write. Process all node types.

**normalize_rera.py:** Only touch `root_source == "Rera"` nodes. Require `rera_number` OR `rera_ack_number` facts as confirmation. Set `scoring_hint: {"direction": "TextMatch", "weight": 3.0}`.

## Constraints

- Do NOT modify backend Rust code — pure data cleanup day
- Do NOT modify validation harness — stable measurement tool
- Do NOT run enrichment skills other than market pricing
- If Gemini API fails for any society, log and continue

## Success Criteria

1. `dedup_facts.py` exists, runs in <5s, removes all duplicate keys
2. `normalize_rera.py` exists, runs in <2s, fixes RERA-rooted nodes
3. Market pricing facts present on 10/10 validation societies
4. Re-run validation: 0 provenance_duplicates, ≤1 trust_badges_rera, 0 market_pricing
5. Total WARN ≤22 (down from 34)
