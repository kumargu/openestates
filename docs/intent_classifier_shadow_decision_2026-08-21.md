# Intent classifier decision: remove the shadow path

## Experiment scope

The experiment classified only buyer-language clauses left after deterministic
BHK, budget, evidence, relation, area, and entity spans were owned. Predictions
were internal diagnostics and could not change constraints, recall, ranking,
proof, or the API response.

Training labels referenced `fact_registry.json` preference patterns. Training
phrases combined those patterns with polarity-compatible
`answers_preferences` from the facts named by each preference. Ambiguous
phrases owned by more than one label were excluded.

## Baselines

- Search contract: `backend/tests/search_quality.rs`
- Baseline: 27/32 labelled quality checks, with all four test cases passing.
- Hardcoding audit: two pre-existing comment-only `office` findings in
  `backend/src/search/geo.rs`; no blocked config aliases.
- Held-out classifier bank:
  `data/validation/query_bank/search_intent_classifier_v1.json`

## Experiment result

The first run produced:

- 24 configured labels
- 976 generated training examples
- 48 independently written held-out clauses
- 7 selected, all 7 correct
- Selected precision: 100%
- Overall accuracy: 14.6%
- Abstentions: 41/48
- Warm prediction p95: 74 microseconds
- Model size: 9.65 MB

The classifier was fast enough, but it did not close the semantic intent gap.
Character and word n-grams generalized spelling and nearby phrasing; they did
not reliably connect novel concepts such as `honking` to noise or `gridlock`
to traffic without labelled examples.

## Config vocabulary follow-up — 2026-08-22

Property-relevant wording families were expanded in `fact_registry.json` for
noise, traffic, maintenance, reviews, commute, construction quality, open
space, ventilation, waterlogging, and handover risk. All ten requested buyer
queries then compiled deterministically to configured preferences.

After retraining:

- 1,168 generated training examples
- 21/48 selected, 20 correct
- Selected precision: 95.2%
- Overall accuracy: 41.7%
- Abstentions: 27/48
- Warm prediction p95: 82 microseconds

The shadow model still abstained on `consistently cared for` and `daily
gridlock`, while deterministic config recognized both. Configured property
language therefore guaranteed known behavior while the classifier did not
provide a reliable fallback for otherwise unowned clauses.

## Final decision — 2026-08-22

Remove the classifier instead of retaining it in shadow mode.

- Remove the fastText crate and lockfile entries.
- Remove the training/evaluation binary, runtime model loader, residual-clause
  extraction, traces, and application-state wiring.
- Remove classifier mechanics and labels from `search_intent.json` and its Rust
  config model.
- Keep the useful buyer-language expansions in `fact_registry.json`, where
  product vocabulary already belongs.
- Use the deterministic configured compiler for intent and the promoted
  serving bundle for entity resolution and evidence capabilities.
- Treat missing soft evidence as an additive ranking/proof concern. Only a
  config-marked `required` preference may gate eligibility.

This leaves one semantics owner and one execution path. It also preserves the
request-path rule: local artifacts only, with no network or LLM classifier in
`/api/search`.

Reintroducing a learned compiler would require an independently reviewed query
bank, a clear improvement over configured deterministic behavior, and a design
that does not become a competing source of product vocabulary, hard
constraints, recall, or proof semantics.

## Verification

- Rust library: 620/620 passed.
- Search contracts: 11/11 efficiency, 4/4 quality, and 5/5 quality contract.
- Serving runtime: 5/5 passed.
- Frontend: 145/145 tests and production build passed.
- Fresh-server smoke suite: 50/50 passed.
- Production hardcoding audit: no blocked aliases; two existing comment-only
  `office` warnings remain.
