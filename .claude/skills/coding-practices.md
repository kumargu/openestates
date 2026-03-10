# Skill: OpenEstates Coding Practices

## When to use
Before writing any code. This is the quality bar and design philosophy for every change.

---

## 1. Frontend: Sleek, Calm, Premium

The frontend IS the product. Every pixel communicates trust.

### Design principles
- **Calm over busy** — whitespace is a feature, not wasted space. Let content breathe.
- **High information density done right** — show a lot, but never feel cluttered. Think Bloomberg terminal aesthetics meets Airbnb polish.
- **Motion with purpose** — subtle transitions on state changes (loading → loaded, collapsed → expanded). No gratuitous animation.
- **Mobile-first responsive** — property search happens on phones. Design for thumb reach.
- **Dark mode ready** — use CSS variables / Tailwind semantic tokens from day one.

### Component quality bar
- Every component should feel like it belongs in a design system, even before we have one.
- Consistent spacing scale (4px base). Consistent border radii. Consistent shadow depths.
- Typography hierarchy: max 3 font sizes per view. Headings, body, caption.
- Colors carry meaning: green = positive signal, amber = caution, red = risk. Never decorative.
- Loading states are not optional — skeleton loaders, not spinners. Never blank screens.
- Error states show what went wrong AND what the user can do about it.
- Empty states guide the user ("No shortlisted properties yet — search to find your first match").

### Code patterns
```typescript
// DO: Composable, single-responsibility components
<PropertyScoreBar score={72} label="Family Friendly" />
<SignalChip type="positive" text="Well maintained" />
<CautionChip text="Traffic congestion" />

// DON'T: Monolithic components that render everything
<PropertyCard property={p} showScore showSignals showCautions showCompare ... />
```

- Prefer `React.lazy` + `Suspense` for route-level code splitting.
- Extract shared UI into `frontend/src/components/ui/` — buttons, chips, cards, modals.
- Domain components in `frontend/src/components/` — PropertyCard, ScoreBreakdown, CompareTable.
- No inline styles. No `style={{}}`. Tailwind classes or CSS modules only.

---

## 2. User Journey: Minimize Friction, Maximize Flow

### The 3-click rule
A user should go from landing → understanding a property's value in 3 clicks or fewer:
1. Search / browse
2. See ranked results with clear "why"
3. Tap into detail → full conviction

### Journey design rules
- **No dead-end pages** — every page has a clear next action. Results → detail → shortlist → compare.
- **No unnecessary intermediate pages** — if a modal or drawer can do it, don't navigate. Shortlist toggle = inline action, not a new page.
- **Progressive disclosure** — show the headline, let users drill down. Score bar → click → full dimension breakdown.
- **State survives navigation** — search query, filters, scroll position persist when going back. Use URL params, not ephemeral state.
- **Instant feedback** — shortlist toggle, compare add, search submit should feel instant. Optimistic UI updates, background sync.

