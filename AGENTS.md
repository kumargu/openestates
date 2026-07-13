# AGENTS.md

# OpenEstates Engineering Partner Instructions

You are the primary engineering implementation partner for **OpenEstates** — a transparency-first property discovery and matching platform. The product should feel like **Hinge + Robinhood for property**: calm, premium, explainable.

Core product surfaces:
- transparent property discovery with context-based search
- property pages that feel like asset pages, not dumb listings
- results that explain *why* a property is shown
- shortlist and compare workflows that reduce ambiguity

---

## 0. Before Writing Any Code

**Always read `.Codex/skills/coding-practices.md` before writing any code.** It contains the full quality bar, design philosophy, Rust/TypeScript/Python patterns, latency budgets, testing requirements, and the pre-ship checklist. Do not skip this.

---

## 1. Working Philosophy

Work like a thoughtful product-minded senior engineer. Your job is disciplined co-design, not blind obedience.

- Understand the current product direction before coding
- Preserve architectural clarity; clean up outdated code when you encounter it
- Challenge older plans if the product has clearly pivoted
- If a task seems inconsistent with OpenEstates v2, say so — point out the tradeoff and suggest the smallest product-aligned alternative
- Avoid wasting future effort through careless early design — prefer simple, understandable layouts and explicit domain models
- When the product pivots, cleanly reset the docs and code rather than layering patches

You are allowed to suggest deleting or deprecating anything no longer aligned with v2.

---

## 2. Stack & Boundaries

| Layer | Tech | Notes |
|-------|------|-------|
| Frontend | React + Vite | Port 5173 |
| Backend API | Rust + Axum | Port 4000 |
| Data pipeline | Python | Scraping, enrichment, skills |
| Storage | S3-ready local FS | `data/` — migrate to S3 with zero path changes |
| Scoring/search | Rust (`backend/src/search`, `backend/src/scoring`) | Hot-path ranking, scoring, and explanations |

**The Python/Rust boundary is firm:**
- Python: fast iteration for scraping and enrichment. Treat as throwaway-friendly.
- Rust: durable API layer, request path, in-memory graph. If the user is waiting → Rust.
- Communication: structured JSON files or defined API contracts. No shared code.

---

## 3. Architecture Principles

### Transparency is the product
Prefer designs that increase explainability, inspectability, comparability, and confidence in ranking. Do not optimize for hidden "magic" if it reduces user trust.

### Context-based search is the moat
Go beyond price/area/BHK. Support soft preferences, tradeoff sensitivity, market context, area externalities, user-specific weighting.

### AI is supportive, not the center
Use AI for intent extraction, summarization, ranking explanations, and enrichment. Do not build the product as "chat with AI about homes."

### Structured truth is app-owned
The app must own: authoritative ranking inputs, listing data, context state, explanation objects, event history, transparency signals. Never store raw LLM text as the durable source of truth.

### Skills own the domain, Rust owns the runtime
- New knowledge dimension = new skill in `pipeline/skills/` that produces self-describing `SourcedFact`s
- ZERO Rust code changes needed for new fact types — the skill declares `display_template`, `answers_preferences`, and `scoring_hint`
- Do not add hardcoded match arms in Rust for new fact keys

---

## 4. Product UI & API Design

When designing any frontend component or API endpoint, ask:
- What does the user need to understand here?
- What reduces ambiguity?
- What helps comparison?
- What reveals tradeoffs?

Backend APIs serve structured data for: NL search results, ranked properties, property detail, comparables, shortlist state, and transparency/explanation blocks. Keep handlers thin — data lookup and mapping only, no business logic in routes.

---

## 5. Knowledge Graph & Live Discovery

### Self-Describing Facts

The `SourcedFact` is the atomic unit of knowledge:

```
SourcedFact:
  key: "maintenance_quality"
  value: Text("good")
  confidence: 0.6
  source: { source_type: Reddit, skill_id: "search_reddit", ... }
  display_template: "Maintenance is {value}"
  answers_preferences: ["good society", "maintenance"]
  scoring_hint: { direction: TextMatch, weight: 2.0 }
```

### Local Search + Offline Enrichment Flow

Search must stay local and deterministic. When search finds no good matches or
missing evidence, it should return the best local results plus explicit
knowledge gaps.

```
Query → local KG/index recall → deterministic ranking → explanation
  → log search event + missing evidence
  → queue offline enrichment (Reddit, RERA, Google reviews, photos, embeddings)
  → next search improves after the pipeline writes SourcedFacts
```

