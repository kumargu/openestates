# Day 28: Unified Enrichment Engine + Dead Code Cleanup

## Dependency

Day 27 complete. We have:
- 55 societies in KG (23 RERA-verified, 48 Reddit-enriched, all with images)
- Working RERA tile, area intelligence tile, sticky sidebar on property page
- `orchestrate.py` doing per-entity enrichment with freshness tracking
- Multiple redundant scripts: `enrich_all.py`, `discover.py`, `integration_rera.py`
- Dead engine stubs: `match_engine.py`, `scoring.py`

---

## 1. The Problem

Enrichment is scattered across 4+ entry points. Adding a new data source means writing a new script. Running the system requires tribal knowledge about which scripts to invoke, in what order, with what flags. This doesn't scale — and it definitely can't be run by agents.

**Today's goal: one command to enrich everything, one command to check status.**

```bash
# This is all an agent needs to know
python3 -m pipeline.enrich                           # fill all gaps
python3 -m pipeline.enrich --plan                    # show what would run
python3 -m pipeline.enrich --node society:sobha-insignia  # one entity
```

---

## 2. What We're Building

### Phase A: Delete Dead Code (15 min)

Remove 4 files that have zero imports and are fully superseded:

| File | Why dead | Replaced by |
|------|----------|-------------|
| `engine/match_engine.py` | Day 1 stub, `NotImplementedError` | `engine/ranker.py` |
| `engine/scoring.py` | Day 1 stub, `NotImplementedError` | `engine/dimensions.py` |
| `pipeline/enrich_all.py` | 515-line batch script | `orchestrate.py` (smarter, fresher) |
| `pipeline/discover.py` | Standalone Gemini discovery | `pipeline/skills/discover_properties.py` |

Verification: `grep -r` confirms zero imports of any of these.

### Phase B: Add `output_keys` to Every Skill (30 min)

Each skill declares what facts it produces. This enables cheap gap detection without LLM calls.

```python
class FetchReraSkill(BaseSkill):
    skill_id = "fetch_rera"
    output_keys = ["rera_registered", "rera_number", "rera_status", "rera_completion_date", ...]

class SearchRedditSkill(BaseSkill):
    skill_id = "search_reddit"
    output_keys = ["reddit_thread_count", "reddit_total_score", "reddit_threads", ...]
```

Add `output_keys: list[str] = []` to `BaseSkill` in `pipeline/skills/base.py`. Then populate for each skill:

| Skill | output_keys count |
|-------|------------------|
| `search_reddit` | 4 keys |
| `learn_society` | 12 keys |
| `fetch_rera` | 18 keys |
| `fetch_images` | 2 keys |
| `fetch_google_reviews` | 6 keys |
| `learn_area` | 10 keys |
| `score_society` | 7 keys |
| `embed_entity` | 2 keys |

### Phase C: Build the Enrichment Engine (2 hours)

**File: `pipeline/enrich.py`** (~200 lines)

Replaces `orchestrate.py` with a registry-driven engine.

#### C.1 Skill Registry

```python
SKILL_REGISTRY = {
    "search_reddit":      {"node_types": ["society"], "pool": "reddit",  "max_age_days": 14, "cost_tier": "free",     "priority": 1, "depends_on": []},
    "fetch_rera":          {"node_types": ["society"], "pool": "rera",    "max_age_days": 30, "cost_tier": "free",     "priority": 1, "depends_on": []},
    "fetch_images":        {"node_types": ["society"], "pool": "image",   "max_age_days": 60, "cost_tier": "free",     "priority": 2, "depends_on": []},
    "fetch_google_reviews":{"node_types": ["society"], "pool": "google",  "max_age_days": 30, "cost_tier": "cheap",    "priority": 2, "depends_on": []},
    "learn_society":       {"node_types": ["society"], "pool": "llm",     "max_age_days": 30, "cost_tier": "cheap",    "priority": 2, "depends_on": ["search_reddit"]},
    "learn_area":          {"node_types": ["area"],    "pool": "llm",     "max_age_days": 30, "cost_tier": "cheap",    "priority": 2, "depends_on": []},
    "score_society":       {"node_types": ["society"], "pool": "llm",     "max_age_days": 7,  "cost_tier": "moderate", "priority": 3, "depends_on": ["learn_society", "fetch_rera"]},
    "embed_entity":        {"node_types": ["society", "area", "property"], "pool": "embedding", "max_age_days": 30, "cost_tier": "cheap", "priority": 3, "depends_on": []},
}
```

