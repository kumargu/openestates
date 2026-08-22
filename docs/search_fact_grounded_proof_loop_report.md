# Search Fact-Grounded Proof Loop — Progress Report

Date: 2026-08-23

## Current assessment

`controlled_search_regional_consolidation_verified`

One controlled bank now holds the original 10 conversational buyer queries, 18
varied-language queries, 3 multi-decision ranking queries, 9 multi-OR recovery
cases, and 10 regional-mix cases. Its reusable fixture profiles keep scenario
inventory isolated while sharing one typed expectation contract.

The controlled fixtures are now a first-class product specification. They
should drive future search, DAG, API proof, comparison, and UI journey tests.
They must not be deleted or weakened to accommodate current data coverage.

Latest verified checkpoint:

- consolidated controlled search bank: 50/50 scenarios passed;
- clean-serving-v7 bank: 149/149 checks passed;
- full Rust suite: 642 library tests and all integration/doc contracts passed;
- search unit slice: 327 passed;
- search efficiency: 11 passed;
- isolated API smoke suite: 50/50 passed;
- frontend production build: passed;
- CI clippy and all-target/all-feature clippy: passed;
- hardcoding gate: zero vocabulary, fact-key, or blocked-alias findings;
- live benchmark artifacts:
  `/tmp/openestates-search-regional-consolidation.json` and
  `/tmp/openestates-search-regional-consolidation.md`.

The prior post-merge green statement was stale. A fresh independent review ran
the preserved live bank and found 148/149 checks: the branch-dispatch path
split `Godrej Air 2BHK or 3BHK` into one scoped branch and one bare 3BHK branch.
The same review found numeric normalization reversing order immediately above
`1.0`, and buyer-visible branch rails bypassing the internal global result
limit. Those three findings are now fixed and frozen in controlled scenarios.

The result limiter now round-robins across branches, preserves each branch's
rank order, deduplicates property IDs globally, and returns the identical
selected subset to diagnostics and buyer output. Numeric normalization is
strictly monotonic across negative, fractional, boundary, and large values in
both scoring directions. The regressed live query now returns exactly two
homes: Godrej Air 2BHK and Godrej Air 3BHK.

The final audit tightened the executable contract without changing its buyer
decisions. Numeric ranking now preserves exact direction within configured
threshold buckets, named-place typos reuse the existing edit-distance matcher,
and search no longer projects `builder_delivery_rate` through a fact-key branch.
The hardcoding audit is now a tested failing gate for production product-fact
comparisons.

The branch is synchronized with `origin/main` at `f245e7fc`. The merge keeps
main's expanded gallery curation together with alias-aware local media lookup;
the full post-merge backend and API suites pass. No remote branch was pushed.

The stale classifier-only bank and four fragmented controlled banks were
replaced by `search_product_scenarios_v1.json`. Historical live-bundle banks
were preserved; they describe pinned data runs and should not be merged into
the controlled product model. Selected classifier-era phrases now have
complete search context and executable result/proof expectations instead of
isolated labels.

The regional profile adds three price tiers or configurations per region and
three coordinate-backed metro anchors. Its first diagnostic run found that
`2BHK or 3BHK near Nagawara Metro under 1.8 Cr` leaked unrelated 2BHKs from
other regions even though the named North Bengaluru homes were present. The
case now asserts exact branch membership plus metro proof for both BHKs. The
generic branch dispatcher recognizes a trailing resolved geo scope shared by
preceding bare-BHK alternatives and lets the combined Boolean plan apply it to
every branch. The rerun returns only the North Bengaluru 2BHK and 3BHK, both
with `Nagawara Metro` proof.

The independent review also found that result-level area, BHK, budget, and
distance assertions were weaker than their typed intent checks. They now
validate every returned home, with branch-aware exceptions only where the bank
declares independent alternatives. That stricter gate exposed a generic
place-family `proof_gap`: a bounded metro result was correctly filtered but its
parsed serving-fact distance was not copied to proof focus. Generic hard and
positive fact evidence now carries parsed distance into the proof contract.
The production hardcoding audit and its failure-mode unit tests are now CI
steps, and all clippy findings blocking the existing backend CI command are
closed.

