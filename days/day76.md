# Day 76: Phase 1 Kickoff — KG-Only Loading, Remove Seed Bootstrap, Clean Dead Code

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 4 of 14. Phase 1: Data Cleanup — Day 1 of 3.

## Day 75 Grade
**A.** Phase 0 Gate closed. 207 PASS / 5 WARN / 0 FAIL. Search 10/10, seller matching 7/7. All targets exceeded.

## Feedback Disposition

### Builder Feedback (Day 75):
- **force=True for Google reviews** — Accepted. Correct for enrichment refresh.
- **search verification gate at max 2 exceptions** — Accepted. Keep threshold.
- **interest records persist in data/interests/** — Accepted. Distinguishable by buyer_name.
- **Wrote KG facts directly to JSON instead of Rust API** — Accepted. Matches pipeline pattern.
- **Simple search queries for Phase 0** — Accepted. Adversarial testing is Sprint 5.
- **NBR Group 17 facts vs 44-66** — Accepted as known limitation.
- **4 validation properties have no seller** — By design.
- **NBR Group missing RERA data** — Accepted. Gemini-discovered, no RERA source.

### Verifier Observations (Day 75):
- **Unused `build_embed_text` in semantic.rs** — Fix today. Delete the function.
- **Alternative data sources for Gemini-discovered** — Phase 2 scope.
- **Minimum fact-count threshold** — Phase 2 scope.
- **Dry-run mode for seller verification** — Nice-to-have, not blocking.

## Goal

Restructure backend startup to load societies and areas from KG directly, remove the seed-JSON bootstrap fallback, and clean dead code. This is the prerequisite for all Phase 1 work (days 77-78 depend on the backend loading only from KG).

## Product Reason

The current startup loads properties/societies/areas from `data/seed/*.json` into flat `Vec` fields on `AppState`, THEN separately loads the KG from `data/knowledge/nodes/`. This dual loading means seed data and KG data can diverge, causing trust badge inconsistencies. After Day 76, societies and areas come from the KG — single source of truth.

## Deliverables

### 1. Remove `bootstrap_from_seed()` call from `data_loader.rs`
The KG always exists on disk (259 nodes). Remove the bootstrap fallback. Replace with a clear error: "No knowledge graph found. Run pipeline/seed.py first."

### 2. Delete `bootstrap.rs`
**File:** `backend/src/knowledge/bootstrap.rs` — DELETE entirely (225 lines dead code).
Remove `pub mod bootstrap;` from `backend/src/knowledge/mod.rs`.

### 3. Delete legacy `load_seed_data()` function
**File:** `backend/src/data_loader.rs` — Remove the `#[allow(dead_code)]` functions.

### 4. Remove unused `build_embed_text`
**File:** `backend/src/search/semantic.rs` — Delete unused function (Day 75 verifier feedback).

### 5. Derive `societies` Vec from KG at startup
Add `fn societies_from_graph(graph: &KnowledgeGraph) -> Vec<Society>` in `data_loader.rs`. Read KG society nodes, extract facts into Society struct. Remove `seed/societies.json` loading.

### 6. Derive `areas` Vec from KG at startup
Same pattern: `fn areas_from_graph(graph: &KnowledgeGraph) -> Vec<AreaProfile>`. Remove `seed/area_profiles.json` loading.

### 7. Compile check and cargo test
`cargo check` + `cargo test` after all changes.

## Sequencing

1. Delete `build_embed_text` from `semantic.rs`
2. Remove `pub mod bootstrap;` from knowledge mod + delete `bootstrap.rs`
3. Remove legacy `load_seed_data()` from `data_loader.rs`
4. Write `societies_from_graph()` and `areas_from_graph()`
5. Update `load_app_state()` to use KG-derived societies/areas
6. Remove bootstrap fallback from graph loading
7. `cargo check` + `cargo test`

## Success Criteria

1. `cargo check` passes with zero warnings in changed files
2. `cargo test` passes (65+ tests)
3. `bootstrap.rs` is deleted
4. `build_embed_text` is gone from `semantic.rs`
5. `load_seed_data()` is gone from `data_loader.rs`
6. Backend starts successfully, loading societies and areas from KG nodes
7. Startup log shows KG-derived loading (not seed file loading)
8. Search still returns results for `3bhk whitefield`

## What Day 76 Does NOT Do

- Remove `data/seed/properties.json` loading — Day 77 scope
- Delete seed JSON files from disk — Day 78 scope
- Remove legacy preference scoring from `text.rs` — Day 77 scope
- Migrate 48 legacy societies to RERA entries — Day 78 scope
- Audit/deduplicate KG facts — Day 78 scope
