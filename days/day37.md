# Day 37: Search Quality Fixes — Address Day 36 Findings

## 1. Goal

Fix the top issues found during the fuzzy testing checkpoint (Day 36). This day's scope is determined by the evaluation report at `docs/eval_search_v1.md`.

## 2. Product Reason

Day 36 tested the search system against 20 messy real-world queries and documented what works and what doesn't. Today we fix the highest-impact issues. This is the feedback loop that turns a technically functional search into a product-quality one.

## 3. Deliverables

**This day is adaptive — deliverables depend on Day 36 findings.** Below are the LIKELY fixes based on known architectural gaps. The builder should read `docs/eval_search_v1.md` FIRST and prioritize accordingly.

### Likely Fix 1: Preference expansion taxonomy gaps

The taxonomy from Day 32 may miss mappings that real queries need. If the eval found queries where preferences weren't matched despite relevant facts existing, expand the taxonomy.

Example gaps to check:
- "breathing room" → should map to density, open_space
- "shady builder" → should map to builder_reputation, litigation_risk
- "daily life easier" → should map to maintenance_quality, community_vibe, livability
- "not too far" → should map to metro_distance, commute

**Fix:** Update the Gemini prompt taxonomy in `backend/src/search/text.rs`.

### Likely Fix 2: Scoring weight calibration

If the eval found that:
- All results score very similarly (no differentiation)
- Negative preferences don't penalize enough
- Buyer archetypes don't produce different enough rankings

**Fix:** Adjust weights in the archetype profiles and the `score_society_for_intent()` function. The negative preference weight multiplier may need to increase from 1.0 to 1.5-2.0.

### Likely Fix 3: Explanation genericness

If many explanation cards say similar things ("Signals suggest maintenance is good"), the display_templates on facts may be too uniform.

**Fix:** Audit the display_templates across society nodes. Improve them to be more specific. This might require updating skills that generate facts (learn_society.py, score_society.py) to produce better templates.

### Likely Fix 4: Missing fact coverage

If many queries hit "no_data" for common preferences, the enrichment hasn't covered key dimensions.

**Fix:** Run targeted enrichment for the most common missing fact keys. Use `pipeline/skills/identify_gaps.py` to find what's missing, then `pipeline/skills/learn_society.py` or `pipeline/skills/score_society.py` to fill gaps.

### Likely Fix 5: Area signal quality

If area-level concerns aren't surfacing well (e.g., waterlogging risk not penalizing Whitefield societies), the area nodes may lack the right fact keys or scoring_hints.

**Fix:** Check area nodes in `data/knowledge/nodes/area/`. Ensure key facts have `scoring_hint` with correct direction (LowerIsBetter for risks) and `answers_preferences` that map to user language.

### D-Final: Re-run evaluation

After fixes, re-run the 20-query evaluation from Day 36 and compare scores. Document improvement in `docs/eval_search_v1.md` (append a "Post-Fix" section).

## 4. Technical Guidance

**Start by reading:** `docs/eval_search_v1.md` — the entire day's work is driven by this document.

**Files likely to change:**
- `backend/src/search/text.rs` — taxonomy updates, scoring weight adjustments
- `backend/src/search/scoring.rs` — if score differentiation is weak
- `pipeline/skills/learn_society.py` or `score_society.py` — if display_templates need improvement
- Area node JSON files — if area facts lack scoring_hints
- `pipeline/eval_search.py` — re-run evaluation

**Priority order:** Fix what affects the most queries first. If 12/20 queries have weak negative preference handling, fix that before fixing 2/20 queries with wrong archetype detection.

## 5. Constraints

- Do NOT add new features — only fix issues found in evaluation
- Do NOT change the architecture — only calibrate weights, expand taxonomy, improve data
- Focus on the top 3-5 issues, not every small nit
- Re-run the eval after fixes to prove improvement

## 6. Success Criteria

- [ ] Read `docs/eval_search_v1.md` and identified top 3-5 priority fixes
- [ ] Fixes implemented for highest-impact issues
- [ ] Re-ran 20-query evaluation
- [ ] Documented before/after comparison in eval report
- [ ] Overall evaluation scores improved (more queries score 4-5 on rubric)
- [ ] Negative preferences now visibly penalize results for relevant queries
- [ ] `cargo check` passes
- [ ] `npm run build` passes