What changed generically:

- config-owned discourse structure for alternatives, conditionals, shared
  suffixes, ordinal comparisons, conjunction, and scoped `unless` exclusions;
- a single spanned tokenizer with sentence and em-dash boundaries;
- required negative lifecycle evidence that fails closed without making
  ordinary risk preferences hard;
- generic nearby-family and multi-anchor proof behavior;
- controlled nearby facts for metro, hospital, and tech-park proof.
- config-owned `rate it well` review language;
- longest-overlap ownership for equivalent maximum-budget wording such as
  `costing no more than`.
- config-owned ranking instructions and scope boundaries that compile an
  ordered preference list without branching on any preference or fact key;
- lexicographic comparison of the same fact-backed preference scores already
  used by normal ranking, with missing evidence ranked behind present proof;
- sentence-final decimal constraint tokenization and configured natural forms
  such as `quietness` and `quieter`;
- decision-only fixture candidates isolated from the original 28 scenarios.

The controlled fixture remains the product-model seed. A later structural pass
may externalize its entities, homes, places, facts, and evidence gaps into a
versioned generic scenario document reusable by DAG, API, compare, and UI
contracts. That work is noted but intentionally deferred.

Next work is deliberately separate: choose a small live-DAG transfer subset,
inspect promoted Parquet before changing code, and fix data/materialization
gaps through the DAG. Do not expand society eligibility or add more
parser/search machinery unless that bank exposes a classified miss.

## Earlier assessment

`continue_as_search_experiment`

The original search experiment succeeded, then drifted into catalog expansion
and society-eligibility work. Stop optimizing the number of fully eligible
societies. The next loop should test whether search can retrieve, constrain,
rank, and explain whatever valid DAG facts are present, including for societies
that are incomplete for normal buyer-detail publication.

Buyer production eligibility remains intact. The revised implementation uses a
separate search-experiment bundle admission policy and an explicit immutable
materialization pin; it does not globally weaken
`app/config/dag/serving_eligibility.json` or move the production pointer.

## Original goal and where the work drifted

The goal was to prove the full local-search chain against promoted facts:

`Parquet facts -> runtime indexes -> intent and entity resolution -> recall -> hard constraints -> ranking -> proof`

The first Godrej Air loop did exactly that. It classified five failed cases as
two generic defects, fixed them, and moved the frozen bank from 131/149 to
149/149 checks without adding project or locality shortcuts.

Later increments remained useful search transfer tests, but admission became
the dominant activity. Work shifted toward media lookup, RERA receipt
regeneration, carpet-area completeness, quarantine counts, and production
catalog releases. Those are catalog-quality concerns, not evidence that search
itself is improving. Ajmera Nucleus and Mantri Serenity1 are the clearest
examples: their exclusion was driven by full eligibility inputs rather than a
failed search-proof test.

## Search implementation audited on this branch

The current branch contains a substantial deterministic search implementation:

- spanned query planning for BHK, budget, area, evidence, named-place relation,
  and distance clauses;
- a Boolean constraint AST that preserves grouped alternatives, exclusions,
  and branch-specific budgets through recall and final eligibility;
- canonical serving-entity joins for societies, builders, areas, aliases, and
  property membership;
- structured, Tantivy, and geo recall with ambiguity-safe society typo
  resolution and abstention for unresolved hard named entities or compact
  unexplained project-like prefixes;
- fact-backed hard constraints, optional `no_data` preference coverage,
  required-evidence gates, generic scoring, and proof focuses derived from the
  same serving facts;
- buyer-safe branch-preserving `resultSets`, internal evidence-gap logging, and
  serving-version-keyed caching for both successful and zero-result responses.

The FastText shadow classifier introduced earlier in the branch was removed.
The current code deliberately uses deterministic query compilation plus
configured ontology and serving entities; the branch name is stale.

One architectural constraint explains the eligibility drift: incomplete
societies are removed atomically while the serving bundle is built, before the
runtime search index sees them. Search's own matching code already behaves in
the desired fact-grounded way for admitted properties: hard terms require a
value or matching fact, required preferences require evidence, and missing
optional facts are marked `no_data` without becoming reasons. The next
experiment therefore belongs at search-bundle admission, not in looser runtime
matching.