#### C.2 Cheap Gap Detection

Scan every node's facts. For each skill in the registry:
- Does this skill apply to this node type?
- Are any of the skill's `output_keys` present in the node's facts?
- If present, are they older than `max_age_days`?

Result: a work queue of `(node_id, skill_id, reason)` tuples, sorted by priority.

**Zero LLM calls for gap detection.** Just key-presence + age checks.

#### C.3 Execution with Dependency Ordering

```python
def execute(work_queue, budget=None):
    """Run skills in dependency + priority order."""
    completed = set()  # (node_id, skill_id) pairs

    for item in sorted(work_queue, key=lambda w: REGISTRY[w.skill_id]["priority"]):
        # Check dependencies
        for dep in REGISTRY[item.skill_id]["depends_on"]:
            if (item.node_id, dep) in work_queue and (item.node_id, dep) not in completed:
                # Dependency not yet run — it will be handled in priority order
                continue

        # Check budget
        if budget is not None and cumulative_cost > budget:
            logger.info("Budget cap reached ($%.2f). Stopping.", budget)
            break

        # Run skill
        run_skill(item)
        completed.add((item.node_id, item.skill_id))
```

#### C.4 Failure Tracking

Simple JSON file at `data/cache/enrich_failures.json`:

```json
{
    "rera": {"consecutive_failures": 3, "last_failure": "2026-03-10T14:00:00Z", "backoff_until": "2026-03-11T14:00:00Z"},
    "reddit": {"consecutive_failures": 0}
}
```

If a source pool has 3+ consecutive failures, skip it until `backoff_until`. Reset on success.

#### C.5 CLI

```bash
# Default: scan all nodes, fill gaps, refresh stale
python3 -m pipeline.enrich

# Plan mode: show what would run, no execution
python3 -m pipeline.enrich --plan

# Single node
python3 -m pipeline.enrich --node society:sobha-insignia

# Budget cap
python3 -m pipeline.enrich --budget 1.00

# Only free skills (safe for frequent runs)
python3 -m pipeline.enrich --cost-tier free

# Force everything (ignore cache, freshness)
python3 -m pipeline.enrich --force

# Specific node type
python3 -m pipeline.enrich --type society

# Notify backend after
python3 -m pipeline.enrich --reload
```

#### C.6 Structured Output

```
$ python3 -m pipeline.enrich --plan

ENRICHMENT PLAN
===============
Nodes scanned: 67 (55 society, 5 area, 7 property)
Work items: 23

  Priority 1 (free):
    society:brigade-woods         fetch_rera        missing
    society:brigade-woods         search_reddit     missing
    society:godrej-splendour      search_reddit     missing
    society:assetz-marq-3         fetch_rera        missing
    ... (5 more)

  Priority 2 (cheap):
    society:brigade-woods         learn_society     missing (depends: search_reddit)
    society:prestige-lakeside     fetch_google_reviews  stale (42 days)
    ... (8 more)

  Priority 3 (moderate):
    society:sobha-insignia        score_society     stale (15 days)
    ... (3 more)

Estimated cost: $0.14
Estimated time: ~4 min
```

After execution:

```
ENRICHMENT COMPLETE
===================
  Executed: 23 skills
  Succeeded: 21
  Cached (skipped): 5
  Failed: 2 (rera pool — portal timeout, backed off)
  Facts written: 187
  Cost: $0.12
  Time: 3m 42s

  Backend reloaded: 229 nodes, 2868 facts
```

### Phase D: Migrate orchestrate.py References (15 min)

- Update CLAUDE.md section 17 to reference `pipeline/enrich.py` instead of `orchestrate.py`
- Keep `orchestrate.py` for one more day as a thin wrapper that imports from `enrich.py` (backward compat)
- Update any pipeline agent references

### Phase E: Verify (30 min)

1. Run `python3 -m pipeline.enrich --plan` — should show the 7 missing Reddit societies + any stale facts
2. Run `python3 -m pipeline.enrich --cost-tier free` — should fill Reddit + RERA gaps (free skills only)
3. Run `python3 -m pipeline.enrich --node society:brigade-woods` — should enrich one entity end-to-end
4. Verify `data/knowledge/nodes/society/brigade-woods.json` has new facts
5. Run `python3 -m pipeline.enrich --reload` — should trigger backend hot-reload
6. Hit `http://localhost:4000/api/properties/` and verify fresh data
7. Type checks: `cargo check`, `npx tsc --noEmit`

