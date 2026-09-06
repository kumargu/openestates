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

## Checkpoint 1–3 foundation — verified evaluation and OSM correctness

- Recorded: 2026-09-06 Asia/Kolkata
- Safety base: `42bfff34`
- Worktree: `feat/issue-118-spatial-intent`
- Candidate serving bundle: not generated yet. The promoted bundle remains
  `search-proximity-category-v7-2026-09-04` and has no area topology.

### Implemented

- Replaced revision API tests' developer-catalog dependency with a hermetic,
  pinned fixture runtime.
- Added four-state predicate evaluation (`Satisfied`, `Unsatisfied`, `Unknown`,
  `Unsupported`) with fail-closed Boolean composition. Negation preserves
  `Unknown` and `Unsupported`.
- Added verified spatial evaluation before ranking. Required containment,
  adjacency, and bounded distance predicates cannot pass from recall alone.
- Added footprint-aware geometry distance with explicit point-fallback metric
  identity and proof records projected from verified matches.
- Added typed inventory-option evaluation so BHK and price come from one
  configuration observation.
- Added semantic-contract digesting to runtime identity and an initial
  `CompiledSearchPlan` representation.
- Added typed revision patch variants and fail-closed clarification for
  untargeted multi-branch budget corrections.
- Moved the OSM broad-region bbox, admin levels, and output mode into the DAG
  source-adapter config. The collector now requires `out body geom` and retains
  OSM relation member type, ref, and role.
- Added a captured real OSM fixture for relation `19883493`; its seven member
  ways assemble into one closed polygon. The fixture is test evidence only and
  is not runtime/parser vocabulary.
- Updated frozen cases where unqualified `near` is intentionally soft. Explicit
  bounds, `inside`, and `adjacent` remain hard.

### Gates

- `python3 -m unittest pipeline.test_osm_locality_boundaries`: 4 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `cargo test --lib assets::locality`: 1 passed (712 filtered).
- `cargo test --test search_conversational_semantics_contract`: 8 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- `cargo test --test serving_bundle_contract`: 3 passed.
- `git diff --check`: passed.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, 0 blocked aliases;
  delta from base remains zero.
- Existing macOS compact-unwind linker warning remains unchanged.

### Correctness decisions and gaps

- `near` without a numeric bound is a ranking preference; non-proximate homes
  remain eligible. This is an explicit contract correction.
- The compiled-plan data type exists, but branch execution still uses legacy
  raw-string splitting and reparsing. Classified as an `architecture_gap` and
  the next blocking implementation slice.
- Revision patches are typed at classification time but still lower through
  string construction. Direct compiled-plan patch application remains open.
- No East Bengaluru candidate bundle or coverage report exists yet. This is a
  pending Checkpoint 3 deliverable, not a claim about the promoted bundle.

### Next exact command

Inspect and replace the raw branch execution path beginning at:

```bash
sed -n '150,940p' backend/src/search/engine.rs
```

Compile branch structure before candidate recall, execute prepared branches,
and keep topology grouping presentation-only so the logical root remains
`Any`.

## Checkpoint 3A — OSM society footprints and offline topology

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `2f0aefba`

### Implemented

- Converged OSM society footprint materialization from the legacy
  `society.boundary_geojson` key onto canonical `geo.geometry_geojson`.
  Fact-registry scopes and the existing scene anchor now consume the same fact.
- Corrected society Overpass collection to request `out body center geom`, so
  multipolygon member ways are present for assembly.
- Added config-owned topology thresholds for society overlap, area
  containment, point confidence, ambiguity, and same-level adjacency.
- Topology candidate recall now uses the geometry R-tree and exact polygon
  operations. It derives society/place memberships, nested area ancestry, and
  same-admin-level adjacency without treating nested areas as adjacent.
- Society containment preserves valid memberships at multiple administrative
  levels while failing closed on competing same-level ambiguity.
- Nearby-place materialization uses society footprint bounds for R-tree recall
  and exact footprint-to-point distance when a polygon exists. Trusted
  coordinate distance remains the explicit fallback when no footprint exists.
