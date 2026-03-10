# Day 21: Live Discovery — The System That Learns by Being Searched

## 0. Pre-flight: Review Day 20 Output + Tech Debt Cleanup

Before writing any new code:

### 0.1 Verify Day 20

1. **Check what Day 20 actually built** — compile the Rust backend, verify knowledge graph types exist and load correctly, confirm skills framework runs.
2. **Accept or fix** — if Day 20 code is solid, build on it. If it's broken, fix it first.
3. **Don't rewrite** — Day 20 built the knowledge graph foundation, skills framework, and robot generator. Those are the base. Today extends them.

```bash
# Verify Day 20 state
cd backend && cargo check          # Does it compile?
cd ../pipeline && python3 -c "from skills.base import BaseSkill; print('Skills OK')"
ls data/knowledge/nodes/           # Are there nodes?
```

If any of these fail, fix them before proceeding. Do not stack new features on broken foundations.

### 0.2 Tech Debt Cleanup (start of day)

Delete dead v1 code that is confirmed unused. Nothing imports from these:

```bash
# Dead v1 directories — no imports anywhere in the codebase
rm -rf agents/          # v1 TUI signal extraction, replaced by intent parser
rm -rf simulation/      # v1 synthetic market generator, never used in v2
rm -rf research/        # v1 reddit client, replaced by pipeline/skills/search_reddit.py

# Dead one-off scripts — brainstorming artifacts, not reusable
rm pipeline/brainstorm_day19.py
rm pipeline/brainstorm_search.py
rm pipeline/migrate_to_lake.py   # one-off migration, already run
```

**`engine/` stays** — it has real scoring code (dimensions.py, ranker.py, scorer.py, vector_search.py) used by the pipeline. The cleanup_plan.md was wrong about this; it was written when engine/ only had stubs. Update `docs/cleanup_plan.md` to reflect this.

After deletion, verify nothing broke:
```bash
cd backend && cargo check
python3 -c "from pipeline.skills.base import BaseSkill; print('OK')"
```

---

## 1. The Big Idea

Today we close the loop: **when the system doesn't know something, it goes and finds out — in real time.**

Right now, search is static. We have ~135 properties across 8 areas. A user searching for "3bhk near Kadugodi metro" gets nothing because we haven't pre-indexed Kadugodi. That's unacceptable.

After today: the system detects the gap, calls Gemini with Google Search grounding, discovers real projects near Kadugodi, ingests them into the knowledge graph, and returns results — all within a single search request (~3-5 seconds). The discovered data persists, so the next search is instant.

---

## 2. Architecture

### 2.1 The Search Flow (Before & After)

**Before (static):**
```
Query → Parse intent → Filter seed data → Return matches (or nothing)
```

**After (live discovery):**
```
Query → Parse intent → Search existing corpus
  ├── Good matches → return immediately
  └── Poor/no matches → Live Discovery
       → Check discovery cache (same area+intent recently? skip)
       → Gemini 2.5 Flash + Google Search grounding
       → Parse JSON → Vec<DiscoveredProperty>
       → Ingest into knowledge graph + seed data
       → Score & rank against intent
       → Return with "Just discovered" badges
       → Queue background enrichment
```

### 2.2 Where Each Piece Lives

| Concern | Where | Why |
|---------|-------|-----|
| Live discovery (Gemini call) | **Rust backend** | It's an HTTP call. No need for Python at query time. |
| Intent parsing | **Rust backend** | Already exists in `search/intent.rs` |
| Discovery cache | **Rust backend** | In-memory LRU, keyed by area+intent hash |
| Background enrichment | **Python pipeline** | Reddit, RERA, photos, embeddings — batch work |
| Seed data persistence | **Rust backend** | Append discovered properties to `data/seed/` on disk |

### 2.3 Key Principle: One HTTP Call, Not a Script

The live discovery is a single `reqwest` call to Gemini's API from Rust. We do NOT:
- Shell out to Python
- Run a pipeline script
- Create a new microservice

