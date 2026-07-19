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
- **Storage unchanged in shape** — `LakeKey` paths, Parquet serving tables, versioned bundles, local FS → S3 with zero path rewrites
- **KG stays identity** — facts attach to entities; KG is not a second truth store

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

DAG config files live in git (`data/dag/`), not the lake. They are small, diffable, and loaded at DAG plan time.

Optional later: snapshot config hash into serving `manifest.json` for reproducibility.

---

## 2. Config package (single source of product semantics)

Create `data/dag/` and merge existing registries.

```text
data/dag/
  manifest.json                 # version, includes[]
  ontology.json                 # entity types + relations
  fact_registry.json            # all leaves (merged)
  concern_taxonomy.json         # issue #2 signals → fact_key mapping
  resolution_policies.json
  asset_registry.json           # exported from registry.rs
  crawl_policies/
    global.json
    google_places_weekly.json
    reddit_threads_daily.json
    ...
  source_adapters/
    legacy_seed.json
    reddit_theme.json
    google_reviews.json
    rera.json
    registered_transactions.json  # stub for future
  ui_surfaces.json              # section_kind, constellation, tile_chip rules
```

### 2.1 Merge map (eliminate duplicate registries)

| Current file | Fate |
|--------------|------|
| `data/search/fact_schema_registry.json` | Merge into `fact_registry.json` (search section) |
| `data/product/livability_theme_registry.json` | Merge into `concern_taxonomy.json` + `fact_registry.json` |
| `backend/src/assets/registry.rs` | Export to `asset_registry.json`; Rust becomes loader |
| `data/product/buyer_context_sections.json` | Merge UI grouping into `ui_surfaces.json` |
| `data/product/approach_road_visuals.json` | Keep as product overlay; reference fact_keys only |

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
    "source_priority": ["google", "reddit_theme", "legacy_seed"],
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
| `assets/registry.rs` | Full asset DAG | `data/dag/asset_registry.json` |
| `search/text.rs` | `legacy_preference_score`, `legacy_fact_key_for_preference` | Delete after bootstrap + eval |
| `routes/search.rs` | `legacy_preference_to_fact_key` | Delete |
| `search/intent.rs` | Preference → seed field key arrays | Read from `fact_registry.json` |
| `search/schema.rs` | `include_str!(fact_schema_registry.json)` | Load unified `fact_registry.json` |
| `livability_brief.rs` | `include_str!(livability_theme_registry.json)` | Load `concern_taxonomy.json` |
| `data_loader.rs` | `.unwrap_or(0.5)` on seed scores | Hydrate from facts; null if missing |
| `routes/properties.rs` | Evidence section builders (`rera`, `reviews`, …) | Group facts by `ui.section_kind` from config |
| `routes/properties.rs` | `include_str!(approach_road_visuals.json)` | Reference fact_keys; visuals optional overlay |
| `scoring/transparency.rs` | Seed-field composite | DAG-backed confidence components |

### 4.2 Pipeline (Python)

| Location | What is hardcoded | Target |
|----------|-------------------|--------|
| `collect_asset_sources.py` | `OPENESTATES_SKIP_REDDIT` | `crawl_policies/reddit_threads_daily.json` |
| `skills/search_reddit.py` | Ad hoc output shape | Emit `concern_taxonomy` signal keys only |
| Asset source entity lists | Inline seeds | `data/dag/bootstrap/entity_seeds.json` |

### 4.3 Frontend (React)

| Location | What is hardcoded | Target |
|----------|-------------------|--------|
| `lib/evidence.ts` | `SECTION_CONSTELLATION`, `SECTION_DISPLAY_TITLES` | Backend sends `constellation` per section from config |
| `pages/PropertyPage.tsx` | `riskSignalsFor()` seed scores | Remove; use `livability_brief` risk block |
| `pages/PropertyPage.tsx` | `buildDecision()` seed trust/risk | Brief confidence + RERA + market only |
| `lib/types.ts` | Seed score fields on `Property` | Keep only listing fields; quality via evidence |
| `components/evidence/EvidenceSectionCard.tsx` | Orphan | Delete |
| `main.tsx` meta | "transparency scores and tradeoffs" | Receipts/livability copy |

---

## 5. Execution phases

Each phase has **deliverables**, **acceptance criteria**, and **storage checks**.

---

### Phase 0 — Design lock & config scaffold (3–5 days)

**Goal:** Files exist; no runtime behavior change.

**Work:**