## Verification status

Recorded final artifacts:

- clean-serving-v7 bank: 149/149 checks;
- South 40 bank: 57/57 checks;
- Sarjapur 41 bank: 49/49 checks;
- South 43 bank: 67/67 checks;
- 100% recall and proof precision, zero hard-constraint violations, zero
  unsupported claims, stable ordering, and endpoint p95 between 18.53 and
  24.81 ms in the final candidate runs.

Fresh verification after resuming the stopped session and completing the
bundle-switch guardrail:

- `cargo check`: passed.
- Full Rust suite: 629 library tests and all integration binaries passed.
- Search-experiment admission and pointer-isolation tests: 4 passed.
- Search-efficiency integration contract: 11 passed.
- Python benchmark and collection tests: 91 passed, 1 skipped.
- Frontend suite: 145 tests passed; production build passed.
- Search hardcoding audit: zero blocked search-config alias findings.
- South 43 guardrail bank: 82/82 checks, 24.21 ms endpoint p95.
- Mixed 45-society bank: 408/408 checks, 21.15 ms endpoint p95.
- Experiment API smoke suite after the guardrail: 50/50 checks passed.

The three stale Rust failures were old automatic-relaxation expectations, not
search misses. Automatic BHK/budget relaxation and its dormant policy/API/test
surface have now been removed. Explicit budgets and BHKs remain hard; any
future alternatives must be a separately labeled tradeoff result set.

## Resumed-session implementation checkpoint

Implemented:

- `ServingAdmissionProfile::{BuyerCatalog, SearchExperiment}`;
- a minimal experiment policy requiring projected property, area, and
  configuration, without buyer-only media, size, builder, RERA, or road gates;
- admission-profile markers in serving manifests and quarantine reports;
- explicit experiment constructors on the serving builder/materializer;
- profile-aware validation and a buyer-catalog gate that rejects experiment
  bundles;
- runtime coverage proving an incomplete property matches only supported
  BHK/budget/review facts and that unknown price fails a hard budget;
- end-to-end deletion of implicit relaxation from Rust, config, benchmark,
  smoke test, frontend type, and stale tests.

The fact-first query bank is now version 2. Its two former relaxation cases are
recorded as hard-constraint abstentions. This is an intentional contract
revision, not a data-oracle adjustment: the underlying bundle facts did not
change.

## What has and has not been proved

Proved so far:

- exact and typo society resolution;
- hard BHK, budget, area, project-state, rating, and acreage constraints;
- named metro and hospital proof, including two false-proof sentinels;
- grouped configuration alternatives and stable public result branches;
- listing-band budget overlap rather than misleading midpoint filtering;
- deterministic ordering, proof handles, zero-result behavior, and warm
  latency on cohorts up to 45 societies and 107 properties;
- incomplete societies participate when the query is supported by their facts,
  while missing media, size, price, and review evidence never become proof;
- unknown price, unsupported BHK, missing numeric evidence, and impossible
  acreage thresholds fail closed;
- experiment bundles remain outside catalog promotion and outside the global
  current serving pointer;
- named-place search proof survives into the property surface with the same
  fact, entity, distance, source receipt, and serving version.
- absent project-like names fail closed instead of degrading into a broad
  BHK/budget query, while the same name resolves when its serving entity is
  present;
- successful and zero-result cache entries are isolated by serving version,
  and the prior bundle preserves its ordered outcomes after rollback.

Not yet proved:

- broad transfer across schools, tech parks, negative risks, water, traffic,
  noise, builder/RERA evidence, and configured required preferences;
- recall and latency on a materially larger cohort;
- generalized fuzzy/paraphrase behavior beyond the named query banks;
- correction of the two known durable-data category errors.

## Next steps

1. Add transfer banks for configured required evidence, negative risks, more
   named-place families, and generic Boolean/paraphrase cases.
2. Correct the two durable-data category errors through the DAG and rerun their
   existing false-proof sentinels unchanged.
3. Repeat the bank on a materially larger cohort and publish ordered-id, proof,
   unsupported-claim, and latency deltas.