It's just: build prompt → POST to Gemini → parse JSON → ingest. Same pattern as any API call.

---

## 3. Implementation Plan

### Phase A: Gemini Client in Rust (1-2 hours)

**Goal:** Rust can call Gemini Flash with Google Search grounding and get structured JSON back.

Create `backend/src/discovery/mod.rs`:

```rust
pub struct GeminiClient {
    api_key: String,
    http_client: reqwest::Client,
}

impl GeminiClient {
    /// Discover properties near a location using Gemini + Google Search
    pub async fn discover_properties(
        &self,
        area: &str,
        city: &str,
        constraints: &DiscoveryConstraints,  // bhk, budget, preferences
    ) -> Result<Vec<DiscoveredProperty>> { ... }
}

pub struct DiscoveredProperty {
    pub name: String,
    pub builder: String,
    pub area: String,
    pub configs: Vec<String>,       // "2BHK", "3BHK"
    pub price_range: String,        // "80L - 1.2Cr"
    pub price_per_sqft: Option<u32>,
    pub rera_number: Option<String>,
    pub possession_status: String,
    pub highlights: Vec<String>,
    pub source_url: Option<String>,
}
```

**The Gemini prompt** should match what `pipeline/skills/discover_properties.py` already uses — same JSON schema, same "only real verifiable projects" instruction. Don't reinvent it.

**PAUSE. Test: call Gemini for "3bhk near Kadugodi metro". Does it return real projects? Parse the JSON. Does it deserialize cleanly?**

### Phase B: Discovery Cache (30 min)

**Goal:** Don't call Gemini twice for the same area+intent.

```rust
pub struct DiscoveryCache {
    cache: HashMap<String, CachedDiscovery>,
    ttl: Duration,  // e.g., 24 hours
}

struct CachedDiscovery {
    properties: Vec<DiscoveredProperty>,
    discovered_at: Instant,
}
```

Cache key = `"{area}:{bhk_or_any}:{budget_bucket}"`. Simple. Not over-engineered.

**PAUSE. Test: discover → cache hit → cache miss after TTL.**

### Phase C: Ingestion — Discovery → Knowledge Graph + Seed Data (1-2 hours)

**Goal:** Discovered properties become first-class citizens in the system.

When Gemini returns properties:

1. **Create property entries** matching the existing `Property` struct shape (same as seed data)
2. **Create/update society entries** if the discovered project maps to a society
3. **Add to knowledge graph** as nodes with `source_type: Google`, `confidence: 0.6` (lower than manual seed data)
4. **Append to seed data files** — `data/seed/properties.json` gets new entries with `transparency_tags: ["Discovered via Search", "Verification Pending"]`
5. **Add to in-memory property list** — so subsequent searches within the same session find them

```rust
impl AppState {
    /// Ingest discovered properties into the live system
    pub async fn ingest_discovered(
        &self,
        discoveries: Vec<DiscoveredProperty>,
        triggered_by: &str,  // the search query
    ) -> Vec<Property> { ... }
}
```

**Important:** Use `discovered-{slug}` as the property ID prefix to distinguish from hand-curated seed data.

**PAUSE. Test: discover properties → they appear in subsequent searches → they persist across backend restarts.**

### Phase D: Wire Into Search Route (1 hour)

**Goal:** `/api/search` automatically triggers live discovery when needed.

Modify `backend/src/routes/search.rs`:

```rust
// After existing search returns results...
if results.is_empty() || max_score < DISCOVERY_THRESHOLD {
    if let Some(area) = &intent.area {
        // Check cache first
        if !discovery_cache.has_recent(area, &intent) {
            // Live discovery
            let discovered = gemini_client
                .discover_properties(area, "Bangalore", &intent.into())
                .await?;

            if !discovered.is_empty() {
                // Ingest and re-search
                let new_properties = state.ingest_discovered(discovered, &query).await;
                // Re-run search with expanded corpus
                results = search_with_new_properties(&intent, &new_properties);
                response.discovery_status = Some("discovered_new");
            }
        }
    }
}
```

