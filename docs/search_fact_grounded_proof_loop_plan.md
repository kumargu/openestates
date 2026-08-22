# Search Fact-Grounded Proof Loop

## Objective

Prove that local search retrieves and explains facts already present in a
promoted serving bundle, then improve genuine data gaps through the DAG and
promote a new immutable bundle. The loop must distinguish search defects from
missing data and must never teach the runtime benchmark answers through named
projects, places, aliases, or fact-key branches.

The object under test is the search system: intent compilation, entity
resolution, recall, hard-constraint enforcement, ranking, proof selection, and
detail-proof handoff. Catalog completeness is not the experiment. A society
does not need every buyer-detail field before its valid facts can participate
in a search experiment.

The working method is deliberately open-book:

1. inspect the promoted facts first;
2. write down what the bundle can actually prove;
3. freeze buyer-like queries and expected proof;
4. run search without changing the answers;
5. classify misses;
6. enrich only genuine data gaps through the DAG;
7. validate and promote a new bundle;
8. rerun the identical query set.

## Experiment Boundary

Keep two admission decisions separate:

1. **Search-experiment admission** asks whether an entity and its facts are
   safe to use while measuring search.
2. **Buyer-catalog eligibility** asks whether a home is complete enough for
   normal production result and detail surfaces.

Do not disable buyer-catalog eligibility globally. Build immutable search
experiment bundles under a separate environment or pointer. They may contain
societies that lack media, carpet area, approach-road evidence, or another
unrelated buyer-detail input. They must not silently become the production
buyer catalog.

A society may enter a search experiment when:

- its canonical identity, aliases, property-to-society edges, and runtime
  projection are internally consistent;
- every admitted fact has a typed value, source lineage, and valid search
  metadata when it is intended to influence search;
- at least one minimal property configuration can be projected for runtime
  recall; an explicitly unavailable price is acceptable, but invented values
  are not;
- known conflicting or misclassified facts are excluded from positive proof
  expectations and tracked as data-quality sentinels.

Eligibility is then evaluated per query, not as all-or-nothing society
completeness:

- BHK, budget, named entity, numeric, exclusion, and explicit distance clauses
  remain hard. A result with an unknown or non-matching value does not pass.
- A preference marked `required` must have matching promoted evidence.
- Missing optional evidence is neutral, is recorded internally as `no_data`,
  and must not produce a proof reason.
- Missing facts unrelated to the query must not remove an otherwise valid
  society from the experiment.

Search-experiment activation means starting the runtime with the candidate's
immutable materialization id through
`OPENESTATES_SERVING_MATERIALIZATION_ID`. It does not move the production
catalog/current pointer. Production catalog promotion remains a separate
decision.

## Implementation Under Test

The current branch implements a deterministic local pipeline:

- `QueryPlan` parses spanned BHK, budget, area, relation, distance, and
  configured evidence clauses.
- `CompiledQuery` lowers those clauses into a Boolean constraint AST so grouped
  alternatives and exclusions stay authoritative through recall and ranking.
- `SearchIndex` joins runtime properties to canonical serving society, builder,
  and area entities and performs structured constraint recall.
- `SearchEngine` combines structured, Tantivy, and geo recall; resolves
  serving aliases and ambiguity-safe society typos; abstains on unresolved hard
  named-entity clauses and compact unexplained project-like prefixes; and
  preserves independent result branches. Generic configured preference and
  evidence prose remains searchable rather than being mistaken for a project.
- `TextSearch` enforces the compiled constraints, requires evidence for
  required preferences, treats missing optional facts as `no_data`, ranks from
  serving facts, and creates proof focuses from the same evidence.
- The API exposes buyer-safe `resultSets`, keeps diagnostics and gaps out of
  buyer copy, records enrichment gaps, and caches both successful and
  zero-result responses with the serving version in the cache key.

