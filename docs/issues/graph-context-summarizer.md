# Issue: Graph-walk context summarizer (Waterford / all societies)

**Priority:** P0 — most important product gap  
**Status:** Open  
**Created:** 2026-07-20  
**Test society:** `society:prestige-waterford` / `discovered-prestige-waterford-3bhk`

---

## Problem

The Livability brief and neighborhood context should read like a **simple, factual paragraph** built from graph-walked data — ECC Road, Deens School, Hopefarm metro, Bagmane, hospitals, offices, etc. — not template blocks with theme chips.

Today we have **rich graph data in the lake** (after forced materialization) but the **user-facing summary is still thin, truncated, and sometimes wrong**. The walker was built for this; the deploy path and composer are not finished.

### What users see today (bad)

- Livability brief with "Operating quality / Positive signals / How to judge" blocks and clickable theme chips
- Composed paragraph like: *"prestige waterford sits on ECC Road. Nearby: Bagmane Tech Park, Deens Public School, and Hopefarm Channasandra metro. approach-road waterlogging: mentioned."*
- Missing POIs that **are** in the graph (Shantiniketan mall, Manipal, Marriott, Seegehalli metro)
- Raw fact-key leakage (`approach-road waterlogging: mentioned` instead of plain English)
- Society name from slug (`prestige waterford`) instead of `rera_project_name` / display name

### What we want (good)

One calm paragraph, no tags, no lens headers — e.g.:

> Prestige Waterford sits on ECC Road in Pattandur Agrahara. Nearby are Deens Public School, Hopefarm Channasandra metro, Prestige Shantiniketan mall, Manipal Hospital Whitefield, Bagmane Tech Park, and Seegehalli metro. Residents mention waterlogging on the approach road during monsoon. Google reviews (4.4★) praise amenities, greenery, and connectivity; traffic is the main caution.

---

## What we proved this session

### DAG + lake (after forced rematerialize)

Bundle: `waterford-rich-context-20260719t195318z`

**Graph edges from `society:prestige-waterford` (11):**

| Relation | Target |
|----------|--------|
| `served_by_road` | `road:ecc-road` — ECC Road |
| `served_by_road` | `road_segment:prestige-waterford-approach` — Street View approach |
| `in_area` | `area:pattandur-agrahara` |
| `near_place` | `place:deens-public-school` |
| `near_place` | `place:hopefarm-channasandra-metro` |
| `near_place` | `place:seegehalli-metro` |
| `near_place` | `place:prestige-shantiniketan-mall` |
| `near_place` | `place:manipal-hospital-whitefield` |
| `near_place` | `place:whitefield-marriott` |
| `near_place` | `place:bagmane-tech-park` |
| `maps_to_place` | `place:prestige-waterford-nearby-schools` (generic bootstrap — should be dropped) |

**Society facts already in bundle (sample):**

- `rera_project_name`: PRESTIGE WATERFORD
- `google_rating` / `google_review_count`: 4.4 / 441
- `community_review_summary`, `community_positive_themes`, `community_concern_themes`
- `nearby_schools`, `nearby_metro_stations`, `nearby_hospitals`, `nearby_tech_parks` (Google nearby)
- `approach_road_visual_available` + `media.approach_road_frames` on approach road segment
- `risk.approach_road_waterlogging` on ECC Road

**Raw dump:** `backend/tests/waterford_context_dump.rs` → run with  
`cargo test --release --test waterford_context_dump -- --nocapture`  
(log also at `/tmp/waterford-context-dump.log` from session)

### Code landed (partial — not done)

| Area | Change |
|------|--------|
| `data/validation/society_local_context.json` | Curated Waterford POI graph seed |
| `backend/src/assets/approach_road_graph.rs` | Merges local context into `canonical_road_nodes` |
| `backend/src/entity_context.rs` | Name-based clauses + simple paragraph composer |
| `backend/src/routes/properties.rs` | Livability brief prefers graph `summary_paragraph` |
| `frontend/.../LivabilityBriefCard.tsx` | Single-paragraph mode when `summary_paragraph` set |

---

## Root causes (why deploy is incomplete)

### 1. Composer gaps (`entity_context.rs`)

- [ ] **`max_clauses: 8`** cuts the paragraph before all `near_place` nodes appear
- [ ] **Duplicate road clauses** — both `road:ecc-road` and `road_segment:prestige-waterford-approach` emit `served_by_road` + waterlogging
- [ ] **Generic bootstrap place** (`*-nearby-schools`) should never surface; filter by entity id suffix
- [ ] **Risk fact fallback** not used for `road_segment:*` — leaks `approach-road waterlogging: mentioned`
- [ ] **Society display name** — `entity_display_name` should prefer `rera_project_name` (fact exists; paragraph still used slug in one API run)
- [ ] **Google nearby facts** on society (`nearby_schools`, etc.) not merged into paragraph when graph places exist (dedupe / pick best source)
- [ ] **No merge of review summary** — `community_review_summary` available but not in composed paragraph

### 2. Serving bundle / entity index gap

- [ ] `society:prestige-waterford` has **edges + facts** but was **missing from `entities.parquet`** → `compose_entity_context` returned `null` until `resolve_walk_anchor` was relaxed
- [ ] **All societies** used for property detail should be in serving `entities` table, not only facts/edges

### 3. DAG deploy path (not automatic)

- [ ] Normal `openestates-run-assets` run **did not** rematerialize `canonical_road_nodes` (on_change = skipped)
- [ ] Local context seed only enters lake after **forced** rematerialize of `canonical_road_nodes` → `approach_road_graph_facts` → `kg_society_view` → `search_serving_bundle`
- [ ] No documented one-command promote path for graph-context changes

### 4. Pipeline / graph richness (scale beyond seed)

