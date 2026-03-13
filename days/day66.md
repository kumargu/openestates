# Day 66: Mid-Sprint Review and Data Quality Consolidation

## Mid-Sprint Assessment (Sprint 3: RERA Data Foundation and Trust Model)

1. **Feature coverage is strong.** All planned trust surfaces are built: root_source, trust badges, project status tags, builder trust badges, data freshness indicators, and confidence meters. The UI and backend infrastructure for Sprint 3 is complete.
2. **Data coverage undermines the features.** Only 22 of 70 societies have BuiltBy edges, meaning 48 societies cannot benefit from builder trust scoring. Only 1 of 44 builders has delivery_rate facts because MIN_PROJECTS=2 filters out everyone with only 1 linked project. Zero LocatedIn edges exist.
3. **The confidence meter correctly exposes the problem.** Most results show "Moderate" or "Low" confidence because fact coverage and freshness scores are low. The system is transparent about its own limitations -- which is good, but fixing those limitations is the priority.
4. **Remaining 7 days should focus on data density, not new features.** The trust UI is done. The gap is the data backing it. Increasing BuiltBy edges, lowering MIN_PROJECTS, and adding LocatedIn edges will cause all existing trust features to light up without any new code.
5. **On track for sprint goals but need a pivot from feature-building to data-filling.** Days 59-65 were correctly front-loaded with infrastructure. Days 66-72 should be back-loaded with data quality, edge coverage, and pipeline runs.

## Feedback Resolution

1. **Match quality dimension defaults to 0.0 when no preferences active (Day 65)** -- Accept. When a user searches "3bhk whitefield" with no soft preferences, match_quality is 0.0 because graph_driven_pct is undefined (no preferences to score). The confidence formula weights this at 0.2, so the overall score drops by at most 0.2. This is correct behavior: confidence is lower when we cannot demonstrate graph-driven preference matching.

2. **data_freshness payload on every PropertyCard adds size (Day 65)** -- Accept for now. The payload is small (5 fields, ~120 bytes per card). With 20 results per search, this adds ~2.4KB. Not worth optimizing until we have 100+ results or pagination. Defer to Sprint 5.

3. **Most KG nodes show Stale until enrichment runs more frequently (Day 65)** -- Fix today. The freshness calculation uses `node.updated_at`, but many nodes were last updated when the sprint agent ran enrichment weeks ago. Today's plan includes running `seed_from_rera --backfill` on remaining societies and `compute_builder_delivery_rate` to refresh timestamps. Additionally, consider using the most recent `learned_at` from any fact instead of `updated_at` for freshness calculation, since fact timestamps are more accurate.

4. **fact_count/15 uses arbitrary threshold of 15 facts (Day 65)** -- Adjust to 10. Examining the data: RERA-seeded societies with full enrichment have 20-40 facts, but the median across all 70 societies is closer to 12. A threshold of 15 means well-enriched societies still show less than 100% coverage. Lowering to 10 makes the meter more useful: 10+ facts = full coverage bar, under 10 = proportionally lower. This is a one-line change in `compute_confidence`.

5. **Only 1/41 builders has delivery facts due to MIN_PROJECTS=2 (Days 63-64)** -- Fix today. Lower MIN_PROJECTS to 1. With only 22 BuiltBy edges spread across 44 builders, requiring 2 projects is too restrictive. A builder with 1 project can still have a delivery_rate of 1.0 (on-time) or 0.0 (delayed). The fact's confidence is already 0.9 (not 1.0) to reflect lower certainty. Single-project builders getting a delivery fact is better than 43 builders being blank.

6. **Builder deduplication needed but deferred to Sprint 4 (Days 63-64)** -- Confirmed defer. The RERA seeding created duplicate builder nodes (e.g., "prestige-group" vs "prestige-estates-projects-ltd"). Dedup requires fuzzy matching across builder names and merging edges. This is Sprint 4 scope.

7. **29 of 41 builders have no BuiltBy edges (Day 64)** -- Fix today. The `seed_from_rera` skill creates builder nodes but sometimes fails to create BuiltBy edges when the builder slug doesn't exactly match. Today's plan includes a `backfill_built_by_edges` script that reads each society's RERA facts to find the builder_name, fuzzy-matches to existing builder nodes, and creates missing BuiltBy edges.

8. **compute_confidence returns Some even for no-KG-node results with fallback 0.3 (Day 65)** -- Accept. Returning `Some(ConfidenceScore { overall: 0.3, label: "Low", ... })` for properties without KG nodes is better than returning `None` (which hides the signal). The "Low" label with "No knowledge graph data" explanation is transparent and accurate.

## Goal

Consolidate Sprint 3 data quality: increase BuiltBy edge coverage from 22 to 50+, builder delivery_rate coverage from 1 to 15+, and lower the fact_count threshold. No new features -- purely making existing trust features display meaningful data.

## Product Reason

