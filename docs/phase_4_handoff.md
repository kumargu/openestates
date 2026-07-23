# Phase 4 Handoff — Graph in Bundle + Thin Rust Engine

> **For:** implementation agent  
> **Prerequisite:** Phase 3 complete (bootstrap facts in lake, no `unwrap_or(0.5)` score defaults in `data_loader.rs`)  
> **Parent plan:** [`dag_execution_plan.md`](./dag_execution_plan.md) § Phase 4  
> **Review session:** keep separate; this doc is the full implementation brief.

---

## Mission

Phase 4 has two mandatory halves — **build** and **delete**. It is not done until:

1. The serving bundle materializes **property, area, road, place** entities and **edges**.
2. Legacy hardcoded Rust (`match preference`, `match kind`, `AREA_ALIASES`, etc.) is **removed** and replaced by config-driven generic loops.
3. **Net LOC goes down** — no graph feature ships without deleting what it replaces.

**North star:** New `fact_key`, preference, evidence section, or area alias = JSON edit + bundle promote. **Zero new Rust match arms for product vocabulary.**

---

## Read first (mandatory)

1. `.claude/skills/coding-practices.md`
2. `AGENTS.md` — especially "Skills own the domain, Rust owns the runtime"
3. `docs/dag_execution_plan.md` — §0.1 Thin Rust engine
4. `app/config/coverage.json` — `graph_ui_readiness` + hardcoded audit
5. `docs/storage_and_config_layout.md` — config vs lake separation

---

## Architecture reminder

```text
app/config/          Git schemas (ontology, fact_registry, evidence_sections, search_intent)
data/lake/           Parquet instances (entities, edges, facts, serving bundle)
Rust runtime         Generic engine: load bundle, index edges, score from search_metadata
                     NOT: per-leaf match arms, per-preference if/else, embedded registries
```

**Request path:** Rust reads **serving bundle only** at runtime (facts, search_metadata, edges). Config is baked at materialize time.

---

## What's already done (do not redo)

| Phase | Status |
|-------|--------|
| 0–1 | `app/config/` scaffold, `dag_config` loaders |
| 2 | `fact_registry.json` → `search_metadata` materialization; schema loads from config |
| 3 | seed JSON removed; resolver and `data_loader` use source-backed DAG facts |

**Baseline eval:** `data/validation/eval_search_phase1_baseline.json`  
**Preference coverage:** written on bundle build to `data/validation/preference_coverage.json`

---

## Phase 4A — Build (graph + bundle)

### 4A.1 Canonical entity nodes

| Asset | Source | Output entity prefix |
|-------|--------|----------------------|
| `canonical_society_nodes` | RERA registry | `society:*` |
| road / place nodes | enrichment + `bootstrap/edge_inference.json` | `road:*`, `place:*` |

- Register assets in `app/config/dag/asset_registry.json`
- Follow existing patterns in `backend/src/assets/kg_view.rs`, `skill_facts.rs`
- Edge inference rules: `app/config/bootstrap/edge_inference.json` (rules only, no instances in config)

### 4A.2 Gold KG edges

Materialize in `gold/kg_society_view/.../edges/part-00000.parquet`:

| Edge type | From | To |
|-----------|------|-----|
| `in_society` | property | society |
| `in_area` | society | area |
| `built_by` | society | builder |
| `served_by_road` | society | road_segment |
| `maps_to_place` | society | place |

Schema: `from_entity_id`, `edge_type`, `to_entity_id`, `confidence`, `source_type`  
See `app/config/lake/layout.json` gold zone.

### 4A.3 Serving bundle edges table

Add to `search_serving_bundle`:

- `edges/part-00000.parquet` (copy/project from gold)
- Bump `format_version` in serving `schema.json` if needed
- Update `backend/src/serving/loader.rs` + `parquet.rs` read/write
- Load into `AppState` at startup

### 4A.4 `GraphIndex` module (new)

Create `backend/src/graph/` (or `backend/src/serving/graph_index.rs`):

```rust
pub struct GraphIndex {
    edges_from: HashMap<(String, String), Vec<String>>, // (from_id, edge_type) → [to_id]
    edges_to: HashMap<(String, String), Vec<String>>,   // enrichment / reverse walk
}

impl GraphIndex {
    pub fn from_serving_edges(edges: &[ServingEdgeRecord]) -> Self { ... }
    pub fn walk_out(&self, anchor: &str, hops: &[&str], max_depth: usize) -> Vec<WalkStep> { ... }
}
```

