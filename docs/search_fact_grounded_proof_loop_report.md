# Search Fact-Grounded Proof Loop — Clean Serving v7

Date: 2026-08-22

## Decision

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