The trust UI is complete but mostly shows "Low confidence" and blank builder badges because the underlying data graph has sparse edges and facts. Fixing data density is the highest-leverage work remaining in Sprint 3. Every BuiltBy edge added lights up the BuilderTrustBadge for all properties in that society. Every delivery_rate fact makes "reliable builder" searches work better.

## Deliverables

### 1. Pipeline: Backfill missing BuiltBy edges

**File:** `pipeline/skills/backfill_built_by_edges.py`

Pure computation skill (no LLM, zero cost). For each society node that has a `builder_name` fact but no BuiltBy edge in `edges.json`, fuzzy-match the builder name to existing builder nodes and create the edge. Log matches, misses, and ambiguous cases.

Algorithm:
- Load all society nodes and extract `builder_name` facts
- Load all builder nodes and build a name-to-slug index
- Load edges.json and find societies already having BuiltBy edges
- For each society without a BuiltBy edge: tokenize builder_name, compare against builder name index using the same fuzzy matching logic from `seed_from_rera.py` (token overlap)
- Write new edges to edges.json (append, deduplicated)
- Report: X new edges created, Y societies still unmatched

### 2. Pipeline: Lower MIN_PROJECTS to 1 in compute_builder_delivery_rate

**File:** `pipeline/skills/compute_builder_delivery_rate.py`

Change `MIN_PROJECTS = 2` to `MIN_PROJECTS = 1`. Then re-run the skill to populate delivery facts for all builders that have at least one linked society with a project_status fact.

### 3. Backend: Lower fact_count threshold from 15 to 10

**File:** `backend/src/search/text.rs`

In `compute_confidence`, change `fact_count as f64 / 15.0` to `fact_count as f64 / 10.0`. Update the explanation string accordingly.

### 4. Backend: Use max fact learned_at for freshness instead of node updated_at

**File:** `backend/src/routes/enrichment.rs`

In the DataFreshness computation, instead of using `node.updated_at`, compute freshness from the most recent `learned_at` timestamp across all facts on the node. This is more accurate because a node's `updated_at` may not reflect when its most recent fact was actually learned.

### 5. Re-run pipeline skills in sequence

No new code. Run existing skills to refresh data:

```bash
# 1. Backfill BuiltBy edges
python3 -m pipeline.skills.backfill_built_by_edges

# 2. Re-run delivery rate with MIN_PROJECTS=1
python3 -m pipeline.skills.compute_builder_delivery_rate

# 3. Re-run classify_project_status to fill remaining gaps
python3 -m pipeline.skills.classify_project_status
```

### 6. Verify: Count improvements

After pipeline runs, verify:
- BuiltBy edges: 22 -> 50+ (target)
- Builders with delivery_rate: 1 -> 15+ (target)
- Societies with project_status: 54 -> 60+ (target)

## Files to Modify

| File | Change |
|------|--------|
| `pipeline/skills/backfill_built_by_edges.py` | **New file**: fuzzy-match builder names to create missing BuiltBy edges |
| `pipeline/skills/compute_builder_delivery_rate.py` | Change MIN_PROJECTS from 2 to 1 |
| `backend/src/search/text.rs` | Change fact_count threshold from 15 to 10 in compute_confidence |
| `backend/src/routes/enrichment.rs` | Use max fact learned_at for freshness calculation |

## Technical Guidance

- The fuzzy matching logic for builder names should reuse the tokenization approach from `pipeline/skills/seed_from_rera.py` (the `NOISE_WORDS` set and token overlap scoring). Do not import from seed_from_rera directly -- copy the matching function to keep the skill self-contained.
- When writing new edges to edges.json, load existing edges, append new ones, and write atomically (write to .tmp, then rename). Deduplicate by (from, to, relation) tuple.
- The freshness change in enrichment.rs should use `facts.iter().filter_map(|f| f.learned_at).max()` and fall back to `node.updated_at` if no facts have timestamps.
- The fact_count threshold of 10 is calibrated to the median fact count across RERA-enriched societies. It should be a named constant, not a magic number.

## Constraints

- No new frontend components or pages
- No new API endpoints
- No LLM calls (all pipeline work is pure computation)
- No changes to scoring logic -- only confidence display parameters
- All pipeline scripts must be idempotent (re-running produces same result)

## Success Criteria

1. `cargo check` and `cargo test` pass
2. `npm run build` succeeds
3. BuiltBy edge count in edges.json increases from 22 to 40+ (verified by grep count)
4. Builder nodes with delivery_rate facts increase from 1 to 10+ (verified by grep count)
5. `python3 -m pipeline.skills.backfill_built_by_edges` runs without errors and reports matches
6. `python3 -m pipeline.skills.compute_builder_delivery_rate` runs and produces facts for multiple builders
7. Confidence meter in search results shows improved scores for RERA-rooted societies
8. DataFreshnessBadge shows fresh timestamps after pipeline re-run
