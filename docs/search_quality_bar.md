# Search Quality Bar

OpenEstates search should understand anchored buyer language through configured
intent, serving entities, geo relationships, and DAG facts. The search stack has
three separate jobs: recall enough plausible homes, prove why any home is
ranked, and explain missing evidence honestly. FastEmbed and vector recall are
not part of the current runtime.

The implementation roadmap lives in `docs/search_engine_roadmap.md`; the active
ontology cleanup plan lives in `docs/search_ontology_refactor_plan.md`.

## Current Reset

As of 2026-08-01, search quality work is reset around the pinned Bangalore
bundle `bangalore-catalog-60-coherent-2026-08-01`
(`909a8bd0-3af0-42af-ae26-ba493f54174a`). Older benchmark numbers in this file
are historical context only until rerun against that bundle.

The current architecture rule is stricter than the old benchmark: search should
not recover quality by adding locality, landmark, school, mall, tech-park, or
road aliases to parser config. Named-place quality must come from serving
entities, coordinates, graph edges, and proof facts.

## Proof Loop Discipline

Search quality work must be evidence-led. Before adding more code, run a short
chain audit and decide whether the existing layers still line up with the
roadmap:

1. **Audit the chain:** list relevant local commits or touched files since the
   last search-quality checkpoint, map each to the milestone it was meant to
   serve, and flag duplicated or bypassed paths.
2. **Pin the evidence:** record the serving bundle, benchmark spec, API command,
   config mode, and baseline artifact used for the run. Do not compare results
   across different bundles without saying so.
3. **Classify failures first:** assign each bad case to one primary bucket:
   `data_gap`, `intent_gap`, `proof_gap`, `ranking_gap`, `embedding_gap`, or
   `architecture_gap`.
4. **Change one layer:** make the smallest generic/config-driven change that
   addresses the dominant bucket. Avoid query-specific patches.
5. **Rerun and decide:** keep the change only if it improves a stated metric,
   preserves safety, and does not introduce a hardcoding regression. Otherwise
   revert or leave it behind an explicitly experimental flag.

Every benchmark markdown should include a short "proof-loop decision" section:
`keep`, `revert`, `shadow_only`, or `needs_more_data`, with the reason. This is
how we avoid piling code that is not improving search.

Before each loop, also do a shape audit:

- Which files/commits changed since the last loop?
- Which milestone does each change belong to?
- Did any change create hidden filters in config, tests, or runtime?
- Is the benchmark measuring real data-backed improvement or rewarding a
  shortcut?

## Bar

1. Intent parsing captures hard constraints and soft tradeoffs from natural
   buyer language: BHK, area, budget, positive preferences, negative
   preferences, accepted tradeoffs, and unsupported inventory requests.
2. Configured intent and relation parsing handle credible buyer paraphrases
   without locality- or landmark-specific code. Named entities come from the
   serving bundle, not parser aliases.
3. Multiple proximity clauses remain independent. Each resolved anchor may add
   ranking evidence and proof; an unresolved secondary anchor records a gap and
   never invents a destination.
4. Proof precision beats recall. A result may be lexically or structurally recalled, but it
   must not claim review quality, water safety, builder trust, maintenance
   quality, or legal safety unless the serving facts contain matching evidence.
5. Over-constrained and negative-only queries must not silently look broken.
   When the local bundle has no proveable candidates, search should return a
   deterministic relaxation or a recorded enrichment gap rather than fake
   evidence.
6. Warm request latency stays bounded by in-memory structured, Tantivy, geo,
   and fact lookup.

## Current Checks

The short ad hoc suite is `pipeline/eval_search.py`. Before changing the
buyer-language benchmark, profile the promoted Parquet bundle and choose queries
from fact families that actually exist with usable confidence:

```bash
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git \
cargo run --manifest-path backend/Cargo.toml --locked \
  --bin openestates-profile-serving-bundle -- \
  --limit 120 --markdown > tmp/serving_bundle_fact_profile.md
```

The stronger buyer-language benchmark is:

```bash
python3.10 -m pipeline.benchmark_search_quality \
  --base-url http://127.0.0.1:4011 \
  --spec data/validation/search_quality_queries_v1.json \
  --output tmp/search_quality_benchmark_v1_data_backed.json \
  --markdown-output tmp/search_quality_benchmark_v1_data_backed.md
```

The suite manifest is `data/validation/search_quality_queries_v1.json`. Reusable
queries live under `data/validation/query_bank/`; add new real buyer queries
there first, then include them in a suite with `case_files`. This lets the same
case bank feed ad hoc runs, future integration tests, and regression suites as
the DAG grows.

The contract tests that encode the product bar are:

- `backend/tests/search_efficiency_contract.rs`
- `backend/tests/search_quality_contract.rs`

Historical only: the previous promoted FastEmbed bundle was
`2026-07-21-generated-context-waterford-brigade-semantic-20260722053602`.
That profile showed 16,431 entities, 146 properties, 94,032 fact rows, 94,018
search metadata rows, and 9,930 precomputed semantic embedding rows.

