# Day 30: Progressive Async Enrichment — Search-Triggered Background Learning

## Dependency

Day 29 complete. We have:
- Structured match explanations showing graph vs legacy scoring per result
- 55 societies in KG with self-describing facts
- `enrich.py` doing batch gap-fill across all entities (cron-style)
- Live discovery via Gemini Flash when search results are poor
- Enrichment queue in Rust (`graph.enrichment_queue`) that nobody reads
- **Gap: discovered entities sit with bare-bones data until the next `enrich.py` cron run**
- **Gap: no area-warming — searching "Whitefield" doesn't pre-enrich other Whitefield societies**

---

## 1. The Problem

Today the system has two data-filling paths that don't talk to each other:

```
Path 1: Live Discovery (Rust, real-time)
  User search → poor results → Gemini Flash → bare-bones entity
  → queues enrichment tasks in memory → nobody picks them up

Path 2: Batch Enrichment (Python, cron)
  enrich.py → scans ALL entities → fills ALL gaps → slow, uniform
  → no awareness of user interest → treats Whitefield = Yelahanka
```

This means:
- A discovered entity stays thin until the next cron run (could be hours)
- Searching "quiet apartment whitefield" doesn't warm up other Whitefield societies
- The user clicks a result → property page has no RERA, no Reddit, no scores
- Rate-limited scraping happens in bulk bursts (ban risk) instead of slow trickle

**Today's goal: search interest drives background enrichment in progressive waves, throttled to avoid bans.**

---

## 2. What We're Building

### Phase A: Rust — Write Enrichment Requests After Search (45 min)

After every search that identifies an area, Rust appends a request to `data/knowledge/enrichment_pending.jsonl`:

```json
{"area": "Whitefield", "entities": ["society:prestige-lakeside-habitat", "society:sobha-insignia"], "preferences": ["quiet", "metro access"], "query": "quiet apartment near metro whitefield", "ts": "2026-03-11T10:00:00Z"}
```

**Rules:**
- Only write when the search has an identified area (no gibberish queries)
- Deduplicate: if the same area was requested in the last 10 minutes, skip
- Include the matched entity IDs (Wave 1 targets) AND the user preferences (so enrichment can prioritize relevant skills)
- Fire-and-forget: `tokio::spawn` a blocking write, don't block the search response
- Also write when live discovery creates new entities (these need enrichment urgently)

**File:** append-only JSONL. The daemon reads and truncates after processing.

#### A.1 New module: `backend/src/enrichment_queue.rs`

```rust
/// Append an enrichment request to the pending JSONL file.
/// Called via tokio::spawn — non-blocking to the search handler.
pub fn append_enrichment_request(
    project_root: &Path,
    area: &str,
    entity_ids: Vec<String>,
    preferences: Vec<String>,
    query: &str,
)
```

Simple file append with a file lock (or atomic write). ~40 lines.

#### A.2 Wire into search.rs

After the search response is built (but before returning), spawn the write:

```rust
// After building results, before returning Json(...)
if let Some(ref area) = parsed_intent.area {
    let entity_ids: Vec<String> = results.iter()
        .filter_map(|r| /* extract society node IDs */)
        .collect();
    let root = state.project_root.clone();
    let area = area.clone();
    let prefs = parsed_intent.preferences.clone();
    let q = query.clone();
    tokio::spawn(async move {
        enrichment_queue::append_enrichment_request(&root, &area, entity_ids, prefs, &q);
    });
}
```

### Phase B: Python — Progressive Enrichment Daemon (2 hours)

**File: `pipeline/enrich_async.py`** (~200 lines)

A long-running daemon that watches `enrichment_pending.jsonl` and processes requests in progressive waves with throttling.

#### B.1 The Wave Model

