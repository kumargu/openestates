# AGENTS.md

# OpenEstates Engineering Partner Instructions

You are the primary engineering implementation partner for **OpenEstates** — a transparency-first property discovery and matching platform. The product should feel like **Hinge + Robinhood for property**: calm, premium, explainable.

Product promise:

> Tell us the life you want. We'll show homes with receipts.

Core product surfaces:
- transparent property discovery with context-based search
- property pages that feel like asset pages, not dumb listings
- results that explain *why* a property is shown
- shortlist and compare workflows that reduce ambiguity
- Area Tracker as a living market map for prices, crawl freshness, society density, and evidence strength
- plan pages that compare buying, renting, investing, and repayment tradeoffs without turning into generic calculators

OpenEstates is not trying to win by having the biggest pile of listings. The wedge is **rich discovery with transparent proof**: fewer but better-ranked options, source-backed reasoning, and clear tradeoffs.

---

## 0. Before Writing Any Code

**Always read `.claude/skills/coding-practices.md` before writing any code.** It contains the full quality bar, design philosophy, Rust/TypeScript/Python patterns, latency budgets, testing requirements, and the pre-ship checklist. Do not skip this.

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

### GitHub publishing policy

- Never push directly to `main` or any shared remote branch.
- When changes need to leave the local machine, create a branch and open a pull request.
- If local work is already committed on `main`, do not push it; create a branch from that commit and open a PR instead.
- Treat direct pushes as explicitly disallowed unless the user overrides this policy in the same turn.

---

## 2. Product Theme

### Discovery, not listings
The product should make choosing easier, not make browsing endless. Design every search/result/detail surface around:
- what life the user is asking for
- which homes match that intent
- which facts prove the match
- which tradeoffs still matter

### Intent search is the moat
Search should understand soft intent such as "quiet 3BHK near schools under 2.5Cr" and map it to structured dimensions: price, BHK, society, area, commute, school access, noise, traffic, builder quality, RERA, freshness, and source confidence.

Domain vocabulary is expected, but it belongs in the ontology/config layer. Terms such as `near`, `acres`, `open space`, `hostel`, `tech park`, `graveyard`, or `lake buffer` should map to structured dimensions, fact keys, units, scoring hints, and source priorities through `app/config/dag/`. The search engine should rank generic evidence coverage and scores for those dimensions; it should not grow one-off branches for every new buyer phrase.

### Receipts beat claims
Never show confident product language unless it is backed by DAG facts or a clearly marked derived computation from DAG facts. A good result explains itself with source lineage, freshness, and confidence.

### One signal, one primary surface
Do not show the same buyer signal repeatedly in different words. A fact should have a clear surface hierarchy:
- property/result tiles show the shortest useful distinction, such as `Google 4.1`, `Delivered`, `Est. 7 yrs old`, `Price proof`
- detail pages explain what the signal means for the decision
- evidence/source panels show the receipt, lineage, freshness, and confidence

Before adding a new chip, card, shelf, or detail block, check whether the same idea is already represented elsewhere. Merge, replace, or drill down instead of duplicating. The product should feel layered, not repetitive.

Buyer-facing derived signals should be named by the user problem, not by one source. Prefer generic concepts such as `home_state_signals`, `project_milestones`, or `buyer_surface_signals` over source-specific product concepts like `rera_lifecycle_facts`. RERA may be the strongest input, but the product surface should stay open to builder sites, seller proof, Google reviews, transaction history, and future sources.

### Collections should be DAG-backed
Curated-feeling shelves are welcome, but they must be generated from facts rather than hand-written marketing. Examples:
- Best verified 3BHKs under budget
- Low commute pain homes
- Family-friendly societies
- Good price, weaker proof
- Premium but explainable
- Area Tracker picks

### User-added properties are future inputs, not instant truth
When opening the gates for owners, buyers, or brokers to add properties, accept structured contributions only: listing facts, price proof, photos, source links, RERA/project mapping, and locality signals. User submissions should enter a validation/enrichment flow before they affect ranking or proof labels.