## Incomplete-society experiment — 2026-08-22

Two immutable experiment materializations now prove the admission boundary
without weakening buyer-catalog eligibility:

- scoped incomplete candidate:
  `search-experiment-incomplete-south-2026-08-22-v3`, materialization
  `f48babc6-c468-4c97-96fa-1243f0b57f39`, sourced from KG materialization
  `bdec566b-1c7d-41ae-a792-3a5065854b75`;
- mixed candidate:
  `search-experiment-mixed-south-45-2026-08-22-v1`, materialization
  `d8fcef1a-f54b-4261-9fb0-583a96c8fee1`, based on the validated South 43
  release and adding Ajmera Nucleus plus Mantri Serenity1.

The scoped candidate projected 4 societies and 10 properties. Its frozen
12-case bank passed 76/76 checks with 100% recall and proof precision, zero
hard-constraint violations, zero unsupported claims, stable ordering, and
17.41 ms endpoint p95.

The mixed candidate projected 45 societies and 107 properties from 755
entities, 14,753 facts, 14,551 search metadata rows, and 3,450 edges. The
combined 57-case suite passed 398/398 checks with recall@1/@3/@5 at 100%, MRR
1.0, proof precision 100%, zero hard-constraint violations, zero unsupported
claims, stable ordering, 13.49 ms endpoint p50, and 20.45 ms endpoint p95.

The incomplete cases proved the intended semantics:

- Mantri Serenity1 recalls for exact name, its supported 3BHK, delivered state,
  and verified 19.59-acre fact despite missing size;
- Mantri fails an explicit budget because price is unavailable, fails 2BHK
  because that configuration is unsupported, and fails a 20-acre threshold;
- Ajmera Nucleus recalls despite missing media, and its 2BHK listing band
  overlaps a ₹1.5 Cr budget but not ₹1.2 Cr;
- Ajmera fails an explicit Google-rating constraint because no rating fact is
  present, while optional `good reviews` remains neutral and emits no rating
  proof.

The CLI now provides explicit unpromoted materialization and mixed-extension
commands, validates the resulting bundle, and preserves immutable lineage.
Experiment materializers reject current-pointer promotion. Production and
staging catalog pointers remain on release
`369df41c-72ba-45b5-bc15-9a3655ca007e`, and the global serving pointer remains
`fe16ca1a-a301-4306-ae10-06cc27f792e2`; the experiment was activated only by
its immutable materialization id.

Artifacts:

- ledger: `data/validation/search_fact_ledger_incomplete_south_experiment_v1.json`;
- incomplete bank:
  `data/validation/query_bank/search_incomplete_south_experiment_v1.json`;
- scoped suite:
  `data/validation/search_quality_incomplete_south_experiment_v1.json`;
- mixed suite:
  `data/validation/search_quality_mixed_south_experiment_v1.json`;
- measured outputs: `tmp/search-proof-loop/incomplete-south-experiment-v1/`
  and `tmp/search-proof-loop/mixed-south-45-experiment-v1/`.

The follow-up proof-handoff bank runs through the same benchmark entry point.
For `Godrej Air 3BHK near Hoodi Metro`, it retrieved the search focus, called
the configured `around_this_home` detail surface with that focus, and verified
the Hoodi place entity, `nearby_metro_stations` fact, 100 m distance, focused
feature, Google receipt lineage, and mixed-bundle version. The case passed
13/13 checks with 12.76 ms endpoint p50 and p95.

- handoff bank:
  `data/validation/query_bank/search_proof_handoff_mixed_v1.json`;
- handoff suite:
  `data/validation/search_quality_proof_handoff_mixed_v1.json`;
- measured output:
  `tmp/search-proof-loop/mixed-south-45-experiment-v1/proof-handoff.json`.

## Absent-project and bundle-isolation checkpoint — 2026-08-22

A bundle comparison exposed a generic intent gap. South 43 does not contain
Ajmera Nucleus, but `Ajmera Nucleus 2BHK under 1.5cr` discarded the unresolved
name and returned 14 unrelated homes as exact matches. Synthetic unknown names
behaved the same way. The mixed bundle resolved Ajmera correctly, proving this
was runtime interpretation rather than missing mixed-bundle data.

