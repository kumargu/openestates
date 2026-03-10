# Day 40: Data Enrichment Sprint + Second Fuzzy Checkpoint

## 1. Goal

Bulk enrich all entities with missing critical facts, then re-run the full evaluation to establish a quality baseline for context-based search v1.

## 2. Product Reason

The search quality ceiling is bounded by data coverage. Days 31-39 built the search architecture. But if 30% of societies lack `maintenance_quality` facts, or area nodes lack `waterlogging_risk` scoring_hints, the architecture can't deliver good results.

This day is half enrichment work, half final evaluation. It establishes the v1 quality baseline that we can track improvements against.

## 3. Deliverables

### D1: Gap analysis across all entities

Run `pipeline/skills/identify_gaps.py` (or equivalent) to audit:

1. **Critical fact coverage** — for each society, check presence of:
   - maintenance_quality
   - family_friendly
   - builder_trust / builder_reputation
   - value_for_money
   - water_supply or waterlogging_risk
   - calm_environment or noise_score

2. **Scoring hint coverage** — for each fact, check if it has:
   - `scoring_hint` with direction + weight
   - `answers_preferences` that maps to user language

3. **Area fact coverage** — for each area, check:
   - waterlogging_risk / water_supply
   - traffic_score
   - metro_access
   - livability_score
   - All with scoring_hints

Output: gap report at `docs/enrichment_gaps.md` with counts and priorities.

### D2: Targeted enrichment run

For the top 20 entities with the most gaps:
1. Run `learn_society` (if missing Reddit-derived facts)
2. Run `score_society` (if missing scored dimensions)
3. Run `embed_entity` with aspect embeddings (if missing embeddings)

Use `pipeline/enrich.py` or direct skill calls. Respect rate limits.

### D3: Fix scoring_hints on existing facts

Some facts may exist but lack `scoring_hint` or `answers_preferences`. Write a one-time migration script that:
1. Scans all facts across all nodes
2. For facts with known keys (maintenance_quality, family_friendly, etc.), adds the appropriate scoring_hint if missing
3. Adds answers_preferences mappings if missing

This is a data quality fix, not a code change.

### D4: Re-run full evaluation

Run the same 20-query evaluation from Day 36:
1. Execute `pipeline/eval_search.py` against live backend
2. Score each query on the 8-dimension rubric
3. Compare against Day 36 scores and Day 37 post-fix scores

### D5: Final evaluation report

Output: `docs/eval_search_v2.md`

Structure:
1. **Coverage report:** fact coverage before/after enrichment
2. **Quality scores:** per-query rubric scores
3. **Comparison:** v1 (Day 36) vs v2 (Day 40) scores
4. **System stats:**
   - Average graph_driven_pct across queries
   - Average facts_consulted per result
   - Average latency
   - Percentage of queries where negative preferences produce visible ranking changes
5. **Remaining gaps:** what would need to be fixed for v3
6. **Verdict:** is context-based search demonstrably better than filter-only search?

### D6: Baseline comparison (final)

For the same 5 representative queries as Day 36:
- **Baseline A:** structured filter only
- **Baseline B:** pre-Day-31 search
- **Current system:** full context search stack

Document the delta. This is the proof that the 10-day sprint produced real product improvement.

## 4. Technical Guidance

**Scripts to run:**
- `pipeline/skills/identify_gaps.py` — gap analysis
- `pipeline/enrich.py` — targeted enrichment
- `pipeline/scripts/reembed_all.py` — re-embed after enrichment
- `pipeline/eval_search.py` — evaluation

**Migration script for scoring_hints:**
```python
# One-time fix: pipeline/scripts/fix_scoring_hints.py
HINT_DEFAULTS = {
    "maintenance_quality": {"direction": "HigherIsBetter", "weight": 2.0},
    "family_friendly": {"direction": "HigherIsBetter", "weight": 2.0},
    "water_supply": {"direction": "HigherIsBetter", "weight": 2.0},
    "waterlogging_risk": {"direction": "LowerIsBetter", "weight": 2.0},
    "noise_score": {"direction": "LowerIsBetter", "weight": 1.5},
    "traffic_score": {"direction": "LowerIsBetter", "weight": 1.5},
    "builder_reputation": {"direction": "HigherIsBetter", "weight": 1.5},
    # ... etc
}

PREF_DEFAULTS = {
    "maintenance_quality": ["good maintenance", "well maintained", "maintenance"],
    "family_friendly": ["family friendly", "family", "kids", "children"],
    "water_supply": ["water", "water issues", "tanker"],
    "waterlogging_risk": ["flooding", "waterlogging", "water issues"],
    # ... etc
}
```

**Order of operations:**
1. Gap analysis (understand what's missing)
2. Targeted enrichment (fill the biggest gaps)
3. Fix scoring_hints (make existing facts scorable)
4. Re-embed (update embeddings with new facts)
5. Reload backend (`POST /api/admin/reload`)
6. Run evaluation

**The backend must be running** for the evaluation. Start with `cargo run` in backend/.

## 5. Constraints

- Enrichment budget: max $5 in LLM costs for this day's enrichment
- Rate limits: respect existing throttling (5s between Reddit calls, 10s between LLM calls)
- Do NOT change search code — this day is data quality + evaluation only
- Do NOT change the evaluation rubric — same rubric as Day 36 for apples-to-apples comparison
- The scoring_hint migration is additive — don't overwrite existing hints

## 6. Success Criteria

- [ ] Gap analysis report generated at `docs/enrichment_gaps.md`
- [ ] Top 20 entities enriched with missing critical facts
- [ ] scoring_hints added to facts that lacked them
- [ ] answers_preferences added to facts that lacked them
- [ ] All entities re-embedded with updated facts
- [ ] 20-query evaluation completed
- [ ] Evaluation report at `docs/eval_search_v2.md` with before/after comparison
- [ ] graph_driven_pct > 70% for majority of queries (up from Day 36 baseline)
- [ ] At least 15/20 queries score >= 3/5 on relevance rubric
- [ ] Negative preference queries show measurable ranking differences
- [ ] Baseline comparison demonstrates improvement over filter-only search
- [ ] `cargo check` passes
- [ ] `npm run build` passes
