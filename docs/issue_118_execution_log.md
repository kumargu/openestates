# Issue 118 execution log

## Consolidated continuation plan — Issues 118, 123, and 124

- Reset recorded: 2026-09-06 Asia/Kolkata
- Continuation branch: `feat/issue-118-consolidated`
- Continuation HEAD: Checkpoint 13 (`refactor: require exact spatial evaluations`)
- Preserved source branch: `feat/issue-118-spatial-intent`
- Merge base: `d1c06e2b` (`main` at the start of Issue 118 work)
- Worktree at reset: clean

This document is the single source of memory for the remaining Issue 118 work.
The detailed checkpoint history below remains authoritative for what each
local commit implemented and proved. This section supersedes the older
checkpoint-local next steps where they differ from the consolidated plan.

### Completed local commits

1. `87fe103d` — compile conversational search revisions.
2. `42bfff34` — preserve the original spatial experiment and baseline.
3. `2f0aefba` — add four-state, fail-closed search evidence foundations.
4. `ce50c216` — derive OSM footprint topology offline.
5. `529a7d24` — compile branches before execution and preserve logical ORs.
6. `42e11f88` — resolve named places through sourced area context.
7. `0461a73c` — fail closed on unknown inventory predicates.
8. `e81cc950` — generalize predicate-family revision selection.
9. `d1203aea` — share typed branch predicate replacement.
10. `dcd7abbc` — consolidate Issue 118 execution memory.
11. `88501c65` — pin search construction to one runtime snapshot.
12. `c574e092` — define and validate durable evidence identities.
13. `820c6b95` — preserve validated observations in serving fact rows.
14. `cb7db89d` — trace external-listing observations through the DAG.
15. `59246b97` — qualify inventory search evidence with snapshot-owned receipts.
16. `72fe4bb6` — focus Issue 118 test investment without weakening the query bank.
17. `2ea0f2ec` — remove timestamp-derived confidence from search ranking.
18. `0df8f26c` — unify inventory eligibility and proof projection around
    snapshot-owned receipts.
19. Checkpoint 13 (current log commit) — remove automatic spatial success from
    the shared Boolean evaluator.

### Pulled-forward commitment audit

