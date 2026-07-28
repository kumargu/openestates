# OpenEstates Local Memory

Last refreshed: 2026-07-26

## Product Direction

OpenEstates is a transparency-first property discovery and matching platform for Bengaluru real estate.

Product feel: calm, premium, explainable. The shorthand is "Hinge + Robinhood for property".

Core promise: "Tell us the life you want. We'll show homes with receipts."

The product does not compete on listing volume. It competes on fewer, better-ranked homes, clear tradeoffs, and source-backed reasoning. Search and detail pages must explain why a home appears, what proves the claim, and what caveats remain.

## Current Stack

- Frontend: React 19 + Vite, port 5173.
- Backend: Rust + Axum, port 4000.
- Data pipeline: Python scripts and skills for offline source collection and fact production.
- Runtime storage: S3-ready local lake under `data/lake/`, with promoted serving bundles.
- Config control plane: JSON under `app/config/`.
- Search: Rust hot path using local structured recall, Tantivy recall, semantic index, geo recall, deterministic ranking, and diagnostics.

## Current Architecture

The active architecture is DAG/config/serving-bundle first. Older memory about `engine/`, day specs, `agents/`, `simulation/`, or flat `data/seed` as the main runtime source is stale.

Preferred data flow:

```text
source input -> Python source collector/skill -> Rust asset DAG -> data/lake artifacts
  -> promoted search serving bundle -> Rust AppState -> API -> React UI
```

Runtime startup is strict:

- `backend/src/main.rs` builds routes and calls `data_loader::load_app_state`.
- `backend/src/data_loader.rs` requires a promoted serving bundle and refuses to silently fall back to legacy `data/knowledge`.
- `backend/src/serving/loader.rs` loads current bundle materialization, Parquet tables, Tantivy index, graph index, geo index, fact index, and optional semantic embeddings.
- `backend/src/state.rs` owns shared runtime state behind `Arc` and `RwLock`.

## Source Of Truth

- `app/config/` is source of truth for behavior: schemas, policies, registries, product surface mapping, crawl policies, scoring weights, labels, thresholds, and runtime defaults.
- `data/lake/` is source of truth for enriched facts and serving artifacts.
- `data/cache/` and `tmp/` are rebuildable, not source truth.
- Legacy `data/knowledge/` is import or migration input only. Do not add new request-path dependence on it.

Before editing DAG/config, read:

1. `app/config/manifest.json`
2. `app/config/dag/manifest.json`
3. The one specific config file for the task

Routing:

- Add fact/leaf: `app/config/dag/concern_taxonomy.json` and `app/config/dag/fact_registry.json`
- Add source: `app/config/dag/source_adapters/` and `resolution_policies.json`
- Add crawl asset: `asset_registry.json` and `crawl_policies/`
- Add UI surface mapping: `ui_surfaces.json`
- Product homepage/evidence layout: `app/config/product/`
- Lake path/sync contract: `app/config/lake/`

## Backend Development Rules

Rust owns request-path work. If the user is waiting, it belongs in Rust.

Important modules:

- `backend/src/routes/`: thin HTTP handlers only.
- `backend/src/search/`: intent parsing, recall, geo/semantic/text scoring, ranking, diagnostics.
- `backend/src/serving/`: serving bundle schema, Parquet reads, Tantivy hydration, fact index.
- `backend/src/assets/`: asset DAG planner, executor, materializers, run manifests, lake writes.
- `backend/src/dag_config/`: config loaders and validators.
- `backend/src/models/`: serde API/domain structs.
- `backend/src/scoring/`: deterministic scoring and policy.
- `backend/src/recommendations/`: related homes and path alternatives.
- `backend/src/lake/`: local/S3-ready lake abstraction and keys.

Rules:

- No LLM or network calls in `/api/search`.
- Request path reads promoted local serving data only.
- Handlers assemble response shapes; durable logic goes in domain/search/serving modules.
- API structs should use `serde(rename_all = "camelCase")` where frontend consumes them.
- Missing API keys or optional embedders must degrade gracefully.
- Avoid `unwrap()` outside startup contracts and tests.
- Keep new fact types config-driven; do not add hardcoded `match fact_key` branches unless there is no generic route.

Main runtime routes include:

- `GET /api/health`
- `GET /api/properties`
- `GET /api/properties/{id}`
- `GET /api/properties/{id}/evidence`
- `GET /api/properties/{id}/recommendations`
- `POST /api/properties/evidence/batch`
- `GET /api/areas`
- `GET /api/areas/tracker`
- `GET /api/discovery`
- `GET /api/search?q=...`
- `GET /api/societies/search?q=...`
- `GET /api/admin/data-health`
- `POST /api/admin/asset-runs`

## Frontend Development Rules

The frontend is the product surface. It should feel like a premium, quiet decision workspace, not a marketing site or generic listing app.

Important paths:

- `frontend/src/main.tsx`: routes and shell.
- `frontend/src/components/workspace/`: app frame and navigation.
- `frontend/src/pages/`: home/search, property detail, home plan, compare.
- `frontend/src/components/evidence/`: proof and evidence surfaces.
- `frontend/src/components/property/`: detail-specific property components.
- `frontend/src/features/home-plan/`: buy/rent/invest/payoff planning surfaces.
- `frontend/src/lib/api.ts`: API boundary. Components should not call `fetch` directly.
- `frontend/src/lib/types.ts`: API/client types.
- `frontend/src/styles/`: evidence, property scene, workspace CSS.

Rules:

- Buyer-facing copy must never sound like pipeline/debug/agent notes.
- Do not show raw "missing evidence", "still enriching", renderer notes, or internal provenance unless the buyer needs to act on it.
- Prefer compact buyer facts, confidence, source freshness, and source links.
- Do not duplicate the same signal across chips/cards/sections. Pick a hierarchy.
- Keep API calls in `lib/api.ts`; keep components declarative.
- Use loading/error/empty states for every user-visible async surface.
- Preserve route-level lazy loading and the `WorkspaceFrame`.

Current frontend routes:

- `/`
- `/results` redirects to `/`
- `/property/:id`
- `/property/:id/plan`
- `/compare`

## Pipeline And Asset DAG

Python is for offline data/source work. Rust owns durable materialization, lineage, and runtime serving.

Important paths:

- `pipeline/collect_asset_sources.py`: source-input bridge called by Rust asset DAG.
- `pipeline/skills/base.py`: `SourcedFact`, `FactSource`, `SkillResult`, cache/cost/retry abstraction.
- `pipeline/skills/`: source adapters and fact producers.
- `pipeline/sources/`: external source collectors.
- `backend/src/bin/openestates-plan-assets.rs`
- `backend/src/bin/openestates-run-assets.rs`
- `backend/src/bin/openestates-compact-lake.rs`
- `backend/src/bin/openestates-build-semantic-embeddings.rs`

Rules:

- New data source should usually become a skill or source adapter, not a random top-level script.
- Skills emit structured `SourcedFact`s with provenance, confidence, display/search metadata, and cost where relevant.
- Durable artifacts are lake outputs with manifests and current pointers.
- One-off scripts should not persist unless they become tests or first-class commands.
- External API work belongs offline unless explicitly designed as timeout-bounded optional work.

## Config-Driven Development

Default rule: if adding a list, threshold, label, source mapping, display template, scoring weight, skip policy, or new fact category, add or extend config first.

Good places for variation:

- `app/config/dag/fact_registry.json`: fact semantics, display templates, scoring hints.
- `app/config/dag/concern_taxonomy.json`: buyer concern/leaf taxonomy.
- `app/config/dag/scoring_policy.json`: scoring policy.
- `app/config/dag/search_intent.json`: intent mapping.
- `app/config/dag/ui_surfaces.json`: where facts surface.
- `app/config/dag/evidence_sections.json`: property evidence layout.

Bad patterns:

- Hardcoded fact-key lists in Rust or React.
- Source-specific buyer-facing names when the signal should be source-agnostic.
- Request-time joins against heavy source datasets.
- Cache output treated as truth.

## Local Commands

Run both services:

```bash
./dev.sh
```

Backend:

```bash
cd backend
cargo check
cargo test
cargo run
```

If Cargo sparse index DNS fails, use:

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test
```

Frontend:

```bash
cd frontend
npm run build
npm run test
npm run dev
```

Smoke tests:

```bash
./tests/smoke_test.sh
```

Asset DAG examples:

```bash
cd backend
cargo run --bin openestates-plan-assets
cargo run --bin openestates-run-assets
cargo run --bin openestates-profile-serving-bundle
```

## Verification Expectations

- Backend changes: `cargo check`, targeted `cargo test`, and smoke tests when routes/API/runtime are touched.
- Frontend changes: `npm run build`; run frontend tests if changing logic under `frontend/tests`.
- Search changes: run search unit/contract tests and use `/api/search?q=...&debug=1` for diagnostics.
- Pipeline changes: run the specific `pipeline/test_*.py` or direct module command, then inspect emitted facts.
- Config changes: verify startup can load config and run tests that exercise the affected registry/policy.

## Git And Publishing State

Remote:

```text
origin https://github.com/kumargu/openestates.git
```

This local checkout at `/Users/gulshan.kumar/openestates` is the full detailed history copied from `arca.ssh`.

As of 2026-07-26:

- Local `main` has 144 detailed commits beyond the old GitHub state.
- GitHub `main` was updated with one sanitized commit: `418f523 Sync OpenEstates code without generated data`.
- That sanitized commit excluded `data/`, `backend/data/`, and `tmp/`, used `Gulshan <71965388+kumargu@users.noreply.github.com>`, and normalized Databricks npm-proxy lockfile URLs to public npm registry URLs.
- A separate sanitized worktree exists at `/Users/gulshan.kumar/openestates-push-no-data`.
- Do not push the original 144-commit `main` to GitHub unless explicitly requested and after re-checking data/secrets/history.

For future personal commits, use repo-local identity:

```bash
git config user.name "Gulshan"
git config user.email "71965388+kumargu@users.noreply.github.com"
```

## Secrets And Data Hygiene

- `.env.local` exists locally and must not be committed.
- Do not commit `data/`, `backend/data/`, `tmp/`, `.venv/`, caches, or generated lake artifacts unless explicitly requested.
- Run a staged grep for tokens/private keys/company-specific domains before pushing public code.
- Lockfiles should use public npm registry URLs, not internal proxy URLs, for public GitHub pushes.

Suggested staged scan:

```bash
git grep --cached -n -I -E '(AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|BEGIN (RSA|OPENSSH|DSA|EC|PRIVATE) KEY|github_pat_|ghp_[A-Za-z0-9_]{20,}|xox[baprs]-|dapi[a-f0-9]{32}|databricks\\.com|cloud\\.databricks\\.com)'
```

## How To Approach New Work

1. Read `AGENTS.md`.
2. Read `.claude/skills/coding-practices.md`.
3. Identify which layer owns the change: config, Rust runtime, React UI, Python pipeline, or DAG materialization.
4. Prefer config and generic loaders over hardcoding.
5. Keep request-path behavior local, deterministic, and fast.
6. Keep buyer UI copy calm and free of internal implementation language.
7. Verify with the smallest meaningful build/test/smoke command.
8. Update this memory when architecture or workflow changes.