The earlier FastText shadow classifier is not part of the current branch. It
was removed in favor of deterministic query compilation and configured
ontology resolution; the branch name is historical.

## Verified Starting Point

As of 2026-08-22, the current search serving pointer is:

- bundle: `clean-serving-v7-2026-08-16`
- materialization: `fe16ca1a-a301-4306-ae10-06cc27f792e2`
- run: `cc973fb5-14f5-4242-9cfe-c52ba7738210`
- entities: 697
- facts: 13,310
- graph edges: 3,353
- search metadata rows: 12,969
- quarantined societies: 28
- current runtime projection: 37 eligible societies and 86 searchable
  properties

Older immutable `catalog-118-*` bundles exist and several contain 776 entities
and roughly 15,560 facts. The `118` name describes that catalog generation; it
must not be treated as the current eligible runtime count. Before using it as
an expansion input, profile its society membership, identity mappings,
quarantine status, and lineage directly.

## Implementation Checkpoint — 2026-08-22

### Controlled conversational model checkpoint

The first next-phase milestone is complete. The 10 frozen buyer queries in
`data/validation/query_bank/search_conversational_semantics_v1.json` now run
against the controlled product model in
`backend/tests/search_conversational_semantics_contract.rs`.

The fixture is a durable executable specification, not temporary test data. It
models inventory, canonical places, lifecycle state, nearby facts, optional
evidence, ranking, exclusions, result branches, and proof handoff. New buyer
capabilities should be added to this controlled model before DAG, API, or UI
wiring, then proved separately against a live immutable bundle.

The frozen scenarios now prove:

- independent society, area, BHK, and budget alternatives;
- shared constraints and exclusions across alternatives;
- ordinal references such as `first` and `second` in comparisons;
- `unless` as a scoped budget/lifecycle exclusion;
- named and generic place proximity, including conjunctive `both` proof;
- required lifecycle evidence without turning unrelated negative preferences
  into hard filters;
- optional missing evidence remains eligible and produces no false claim;
- balanced dual-commute ranking and search-to-detail proof labels.

Verification:

- controlled conversational contract: 10/10 queries passed;
- search/unit regression suite: 321 passed;
- search efficiency contract: 11 passed;
- search quality integrations: 9 passed;
- isolated API smoke suite: 50 passed;
- production hardcoding audit: zero blocked search-config aliases.

Commits:

- `86c09aa0` freezes the conversational query semantics;
- `d807947b` adds the executable product model and preservation rule;
- `fcd03cf4` makes the frozen model executable.

### Varied buyer-language checkpoint

The controlled model now also executes the 18 cases in
`data/validation/query_bank/search_buyer_language_v1.json`. Together with the
original bank, 28 frozen buyer searches cover simple constraints,
conversational preferences, society and place typos, compact/broken grammar,
equivalent budget wording, branch alternatives, missing optional evidence,
hard abstention, and false-proof sentinels.

Query-bank consolidation stayed deliberately narrow:

- immutable live-bundle banks remain separate because they record specific
  DAG materializations and data expectations;
- the obsolete `search_intent_classifier_v1.json` bank was removed after the
  FastText shadow path had already been deleted;
- useful human phrases from that classifier experiment now appear inside full
  buyer searches with observable intent, results, exclusions, and proof
  expectations in `search_buyer_language_v1.json`;
- Git history remains the archive for the retired classifier-only corpus.

The new bank exposed two classified `intent_gap` defects:

- `rate it well` was absent from the configured review-quality vocabulary;
- the unresolved-entity guard selected the shorter nested `more than`
  operator inside `costing no more than`, contaminating the area span.

Both fixes are generic. Review language and maximum-budget forms live in DAG
config, while the parser now chooses the closest-ending, longest overlapping
operator. No named project, locality, place, or fact-key branch was added.

Verification:

- controlled product-model banks: 28/28 queries passed;
- `cargo check`: passed;
- full Rust suite: 634 library tests and all integration binaries passed;
- search efficiency contract: 11 passed;
- search quality integrations: 9 passed;
- production hardcoding audit: zero blocked search-config aliases.

Commits:

- `f7ae7f89` freezes and consolidates the varied buyer-language bank;
- `0779365b` makes all 18 new cases executable and fixes their two generic
  intent gaps.

Long-term fixture note: the controlled entities, homes, places, facts, and
missing-evidence sentinels should eventually move from embedded Rust setup into
a versioned generic product-scenario document consumed by search, DAG, API,
detail, compare, and UI journey contracts. That migration is structural work,
not a reason to mix controlled scenarios with live Parquet banks, and it is
deferred so this pass remains focused on search quality.

Next boundary: pause runtime search changes. When work resumes, select a small
subset of these 28 semantic classes for a separate live-DAG transfer bank,
inspect promoted Parquet first, and classify each miss before changing code or
data. Do not expand society count or catalog eligibility as part of that loop.

## Earlier implementation checkpoint — 2026-08-22

Completed and verified:

- added explicit `buyer_catalog` and `search_experiment` serving-admission
  profiles;
- added `app/config/dag/search_experiment_eligibility.json`, requiring only a
  projected property, area, and BHK/configuration for experiment admission;
- stamped manifests and quarantine reports with their admission profile;
- made normal serving builders default to `buyer_catalog` and added explicit
  experiment builder/materializer constructors;
- made buyer-catalog promotion reject search-experiment bundles;
- proved in unit/runtime tests that missing media, size, and builder fields do
  not block an experiment property, while unknown price still fails a hard
  budget;
- removed automatic BHK/budget relaxation from engine, AST, scoring policy,
  benchmark contract, smoke contract, and frontend response types;
- updated the fact-first bank to version 2 so the two former relaxation cases
  are explicit hard-constraint abstentions.

Verification at this checkpoint:

- `cargo check`: passed;
- full Rust suite: passed (625 library tests and all integration binaries);
- Python benchmark/pipeline suite: 90 passed, 1 skipped;
- production search-hardcoding audit: 0 blocked aliases.

The operational boundary is now proved:

- `materialize-search-experiment` builds and validates an immutable,
  unpromoted experiment bundle from a pinned KG materialization;
- `extend-serving --search-experiment` adds an explicitly scoped experiment
  cohort to a validated buyer-catalog baseline without creating a catalog
  release or moving a pointer;
- experiment-profile materializers reject all generic current-pointer
  promotion methods, so activation requires an explicit immutable
  `OPENESTATES_SERVING_MATERIALIZATION_ID` pin;
- a four-society incomplete candidate produced 10 runtime properties and
  passed 76/76 frozen checks at 17.41 ms endpoint p95;
- a mixed 45-society candidate produced 107 runtime properties and passed all
  398/398 checks across the old regression banks plus the incomplete-society
  bank at 20.45 ms endpoint p95.

Result-to-detail proof lineage is also proved for the named-place path. The
benchmark now follows a search focus into the configured detail surface and
checks that the fact, entity, distance, feature, receipt lineage, and immutable
serving version agree. The Hoodi Metro case passed all 13/13 checks at 12.76 ms
endpoint p95.

Bundle isolation is now proved for the current experiment pair. The same
Ajmera query returns zero against South 43, where the entity is absent, and one
exact result against the mixed bundle, where it is present. Synthetic unknown
project names abstain on both bundles. Search cache keys include the immutable
serving version, zero-result entries cannot cross versions, reload clears the
cache, and the South 43 ordered broad-query results remain unchanged when the
mixed bundle is removed.

The next useful boundary is transfer across configured required evidence,
negative risks, and more named-place families. Do not expand society count
again until one of those tests exposes a classified search or data gap.

## Non-Negotiable Invariants

- The promoted serving bundle is search truth. Raw captures are lineage inputs,
  not runtime truth.