| Commitment | Current state |
|---|---|
| Snapshot-only search construction (#123) | Implemented for `SearchEngine`: its only constructor input is one `SearchRuntimeSnapshot`; the lower-level parallel `TextSearch` evaluator remains to be removed. |
| Four-state evaluation (#123) | Inventory and required spatial predicates now flow through the shared Boolean evaluator. Area/entity/evidence predicates still have empty-success paths. |
| Stable spatial/price/BHK evidence references (#123) | BHK/price verified matches share one validated, snapshot-qualified external-listing `EvidenceRef`; spatial matches still emit synthesized legacy observation strings. |
| Semantic-contract digest (#123) | Partial: config inputs are hashed, but resolved bindings and evaluator/algorithm versions are not comprehensive. |
| Capability/evaluator/proof bindings (#123) | Incomplete. |
| Touched mixed-state and duplicate-path removal (#123) | Inventory duplicate evaluation/projection and automatic spatial success are removed; other duplicate paths remain. |
| OSM locality and society geometry (#124) | Implemented and fixture-tested. |
| Google/OSM canonical identity (#124) | Incomplete: provider-independent place/locality crosswalks are not fully materialized. |
| Area hierarchy and topology (#124) | Implemented synthetically; not yet verified in a real candidate bundle. |
| Footprint containment and distance (#124) | Implemented, but a separate spatial evaluation path remains. |
| Typed spatial derivations (#124) | Partial: durable lineage and observation identities are missing. |

### Architectural lessons and non-negotiable gates

- Recall returns candidate IDs; it never proves a predicate.
- Required predicates accept only `Satisfied(VerifiedMatch)` and negated
  `Unknown` remains `Unknown`.
- Buyer text is compiled once under one pinned runtime snapshot. Revisions
  authenticate and patch that compiled plan; they do not reparse a projected
  query string.
- Product vocabulary, aliases, category meaning, capability eligibility,
  scoring policy, and proof destinations remain config/DAG-owned. Rust owns
  only structural Boolean, comparison, geometry, and evaluation mechanics.
- BHK and price must be verified by the same inventory observation.
- Observation times are provenance metadata only. Timestamp ordering, elapsed
  time, freshness decay, and age windows cannot affect search eligibility,
  matching, confidence, scoring, or ranking.
- Spatial recall uses an index followed by exact geometry evaluation. Polygon
  holes and all MultiPolygon parts must survive materialization and serving.
- A trusted point is an explicitly labelled distance fallback only. It cannot
  prove whole-footprint containment, intersection, or adjacency.
- Provider identities are retained, but runtime aliases come only from
  validated serving crosswalk records. Ambiguous identity candidates remain
  separate and diagnostic.
- Logical alternatives execute independently. Presentation topology may group
  branches but cannot coalesce them. Deduplication happens only after
  evaluation and retains all matching branch proof sets.
- Coverage gaps are informational. Corrupt geometry, invalid merges, dangling
  or cross-snapshot evidence, invalid derivations, collector defects, and
  advertised capabilities without evaluator/proof bindings block promotion.
- Test investment is integration-first while the architecture is moving:
  preserve the frozen query-bank contract, one vertical DAG/serving contract,
  smoke coverage, and the hardcoding audit. Add unit tests only for compact,
  stable algorithms or safety invariants (identity validation, four-state
  Boolean logic, exact geometry), not to duplicate evolving fixture plumbing.
  Keep existing tests and update shared fixtures when plumbing changes; do not
  clone fixtures or weaken product assertions merely to make a test green.

### Remaining implementation plan

#### 1. Contract and identity foundation

- Introduce durable `ObservationId`, `DerivationId`, and snapshot-qualified
  `EvidenceRef` contracts. Source observations retain provider identity,
  subject, observation time, source URL, and asset lineage. Derived relations
  retain algorithm version and input evidence references. No identifier may be
  fabricated solely from property ID, fact key, or query text.
- Version serving records so these identities survive facts, inventory options,
  spatial relations, runtime evaluation, result membership, and API proofs.
- Make one `Arc<SearchRuntimeSnapshot>` (or a validated context owning it) the
  only construction input for search. Remove independent property/index/bundle/
  society/graph generation assembly.
- Compile a capability catalog from DAG config: capability ID, subject/target
  scope, operators, value/unit rules, eligible observations, resolution policy,
  evaluator binding, proof projection, and evidence destination.
- Reject advertised capabilities without evaluator and proof bindings. Treat a
  missing per-entity observation as coverage, not a schema error.
- Derive the semantic digest from resolved capability bindings, relation
  definitions, eligibility policies, proof bindings, and evaluator/algorithm
  versions.

#### 2. Canonical entities and spatial evidence

- Materialize provider-independent areas, places, and societies offline while
  retaining OSM and Google identities. Merge only when type, normalized
  identity, market region, parent/spatial compatibility, and uniqueness agree;
  place crosswalks also require configured category, area context, and
  coordinate tolerance.
- Preserve complete Polygon/MultiPolygon geometry with holes end-to-end.
- Materialize typed containment, same-level adjacency, footprint distance, and
  explicitly labelled trusted-point fallback derivations. Every derivation
  records subject, target, metric, typed value/unit, input references,
  algorithm version, confidence, snapshot identity, and semantic contract.
- Keep gate access, route geometry/travel time, full-footprint Google
  collection, internal amenities, drain/lake/power migration, and UI/map/
  imagery deferred. Requests for those predicates return `Unsupported`.

#### 3. One generic compiled search path

- Converge onto two authoritative predicate shapes:
  `CapabilityPredicate` and `EntityRelationPredicate`, each with stable
  predicate identity, typed operator/value/unit, required state, source span,
  and explicit resolution state where applicable.
- Compile generic category clauses even without a named entity, preserving
  unresolved and ambiguous targets.
- Execute one `CompiledSearchPlan` against its pinned snapshot. One evaluator
  traverses the Boolean tree for spatial, inventory, identity, and configured
  evidence predicates and emits four-state results plus verified matches.
- Rank and project proofs exclusively from those evaluations. Remove raw
  discourse branch-query construction, connected-scope execution coalescing,
  automatic spatial success, string relation dispatch, empty satisfied
  evidence, duplicate Boolean compatibility matchers, and runtime aliases
  superseded by serving crosswalks after replacement gates pass.

#### 4. Typed stateless revisions

- Recompile and authenticate `parentQuery` under the requested pinned runtime.
  Apply typed revision patches directly to predicate and branch IDs in that
  parent plan, then execute the patched plan without reparsing
  `candidate_query`.
- Render deterministic `activeQuery` only for the public response and HMAC
  revision identity. Preserve existing target/clarification rules, branch and
  revision limits, area-only inheritance, and non-execution for clarification
  or checkpoint outcomes.
- Delete string-editing helpers only after direct and revised equivalents have
  identical ordering, branch membership, metrics, values, observation
  references, and proofs.

#### 5. East Bengaluru candidate and final gates

- Add config-owned pacing, retry/backoff, `Retry-After`, bounded output, and
  honest failure diagnostics to offline OSM society collection.
- Clone the existing lake into an isolated copy-on-write Issue 118 candidate;
  never mutate the main lake and never push or promote remotely. Force fresh
  locality and society OSM assets, then rebuild `current_project_facts`,
  `kg_society_view`, and `search_serving_bundle`.
- Extend the existing Parquet profiler with canonical/provider entities and
  aliases; observation/derivation IDs; geometry type/validity; containment,
  adjacency, and proximity edges; ambiguous identities/topology gaps;
  capability coverage and verified real examples; checksums, lineage, bundle
  size, and semantic digest.
- Final contracts cover evidence survival through Parquet/runtime/API, row-
  order stability, invalid/cross-subject/cross-snapshot reference rejection,
  holes and multipart geometry, area/place identity separation, distinct
  spatial metrics/fallback, Boolean and branch-local semantics through 3/8/16
  branches, direct/revised equivalence, hardcoding audit stability, Rust and
  Python tests, smoke tests, candidate validation, latency/memory benchmarks,
  formatting, and `git diff --check`.

Each checkpoint updates this log, runs its focused contract plus Cargo check and
the hardcoding audit, and creates one coherent local commit. A correctness
failure stops further stacking; honest coverage gaps do not.

### Candidate identity at reset

- Issue 118 candidate lake: not created.
- Issue 118 candidate bundle: not generated.
- Main local lake must remain read-only for candidate work.
- Current main-lake serving pointer (inspection only):
  `data/lake/manifests/assets/search_serving_bundle/partition=global/current.json`
- Current pointed version: `search-proximity-category-v7-2026-09-04`
- Current materialization: `653cd6fd-c41e-412e-9764-208bfba07240`
- Current run: `4790e79e-3fe1-4b05-bc16-4279f3cfab30`
- The current bundle has no verified Issue 118 area-topology coverage and is
  not an Issue 118 candidate.

### Current verified gate summary

The detailed commands and counts remain recorded in the checkpoints below.
Through the Checkpoint 13 working tree, focused Rust search and serving
contracts, the frozen query bank, all-target Cargo check, formatting, and
`git diff --check` pass. The hardcoding audit remains at 330 findings, 28
fact-key comparisons, and zero blocked aliases, with zero delta from the
recorded base. These gates must be rerun for every touched slice.

### Exact next command

```bash
sed -n '180,330p' backend/src/serving/evidence.rs
sed -n '1,240p' backend/src/serving/topology.rs
sed -n '1080,1150p' backend/src/search/geo.rs
rg -n "observation_ids|evidence_refs|DerivedEvidence" backend/src/serving backend/src/search
```

Replace synthesized spatial observation strings with snapshot-qualified
`DerivedEvidence` references backed by the exact input geometry observations.
Keep containment, adjacency, footprint distance, and point fallback distinct.

## Checkpoint 11 — timestamps removed from search confidence

- Parent commit: `72fe4bb6`
- Classified issue: ranking `architecture_gap`

### Implemented

- Removed fact age, ingestion recency, and bulk-timestamp heuristics from graph
  and serving confidence scores. Search ranking can no longer change as wall
  time passes or because rows were ingested together.
- Rebalanced the remaining source quality, evidence coverage, match quality,
  and fact quality components to sum to one.
- Kept observation timestamps on evidence records for provenance. Existing
  timestamp-focused tests were retained and changed to assert that timestamps
  do not affect confidence.
- Recorded the integration-first test policy and the prohibition on timestamp-
  based product logic in `AGENTS.md`.

### Gates

- Focused confidence/search unit gate: 24 passed.
- Frozen conversational query bank: 10 passed with unchanged expectations.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, zero blocked aliases;
  delta remains zero.
- `cargo fmt` and `git diff --check`: passed.
- The first focused run found only a test assertion type mismatch, not a
  product bug; the production API type was left unchanged.

### Candidate identity

No Issue 118 candidate lake or bundle exists. No lake pointer changed.

### Next exact command

```bash
sed -n '1,240p' backend/src/search/evaluation.rs
sed -n '1,240p' backend/src/search/text.rs
rg -n "InventoryOption::from_property|BooleanEvaluation::satisfied\(Vec::new" backend/src/search
```

Move inventory predicate evaluation and verified-match construction into the
shared four-state evaluator. Pass the snapshot's observed inventory options
through `TextSearch`, retain those exact matches on result cards, and remove
the later duplicate inventory proof projection from `SearchEngine`.

## Checkpoint 12 — unified inventory predicate evaluation

- Parent commit: `2ea0f2ec`
- Classified issue: inventory evaluator `architecture_gap`

### Implemented

- Added one receipt-aware BHK and budget evaluator to `InventoryOption`. It
  validates property, canonical society, snapshot, and evidence identity before
  returning a four-state result.
- `TextSearch` now receives the snapshot inventory map and evaluates hard BHK
  and budget predicates from it. Missing, mismatched, legacy, or cross-snapshot
  receipts remain `Unknown` and cannot pass positive or negated eligibility.
- The exact verified matches produced during eligibility now flow directly to
  result cards. Removed the later `SearchEngine` inventory proof reconstruction
  and the unused raw-property `InventoryOption` constructor/matchers.
- Final branch membership uses the same receipt context, so result-set grouping
  cannot re-admit a property using raw card fields.
- Updated existing unit and integration fixtures to provide controlled
  receipts through shared helpers. No tests were added or removed.

### Gates and test value

- Frozen conversational query bank: 10 passed with unchanged expectations.
- Search efficiency contract: initially caught a real missing-receipt gap in
  its `SearchRuntimeSnapshot` fixture; after fixing that shared fixture, 11
  passed.
- Search quality contract: 5 passed.
- Existing `TextSearch` tests: 70 passed and two explanation tests exposed a
  canonical society-ID mismatch in their fixture input. After aligning those
  two inputs, both focused reruns passed.
- Existing inventory projection and no-KG route tests passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed
  without warnings.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, zero blocked aliases;
  delta remains zero.
- `cargo fmt` and `git diff --check`: passed.

### Candidate identity

No Issue 118 candidate lake or bundle exists. No lake pointer changed.

### Next exact command

```bash
sed -n '540,730p' backend/src/search/engine.rs
sed -n '3140,3280p' backend/src/search/text.rs
rg -n "ConstraintTerm::Spatial|BooleanEvaluation::satisfied\(Vec::new" backend/src/search
```

Pass the already-computed exact spatial evaluations into the same `TextSearch`
Boolean traversal. Remove automatic empty spatial success while preserving the
query-bank branch ordering and exact geometry gate.

## Checkpoint 13 — exact spatial evaluation in the shared path

- Parent commit: `0df8f26c`
- Classified issue: spatial evaluator `architecture_gap`

### Implemented

- Replaced `TextSearch`'s automatic spatial success with exact verified matches
  computed from the pinned spatial and fact indexes.
- Hard spatial terms now participate in the same Boolean traversal as inventory
  terms. A missing, wrong-target, wrong-relation, wrong-subject, or wrong-
  snapshot match is `Unknown` and cannot pass eligibility.
- Removed optional spatial terms from the hard-eligibility AST. Their verified
  evaluations remain attached to result proof/ranking data without excluding
  otherwise valid homes.
- Removed the post-search spatial proof merge. The matches used by evaluation
  now flow directly into result cards.
- Renamed the paired runtime input to `SearchEvaluationContext`; it owns the
  snapshot identity plus inventory and spatial evaluations without independent
  runtime assembly.

### Gates and test value

- The first frozen query-bank run caught a real generic bug: optional named-
  place intent had been compiled as hard eligibility and masked by the old
  automatic-success branch. After separating optional evidence from hard
  predicates, all 10 query-bank contracts passed unchanged.
- Search efficiency contract: 11 passed, including the large spatial corpus.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed
  without warnings.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, zero blocked aliases;
  delta remains zero.
- `cargo fmt` and `git diff --check`: passed.

### Deliberate boundary

Spatial evaluation still emits synthesized legacy `observation_ids` and no
snapshot-qualified `EvidenceRef`. Durable derived spatial evidence is the next
blocking identity step.

### Candidate identity

No Issue 118 candidate lake or bundle exists. No lake pointer changed.

### Next exact command

```bash
sed -n '180,330p' backend/src/serving/evidence.rs
sed -n '1,240p' backend/src/serving/topology.rs
sed -n '1080,1150p' backend/src/search/geo.rs
rg -n "observation_ids|evidence_refs|DerivedEvidence" backend/src/serving backend/src/search
```

Replace synthesized spatial observation strings with snapshot-qualified
`DerivedEvidence` references backed by the exact input geometry observations.
Keep containment, adjacency, footprint distance, and point fallback distinct.

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

## Plan reset — generic predicate architecture before more features

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `0461a73c`
- Current uncommitted experiment: area-only revision alternatives and their API
  contracts. This work passes its focused gates but is not ready to commit.

### Why the plan changed

The first area-alternative implementation added an area-specific Boolean-tree
walker. That solves one example while creating the wrong extension point for
budget, BHK, evidence, society, builder, and spatial revisions. This is an
`architecture_gap`, not accepted implementation. Do not add another
family-specific AST traversal.

Issue 118 now gates continued feature work on one typed predicate architecture:

1. `ConstraintTerm` owns its structural predicate family and optional source
   span.
2. `ConstraintExpr` owns the single polarity-aware traversal for selecting
   spans or terms by predicate family.
3. Revision patches identify predicate families with typed values, never
   strings such as `"price"`.
4. Area-only alternatives use the generic location-scope family selection
   (`area`, `society`, and `spatial`) and clarify when more than one distinct
   positive scope exists.
5. Spatial resolution preserves the parser's source span through the resolved
   clause and compiled AST, so `near`, `inside`, and future relation revisions
   use the same machinery as BHK and budget.
6. Recall, evaluation, ranking, and proof consume compiled predicates and
   `VerifiedMatch`; they must not introduce product-vocabulary branches or
   predicate-family copies of Boolean semantics.

### Hardcoding and cleanup rule

When touched Issue 118 code contains a family-specific tree walk, string family
tag, raw-query branch mutation, always-true predicate evaluation, or duplicated
proof construction, either replace it with the generic typed path in the same
checkpoint or record it as a blocking removal item. Named areas, societies,
places, and aliases remain DAG/serving data, never Rust or parser config.

Abstraction stays structural rather than speculative: code may distinguish
generic protocol families (`budget`, `spatial`, `evidence`), Boolean operators,
metrics, and evaluation states. It may not distinguish named buyer vocabulary,
localities, place categories, or fact keys in control flow.

### Reloaded checkpoints

1. Add generic predicate-family, polarity, source-span, selection, and patch
   operations to the AST, with shared tests for BHK, budget, area, society,
   evidence, and spatial terms.
2. Refactor compiled revisions onto those operations; delete the area-only
   walker, string family tags, and superseded string-surgery paths as their
   typed replacements pass.
3. Converge recall, exact evaluation, ranking, and proof on the same compiled
   predicate and verified-evidence contracts.
4. Generate and inspect the East Bengaluru candidate DAG bundle and topology
   coverage without adding named production hardcoding.
5. Run touched-path cleanup plus the full correctness, API, DAG, hardcoding,
   benchmark, smoke, and diff gates.

### Baseline before the reset

- `cargo test --lib search::revision::tests`: 12 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Next exact command

Add the generic family/polarity/span selection API in
`backend/src/search/ast.rs`, carry relation target spans into spatial AST terms,
replace `collect_positive_area_spans`, and prove the same API across predicate
families before changing revision behavior further.

## Checkpoint 5A — generic predicate selection foundation

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `0461a73c`

### Implemented

- Added typed structural `PredicateFamily` and `PredicatePolarity` contracts.
  Every `ConstraintTerm` now exposes its family and optional source span.
- Added one distinct, polarity-aware `ConstraintExpr::source_spans_for` traversal.
  It handles `And`, `AnyOf`, nested `Not`, and double negation for every current
  predicate family.
- Replaced the compiler's parallel family enum with a small compilation key
  built on the authoritative `PredicateFamily`. Evidence-field discrimination
  remains internal and config-derived.
- Carried relation target spans through resolved geo clauses into compiled
  spatial terms. Named-place and area relations can now participate in the same
  revision machinery as BHK, budget, entity, and evidence predicates.
- Deleted `collect_positive_area_spans`. Area-only alternatives select the
  generic positive location-scope family set (`Area`, `Society`, `Spatial`),
  deduplicate overlapping term spans, and clarify when a branch has multiple
  distinct scopes.
- Replaced revision patch family strings such as `"price"` with typed
  `PredicateFamily::Budget` values.
- Preserved the single-parent area-alternative behavior and fail-closed
  multi-parent behavior from the uncommitted experiment.

### Gates

- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `cargo test --lib search::ast::tests`: 30 passed.
- `cargo test --lib search::geo::tests`: 26 passed.
- `cargo test --lib search::revision::tests`: 14 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- `cargo test --test search_conversational_semantics_contract`: 10 passed.
- `cargo test --test search_efficiency_contract`: 11 passed.
- `./tests/smoke_test.sh`: 53 passed against the pinned local development
  release `waterford-osm-arrival-2026-08-31-release` using the main worktree's
  lake URL. The feature worktree intentionally contains no local lake assets.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, 0 blocked aliases; delta remains zero.
- `git diff --check`: passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Remaining architecture gap

Revision classification now carries typed family identities and uses generic
AST selection, but candidate execution still lowers through edited canonical
query text before ordinary search recompiles it. Replace this with typed patch
application to the authenticated compiled parent plan, retaining canonical
`activeQuery` only as the deterministic public projection. Delete the replaced
string-surgery helpers when that gate passes.

### Next exact command

Design the smallest branch-level typed patch operation on `IntentBranch` /
`CompiledSearchPlan`, then migrate budget replacement and alternative copying
one at a time. Rerun direct-versus-revised ordering and proof equivalence after
each migration; do not add predicate-family-specific AST walkers.

## Checkpoint 5B — shared branch predicate replacement

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `e81cc950`

### Implemented

- Removed budget revision's independent raw-parent parser walk and branch-span
  filtering.
- Added one byte-safe branch replacement path driven by
  `ConstraintExpr::source_spans_for`. Budget replacement and location-scope
  alternatives now share the same family/polarity selection, reverse-ordered
  source editing, bounds validation, and canonical whitespace normalization.
- Legacy callers without an authenticated runtime plan now compile a temporary
  branch plan through the ordinary query planner. The API route continues to
  use the authenticated parent search's snapshot-pinned compiled plan.
- Canonical revised queries are projected from compiled branch queries in
  logical order. A targeted budget revision edits only its selected branch;
  `both`/`all` requires a compatible budget predicate in every selected branch
  and otherwise fails closed.

### Simplify review

The focused simplify pass found the duplicate budget parser walk as the one
material cleanup. The typed predicate enums, compilation discriminator, and
polarity-aware AST traversal each own distinct responsibilities and were
retained. No speculative named-entity or fact-key abstraction was added.

### Gates

- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `cargo test --lib search::revision::tests`: 14 passed.
- `cargo test --test search_revision_api_contract`: 3 passed.
- `cargo test --test search_conversational_semantics_contract`: 10 passed.
- `cargo test --test search_efficiency_contract`: 11 passed.
- `git diff --check`: passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Remaining architecture gap

Typed revision classification and source replacement are now generic, but the
candidate is still recompiled from the deterministic `activeQuery` projection.
Direct mutation/execution of a compiled plan requires spatial predicates to
carry all evaluator inputs (including bounds/metrics and generic category
clauses), so that work must not be faked by treating the current incomplete
spatial term as authoritative.

### Next exact command

Extend the generic spatial predicate model to retain relation metric, optional
bound, resolution state, and category/capability identity. Then make prepared
branch execution reconstruct only runtime indexes from those predicates; do
not reparse buyer text or add place-family branches in Rust.

## Checkpoint 6 — snapshot-only search engine construction

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `dcd7abbc`
- Classified miss: `architecture_gap`
- Baseline executable contract: `data/validation/search_query_bank.json`

### Implemented

- Replaced the public mixed-field `SearchEngine` assembly surface with one
  constructor that accepts a validated `SearchRuntimeSnapshot`.
- Search now reads properties, property lookup, local index, bundle indexes,
  societies, society names, facts, capabilities, and runtime identity from the
  same snapshot. The serving bundle is no longer optional on the engine path.
- Removed the separately supplied knowledge graph from ranking. The runtime
  graph remains a post-search context for best-effort learning-gap logs; it
  cannot change eligibility, ordering, or proof for a pinned snapshot.
- Migrated serving, efficiency, conversational, API, and engine fixtures to
  construct complete snapshots. A fixture regression that omitted snapshot-
  owned society names failed the frozen branch contract and was corrected by
  populating typed society records rather than adding a side channel.
- Search runtime diagnostics and verified-match snapshot identity now read the
  snapshot version key used by the cache, instead of independently consulting
  optional engine fields.

### Gates

- Baseline before the change:
  - `cargo test --test serving_runtime_contract`: 5 passed.
  - `cargo test --test search_conversational_semantics_contract`: 10 passed.
  - `cargo test --test search_efficiency_contract`: 11 passed.
- After the change:
  - `cargo test --lib search::engine::tests`: 39 passed.
  - `cargo test --test serving_runtime_contract`: 5 passed.
  - `cargo test --test search_conversational_semantics_contract`: 10 passed.
  - `cargo test --test search_efficiency_contract`: 11 passed.
  - `cargo test --test search_revision_api_contract`: 3 passed.
  - `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
  - `./tests/smoke_test.sh`: 53 passed against local bundle
    `waterford-osm-arrival-2026-08-31-release`.
  - `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
    comparisons, zero blocked aliases; delta remains zero.
  - `cargo fmt --check` and `git diff --check`: passed.
- The existing macOS compact-unwind linker warning remains unchanged.

### Remaining gaps

- `SearchRuntimeSnapshot::new` still accepts pre-built components. That is the
  validated snapshot boundary, not an engine bypass; candidate bundle loading
  and reload atomically publish the completed value.
- Dynamic graph context still exists in `compute_search` for logging only. If
  future response fields consume it, graph generation must become snapshot-
  owned before those fields can affect the public contract.
- Durable observation/derivation identity, comprehensive capability bindings,
  the unified evaluator, and typed direct revision execution remain open.
- Candidate identity remains unchanged: no isolated Issue 118 lake or bundle
  exists yet.

### Next exact command

```bash
rg -n "struct ServingFactRecord|struct ServingEdgeRecord|InventoryOption|VerifiedMatch|observation_ids|evidence_reference" backend/src backend/tests
sed -n '1,260p' backend/src/serving/types.rs
sed -n '1,240p' backend/src/search/evaluation.rs
sed -n '780,930p' backend/src/search/engine.rs
sed -n '1040,1160p' backend/src/search/geo.rs
```

Define typed `ObservationId`, `DerivationId`, and snapshot-qualified
`EvidenceRef` validation first. Do not rename synthesized strings into typed
wrappers and call them durable; producers must carry provider/source lineage or
derived input references before their receipts can verify a predicate.

## Checkpoint 7 — typed evidence identity validation

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `88501c65`
- Classified miss: `proof_gap` with a serving-schema `architecture_gap`

### Implemented

- Added opaque, content-addressed `ObservationId` and `DerivationId` types.
- Added snapshot- and subject-qualified `EvidenceRef` with fail-closed
  validation for expected subject and runtime snapshot.
- Added `SourceObservation`, which requires provider identity, provider record
  identity, subject, observation time, and non-empty asset lineage while
  retaining its source URL.
- Added `DerivedEvidence`, which requires subject, optional target, typed
  relation/metric strings, algorithm version, and at least one input evidence
  reference from the same snapshot.
- Canonicalized derivation input order before hashing so Parquet row order
  cannot change derivation identity.
- Added record validation that recomputes content IDs and rejects tampered
  identity fields. Raw deserialization alone is not proof; candidate loading
  must call these validators when the records enter the serving schema.

### Deliberate boundary

Existing `VerifiedMatch.observation_ids` remain legacy synthesized strings in
this checkpoint. They were not wrapped or relabelled as durable observations.
Inventory, geometry, topology, and proximity producers must first emit actual
source/derivation records with asset lineage; only then may evaluation and API
proof switch to `EvidenceRef`.

### Gates

- `cargo test --lib serving::evidence::tests`: 5 passed, covering stable
  provider-qualified observation identity, tamper rejection, subject/snapshot
  mismatch rejection, required derivation inputs, cross-snapshot rejection,
  and row-order-invariant derivation identity.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, zero blocked aliases; delta remains zero.
- `cargo fmt --check` and `git diff --check`: passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Candidate identity

No candidate lake or bundle exists. The current main-lake pointer remains
inspection-only and unchanged.

### Next exact command

```bash
rg -n "write_facts_parquet|read_facts_parquet|ServingBundleSchema|format_version|ServingFactRecord" backend/src/serving backend/src/assets backend/tests/serving_bundle_contract.rs
sed -n '1,360p' backend/src/serving/parquet.rs
sed -n '1,260p' backend/src/serving/builder.rs
```

Version the serving schema and add validated observation records or explicit
observation columns to facts/inventory without forcing fabricated identities
onto legacy bundles. Candidate validation must reject an advertised evidence
capability whose selected rows lack valid identity/lineage; migration coverage
for old rows remains informational until that capability is advertised.

## Checkpoint 8 — serving fact observation schema

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `c574e092`
- Classified miss: serving-schema `architecture_gap`

### Implemented

- Bumped newly built serving bundles from format 8 to format 9.
- Added an optional validated `SourceObservation` to `ServingFactRecord` and an
  `observation_json` Parquet column declared as a source-observation record in
  the v9 schema descriptor.
- Revalidated content identity and fact-subject binding before Parquet writes,
  after Parquet reads, and during serving-record construction. Invalid or
  cross-subject identities now block bundle construction/loading.
- Preserved backward compatibility: v8 and legacy Parquet omit the new column
  and load with `observation: None`. Existing fixtures and producers were
  migrated to explicit `None`; no synthetic legacy identifier was promoted.
- Proved a provider-qualified source observation survives a fact Parquet round
  trip unchanged and that cross-subject observation binding fails closed.

### Deliberate boundary

Format 9 provides the validated transport but does not advertise evidence
capabilities and does not fabricate coverage. The current graph-to-serving
projection lacks provider record identity plus asset lineage, so it still emits
`None`. The first real producer must be migrated upstream before runtime
evaluation or API proof may consume `EvidenceRef` from these rows.

### Gates

- Baseline `cargo test --test serving_bundle_contract`: 3 passed.
- `cargo test --lib serving::parquet::tests`: 2 passed.
- `cargo test --test serving_bundle_contract`: 3 passed, including legacy
  compatibility and a newly built format 9 bundle.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `./tests/smoke_test.sh`: 53 passed against the local
  `waterford-osm-arrival-2026-08-31-release` v8 bundle, proving old bundles
  remain loadable.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, zero blocked aliases; delta remains zero.
- `cargo fmt --check` and `git diff --check`: passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Candidate identity

No candidate lake or bundle exists. The main local lake was read only for the
v8 smoke test and its serving pointer remains unchanged.

### Next exact command

```bash
rg -n "KgViewFactRecord|write_facts_parquet|read_facts_parquet|source_url|learned_at" backend/src/assets/kg_view.rs backend/src/assets/compaction.rs backend/src/assets/skill_facts.rs
sed -n '360,520p' backend/src/assets/kg_view.rs
sed -n '660,760p' backend/src/assets/kg_view.rs
sed -n '880,990p' backend/src/assets/kg_view.rs
```

Trace upstream fact provenance into `KgViewFactRecord` and identify the first
producer that already has a genuine provider record ID and asset lineage.
Migrate that producer through the v9 observation column without deriving an ID
from only entity/fact/query strings. Keep all other facts explicitly `None`.

## Checkpoint 9 — external listing observation lineage

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `820c6b95`
- Classified miss: `proof_gap` in the offline fact lineage chain

### Implemented

- Versioned skill-fact and KG-view fact schemas from format 2 to format 3 with
  optional observation provider, provider observation ID, and asset-lineage
  columns. Older Parquet rows remain readable as explicit empty provenance.
- Defined an immutable external-listing source-record identity from the full
  raw observation payload, including provider, source URL, values, and
  observation time. It is not derived from a property ID, fact key, or query.
- Attached the raw listing materialization ID and each hashed raw artifact to
  every fact derived from that listing observation.
- Preserved those fields through skill-fact Parquet, current-project-fact
  compaction, KG-view Parquet, and serving construction. Partial provenance now
  blocks serving construction instead of silently becoming evidence.
- Constructed a validated `SourceObservation` only after the complete tuple
  reaches the serving builder. Other producers remain explicit `None`.
- Extended the vertical DAG contract to prove BHK and price facts reach the
  loaded serving bundle with the same observation ID and intact artifact and
  materialization lineage.

### Deliberate boundary

Search `InventoryOption` still fabricates `inventory-option:{property_id}` and
`VerifiedMatch.observation_ids` is still a legacy string vector. This checkpoint
establishes the durable fact source needed to remove that path; it does not
mislabel the old evaluator output as verified evidence. Spatial derivations are
also unchanged.

### Gates

- `cargo check --all-targets`: passed.
- Asset provenance unit tests: 5 passed across external-listing emission,
  skill-fact Parquet, KG-view Parquet, and serving observation construction.
- `cargo test --test skill_facts_asset_contract`: 5 passed.
- `cargo test --test project_enrichment_vertical_contract`: 1 passed with the
  new complete observation-lineage assertion.
- `cargo test --test kg_society_view_contract`: 2 passed.
- `cargo test --test serving_bundle_contract`: 3 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check`: passed.
- `./tests/smoke_test.sh`: 53 passed against the unchanged local v8 serving
  release.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, zero blocked aliases; delta remains zero.
- `cargo fmt --check` and `git diff --check`: passed.
- Existing macOS compact-unwind linker warning remains unchanged.

### Process note

The serving v9 contract from Checkpoint 8 was the baseline for this lineage
slice, but the focused upstream asset contracts were first run after the edit.
The next inventory-evaluator checkpoint must run its focused baseline before
any code change.

### Candidate identity

No candidate lake or bundle exists. The main local lake was read only for the
legacy serving smoke test; no current pointer changed.

### Next exact command

```bash
rg -n "InventoryOption::from_property|verified_inventory_matches|inventory_verified_match|listing_[0-9].*bhk|listing_price" backend/src/search backend/src/data_loader.rs backend/tests
sed -n '1,130p' backend/src/search/evaluation.rs
sed -n '800,920p' backend/src/search/engine.rs
sed -n '350,460p' backend/src/data_loader.rs
```

Replace synthesized inventory evidence with snapshot-qualified references to
the exact external-listing observation retained on serving facts. BHK and price
must resolve from the same observation; legacy rows remain `Unknown`, not
verified. Baseline the focused inventory contracts before editing.

## Checkpoint 10 — snapshot-qualified inventory receipts

- Recorded: 2026-09-06 Asia/Kolkata
- Parent commit: `cb7db89d`
- Classified miss: inventory `proof_gap` with a remaining evaluator
  `architecture_gap`

### Implemented

- Added snapshot-owned inventory-option materialization from the exact
  external-listing JSON observation retained in serving facts. An option is
  eligible only when BHK and both price bounds agree with the runtime property
  on one source observation.
- Selected multiple matching observations deterministically by durable
  observation ID, so serving-row order cannot change the chosen receipt.
- Rejected missing, tampered, cross-subject, and snapshot-mismatched evidence
  references before constructing a verified match. Legacy serving facts with
  no observation remain ineligible for verified inventory receipts.
- Added `EvidenceRef` projection to `VerifiedMatch`. Inventory BHK and price
  matches use the same reference, with the canonical society as evidence
  subject and the runtime property as target. The old
  `inventory-option:{property_id}` string is no longer emitted.
- Precomputed observed inventory options in `SearchRuntimeSnapshot`; request
  execution only reads the pinned snapshot. Exact property society identities
  remain usable when the serving canonical ID already equals the property node
  ID, while canonical crosswalk mappings still take precedence.
- Updated the controlled conversational fixture to carry genuine,
  content-addressed inventory observations without changing recall input. The
  frozen query bank and ordered branch results remain unchanged.

### Deliberate boundary

`TextSearch` still evaluates required BHK and budget predicates from projected
`Property` fields through `InventoryOption::from_property`, and its Boolean
compatibility path can return satisfied evaluations without verified evidence.
This checkpoint removes fabricated proof output; it does not yet claim unified
fail-closed inventory eligibility. Spatial matches also still carry synthesized
legacy observation strings and no `EvidenceRef`.

### Gates

- `cargo test --lib search::evaluation::`: 5 passed, including row-order
  invariance, snapshot/subject qualification, legacy rejection, and rejection
  when BHK and price occur on different observations.
- Focused inventory match projection test: 1 passed, proving BHK and price use
  the same validated reference and a cross-snapshot reference cannot verify.
- `cargo test --lib search::engine::tests`: 40 passed.
- `cargo test --test search_conversational_semantics_contract`: 10 passed with
  unchanged frozen ordered results and durable inventory receipt assertions.
- `cargo test --test search_efficiency_contract`: 11 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed.
- `./tests/smoke_test.sh`: 53 passed against the unchanged local v8 catalog
  release after explicitly starting `openestates-api`.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, zero blocked aliases; delta remains zero.
- `cargo fmt` and `git diff --check`: passed.
- The repository's existing macOS compact-unwind linker warning remains.

### Process note

The continuation began with the inventory patch already dirty, so its focused
baseline could not be reconstructed without discarding carried-over work. The
patch was compiled first, then the missing focused contracts were added and
the frozen before/after expectations were preserved. No correctness failure
was stacked.

### Candidate identity

No Issue 118 candidate lake or bundle exists. The main local lake was read only
for the legacy serving smoke test; no current pointer changed.

### Next exact command

```bash
sed -n '3200,3420p' backend/src/search/text.rs
sed -n '140,240p' backend/src/search/ast.rs
rg -n "BooleanEvaluation::satisfied\(Vec::new|InventoryOption::from_property|property_matches_constraint_term" backend/src/search
```

Replace the parallel property-field inventory eligibility path with the
snapshot's observed inventory options and four-state evaluation. Required BHK
and budget predicates must accept only validated matches projected from the
same `EvidenceRef`; preserve the frozen ordered results with observed fixture
receipts rather than empty satisfied evaluations.