The fix is generic: query planning removes spans already owned by structured
constraints, configured preferences, and resolved serving entities, then
abstains only for a compact unexplained project-like name. It does not add an
Ajmera alias or project-specific branch. Generic `home`/`homes` vocabulary
prevents descriptive community queries from being misclassified.

Frozen results:

- South 43 v2: 13 cases, 82/82 checks, 24.21 ms endpoint p95;
- mixed experiment v2: 59 cases, 408/408 checks, 21.15 ms endpoint p95;
- absent Ajmera: zero results on South 43;
- present Ajmera: exactly `discovered-ajmera-nucleus-2bhk` on mixed;
- `Foo Bar Residency` and `Unknown Heights`: zero results on both;
- plain `2BHK under 1.5cr`: the original 14 South 43 results retain their
  order; mixed adds only the admitted Ajmera result;
- configured optional preferences still return candidates and do not become
  false project names.

The cache contract now directly proves that an identical zero-result response
stored under one serving version cannot be read under another, and that reload
clearing removes it. Both experiment runtimes remained explicitly pinned; no
catalog or global serving pointer moved.

Artifacts:

- common unknown-name bank:
  `data/validation/query_bank/search_unknown_project_guardrail_v1.json`;
- South 43 absent-Ajmera bank:
  `data/validation/query_bank/search_absent_ajmera_south_43_v1.json`;
- measured outputs: `tmp/search-proof-loop/bundle-switch-v1/`.

## Historical execution log

### First-loop decision

`keep`

The current promoted bundle remains `clean-serving-v7-2026-08-16`. This loop
found and fixed two generic runtime defects, but it did not prove a buyer-visible
data gap that warrants a new DAG materialization or promotion.

## Pinned baseline

- Git starting commit: `18607460`
- Serving materialization: `fe16ca1a-a301-4306-ae10-06cc27f792e2`
- Source run: `cc973fb5-14f5-4242-9cfe-c52ba7738210`
- 697 entities, 13,310 facts, 3,353 edges, 12,969 search metadata rows
- 37 eligible societies, 86 searchable properties, 28 quarantined societies
- Hardcoding audit: passed; no blocked search-config alias findings
- Fact ledger: `data/validation/search_fact_ledger_clean_serving_v7.json`
- Frozen bank: `data/validation/query_bank/search_clean_serving_v7_godrej_air.json`

Local artifacts:

- Warm baseline: `tmp/search-proof-loop/clean-serving-v7-warm-baseline/`
- After sibling fix: `tmp/search-proof-loop/clean-serving-v7-no-siblings/`
- Final generic fixes: `tmp/search-proof-loop/clean-serving-v7-generic-fixes/`

## What the bundle proved

Godrej Air has buyer-visible 2BHK and 3BHK projections, 5.3 acres of project
land, delivered state, Google 4.2 from 839 reviews, Hoodi metro at 100 m, and
Manipal Hospital Whitefield at 1.8 km. The Whitefield runtime cohort also
provided transfer cases for BHK, area, and budget constraints.

Two computed proximity rows are categorically wrong in the durable data:

- Cult Neo Gym appears under `nearby_hospitals`.
- Sri Sathya Sai Hospital appears under `nearby_metro_stations`.

The frozen safety sentinels showed that neither row currently leaks a false
buyer-visible proof reason for its named query. They remain DAG data-quality
follow-ups, not justification for an unproven runtime patch.

## Baseline and classifications

The warm baseline ran every case once for warmup and three times for
measurement and ordering stability:

- 20 cases; 131/149 checks passed (87.9%)
- endpoint p50 14.73 ms; p95 38.14 ms
- all repeated ordered result lists were stable

The five failed cases reduced to two causes:

1. `architecture_gap`: the API route appended synthetic sibling configurations
   after the engine had enforced hard BHK and budget constraints. This affected
   exact 2BHK, exact 3BHK, budget, and delivered-state cases.
2. `intent_gap`: fuzzy recall ranked the intended society for `Godrej Ari`, but
   serving-entity resolution did not convert a unique minor typo into a hard
   named-society constraint.