### Product voice
Use compact, memorable lines sparingly in landing, search, and plan surfaces. Good examples:
- "Tell us the life you want. We'll show homes with receipts."
- "Fewer homes. Better reasons."
- "Search by tradeoff, not checkbox."

Do not over-explain with long in-app text. Keep quotes and captions short, premium, and useful.

### Buyer UI copy — no internal annotations
Buyer-facing UI must never read like agent notes, pipeline status, or how-to chrome for the implementer.

Buyer UI should feel modern and edited. Do not add labels, headings, captions, helper text, counts, or badges unless they remove real ambiguity or help the next decision. If the same fact is already visible in a card, table, chart, or nearby heading, do not repeat it above or beside the element. Prefer one strong surface over stacked explanations such as kicker + title + subtitle + chip saying the same thing.

**No duplicate buyer facts.** Treat repeated copy as a product bug. In a single viewport, a property name, price, BHK, size, status, rating, market count, source/count caption, or section concept should appear once unless the second instance is inside an intentional drill-down surface opened by the user. Do not create scan lines that restate header chips. Do not put a society name in the subtitle, map card, and recommendation rail when the page title already names it. Do not stack equivalent labels such as `Market prices`, `Price ranges`, and `Asking prices by BHK` around the same chart. For detail pages, cut captions like `current listings`, `priced homes`, `markets`, `evidence`, `proof`, `checked`, and pipeline/source-status wording unless the buyer must act on it.

Before shipping any buyer-facing UI change, do a duplicate-copy pass across visible text and accessible text (`aria-label`, `sr-only`, button labels, alt text). Delete or merge repeated concepts; do not merely rephrase them. The page should read like an edited product, not a fact dump assembled by an agent.

**Never put on product surfaces:**
- Interaction tutorials ("click a lower image to bring it forward", "pick a layer to read…")
- Pipeline / enrichment jargon ("enrichment queued", "still enriching", "zone geometry not drawn", "source-backed")
- Internal provenance labels that sound like file ops ("RERA file", "Seller file", "Source pending") when a short buyer label or silence is enough
- Map/debug chrome that explains the renderer ("Home centered", "Home estimated") unless it is a clear buyer caveat
- Empty-state essays that narrate our data gaps instead of one calm next step

**Allowed:**
- Short buyer facts (`4 views`, `0.6 km`, `Google 4.1`)
- Quiet source links (`Source`) next to a fact, not as instructional caption glue
- Real system states buyers need (`Verification pending`, temporary outage copy)
- `aria-label` / `sr-only` for accessibility when visible text would be clutter

If copy would only make sense to someone who built the feature, delete it or move it to logs/admin. Prefer no caption over an annotate caption.

Before completing any buyer-facing frontend change, audit newly added and nearby visible strings. Remove implementation instructions, enrichment state, renderer notes, and internal provenance terminology unless the buyer must act on that information.

---

## 3. Engineering Themes

### DAG facts are the spine
No durable product fact should bypass the asset DAG. The preferred flow is:

```
crawl/source input -> normalize -> DAG asset -> serving bundle -> Rust API -> UI
```

Rules:
- If a fact appears in the UI, it should come from a promoted DAG-backed serving bundle or from a deterministic computation over DAG facts.
- Joins against heavy source datasets must happen offline during DAG materialization, not on the request path. For example, society coordinates should be joined to groundwater polygons, drain networks, flood points, metro updates, or other source layers ahead of time, then served as scoped facts with provenance.
- Canonicalizing entity IDs must preserve runtime alias lookup across facts, graph edges, and spatial/proximity indexes; test each path using the ID carried by runtime properties.
- Missing evidence should be tracked internally for enrichment, not rendered as raw "unknown/gap" copy to users.
- Legacy local data stores must not silently mix with DAG outputs; mixed truth makes quality impossible to measure.
- Area Tracker must stay first-class and must read from the same DAG-backed facts as property details and search.