1. Create `data/dag/manifest.json` and directory layout
2. Draft `concern_taxonomy.json` from issue #2 + livability themes (~60 leaves)
3. Draft `ontology.json` (entity types + relations)
4. Draft `resolution_policies.json` + `ui_surfaces.json`
5. Document confidence → proof label mapping
6. Add cross-links in `dag_convergence_design.md`

**Acceptance:**

- [x] `data/dag/` scaffold created (`manifest`, `ontology`, `resolution_policies`, `concern_taxonomy`)
- [x] 78 leaves merged from livability themes + issue #2
- [x] `data/dag/README.md` — agent read-one-file-per-task routing
- [ ] Every issue #2 signal maps to a `fact_key` or is explicitly deferred
- [ ] `ui_surfaces.json` (Phase 1)
- [ ] Storage section reviewed against `lake/keys.rs` and `serving/parquet.rs`

**Token-cost rule:** agents and humans edit **one config file per PR**; load via `manifest.json` routing only.

---

### Phase 1 — Config loaders, zero behavior change (5–7 days)

**Goal:** Rust/Python load DAG config; existing tests pass.

**Work:**

1. Export `registry.rs` → `data/dag/asset_registry.json`
2. Add `backend/src/dag_config/` module: load + validate JSON at startup (DAG plan only)
3. `AssetRegistry::from_config()` with fallback to embedded default until parity proven
4. Unit tests: config round-trip equals current topological order
5. Move Reddit skip to `crawl_policies/reddit_threads_daily.json`; keep env override as deprecated shim

**Acceptance:**

- [ ] `cargo test` asset registry tests pass from JSON loader
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

- [ ] Search preference coverage report generated per run
- [ ] Existing search tests pass
- [ ] `eval_search.py` no regression on benchmark queries
- [ ] `search_metadata` row count stable or explainable

**Storage check:** `search_metadata` Parquet schema unchanged; row content richer not wider.

---

### Phase 3 — Legacy bootstrap import (7–10 days)

**Goal:** Current seed/listing data becomes low-confidence sourced facts; stop fake defaults.

**Work:**

1. Implement `source_adapters/legacy_seed.json` importer asset (`legacy_seed_facts`)
2. Map seed JSON → `entity_id` + `fact_key` + `source_type: LegacySeed` + `confidence: 0.25`
3. Mark risk/quality leaves `internal_only: true` until superseded
4. Change `data_loader.rs`: hydrate `Property` listing fields from property facts; remove score defaults
5. Resolver applies `resolution_policies.json`

**Acceptance:**

- [ ] No `unwrap_or(0.5)` on quality/risk fields in `data_loader.rs`
- [ ] Coverage report shows bootstrap fact counts per entity
- [ ] Property page no longer shows identical fake risk bars
- [ ] Legacy facts visible in admin/data-health, not as buyer proof

**Storage check:**

- New silver asset: `legacy_seed_facts` under standard `silver/facts/...` layout
- Serving bundle `fact_count` increases; `entities` may add `property:*`

---

### Phase 4 — Entity expansion: property + area (10–14 days)

**Goal:** Serving bundle materializes full graph for search and UI.

**Work:**

1. `canonical_property_nodes` asset from listings + seed
2. `canonical_area_nodes` from RERA localities + alias config
3. KG edges: `property→society`, `society→area`, `society→road` (where known)
4. Serving bundle includes `property:*`, `area:*`, optional `road:*`
5. Search hard filters read property facts (BHK, price, area) from bundle
6. `entity_refs` on search results populated for all listing cards

**Acceptance:**

- [ ] Bundle entity counts: properties > 0, areas > 0
- [ ] Search "3bhk whitefield under 2cr" uses property + society facts
- [ ] Tiles show `entity_refs` and match reasons from leaves
- [ ] Parquet entity table row growth documented

**Storage check:**

- `entities.parquet` grows; consider `part-NNNNN.parquet` sharding if >500k rows (future)
- Tantivy index rebuild acceptable; keep in bundle version folder

---

### Phase 5 — UI truth consolidation (5–7 days)

**Goal:** One signal, one surface; config-driven evidence sections.

**Work:**

1. Remove `riskSignalsFor` / `RiskBar` from `PropertyPage.tsx`
2. De-duplicate theme chips: brief owns risk/operating prose; pulse owns review receipts
3. Evidence sections grouped by `ui.section_kind` from config (backend)
4. Delete `EvidenceSectionCard.tsx` orphan
5. Proof labels instead of confidence bars in evidence UI
6. Update SEO meta copy

**Acceptance:**

- [ ] No seed-derived risk on property page
- [ ] Livability brief is single risk surface in action rail
- [ ] New `fact_key` with `ui.section_kind` appears without frontend code change
- [ ] `tsc --noEmit` clean

