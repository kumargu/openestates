# Next Plan — Phases 6–10 (after 0–5 complete)

> **Status:** Phases 0–5 committed (`ab9e302`). This doc is the roadmap + Phase 6 implementation brief.  
> **Parent:** [`dag_execution_plan.md`](./dag_execution_plan.md)

---

## Where we are

| Phase | Status |
|-------|--------|
| 0–1 | ✅ `app/config/` control plane + `dag_config` loaders |
| 2 | ✅ `fact_registry` → `search_metadata` + search schema |
| 3 | ✅ Legacy seed → sourced facts; no fake score defaults |
| 4 | ✅ Property/area nodes, serving edges, `GraphIndex`, legacy search scoring deleted |
| 5 | ✅ Config-driven evidence UI; buyer-facing scores stripped; brief/pulse dedup |

**Foundation in place:** config owns semantics, bundle owns instances, Rust engine is thin, graph index exists (no summary API yet).

---

## Recommended sequence

```text
Phase 6   Reddit concern pipeline (data flywheel, issue #2)     ← START HERE
    ↓
Phase 8′  Discovery + search tiles + drill-down (UI remnants)  (parallel OK)
    ↓
Phase 9a  Road/place enrichment + visuals → lake                (unblocks graph copy)
    ↓
Phase 10  EntityContext API + generic graph summary UI         (API last)
    ↓
Phase 9b  New sources, S3 cutover, scale crawl                (ongoing)
```

**Do not start Phase 10** until Phase 9a puts facts on `road:*` / `place:*` nodes — otherwise graph traversal has nothing to say.

---

## Phase 6 — Reddit concern pipeline (primary next work)