---

## 3. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Delete 4 dead files | 15 min |
| **B** | Add `output_keys` to all skills | 30 min |
| **C** | Build enrichment engine (`pipeline/enrich.py`) | 2 hours |
| **D** | Update references, backward compat wrapper | 15 min |
| **E** | Verify end-to-end | 30 min |
| **Total** | | ~3.5 hours |

---

## 4. Files

### Delete
- `engine/match_engine.py`
- `engine/scoring.py`
- `pipeline/enrich_all.py`
- `pipeline/discover.py`

### New
- `pipeline/enrich.py` — unified enrichment engine (~200 lines)

### Modified
- `pipeline/skills/base.py` — add `output_keys` to BaseSkill
- `pipeline/skills/search_reddit.py` — add output_keys
- `pipeline/skills/learn_society.py` — add output_keys
- `pipeline/skills/fetch_rera.py` — add output_keys
- `pipeline/skills/fetch_images.py` — add output_keys
- `pipeline/skills/fetch_google_reviews.py` — add output_keys
- `pipeline/skills/learn_area.py` — add output_keys
- `pipeline/skills/score_society.py` — add output_keys
- `pipeline/skills/embed_entity.py` — add output_keys
- `CLAUDE.md` — update enrichment references

---

## 5. Design Principles (Day 28 Specific)

### One entry point, not many scripts
Adding a new data source should require:
1. Write a skill file in `pipeline/skills/`
2. Add one line to `SKILL_REGISTRY` in `enrich.py`
3. Done. No new script, no new cron, no new orchestration code.

### Cheap before expensive
Gap detection is free (key-presence check). Free skills run before paid ones. Budget caps prevent runaway costs. An agent can safely run `--cost-tier free` hourly with zero cost risk.

### Plan before execute
`--plan` mode shows exactly what would run, estimated cost, estimated time. An agent can inspect the plan, decide whether to proceed, and run with `--budget` if cautious.

### Fail gracefully, not loudly
Source pool down? Back off exponentially. One skill fails? Log it, continue with the rest. Budget exceeded? Stop cleanly, report what was done. Never crash. Never leave partial state.

### The system runs itself
After today, enrichment is: one cron job (`python3 -m pipeline.enrich --reload`), running daily. It scans the entire graph, fills gaps, refreshes stale data, reloads the backend. An agent (or a cron daemon) can run this with zero context about what's missing or what's changed.

---

## 6. What NOT to Build Today

- Async worker pools (concurrent execution) — sequential is fine for 55 entities. Parallelize when we hit 500.
- Web dashboard for enrichment status — CLI output is enough.
- Database for failure tracking — JSON file is fine.
- Webhook-triggered enrichment (new node → immediate enrich) — cron covers this for now.
- Cost optimization (model routing, prompt caching) — premature. Track costs first, optimize later.

---

## 7. Success Criteria

- [ ] 4 dead files deleted, zero import errors
- [ ] All 8 skills have `output_keys` populated
- [ ] `python3 -m pipeline.enrich --plan` shows accurate gap analysis
- [ ] `python3 -m pipeline.enrich --cost-tier free` fills gaps without LLM costs
- [ ] `python3 -m pipeline.enrich --node society:brigade-woods` enriches one entity
- [ ] `python3 -m pipeline.enrich --budget 0.50` respects budget cap
- [ ] Failure tracking works (backs off after 3 failures)
- [ ] `--reload` triggers backend hot-reload
- [ ] `orchestrate.py` still works (backward compat wrapper)
- [ ] Structured output: plan shows items, execution shows summary with cost
- [ ] An agent can run the full enrichment loop with zero human guidance

---

## 8. After Day 28

```
The Operational Model:

  Daily cron (or agent):
    python3 -m pipeline.enrich --reload --budget 2.00

  That's it. One command. It:
    1. Scans 67+ nodes for missing/stale facts
    2. Plans work in dependency + priority order
    3. Runs free skills first, then cheap, then moderate
    4. Stops if budget exceeded
    5. Backs off from failing sources
    6. Writes facts to disk
    7. Reloads backend
    8. Prints structured summary

  Adding a new data source:
    1. Write pipeline/skills/fetch_xyz.py (extends BaseSkill)
    2. Add to SKILL_REGISTRY in enrich.py
    3. Next enrichment run automatically picks it up

  No new scripts. No new crons. No manual steps.
```