### Anti-patterns to avoid
- Wizard flows for simple actions (don't make users click "Next" 4 times to set preferences)
- Confirmation dialogs for reversible actions (shortlist add/remove = toggle, no "Are you sure?")
- Login walls before value is shown (anonymous browsing first, auth for save/compare)
- Separate search results page when inline results would work
- Pagination when infinite scroll or "load more" is better for browsing

---

## 3. Code Quality Bar

### Every PR / change must pass this mental checklist

| Check | Question |
|-------|----------|
| **Compiles clean** | `cargo check` (Rust), `npx tsc --noEmit` (TS) — zero warnings |
| **Types are tight** | No `any` in TypeScript. No unnecessary `unwrap()` in Rust. |
| **No dead code** | No commented-out blocks, no unused imports, no orphan files |
| **Names are clear** | A new reader understands what `score_society_maintenance()` does without reading the body |
| **Functions are small** | If a function is > 40 lines, it probably does too much |
| **Error handling is real** | Rust: `Result` types with meaningful errors. TS: try/catch at boundaries, typed error states in UI |
| **No magic numbers** | Constants have names. Thresholds are documented. |
| **Modules have boundaries** | Files don't reach into other modules' internals |

### Rust-specific
- Prefer `&str` over `String` in function args where possible.
- Use `serde(rename_all = "camelCase")` on all API response structs — frontend expects camelCase.
- Handler functions stay thin: extract logic into domain functions that are testable without HTTP context.
- Group related routes in one file, but split when a file exceeds ~200 lines.
- See **Section 8: Rust as the Hot Path** for the full Rust philosophy.

### TypeScript-specific
- Use `type` over `interface` for API response shapes (consistency with the codebase).
- API types in `types.ts`, API calls in `api.ts` — never fetch inside components.
- Hooks for data fetching: `useProperties()`, `useSearch(query)` — components stay declarative.
- Avoid `useEffect` for derived state — use `useMemo` or compute inline.

### Python pipeline-specific
- Type hints on all function signatures — `def score(facts: list[SourcedFact]) -> float:`
- Pydantic models for any structured data crossing boundaries (API responses, skill outputs).
- No global mutable state. Functions take inputs, return outputs.
- `if __name__ == "__main__":` block on every script for direct execution.

---

## 4. Claude Skills Are the Intelligence Layer

### Default to skills for any ML/AI/enrichment work

When you need the system to "understand" or "judge" something, that's a skill:

| Need | Solution |
|------|----------|
| Score a society on a dimension | `pipeline/skills/score_society.py` |
| Extract intent from user query | Skill, not regex |
| Summarize Reddit threads | Skill with Claude/Gemini |
| Detect gaps in entity knowledge | `pipeline/skills/identify_gaps.py` |
| Classify images | Skill with vision model |
| Generate ranking explanations | Skill, not template strings |

### Skill design rules
- Every skill produces `SourcedFact` entries — never raw text dumps.
- Every fact has `display_template`, `answers_preferences`, `scoring_hint` — self-describing.
- Skills are idempotent — running twice with same input produces same output.
- Skills are cached — same input + version hash = skip execution, return cached result.
- Skills log cost — every LLM call records token count and estimated cost.
- New knowledge dimension = new skill, ZERO Rust code changes.

### When NOT to use a skill
- Pure data fetching (HTTP GET, scraping) — use a crawler or plain script.
- Deterministic transformations (JSON reshape, filtering) — use a function.
- Config/constants — use a file.

---

## 5. Continuous Cleanup: Code Debt Is a Tax on Every Future Change

### Cleanup is not a separate task — it's part of every task

When touching a file:
- Remove unused imports you see.
- Delete commented-out code you encounter.
- Rename unclear variables in the functions you modify.
- If a function you're editing has a dead code path, remove it.

### Weekly cleanup triggers (ask yourself)
- Are there files that haven't been touched in 5+ days and aren't referenced anywhere? → Delete.
- Are there TODO comments older than 2 weeks? → Either do them or delete them.
- Are there duplicate functions doing similar things? → Merge into one.
- Are there scripts that were superseded by skills? → Delete the scripts.
- Does `cargo check` or `tsc` produce warnings? → Fix them.

### Deletion policy
- **Delete confidently** when: code is unreferenced, tests pass without it, no runtime import.
- **Deprecate first** when: code might be used by external scripts or other team members.
- **Document the deletion** in the day's commit message — "Removed X because Y replaced it."

### Specific cleanup targets (keep this list updated)
- `data/intelligence/` — old pre-knowledge-graph format. Migrate to `data/knowledge/` as skills run.
- Any `brainstorm_*.py` or `migrate_*.py` script — one-time scripts should not persist.
- Frontend components with no route or import — orphan components are dead code.

---

## 6. Long-Term Vision: Build for the Startup That Might Be

### Architecture bets that compound

| Decision | Why it matters long-term |
|----------|------------------------|
| **Self-describing facts** | New dimensions without code deploys. Non-engineers can add knowledge types. |
| **S3-compatible storage layout** | Local dev → S3 prod with zero path changes. Scale storage independently. |
| **Skills as intelligence** | Swap Claude for GPT or local model without touching Rust. Intelligence is pluggable. |
| **Knowledge graph over flat data** | Supports cross-entity queries, recommendations, pattern detection at scale. |
| **Transparency as product** | Regulatory moat. Users trust explainable systems. Competitors can't fake this. |
| **Rust backend** | When scale hits, the backend won't be the bottleneck. Async by default. |

### Design decisions to protect
- **Never store raw LLM output as source of truth** — always parse into structured SourcedFacts.
- **Never hardcode city/area-specific logic** — the system should work for any geography.
- **Never couple frontend to backend internals** — API contracts are the boundary.
- **Never skip provenance** — every fact, score, and ranking must trace to its source.

### Things to build toward (but not prematurely)
- Multi-city support — storage layout already supports it, keep code city-agnostic.
- User accounts + saved preferences — design data models to be user-scoped when the time comes.
- Real-time data (price changes, new listings) — event-driven updates to the knowledge graph.
- Collaborative features (share shortlist, compare with partner) — keep state serializable.
- Mobile app — React Native or web wrapper. Keep business logic in API, not frontend.

### Scale readiness without premature optimization
- Keep data access patterns simple (full scan is fine for < 10K entities).
- Use in-memory caches, not distributed caches, until proven necessary.
- Monorepo is fine. Microservices are not needed yet.
- SQLite or Postgres when JSON files get painful — not before.

---

## 8. Rust as the Hot Path

### The principle: if it's on the request path, it's Rust

Rust owns everything that needs to be fast, reliable, and always-on. Python is for offline batch work. The boundary is simple: **user is waiting → Rust. User is not waiting → Python.**

### What MUST live in Rust

| Concern | Why Rust | Example |
|---------|----------|---------|
| **Search & ranking** | Sub-100ms responses. Users won't wait. | Intent parsing, scoring, filtering, sorting |
| **Live discovery** | Real-time Gemini calls + ingestion on cache miss | `backend/src/discovery/` |
| **Knowledge graph reads** | In-memory graph traversal, fact lookups, neighbor queries | `backend/src/knowledge/` |
| **API response assembly** | Join properties + societies + facts + scores into response shapes | Route handlers |
| **Caching (request-level)** | LRU + TTL for search results, discovery responses | `backend/src/cache/` |
| **Rate limiting** | Per-endpoint, per-IP, per-discovery limits | Middleware |
| **Data validation on ingest** | When Python skills push facts via API, Rust validates before storing | POST handlers |

### What stays in Python

| Concern | Why Python | Example |
|---------|-----------|---------|
| **Batch enrichment** | Can take minutes per entity. No user waiting. | Reddit scraping, RERA verification |
| **LLM skill execution** | Claude/Gemini calls for scoring, synthesis. Latency-tolerant. | `pipeline/skills/` |
| **Crawling & scraping** | Messy HTML, retry logic, rate limits. Python is more forgiving. | `pipeline/crawlers/` |
| **Data migration & cleanup** | One-time or periodic. Not on any hot path. | Seed data transforms |
| **Embedding generation** | Batch vectorization. Can run overnight. | `pipeline/skills/` (embedding skills) |

### Rust performance rules

- **Zero allocations on the hot path where practical** — use `&str`, slices, iterators. Don't `.clone()` unless you must.
- **Pre-compute at startup, serve at request time** — load seed data, build indexes, warm caches in `main.rs` before accepting requests. Request handlers should be lookups, not computations.
- **`RwLock` not `Mutex` for shared state** — multiple concurrent readers (search requests) should never block each other. Writers (live discovery ingestion) are rare.
- **Stream large responses** — if returning 100+ entities, consider `axum::body::Body` streaming instead of collecting into a `Vec` then serializing.
- **Async all the way** — every I/O operation (disk reads, Gemini HTTP calls, fact pushes) must be async. Never block the Tokio runtime with synchronous I/O.
- **Batch disk writes** — when ingesting discovered entities, buffer and write in batches, not one `fs::write` per entity.

### Async-first latency patterns

The golden rule: **never make the user wait for work that can happen in the background.** Rust's async runtime (Tokio) makes this nearly free — use it aggressively.

#### Pattern 1: Parallel fan-out with `tokio::join!`
When a request needs data from multiple independent sources, fetch them all concurrently:
```rust
// DO: parallel — total latency = max(a, b, c)
let (text_results, semantic_scores, kg_facts) = tokio::join!(
    search_text(&query),
    compute_semantic_scores(&query),
    fetch_kg_context(&area),
);

// DON'T: sequential — total latency = a + b + c
let text_results = search_text(&query).await;
let semantic_scores = compute_semantic_scores(&query).await;
let kg_facts = fetch_kg_context(&area).await;
```
Use `tokio::join!` for 2-3 tasks. Use `futures::future::join_all` for dynamic-length task lists.

#### Pattern 2: Best-effort background enrichment with `tokio::spawn`
When extra work would improve results but isn't required for the response:
```rust
// Return results immediately, enrich in background
let results = search_text(&query).await;

// Fire-and-forget: cache the query embedding for next time
tokio::spawn(async move {
    if let Some(embedding) = embed_client.embed(&query).await {
        embedding_cache.insert(query_hash, embedding).await;
    }
});

Ok(Json(results))
```
Rules for `tokio::spawn`:
- The spawned future must be `'static` — clone/arc what you need.
- Log errors inside the spawn, don't let them vanish silently.
- Never spawn CPU-heavy work — use `tokio::task::spawn_blocking` instead.

#### Pattern 3: Timeout-bounded optional enrichment
When an external API call (Gemini, embeddings) would improve results but has unpredictable latency:
```rust
use tokio::time::{timeout, Duration};

// Give the embedding API 200ms. If it's slow, skip the boost.
let semantic_boost = match timeout(
    Duration::from_millis(200),
    compute_semantic_scores(&embed_client, &graph, &query)
).await {
    Ok(scores) => scores,
    Err(_) => HashMap::new(),  // Timed out — graceful degradation
};
```
Apply this to any external API call on the request path. The user gets fast results regardless of third-party latency.

#### Pattern 4: Pre-warm caches at startup
Don't wait for the first request to populate caches:
```rust
// In main.rs, after loading data:
tokio::spawn(async move {
    // Pre-embed common search patterns from search logs
    for pattern in common_search_patterns(&search_log).await {
        if let Some(emb) = embed_client.embed(&pattern).await {
            embedding_cache.insert(hash(&pattern), emb).await;
        }
    }
    tracing::info!("Pre-warmed {} embedding cache entries", count);
});
```

#### Pattern 5: Read-through cache with async fill
Cache misses trigger async computation. Subsequent requests get the cached value:
```rust
pub async fn get_or_compute<F, Fut>(&self, key: K, compute: F) -> V
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = V>,
{
    if let Some(cached) = self.cache.read().await.get(&key) {
        return cached.clone();
    }
    let value = compute().await;
    self.cache.write().await.insert(key, value.clone());
    value
}
```

#### When NOT to go async
- **Pure CPU computation** (scoring, sorting, filtering in-memory data) — these are already fast. Async adds overhead for no gain. Just run them synchronously in the async context.
- **Trivial operations** (hashmap lookups, string formatting) — don't wrap in futures.
- **Sequential dependencies** — if B truly depends on A's result, await A then B. Don't fake parallelism.

### Moving logic from Python to Rust — when to pull the trigger

Ask: **Is this on the user's critical path?**

| Signal | Action |
|--------|--------|
| Python function called during API request handling | Move to Rust immediately |
| Python script called by a cron/batch job | Keep in Python |
| Python function that's slow but only runs offline | Keep in Python, optimize later |
| Python logic that's simple and deterministic (scoring formula, text matching) | Good candidate for Rust — easy to port, big latency win |
| Python logic that's complex and LLM-dependent (synthesis, explanation generation) | Keep in Python as a skill — Rust just serves the cached result |

### Rust code organization

```
backend/src/
  main.rs              # Router, startup, server config
  state.rs             # AppState — all in-memory data behind Arc
  data_loader.rs       # Startup: load seed + knowledge graph into memory
  models/              # Serde structs — domain entities
  routes/              # Thin HTTP handlers — extract, lookup, respond
  search/              # Search engine — intent parsing, text scoring, semantic boost
    intent.rs          # Query intent extraction
    text.rs            # Text-based search + graph preference scoring
    semantic.rs        # Semantic similarity scoring (embedding-based)
  scoring/             # Theme computation — KG-facts-first, used by routes
  knowledge/           # Knowledge graph — nodes, edges, facts, traversal, embeddings
    embed_client.rs    # Async Gemini embedding client (query → 768-dim vector)
    embeddings.rs      # Cosine similarity, similar_to_vector(), embedding stats
  discovery/           # Live discovery — Gemini client, cache, ingestion
  cache/               # LRU + TTL caches (search results, embeddings, discovery)
  storage/             # StorageBackend trait (local FS now, S3 later)
```

**Module boundaries are strict:**
- `routes/` calls into `search/`, `knowledge/`, `discovery/`, `scoring/` — never the reverse.
- `search/` reads from `state` (knowledge graph, properties) — never writes. Calls `knowledge/embed_client` for query embedding.
- `discovery/` is the only module that mutates `state` at runtime (via `RwLock` write).
- `scoring/` is stateless — takes inputs, returns computed themes. No I/O.
- `models/` has no logic — pure data structs with derives.
- **External API calls** (`embed_client`, `discovery/gemini_client`) are always behind `Option` — missing API key = graceful degradation, never a crash.

### The latency budget

For a search request, the budget is **200ms total** from request to response:

| Phase | Budget | What happens | Async strategy |
|-------|--------|-------------|----------------|
| Intent parsing | 5ms | Regex + keyword extraction (local, no LLM) | Sync — too fast to bother |
| Graph lookup | 10ms | In-memory traversal, fact matching | Sync — in-memory |
| Text scoring & ranking | 20ms | Score all matching entities, sort | Sync — CPU-only |
| Semantic boost (cache hit) | 5ms | Cosine similarity against cached query embedding | Sync — just math |
| Semantic boost (cache miss) | 200ms | Embed query via Gemini API | **`tokio::join!` with text search, timeout 200ms** |
| Response assembly | 5ms | Join data, serialize JSON | Sync |
| **Subtotal (warm cache)** | **~45ms** | Text + cached semantic in parallel | |
| **Subtotal (cold cache)** | **~200ms** | Text + live embedding in parallel (timeout-bounded) | |
| Live discovery (corpus miss) | +150ms | Gemini Flash call + ingestion | **`tokio::join!` with search, runs in parallel** |
| Background cache fill | 0ms (user) | Spawn embedding cache write after response | **`tokio::spawn` — fire-and-forget** |

**Key principle:** External API calls (Gemini embed, Gemini discovery) run in **parallel with local computation**, bounded by timeout. The user never waits for the slower path unless both paths are slow.

If any phase consistently exceeds its budget, that's a performance bug worth investigating.

---

## 9. Code Review Mental Model

When reviewing or writing code, ask these questions in order:

1. **Does this serve the user?** If a change doesn't improve search, discovery, transparency, or trust — why are we doing it?
2. **Is this the simplest version?** Can it be done with fewer files, fewer abstractions, fewer lines?
3. **Will I understand this in 2 weeks?** If not, rename things or add a one-line comment.
4. **Does this make the next change harder or easier?** Good code opens doors. Bad code closes them.
5. **Am I building for today's product or yesterday's prototype?** Kill legacy assumptions early.

---

## 10. Testing: Verify Every Change

### Smoke tests are mandatory after every task

Run `./tests/smoke_test.sh` after every task. It exercises all API endpoints and validates response shapes. Zero failures = ship it. Any failure = investigate before proceeding.

```bash
# Start backend, run smoke tests
cd backend && cargo run &
sleep 3
./tests/smoke_test.sh        # default port 4000
./tests/smoke_test.sh 8080   # custom port
```

### What the smoke tests cover

| Area | Tests | What's validated |
|------|-------|-----------------|
| Health | 1 | Server is up, returns `{"status":"ok"}` |
| Properties list | 4 | Returns array, non-empty, has required fields (id, title, area, price, bhk), has transparency_tags |
| Property detail | 6 | Returns property, themes (value/commute/society/risk), tradeoffs (headline/strengths/cautions), market_activity, similar_properties array, valid theme labels |
| Property 404 | 1 | Bad ID returns HTTP 404 |
| Search | 5 | Returns results for valid query, match fields present, intent parsed, query echoed, empty query returns empty results |
| Areas | 3 | Returns array, non-empty, has required fields |
| Knowledge graph | 3 | Stats have node/fact counts, society nodes exist |
| Shortlist | 1 | Returns object with shortlist array |

### Rust unit tests

Run `cd backend && cargo test` for unit tests. Current tests cover:
- Cosine similarity (exact match, orthogonal, known angle)
- Intent parsing (area + BHK extraction, budget parsing)

When adding new Rust modules, add tests in the same file:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function() {
        assert_eq!(my_function(input), expected);
    }
}
```

### Frontend build check

```bash
cd frontend && npm run build
```
This runs TypeScript type checking + Vite build. Zero errors = frontend is deployable.

### What to test after specific changes

| Change type | Tests to run |
|-------------|-------------|
| Rust backend (any) | `cargo check` + `cargo test` + `./tests/smoke_test.sh` |
| API response shape change | Smoke tests + `cd frontend && npx tsc --noEmit` (ensure TS types match) |
| Frontend component | `npm run build` |
| Scoring/themes change | Smoke tests (themes validated in property detail tests) |
| Search logic change | Smoke tests (search tests cover results, intent, match fields) |
| Knowledge graph change | Smoke tests (KG stats + node tests) |
| New API endpoint | Add a test to `tests/smoke_test.sh`, then run it |
| Python pipeline/skill | Run the skill directly + verify output |

### Adding new smoke tests

The smoke test uses a `check` function:
```bash
check "Human-readable test name" \
  "${BASE}/api/endpoint" \
  '.jq_expression_that_must_be_truthy' \
  "description of what failed"