```
Request: { area: "Whitefield", entities: [A, B, C], preferences: ["quiet", "metro"] }

Wave 1 — Exact matches (immediate, free skills)
  For each entity in request.entities:
    → search_reddit (if missing)     [free, 5s delay between calls]
    → fetch_rera (if missing)        [free, 5s delay]
    → fetch_google_reviews           [cheap, 5s delay]

  WHY: User is looking at these results RIGHT NOW.
  Property page needs Reddit data, RERA badge, Google rating.

Wave 2 — Exact matches (LLM enrichment)
  For each entity in request.entities:
    → learn_society (if reddit done)  [cheap, 10s delay]
    → score_society                   [moderate, 10s delay]
    → embed_entity                    [cheap, 5s delay]

  WHY: User may click a result. LLM scores and explanations
  make the property page and match explanations richer.

Wave 3 — Area neighbors (free skills only)
  Find all societies in same area NOT in request.entities:
    → search_reddit                  [free, 15s delay]
    → fetch_rera                     [free, 15s delay]

  WHY: User may refine search or browse more results.
  Next search for this area will have more data.

Wave 4 — Area neighbors (LLM enrichment, budget-capped)
  For the top-N neighbors (by existing fact count, lowest first):
    → learn_society                  [cheap, 20s delay]
    → score_society                  [moderate, 20s delay]
  Cap: max 5 entities per wave, max $0.50 per request.

  WHY: Diminishing returns. Only enrich what's most likely useful.
```

#### B.2 Throttling & Rate Limits

```python
WAVE_CONFIG = {
    1: {"delay_between_calls": 5,  "max_entities": None, "cost_tiers": ["free"]},
    2: {"delay_between_calls": 10, "max_entities": None, "cost_tiers": ["free", "cheap", "moderate"]},
    3: {"delay_between_calls": 15, "max_entities": 10,   "cost_tiers": ["free"]},
    4: {"delay_between_calls": 20, "max_entities": 5,    "cost_tiers": ["free", "cheap", "moderate"]},
}

# Global rate limits (shared with enrich.py via failure tracking)
GLOBAL_LIMITS = {
    "reddit": {"calls_per_hour": 30, "min_delay_seconds": 5},
    "rera":   {"calls_per_hour": 20, "min_delay_seconds": 10},
    "google": {"calls_per_hour": 30, "min_delay_seconds": 5},
    "llm":    {"calls_per_hour": 60, "min_delay_seconds": 3},
}
```

#### B.3 Finding Area Neighbors

```python
def find_area_neighbors(area: str, exclude: list[str]) -> list[str]:
    """Find all society entities in the same area, excluding already-targeted ones."""
    kg_dir = Path("data/knowledge/nodes/society")
    neighbors = []
    for f in kg_dir.glob("*.json"):
        node = json.loads(f.read_text())
        # Check if any fact indicates this society is in the target area
        node_area = None
        for fact in node.get("facts", []):
            if fact["key"] == "area":
                node_area = fact["value"].get("data", "")
                break
        if not node_area:
            # Fallback: check edges or name patterns
            continue
        if node_area.lower() == area.lower() and node["id"] not in exclude:
            neighbors.append(node["id"])
    return neighbors
```

Actually simpler: scan `data/seed/properties.json` for properties in the same area → extract their society_ids → map to KG node IDs. The seed data already has area fields.

#### B.4 Request Processing Loop

```python
async def run_daemon():
    """Watch for enrichment requests, process in waves."""
    pending_path = Path("data/knowledge/enrichment_pending.jsonl")

    while True:
        # Read and consume pending requests
        requests = read_and_truncate(pending_path)

        if not requests:
            await asyncio.sleep(10)  # Poll every 10 seconds
            continue

        # Deduplicate by area (keep most recent per area)
        by_area = deduplicate_requests(requests)

        for area, request in by_area.items():
            logger.info(f"Processing enrichment for {area}: {len(request['entities'])} entities")

            # Wave 1: exact matches, free skills
            await run_wave(1, request["entities"], request["preferences"])

            # Wave 2: exact matches, LLM skills
            await run_wave(2, request["entities"], request["preferences"])

            # Wave 3: area neighbors, free skills
            neighbors = find_area_neighbors(area, request["entities"])
            if neighbors:
                await run_wave(3, neighbors, request["preferences"])

            # Wave 4: area neighbors, LLM skills (budget-capped)
            if neighbors:
                await run_wave(4, neighbors[:5], request["preferences"])

            # Notify backend to reload
            reload_backend()
```

#### B.5 Skill Execution (reuses enrich.py infrastructure)

