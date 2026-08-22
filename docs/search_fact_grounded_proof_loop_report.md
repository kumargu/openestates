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