```

When adding a new API endpoint, add at least:
1. A basic 200 response check
2. A response shape validation (required fields present)
3. An edge case (bad input returns appropriate error)

### Test philosophy

- **Smoke tests over mocks** — hit real endpoints with real data. Catches integration bugs that unit tests miss.
- **Shape over value** — test that fields exist and have correct types, not specific values (seed data changes).
- **Fast over comprehensive** — the full smoke suite runs in <5 seconds. Keep it fast so it gets run.
- **Exit code matters** — `smoke_test.sh` exits with failure count. CI can gate on `exit 0`.

---

## 11. Operational Overhead: Build for Agent-Driven Operations

### The principle: this system will be run by agents, not humans

Every operational task — enrichment, deployment, monitoring, cleanup — must be automatable with a single command. No interactive prompts. No manual steps. No scripts that require human judgment to run.

### Rules for low-ops overhead

| Rule | Why |
|------|-----|
| **One entry point per concern** | `python3 -m pipeline.enrich` handles ALL enrichment. Not 5 scripts. |
| **Idempotent commands** | Running the same command twice produces the same result. Safe to retry. |
| **No interactive prompts** | Every script accepts flags, not stdin. Agents can't type "y" to confirm. |
| **Structured output** | JSON for machine consumption, human-readable summaries on stderr. |
| **Exit codes matter** | 0 = success, 1 = failure. Agents gate on this. |
| **Self-healing defaults** | Missing env var = graceful skip, not crash. Missing data = partial result, not abort. |
| **Cost-aware by default** | Every LLM/API call logs estimated cost. Budget caps prevent runaway spending. |

### Script discipline (firm)

- **No proliferating scripts.** New data source = new skill file in `pipeline/skills/`, registered in the enrichment engine. Not a new top-level script.
- **No one-off test scripts** that persist. Write them, run them, delete them. Or put them in `tests/`.
- **No manual orchestration steps.** If step A must happen before step B, the engine handles ordering. Not a README instruction.
- **Delete scripts that are superseded.** If `pipeline/enrich.py` replaces `pipeline/enrich_all.py`, delete the old one same day.

### Agent readiness checklist

When writing any operational code, ask:
- Can an agent run this with zero context? (flags, not tribal knowledge)
- Can an agent understand the output? (JSON, structured logs, exit codes)
- Can an agent recover from failure? (idempotent, retry-safe, partial results)
- Can an agent decide whether to run this? (staleness check, `--plan` mode)
- Does this require a human in the loop? If yes, redesign until it doesn't.

---

## 12. Cost Sensitivity: Every API Call Has a Price

### Track costs at every layer

| Layer | Tracking | Budget |
|-------|----------|--------|
| **LLM skills** (Claude, Gemini) | `SkillCost.estimated_usd` per run | $1/day default cap |
| **External APIs** (Reddit, RERA, SerpAPI) | `SkillCost.api_calls` per run | Rate-limited per source |
| **Embedding API** (Google) | Token count per call | Batch, not per-request |
| **Live discovery** (Gemini Flash) | Per-query, rate-limited | Max N/hour in Rust |

### Cost tiers for skills

```
free:      Reddit API, RERA scraping, image search (no LLM)
cheap:     Gemini Flash ($0.001/call), embeddings ($0.0001/call)
moderate:  Claude Sonnet ($0.006/call), Gemini grounded search
expensive: Claude Opus ($0.06/call) — only for high-value judgment
```

### Rules

- **Default to the cheapest model that works.** Gemini Flash for synthesis, not Opus.
- **Cache aggressively.** Same input + version = skip. Skills have built-in caching.
- **Budget caps per run.** `--budget 1.00` stops execution when estimated cost exceeds $1.
- **Log cumulative cost.** Every enrichment run prints total spend at the end.
- **Free skills run first.** Always: free → cheap → expensive. Never burn $$ when free data is missing.
- **Never call LLMs in the request path** unless timeout-bounded and cached. User latency > cost optimization.

---

## 13. Latency Sensitivity: Users Won't Wait

### Hard latency budgets

| Operation | Budget | Strategy |
|-----------|--------|----------|
| Search (warm cache) | 50ms | In-memory graph + text scoring |
| Search (cold cache) | 200ms | Parallel: text + semantic embed, timeout-bounded |
| Property detail | 30ms | Pre-computed, served from memory |
| Live discovery | 200ms | Parallel with search, Gemini Flash |
| Enrichment (per entity) | 5-30s | Background only, never request path |

### Rules

- **Never block a response on an external API** without a timeout. Use `tokio::timeout`.
- **Parallel fan-out by default.** `tokio::join!` for independent data fetches.
- **Degrade gracefully.** If semantic search times out, return text-only results. Never error.
- **Pre-compute at startup.** Load data, build indexes, warm caches before accepting requests.
- **Background enrichment is invisible to users.** They see what's cached. Pipeline fills gaps async.

---

## 14. Testing Principles: Fast, Real, Layered

### The testing pyramid for OpenEstates

```
Layer 1: Type checking (instant, every save)
  cargo check, npx tsc --noEmit
  Catches: type mismatches, missing fields, import errors