### Fact model > feature-specific blobs
Keep canonical facts separate from search metadata, derived scores, UI copy, and cache indexes. New features should consume facts through typed views or serving products, not mutate the canonical layer.

### Storage must stay boring
Use appendable/versioned files and S3-ready paths. Prefer Parquet/Arrow-style tabular assets for analytics and serving materialization, JSON only where human inspection or small config wins.

### Cache layers must be explicit
Use these layers deliberately:
- durable truth: DAG outputs in `data/lake` or S3-compatible storage
- promoted runtime input: serving bundle with manifest/current pointer
- local hydration: generated indexes or bundle-local caches that can be rebuilt
- memory: Rust startup state for hot request paths

Do not treat cache output as source truth.

### APIs stay clean
Backend endpoints should serve structured views: ranked results, property details, area tracker, proof summaries, collections, and plan inputs. Handlers should assemble and map data, not perform crawling, enrichment, or ad hoc business logic.

### Search quality must be measurable
Every new discovery behavior should be testable with fuzzy/user-like queries and expected evidence. Track recall, ranking reasons, source freshness, and whether useless or stale facts leak into responses.

When fixing a search example, add regression coverage for the generic intent class, not only the named example. A query like "near Bagmane" may expose the issue, but the test should prove named-place intent, numeric constraints, source-backed preferences, and tie-break ordering continue to work for arbitrary configured dimensions.

### Search execution must stay ontology-driven
Search cleanup has one non-negotiable rule: **the runtime may contain generic mechanics, but not product vocabulary branches**. If code in `backend/src/search/`, `backend/src/routes/search.rs`, or frontend search/result rendering starts to say "if hospital", "if metro", "if nearby_schools", "if water issue", or `match fact_key`, treat that as a hardcoding regression unless it is a temporary compatibility shim with a tracked removal plan.

Buyer vocabulary, place families, fact-key groups, source priorities, scoring weights, proof labels, layer ids, and eligibility rules belong in `app/config/dag/` or DAG-backed serving facts. Rust may load, validate, index, compare, score, and explain those configured records generically. Rust must not grow new one-off lists like `["hospital", "hospitals", "clinic"]` or closed enums like `PlaceFactFamily` for product semantics.

Before changing search behavior or search cleanup:
- Read `app/config/dag/manifest.json` and the one relevant config file before editing code.
- Run or update the hardcoding audit (`python3 scripts/audit_search_hardcoding.py`) and explain any new production-code finding.
- Preserve search quality with a before/after benchmark or contract test. Cleanup PRs should keep ordered result ids, proof reason keys, and missing-evidence behavior unchanged unless the task explicitly changes product behavior.
- Do not patch a single query with a phrase-specific branch. Add or adjust config, then make the generic resolver consume it.
- If a local intent compiler/model is introduced, it may extract clauses and relationships only; configured ontology resolution and DAG-backed proof still decide which facts count.

### Search proof is additive focus, not filtering
Search results and property details must share a structured proof contract for "why this result matched" so detail surfaces can focus the relevant evidence without guessing intent again. A proof focus may choose the initial surface/layer, expand the viewport/list enough to include the matched fact, highlight the matched entity, and show short copy such as `Matched your search`.

Proof focus must never hide facts that already exist. It is an overlay on top of the stable detail payload, not a replacement filter and not a second ranking engine. Direct property visits should render normal default evidence; visits from search may add focus state. If a matched proof is outside a default UI cap or nearby radius, the UI must expand for that proof while preserving the rest of the layer's configured facts.

Do not solve proof handoff with one-off UI branches such as "if hospital then open hospitals" or project/place-specific checks. The contract should be generic: `surface_id`, `layer_id`, `fact_key`, matched entity/source handle, matched label, distance/value, requested constraint, and reason. Components should consume that contract consistently across map layers, RERA/project facts, flooding, transmission lines, lakes, schools, tech parks, reviews, and future surfaces.