- New or refreshed facts enter search only through:
  `source -> DAG materialization -> serving bundle -> validation -> promotion`.
- Every run creates a new immutable bundle version. Never edit a promoted
  Parquet file or reuse a version name.
- Promotion moves the current pointer only after validation. A failed candidate
  leaves the current pointer unchanged.
- Never add `Godrej Air`, `Hoodi`, `Manipal`, Whitefield societies, or other
  benchmark entities as parser aliases or runtime branches.
- Never add expected live-bundle benchmark facts directly to Rust, parser
  config, or search metadata. Controlled product-model fixtures may contain
  explicit mock facts, but they must remain clearly isolated from live-bundle
  banks. Fix the source/DAG path when a live fact is absent.
- BHK, budget, named-society, numeric, exclusion, and distance constraints stay
  hard. Missing soft evidence stays additive unless its config explicitly says
  `required`.
- Search and property detail must cite the same promoted fact and lineage.

## Phase 0 — Freeze the Baseline

Record before any behavior or data change:

- git commit and dirty diff summary;
- bundle version, materialization id, and manifest counts;
- entity-type and society counts;
- quarantine count and reason distribution;
- current search contract results;
- production hardcoding audit result;
- baseline artifact directory and timestamp.

All before/after runs in one loop use the same code unless the classified miss
requires a generic runtime fix. If code changes, rerun the old bundle first so
the data and code effects remain separable.

## Phase 1 — Build the Fact Ledger

Start with one small Whitefield cohort: Godrej Air plus a few nearby societies
that share useful comparison facts. Query the current bundle's `entities`,
`facts`, `edges`, and `search_metadata` Parquet directly.

For every proposed fact, record:

- canonical entity id and aliases carried by the bundle;
- fact key, typed value, unit, confidence, and observation time;
- source type, source URL/locator, and input lineage;
- whether the fact has search metadata;
- whether it is eligible for search ranking and proof in the experiment;
- whether it separately meets normal buyer-catalog display eligibility;
- related place/entity id for proximity evidence;
- expected detail-surface proof handle.

Initial hypotheses to verify, not assume:

- Godrej Air has searchable 2 BHK and 3 BHK configurations;
- its promoted land-area fact supports the claimed acreage;
- a Hoodi Metro place entity and distance fact are linked to the society;
- the intended Manipal Hospital entity is unambiguous and linked by a distance
  fact;
- Whitefield area membership is canonical rather than inferred from text.

Each hypothesis receives one status:

- `proved_and_searchable`
- `proved_but_unannotated`
- `proved_below_threshold`
- `present_under_alias`
- `present_but_ignored`
- `absent_from_bundle`
- `conflicting_or_ambiguous`

Only `proved_and_searchable` facts become positive benchmark expectations.
Absent or ambiguous facts become explicit data-gap sentinels; search must not
claim them.

## Phase 2 — Freeze the Query Bank

Write queries after the ledger is complete and before calling `/api/search`.
Store the query bank with the baseline bundle identity and expected structured
outcomes.

The first bank should contain 15–25 queries across these classes:

1. Exact society recall: society name and a minor typo.
2. Configuration constraints: verified 2 BHK, 3 BHK, and an unsupported
   configuration sentinel.
3. Numeric constraints: verified land-area or price facts, including one
   impossible threshold.
4. Named-place proximity: Hoodi Metro and the exact verified Manipal entity.
5. Compound intent: society or place + BHK + budget + one soft preference.
6. OR branches: two verified configurations or two independently resolvable
   alternatives.
7. Broad Whitefield discovery: fact-backed comparisons across several
   societies.
8. Proof sentinels: facts known to be absent or ambiguous must not produce a
   confident reason.

Expected outcomes must be structured, not prose snapshots:

- expected or forbidden entity ids;
- allowed top-k range;
- exact hard constraints that every returned result must satisfy;
- expected proof fact keys and matched entity ids;
- allowed result tier: `exact` or `supported`;
- whether zero results is correct;
- facts that must not be claimed.

## Phase 3 — Measure Search Quality and Efficiency

Run the frozen bank against the pinned baseline and save the full responses.
Measure:

- recall at 1, 3, and 5;
- mean reciprocal rank for expected societies;
- hard-constraint violation count;
- proof precision: reason key and matched entity agree with the ledger;
- unsupported-claim count;
- OR-branch membership and ordering;
- exact versus explicitly supported-result tier correctness;
- candidate count before and after hard filtering;
- warm request p50 and p95 latency;
- deterministic ordered-result stability across repeated runs.

The warm local-search target remains p95 at or below 50 ms. A loop must not
regress latency by more than 10% without a documented reason and follow-up.

## Phase 4 — Classify Every Miss

Every failed expectation gets exactly one dominant class before any fix:

- `data_gap`: the fact/entity is absent from the promoted bundle;
- `intent_gap`: generic configured language did not compile correctly;
- `proof_gap`: recall is correct but the reason or detail focus is missing or
  wrong;
- `ranking_gap`: the right candidate is eligible but ordered poorly;
- `embedding_gap`: semantic recall failed while structured resolution is sound;
- `architecture_gap`: truth is present but a loader/index/API boundary bypasses
  or loses it.

For every suspected data, recall, or proof gap, inspect Parquet again before
editing runtime code.

## Phase 5 — Fix One Layer at a Time

- `data_gap`: repair or run the relevant source/DAG asset for a small explicit
  entity scope. Preserve source receipts and canonical identity.
- `intent_gap`: adjust generic ontology/config and its compiler consumption;
  never add a named entity shortcut.
- `proof_gap`: repair the generic fact-to-proof contract without filtering
  other detail facts.
- `ranking_gap`: change one generic scoring/tie-break rule and measure the same
  bank immediately.
- `embedding_gap`: change the offline embedding/index path only after structured
  recall has been ruled out.
- `architecture_gap`: fix the earliest boundary that drops existing truth:
  Parquet loader, alias index, spatial index, capability index, scoring, or API
  mapping.

Keep a change only when it improves a declared metric or removes a verified
architecture defect without quality loss. Otherwise revert it or record it as
unproven.

## Phase 6 — Build and Pin a Search Experiment Bundle

The next candidate should deliberately include both complete and incomplete
societies with useful facts. It may reuse entities from historical bundles, but
it must be rebuilt through current DAG policy and current canonical identities.
Do not spend the loop filling unrelated catalog requirements merely to admit a
search test case.

For every candidate:

1. Run the DAG with a unique version and pinned source inputs.
2. Materialize a new KG view and search serving bundle; do not mutate the
   current bundle.
3. Validate artifact hashes, complete lineage, alias consistency, edge
   integrity, search annotations, and search-experiment admission.
4. Publish an admission report that distinguishes invalid identity/fact rows
   from fields that are merely absent and irrelevant to the tested query.
5. Confirm the candidate includes incomplete societies and that each can be
   recalled only for constraints and preferences its facts actually support.
6. Run the frozen query bank and standard contracts against the candidate.
7. Pin the test runtime to the immutable materialization id with
   `OPENESTATES_SERVING_MATERIALIZATION_ID`. Do not advance the production
   buyer-catalog environment as part of this loop.
8. Reload the API and verify that the reported bundle version matches the
   experiment version.

Experiment materialization is append-only progress: the old bundle remains
available for exact rollback and before/after comparison. Experiment bundles
stay unpromoted and are selected by immutable id. A later buyer-catalog release
may independently apply the full eligibility policy.

## Phase 7 — Rerun Without Moving the Goalposts

After promotion, rerun the identical query bank and publish a diff containing:

- ordered result ids per branch;
- changed proof keys and matched entity ids;
- hard-constraint and unsupported-claim deltas;
- recall/MRR and latency deltas;
- society, property, fact, metadata, and quarantine count deltas;
- the decision: `keep`, `rollback`, or `needs_data_follow_up`.

Do not rewrite expectations merely because the new result is convenient. Change
an expectation only when the fact ledger proves that truth itself changed, and
record that lineage.

## Expansion Cadence

Expand fact coverage in controlled cohorts rather than optimizing the eligible
society count:

1. Godrej Air and its Whitefield named-place facts.
2. Five to ten transfer societies with a mixture of complete and incomplete
   buyer-detail projections.
3. Multiple fact families: configuration, price, state, reviews, named places,
   numeric project facts, negative risks, and missing optional evidence.
4. A larger historical cohort for recall, latency, alias, and ordering tests.
5. New regional cohorts only when they add a search behavior or fact family
   that is not already covered.

Each batch produces its own immutable bundle, ledger delta, query-bank result,
and experiment decision. Society count is a diagnostic, not a success metric.

## Next Verification Matrix

Before further catalog enrichment, extend the frozen proof loop with:

1. **Incomplete-society transfer:** missing media, carpet area, price, or an
   unrelated evidence family does not block supported searches; the missing
   field never becomes a claim.
2. **Per-query hard gates:** unknown price, BHK, numeric value, or named-place
   distance cannot satisfy the corresponding clause.
3. **Soft and required evidence:** optional missing evidence remains neutral;
   configured required evidence filters the result; negative no-data handling
   does not become a positive claim.
4. **Fact-family transfer:** named schools, hospitals, metro, tech parks,
   reviews, project state, acreage, water/noise/traffic risks, and builder/RERA
   proof work through generic config and serving facts.
5. **Boolean and language transfer:** grouped OR branches, exclusions, repeated
   constraints, paraphrases, punctuation, minor typos, and ambiguous names keep
   the same constraint semantics.
6. **Proof handoff:** the result reason and detail focus cite the same fact,
   entity handle, value/distance, source, and serving version.
7. **Bundle switching:** cache entries do not cross serving versions, including
   zero-result responses, and rollback restores the prior ordered results.
8. **Scale:** repeat the bank on a materially larger cohort and record complete
   candidate counts, recall, ordering stability, and warm p50/p95.
9. **Data-quality sentinels:** known misclassified facts and absent facts never
   produce confident proof.

Budget behavior is settled for this experiment: explicit budgets and BHKs are
hard, automatic relaxation code and contracts have been removed, and the full
test suite passes. If a future buyer experience offers alternatives, return
them only as an explicit tradeoff result set and never as an exact match.

Progress as of 2026-08-22:

- complete: incomplete-society transfer, hard price/BHK/numeric gates, optional
  missing-evidence neutrality, mixed-cohort regression, and pointer isolation;
- complete: named-place result-to-detail proof handoff;
- complete: bundle-version cache isolation, South 43 rollback stability, and
  generic abstention for project-like names absent from the active bundle;
- next: required-evidence transfer, more fact families and paraphrases;
- later: a
  materially larger cohort, and durable-data corrections for known category
  errors.

## First-Loop Definition of Done

- Godrej Air hypotheses are verified directly from promoted Parquet.
- A frozen 15–25 query bank covers exact, typo, BHK, numeric, proximity,
  compound, OR, broad-area, and negative-control cases.
- Baseline and after artifacts identify the exact bundle and code revision.
- Zero hard-constraint violations and zero unsupported confident claims.
- Expected society recall and proof handles meet the declared top-k targets.
- Warm p95 search remains within 50 ms and within 10% of baseline.
- Any enrichment is DAG-produced with source lineage.
- At least one incomplete society proves the per-query admission behavior.
- A new search experiment bundle is retained and pinned only if validation and
  the frozen bank pass; all pointer promotion is out of scope.
- The old bundle remains available for rollback.
