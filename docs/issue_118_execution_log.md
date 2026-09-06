# Issue 118 execution log

## Goal and invariants

Implement human-like, topology-aware backend search with one compiled logical
plan per request, fail-closed entity resolution, branch-local predicates,
verified evaluation-backed ranking and proof, and stateless typed revisions.
Runtime search remains local and snapshot-only. Product vocabulary and named
entities remain config/DAG-owned.

Correctness blocks promotion; incomplete but honestly reported enrichment does
not. Recall never proves a predicate, required predicates accept only verified
satisfaction, and missing evidence cannot prove spatial relations or negation.

## Checkpoint 0 — preserved experiment baseline

- Recorded: 2026-09-06 Asia/Kolkata
- Branch: `feat/issue-118-spatial-intent`
- Base: `d1c06e2b` (`main` at session start)
- Starting HEAD: `87fe103d` (`feat: compile conversational search revisions`)
- Starting worktree: 38 modified tracked files and 9 untracked files; 2,120
  insertions and 121 deletions before this log.
- Current promoted asset pointer:
  `data/lake/manifests/assets/search_serving_bundle/partition=global/current.json`
- Inspected bundle:
  `data/lake/serving/search_bundle/version=search-proximity-category-v7-2026-09-04`
- Development catalog release expected by API contracts:
  `waterford-osm-arrival-2026-08-31-release`
- Spatial baseline details: `docs/issue_118_spatial_baseline.md`

### Chain audit

- `87fe103d` adds compiled conversational revision semantics, query-bank cases,
  and search-intent config.
- The preserved uncommitted experiment adds locality collection/materialization,
  serving topology and geometry, spatial evaluation, stateless revision routing,
  and expanded contracts.
- The current promoted bundle has no area entities or topology. This is a
  `data_gap`; it is not evidence for the new spatial runtime.
- No new blocked parser aliases were found. The experiment does not change the
  hardcoding-audit total.
- The API revision tests depend on external local catalog state and are not yet
  hermetic. Classified as an `architecture_gap` in the test/runtime boundary.

### Baseline gates

- `cargo check`: passed. macOS linker emitted the repository's existing
  compact-unwind size warning for two binaries.
- `cargo test --test search_conversational_semantics_contract`: 7 passed.
- `cargo test --test search_revision_api_contract`: 3 failed before assertions
  because `OPENESTATES_SERVING_ENV=dev` has no catalog release pointer.
- `python3 -m unittest pipeline.test_osm_locality_boundaries`: 2 passed.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, 0 blocked search-config aliases. The base branch also reports
  330 findings, so the delta is zero.
- `git diff --check`: passed.

### Decisions and classified gaps

- Preserve the prior experiment as a local safety checkpoint before modifying
  it. Do not push.
- Fix the revision API test fixture to construct a pinned, hermetic runtime;
  do not relax serving-bundle startup validation.
- Keep recommendation-coordinate coverage at its existing independent gate.
  The recorded 88.4% coverage is a non-blocking pre-existing `data_gap` for
  Issue 118.

### Next exact command

After the safety commit, rerun:

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --test search_revision_api_contract
```

Then inspect the test runtime construction and replace its external catalog
dependency with a pinned fixture.
