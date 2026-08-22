# fastText residual intent classifier: shadow decision

## Scope

The experiment classifies only buyer-language clauses left after deterministic
BHK, budget, evidence, relation, area, and entity spans are owned. Predictions
are internal diagnostics and cannot change constraints, recall, ranking, proof,
or the API response.

Training labels reference `fact_registry.json` preference patterns. Training
phrases combine those patterns with polarity-compatible
`answers_preferences` from the facts named by each preference. Ambiguous phrases
owned by more than one label are excluded.

## Baselines

- Search contract: `backend/tests/search_quality.rs`
- Baseline: 27/32 labelled quality checks, with all four test cases passing.
- Hardcoding audit: two pre-existing comment-only `office` findings in
  `backend/src/search/geo.rs`; no blocked config aliases.
- Held-out classifier bank:
  `data/validation/query_bank/search_intent_classifier_v1.json`

## Result

- 24 configured labels
- 976 generated training examples
- 48 independently written held-out clauses
- 7 selected, all 7 correct
- Selected precision: 100%
- Overall accuracy: 14.6%
- Abstentions: 41/48
- Warm prediction p95: 74 microseconds
- Model size: 9.65 MB

The classifier is fast enough, but it does not close the semantic intent gap.
Character and word n-grams generalize spelling and nearby phrasing; they do not
reliably connect novel concepts such as `honking` to noise or `gridlock` to
traffic without labelled examples.

## Decision

Keep the implementation in `shadow` mode. It is useful as a deterministic,
versioned measurement harness, but it is not eligible to influence ranking.
The next proof step is independently reviewed buyer-query training data grouped
by wording family. Threshold reduction is not an acceptable substitute because
it would trade abstention for false intent selections.

## Config vocabulary follow-up — 2026-08-22

Property-relevant wording families were expanded in `fact_registry.json` for
noise, traffic, maintenance, reviews, commute, construction quality, open
space, ventilation, waterlogging, and handover risk. All ten requested buyer
queries now compile deterministically to configured preferences.

After retraining:

- 1,168 generated training examples
- 21/48 selected, 20 correct
- Selected precision: 95.2%
- Overall accuracy: 41.7%
- Abstentions: 27/48
- Warm prediction p95: 82 microseconds

The shadow model still abstains on `consistently cared for` and `daily
gridlock`, while deterministic config recognizes both. This supports the
existing boundary: configured property language guarantees known behavior;
fastText remains an abstaining classifier for otherwise unowned clauses.
