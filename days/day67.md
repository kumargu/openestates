# Day 67: SocietyInArea Edge Backfill and Area-Aware Search

## Mid-Sprint Status (Sprint 3, Day 10 of 14)

**Coverage snapshot after day 66:**
- 70 BuiltBy edges (great)
- 32 builders with delivery_rate (great)
- 22 SocietyInArea edges (48 societies still missing -- biggest remaining gap)
- 59/70 societies with project_status

**Feedback resolution from day 66:**

1. **Sumadhura has 3 builder node variants** -- Confirmed Sprint 4 dedup scope. No action today.
2. **MIN_PROJECTS=1 tradeoff** -- Accepted. Confidence 0.9 mitigates single-project stats. No change needed.
3. **Freshness uses fact learned_at** -- Accepted. Already implemented in day 66. No change needed.
4. **11 societies without RERA dates** -- Accepted. These are non-RERA/discovered societies. Cannot manufacture possession dates. No action.
5. **5 builders with no linked societies** -- Sprint 4 dedup. These are duplicate RERA-created builder nodes.
6. **7 builders with linked societies but no project_status** -- Their societies lack RERA dates. Cannot compute delivery_rate without dates. Accepted as a data boundary.

## Goal

Backfill SocietyInArea edges from 22 to 65+ and wire these edges into backend search so area-based queries use graph traversal instead of (or in addition to) text matching. This is the single highest-leverage data gap remaining in Sprint 3.

## Product Reason

SocietyInArea edges are the link between "what the user searches for" (an area) and "what the system knows" (society facts). Without them, area-based search relies on string matching against property records. With them, the system can:
- Use the knowledge graph to find all societies in an area, including societies that spell the area differently
- Surface area-level context (facts from the area node) alongside results
- Enable future features: "X societies in this area", area comparison, area-level trust signals

Every SocietyInArea edge created today lights up area context for all properties in that society -- zero new UI code needed.

## Deliverables

### 1. Pipeline: `backfill_located_in_edges.py` -- Create SocietyInArea edges

**File:** `pipeline/skills/backfill_located_in_edges.py`

Pure computation skill (no LLM, zero cost). Follows the exact pattern of `backfill_built_by_edges.py`.

**Algorithm:**

1. Load all society nodes from `data/knowledge/nodes/society/*.json`
2. Load all area nodes from `data/knowledge/nodes/area/*.json` and build an index: `{area_slug: (node_id, name)}`
3. Load `data/knowledge/edges.json` and find societies that already have SocietyInArea edges
4. For each society WITHOUT a SocietyInArea edge:
   a. Extract the `area` fact value (e.g., "Sarjapur Road")
   b. Slugify it: `re.sub(r'[^a-z0-9]+', '-', area.lower()).strip('-')` -- produces e.g., `sarjapur-road`
   c. Direct match: check if `area:{slug}` exists in the area node index
   d. Fuzzy match: if direct match fails, tokenize both sides and use Jaccard similarity (threshold 0.5) against all area slugs
   e. If matched, append a new SocietyInArea edge to edges list
5. Save edges atomically (write to `.tmp`, rename)
6. Print report: new edges, already had, unmatched

**Edge format** (matches existing convention):
```python
{
    "from": "society:prestige-lakeside-habitat",
    "to": "area:whitefield",
    "relation": "SocietyInArea",
    "weight": 1.0,
    "source": {
        "source_type": "Computed",
        "skill_id": "backfill_located_in_edges",
    },
}
```

**Expected outcome:** ~48 new edges, total ~70 SocietyInArea edges.

### 2. Backend: Use SocietyInArea edges in area-based search filtering

**File:** `backend/src/search/text.rs`

Add graph-based area check as a third matching strategy in the area_penalty block:

Current logic:
1. Exact text match -> penalty 0.0
2. `area_is_nearby` -> penalty -2.0
3. Otherwise -> `return None` (exclude)

New logic:
1. Exact text match -> penalty 0.0
2. `area_is_nearby` -> penalty -2.0
3. Graph SocietyInArea edge match -> penalty -1.0 (between exact and nearby)
4. Otherwise -> `return None` (exclude)

### 3. Backend: Surface area node facts in knowledge_context

**File:** `backend/src/routes/search.rs`

Extend `knowledge_context` to include claims from the matched area node when a SocietyInArea edge exists. Find the area node using `graph.get_node(&format!("area:{}", slugify(area_name)))` and extract its facts with display_templates.

### 4. Verify and run

```bash
# 1. Create SocietyInArea edges
python3 -m pipeline.skills.backfill_located_in_edges

# 2. Rebuild and test backend
cd backend && cargo check && cargo test

# 3. Verify edge counts
python3 -c "
import json
edges = json.loads(open('data/knowledge/edges.json').read())
sia = [e for e in edges if e['relation'] == 'SocietyInArea']
print(f'SocietyInArea: {len(sia)}')
"
```

## Files to Modify

| File | Change |
|------|--------|
| `pipeline/skills/backfill_located_in_edges.py` | **New file**: create SocietyInArea edges from society area facts |
| `backend/src/search/text.rs` | Add graph-based area matching using SocietyInArea edges |
| `backend/src/routes/search.rs` | Include area node facts in knowledge_context claims |

## Technical Guidance

- Follow `backfill_built_by_edges.py` pattern exactly
- Slug normalization: `re.sub(r'[^a-z0-9]+', '-', name.lower()).strip('-')` plus collapse multiple dashes
- The `Relation::SocietyInArea` enum variant already exists in `backend/src/knowledge/edge.rs`
- `graph.neighbors()` already supports relation filtering
- Guard graph checks with `if let Some(g) = graph`

## Constraints

- No new frontend components or pages
- No LLM calls (pure computation + Rust code changes)
- No new API endpoints
- Pipeline skill must be idempotent
- Backend changes must compile and pass existing tests

## Success Criteria

1. `python3 -m pipeline.skills.backfill_located_in_edges` runs without errors
2. SocietyInArea edge count increases from 22 to 65+ in edges.json
3. At most 2-3 societies remain unmatched
4. `cargo check` passes
5. `cargo test` passes
6. `npm run build` succeeds
7. Knowledge context in search response includes area-level claims when available