- [ ] `google_nearby_place_facts` writes **text facts on society**, not `place:*` entities + `near_place` edges
- [ ] `near_place` edge type was **pending** in `entity_context.json` — now used in seed but not from live Google pipeline
- [ ] Showcase seed (`society_local_context.json`) is **Waterford-only** — other societies still get generic approach-road bootstrap

### 5. UI / product surface

- [ ] Livability brief still has **fallback template blocks** when graph context empty
- [ ] `EntityContextCard` hidden when approach road trail shows — graph paragraph should be **the** "Before you shortlist" surface (single card)
- [ ] Property page should not duplicate context in two cards

---

## Architecture decision (locked for next session)

| Layer | Role |
|-------|------|
| **Lake / DAG** | `place:*` entities, typed edges (`served_by_road`, `near_place`, `in_area`), leaf facts |
| **Rust request path** | Graph walk + deterministic paragraph join — **no LLM** |
| **Optional offline** | Python skill could materialize `society.context_summary` into bundle later; not required for v1 |

Do **not** add LLM to `/api/properties/{id}` hot path.

---

## Next session — implementation plan

### Step 1: Fix composer (Rust only)

1. Raise or restructure `max_clauses` — prefer **one sentence per category**: road, area, nearby list (all places), one concern line, optional review line
2. Dedupe roads: prefer named road (`road:ecc-road`) over `road_segment:*-approach` when both `served_by_road`
3. Always use `fact_registry` / fallback strings for risk facts — never `fact_key: value` in user text
4. Resolve society name from `rera_project_name` → entity.name → title-case slug
5. Add unit test with full Waterford fixture asserting **all 7 POIs** appear in paragraph
6. Optionally append one line from `community_review_summary` when present (short, not a second card)

**Files:** `backend/src/entity_context.rs`, `app/config/dag/entity_context.json`

### Step 2: Serving bundle entity coverage

1. Ensure every `society:*` with properties appears in `entities.parquet` (name + searchable_text)
2. Add contract test: societies with `served_by_road` edges must resolve `compose_entity_context` ≠ null

**Files:** `backend/src/serving/builder.rs`, `backend/tests/serving_bundle_contract.rs`

### Step 3: DAG promote playbook

Document and script:

```bash
cd backend && cargo run --release --bin openestates-run-assets -- \
  --partition dt=$(date +%Y-%m-%d) \
  --partition subreddit=bangalorerealestates
# When graph seed / approach_road_graph changes, force:
cargo test --release --test waterford_context_dump materialize_and_dump_waterford_context -- --nocapture

curl -X POST http://127.0.0.1:4000/api/admin/serving-bundle/reload -H "X-Admin-Token: dev"
```

Or add `--force canonical_road_nodes,approach_road_graph_facts,kg_society_view,search_serving_bundle` to CLI.

### Step 4: Google nearby → graph edges (pipeline)

1. `google_nearby_place_facts` should emit `place:{slug}` entities + `near_place` edges, not only `nearby_schools` text on society
2. Dedupe against `society_local_context` seed where place_id matches
3. Re-run for Waterford gate location; verify Deens / metro / hospitals appear without manual seed

**Files:** `backend/src/assets/google.rs`, `pipeline/collect_asset_sources.py`, `app/config/bootstrap/edge_inference.json`

### Step 5: UI cleanup

1. Livability brief = **only** `summary_paragraph` when present (remove block/chip fallback for enriched societies)
2. Show graph context even when `ApproachRoadTrail` is visible (or merge into one section)
3. Smoke: `curl /api/properties/discovered-prestige-waterford-3bhk` → `livability_brief.summary_paragraph` contains `ECC Road` and `Deens`

**Files:** `PropertyPage.tsx`, `LivabilityBriefCard.tsx`, `tests/smoke_test.sh`

---

## Acceptance criteria (definition of done)

- [ ] **Waterford property page** shows one paragraph, no theme chips, mentioning at minimum: ECC Road, Deens School, Hopefarm metro, Shantiniketan mall, Manipal, Bagmane, Seegehalli metro (or live Google nearby equivalent)
- [ ] **No raw fact keys** in user-visible summary text
- [ ] Society name displays as **Prestige Waterford** (proper casing)
- [ ] `GET /api/entities/society:prestige-waterford/context` returns non-null `summary_paragraph` on promoted bundle **without** forced test materialization
- [ ] Normal DAG run after editing `society_local_context.json` picks up changes (or documented force path)
- [ ] `cargo test` + smoke test include Waterford context assertion
- [ ] `eval_search` / property detail smoke unchanged for non-enriched societies (graceful fallback)

---

## Verification commands

```bash
# Dump full Waterford graph + facts + composed context
cd backend && cargo test --release --test waterford_context_dump materialize_and_dump_waterford_context -- --nocapture

# Live API
curl -s http://127.0.0.1:4000/api/entities/society:prestige-waterford/context | jq .
curl -s http://127.0.0.1:4000/api/properties/discovered-prestige-waterford-3bhk | jq '.livability_brief.summary_paragraph'

# Bundle contains POI edges
rg -a "deens-public-school|ecc-road|bagmane-tech-park" \
  data/lake/serving/search_bundle/version=*/edges/part-00000.parquet
```

---

## References

- Config contract: `app/config/dag/entity_context.json` (`example_output` is the target shape)
- Local POI seed: `data/validation/society_local_context.json`
- Approach road bootstrap: `data/validation/approach_road_visuals.json`
- AGENTS.md: no LLM on search/property hot path; config over hardcoding
- Session dump log: `/tmp/waterford-context-dump.log`

---

## Out of scope (this issue)

- LLM polish of paragraphs
- Compare page / shortlist changes
- Expanding `society_local_context.json` to all 146 properties (follow-on: Google nearby → edges)
