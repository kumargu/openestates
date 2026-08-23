# Golden Intent + Gap Query Set

This suite is the pre-LLM search benchmark. It tests whether OpenEstates turns
messy buyer language into structured intent and missing-evidence requests.

Files:

- `data/validation/query_bank/golden_intent_gap_v1.json`
- `data/validation/search_quality_golden_intent_gap_v1.json`
- `data/validation/query_bank/guardrail_queries_v1.json`
- `data/validation/search_guardrails_v1.json`

The suite has 100 buyer briefs across family, commute, legal safety, water risk,
maintenance, investment, parents, schools, hospitals, and avoid-style queries.
It intentionally does not assert specific result titles because it is not tied
to the current Whitefield serving bundle.

Run it against a live backend with:

```bash
python3.10 -m pipeline.benchmark_search_quality \
  --base-url http://127.0.0.1:4000 \
  --spec data/validation/search_quality_golden_intent_gap_v1.json \
  --output tmp/search_quality_golden_intent_gap_v1.json \
  --markdown-output tmp/search_quality_golden_intent_gap_v1.md
```

Use this to compare the deterministic parser and any future small LLM parser.
The LLM should be judged on:

- hard constraints: area, BHK, budget, excluded areas
- soft intent: positive and negative preferences
- buyer archetype
- accepted tradeoffs and unsupported inventory
- missing evidence keys
- no semantic or LLM-generated proof claims without serving facts

Do not optimize this suite by adding query-specific ranking hacks. A good fix
should improve a class of buyer language or a canonical evidence family.

## Guardrail Suite

The guardrail suite checks that vague, no-home-intent, assistant-directed, and
unsupported queries are stopped before ranking. These cases should return zero
property results plus a `search_guidance` object with a response mode such as
`too_short`, `out_of_scope`, `needs_home_anchor`, `needs_more_specifics`, or
`unsupported_inventory`.

Run it with:

```bash
python3.10 -m pipeline.benchmark_search_quality \
  --base-url http://127.0.0.1:4000 \
  --spec data/validation/search_guardrails_v1.json \
  --output tmp/search_guardrails_v1.json \
  --markdown-output tmp/search_guardrails_v1.md
```