**Add to SearchResponse:**
```rust
pub struct SearchResponse {
    // ... existing fields ...
    pub discovery_status: Option<String>,  // "from_cache" | "discovered_new" | null
    pub discovery_count: Option<usize>,    // how many new properties were found
}
```

**PAUSE. Test end-to-end: search for an area we don't have → Gemini discovers → results returned with discovery badge → search again → instant from cache.**

### Phase E: Background Enrichment Queue (1 hour)

**Goal:** Discovered properties get queued for deeper enrichment.

When live discovery happens, queue enrichment tasks:

```rust
// After ingestion
for property in &new_properties {
    graph.queue_enrichment(EnrichmentTask {
        entity_id: property.id.clone(),
        skill_needed: "learn_society".into(),
        priority: 1.0,
        triggered_by: vec![query.clone()],
        status: TaskStatus::Pending,
    });
}
```

The enrichment queue is already defined in the knowledge graph. The Python pipeline reads it via `GET /api/knowledge/enrichment/queue` and runs skills against pending tasks.

**Don't build the enrichment executor today.** Just queue the tasks. The pipeline's `enrich_all.py` (or a future `pipeline/enrich.py`) will process them.

**PAUSE. Test: discover → enrichment tasks appear in queue → `GET /api/knowledge/enrichment/queue` returns them.**

### Phase F: Frontend Discovery UX (1-2 hours)

**Goal:** The user sees when results were just discovered vs. from existing data.

Update the search results UI:

1. **Discovery banner** — when `discovery_status === "discovered_new"`:
   ```
   "We just found 6 new properties near Kadugodi. These are fresh — we're still verifying details."
   ```

2. **Per-card badge** — properties with `transparency_tags` containing "Discovered via Search" get a subtle "New discovery" chip.

3. **Confidence indicator** — discovered properties show "Verification pending" instead of full trust signals.

4. **Progressive enrichment** — as background enrichment runs and adds RERA, Reddit, Google Reviews data, the badges upgrade automatically on next page load.

**PAUSE. Test: search for unknown area → see discovery banner + badges → search again → results load instantly without banner.**

---

## 4. The Enrichment Feedback Loop

```
Day 21 establishes this cycle:

User searches "3bhk Electronic City"
  → No data for Electronic City
  → Gemini discovers 6 projects
  → Ingested with confidence 0.6, "Verification Pending"
  → Enrichment queue: learn_society × 6, verify_rera × 6
  → Results returned to user immediately

Background (pipeline reads queue):
  → verify_rera: RERA check → confidence bumps to 0.9 for verified ones
  → learn_society: Reddit + Gemini → maintenance, sentiment, family scores
  → fetch_google_reviews: ratings, review themes
  → embed_entity: embeddings for semantic search

Next user searches "quiet family flat Electronic City"
  → Instant results from enriched corpus
  → Full transparency signals: RERA verified, Reddit sentiment, Google rating
  → System is now an expert on Electronic City
```

---

## 5. Constraints & Guard Rails

### 5.1 Cost Control
- **Max 10 live discoveries per hour** — configurable via env var `MAX_DISCOVERIES_PER_HOUR`
- **Gemini Flash is cheap** — ~$0.001-0.003 per call. 10/hour = $0.72/day max.
- **Cache aggressively** — 24-hour TTL means at most 1 Gemini call per area per day.

### 5.2 Quality Control
- **Only trigger on recognizable areas** — intent parser must extract a valid area name. No Gemini calls for "asdfghjkl".
- **Validate Gemini response** — reject entries without project name, builder, or area. Don't ingest garbage.
- **Cap at 8 properties per discovery** — match what `discover_properties.py` requests.

### 5.3 Trust & Transparency
- **Discovered data starts at confidence 0.6** — lower than manual seed (0.8) or RERA (1.0).
- **Always show provenance** — "Discovered via Google Search" with timestamp.
- **Upgrade path is clear** — enrichment skills raise confidence as they verify.

