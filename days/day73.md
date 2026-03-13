# Day 73: Sprint 4 Kickoff — Validation Gate Baseline

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 1 of 14. Phase 0: Validation Gate.

## Day 72 Grade
Clean. Sprint 3 closed with all 65 tests passing, clean build, retrospective written, clean working tree. Local and remote have diverged (7 local vs 5 remote commits). Must resolve before new work.

## Feedback Addressed
- **Git divergence (Day 72):** Resolve local/remote divergence as first step
- **Zero frontend tests (Day 72):** Not Phase 0 scope, noted for later in sprint
- **No integration tests (Day 72):** Phase 0 validation IS the integration test — programmatic end-to-end verification
- **Freshness threshold (Day 70/71):** Accept current behavior, monitor during validation
- **canonical_builder query-time resolution (Day 70):** Accept risk, validation will expose if it matters
- **Daily commits (Day 72 verifier):** Sprint 4 targets daily commits starting today

## Goal

Establish the Sprint 4 validation baseline. Resolve git divergence, select 10 properties for end-to-end validation, build a Python validation harness that checks each property against every Phase 0 criterion, and produce a gap report.

## Product Reason

Sprint 4's mandate is "clean house, validate end-to-end, scale to 100 properties." Before scaling, we need proof that the current 10 best-enriched properties actually work through every layer. A validation script that runs in seconds replaces hours of manual checking and becomes the regression gate for all Sprint 4 work.

## Deliverables

### 1. Resolve git divergence
- `git fetch origin && git merge origin/main`
- Resolve conflicts favoring local Sprint 3 work
- Verify: `cargo test` (65+ tests), `npm run build`

### 2. Select 10 validation properties
Pick 10 properties maximizing coverage: multiple areas, builders, BHK configs, some with sellers, some without. Store as `pipeline/validation/validation_set.json`.

### 3. Build validation harness (main deliverable)
Create `pipeline/validation/validate_phase0.py` checking each property against:

- **Seed data:** required fields, hero image exists, society mapping
- **KG society node:** RERA facts, builder info, project status, units
- **Market pricing:** pricing_* facts, price_per_sqft, configurations
- **Reviews:** Reddit threads, Google sentiment
- **Property KG node:** exists, embedding computed, fact count
- **Trust badges:** RERA badge eligibility, root source
- **Provenance chain:** every fact has source, skill_id, learned_at; flag duplicates
- **Seller matching:** seller exists, property linked, prompt present, completeness
- **Search readiness:** embedding computed, self-describing scoring facts present

Output: structured report to stdout + JSON to `data/validation/phase0_baseline.json`.

### 4. Run validation and analyze gaps
Document baseline pass/warn/fail counts and top gaps ranked by frequency.

## Non-goals
- Do NOT fix any gaps today — harness is the deliverable
- Do NOT run enrichment skills
- Do NOT modify backend or frontend code

## Success Criteria
1. Git divergence resolved — merge commit, 65+ tests pass, clean build
2. `pipeline/validation/validation_set.json` with 10 property IDs
3. `python3 -m pipeline.validation.validate_phase0` runs < 5 seconds, produces report
4. `data/validation/phase0_baseline.json` with baseline gap data
5. Every Phase 0 criterion has a corresponding check
6. Gap analysis identifies actionable items for Days 74-75