```python
async def run_wave(wave_num: int, entities: list[str], preferences: list[str]):
    config = WAVE_CONFIG[wave_num]

    for entity_id in entities[:config["max_entities"]]:
        for skill_id, skill_config in SKILL_REGISTRY.items():
            # Skip if wrong cost tier for this wave
            if skill_config["cost_tier"] not in config["cost_tiers"]:
                continue

            # Skip if entity type doesn't match skill
            entity_type = entity_id.split(":")[0]
            if entity_type not in skill_config["node_types"]:
                continue

            # Skip if already fresh
            if not needs_enrichment(entity_id, skill_id):
                continue

            # Check rate limits
            if is_rate_limited(skill_config["pool"]):
                continue

            # Run skill
            try:
                run_skill(entity_id, skill_id)
                record_success(skill_config["pool"])
            except Exception as e:
                logger.warning(f"Skill {skill_id} failed for {entity_id}: {e}")
                record_failure(skill_config["pool"])

            # Throttle
            await asyncio.sleep(config["delay_between_calls"])
```

#### B.6 CLI

```bash
# Run as daemon (watches for requests, processes in waves)
python3 -m pipeline.enrich_async

# Run once and exit (process current pending, don't loop)
python3 -m pipeline.enrich_async --once

# Dry run (show what would be processed)
python3 -m pipeline.enrich_async --dry-run

# With custom delays (for testing)
python3 -m pipeline.enrich_async --delay-multiplier 0.1
```

### Phase C: Backend Reload After Enrichment (30 min)

Both `enrich.py` and `enrich_async.py` should call the admin reload endpoint after writing facts:

```python
def reload_backend():
    """POST to /api/admin/reload-knowledge to hot-reload the graph."""
    token = os.environ.get("ADMIN_TOKEN", "dev-token")
    try:
        req = urllib.request.Request(
            "http://localhost:4000/api/admin/reload-knowledge",
            method="POST",
            headers={"X-Admin-Token": token},
        )
        urllib.request.urlopen(req, timeout=5)
        logger.info("Backend reloaded")
    except Exception as e:
        logger.warning(f"Backend reload failed: {e}")
```

This already exists in `enrich.py` (the `--reload` flag). Extract it as a shared utility in `pipeline/utils.py` so both can use it.

`enrich_async.py` reloads after each area's waves complete (not after every skill — that would be too frequent).

### Phase D: Verify End-to-End (30 min)

1. Start the Rust backend: `cargo run`
2. Start the async daemon: `python3 -m pipeline.enrich_async`
3. Search: `curl "http://localhost:4000/api/search?q=quiet+apartment+whitefield"`
4. Check `data/knowledge/enrichment_pending.jsonl` — should have a request
5. Watch daemon logs — should process Wave 1 (free skills), then Wave 2 (LLM), etc.
6. After Wave 1 completes, search again — results should have richer data
7. Check that area neighbors are being enriched (Wave 3-4)
8. Verify rate limiting: send 5 rapid searches — daemon should deduplicate
9. `cargo check` + `npm run build` pass

---

## 3. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Rust: write enrichment requests after search | 45 min |
| **B** | Python: progressive enrichment daemon | 2 hours |
| **C** | Shared backend reload utility | 30 min |
| **D** | Verify end-to-end | 30 min |
| **Total** | | ~4 hours |

---

## 4. Files

### New
- `backend/src/enrichment_queue.rs` — append enrichment requests to JSONL (~40 lines)
- `pipeline/enrich_async.py` — progressive enrichment daemon (~200 lines)
- `pipeline/utils.py` — shared utilities (reload_backend, rate limit helpers)

### Modified
- `backend/src/main.rs` — register enrichment_queue module
- `backend/src/routes/search.rs` — spawn enrichment request write after search
- `pipeline/enrich.py` — extract reload_backend to shared utility, import from utils.py

---

## 5. Design Principles (Day 30 Specific)

### Interest-driven, not uniform
Today's enrichment is uniform: `enrich.py` scans everything, fills everything. After today, user searches create a heat map of interest. Whitefield gets searched → Whitefield gets enriched first. Areas nobody searches stay cold. This is how the system should allocate its scraping budget.

### Slow is a feature
The 5-20 second delays between calls aren't a bug. They're rate limit protection. Reddit, RERA portal, and Google all rate-limit aggressive scrapers. A slow trickle of requests looks like a human. A burst of 50 requests in 10 seconds looks like a bot.

### Waves, not waterfalls
Each wave is independently useful. Wave 1 alone (free skills) gives the user Reddit threads, RERA status, and Google ratings within minutes. They don't need to wait for Wave 4 (LLM scoring of neighbors). Each wave adds value, and later waves can be dropped if the budget runs out.

