# OpenEstates Project Memory

## Project Summary
Transparency-first property discovery and matching platform (v2). Web-first product, not a terminal prototype.
Tagline: "Hinge + Robinhood for property" — calm, premium, explainable.

## Stack (v2)

| Layer | Tech |
|---|---|
| Data collection / crawling / enrichment | Python (httpx, pydantic, anthropic SDK) |
| Backend API | Rust + Axum |
| Frontend | React |
| Storage | S3-ready data lake (local FS now, S3 later) |
| Scoring engine | Python (engine/) — multi-dimensional scoring + vector search |
| Vector embeddings | OpenAI ada / numpy (FAISS later) |
| Caching | Rust in-memory LRU (backend), HTTP response cache (pipeline) |

Python handles data work. Rust+Axum serves the structured API. Clean boundary via `data/lake/`.

## Key Paths (v2)
- Day specs: `days/dayNN.md`
- Docs/blueprint: `docs/` (being created Day 5+)
- Data: `data/` (JSON seed files)
- GitHub: `https://github.com/kumargu/openestates` (private)

## v2 Project Layout
```
frontend/               React web app
backend/                Rust + Axum API
  src/storage/          StorageBackend trait, LocalFsBackend (S3-ready)
  src/cache/            Cache trait, InMemoryCache (LRU+TTL)
  src/discovery/        GeminiClient, DiscoveryCache, ingestion (live discovery)
  src/search/           TextSearch (vector search later)
pipeline/               Python data collection
  pipeline/storage/     StorageClient, KeyBuilder, ManifestIndex
  pipeline/crawlers/    BaseCrawler, CrawlCache, RateLimiter
  pipeline/enrichment/  BaseEnricher, ClaudeEnricher, Embedder
engine/                 Scoring engine (scorer, ranker, dimensions, vector_search)
data/lake/              S3-structured data lake
  {entity}/{city}/{area}/{id}/data.json
  manifests/*.json      Lightweight indexes
data/seed/              Legacy flat JSON (backward compat)
docs/                   Architecture, cleanup plan, blueprints
.claude/skills/         Claude Code workflow skills (5 skills)
days/                   Day spec files
```

## Architecture Docs
- `docs/architecture_v2.md` — Full scalable architecture design (storage, vectors, caching, data flow)
- `docs/cleanup_plan.md` — Dead code identification, refactoring targets
- `.claude/skills/` — 5 skills: add-crawler, add-api-endpoint, data-enrichment, debug-pipeline, run-scoring

## v1 Code Cleanup (completed Day 21)
- `agents/`, `simulation/`, `research/` — DELETED Day 21
- `pipeline/brainstorm_day19.py`, `pipeline/brainstorm_search.py`, `pipeline/migrate_to_lake.py` — DELETED Day 21
- `engine/` — NOT dead code, has active scoring modules (dimensions.py, ranker.py, scorer.py, vector_search.py)

## Workflow
- Each day has a spec at `days/dayNN.md`
- Read CLAUDE.md + LEARNING.md before any architectural decisions
- Update README.md daily build log after each day
- Day 5 output: `docs/openestates_v2_surfaces_and_data.md` (main product blueprint)

## Day Progress
- Days 1-5: DONE — v1 prototype → v2 reset, product blueprint
- Day 6: DONE — seed dataset: 20 properties, 5 area profiles, 12 societies
- Day 7: DONE — Rust+Axum backend, React frontend, all API endpoints, full page shells
- Day 8: DONE — contract stabilization (was already done in Day 7 session)
- Day 9: DONE — NL search, match labels, shortlist compare (also done in Day 7 session)
- Day 10: DONE — property detail conviction widgets, theme-based compare workspace, new compare.ts + market.ts
- Day 11: DONE — fixed Playwright review pipeline, hardened conviction surfaces
- Day 12: DONE — restored build/review loop trust, first verifiable transparency journey
- Day 13: DONE — isolated review harness, proved live rendering
- Day 14: DONE — separated tooling failure from product, intentional fallback UX, review gate
- Day 15: DONE — render truth without review tool, product-owned offline/fallback states
- Day 21: DONE — Live Discovery: Gemini client in Rust, discovery cache (LRU+TTL+rate limit), ingestion into knowledge graph + seed data, search route wiring, enrichment queue, frontend discovery UX (banner + badges). Deleted dead v1 code. Properties list changed to RwLock<Vec<Property>> for runtime mutation.

Latest Vercel deploy: `https://frontend-i553r6p0q-kumargulshan2192-8341s-projects.vercel.app`