---

### Phase 6 — Reddit concern pipeline (7–10 days)

**Goal:** Issue #2 taxonomy operational; derived signals only.

**Work:**

1. `source_adapters/reddit_theme.json` — derived signals, max confidence 0.45, no raw text
2. Classifier maps threads → `concern_taxonomy` signal keys
3. `reddit_resident_facts` emits `SourcedFact` rows per society/area
4. Crawl policy: disabled by default; isolated worker config stub
5. Seed POC: 10–20 societies from issue #2 qualitative table as manual/low-confidence facts
6. Enrichment queue: search demand → `entity_selector` priority

**Acceptance:**

- [ ] No Reddit comment bodies in lake Parquet
- [ ] `community_concern_themes` aligns with taxonomy keys
- [ ] Reddit facts lose to Google/RERA in resolver tests
- [ ] Compliance note in issue #2 satisfied

**Storage check:** Reddit artifacts stay in `raw/source=reddit/` partitions; facts in standard silver layout.

---

### Phase 7 — Search legacy deletion (3–5 days)

**Goal:** Remove hardcoded preference scoring.

**Work:**

1. Delete `legacy_preference_score`, `format_legacy_display`, `legacy_fact_key_for_preference`
2. Delete `routes/search.rs` legacy preference map
3. `local_fallback_allowed` → always false when serving node exists (or remove path)
4. Re-run `eval_search.py` + search benchmark

**Acceptance:**

- [ ] No `source_type: "Seed"` in match reasons for bundled societies
- [ ] Benchmark recall/precision within agreed tolerance
- [ ] Preference coverage report shows source-backed matches

---

### Phase 8 — Dynamic UI discovery (7–10 days)

**Goal:** Frontend discovers leaves via API metadata, not const maps.

**Work:**

1. API: evidence sections include `constellation`, `fact_keys[]`, `proof_label`
2. Optional: `GET /api/config/fact-registry` for dev/debug (not request hot path)
3. Remove `SECTION_CONSTELLATION` hard map; use API fields
4. Tile chips driven by `ui.tile_eligible` leaves on match reasons
5. Livability brief `fact_keys` linked to evidence drill-down

**Acceptance:**

- [ ] Add a new leaf in config → appears in correct section after bundle promote (no React edit)
- [ ] Compare page can show same evidence model

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

## 6. Workstreams (parallelizable)

```text
Stream A — Config & DAG loader        (Phases 0–1)
Stream B — Fact registry & resolver   (Phases 2–3)
Stream C — Entity materialization     (Phase 4)
Stream D — UI consolidation           (Phase 5)
Stream E — Reddit taxonomy pipeline   (Phase 6)
Stream F — Search cleanup             (Phase 7)
Stream G — Dynamic UI                 (Phase 8)
```

**Critical path:** A → B → C → F (search needs property entities + real facts)  
**Can parallel:** D after B starts; E after A; G after D+F

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

data/validation/enrichment_gaps.json
  - entity_id × missing_fact_key × search_demand_score

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
- [ ] Property + society + area entities in bundle
- [ ] Crawl frequency/skip controlled by `crawl_policies/`
- [ ] Legacy preference scoring deleted with eval green
- [ ] UI shows proof labels, not numeric confidence
- [ ] Parquet + S3 key layout unchanged in contract (format_version bumped only if needed)
- [ ] Issue #2 acceptance criteria met for POC societies

---

## 10. Immediate next actions (this week)

1. **Phase 0:** Create `data/dag/concern_taxonomy.json` (issue #2 + livability merge)
2. **Phase 0:** Create `data/dag/ontology.json` + `resolution_policies.json`
3. **Phase 1 spike:** Export `asset_registry.json` from current `registry.rs`
4. **Quick win:** Remove seed risk list from `PropertyPage.tsx` (Phase 5 item; safe once brief exists)
5. **Baseline:** Run `eval_search.py` and save snapshot before search changes

---

## 11. References

- [`docs/dag_convergence_design.md`](./dag_convergence_design.md) — architecture
- [`docs/livability_brief_plan.md`](./livability_brief_plan.md) — brief composer (already shipped Phase 1)
- [`backend/src/lake/keys.rs`](../backend/src/lake/keys.rs) — S3-compatible key rules
- [`backend/src/serving/parquet.rs`](../backend/src/serving/parquet.rs) — serving table schema
- [GitHub issue #2](https://github.com/kumargu/openestates/issues/2) — concern taxonomy research
