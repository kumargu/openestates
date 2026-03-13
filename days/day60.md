# Day 60: seed_from_rera — RERA Seeding Skill

## Goal

Build the `seed_from_rera` skill that fuzzy-matches existing societies against the cached RERA listing (9,469 projects), backfills RERA facts for the 32 societies currently missing them, and seeds new RERA-verified society nodes from the registry.

## Product Reason

32 of 55 existing societies have no RERA date facts because `fetch_rera` searched by exact project name and missed them (e.g., "Sobha Neopolis" registered as "Sobha Neopolis Phase 4 - 3, 4, 8 & 9"). Without RERA dates, these societies cannot get `project_status` classification, and searches like "ready to move in Whitefield" miss them. Additionally, the graph has only 55 hand-curated societies while RERA has 9,469 projects. This skill unlocks scaling to 50-100+ RERA-verified societies.

## Sprint Context

Day 2 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Addressed

1. **32 societies lack RERA dates** → backfill mode fixes this via fuzzy matching
2. **No Seller/Legacy root_source nodes** → expected, no action needed
3. **Verifier: fill RERA dates** → exactly what this day does
4. **Sprint vision: seed_from_rera** → core deliverable

## Deliverables

### 1. Fuzzy name matcher for RERA listing

**File:** `pipeline/skills/seed_from_rera.py`

Import from existing `fetch_rera.py`: `scrape_rera_listing()`, `fetch_rera_detail()`, `parse_rera_detail()`, `rera_detail_to_facts()`.

Fuzzy matching via Jaccard token similarity:
- Tokenize names, remove noise words (phase, tower, pvt, ltd, etc.)
- Score = |intersection| / |union|
- Accept matches above 0.5 threshold
- For multi-phase projects, pick latest completion date

### 2. Backfill mode: enrich 32 existing societies

CLI: `python3 -m pipeline.skills.seed_from_rera --backfill`

For each society with `rera_registered: False` or missing RERA date facts:
1. Fuzzy-match against RERA listing
2. Fetch detail page (cached 30 days)
3. Convert to SourcedFacts via `rera_detail_to_facts()`
4. Write facts to society JSON (atomic writes)
5. Update `root_source` to `"Rera"`

### 3. Seed mode: discover new societies from RERA

CLI: `python3 -m pipeline.skills.seed_from_rera --seed --area "Whitefield" --limit 10`

For RERA projects in target area not already in KG:
1. Create new society node at `data/knowledge/nodes/society/{slug}.json`
2. Create/update builder node
3. Add RERA facts, set `root_source: "Rera"`
4. Create edges in `data/knowledge/edges.json`

### 4. Re-run classify_project_status

After backfill, re-run `classify_project_status` to expand coverage from 23 → 40+ societies.

### 5. Builder delivery track record (stretch)

For builders with 2+ RERA projects, compute `builder_delivery_rate` fact.

## Technical Guidance

- Import from `fetch_rera.py` — do NOT duplicate scraping logic
- Follow `classify_project_status.py` pattern for batch file processing
- Atomic writes: `.json.tmp` + `os.rename()`
- Rate limit: 1 second between RERA detail fetches (already in fetch_rera)
- Check for existing nodes before creating (idempotent)
- Log all match decisions for auditability

## Constraints

- No LLMs — pure data matching and scraping
- Do not modify `fetch_rera.py`
- Cache all RERA responses
- Limit seed mode to configurable number per run

## Success Criteria

1. `--backfill` matches and enriches ≥20 of 32 societies missing RERA dates
2. Each enriched society gets 15+ RERA SourcedFacts
3. `root_source` updated to `"Rera"` for matched societies
4. After re-running `classify_project_status`, ≥40 of 55 societies have `project_status`
5. `--seed --area X --limit 10` creates 10 new RERA-verified society nodes
6. No duplicate nodes created (idempotent re-runs)
7. Match log printed: society name → RERA project → score → action