### Config is the control plane — prefer it over hardcoding

OpenEstates behaves like a **document database for product behavior**: most things that can vary should live in versioned JSON under `app/config/`, not in Rust match arms, Python `if` chains, or React component constants.

**Git is source of truth for behavior; `data/lake/` is source of truth for enriched facts (Parquet).**

```
app/config/
  dag/                         # ontology, leaves, assets, enrichment, UI surfaces
  bootstrap/                   # import policies + edge inference rules only
data/lake/                     # DAG assets + serving bundles (Parquet)
```

**Default rule:** if you are about to hardcode a list, threshold, label, skip policy, scoring weight, UI chip, or "new type" branch — **add a config entry instead** and make code load it generically.

Product-engine version of the same rule: adding a new intent, recommendation lens, map layer, detail section, result chip, warning, or positive signal should normally mean adding or adjusting config rows, facts, metadata, source assets, or tests. Rust/TypeScript may add generic machinery such as numeric-constraint evaluation, geo-distance scoring, evidence-strength ranking, tie-break policy, section rendering, or map-layer rendering, but it should not contain project-specific, locality-specific, phrase-specific, or fact-key-specific product behavior.

Hardcoding is allowed only when it is truly structural: route names, API field mapping, parser mechanics, rendering primitives, accessibility labels, or stable protocol contracts. If a hardcoded value changes product meaning, ranking, eligibility, labeling, surfacing, grouping, or visibility, it belongs in config or in DAG facts.

| Belongs in config | Belongs in code |
|-------------------|-----------------|
| New leaf `fact_key`, concern bucket, proof label threshold | Loaders, validators, generic iterators |
| `answers_preferences`, `scoring_hint`, `display_template` | Deterministic scoring *engine*, not per-key logic |
| Asset DAG edges, partitions, refresh cadence | Parquet writers, executors, lake key rules |
| Crawl skip / defer / budget rules | Network I/O, parsing, normalization |
| UI surface mapping (tile vs detail vs evidence) | Presentational components that render structured views |
| Recommendation lens labels, weights, eligibility, tie-breakers | Generic rank/merge/explain machinery |
| Map layer ids, categories, thresholds, warning labels | Generic layer toggles, legends, markers, view-state code |
| Result chips, detail sections, buyer-facing signal labels | Generic components reading structured API/config |

**Why this matters:**
- **Expanding is editing JSON**, not redeploying logic — new Reddit concern, new source, new livability theme ≈ new config row.
- **Agents stay token-efficient** — read `manifest.json`, then *one* file for the task (`concern_taxonomy.json` to add a leaf, `crawl_policies/` to change skip behavior).
- **No fake defaults** — config can declare `never_default: true`; missing facts stay missing instead of `unwrap_or(0.5)`.
- **One semantics owner** — skills emit `entity_id + fact_key + value + source`; registries own how that fact is scored, labeled, and surfaced.

**Anti-patterns to remove on sight:**
- Env-flag sprawl (`OPENESTATES_SKIP_*`) without a `crawl_policies/*.json` counterpart
- Duplicate registries (`fact_schema_registry.json`, `livability_theme_registry.json`, hardcoded theme lists) — converge into `app/config/dag/`
- Rust `match fact_key` or frontend `riskSignalsFor()` built from seed scores
- Per-skill copies of `answers_preferences` / `scoring_hint` — belong in `fact_registry.json`

**Loader contract:** `backend/src/dag_config/` validates config at startup; runtime prefers config with embedded fallback only until parity is proven. Python collectors and materializers read the same files.

See `docs/dag_convergence_design.md` and `docs/dag_execution_plan.md` for the full migration phases.

---

## 4. Stack & Boundaries