Layer 2: Unit tests (seconds, every commit)
  cargo test, pytest
  Catches: logic bugs in scoring, parsing, similarity

Layer 3: Smoke tests (5 seconds, every deploy)
  ./tests/smoke_test.sh
  Catches: API shape changes, missing endpoints, 500 errors

Layer 4: Integration tests (minutes, weekly)
  python3 -m pipeline.enrich --plan (dry run)
  Catches: pipeline wiring, skill failures, data quality

Layer 5: Journey tests (minutes, before release)
  pipeline/journey_property_to_shortlist.py
  Catches: end-to-end user flow breaks
```

### Rules

- **Type checking is non-negotiable.** Zero warnings in both Rust and TypeScript.
- **Smoke tests run after every backend change.** They take 5 seconds. No excuse to skip.
- **Test shape, not values.** Seed data changes. Test that fields exist and have correct types.
- **No mocks for API tests.** Hit real endpoints with real data. Integration bugs hide behind mocks.
- **Tests must be fast.** If a test takes > 30s, it's too slow. Speed is a feature of tests.
- **Pipeline `--plan` mode is a test.** Shows what would run without executing. Validates wiring.

---

## Checklist: Before Shipping Any Change

**Build & Types**
- [ ] Rust: `cargo check` + `cargo test` — zero warnings
- [ ] Frontend: `npx tsc --noEmit` — zero errors
- [ ] Smoke tests pass (if backend changed): `./tests/smoke_test.sh`

**Code Quality**
- [ ] No `any` in TypeScript, no unnecessary `unwrap()` in Rust
- [ ] No dead code added (unused functions, commented blocks, orphan files)
- [ ] API types in `types.ts` match Rust response structs
- [ ] New AI/ML logic lives in a skill, not hardcoded in Rust

**UX & Product**
- [ ] Loading and error states handled in any new UI
- [ ] User can reach the new feature in ≤ 3 clicks from home

**Operational Readiness**
- [ ] New scripts are not needed — extend existing entry points instead
- [ ] Any CLI command works without interactive prompts
- [ ] Costs logged for any new LLM/API integration
- [ ] No request-path code blocks on external APIs without timeout
- [ ] Commit message explains WHY, not just WHAT
