# Final Phases — Tightened Plan (post Phase 6)

> **Status:** Phases 0–6 complete. One remaining product arc: **finish config-driven surfaces → put road/place facts in lake → EntityContext API last**.  
> **Parent:** [`dag_execution_plan.md`](./dag_execution_plan.md) · [`coverage.json`](../app/config/coverage.json)

---

## Shape check — are we on track?

**Yes, structurally.** The v2 bet is holding:

| Layer | Intended | Actual |
|-------|----------|--------|
| Semantics | `app/config/dag/` owns keys, scoring, resolution, surfaces | ✅ `fact_registry`, `concern_taxonomy`, `resolution_policies`, `ui_surfaces`, `search_intent` |
| Instances | Lake Parquet + serving bundle | ✅ entities, facts, edges, `GraphIndex` at runtime |
| Request path | Rust reads bundle only; no crawl/LLM on `/api/search` | ✅ |
| UI | Config-driven evidence; one signal per surface | ✅ Phase 5; brief/pulse dedup |
| Expand | New leaf = JSON row, not Rust `match` arm | ✅ mostly true |

**Honest gaps (don't add more code until these are closed or deleted):**

| Gap | Risk | Action |
|-----|------|--------|
| `data/product/*.json` duplicates | Two sources of truth | ✅ Deleted; approach road bootstrap in `data/validation/` + DAG assets only |
| `discovery.rs` embedded fallback shelves | Hidden hardcode if config missing | ✅ Removed — config only |
| `coverage.json` `gaps_before_API` | Stale | ✅ Updated after Gates B/C |
| Reddit POC JSON | Scaffold only | Keep until crawl worker; delete import path when live |
| `registry.rs` embedded asset graph | Test-only drift | Delete when asset_registry parity proven |
| Match reason **string** vs structured chips | Tiles still flatten to prose | ✅ Gate A — structured `MatchExplanationBlock` |
| Road/place facts sparse | Graph walks say nothing | ✅ Gate B — `canonical_road_nodes` + `approach_road_graph_facts` |
| EntityContext API | Contract only | ✅ Gate C — composer + endpoints + `EntityContextCard` |

**Anti-pattern to avoid:** new skills, adapters, or React components that don't consume an existing config primitive (`fact_key`, `ui_surfaces`, `enrichment_targets`).

---

## Remaining work — one arc, three gates

Old labels (8′, 9a, 10, 9b) collapse into **three gates**. Do not start Gate C until Gate B acceptance is green.

```text
Gate A — Surface truth (UI remnants)     ~3–4 days
Gate B — Shared nodes in lake            ~4–5 days   ← blocks graph copy
Gate C — EntityContext API + renderers   ~5–7 days   ← API last
Ops     — scale / S3 / crawl             ongoing     ← not on critical path
```

### Gate A — Surface truth ✅

| # | Deliverable | Status |
|---|-------------|--------|
| A1 | Search tile chips from `MatchReason` + registry | ✅ `MatchExplanationBlock` on tiles; prose hidden when structured reasons exist |
| A2 | `discovery_home.json` `receipt_copy` | ✅ embedded config; API field renamed |
| A3 | Livability brief themes → evidence drill-down | ✅ `evidence-nav.ts` + section anchors |
| A4 | Single discovery config path | ✅ `include_str!`; legacy `data/product/` copies deleted |

---

### Gate B — Shared nodes in lake ✅

**Goal:** `society → served_by_road → road_segment` and `place:*` have facts so graph traversal is non-empty.

| # | Deliverable | Status |
|---|-------------|--------|
| B1 | Migrate `approach_road_visuals.json` → facts on `road_segment` | ✅ DAG assets; no runtime `data/product/` read |
| B2 | Approach-road concern facts on `road_segment` | ✅ `risk.approach_road_waterlogging`, `media.approach_road_frames` |
| B3 | `place:*` + `maps_to_place` edges from visuals bootstrap | ✅ `canonical_road_nodes` materializer |
| B4 | `enrichment_targets.json` → single runner entry | ⏳ Ops — not blocking graph copy |

**Acceptance:** `road_graph_rows_include_served_by_road_edges` + bundle-backed `approach_road_media_for` via `GraphIndex::walk_out`.

---

### Gate C — EntityContext ✅

**Goal:** Deterministic graph summary API — no LLM on hot path.

| # | Deliverable | Status |
|---|-------------|--------|
| C1 | `EntityContextComposer` in Rust | ✅ `entity_context.rs` |
| C2 | `GET /api/properties/{id}/context` | ✅ + `/api/entities/{entity_id}/context` |
| C3 | Generic React renderer | ✅ `EntityContextCard` on property page |
| C4 | `coverage.json` + `entity_context.json` status → `implemented` | ✅ |

---

### Ops (ongoing — not a gate)

- S3 lake cutover when bundle promote works locally
- Reddit isolated worker → delete `reddit_poc_import`
- `enrichment_gaps.json` → crawl prioritization
- Delete `data/intelligence/` (already empty)
- Remove embedded asset graph in `registry.rs`

---

## Next iteration (before / with DAG run)

**Purpose:** Prove Phase 6 path end-to-end and baseline bundle — not build new features.

### Step 1 — Run DAG (you)

```bash
# From repo root — promote serving bundle with reddit_resident_facts (POC facts on skip path)
cd backend && cargo run --release --bin openestates-run-assets -- \
  --assets reddit_resident_facts,search_serving_bundle \
  --partition dt=$(date +%Y-%m-%d)

# Reload without restart
curl -X POST http://localhost:4000/api/admin/serving-bundle/reload \
  -H "X-Admin-Token: $OPENESTATES_ADMIN_TOKEN"
```

### Step 2 — Verify (agent or you)

| Check | Command / endpoint |
|-------|-------------------|
| RedditTheme in bundle | `GET /api/admin/data-health` → `reddit_theme.fact_count > 0` |
| Resolver | Society with Google + Reddit same key → Google wins in property detail |
| Search regression | `python3.10 -m pipeline.eval_search` |
| Compliance | `python3.10 -m pipeline.audit_reddit_compliance` |
| Rust | `cd backend && cargo test` |

### Step 3 — Housekeeping (small, same iteration)

- [x] Update `coverage.json` `graph_ui_readiness.gaps_before_API`
- [ ] Mark Phase 6 + Gate A commits if not yet committed
- [x] Gate A surface truth implemented — run DAG next

---

## After DAG — recommended build order

```text
1. Gate A (surface truth)     — parallel-safe, unblocks tile quality
2. Gate B (road/place lake)   — required before any graph API
3. Gate C (EntityContext)     — last
```

**Estimated total:** ~12–16 focused days if scope is held (no compare, no new sources).

---

## Definition of done (entire arc)

1. Config defines semantics; lake defines instances; Rust/UI are thin loaders.
2. Search tiles and evidence panels need **no React edit** for new `fact_key` with `ui.tile_eligible` / section mapping.
3. Graph walk from society reaches road with facts; EntityContext returns deterministic clauses.
4. No duplicate `data/product/` runtime reads; POC/bootstrap paths documented or removed.
5. `eval_search` + `cargo test` green after each gate.

**Then:** product iteration on search quality and crawl scale — not new architecture phases.