- **Unit tests only** — no HTTP endpoint (Phase 10)
- Test case: `society:prestige-waterford` → `served_by_road` → `road:ecc-road`
- Contract reference: `app/config/dag/entity_context.json` (`max_hops: 2`)

### 4A.5 Search + tiles

- Hard filters (BHK, price, area, budget) read from **property facts** in serving bundle, not seed `Property` fields
- Populate `entity_refs` on all search result cards (`routes/search.rs`)
- Verify query: `"3bhk whitefield under 2cr"` uses property + society facts

### 4A.6 Area aliases (config migration)

- Move `AREA_ALIASES` from `backend/src/search/intent.rs` into `app/config/dag/search_intent.json`
- Loader in `dag_config`; intent parsing reads config at startup
- Delete the const array in Rust as part of 4B

---

## Phase 4B — Delete + thin Rust engine (required)

**Rule:** Every deleted `match` arm → replaced by a config loop, not a shorter hardcoded list.

### 4B.1 Search legacy scoring — DELETE

**File:** `backend/src/search/text.rs`

| Function | Action |
|----------|--------|
| `legacy_preference_score` | **Delete** (~200-line `match preference`) |
| `legacy_fact_key_for_preference` | **Delete** |
| `format_legacy_display` | **Delete** |
| `legacy_negative_preference_evaluation` | **Delete** |
| `local_fallback_allowed` | **Delete** or always false when serving node exists |

**Replace with:** generic scorer over `PreferencePatternSpec` + bundle `search_metadata` + facts on `property:*` / `society:*`.

```rust
// Target pattern
for pattern in positive_preference_patterns() {
    if patterns_match_query(&pattern.patterns, query) {
        score += score_entity_for_fact_keys(entity_id, &pattern.expanded_keys, serving);
    }
}
```

**File:** `backend/src/routes/search.rs`

| Function | Action |
|----------|--------|
| `legacy_preference_to_fact_key` | **Delete** — use `fact_registry` patterns |

### 4B.2 Registry fallbacks — DELETE files + includes

| Item | Action |
|------|--------|
| `include_str!(fact_schema_registry.json)` fallback in `search/schema.rs` | Remove fallback; fail loud if `fact_registry.json` missing |
| `data/search/fact_schema_registry.json` | **Delete from repo** |
| `include_str!(livability_theme_registry.json)` in `livability_brief.rs` | Load `concern_taxonomy.json` + `fact_registry` |
| `data/product/livability_theme_registry.json` | **Delete from repo** |

### 4B.3 Property evidence sections — wire config, delete matches

**File:** `backend/src/routes/properties.rs`

Delete hardcoded:

- `evidence_presentation_for_kind` (`match kind { "rera" => ... }`)
- `evidence_section_priority` (`match kind { ... }`)
- Per-`kind` fact key string lists

**Wire:** `app/config/product/evidence_sections.json` (already exists, partially used)

- Load at startup via `dag_config` or existing product config loader
- Generic loop: `for section in evidence_sections { build_panel(section.kind, section.facts, section.presentation) }`

### 4B.4 Property model cleanup

**Files:** `backend/src/models/property.rs`, `frontend/src/lib/types.ts`

Remove seed score fields if no longer referenced after search migration:

- `society_quality_score`, `builder_quality_score`, `noise_score`, etc.
- Keep listing fields (price, bhk, area, society_id) until fully on property facts

### 4B.5 Data directory cleanup

| Path | Action |
|------|--------|
| `data/knowledge/` runtime reads | Remove stragglers; facts live in lake |
| `data/intelligence/` | Delete when empty / migrated |
| Embedded default in `assets/registry.rs` | Remove if JSON loader parity proven in tests |

### 4B.6 Update audit

Mark removed items in `app/config/coverage.json` (`still_hardcoded_in_rust_pending_migration`).

---

## Suggested PR sequence