The current reset bundle is `bangalore-catalog-60-coherent-2026-08-01`, with
286 entities, 6,901 facts, 1,894 graph edges, and 6,754 search metadata rows.
Profile it before using it as a benchmark baseline; do not compare its score
directly with older Waterford/Whitefield bundle runs.

Scoreable fact families in the current bundle include RERA/legal status,
builder RERA track record, Google/community review evidence, metro access,
nearby schools/hospitals/tech parks, listing price facts, home state, and
timeline state. These are fair search-quality benchmark inputs.

Water/tanker/maintenance-negative language is not yet a fair search-quality
input. The current bundle has only one `operating.tanker_dependence` row with
confidence `0.4`, no `maintenance_sentiment`, no
`lifecycle.builder_reputation_negative`, no `water_supply_risk`, no
`waterlogging_risk_score`, and only low-confidence approach-road waterlogging
rows. These stay as data-gap sentinels until DAG collection/enrichment promotes
enough sourced facts.

Historical experiment only: the old data-backed buyer-language benchmark against the promoted offline
FastEmbed bundle is 55/76 scoreable checks, or 72.4%. Overall, including
data-gap sentinels, it is 70/99 checks, or 70.7%. Recall is healthy for this
suite: 19/19 cases returned results. Safety is clean: 19/19 checks pass for
"semantic score must not become proof." The quality gaps recorded in that run were:

- Intent parsing misses multi-line and typo-heavy budget phrasing such as
  `budget ideally under 2.5Cr.`, `under 2.8Cr.`, and `undr 2.5cr`.
- Intent extraction does not consistently map buyer phrases like safe
  paperwork, RERA clarity, review receipts, legal papers, builder track record,
  and approval documents to canonical preferences.
- Proof surfacing is incomplete. RERA, listing, builder-track-record, and
  review facts exist in Parquet, but many matching queries return no proof
  reason keys from those facts.
- Data-gap sentinel queries correctly find local candidates, but the API does
  not yet report the missing evidence keys for tanker dependence, maintenance
  sentiment, BBMP/OC approval, or waterlogging.

## Diagnosing Poor Matches

Every bad result should be assigned to one primary bucket before we change
ranking weights:

1. `data_gap`: the query asks for a fact family that is missing, sparse, stale,
   or below support confidence in the serving Parquet. Example: tanker
   dependence today.
2. `intent_gap`: the fact exists, but the parser does not extract the hard
   constraint or canonical preference. Example: `safe paperwork` not becoming
   `legal_safety`.
3. `proof_gap`: the fact exists and recall found candidates, but result reasons
   do not surface the matching fact. Example: RERA/listing facts present but no
   proof reason appears.
4. `ranking_gap`: the right candidate is recalled but ranked below weaker
   candidates even with matching proof.
5. `embedding_gap`: reserved for an explicitly scoped future embedding
   experiment. The current deterministic runtime does not assign ordinary
   misses to this bucket.
6. `architecture_gap`: the code path works for a case but is in the wrong layer,
   duplicates another layer, bypasses DAG/config ownership, or cannot explain
   itself through the shared proof contract. This bucket blocks more feature
   work until the chain is simplified or documented as a temporary shim.

This matters because recall mechanics cannot fix missing facts, and DAG
collection cannot fix weak query parsing. The benchmark report should make that
separation visible before we decide what to build next.

## Coordinate Enrichment

Geospatial facts should become first-class serving facts, not ad hoc search
logic. Each place-like entity should carry coordinates in Parquet when the
source can prove them:

- societies/projects: `geo.latitude`, `geo.longitude`, `geo.source`,
  `geo.confidence`, and optional `geo_geohash`
- external places such as malls, schools, hospitals, tech parks, metro
  stations, approach-road points, and waterlogging spots with the same shape
- deterministic derived facts such as `distance_to_nearest_metro_km`,
  `distance_to_named_landmark_km`, `distance_to_nearest_hospital_km`, or
  `within_15_min_drive_to_office_hub`

For a query like "near a named mall" or "close to metro but not cut off from
hospitals", search should parse the place mention, resolve it to a place/entity
with coordinates, compute local distances over the serving bundle, and
rank/explain with those derived distance facts. The proof reason must come from
the coordinate-backed derived fact.

## Next Data Work

To make tanker, maintenance, builder-negative, BBMP/OC, and monsoon-drainage
queries scoreable, collect or enrich those signals through the DAG first, then
materialize them as sourced facts with search metadata. Only after those facts
are promoted should the query families move from `data_gap` mode to
`data_backed` mode.

The next enrichment pass should prioritize:

- Coordinates for every scored society/project and every nearby place used in
  search explanations.
- Place-resolution facts for malls, metro stations, schools, hospitals, tech
  parks, major junctions, and approach roads.
- Resident/source evidence for water reliability, tanker dependence,
  maintenance sentiment, association issues, and monsoon access risk.
- Search metadata for each new fact family, including `answers_preferences`
  and scoring hints, so the facts become searchable without hardcoded term
  lists.
