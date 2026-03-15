# Day 78: Final Phase 1 Cleanup — Dead Code, Stale References, Crash Safety

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 6 of 14. Phase 1: Data Cleanup — Day 3 of 3 (FINAL).

## Day 77 Grade
**A.** Properties load from KG. Migrated 20 seed-only properties to KG nodes (146 total). Added properties_from_graph(), deleted persist_to_seed(), updated registration publish to use KG. Fixed stale doc comments, cleaned allow(dead_code). 72 tests pass.

## Feedback Disposition

### Day 77 Builder Feedback:
- **society_quality_score defaults to 0.5 via .max(0.5)** — Accepted. 0.0 is genuinely no-data, 0.5 is a safe neutral default.
- **price_per_sqft computed from price/carpet_area_sqft** — Accepted. Derived field, no separate KG fact needed.
- **Kept get_fact_history() with allow(dead_code)** — Delete today. Zero callers in production code.
- **derive_society_id() best-effort slug derivation** — Accepted, matches ingest pattern.
- **themes.rs still references seed data in comments** — Fix today.
- **Some allow(dead_code) annotations remain** — Audit and fix today.
- **No KG property nodes have transparency_tags/images/hero_image** — Accepted. Data gap for Phase 2 enrichment.

### Day 77 Verifier Suggestions:
1. Delete 4 dead functions behind allow(dead_code) in edge.rs/graph.rs/node.rs — Do today.
2. Sweep seed data references in scoring/themes.rs comments — Do today.
3. Per-node save_node() in ingest_discoveries() for crash safety — Do today.
4. Rename seed_root variable in data_loader.rs — Do today (remove entirely).

### Day 75 Builder Feedback (still open):
- **Test interest records with buyer "Phase0 Test"** — Accepted, distinguishable test data.
- **NBR Group 17 facts vs 44-66 for others** — Accepted, Gemini-discovered limitation.

## Goal

Complete all Phase 1 cleanup items. After Day 78, the codebase has zero stale seed/bootstrap references, zero unnecessary `allow(dead_code)` annotations, and improved crash safety in the discovery ingestion path. The `data/seed/` directory is deleted. Legacy scoring fallback is documented as intentionally retained (KG property nodes lack the scores that would replace it).

## Product Reason

Dead code, stale comments, and misleading variable names slow down future contributors and create confusion about what is live vs. deprecated. Crash safety in ingest prevents data loss during live discovery. Deleting `data/seed/` is the final signal that the KG is the single source of truth.

## Deliverables

### D1: Delete dead functions with `allow(dead_code)`

Audit every `allow(dead_code)` annotation. For each: (a) is it called? (b) is it test-only? (c) is it a trait implementation?

**Delete (zero callers):**
- `knowledge/node.rs` — `get_fact_history()`. No callers.
- `knowledge/edge.rs` — `with_weight()`, `with_metadata()`. No callers.
- `knowledge/graph.rs` — `get_node_mut()`. No callers.
- `storage/keys.rs` — `StorageKey::seed_file()`. No callers (no "seed/" prefix used).

**Keep but remove `allow(dead_code)` (actually used or serde):**
- Trait types in `cache/mod.rs`, `storage/mod.rs` — used via `Arc<dyn T>`.
- Serde fields in `discovery/gemini.rs` — needed for deserialization, keep annotation.
- `storage/local_fs.rs` — `collect_keys_recursive` called by `list()`.

### D2: Sweep stale seed/bootstrap references in comments

Update all comments referencing "seed data" as if it is still the data source:
- `scoring/themes.rs` — 6+ references to "seed data" → change to "Property struct"
- `scoring/mod.rs` — "seed data thresholds"
- `discovery/ingest.rs` — "same shape as seed data"
- Rename `seed_avg` variable → `prop_avg`

### D3: Remove `seed_root` from data_loader.rs and LocalFsBackend

Since no code calls `storage.get("seed/...")` anymore:
- Remove `seed_root` field from `LocalFsBackend`
- Remove `seed_root` parameter from constructor
- Remove `seed/` prefix handling from `resolve_path()`
- Remove `seed_root` variable from `data_loader.rs`

### D4: Per-node `save_node()` in discovery ingestion for crash safety

Replace `save_graph(&kg_dir, &graph)` in `routes/search.rs` with per-node `save_node()` calls for newly created nodes. Each `save_node()` uses tmp+rename for atomicity. If crash occurs, surviving nodes are intact.

### D5: Delete `data/seed/` directory

Verify no code reads from it. Delete via `git rm -r data/seed/`.

### D6: Assess legacy scoring fallback removal

**Decision: Do NOT remove today.** KG property nodes have only 7-8 fact keys, no `answers_preferences` or `scoring_hint`. Legacy fallback is actively needed. Add TODO comment referencing Phase 2.

### D7: Tests

Run `cargo test` after each deliverable. Expected: 72 tests pass, zero warnings.

## Constraints

- Do NOT remove legacy_preference_score() or legacy_preference_to_fact_key() — actively needed.
- Do NOT remove allow(dead_code) from serde deserialize-only fields.
- Delete data/seed/ via git rm so it is recoverable.
- Run cargo test after D1, D2, D3, D4 individually.

## Success Criteria

1. `cargo build` succeeds with zero unnecessary `allow(dead_code)` warnings
2. `cargo test` passes with >= 72 tests
3. Zero comments referencing "seed data" as a live data source
4. No `seed_root` variable in data_loader.rs or LocalFsBackend
5. `data/seed/` directory deleted from working tree
6. Discovery ingestion uses per-node save_node(), not save_graph()
7. No StorageKey::seed_file() method
8. Legacy scoring fallback documented as intentionally retained with Phase 2 TODO