**Goal:** Issue [#2](https://github.com/kumargu/openestates/issues/2) taxonomy operational. Derived signals only — no Reddit comment bodies in lake Parquet.

**Handoff:** implement per sections below. Estimated 7–10 days.

### 6.1 Config

| Task | File |
|------|------|
| Add `source_adapters/reddit_theme.json` | max confidence 0.45, signal_key → `fact_key` map from `concern_taxonomy.json` |
| Register in `dag/manifest.json` `includes` | alongside `legacy_seed.json` |
| Verify `crawl_policies/reddit_threads_daily.json` | disabled by default; skip rules documented |
| Cap in `resolution_policies.json` | `RedditTheme` loses to Google/RERA in resolver tests |

### 6.2 Pipeline

| Task | Notes |
|------|-------|
| Wire `reddit_resident_facts` skill → canonical `fact_key` only | No ad hoc output shapes; registry owns semantics |
| Classifier maps thread themes → `concern_taxonomy` signal keys | Reuse / extend `pipeline/skills/mine_reddit_intent_themes.py` or `search_reddit.py` |
| Emit `SourcedFact` rows: `entity_id`, `fact_key`, derived value, `source_type: RedditTheme`, `confidence ≤ 0.45` | No raw comment text in silver/gold/serving |
| Raw crawl → `raw/source=reddit/` only | Immutable snapshots; compliance boundary |

### 6.3 POC data

- Seed **10–20 societies** from issue #2 qualitative table as manual low-confidence facts (or one-off import asset)
- Verify they appear in bundle, lose in resolver to Google/RERA, visible in admin/data-health

### 6.4 Enrichment flywheel (minimal)

- On search with missing evidence, log `entity_id × fact_key` gaps (extend `preference_coverage` or `enrichment_gaps.json`)
- `enrichment_targets.json` already exists — wire **one** path: search demand → priority list for `openestates-enrich` (stub OK if CLI not ready)

### 6.5 Acceptance

- [ ] No Reddit comment bodies in lake Parquet (audit `raw/` vs `silver/` columns)
- [ ] Reddit facts use `concern_taxonomy` `fact_key`s only
- [ ] Resolver test: RedditTheme fact loses to Google/RERA on same key
- [ ] `cargo test` + `eval_search.py` no regression
- [ ] Crawl policy `enabled: false` until isolated worker — empty inputs path tested

### 6.6 Out of scope

- Enabling live Reddit crawl in production
- EntityContext API
- Buyer-facing proof tiers / scores

---

## Phase 8′ — Discovery & tiles (remnants after Phase 5)

Phase 5 absorbed most of Phase 8 (evidence sections, constellation, no `SECTION_CONSTELLATION`). **Remaining UI/config work:**

| Task | Where |
|------|-------|
| Search **match reason chips** from `fact_registry` / `ui_surfaces.json` — not hardcoded labels | `search/text.rs` → API; `LivingEvidenceTile` |
| `discovery_home.json` shelves: replace `proof_label` with receipt copy ("N societies · RERA + Google") | `discovery.rs`, `HomePage.tsx` |
| Livability brief `fact_keys` → link to evidence section drill-down | `LivabilityBriefCard` + API entity refs |
| **Compare** flow (if product wants it): same evidence model as property page | new route or extend shortlist — defer if not MVP |
| Add leaf in config → appears on **search tile** match reasons without React edit | acceptance test |

**Prerequisite:** Phase 6 helps tile chips have real Reddit-derived facts (optional for first PR).

---

## Phase 9a — Enrichment before graph UI (bridge to Phase 10)

**Goal:** Shared nodes (`road:*`, `place:*`) have facts so graph walks produce real copy.

| Task | Notes |
|------|-------|
| Enrich `road_segment` from approach road / geocoding | Facts on road, not duplicated per society |
| `place` nodes for schools, metro (Google nearby) | `maps_to_place` / future `near_place` |
| Migrate `data/product/approach_road_visuals.json` → lake media facts on `road_segment` | per `coverage.json` |
| Wire `enrichment_targets.json` → enrichment runner | one entry point, not scripts |
| Optional: `near_place` in `ontology.json` | if proximity without full place identity |

**Acceptance:**

- [ ] `GraphIndex::walk_out(society, served_by_road)` reaches road with ≥1 fact in bundle
- [ ] `coverage.json` graph gaps for road/place updated to done

---

## Phase 10 — EntityContext API (last)

**Contract:** `app/config/dag/entity_context.json`  
**Prerequisites:** Phase 4 edges ✅, Phase 9a road/place facts, Phase 8′ optional.

| Task | Notes |
|------|-------|
| `EntityContextComposer` in Rust | bounded `walk_out`, `ui_surfaces.json` hops baked at materialize |
| `clauses[]` + `summary_paragraph` | deterministic templates only — **no LLM on hot path** |
| `GET /api/entities/{id}/context`, `GET /api/properties/{id}/context` | serving bundle only |
| Generic React renderers by `presentation.variant` | property page consumes `summary` + `surfaces[]` |

**Explicitly not:** user-facing confidence or proof tiers in summary.

---

## Phase 9b — Scale & ops (ongoing)

- `registered_transactions` adapter stub → real
- S3 cutover (`OPENESTATES_LAKE_URL=s3://...`)
- `entity_selector` + crawl tier prioritization from `enrichment_gaps.json`
- Delete `data/intelligence/` when empty
- Remove embedded asset graph default in `registry.rs` when JSON parity proven

---

## Housekeeping (any phase)

| Item | Action |
|------|--------|
| `coverage.json` `graph_ui_readiness` | Update: edges + GraphIndex done; stale Phase 4 gaps |
| `dag_execution_plan.md` §6 workstreams | Mark A–D ✅; E = next |
| Delete legacy JSON in `data/search/`, `data/product/` | If still referenced only in `merged_from` metadata, remove files + update `build_dag_json.py` |
| `tmp/` | Do not commit |

---

## Suggested commit sequence (Phase 6)

```text
Commit 1  source_adapters/reddit_theme.json + resolution cap + manifest
Commit 2  Skill/classifier → concern_taxonomy fact_keys; no raw text in silver
Commit 3  POC society facts import + resolver tests (Reddit loses to Google)
Commit 4  enrichment_gaps logging from search + enrichment_targets stub wire
Commit 5  eval_search baseline refresh + issue #2 acceptance checklist
```

---

## Testing (Phase 6)

```bash
cd backend && cargo test
cd backend && cargo test reddit
python3 -m pipeline.eval_search

# Compliance audit — no comment bodies in facts parquet
# (adjust path to promoted bundle)
python3 -c "
import pyarrow.parquet as pq
from pathlib import Path
p = Path('data/lake/serving/search_bundle')  # or current bundle path
# scan facts for suspicious long text fields from reddit source_type
"
```

---

## Definition of done (Phase 6)

Reddit-derived **concern signals** flow through the same path as Google/RERA: taxonomy key → skill → silver facts → gold → serving → search/brief/evidence — with compliance boundaries enforced and resolver ranking Reddit below stronger sources.

**Then:** Phase 8′ (tiles/discovery) in parallel with Phase 9a (road/place enrichment) → Phase 10 graph summary API.