---

## 6. What NOT to Build Today

- Streaming/SSE for progressive loading (regular request-response is fine for 3-5 second discovery)
- Python-side live discovery (Rust handles the Gemini call directly)
- New standalone scripts for discovery (extend existing backend code)
- Full area sweeps or robot generator changes (those are batch, not live)
- Embedding-based similarity matching (that's a separate phase)

---

## 7. Success Criteria

- [ ] Gemini client in Rust can discover properties for any Bangalore area
- [ ] Discovery cache prevents duplicate Gemini calls (same area+intent within TTL)
- [ ] Discovered properties are ingested into knowledge graph + seed data
- [ ] Discovered properties persist across backend restarts
- [ ] `/api/search` triggers live discovery when no good matches exist
- [ ] Search response includes `discovery_status` and `discovery_count`
- [ ] Enrichment tasks are queued for discovered properties
- [ ] Frontend shows discovery banner and per-card badges
- [ ] End-to-end: search unknown area → discover → see results → search again → instant
- [ ] Backend compiles and all existing tests pass
- [ ] Previous Day 20 features (knowledge graph, skills framework) still work

---

## 8. File Changes Expected

**New:**
- `backend/src/discovery/mod.rs` — GeminiClient, DiscoveryCache, DiscoveredProperty, ingestion logic

**Modified:**
- `backend/src/main.rs` — add GeminiClient to AppState
- `backend/src/routes/search.rs` — wire live discovery into search flow
- `backend/src/state.rs` — add discovery client + cache to AppState
- `frontend/src/pages/SocietySearchPage.tsx` or equivalent — discovery UX
- `frontend/src/lib/types.ts` — add discovery_status to SearchResponse

**Not touched:**
- `pipeline/` — no new scripts. Background enrichment uses existing skills.
- No new top-level Python scripts.

---

## 9. End-of-Day: Tech Debt Cleanup + CLAUDE.md Update

After all phases are done and tested:

### 9.1 Update CLAUDE.md

- Update **Section 16 (Architecture Reference)** cleanup status — remove `agents/`, `simulation/`, `research/` from "dead code" list (they're now deleted). Note that `engine/` is active code, not dead.
- Update **Section 17 (Knowledge Graph)** subsection numbers — they still say 14.1, 14.2, etc. from before the renumbering. Fix to 17.1, 17.2, etc.
- Review if any other sections reference deleted code or outdated assumptions.

### 9.2 Update docs/cleanup_plan.md

- Mark `agents/`, `simulation/`, `research/` as **DONE (deleted Day 21)**.
- Correct the `engine/` entry — it is NOT dead code. It has real scoring modules (dimensions.py, ranker.py, scorer.py, vector_search.py, types.py). Remove from "dead code" section and note it as active.
- Mark brainstorm scripts as **DONE (deleted Day 21)**.
- Add any new cleanup items discovered during the day.

### 9.3 Verify Everything Still Works

```bash
# Full health check
cd backend && cargo check && cargo test
cd ../frontend && npm run build
cd ../pipeline && python3 -c "from skills.base import BaseSkill; print('Skills OK')"

# Verify live discovery works end-to-end
curl "http://localhost:4000/api/search?q=3bhk+near+Kadugodi+metro"
```

### 9.4 Update Memory

Update `.claude/projects/-Users-gulshan-kumar-openestates/memory/MEMORY.md` with:
- Day 21 status
- New files created (discovery module)
- Dead code that was removed
- Any architectural decisions made during implementation

---

## 10. Estimated Cost

| Action | API | Cost |
|--------|-----|------|
| Live discovery (per call) | Gemini 2.5 Flash | ~$0.002 |
| 10 discoveries during dev | Gemini 2.5 Flash | ~$0.02 |
| Background enrichment (per entity) | Claude/Gemini | ~$0.05 |
| Total Day 21 | — | ~$0.50-1.00 |
