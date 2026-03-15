# Day 77: Properties Load from Knowledge Graph

## Sprint Position
Sprint 4 (Data Cleanup & RERA Expansion), Day 5 of 14. Phase 1: Data Cleanup — Day 2 of 3.

## Day 76 Grade
**A.** Deleted bootstrap.rs (225 lines), build_embed_text, load_seed_data. Societies and areas now derived from KG at startup. 69 tests pass, zero warnings.

## Feedback Disposition

### Day 76 Builder Feedback:
- **Society ID format: stripped society: prefix** — Correct, matches route expectations.
- **FactStr/FactTags wrapper types** — Good pattern, reuse for properties.
- **allow(dead_code) on manual(), default_confidence(), node_id()** — Fix today: use `#[cfg(test)]` if test-only, else delete.
- **Properties still load from seed JSON** — Fix today (main deliverable).
- **Sparse KG area data** — Accepted. Defaults are fine; enrichment fills gaps over time.
- **22/70 societies missing year_built and total_units** — Accepted. Default to 0.

### Day 76 Verifier Observations:
- **Stale doc comment in marketplace.rs referencing seed/bootstrap** — Fix today.
- **Stale doc comment in store.rs mentioning bootstrap** — Fix today.
- **Day 77 should remove properties seed loading** — Main deliverable.
- **Integration test: societies_from_graph > 0 from real KG** — Add today.

## Goal

Eliminate `data/seed/properties.json` as a data source. Properties derive from KG property nodes at startup, matching the pattern Day 76 established for societies and areas. After Day 77, the backend loads **zero** data from seed JSON — the Knowledge Graph is the single source of truth.

## Product Reason

Seed JSON has hardcoded scores with no provenance (society_quality_score: 0.5, noise_score: 0.5). KG property nodes have SourcedFacts with confidence and skill_id. Loading from KG means every property field traces to its source — completing the transparency promise.

## Deliverables

### D1: `properties_from_graph()` in `backend/src/data_loader.rs`

Add `properties_from_graph(&KnowledgeGraph) -> Vec<Property>`, following the `societies_from_graph()` pattern. Map KG fact keys to Property struct fields using existing `fact_text()`, `fact_numeric()`, `fact_tags()` helpers. Missing facts get sensible defaults.

### D2: Replace seed loading in `load_app_state()`

Replace the `load_via_storage` call for properties with `properties_from_graph(&graph)`. Remove `seed_root` variable if no longer used.

### D3: Replace `persist_to_seed()` with KG persistence in discovery ingest

In `backend/src/discovery/ingest.rs`:
- Create KG property nodes in `ingest_discoveries()` (currently only creates society nodes)
- Delete `persist_to_seed()`
- Save new property nodes via `knowledge::store::save_node()`

Update `backend/src/routes/search.rs` to use KG persistence instead of `persist_to_seed`.

### D4: Replace seed persistence in registration publish

In `backend/src/routes/registration.rs`, replace seed JSON append with KG property node creation + save.

### D5: Fix stale doc comments

Update all comments referencing seed/bootstrap in:
- `data_loader.rs` — "Properties still load from seed JSON"
- `discovery/ingest.rs` — "Persisted to data/seed/properties.json"
- `knowledge/store.rs` — "Used for bootstrap"
- `routes/enrichment.rs` — "overlays KG facts onto seed data" (if exists)
- `models/marketplace.rs` — bootstrap reference (if exists)

### D6: Tests

1. `test_properties_from_graph()` — property node with known facts → correct Property struct
2. `test_property_sparse_data_defaults()` — minimal node → sensible defaults
3. `test_property_optional_fields()` — None when absent, Some when present
4. Integration test: real KG → properties_from_graph produces > 0

### D7: Clean up `allow(dead_code)` annotations

Use `#[cfg(test)]` instead of `#[allow(dead_code)]` for test-only functions.

## Constraints

- Do NOT delete `data/seed/properties.json` — keep as artifact until Day 78 verifies
- Property struct shape stays the same (API contract)
- Seed has ~155 properties, KG has ~126 nodes — reconcile or accept the delta with a warning
- Run `cargo test` after each deliverable

## Success Criteria

1. `cargo build` succeeds with zero warnings about seed/bootstrap
2. `cargo test` passes with >= 72 tests (69 existing + 3 new)
3. Backend startup shows `Derived N properties from knowledge graph` with N >= 126
4. `data/seed/properties.json` is no longer read at startup
5. Live discovery creates KG property nodes, not seed JSON entries
6. Registration publish creates KG property nodes
7. All stale doc comments updated
8. No unnecessary `allow(dead_code)` annotations