### File-based queue keeps boundaries clean
Rust writes JSONL. Python reads JSONL. No shared memory, no IPC, no gRPC, no message broker. The file is the contract. If the daemon is down, requests accumulate. When it starts, it processes them. If Rust crashes, the daemon keeps running. This is the simplest thing that works.

### Two enrichment paths, one skill infrastructure
`enrich.py` (batch) and `enrich_async.py` (progressive) both call the same skills, use the same freshness tracker, share the same failure backoff. The difference is scheduling strategy: batch fills all gaps uniformly; async follows user interest in prioritized waves.

---

## 6. What NOT to Build Today

- WebSocket push to frontend ("your results are getting richer") — future, requires session tracking
- Priority queue with persistence (Redis, SQLite) — JSONL file is fine at this scale
- Concurrent skill execution within a wave — sequential with delays is intentional (rate limit protection)
- Preference-weighted skill ordering (enrich "quiet" skills before "metro" skills) — all skills run for each entity anyway, the preference data just guides which entities to target
- Auto-scaling wave delays based on error rates — fixed delays are fine, backoff handles failures

---

## 7. Success Criteria

- [ ] Search with an area appends to `enrichment_pending.jsonl`
- [ ] Duplicate area searches within 10 minutes don't create duplicate requests
- [ ] Daemon picks up requests and processes Wave 1 (free skills) with 5s delays
- [ ] Daemon processes Wave 2 (LLM skills) with 10s delays
- [ ] Daemon finds and enriches area neighbors (Wave 3-4)
- [ ] Daemon respects failure backoff (shared with enrich.py)
- [ ] Daemon reloads backend after completing each area's waves
- [ ] `--once` mode processes pending and exits
- [ ] `--dry-run` shows what would be processed without executing
- [ ] No interference with `enrich.py` (both can run, shared freshness tracker prevents duplicates)
- [ ] `cargo check` + `npm run build` pass

---

## 8. The Complete Data Flow After Day 30

```
User searches "quiet apartment near metro whitefield"
  │
  ├── IMMEDIATE (in request, <500ms)
  │   Text search + graph scoring + semantic boost
  │   Returns results with match_explanation
  │   (graph facts → structured reasons, legacy → "Seed" reasons, gaps → "no_data")
  │
  ├── IMMEDIATE (if max_score < 0.15)
  │   Live Discovery → Gemini Flash → new entities
  │   Bare-bones data, "Verification Pending" tags
  │
  ├── FIRE-AND-FORGET (after response sent)
  │   Rust appends to enrichment_pending.jsonl:
  │   {area: "Whitefield", entities: [...], preferences: ["quiet", "metro"]}
  │
  ├── WAVE 1 (~30s later, free skills, 5s between calls)
  │   search_reddit for each matched society
  │   fetch_rera for each matched society
  │   fetch_google_reviews
  │   → Backend reloaded
  │   → User refreshes or clicks result → sees Reddit data, RERA badge, ratings
  │
  ├── WAVE 2 (~3min later, LLM skills, 10s between calls)
  │   learn_society → structured facts from Reddit threads
  │   score_society → 6-dimension scores with explanations
  │   embed_entity → vector embeddings
  │   → Backend reloaded
  │   → User searches again → sees graph-driven match_explanation, higher graph_driven_pct
  │
  ├── WAVE 3 (~8min later, area neighbors, free skills, 15s between calls)
  │   Other Whitefield societies: search_reddit, fetch_rera
  │   → Next search for Whitefield has more options with data
  │
  └── WAVE 4 (~15min later, area neighbors, LLM skills, budget-capped)
      Top 5 least-enriched Whitefield societies: learn_society, score_society
      → Whitefield is now a deeply enriched area
      → Future searches are instant and rich

Meanwhile, enrich.py cron runs nightly to fill any remaining gaps across ALL areas.
```

---

## 9. Day 31+ Preview

With progressive enrichment in place:

- **Day 31: Interest Heatmap** — aggregate enrichment_pending.jsonl to see which areas users search most → prioritize batch enrichment budget
- **Day 32: Compare Workspace V2** — side-by-side match explanation comparison using the richer data from progressive enrichment
- **Day 33: Search Quality Dashboard** — track graph_driven_pct over time, see enrichment impact on search quality
- **Day 34: Frontend "Getting Smarter" indicator** — show users that background enrichment is happening ("We're learning more about this area...")