The `Godrej Air 3BHK under 3cr` oracle was corrected before behavior changes
were evaluated: the promoted listing band starts at ₹2.80 Cr, so the bundle
does prove qualifying inventory even though the projected midpoint card is
₹3.175 Cr.

## Changes retained

- Adapted the benchmark to the buyer-safe `resultSets` API and preserved branch
  identity, branch order, tiers, tradeoff labels, proof handles, and result
  order in artifacts.
- Added public-outcome checks for zero results, total matches, hard BHK, budget,
  area, tiers, ordered prefixes, forbidden IDs, and proof entity/distance.
- Added warmup, repeated timing, and deterministic ordering checks.
- Removed post-ranking sibling injection from the public search response.
- Added generic, ambiguity-safe, multi-token society typo resolution over
  serving entities. No project or locality alias was added.

## Final result

- 20/20 cases passed; 149/149 checks passed
- endpoint p50 14.81 ms; p95 39.33 ms
- recall@1/@3/@5 100%; mean reciprocal rank 1.0 across 17 ranked oracle cases
- proof precision 100%
- zero hard-constraint violations
- zero unsupported proof claims in the frozen sentinels
- stable ordered results across all three measured repetitions
- serving pointer unchanged

The next proof loop should profile five to ten additional Whitefield societies,
add only fact-supported transfer queries, and run the same frozen bank before
considering an immutable candidate bundle promotion.

## South Bengaluru 40-society increment

The next immutable candidate added Brigade Komarla Heights, Godrej Eternity,
and SNN Raj Etternia through scoped RERA collection and catalog extension:

- DAG run: `32230a76-6a44-4462-98f1-7e5e4d9b4d7c`
- Scoped serving materialization: `24d91f2d-be41-4786-bdbf-aa548bb6c4ea`
- Merged serving bundle: `south-40-candidate-v2-2026-08-22`
- Merged serving materialization: `dd88e141-af97-42f7-b97f-4a9512428aea`
- Catalog release: `6530c7d4-dc46-4b36-93e3-976118871dc1`
- Runtime projection: 40 societies, 94 searchable properties, 13,691 facts,
  and 13,350 search metadata rows
- Quarantined societies: 0

The first regional pass classified two failures as `ranking_gap` and
`intent_gap`. Disabling configured budget expansion kept hard budgets hard and
fixed the first. The second came from generic named-entity scoping: in a query
starting with `under construction`, the resolver treated that first `under` as
the operator for a later numeric budget and expanded the area scope to
`Padmanabhanagar under`. Budget operators now bind only when immediately
adjacent to the parsed numeric budget.

Final proof artifacts:

- Frozen 20-query bank: `tmp/search-proof-loop/south-40-candidate/final-frozen-bank.json`
  — 149/149 checks passed
- South Bengaluru 8-query bank: `tmp/search-proof-loop/south-40-candidate/final-regional-bank.json`
  — 57/57 checks passed
- Fact ledger: `data/validation/search_fact_ledger_south_40_candidate.json`
- Query bank: `data/validation/query_bank/search_south_40_candidate.json`

The hardcoding audit reported zero blocked search-config aliases. Catalog
validation passed every required gate; its only warning correctly records four
collected RERA societies outside this release's catalog scope.

## Sarjapur 41-society increment

Godrej Lakeside Orchard was added through a scoped DAG run and rebased onto the
promoted South Bengaluru catalog without changing the existing 40 societies:

- DAG run: `10e1ce33-593f-48a4-91d2-56fdab7fee83`
- Scoped serving materialization: `fa6125a2-d953-48c6-a772-a1e18d256c26`
- Merged serving bundle: `sarjapur-41-candidate-2026-08-22`
- Merged serving materialization: `464b88a1-cb7c-4e00-ac31-14e4096280b7`
- Catalog release: `86cdcd9a-0733-4875-8f91-e8846c612f32`
- Runtime projection: 41 societies, 97 searchable properties, 13,993 facts,
  and 13,834 search metadata rows
- Quarantined societies: 0