```text
PR 1  canonical society/road nodes + search hard filters + entity_refs
PR 2  gold edges + serving edges.parquet + GraphIndex + walk unit tests
PR 3  delete search legacy scoring (text.rs + search.rs) — eval gate
PR 4  wire evidence_sections.json, delete properties.rs match arms
PR 5  livability_brief → concern_taxonomy; delete livability_theme_registry.json
PR 6  AREA_ALIASES → search_intent.json; delete fact_schema_registry.json + data/knowledge stragglers
```

Run `eval_search.py` **before and after** PR 3 and PR 6.

---

## Acceptance checklist

### Graph / bundle

- [ ] Serving bundle: `properties > 0`, `areas > 0`, `roads > 0` (where inferable)
- [ ] `edges/part-00000.parquet` in bundle; row count documented
- [ ] `GraphIndex::walk_out` unit test: `society → served_by_road → road_segment`
- [ ] Search `"3bhk whitefield under 2cr"` uses property + society facts from bundle
- [ ] All search result tiles have `entity_refs`

### Thin engine / deletion

- [ ] No `legacy_preference_score`, `local_fallback_allowed` in search hot path
- [ ] No `match preference { "metro access" => ... }` product arms in `search/text.rs`
- [ ] No `match kind { "rera" => ... }` for evidence in `properties.rs`
- [ ] No `AREA_ALIASES` const in `search/intent.rs`
- [ ] `data/search/fact_schema_registry.json` **deleted**
- [ ] `data/product/livability_theme_registry.json` **deleted**
- [ ] `coverage.json` updated

### Quality gates

- [ ] `cargo test` green
- [ ] `python3 -m pipeline.eval_search` — no regression vs `eval_search_phase1_baseline.json`
- [ ] Net LOC reduction noted in PR description

---

## Explicitly out of scope

| Item | Phase |
|------|-------|
| `GET /api/entities/{id}/context` (EntityContext API) | 10 |
| Summary paragraph composer / generic React renderers | 10 |
| Reddit concern pipeline | 6 |
| Frontend `SECTION_CONSTELLATION` const maps | 8 |
| `near_place` ontology edge (optional; add if place enrichment needs it) | 4 optional |
| S3 production cutover | 9 |
| Git commits / PR creation | only if user asks |

---

## Key files reference

```text
# Config
app/config/dag/ontology.json
app/config/dag/fact_registry.json
app/config/dag/search_intent.json
app/config/dag/entity_context.json          # contract only — no API yet
app/config/product/evidence_sections.json
app/config/bootstrap/edge_inference.json
app/config/lake/layout.json                 # serving edges schema
app/config/coverage.json

# Rust — build
backend/src/assets/kg_view.rs
backend/src/assets/registry.rs
backend/src/serving/builder.rs
backend/src/serving/loader.rs
backend/src/serving/parquet.rs
backend/src/data_loader.rs
backend/src/state.rs                        # AppState + GraphIndex

# Rust — delete / thin
backend/src/search/text.rs                  # legacy_preference_score
backend/src/search/schema.rs
backend/src/search/intent.rs                # AREA_ALIASES
backend/src/routes/search.rs
backend/src/routes/properties.rs            # evidence match arms
backend/src/livability_brief.rs

# Tests / eval
backend/tests/search_quality_contract.rs
data/validation/eval_search_phase1_baseline.json
pipeline/eval_search.py
```

---

## Testing commands

```bash
# Before starting — confirm Phase 3
rg 'unwrap_or\(0\.5\)' backend/src/data_loader.rs   # should be empty

# Rust
cd backend && cargo test
cd backend && cargo test search_quality

# Search eval (backend on :4000)
python3 -m pipeline.eval_search --output /tmp/eval_phase4.json

# Regenerate DAG config if taxonomy touched
python3.10 pipeline/tools/build_dag_json.py

# DAG plan dry-run
cargo run --bin openestates-run-assets -- --dry-run
```

---

## Definition of done

Phase 4 is complete when the serving bundle is the **only** runtime truth for entities, edges, facts, and search metadata — and the Rust codebase no longer encodes product vocabulary in large `match` blocks. The graph index exists and is tested; the summary API does not.

**Add-leaf test (manual):** Add a dummy entry to `fact_registry.json` + `concern_taxonomy.json`, rebuild bundle, confirm it appears in `search_metadata` and preference scoring **without any Rust code change**.
