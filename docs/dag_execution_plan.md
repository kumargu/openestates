# DAG Convergence — Execution Plan

> **Status:** approved direction — execute in phases  
> **Architecture:** see [`dag_convergence_design.md`](./dag_convergence_design.md)  
> **Issue context:** [Reddit concern taxonomy (#2)](https://github.com/kumargu/openestates/issues/2)

---

## 0. North star

One truth path for search, tiles, property page, and livability brief:

```text
config (ontology + fact leaves + sources + crawl policy)
  → DAG assets (crawl / derive / resolve)
  → KG entities + edges
  → serving bundle (Parquet + Tantivy, S3-ready keys)
  → Rust API + React UI
```

**Non-negotiables:**

- **No fake defaults** — missing facts stay missing (`never_default: true` for quality/risk leaves)
- **Confidence internal only** — users see proof labels + sources, not numeric scores
- **Leaves are additive** — new facts via config, not Rust match arms
- **Thin Rust engine** — Rust loops over config structs; it does not enumerate product semantics. New `fact_key`, preference label, evidence section, or area alias = JSON edit + bundle promote, **zero new `match` arms** for domain keys (see §0.1)
- **Storage unchanged in shape** — `LakeKey` paths, Parquet serving tables, versioned bundles, local FS → S3 with zero path rewrites
- **KG stays identity** — facts attach to entities; KG is not a second truth store

### 0.1 Thin Rust engine (config vs code)

**Goal:** Move product semantics out of Rust `match` / `if preference == "..."` blocks into `app/config/`. The runtime should be a small generic interpreter:

```text
config JSON  →  load at startup / bake into bundle  →  generic engine (lookup, score, render)
```

| Belongs in **config** | Belongs in **Rust** (stable engine only) |
|-----------------------|------------------------------------------|
| `fact_key`, preference labels, area aliases | Parquet load, edge index, bounded graph walk |
| `answers_preferences`, `scoring_hint`, thresholds | Scoring *algorithms* (`TextMatch`, `LowerIsBetter`) |
| Evidence section `kind`, `fact_keys[]`, presentation variant | Map `variant` → React component name |
| `display_template`, proof label thresholds | Template substitution `{value}` |
| Source priority tiers | Resolver tie-break logic |

**Anti-pattern (delete in Phase 4):** `match preference { "metro access" => ..., "quiet neighborhood" => ... }` or `match kind { "rera" => ... }` with dozens of arms.

**Allowed match arms:** value-type dispatch (`FactValue::Text` vs `Number`), scoring direction enum, HTTP/error handling — not per-leaf product vocabulary.

**Acceptance for convergence:** Adding a leaf in `concern_taxonomy.json` + `fact_registry.json` requires no Rust change and no frontend const map.

---

## 1. Storage & S3 constraints (must remain intact)

These are already correct. Every phase must preserve them.

### 1.1 Lake object model

| Principle | Current implementation | Rule |
|-----------|------------------------|------|
| Logical keys | `LakeKey` relative paths | Never embed `data/lake/` or bucket name in artifact manifests |
| Backend swap | `OPENESTATES_LAKE_URL` → local or `s3://bucket/prefix` | Same keys on both backends |
| Asset paths | `assets/paths.rs`, `serving/search_bundle/version=.../` | Partitioned, appendable, hash in manifest |
| Raw vs silver | `raw/source=...`, `silver/facts/entity_type=...` | Keep layered layout |

### 1.2 Parquet serving bundle (hot path)

Current tables (format v2):

- `entities/part-00000.parquet` — entity_id, entity_type, name, root_source, searchable_text
- `facts/part-00000.parquet` — entity_id, fact_key, typed value columns, confidence, source_*, learned_at
- `search_metadata/part-00000.parquet` — entity_id, fact_key, display_template, answers_preferences, scoring_*
- `edges/part-00000.parquet` — **planned Phase 4** — from_entity_id, edge_type, to_entity_id, confidence, source_type

**Storage optimizations to keep and extend:**

| Technique | Where | Why |
|-----------|-------|-----|
| ZSTD compression | `serving/parquet.rs`, asset writers | Smaller lake objects, faster S3 egress |
| Typed value columns | `value_text`, `value_number`, `value_tags`, … | Columnar pruning; no JSON blobs in facts table |
| List columns for metadata | `answers_preferences`, `scoring_thresholds` | Compact vs repeated rows |
| Entity-indexed facts | `ServingFactIndex.by_entity` | O(1) lookup at API startup |
| Tantivy sidecar | `tantivy_index/` in bundle | Text recall without scanning all facts |
| Versioned bundles | `version={slug}/manifest.json` + `current.json` pointer | Promote without mutating prior bundle |
| Local hydration cache | `data/cache/search_bundle/materialization=...` | Rebuildable; not source of truth |

**Do not:**

- Store raw Reddit comment text in serving Parquet (compliance + size)
- Add wide JSON fact blobs per row (breaks column pruning)
- Collapse all history into one mutable Parquet file (keep versioned materializations)
- Change serving table column names without `format_version` bump

### 1.3 Config storage

DAG config lives in git (`app/config/`), not the lake. Small, diffable, loaded at DAG plan time and baked into materialized bundles.

Optional later: snapshot config hash into serving `manifest.json` for reproducibility.

See [`app/config/coverage.json`](../app/config/coverage.json) for config vs lake vs Rust hardcoding audit.

---

## 2. Config package (single source of product semantics)

Layout: `app/config/` (see [`storage_and_config_layout.md`](./storage_and_config_layout.md)).

```text
app/config/
  manifest.json                 # root index
  coverage.json                 # audit + graph_ui_readiness
  dag/
    manifest.json
    ontology.json               # entity types + relations
    concern_taxonomy.json       # leaf definitions
    fact_registry.json          # leaf search/display semantics
    resolution_policies.json
    asset_registry.json
    enrichment_targets.json
    ui_surfaces.json            # surface → traversal + leaf_keys
    entity_context.json         # FUTURE API contract (Phase 10)
    crawl_policies/
    search_intent.json
  bootstrap/
    policy.json                 # importer rules → lake only
    edge_inference.json         # how to infer served_by_road, in_area, …
  lake/
    layout.json                 # Parquet zones + serving bundle schema
    sync.json
  product/
    discovery_home.json
    evidence_sections.json
  runtime/
    defaults.json
```

### 2.1 Merge map (eliminate duplicate registries)

| Current file | Fate |
|--------------|------|
| `data/search/fact_schema_registry.json` | Merged into `app/config/dag/fact_registry.json` |
| `data/product/livability_theme_registry.json` | Merged into `concern_taxonomy.json` + `fact_registry.json` |
| `backend/src/assets/registry.rs` | Exported to `asset_registry.json`; Rust loader in `dag_config/` |
| `data/product/buyer_context_sections.json` | Copied to `app/config/product/evidence_sections.json` |
| `data/product/approach_road_visuals.json` | Move to lake as enriched media facts on `road_segment` |

### 2.2 Leaf schema (every fact)

```json
{
  "fact_key": "operating.tanker_dependence",
  "value_types": ["bool", "tags", "text"],
  "entity_types": ["society", "area"],
  "display_template": "Tanker dependence: {value}",
  "answers_preferences": ["tanker", "water shortage"],
  "scoring_hint": { "direction": "LowerIsBetter", "weight": 1.5, "thresholds": [] },
  "resolution": {
    "strategy": "prefer_highest_confidence",
    "source_priority": ["google", "reddit_theme"],
    "never_default": true
  },
  "proof_label_thresholds": {
    "verified": 0.8,
    "supported": 0.6,
    "early_signal": 0.35
  },
  "ui": {
    "constellation": "risk",
    "section_kind": "operating",
    "brief_lens": "operating",
    "tile_eligible": false,
    "community_pulse_eligible": true
  },
  "enrichment_priority": "high"
}
```

---

## 3. Confidence policy (internal vs user-facing)

| Layer | Uses confidence | User sees |
|-------|-----------------|-----------|
| Resolver | pick winning leaf among sources | — |
| Search ranking | `evidence_is_confident_enough`, tie-break | — |
| Evidence section order | sort items by confidence | — |
| Enrichment queue | prioritize low-confidence gaps | — |
| Livability brief | aggregate → `confidence_label` | "Well supported" / "Early signals" |
| Tiles / side panel | fact count + proof heat | "42 facts · Supported" |
| SEO / JSON-LD | — | factual claims + URLs only |

**Remove over time:** raw `confidence_pct` bars, seed-derived risk scores, `TransparencyScore` as buyer-facing number.

---

## 4. Hardcoding cut list

Track and eliminate these as phases complete.

### 4.1 Backend (Rust)

| Location | What is hardcoded | Target |
|----------|-------------------|--------|
| `assets/registry.rs` | Full asset DAG | `app/config/dag/asset_registry.json` ✅ loader |
| `search/text.rs` | `legacy_preference_score`, `legacy_fact_key_for_preference` | Delete after bootstrap + eval |
| `routes/search.rs` | `legacy_preference_to_fact_key` | Delete |
| `search/intent.rs` | Preference → seed field key arrays | Read from `fact_registry.json` |
| `search/schema.rs` | `include_str!(fact_schema_registry.json)` | Load `app/config/dag/fact_registry.json` |
| `livability_brief.rs` | `include_str!(livability_theme_registry.json)` | Load `concern_taxonomy.json` |
| `data_loader.rs` | `.unwrap_or(0.5)` on seed scores | Hydrate from facts; null if missing |
| `routes/properties.rs` | Evidence section builders (`rera`, `reviews`, …) | Group facts by `ui.section_kind` from config |
| `routes/properties.rs` | `include_str!(approach_road_visuals.json)` | Reference fact_keys; visuals optional overlay |
| `scoring/transparency.rs` | Seed-field composite | DAG-backed confidence components |

### 4.2 Pipeline (Python)

| Location | What is hardcoded | Target |
|----------|-------------------|--------|
| `collect_asset_sources.py` | `OPENESTATES_SKIP_REDDIT` | `crawl_policies/reddit_threads_daily.json` ✅ |
| `skills/search_reddit.py` | Ad hoc output shape | Emit `concern_taxonomy` signal keys only |
| Asset source entity lists | Inline seeds | `data/dag/bootstrap/entity_seeds.json` |

### 4.3 Frontend (React)

| Location | What is hardcoded | Target |
|----------|-------------------|--------|
| `lib/evidence.ts` | `SECTION_CONSTELLATION`, `SECTION_DISPLAY_TITLES` | Backend sends `constellation` per section from config |
| `pages/PropertyPage.tsx` | `riskSignalsFor()` seed scores | Remove ✅; use `livability_brief` risk block |
| `pages/PropertyPage.tsx` | `buildDecision()` seed trust/risk | Brief confidence + RERA + market only ✅ |
| `lib/types.ts` | Seed score fields on `Property` | Keep only listing fields; quality via evidence |
| `components/evidence/EvidenceSectionCard.tsx` | Orphan | Delete ✅ |
| `main.tsx` meta | "transparency scores and tradeoffs" | Receipts/livability copy |

---

## 5. Execution phases

Each phase has **deliverables**, **acceptance criteria**, and **storage checks**.

---

### Phase 0 — Design lock & config scaffold ✅ (done)

**Goal:** Files exist; no runtime behavior change.

**Work:**

1. Create `app/config/` and directory layout
2. Draft `concern_taxonomy.json` from issue #2 + livability themes (78 leaves)
3. Draft `ontology.json` (7 entity types + 6 relations)
4. Draft `resolution_policies.json`, `ui_surfaces.json`, `enrichment_targets.json`
5. Document confidence → proof label mapping
6. `coverage.json` audit + `lake/layout.json` storage contract

**Acceptance:**

- [x] `app/config/` scaffold created
- [x] 78 leaves merged from livability themes + issue #2
- [x] `ui_surfaces.json` — 6 surfaces with traversal hops
- [x] Storage section in `lake/layout.json` matches serving schema
- [x] `entity_context.json` contract stub for future graph UI API
- [ ] Every issue #2 signal maps to a `fact_key` or is explicitly deferred

**Token-cost rule:** agents and humans edit **one config file per PR**; load via `manifest.json` routing only.

---

### Phase 1 — Config loaders, zero behavior change ✅ (mostly done)

**Goal:** Rust/Python load DAG config; existing tests pass.

**Work:**

1. Export `registry.rs` → `app/config/dag/asset_registry.json` ✅
2. Add `backend/src/dag_config/` module ✅
3. `openestates_registry()` with embedded fallback ✅
4. Reddit skip → `crawl_policies/reddit_threads_daily.json` ✅
5. Wire `discovery_home.json` + `evidence_sections.json` in routes ✅

**Acceptance:**

- [x] `dag_config` loaders for manifest, asset registry, crawl policies
- [ ] `cargo test` asset registry tests pass from JSON loader only (no embedded fallback)
- [ ] `openestates-run-assets --dry-run` produces identical plan
- [ ] Parquet output byte-identical for a fixed fixture run (or schema-compatible)

**Storage check:** No new lake tables; manifest keys unchanged.

---

### Phase 2 — Leaf registry drives search metadata (7–10 days)

**Goal:** `search_metadata` rows generated from `fact_registry.json`, not per-skill duplication.

**Work:**

1. Merge `fact_schema_registry.json` into `fact_registry.json`
2. Materializers read `answers_preferences` / `scoring_hint` from fact registry when writing `search_metadata`
3. Skills emit canonical `fact_key` only; registry owns search semantics
4. Add `eval_search.py` baseline snapshot before changes

**Acceptance:**

- [x] Search preference coverage report generated per run
- [x] Existing search tests pass
- [x] `eval_search.py` no regression on benchmark queries
- [x] `search_metadata` row count stable or explainable

**Storage check:** `search_metadata` Parquet schema unchanged; row content richer not wider.

---

### Phase 3 — Bootstrap cleanup (7–10 days)

**Goal:** Source-backed DAG facts replace local seed/bootstrap data; stop fake defaults.

**Work:**

1. Remove local seed JSON as a DAG/runtime input
2. Hydrate buyer-facing listing fields from source-backed DAG facts
3. Mark risk/quality leaves `internal_only: true` until supported by real evidence
4. Change `data_loader.rs`: hydrate `Property` listing fields from property facts; remove score defaults
5. Resolver applies `resolution_policies.json`

**Acceptance:**

- [x] No `unwrap_or(0.5)` on quality/risk fields in `data_loader.rs`
- [x] Coverage report shows source-backed fact coverage per entity
- [x] Property page no longer shows identical fake risk bars
- [x] Legacy seed facts are absent from admin/data-health and buyer proof

**Storage check:**

- No new seed/import asset; real crawlers and enrichment assets write standard Parquet facts
- Serving bundle `fact_count` increases; `entities` may add `property:*`

---

### Phase 4 — Entity expansion + edges + **legacy deletion** (10–14 days)

**Goal:** Serving bundle materializes the full graph **and** we delete superseded code paths. Phase 4 is not done until net lines go down — new graph support must come with removal of what it replaces.

**Prerequisite:** Phase 3 landed (bootstrap facts in lake, `data_loader` score defaults gone).

#### 4A — Graph materialization (build)

1. `canonical_society_nodes` from RERA-backed project identity
2. `road_segment` + `place` nodes where enrichment provides them
3. KG edges in gold from DAG assets: `society→area`, `society→road`, `society→place`
4. **Serving bundle includes `edges/part-00000.parquet`** (copied from gold `kg_society_view`)
5. Rust loads edges into `AppState` via `GraphIndex` (`edges_from` / `edges_to`)
6. Bounded `walk_out(anchor, hops, max_depth=2)` — unit tests only, no HTTP (Phase 10)
7. Search hard filters read property facts (BHK, price, area) from promoted serving facts
9. `entity_refs` on search results populated for all listing cards

#### 4B — Legacy deletion + **thin the Rust engine** (required, same phase)

Delete code that existed only because the graph and bundle were incomplete. **No new feature ships without deleting its replacement target.**

**Principle:** Every deleted `match` arm must be replaced by a config-driven loop, not a smaller hardcoded list.

| Delete (hardcoded semantics) | Location | Replaced by |
|------------------------------|----------|-------------|
| `legacy_preference_score` (~200-line `match preference`) | `search/text.rs` | `search_metadata` + generic preference scorer over `PreferencePatternSpec` |
| `legacy_fact_key_for_preference`, `format_legacy_display`, `legacy_negative_preference_evaluation` | `search/text.rs` | `fact_registry` `expanded_keys` + `display_template` |
| `local_fallback_allowed` + seed field reads | `search/text.rs` | Bundle facts on `property:*` / `society:*` |
| `legacy_preference_to_fact_key` | `routes/search.rs` | `fact_registry` preference patterns |
| `include_str!(fact_schema_registry.json)` fallback | `search/schema.rs` | `fact_registry.json` only |
| `include_str!(livability_theme_registry.json)` + theme `match` loops | `livability_brief.rs` | `concern_taxonomy.json` buckets + `fact_registry` |
| `AREA_ALIASES` const (~100+ lines) | `search/intent.rs` | `search_intent.json` `area_aliases` |
| `evidence_presentation_for_kind`, `evidence_section_priority`, per-`kind` fact key lists | `routes/properties.rs` | `app/config/product/evidence_sections.json` (already exists — wire it, delete matches) |
| Seed score fields on `Property` / `PropertyCard` | `models/property.rs`, frontend | DAG facts via serving bundle |
| Runtime reads of `data/knowledge/` | pipeline stragglers | lake gold KG |
| `data/search/fact_schema_registry.json`, `livability_theme_registry.json` | git | merged config |
| Embedded default asset graph (if parity proven) | `assets/registry.rs` | `asset_registry.json` only |
| `data/intelligence/` | git | lake serving bundle |

**What Rust should look like after Phase 4:**

```rust
// GOOD — generic engine
for pattern in registry().positive_preference_patterns() {
    if query_matches(&pattern.patterns, q) {
        score += score_facts_for_keys(&pattern.expanded_keys, &bundle, entity_id);
    }
}

// BAD — delete
match preference {
    "metro access" => if property.metro_distance_mins <= 10 { 2.0 } ...
}
```

**Phase 8 follow-up (not blocking Phase 4):** frontend `SECTION_CONSTELLATION` / display title maps → API fields from config.

**Deletion rules:**
- Run `eval_search.py` before and after each deletion batch; no regression vs baseline
- If a file is only kept for tests, migrate tests to bundle fixtures then delete
- Prefer one PR per deletion cluster (search legacy, registries, data dirs) — easier to bisect
- Update `app/config/coverage.json` as each item is removed

**Acceptance:**

- [x] Bundle entity counts: properties > 0, areas > 0, roads > 0 where inferred
- [x] Bundle includes edges table; row count documented
- [x] Search "3bhk whitefield under 2cr" uses property + society facts
- [x] Tiles show `entity_refs` and match reasons from leaves
- [x] Graph walk `society → served_by_road → road_segment` resolvable in unit test
- [x] **No `legacy_preference_score` or `local_fallback_allowed` in search hot path**
- [x] **`search/text.rs` has no `match preference` arms for product labels**
- [x] **`properties.rs` evidence sections driven by `evidence_sections.json`, not `match kind`**
- [x] **`data/search/fact_schema_registry.json` and `livability_theme_registry.json` deleted**
- [x] **`search/intent.rs` has no hardcoded `AREA_ALIASES`**
- [x] Net LOC reduction documented in PR (build + delete in same phase)

**Storage check:**

- `entities.parquet` + `edges.parquet` in serving bundle
- `format_version` bump if edges table added
- Tantivy index rebuild acceptable; keep in bundle version folder

**Not in Phase 4:** EntityContext HTTP API (Phase 10), Reddit pipeline (Phase 6).

---

### Phase 5 — UI truth consolidation (5–7 days) — done ✅

**Goal:** One signal, one surface; config-driven evidence sections.

**Work:**

1. Remove `riskSignalsFor` / `RiskBar` from `PropertyPage.tsx` ✅
2. De-duplicate theme chips: brief owns risk/operating prose; pulse owns review receipts ✅
3. Evidence sections grouped by `evidence_sections.json` (backend) — extend to `ui.section_kind` from fact registry ✅
4. Delete `EvidenceSectionCard.tsx` orphan ✅
5. Proof labels instead of confidence bars in evidence UI ✅
6. Update SEO meta copy ✅

**Acceptance:**

- [x] No seed-derived risk on property page
- [x] Livability brief is single risk surface in action rail
- [x] New `fact_key` with `ui.section_kind` appears without frontend code change
- [x] `tsc --noEmit` clean

**Not in this phase:** graph traversal summary API (Phase 10).

---

### Phase 6 — Reddit concern pipeline ✅

**Goal:** Issue #2 taxonomy operational; derived signals only.

**Delivered:**

1. `source_adapters/reddit_theme.json` — derived signals, max confidence 0.45, no raw text
2. `reddit_theme_classifier` + `reddit_resident_facts` → `concern_taxonomy` fact_keys
3. POC import (`reddit_poc_import`) loads 15 societies when crawl is skipped
4. Crawl policy disabled by default; empty-input path emits POC facts
5. Search-demand persistence was retired; rebuild it later on a bounded logging store
6. `audit_reddit_compliance.py` for silver/serving Parquet checks

**Acceptance:**

- [x] No Reddit comment bodies in lake Parquet (derived values only)
- [x] Reddit facts use `concern_taxonomy` fact_keys only
- [x] Reddit facts lose to Google/RERA in resolver + data_loader tests
- [x] Admin data-health exposes RedditTheme counts

**Storage check:** Reddit artifacts stay in `raw/source=reddit/` partitions; facts in standard silver layout.

---

### Phase 7 — Search legacy deletion (3–5 days) — **merged into Phase 4**

> Phase 7 items (`legacy_preference_score`, `routes/search.rs` legacy map) are now **required Phase 4B deliverables**. This section kept for traceability only.

**Goal:** Remove hardcoded preference scoring — done as part of Phase 4B once property entities + bundle facts exist.

**Acceptance:**

- [ ] No `source_type: "Seed"` in match reasons for bundled societies
- [ ] Benchmark recall/precision within agreed tolerance
- [ ] Preference coverage report shows source-backed matches

---

### Phase 8 — Config-driven evidence UI (5–7 days)

**Goal:** Frontend discovers section layout from API metadata, not const maps. **Still society-scoped — no graph traversal yet.**

**Work:**

1. Backend: evidence sections include `constellation`, `fact_keys[]`, `proof_label` from `evidence_sections.json` + fact registry
2. Wire `livability_brief` to `concern_taxonomy.json` (replace `livability_theme_registry`)
3. Remove `SECTION_CONSTELLATION` hard map; use API fields
4. Tile chips driven by `ui.tile_eligible` leaves on match reasons
5. Livability brief `fact_keys` linked to evidence drill-down

**Acceptance:**

- [ ] Add a new leaf in config → appears in correct section after bundle promote (no React edit)
- [ ] Compare page can show same evidence model

**Prerequisite:** Phase 2 (`fact_registry` drives search_metadata).

---

### Phase 9 — Fresh crawler & new sources (ongoing)

**Goal:** Plug in sources without DAG reshape.

**Work:**

1. `registered_transactions` adapter + asset (stub → real when data available)
2. Approach road Google place enrichment
3. Scale Google/community crawl via `entity_selector` + tier prioritization
4. S3 production cutover validation (`OPENESTATES_LAKE_URL=s3://...`)

**Acceptance:**

- [ ] New source = new adapter JSON + skill + asset_registry entry
- [ ] Same bundle promote flow locally and on S3
- [ ] Coverage report drives crawl backlog

---

### Phase 10 — EntityContext graph API + generic UI (7–10 days) — **API last**

**Goal:** Dynamic buyer copy from graph traversal — e.g. *"Prestige Waterford sits on ECC Road, where residents report monsoon waterlogging. Deens Public School is nearby."*

**Contract:** `app/config/dag/entity_context.json` (stub today; implementation reads baked bundle data, not live config).

**Prerequisites (must be done first):**

- Phase 4: `edges` table in serving bundle + Rust edge index
- Phase 4: `road_segment` / `place` nodes with facts attachable upstream
- Phase 8: config-driven evidence sections (parallel surface, not blocked)
- Optional: `near_place` relation in `ontology.json` for proximity without full place identity

**Work:**

1. Rust `EntityContextComposer`: walk from anchor (`society:*` or `property:*`) using `ui_surfaces.json` traversal hops (baked into bundle manifest or startup config snapshot)
2. Emit `clauses[]` then `summary_paragraph` via deterministic `display_template` + hop templates (no LLM on hot path)
3. `GET /api/entities/{entity_id}/context` and `GET /api/properties/{id}/context`
4. React: generic renderers by `presentation.variant`; property page consumes `summary` + `surfaces[]`
5. Enrichment flywheel: traversal exposes missing facts on shared nodes (road, area) → `enrichment_targets.json`

**Acceptance:**

- [ ] Society page shows road-linked risk clause when `served_by_road` + road fact exists
- [ ] No new React component per fact_key — variant-driven only
- [ ] API p99 within search latency budget (local bundle only, no network)
- [ ] `eval_search` unchanged — context endpoint is additive

**Explicitly deferred:** LLM polish of summary paragraph; live config reads on request path.

---

## 6. Workstreams (parallelizable)

```text
Stream A — Config & DAG loader        (Phases 0–1) ✅
Stream B — Fact registry & resolver   (Phases 2–3) ✅
Stream C — Entity + edges + delete    (Phase 4) ✅
Stream D — UI truth consolidation     (Phase 5) ✅
Stream E — Reddit taxonomy pipeline   (Phase 6) ✅
Stream F — Search cleanup             (Phase 7) ✅ merged into 4
Stream G — Discovery + tiles          (Phase 8′) ← NEXT
Stream H — Road/place enrichment      (Phase 9a) before graph API
Stream I — EntityContext graph API    (Phase 10 — last)
```

**Critical path:** Phase 9a (shared nodes) → Phase 10 (graph API)
**Can parallel:** Phase 8′ with Phase 9a

### Graph UI readiness (config today, API later)

| Layer | Status | Location |
|-------|--------|----------|
| Node/edge schema | ✅ Ready | `ontology.json` |
| Leaf defs + templates | ✅ Ready | `concern_taxonomy.json`, `fact_registry.json` |
| Surface traversals | ✅ Ready | `ui_surfaces.json` |
| Edge inference rules | ✅ Ready | `bootstrap/edge_inference.json` |
| Gold edges Parquet | ✅ Schema | `lake/layout.json` |
| Serving edges table | ❌ Gap | Phase 4 |
| road/place instances | ❌ Gap | Phase 4 enrichment |
| Rust graph walker | ❌ Gap | Phase 10 |
| HTTP API | ❌ Deferred | Phase 10 |

Full checklist: `app/config/coverage.json` → `graph_ui_readiness`.

---

## 7. Validation & observability (every phase)

### 7.1 Automated

| Check | Command / artifact |
|-------|-------------------|
| Rust unit/integration | `cargo test` |
| Search quality | `pipeline/eval_search.py` + `data/validation/search_quality_benchmark.json` |
| Frontend types | `npx tsc --noEmit` |
| DAG plan | `openestates-run-assets --dry-run` |
| Parquet schema | serving `schema.json` format_version |

### 7.2 Per-run artifacts (lake)

```text
data/validation/coverage_report.json
  - entity_type × fact_key × source_type → count, %

data/validation/resolution_audit.json
  - superseded facts, conflicts, low-confidence winners
```

### 7.3 Admin API (existing + extend)

`GET /api/admin/data-health` should surface:

- promoted bundle version
- entity/fact counts by type
- preference coverage (% societies with source-backed match per preference)
- stale assets per crawl policy TTL

---

## 8. Risk register

| Risk | Mitigation |
|------|------------|
| Bootstrap legacy facts shown as truth | `internal_only`, low confidence, resolver ranks below Google/RERA |
| Serving bundle size explosion | ZSTD, typed columns, entity sharding at scale, Tantivy for text |
| Config drift vs code | Config hash in bundle manifest; CI validates JSON schemas |
| Reddit compliance | Derived signals only; adapter enforces max confidence + no PII columns |
| Search regression on legacy delete | Baseline eval in Phase 2; delete only in Phase 7 |
| S3 path break | All new artifacts via `LakeKey` + `AssetPathBuilder`; no ad hoc paths |

---

## 9. Definition of done (program level)

- [ ] One `fact_registry` / `concern_taxonomy` config drives search, brief, pulse, evidence
- [ ] Serving bundle is the only runtime truth; no seed score defaults
- [ ] Property + society + area + road entities in bundle; **edges table loaded**
- [ ] Crawl frequency/skip controlled by `crawl_policies/`
- [ ] Legacy preference scoring deleted with eval green
- [ ] UI shows proof labels, not numeric confidence
- [ ] Config-driven evidence sections (Phase 8)
- [ ] EntityContext graph summary API for property/society pages (Phase 10)
- [ ] Parquet + S3 key layout unchanged in contract (format_version bumped only if needed)
- [ ] Issue #2 acceptance criteria met for POC societies

---

## 10. Immediate next actions

**Phases 0–6 complete.** Current work is tracked through executable contracts,
config manifests, and the active runbooks retained in this directory.

1. **Phase 8′:** discovery receipt copy; search tile chips from config
2. **Phase 9a:** road/place enrichment + approach road visuals → lake
3. **Do not start:** EntityContext HTTP API until Phase 9a has road facts in bundle

---

## 11. References

- [`docs/dag_convergence_design.md`](./dag_convergence_design.md) — architecture
- [`backend/src/lake/keys.rs`](../backend/src/lake/keys.rs) — S3-compatible key rules
- [`backend/src/serving/parquet.rs`](../backend/src/serving/parquet.rs) — serving table schema
- [GitHub issue #2](https://github.com/kumargu/openestates/issues/2) — concern taxonomy research
