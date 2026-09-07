# Issue 118 execution log

## Consolidated continuation plan — Issues 118, 123, and 124

- Reset recorded: 2026-09-06 Asia/Kolkata
- Continuation branch: `feat/issue-118-consolidated`
- Continuation HEAD: `0d7dcfc2` (`refactor: remove home-state time inference`)
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
19. `c867fd5a` — remove automatic spatial success from the shared Boolean
    evaluator.
20. `1b19ac4c` — qualify spatial matches with durable observations and typed
    derivations.
21. `9b9d9820` — persist qualified containment and adjacency derivations on
    serving edges.
22. `0d7dcfc2` — remove date-derived home state and age facts.
23. Checkpoint 17 (current working tree) — extend the validated 115-property
    catalog with 27 Whitefield societies in an isolated lake.

### Pulled-forward commitment audit

| Commitment | Current state |
|---|---|
| Snapshot-only search construction (#123) | Implemented for `SearchEngine`: its only constructor input is one `SearchRuntimeSnapshot`; the lower-level parallel `TextSearch` evaluator remains to be removed. |
| Four-state evaluation (#123) | Inventory and required spatial predicates now flow through the shared Boolean evaluator. Area/entity/evidence predicates still have empty-success paths. |
| Stable spatial/price/BHK evidence references (#123) | BHK/price share one validated external-listing reference. Containment and adjacency now consume snapshot-qualified offline edge derivations; footprint and point distance derivations remain runtime projections. |
| Semantic-contract digest (#123) | Partial: config inputs are hashed, but resolved bindings and evaluator/algorithm versions are not comprehensive. |
| Capability/evaluator/proof bindings (#123) | Incomplete. |
| Touched mixed-state and duplicate-path removal (#123) | Inventory duplicate evaluation/projection and automatic spatial success are removed; other duplicate paths remain. |
| OSM locality and society geometry (#124) | Implemented and fixture-tested. |
| Google/OSM canonical identity (#124) | Incomplete: provider-independent place/locality crosswalks are not fully materialized. |
| Area hierarchy and topology (#124) | Implemented synthetically; not yet verified in a real candidate bundle. |
| Footprint containment and distance (#124) | Implemented, but a separate spatial evaluation path remains. |
| Typed spatial derivations (#124) | Partial: containment and adjacency derivations are stored on v10 serving edges and survive Parquet/runtime/proof. Distance derivations are not yet stored offline. |

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
- Observation times are provenance metadata only. Product/domain code cannot
  compare, subtract, or order timestamps to infer age, freshness, current
  state, eligibility, matching, confidence, scoring, or ranking.
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

### Current candidate identity

- Isolated candidate lake:
  `/tmp/openestates-issue118-whitefield-add30-lake`.
- Main lake remained untouched and no serving or catalog pointer was promoted.
- Main local lake must remain read-only for candidate work.
- The dangling main-lake serving pointer was removed after its bundle was
  deleted. No main-lake search serving bundle is currently promoted.
- The surviving 115-property baseline selected for this candidate is
  `catalog-115-rera-ten-seed-2026-08-10-v2`.
- Baseline materialization: `764b65da-2173-479f-8411-95efee5e4f34`.
- Baseline run: `8eadc96f-3019-44f3-9cc1-db3425c40e44`.
- Baseline contents: 706 entities, 14,094 facts, and 3,574 edges in serving
  format 6. It is a legacy input corpus, not itself an Issue 118 candidate.
- Candidate objective: retain that 115-property corpus and add 20–30 unique,
  real Whitefield societies selected from canonical/RERA Parquet. Image and
  media completeness are non-blocking because this candidate measures search.
- Identity, geometry, inventory validity, durable evidence, deterministic
  ordering, and proof correctness remain blocking gates.
- Source seed:
  `data/validation/source_entities/whitefield_search_add30_2026_09_06.json`.
- Successful scoped DAG run: `233ebd12-59bc-4d32-a276-f752ead8661d`.
- Scoped serving input: `cb466721-a46c-40b3-9f24-047c8dc80318`, containing
  27 eligible societies, 55 property configurations, 2,786 facts, 27 RERA
  evidence rows, and 60 durable inventory observations. Mahaveer Willet,
  Prestige Dolce Vita, and Republic of Whitefield were honestly quarantined
  for missing size evidence.
- Merged serving candidate: `9d722f44-ac3f-4199-b890-43e18674ee13`, version
  `issue-118-whitefield-115-plus-27-2026-09-06-r2`.
- Merged contents: 170 properties across 78 runtime societies, exactly the 115
  baseline properties plus 55 new configurations across 27 new societies.
- Draft catalog release: `7c875ae9-41bf-42e3-93a8-d23740a68c84`.
  Complete serving-artifact validation passes. Catalog validation remains
  rejected because five legacy property IDs lost their old unsupported
  `-3bhk` suffix under the current projection and the isolated lake's copied
  current dependency pointers do not match this scoped DAG run. Do not hide
  either issue with tombstones or pointer promotion.

### Current verified gate summary

The detailed commands and counts remain recorded in the checkpoints below.
The merged candidate starts through the real API with 170 properties. Plain
2BHK and 3BHK Whitefield searches each return 16 matches with same-observation
BHK/price proofs. Named Prestige Lakeside Habitat search returns exactly one
matching configuration. A two-branch Whitefield 2BHK-or-3BHK query returns 32
unique results without coalescing branches. The live guardrail query bank
passes 30/30 at 27.71 ms endpoint p95. The frozen conversational contract
passes 10/10 and the revision API contract passes 3/3.

Live revision probes exposed gaps not covered by those controlled fixtures:
`Make it 2BHK under 1.6Cr` is treated as a switch and drops inherited
Whitefield context (`intent_gap`), while `Make it under 2Cr` appends a second
budget branch instead of replacing the bound (`architecture_gap`). Do not call
live chained search equivalent to direct search until both are fixed and added
to the shared query bank.

### Exact next command

```bash
python3 scripts/audit_search_hardcoding.py
cd backend && CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets
cd .. && git diff --check
```

After these gates and the pinned-bundle smoke run, commit the candidate slice.
The following checkpoint must then diagnose typed revision replacement using
the two recorded live failures before making any parser or ranking change.

## Checkpoint 17 — 115-property catalog plus 27 Whitefield societies

- Parent commit: `0d7dcfc2`
- Classified work: offline catalog expansion with legacy validation
  `architecture_gap`s

### Implemented

- Selected 30 real Whitefield RERA projects from canonical source data and ran
  the scoped asset DAG in an isolated lake. Current eligibility retained 27
  societies and quarantined three missing-size projects without requiring
  images.
- Kept K-RERA detail/registration receipts when the optional regulatory-list
  table is unavailable; the absent regulatory coverage remains empty and is
  logged rather than discarding valid receipts.
- Added backward-compatible decoding for legacy RERA evidence rows.
- Extended serving construction with an explicit prevalidated-base scope so a
  stricter current policy does not silently delete properties from a validated
  parent catalog.
- Consolidated only deterministic empty duplicate societies from that pinned
  base. The populated Arvind Bel Air identity was retained, its empty duplicate
  was removed, and non-derived relations were redirected. Populated ambiguity,
  candidate/base name collisions, and derived relations still fail closed.
- Made candidate validation reconstruct grandfathered entities and property IDs
  from the immutable `catalog_base_serving` watermark. Legacy packaged media is
  ignored only for those pinned base entities; any new media remains subject to
  content-addressed lake validation.

### Candidate and gates

- Scoped DAG and merged candidate identities are recorded in the current
  candidate section above.
- Exact property arithmetic: 115 base + 55 added = 170.
- Exact runtime-society arithmetic: 51 base + 27 added = 78.
- Merged bundle: 753 entities, 18,556 facts, 20,914 search metadata rows, 5,295
  edges, 35 RERA evidence rows, and zero newly quarantined societies.
- `cargo check --all-targets`: passed without code warnings.
- Complete serving candidate validation: passed.
- Catalog membership projection parity and RERA evidence scope: passed.
- Catalog release remains rejected only for the five explicit legacy ID
  migrations and scoped-vs-current DAG convergence described above.
- Live search and focused integration results are recorded in the current gate
  summary above.

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

## Checkpoint 14 — snapshot-qualified spatial derivations

- Parent commit: `c867fd5a`
- Classified issue: spatial `proof_gap` plus timestamp-selection
  `architecture_gap`

### Implemented

- Replaced synthesized `spatial:{subject}:{relation}:{target}` strings with a
  content-addressed `DerivedEvidence` record. The record retains subject,
  target, relation, metric, typed value/unit, confidence, evaluator version,
  snapshot identity, and sorted source inputs.
- Spatial `VerifiedMatch` now exposes one derivation reference plus the full
  derivation needed to resolve its source observations. Missing, invalid,
  cross-snapshot, or legacy source inputs produce `Unknown`; they cannot verify
  containment, adjacency, footprint distance, point fallback, or a serving
  distance fact.
- Propagated real OSM locality and society-access observation identity into the
  existing skill-fact -> KG-view -> serving-fact path. Provider record identity
  and asset/run lineage are retained; no ID is made from only an entity, fact
  key, or query.
- Kept footprint, adjacency, and point-fallback inputs distinct. Containment
  requires two footprint observations; adjacency requires the society,
  containing area, and target-area footprints; point distance requires
  observations for both points.
- Removed timestamp tie-breaking from coordinate, geometry, category, serving
  projection, bundle de-duplication, property-source, and map-source selection.
  Stable evidence/content identity now breaks equal-confidence ties.
- Removed request-path `days ago` and `Fresh/Recent/Stale` calculations from
  search cards and property details. The optional legacy response field remains
  absent for compatibility; timestamps remain provenance metadata only.
- Kept all existing tests. Extended the frozen containment scenario and the
  existing OSM vertical contract instead of adding another test suite.

### Gates and test value

- Frozen conversational query bank: 10 passed. Its existing containment
  scenario now proves the API match carries a valid derivation with two source
  observations and no synthesized observation string.
- OSM access vertical contract: 3 passed, including observation identity and
  lineage through KG-view rows.
- Search efficiency contract: 11 passed.
- Existing evidence identity tests: 5 passed; existing coordinate-selection
  tests: 3 passed; existing locality lineage test: 1 passed. These confirmed
  stable invariants but did not expose a production bug in this slice.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed
  without code warnings.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, zero blocked aliases;
  delta remains zero.
- `cargo fmt --check` and `git diff --check`: passed.
- API smoke: 52/53 passed against the unchanged promoted v8 bundle. The sole
  `3BHK` failure is an honest coverage gap: v8 has no durable inventory
  observations and therefore cannot satisfy the current fail-closed predicate.
  No matcher or smoke assertion was weakened.
- Existing macOS compact-unwind linker warning remains unchanged.

### Deliberate boundary

The spatial derivation is content-addressed and fully projected in runtime
proof, but it is currently constructed from offline relation results and their
source observations when the pinned snapshot is evaluated. Containment and
adjacency edge rows do not yet store that derivation in Parquet. Legacy spatial
facts and edges remain usable for recall only and cannot verify a match.

The temporal scan was kept scoped to serving and request-path product logic.
Operational run-duration/pacing timestamps remain operational metadata.
The user has now made the product decision: offline `home_state` date
arithmetic must also be removed. Source dates remain provenance facts only.

### Candidate identity

No Issue 118 candidate lake or bundle exists. The main local lake was used only
for a read-only v8 smoke test; no current pointer changed.

### Next exact command

```bash
sed -n '35,70p' backend/src/serving/types.rs
sed -n '300,370p' backend/src/serving/parquet.rs
sed -n '1,220p' backend/src/serving/topology.rs
rg -n "derive_spatial_topology|ServingEdgeRecord|write_edges_parquet" backend/src/serving
```

Materialize typed derivation lineage on offline containment and adjacency edge
rows, preserve legacy read compatibility, and reject dangling derivation
inputs during serving validation. Do not add a broad new unit-test suite; use
the existing serving vertical and frozen query-bank contracts.

## Checkpoint 15 — offline containment and adjacency receipts

- Parent commit: `1b19ac4c`
- Classified issue: serving-edge `proof_gap` with legacy replacement
  `architecture_gap`

### Implemented

- Bumped newly built serving bundles from format 9 to format 10 and added an
  optional `derivation_json` column to edge Parquet. Older edge tables remain
  readable as unqualified recall relations.
- Materialized content-addressed `spatial-topology-v2` derivations for polygon
  containment and same-level adjacency from the exact geometry observations.
  Point geometry no longer produces containment; it remains distance-only.
- Replaced a same-key unqualified legacy topology edge with the qualified
  offline relation. A malformed qualified edge is never silently repaired and
  instead blocks build, load, and release validation.
- Validated derivation identity, snapshot, edge subject/target/relation/
  confidence binding, and resolution of every input observation or derivation
  reference. Dangling and cross-subject inputs cannot reach runtime.
- Search containment now consumes the pinned offline edge derivation directly.
  Adjacency composes the exact qualified containment and adjacency receipts.
- Selected duplicate qualified relation evidence by stable derivation identity,
  never row order or timestamp.

### Gates and test value

- Frozen conversational query bank: 10 passed with unchanged ordered results.
- Search efficiency contract: 11 passed.
- Serving bundle contract: 3 passed, including v10 schema and legacy read
  compatibility.
- Existing topology exact-geometry test: 1 passed. Extending it with a legacy
  same-key edge caught the real suppression bug fixed by this checkpoint.
- Existing Parquet evidence tests: 3 passed. The one compact edge round-trip
  assertion protects stable identity transport; it did not expose a new bug.
- Existing release-validation tests: 2 passed.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed.
- Hardcoding audit: 330 findings, 28 fact-key comparisons, zero blocked aliases;
  delta remains zero.
- `cargo fmt`, `git diff --check`, and API smoke completed. Smoke remains 52/53
  against the unchanged promoted v8 bundle; only `3BHK` fails because that
  bundle has no durable inventory observations.
- Existing macOS compact-unwind linker warning remains unchanged.

### Candidate identity

No Issue 118 candidate lake or bundle exists. Smoke loaded the existing
`waterford-osm-arrival-2026-08-31-release` development bundle read-only. The
repository main-lake pointer remains `search-proximity-category-v7-2026-09-04`
with materialization `653cd6fd-c41e-412e-9764-208bfba07240`; no pointer changed.

### Next exact command

```bash
sed -n '1,220p' backend/src/assets/home_state.rs
sed -n '220,560p' backend/src/assets/home_state.rs
rg -n "signed_duration_since|date_naive|learned_at.*[<>]|max_by_key\(.*learned_at|home_age_years|project_age_years" backend/src app/config/dag backend/tests
```

Remove remaining product-state and age inference from dates. Keep explicit
source status and delay facts, preserve timestamps only as metadata, and update
the existing home-state/DAG contracts without creating a new test suite.

## Checkpoint 16 — no date-derived home state or age

- Parent commit: `9b9d9820`
- Classified issue: offline product-state `architecture_gap`

### Implemented

- Removed completion-date versus wall-clock comparison from the
  `home_state_signals` asset. Completion dates remain upstream source facts but
  no longer decide whether a home is delivered or under construction.
- Removed computed `home_age_years`, `project_age_years`, and age buckets.
- Kept only explicit state inputs: RERA status may map to a controlled home
  state, and a positive source `rera_delay_months` may mark delay. No absence of
  delay is promoted to an `on_track` claim.
- Replaced latest-timestamp selection and `max(source_time, run_time)` with
  deterministic source-content identity. Output timestamps record the asset
  run only and do not change product meaning.
- Strengthened `AGENTS.md`: product/domain code cannot compare, subtract, or
  order timestamps. Operational retry/run measurements remain allowed.

### Gates and test value

- Existing home-state tests: 2 passed after being updated to assert explicit
  state/delay behavior and the absence of generated age facts. No new suite or
  test was added.
- Frozen conversational query bank: 10 passed.
- Focused DAG executor vertical reaches its final search assertion, then fails
  with zero results for `3bhk with greenery in whitefield above 10 acres`.
  The exact failure reproduces unchanged at parent commit `9b9d9820`, so this
  checkpoint did not cause it; it remains a pre-existing fail-closed evidence
  gap to diagnose separately.
- `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo check --all-targets`: passed.
- Hardcoding audit remains 330 findings, 28 fact-key comparisons, and zero
  blocked aliases.
- `cargo fmt` and `git diff --check`: passed.

### Candidate identity

No Issue 118 candidate lake or bundle exists. The DAG vertical used an isolated
temporary lake. The main local lake and current pointer were not changed.

### Next exact command

```bash
sed -n '360,420p' backend/src/community.rs
sed -n '3360,3405p' backend/src/routes/properties.rs
sed -n '1700,1740p' backend/src/assets/kg_view.rs
sed -n '520,565p' backend/src/assets/compaction.rs
sed -n '420,455p' backend/src/assets/rera.rs
rg -n "learned_at\s*[<>]|max_by_key\([^\n]*learned_at" backend/src
```

Replace the remaining timestamp-based product-record selection with stable
content/source identity. Keep operational timings and retry scheduling because
they do not create buyer facts.

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

## Canonical spatial identity checkpoint — candidate validation pause

- Recorded: 2026-09-07 Asia/Kolkata
- Starting HEAD: `0d7dcfc2`
- Classification: original Kadugodi miss fixed; remaining `architecture_gap`
  in ranking/proof projection

### Implemented

- Materialized provider-independent canonical place entities with durable
  `has_provider_binding` derivations while retaining Google and OSM source
  entities and their observation identities.
- Restricted cross-provider merging by configured category, normalized name,
  compatible area context, coordinate tolerance, provider difference, and
  uniqueness. Ambiguous candidates remain separate and diagnostic.
- Added config-owned structural spatial roles. Destinations use qualified point
  distance, footprints may use exact geometry, and regions retain topology.
- Kept `Kadugodi Tree Park Metro` and `Kadugodi Tree Park` as distinct
  canonical entities. Google review facts are projected through canonical
  identity without changing their observation IDs.
- Removed provider rows with canonical bindings from runtime place resolution,
  changed proximity targets to canonical IDs, and consolidated duplicate
  nearby-category parsing onto the DAG configuration type.
- Added canonical binding validation, bumped the serving format to 11, and
  added the catalog serving rebuild command used for this candidate.

### Candidate identity

- Isolated lake: `/tmp/openestates-issue118-whitefield-add30-lake`
- Source serving materialization: `edeae319-b74a-450b-999c-ed3b793eb95a`
- Candidate serving materialization: `880a4181-5ae3-4924-a1b1-db11fc445c47`
- Candidate version:
  `issue-118-whitefield-115-plus-27-canonical-spatial-2026-09-07-r1`
- Format 11; 1,346 entities; 509 canonical spatial entities; 536 provider
  bindings; 18,914 facts; 25,835 search metadata rows; 5,332 edges.
- No ambiguous provider merge was reported. The main lake and all promoted
  pointers remain unchanged.

### Search evidence

- `2BHK within 1 km of Kadugodi Tree Park Metro under 1.6 Cr` now returns one
  verified result: `discovered-parimala-skyview-2bhk`, at 0.469 km.
- `homes near Kadugodi Tree Park Metro` returns 32 results against canonical
  target `place:canonical:kadugodi-tree-park-metro:39a43e56418618c5`.
- `homes near Kadugodi Tree Park` returns a different order against canonical
  target `place:canonical:kadugodi-tree-park:7c2801472b0d38a7`.
- Park Google evidence remains 4.4 / 2,903 reviews; metro evidence remains
  4.5 / 1,536 reviews, with distinct durable Google observation IDs.
- `homes near Manipal Hospital Whitefield` resolves to canonical target
  `place:canonical:manipal-hospital-whitefield:d3d6115585ad7524` and returns
  32 verified results.

### Remaining gap and stop reason

Search still has two parallel named-place evidence paths. Hard eligibility and
`VerifiedMatch` use `SpatialServingIndex::distance_between`, but ranking,
explanation, and `ProofFocus` still call
`serving_named_place_evidence_for_entity`, parse distance from free-form
`nearby_*` text, and associate the row to a resolved entity using token overlap.
For `Manipal Hospital Whitefield`, the identity matcher removes the generic
token `hospital`, then incorrectly accepts rows such as `Manipal Hospital EPIP
Whitefield`. It can therefore display and rank with 0.6 km while the exact
canonical entity's verified distance is 3.330 km. This is the old duplicate
evaluation architecture, not missing Google/OSM data.

No further code change was stacked after finding this inconsistency. The next
change must delete the free-text named-place ranking/proof path and project
ranking reasons, display distance, and proof focus from the same typed spatial
evaluation that produced `VerifiedMatch`. Free-text nearby facts may remain
buyer evidence, but cannot resolve a target identity or drive spatial metrics.

### Gates

- Existing controlled conversational query bank: 10 passed before candidate
  rebuild, including the updated generic Google/OSM/park fixture.
- Proximity unit suite: 9 passed.
- Cargo check and library test compilation passed before the isolated rebuild.
- `cargo fmt --all -- --check`, `git diff --check`, and the hardcoding audit
  pass. The audit remains at the existing 330 warnings, 28 fact-key
  comparisons, and zero blocked aliases.
- Candidate artifact validator checked 45 artifacts and found no canonical
  identity, geometry, derivation, or dangling-evidence issue. It rejected the
  inherited 115 catalog on 186 pre-existing completeness/media findings
  (missing card area/size on older shells and retired packaged image paths).
  The gate was not weakened.

### Next exact command

```bash
sed -n '1220,1345p' backend/src/search/text.rs
sed -n '895,1135p' backend/src/search/geo.rs
sed -n '520,610p' backend/src/search/engine.rs
```

Replace `serving_named_place_evidence_for_entity` and the Haversine fallback
projection with an evaluation projection keyed by predicate, property, and
canonical target. Rerun the four live queries above first; ordering,
explanation distance, proof-focus distance, and verified match must agree
before any additional cleanup.

## Continuation audit — authoritative compiled execution cleanup

- Recorded: 2026-09-06 Asia/Kolkata
- Starting HEAD: `0d7dcfc2`
- Classification: `architecture_gap` plus live-bundle `proof_gap`
- Existing dirty candidate/materializer changes are preserved and are not part
  of the search cleanup baseline.

### Chain audit

- `88501c65` pins `SearchEngine` to one runtime snapshot.
- `0df8f26c` unifies inventory eligibility on snapshot-owned observations.
- `c867fd5a`, `1b19ac4c`, and `9b9d9820` require qualified spatial evaluation
  and persist topology derivations.
- Execution still splits and reparses branch strings, reconstructs entity
  occurrences from matched text, rebuilds result branches with
  `ConstraintExpr::flat_branches`, and creates `CompiledSearchPlan` only after
  search has completed.
- Proximity facts and `near_place` edges still lack typed derivations. The live
  Waterford bundle therefore cannot verify a bounded named-place predicate.

### Baseline

- Artifact: `data/validation/search_query_bank.json`, controlled product and
  Issue 118 suites.
- `cargo test --test search_conversational_semantics_contract`: 10 passed.
- `python3 scripts/audit_search_hardcoding.py`: 330 findings, 28 fact-key
  comparisons, zero blocked aliases.
- Live API `waterford-osm-arrival-2026-08-31-release`:
  - `3BHK in Whitefield near Manipal Hospital Whitefield under 2.5 Cr` returns
    two semantically identical branches.
  - `2BHK within 1 km of Kadugodi Tree Park Metro under 1.6 Cr` returns zero.
  - The same Kadugodi query without the bound returns Godrej Splendour 2BHK,
    proving divergent ranking and hard-evaluation paths.

### Current checkpoint

1. Preserve exact resolved occurrence spans and stop reconstructing them from
   query text.
2. Compile branch identity before execution and project result sets from those
   branches without `flat_branches` re-evaluation.
3. Delete connected-scope coalescing and other replaced compatibility helpers.
4. Materialize typed proximity derivations only from qualified geometry or
   coordinate observations; missing inputs remain a coverage gap.
5. Run the same controlled bank and live queries. On any remaining gap, stop
   without stacking another code change.

### Candidate identity

No new Issue 118 serving candidate is promoted. Port 4125 serves the unchanged
`waterford-osm-arrival-2026-08-31-release` baseline and remains read-only.

### Next exact command

```bash
sed -n '70,115p' backend/src/search/engine.rs
sed -n '1580,1840p' backend/src/search/engine.rs
sed -n '1170,1300p' backend/src/search/engine.rs
```

## Authoritative latest cursor — 2026-09-07

The continuation audit immediately above is historical. The canonical spatial
identity checkpoint is the current state: candidate
`880a4181-5ae3-4924-a1b1-db11fc445c47` fixes the Kadugodi bounded-search miss
and preserves distinct park/metro Google evidence, but is paused on the
free-text ranking/proof distance mismatch documented above. Nothing is
promoted.

### Next exact command

```bash
sed -n '1220,1345p' backend/src/search/text.rs
sed -n '895,1135p' backend/src/search/geo.rs
sed -n '520,610p' backend/src/search/engine.rs
```

Delete the parallel free-text named-place metric path. Ranking, explanation,
proof focus, and hard eligibility must project the same typed evaluation for
the same canonical target before further cleanup.

## Branch-only chained-query audit — 2026-09-07

No serving lookup, recall, ranking, or result assertion was run. The audit
executed 11 focused tests directly against the typed query AST compiler.

- Passed 9: paired area/BHK alternatives; repeated BHK and branch-local budget;
  shared suffix scope; branch-local and repeated evidence thresholds;
  cross-dimension evidence alternatives; resolved society pairing; shared
  society prefix; and society-specific budgets.
- Failed 2:
  - `3BHK in East Bengaluru or 4BHK not in East Bengaluru` incorrectly carries
    the positive East Bengaluru scope into the second branch, making it require
    both East Bengaluru and not-East Bengaluru.
  - `Prestige 3BHK or Godrej Air 4BHK` incorrectly carries the Prestige builder
    into the Godrej Air society branch, making the second branch require both
    entities.

The common cause is `compile_constraint_plan`: it groups area, society, and
builder under one `AlternativeFamily::Entity` for branch detection, but carries
missing terms forward independently inside each concrete entity/polarity group.
It therefore cannot see that a later different entity type or opposite-polarity
entity replaces the earlier scope. This is a compiler architecture gap, not a
data or search-ranking issue. No code change was made after the failure.

The full runtime-resolved compiled plan is also not independently callable:
entity and spatial preparation are private steps inside `SearchEngine::search`.
The raw AST compiler can be tested without search, but bundle-backed society and
place resolution currently cannot. A clean compile-only boundary is required
before evaluating the full chained-query bank without executing search.

### Next exact command

```bash
sed -n '930,1185p' backend/src/search/ast.rs
sed -n '160,430p' backend/src/search/engine.rs
```

Replace per-group scope carry-forward with explicit branch ownership and shared
scope in the compiled plan. Then expose preparation as a compile-only operation
returning `CompiledSearchPlan`; search execution must consume that plan rather
than recompile or reparse it.

## Branch compiler ownership fix

The compiler regression is fixed without changing search data, recall, ranking,
or proof behavior.

- BHK parser terms now retain their containing alternative-cluster span as well
  as their exact source span. This keeps an internal alternative such as
  `not 4 or 5 BHK` inside one exclusion while preserving exact predicate spans.
- Branch anchors are selected from the concrete term groups that actually span
  alternatives. A trailing exclusion is therefore shared across the relevant
  branches instead of becoming local to only the last branch.
- Entity scope is inherited only by a branch with no explicit entity scope.
  An explicit area, excluded area, society, or builder starts a new branch scope
  and cannot accidentally accumulate a previous entity type.
- Added one table-driven compiler contract covering bare inheritance, explicit
  society replacement, builder-to-society replacement, positive-to-negative
  area replacement, grouped exclusions, and shared trailing exclusions.

Focused gates:

- `cargo test --lib search::ast::tests`: 31 passed.
- `cargo test --lib search::parser::tests`: 17 passed.
- `cargo test --lib search::query_plan::tests`: 23 passed.
- Frozen controlled product/query-bank contract: passed.
- `cargo check --all-targets`: passed.
- Hardcoding audit: unchanged at 330 findings, 28 fact-key comparisons, and
  zero blocked aliases.
- `cargo fmt` and `git diff --check`: passed.

The complete conversational contract remains 8/10 because the separate dirty
canonical-spatial-identity work rewrites fixture place IDs while two assertions
still expect the old provider IDs. The compiler/query-bank test passes; no
spatial-identity code was changed as part of this fix.

### Next exact command

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --test search_conversational_semantics_contract named_place_resolution_uses_sourced_area_context_and_fails_closed_without_it -- --exact --nocapture
```

## Geography-first compilation checkpoint — 2026-09-07

- Starting HEAD: `0d7dcfc2` with the existing Issue 118 spatial-identity and
  branch-compiler worktree preserved.
- Classification: `architecture_gap`. The promoted facts/config already define
  `society|place -[in_area]-> area` and direct `adjacent_area` topology, while
  search still splits raw query strings, executes them independently, calls
  `CompiledSearchPlan::combine`, and recompiles revision text.
- Relevant chain: `88501c65` pins one runtime snapshot; `0df8f26c` unifies hard
  inventory evaluation; `c867fd5a`, `1b19ac4c`, and `9b9d9820` qualify and
  persist spatial evidence; the dirty AST checkpoint preserves exact source
  spans and branch ownership. Geography-first compilation is the next
  architectural checkpoint, not a new parser or vocabulary layer.
- Config read before implementation: `app/config/dag/manifest.json`,
  `ontology.json`, and `scoring_policy.json`. The existing Google quality
  policy is 75% normalized rating plus 25% log-scaled review count.
- Baseline artifact: `data/validation/search_query_bank.json`.
- Baseline controlled conversational contract: 8 passed, 2 failed. Both known
  failures assert provider place IDs that the in-progress canonical spatial
  identity checkpoint now rewrites; no geography-first assertion has failed
  yet.
- Baseline hardcoding audit: 330 warnings, 28 fact-key comparisons, zero
  blocked aliases.
- Replacement target: compile one snapshot-bound geography-first plan, derive
  areas only from explicit area entities or direct sourced `in_area` edges,
  retain exact place predicates, execute its branches directly, and delete raw
  branch reparsing, connected execution coalescing, and plan combination.

### Verified compiler checkpoint

- `CompiledSearchPlan` now contains `GeoBranch` records with branch-local
  predicates, source spans, resolved handles, `GeoScope`, and stable geo-cluster
  IDs. Positive society predicates are consumed as sourced area anchors and do
  not survive as eligibility or lexical ranking signals.
- The runtime compiles the top-level typed query once. Execution-time discourse
  splitting, ordinal branch-string construction, connected output coalescing,
  and `CompiledSearchPlan::combine` are deleted.
- Direct areas remain explicit. Society/place anchors derive only across direct
  serving `in_area` edges; the most-specific directly evidenced area wins.
  Missing relations fall back to `BundleWide` with internal resolution gaps.
- Clustering requires direct adjacency, the same area, or one shared immediate
  parent. Cluster membership is pairwise, so A-B and B-C cannot transitively
  merge A with C. Recall still uses only explicit member area IDs.
- Focused compiler gate:
  `cargo test --lib search::compiled_plan::tests::geography_first_compiler_contract`
  passed (1 passed). This is the single table-driven compiler contract required
  by the checkpoint.

### Verified execution and frozen-bank checkpoint

- Branch execution now applies each compiled branch's hard BHK, budget, state,
  exclusion, spatial, and evidence predicates before intersecting its sourced
  area scope. Bundle-wide branches retain the full hard-eligible candidate set
  through evaluation and rank valid Google evidence ahead of missing evidence
  with the configured 75/25 rating/review-count policy.
- The budget parser no longer combines money values from separate discourse
  branches merely because a later clause contains `to`; only adjacent configured
  range connectors (or an inline range) combine values. A focused regression
  preserves both conditional branch budgets and genuine `between ... and ...`
  ranges.
- The frozen bank records the explicit product change: society mentions are
  sourced area anchors, not exact-society preferences; missing place `in_area`
  topology falls back bundle-wide; bundle-wide soft preferences remain proof and
  secondary ranking but do not override Google quality order.
- Focused parser regression passed (1 test). The controlled frozen product suite
  passed after all 60 scenarios executed against the controlled inventory.

### Verified revision and API checkpoint

- Area-only revisions retain the complete single-branch source query while
  replacing only the compiled geographic predicate, so BHK and budget clauses
  cannot disappear when an alternative area is added.
- Repeated `in <area> under <budget>` branches now bound each named-area clause
  against its own following budget operator rather than the first operator in
  the full query.
- Revision API intent projection reads the executed `CompiledSearchPlan`
  directly. It returns stable branch and geo-cluster IDs, exact source spans,
  resolved entity handles, and `GeoScope`, including the sourced `in_area`
  derivation edges. Result cards continue to expose predicate evaluations as
  `verifiedMatches` and their corresponding proof focuses.
- Revision tests passed (14/14), revision-route projection tests passed (3/3),
  and the complete conversational semantics contract passed (11/11), including
  controlled journeys and disconnected 3/8/16-branch cohorts.

### Final verification checkpoint

- The search efficiency contract was updated only where old assertions treated
  a society as an exact eligibility filter or depended on pre-ranking truncation
  for a dangling place. It now verifies sourced-area-anchor/bundle-wide fallback,
  branch-local BHK and budget enforcement, and full hard-eligible evaluation;
  all 11 tests passed.
- `cargo check --all-targets`, `cargo fmt --all -- --check`, JSON validation, and
  `git diff --check` passed.
- The final hardcoding audit matches baseline: 330 warning-only findings, 28
  fact-key comparisons, and zero blocked search-config aliases. No new production
  search hardcoding was introduced.
- Removed-path audit found no `CompiledSearchPlan::combine`, raw paired-ordinal
  branch query generation, connected-scope coalescing, or temporary search-plan
  trace output under `backend/src/search` or `backend/src/routes`.

## Exact society priority checkpoint — 2026-09-07

- Product decision: reverse the geography-first checkpoint's treatment of a
  named society as having no ranking preference. A positive named society
  remains a sourced area anchor and never becomes an eligibility filter, but
  an eligible home in that exact society leads its branch before eligible area
  alternatives. Every BHK, budget, state, exclusion, spatial, and evidence
  predicate still applies before this ordering rule.
- Multiple society branches retain their own exact-first ordering; the existing
  cross-branch round-robin then exposes each eligible named society before
  later area alternatives. No separate buyer-facing row or frontend change is
  part of this checkpoint.
- Classification: `ranking_gap`. The compiler already preserves positive
  society handles in each `GeoBranch`; recall and eligibility are correct, but
  branch ranking currently discards that exact identity after using it to
  derive the area.
- Baseline focused contract:
  `geography_first_execution_uses_area_scopes_and_bundle_wide_google_order`
  passed with `geo-alpha-prime` before `geo-air`, confirming the behavior being
  changed. Baseline hardcoding audit remains 330 warning-only findings, 28
  fact-key comparisons, and zero blocked aliases.
- Implementation boundary: add a config-owned exact-society priority policy and
  stable-partition already eligible branch results by the branch's resolved
  positive society IDs. Preserve the existing rank order within the exact and
  alternative cohorts; do not reuse `match_tier` or add project vocabulary.

### Verified exact-priority behavior

- `search_ranking.exact_society_matches_first` owns the product choice. After
  normal branch ranking, execution stable-partitions already eligible results
  by canonical society entity ID, preserving the prior order inside both the
  exact and area-alternative cohorts.
- Focused contracts pass for bare society ordering, canonical alias resolution,
  two directly connected society branches plus one disconnected branch,
  cross-branch round-robin, an exact home rejected by a hard budget, and a
  dangling `in_area` relation that falls back bundle-wide.
- No UI field or `match_tier` meaning changed. Exact identity affects branch
  order only; sourced area recall, predicate evaluation, and proof projection
  remain intact.
- The complete conversational contract passed (12/12), including all 60
  controlled frozen-bank scenarios. Revision tests passed (14/14). The
  efficiency suite exposed one old global-order expectation for two dangling
  society anchors; its branch contents and hard budgets were already correct,
  and the expectation was updated to the chosen exact-first cross-branch
  round-robin order.
- After updating that explicit product expectation, the complete search
  efficiency contract passed (11/11).

### Final exact-priority verification

- Complete conversational semantics contract: 12/12 passed, including the
  frozen controlled bank and journeys.
- Search revision library contract: 14/14 passed.
- Search efficiency contract: 11/11 passed.
- `cargo check --all-targets`, `cargo fmt --all -- --check`, query-bank JSON
  validation, and `git diff --check` passed.
- The hardcoding audit remains identical to baseline: 330 warning-only
  findings, 28 fact-key comparisons, and zero blocked aliases. No frontend or
  buyer-copy change was made.