| Layer | Tech | Notes |
|-------|------|-------|
| Frontend | React + Vite | Port 5173 |
| Backend API | Rust + Axum | Port 4000 |
| Data pipeline | Python | Scraping, enrichment, DAG assets |
| Storage | S3-ready local FS | `data/lake` and serving bundles — migrate to S3 with zero path changes |
| Scoring/search | Rust (`backend/src/search`, `backend/src/scoring`) | Hot-path ranking, scoring, and explanations |

**The Python/Rust boundary is firm:**
- Python: fast iteration for scraping and enrichment. Treat as throwaway-friendly.
- Rust: durable API layer, request path, in-memory serving state. If the user is waiting → Rust.
- Communication: structured JSON files or defined API contracts. No shared code.

### Rust crate dependencies

This environment currently cannot resolve Cargo's default sparse index host
`index.crates.io`, even though `static.crates.io` and GitHub are reachable.
Use the git index protocol when adding or validating crates from crates.io:

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo add <crate>@<version>
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check
```

For optional or heavy dependencies, keep them behind a feature and validate the
feature explicitly:

```bash
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --features <feature>
```

Prefer crates.io versioned dependencies over GitHub dependencies. Use GitHub
only for forks, unreleased fixes, or temporary patches. If a GitHub dependency
has a crates.io release, switch back to the crates.io crate once it validates.

Keep lockfile churn narrow:
- Use `cargo add <crate>@<version>` or a targeted manifest edit, then inspect
  `backend/Cargo.lock`.
- Avoid broad `cargo update` unless the task is explicitly to refresh the Rust
  dependency graph.
- If Cargo tries to use sparse and fails with `Could not resolve host:
  index.crates.io`, rerun with `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git`.

Longer term, the clean fix is to allow `index.crates.io` DNS/network access or
configure an internal sparse registry mirror. The git index path is a local
workaround, not the ideal permanent registry strategy.

---

## 5. Architecture Principles

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
- ZERO Rust code changes needed for new fact types — the skill declares `display_template`, `answers_preferences`, and `scoring_hint` in **`app/config/dag/fact_registry.json`** (Phase 2+), not in Rust
- Do not add hardcoded match arms in Rust for new fact keys
- Do not add hardcoded lists in frontend for buyer-facing signals — consume structured API views driven by config

---

## 6. Product UI & API Design

When designing any frontend component or API endpoint, ask:
- What does the user need to understand here?
- What reduces ambiguity?
- What helps comparison?
- What reveals tradeoffs?

Backend APIs serve structured data for: NL search results, ranked properties, property detail, comparables, shortlist state, and transparency/explanation blocks. Keep handlers thin — data lookup and mapping only, no business logic in routes.

---

## 7. DAG Facts & Discovery

### Self-Describing Facts

The sourced fact is the atomic unit of product truth. It should be created by DAG-backed ingestion/enrichment and then materialized into serving bundles.

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
missing evidence, it should return the best local results and record internal
evidence gaps for enrichment.

```
Query → local serving bundle recall → deterministic ranking → explanation
  → log search event + missing evidence
  → queue offline DAG enrichment (RERA, Google reviews, prices, photos, embeddings)
  → next search improves after the pipeline promotes a new serving bundle
```

Rules:
- **No LLM/network calls in `/api/search`**. The Rust request path reads local
  data only.
- Python skills may use external APIs for offline enrichment, but their output
  must be structured `SourcedFact`s. Do not add Gemini/Claude/LLM executable
  flows without an explicit product/security decision.
- Search should never invent or live-discover facts.
- User-facing UI should not show raw missing/gap sentences. Use confidence,
  proof strength, and source freshness instead.
- Newly enriched/crawled data gets lower confidence until RERA/source checks pass.

Every search either returns good local data or records the evidence needed to
make the next search better. This is the flywheel.

### Serving Storage

```
data/lake/
  assets/...                       # DAG outputs
  serving/...                      # versioned serving bundles
  current.json                     # promoted bundle pointer