Direct Parquet inspection proved 2BHK and 3BHK RERA configurations,
listing-backed 2/3/4BHK inventory, under-construction and on-track state,
12.07 acres, 698 homes, and Google 3.6 from 196 reviews. The 4BHK projection
is deliberately treated as listing-backed only because the official RERA
configuration fact lists 2BHK and 3BHK.

The first Sarjapur benchmark run exposed two measurement issues rather than a
ranking defect. Its 3BHK band starts at ₹1.70 Cr, so the `under 1.8cr` query is
valid even though the result card displays the ₹2.55 Cr midpoint. Separately,
valid zero-result searches were not cached, making the 13-acre negative control
repeat the deterministic search path at roughly 74 ms. Search cache entries are
already serving-version keyed and replay their log messages, so zero-result
responses now use the same safe cache path as successful responses.

Final proof artifacts:

- Frozen 20-query bank:
  `tmp/search-proof-loop/sarjapur-41-candidate/final-frozen-bank.json` —
  149/149 checks passed, endpoint p95 18.53 ms
- South Bengaluru 8-query bank:
  `tmp/search-proof-loop/sarjapur-41-candidate/final-south-bank.json` —
  57/57 checks passed, endpoint p95 20.41 ms
- Sarjapur 7-query bank:
  `tmp/search-proof-loop/sarjapur-41-candidate/final-sarjapur-bank.json` —
  49/49 checks passed, endpoint p95 23.15 ms
- Fact ledger:
  `data/validation/search_fact_ledger_sarjapur_41_candidate.json`
- Query bank:
  `data/validation/query_bank/search_sarjapur_41_candidate.json`

All three banks retained 100% recall and proof precision, zero hard-constraint
violations, zero unsupported claims, and stable ordering. The hardcoding audit
again reported zero blocked search-config aliases. The optional
`rera_project_plan_frames` asset remained absent because its source payload was
missing; no plan evidence was invented.

## South Bengaluru 43-society increment

Mahaveer Ranches and Prestige Song of the South were added through scoped DAG
collection and catalog extension:

- Media identity fix run: `8a97537f-cead-4bb8-9634-2dd5fdfeca11`
- RERA evidence run: `a18b8ead-284f-4f3c-b6ea-8f490e616068`
- Scoped serving materialization: `269a4bd0-f2b5-4a1a-b05a-07e133e8593c`
- Merged serving bundle: `south-43-candidate-2026-08-22`
- Merged serving materialization: `7dbac2ab-3474-4d81-8b66-6f8251ecdd51`
- Catalog release: `369df41c-72ba-45b5-bc15-9a3655ca007e`
- Runtime projection: 43 societies, 103 searchable properties, 14,463 facts,
  and 14,285 search metadata rows
- Quarantined societies: 0

The first scoped run showed that validated media existed under historical RERA
IDs while three phase-grouped canonical IDs could not see it. The collector now
uses the supplied historical alias only to locate staged media and continues to
emit every fact under the canonical entity ID. This cleared media eligibility
without copying media or weakening the policy.

Ajmera Nucleus passed media eligibility but was not promoted because live
K-RERA detail lookup returned no exact project row, preventing regeneration of
its authoritative evidence receipt. Mantri Serenity1 remains quarantined for
`missing_property_size`.

Final proof artifacts:

- Frozen bank: `tmp/search-proof-loop/south-43-candidate/frozen-bank.json` —
  149/149 checks passed, endpoint p95 20.50 ms
- South 40 bank: `tmp/search-proof-loop/south-43-candidate/south-bank.json` —
  57/57 checks passed, endpoint p95 24.03 ms
- Sarjapur bank: `tmp/search-proof-loop/south-43-candidate/sarjapur-bank.json` —
  49/49 checks passed, endpoint p95 24.81 ms
- New South 43 bank:
  `tmp/search-proof-loop/south-43-candidate/new-south-bank.json` — 67/67
  checks passed, endpoint p95 22.84 ms
- Fact ledger: `data/validation/search_fact_ledger_south_43_candidate.json`
- Query bank: `data/validation/query_bank/search_south_43_candidate.json`

All banks retained 100% recall and proof precision, zero hard-constraint
violations, zero unsupported claims, and stable ordering. The 43-society
release was promoted to staging and production with compare-and-swap; the
41-society release remains the rollback point.