## Day Agent (`pipeline/agent.py`)
- ChatGPT (Firefox Playwright) = product visionary, generates day plans
- Claude Opus = coder, invoked via `claude --model claude-opus-4-6 --print` CLI
- Run single day: `python3 pipeline/agent.py`
- Run multi-day sprint: `python3 -u pipeline/agent.py --loop 6` (overnight mode)
- `--plan-only`: generate plans without coding
- `--resume`: resume from checkpoint
- Checkpoints saved to `pipeline/checkpoints/dayNN.json` after each step
- ChatGPT conversation ID in `.env`: `CHATGPT_CONVERSATION_ID`
- Full loop: plan → code → smoke test → feedback → Vercel deploy → page capture → journey review

## Agent Rules (FIRM)
1. ChatGPT output saved as `.md` — prompt explicitly requests markdown
2. Checkpoint after every step — agent is resumable
3. Use `python3 -u` for unbuffered output when monitoring
4. Firefox must be CLOSED before running (Playwright copies profile cookies)

## Data Lake Key Scheme
`{entity_type}/{city}/{area}/{id}/data.json`
- `properties/bengaluru/whitefield/prop-w-001/data.json`
- `societies/bengaluru/whitefield/prestige-shantiniketan/data.json`
- `manifests/properties_all.json` (index of all properties)
- `manifests/properties_bengaluru.json` (city-filtered index)
- Python and Rust share the same key scheme

## Scoring Engine
- 7 dimensions: value, commute, society_quality, risk, greenery, resale, market_activity
- Each returns 0-1 score with explanation
- `PropertyScorer.score_property(prop_dict, ScoringContext)` → `ScoredProperty`
- `Ranker.rank(properties, context)` → `RankingResult` with ranked + explained results
- Transparency tags auto-generated from scores

## Intelligence Architecture: Claude Skills + Rust Knowledge Graph

This is the core architectural decision (Day 22+). Claude replaces traditional ML:

| Layer | Role | Owns |
|-------|------|------|
| Python scripts | Mechanical data fetching (scrape, crawl, download) | Raw data |
| **Claude Skills** | Judgment, scoring, ranking, pattern detection, explanation | Intelligence decisions |
| **Rust Knowledge Graph** | Durable memory, fast retrieval, structured serving | Facts, scores, edges |
| Backend API | Serves organized data to frontend | API responses |

**Skills think. Graph remembers. Backend serves.**

### How it works
- Scripts fetch raw data → stored in data lake
- Claude Skills read raw data → produce SourcedFacts with scores, explanations, confidence
- Facts pushed to Rust Knowledge Graph via API
- Backend serves graph data to frontend with full explainability

### Why this beats traditional ML
- Zero training data needed — Claude understands domains natively
- No feature engineering — reads raw text and extracts what matters
- New dimensions = new skill, zero retraining (add "ev_charging" → instant)
- Explainable by default — every score has reasoning (transparency = product promise)
- The system learns at the speed of adding facts, not retraining models

### Built Skills (Tier 1 — DONE)
- `score-society` — reads KG facts → 6 dimension scores with explanations (100% test pass rate)
- `rank-for-intent` — given query + societies → ranked list with evidence-based reasoning
- `identify-gaps` — compares KG facts vs desired → enrichment queue with priorities

### Planned Skills (Tier 2)
- `detect-patterns` — scan across entities → find area-level signals
- `audit-quality` — check consistency of scores, facts, images

### Key principle
Scripts are dumb pipes. Skills are the intelligence. Graph is the memory.
Adding a new knowledge dimension requires ZERO Rust code changes — just a new skill that produces self-describing SourcedFacts.

## Day 22
- DONE: Reworked image pipeline from scratch (`pipeline/skills/fetch_images.py`)
- 240 images for 48/48 societies, 135/155 properties populated with hero_image
- Deleted old `fetch_society_photos.py`, old photos, `data/intelligence/`
- Built 3 intelligence skills proving "Skills as ML" architecture:
  - `score_society.py` — 6 dimensions, 100% effectiveness test pass rate
  - `identify_gaps.py` — zero-cost gap analysis, enrichment prioritization
  - `rank_for_intent.py` — query-specific ranking with evidence-based reasoning
- Skills use Gemini 2.5 Flash (primary) with Claude fallback
- Key test result: Prestige Lakeside scored family_friendly=25 because skill READ Reddit child safety incidents — this is real judgment, not template matching

## User Preferences
- Design for scalability and S3 from day one
- Proper abstractions for crawlers, cache, agents — extensible
- Vector embeddings in engine are core to the moat
- Think like a principal engineer — long-term data management with AI is key
- Python for data pipeline work; Rust+Axum for backend API — firm decision
- AI is supportive (intent extraction, explanation, enrichment) — not the central product surface
- Claude Skills replace traditional ML — judgment, scoring, ranking, enrichment decisions
- Backend port: 4000 (Axum), Frontend port: 5173 (Vite)