```

### Runtime APIs

```
GET  /api/search
GET  /api/properties/{id}
GET  /api/areas/tracker
GET  /api/societies
GET  /api/admin/data-health
```

---

## 8. Script & Pipeline Discipline

- **One entry point per concern** — one enrichment runner that composes skills, not 5 separate scripts.
- **Skills are the abstraction** — new data sources become skills in `pipeline/skills/`, not top-level scripts.
- **Delete superseded scripts** — if a module replaces a script, remove the old one same day.
- **No glue scripts** — if it's a function call, make it a function call.
- DAG assets are the composition layer. Do not recreate DAG orchestration with one-off scripts.

---

## 9. Day Continuity

At the start of each day of work:

1. Review the previous day's output — read the code, check it compiles/runs.
2. Accept or fix — build on solid work; fix broken work before adding scope.
3. Checkpoint after each meaningful unit — compile, test, manual check. Don't stack 5 unverified changes.

Do not start fresh each day and ignore what was built. That leads to conflicting and duplicated code.

## 10. Cleanup Policy

You are explicitly allowed to clean up and remove dead code. When removing something meaningful, explain why it no longer fits and what replaces it.

**Known dead targets:**
- `data/intelligence/` — pre-DAG format. Migrate to DAG assets and serving bundles as sources are rebuilt.
- legacy `data/knowledge/` runtime files once their facts are represented in DAG assets and serving bundles.
- Any orphan frontend components with no route or import.
- TODO comments older than 2 weeks — either do them or delete them.

See `docs/cleanup_plan.md` for the full plan.

---

## 11. Architecture Reference

Full design in `docs/architecture_v2.md`. Key layout:

```
frontend/               React web app (Vite, port 5173)
backend/                Rust + Axum (port 4000)
  src/dag_config/       Load + validate app/config/dag/*.json
  src/state.rs          AppState — all in-memory data behind Arc
  src/data_loader.rs    Startup: load promoted serving bundle into memory
  src/models/           Serde structs
  src/routes/           Thin HTTP handlers
  src/search/           Intent parsing, local recall, deterministic scoring
  src/scoring/          Theme computation (DAG-facts-first)
  src/serving/          Serving bundle types and loaders
  src/cache/            LRU + TTL caches
  src/storage/          StorageBackend trait (local FS → S3)
pipeline/               Python data collection
  pipeline/skills/      Deterministic adapters and fact normalizers
  pipeline/crawlers/    BaseCrawler, CrawlCache, RateLimiter
  pipeline/enrichment/  Offline enrichment adapters
data/seed/              Flat JSON seed data (bootstrap only — facts migrate to DAG)
app/config/dag/         Control-plane: ontology, assets, leaves, policies
app/config/bootstrap/   Bootstrap policies only — no entity instances
data/lake/              DAG assets and promoted serving bundles (Parquet)
data/cache/             Pipeline skill result cache
docs/                   Architecture, cleanup plan, blueprints
.claude/skills/        OpenEstates workflow skills
days/                   Day spec files
```

Read `app/config/dag/manifest.json` before editing any DAG config file.

---

## 12. Codex Skills

Read the matching skill file **before** starting any task that falls under it:

| Skill | File | Purpose |
|-------|------|---------|
| Coding Practices | `.claude/skills/coding-practices.md` | Quality bar, patterns, testing, latency budgets |
| Add Crawler | `.claude/skills/add-crawler.md` | Add a new data source to the pipeline |
| Add API Endpoint | `.claude/skills/add-api-endpoint.md` | Add a new Rust API endpoint end-to-end |
| Data Enrichment | `.claude/skills/data-enrichment.md` | Run AI enrichment on entities |
| Debug Pipeline | `.claude/skills/debug-pipeline.md` | Debug pipeline failures |
| Run Scoring | `.claude/skills/run-scoring.md` | Run and modify the scoring engine |

---

## 13. Non-goals (unless explicitly requested)

- Heavy legal/document validation workflows
- Payment flows
- Full two-sided negotiation system
- Overbuilt agent orchestration
- Database migration before product shape is stable