- Spatial topology diagnostics now record missing administrative levels in
  addition to missing geometry and ambiguous relationships.

### Gates

- `python3 -m unittest pipeline.test_osm_access_corridors pipeline.test_osm_locality_boundaries`:
  16 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `cargo test --lib serving::`: 50 passed.
- `cargo test --test osm_access_asset_contract`: 3 passed.
- `git diff --check`: passed.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, 0 blocked aliases;
  delta remains zero.

### Proof and remaining gap

- A regression uses a society polygon whose point anchor is outside the
  configured nearby radius. The R-tree envelope recall plus exact footprint
  distance still derives the edge-place relationship at 0.5 km.
- Collection remains enrichment-limited: OSM may not contain a named polygon
  for every canonical society. Those societies retain trusted point fallback
  and are reported as coverage gaps; no footprint is fabricated.
- Candidate East Bengaluru bundle generation and direct Parquet inspection
  remain pending.

### Next exact command

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test \
  --test search_conversational_semantics_contract \
  --test serving_bundle_contract
```

Then replace raw-string branch execution with prepared compiled branches.

## Checkpoint 2 — compile once and preserve logical branches

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `ce50c216`

### Implemented

- The engine compiles the top-level query once, derives its discourse layout
  from that plan, and prepares each logical branch before recall. Prepared
  branches carry their parsed intent, resolved serving entities, spatial query,
  compiled predicates, and fail-closed resolution state into execution.
- Candidate execution no longer invokes the parser, and aggregate result
  construction no longer reparses the raw query after ranking.
- Connected topology does not collapse `or` alternatives. Combined plans keep
  an `Any` root, execute every branch independently, and preserve the branch's
  local BHK, budget, evidence, and spatial predicates.
- Result traversal remains fair and top-level result IDs remain unique, while
  each result set retains membership in every branch the property matched.
- Search cache entries now retain the authoritative compiled plan. Revision
  branch counts come from that logical plan rather than result sets or legacy
  flattened AST projections.
- Multi-branch budget corrections fail closed unless they target an ordinal
  branch or explicitly say `all`/`both`. The typed replacement patch records
  the selected branch ID.
- Fixed broad local recall accidentally bypassing config-owned spatial pruning.
  The final candidate set applies `broad_local_recall_multiplier` and
  `broad_local_recall_min_extra` before property-index lookup, so a 128-entity
  named-place recall does not rank a 5,000-property structured union.

### Gates

- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `cargo test --lib search::query_plan::tests`: 23 passed.
- `cargo test --lib search::revision::tests`: 10 passed.
- `cargo test --test search_efficiency_contract`: 11 passed, including the
  10,000-property named-place cohort under its 750 ms measured search budget.
- `cargo test --test search_conversational_semantics_contract`: 8 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- `cargo test --test serving_bundle_contract`: 3 passed.
- `cargo fmt --check` and `git diff --check`: passed.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, 0 blocked aliases;
  delta remains zero.
- Existing macOS compact-unwind linker warning remains unchanged.

### Remaining gaps

- Revision classification emits typed patches, but candidate revisions still
  lower to a canonical query string and rerun normal search. Direct patching of
  an authenticated compiled parent plan remains a Checkpoint 5 gap.
- Area-only alternatives do not yet inherit one unambiguous parent branch's
  non-spatial predicates.
- Same-name place resolution still needs explicit area-context disambiguation,
  including the rule that an unknown specific place cannot fall back to a
  similarly named area.
- Candidate East Bengaluru bundle generation, direct Parquet inspection, and
  the topology coverage report remain pending.

### Next exact command

Add controlled resolver scenarios for a same-name hospital in multiple areas
and for a missing specific place whose normalized name overlaps an area. Then
make typed entity-family and same-branch area context resolve those cases
deterministically and fail closed when ambiguity remains.

## Checkpoint 2B — topology-aware, fail-closed place resolution

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `529a7d24`

### Implemented

- Added controlled contracts with two canonical hospitals sharing the same
  display name in different sourced areas. `Manipal Hospital in Whitefield`
  resolves only the entity connected to Whitefield by `in_area`; the unscoped
  name remains ambiguous and executes no candidate.
- Specific place identity is now separated from its optional `in <area>`
  context before lookup. Entity-family compatibility is applied before
  topology context, and ambiguity fails closed after both steps.
- A missing specific identity such as `Hoodi Metro` no longer falls back to
  the `Hoodi` area or to arbitrary members of the metro family. Generic
  requests such as `near metro` and contextual family requests such as
  `my office in Marathahalli` retain their configured category behavior.
- Unresolved spatial targets remain on the geo query as diagnostics. Recall
  returns no candidate whenever any required identity remains unresolved,
  preventing partial execution of a multi-clause spatial request.
- Serving topology now exposes sourced, transitive area scopes for resolution.
  It follows only `in_area` edges and never infers identity from coordinate
  proximity or name similarity.
- Area tokens embedded in a specific place name are no longer promoted to
  standalone area constraints. Explicit branch area mentions and configured
  `in <area>` context remain available for disambiguation.

### Gates

- `cargo test --lib search::geo::tests`: 26 passed.
- `cargo test --lib serving::spatial_index::tests`: 5 passed.
- `cargo test --test search_conversational_semantics_contract`: 10 passed.
- `cargo test --test search_efficiency_contract`: 11 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- `cargo test --test serving_bundle_contract`: 3 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed without new
  Rust warnings.
- `cargo fmt`, `git diff --check`, and the hardcoding audit passed. Audit
  remains 330 findings, 28 fact-key comparisons, and 0 blocked aliases.

### Next exact command

Inspect the four-state predicate contracts and freeze required `Unknown` plus
negated `Unknown` scenarios at the compiled Boolean/evaluation boundary. Then
ensure recall-only evidence cannot enter `VerifiedMatch`, ranking proof, or
negation success.

## Checkpoint 4A — four-state eligibility and inventory receipts

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `42e11f88`

### Implemented

- Replaced the remaining boolean-only non-spatial eligibility gate with
  four-state Boolean evaluation. Missing BHK, price, area, entity-index, or
  required fact evidence evaluates as `Unknown`; only `Satisfied` reaches
  ranking.
- Added a regression proving both a required unknown BHK predicate and its
  negation remain `Unknown`. Negation can no longer turn absent inventory
  evidence into eligibility.
- Kept spatial AST terms separate from recall: the engine's exact spatial
  evaluator must accept hard spatial predicates before TextSearch runs.
  Candidate membership is never inspected as evidence.
- Added BHK and price `VerifiedMatch` receipts from the same typed
  `InventoryOption`. Both carry the same inventory-option observation ID,
  preventing a BHK from one configuration being paired with another option's
  price.
- Price-range eligibility records the actual endpoint used: maximum price for
  a minimum-bound query and minimum price for a maximum-bound query. The
  metric names distinguish those cases.
- Inventory receipts and exact spatial receipts are merged. Spatial proof no
  longer overwrites the BHK/price evidence attached to a result.

### Gates

- `cargo test --lib search::text::tests`: 72 passed.
- `cargo test --lib search::evaluation::tests`: 2 passed.
- `cargo test --test search_conversational_semantics_contract`: 10 passed,
  including same-observation BHK/price plus sourced-containment proof.
- `cargo test --test search_efficiency_contract`: 11 passed.
- `cargo test --test search_revision_api_contract`: 3 passed, including
  identical direct/revised ordering and proof.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed without new
  Rust warnings.
- `cargo fmt`, `git diff --check`, and the hardcoding audit passed. Audit
  remains 330 findings, 28 fact-key comparisons, and 0 blocked aliases.

### Remaining gap

- Required configured evidence constraints now fail closed, including under
  negation, but their `Failed` versus `Unknown` distinction and structured
  `VerifiedMatch` projection still use the legacy evidence matcher. Migrate
  that family when touching its proof path; do not weaken current eligibility.

### Next exact command

Inspect revision lowering and implement area-only `AddAlternative` against one
unambiguous parent branch. It must copy that branch's non-spatial predicates;
multiple plausible parent branches must clarify without executing a candidate.