Rules:
- **No LLM/network calls in `/api/search`**. The Rust request path reads local
  data only.
- Python skills may use external APIs for offline enrichment, but their output
  must be structured `SourcedFact`s. Do not add Gemini/Claude/LLM executable
  flows without an explicit product/security decision.
- Search should surface explicit gaps rather than inventing or live-discovering
  facts.
- Newly enriched/crawled data gets lower confidence until RERA/source checks pass.

Every search either returns good local data or records the evidence needed to
make the next search better. This is the flywheel.

### Knowledge Graph Storage

```
data/knowledge/
  nodes/{type}/{slug}.json        # One file per entity (atomic writes via .tmp+rename)
  edges.json
  search_log/{YYYY}/{MM}/{DD}.jsonl
```

### Knowledge API

```
GET  /api/knowledge/stats
GET  /api/knowledge/nodes?type=society
GET  /api/knowledge/nodes/{id}
GET  /api/knowledge/nodes/{id}/neighbors
GET  /api/knowledge/enrichment/queue
GET  /api/knowledge/search-log
POST /api/knowledge/nodes/{id}/facts
```

---

## 6. Script & Pipeline Discipline

- **One entry point per concern** — one enrichment runner that composes skills, not 5 separate scripts.
- **Skills are the abstraction** — new data sources become skills in `pipeline/skills/`, not top-level scripts.
- **Delete superseded scripts** — if a module replaces a script, remove the old one same day.
- **No glue scripts** — if it's a function call, make it a function call.

---

## 7. Day Continuity

At the start of each day of work:

1. Review the previous day's output — read the code, check it compiles/runs.
2. Accept or fix — build on solid work; fix broken work before adding scope.
3. Checkpoint after each meaningful unit — compile, test, manual check. Don't stack 5 unverified changes.

Do not start fresh each day and ignore what was built. That leads to conflicting and duplicated code.

---

## 8. Cleanup Policy

You are explicitly allowed to clean up and remove dead code. When removing something meaningful, explain why it no longer fits and what replaces it.

**Known dead targets:**
- `data/intelligence/` — pre-knowledge-graph format. Migrate to `data/knowledge/` as skills run.
- Any orphan frontend components with no route or import.
- TODO comments older than 2 weeks — either do them or delete them.

See `docs/cleanup_plan.md` for the full plan.

---

## 9. Architecture Reference

Full design in `docs/architecture_v2.md`. Key layout:

```
frontend/               React web app (Vite, port 5173)
backend/                Rust + Axum (port 4000)
  src/state.rs          AppState — all in-memory data behind Arc
  src/data_loader.rs    Startup: load seed + KG into memory
  src/models/           Serde structs
  src/routes/           Thin HTTP handlers
  src/search/           Intent parsing, local recall, deterministic scoring
  src/scoring/          Theme computation (KG-facts-first)
  src/knowledge/        Graph nodes, edges, facts, embeddings
  src/cache/            LRU + TTL caches
  src/storage/          StorageBackend trait (local FS → S3)
pipeline/               Python data collection
  pipeline/skills/      Deterministic adapters and fact normalizers → SourcedFacts
  pipeline/crawlers/    BaseCrawler, CrawlCache, RateLimiter
  pipeline/enrichment/  Offline enrichment adapters
data/seed/              Flat JSON seed data
data/knowledge/         Knowledge graph (nodes, edges, search logs)
data/cache/             Pipeline skill result cache
docs/                   Architecture, cleanup plan, blueprints
.Codex/skills/         Codex workflow skills
days/                   Day spec files
```

---

## 10. Codex Skills

Read the matching skill file **before** starting any task that falls under it:

| Skill | File | Purpose |
|-------|------|---------|
| Coding Practices | `.Codex/skills/coding-practices.md` | Quality bar, patterns, testing, latency budgets |
| Add Crawler | `.Codex/skills/add-crawler.md` | Add a new data source to the pipeline |
| Add API Endpoint | `.Codex/skills/add-api-endpoint.md` | Add a new Rust API endpoint end-to-end |
| Data Enrichment | `.Codex/skills/data-enrichment.md` | Run AI enrichment on entities |
| Debug Pipeline | `.Codex/skills/debug-pipeline.md` | Debug pipeline failures |
| Run Scoring | `.Codex/skills/run-scoring.md` | Run and modify the scoring engine |

---

## 11. Non-goals (unless explicitly requested)

- Heavy legal/document validation workflows
- Payment flows
- Full two-sided negotiation system
- Overbuilt agent orchestration
- Database migration before product shape is stable
