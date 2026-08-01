# Search Engine Roadmap

OpenEstates search starts the user journey. The user should be able to type a
messy buyer brief, but the backend must turn it into a small set of explicit
dimensions: constraints, places, preferences, proof requirements, tradeoffs, and
missing evidence. This is not general web search. It is a layered, local,
receipt-backed decision engine.

## North Star

Search should answer:

- What did the user ask for?
- Which constraints are hard?
- Which entities and places did the query mention?
- Which homes match the constraints?
- Which homes are close to the requested places?
- Which homes have proof for the requested preferences?
- Which facts are missing and should be enriched offline?
- Why is each result ranked here?

The runtime stays local: no request-time DAG runs, no network calls, no LLM calls,
and no corpus embedding at API startup. Serving Parquet is the source of runtime
truth. Tantivy, structured indexes, semantic vectors, geo indexes, and fact
indexes are all rebuildable serving caches.

## Target Runtime Shape

```text
free text query
  -> SearchIntent
  -> entity/place resolution
  -> structured constraints
  -> parallel recall
       -> in-memory structured recall
       -> Tantivy lexical recall
       -> semantic vector recall
  -> geo scoring
  -> fact/proof scoring
  -> ranking
  -> explanations + learning gaps
```

Recommended module shape:

```text
backend/src/search/
  engine.rs        orchestrates the full request
  intent.rs        free text -> structured SearchIntent
  resolver.rs      society/place/area alias resolution
  constraints.rs   BHK, budget, area, exclusions, relaxation
  recall.rs        structured + Tantivy + semantic recall merge
  geo.rs           coordinates, distance math, geo scores
  facts.rs         serving-fact lookup and proof matching
  rank.rs          combines structured, lexical, semantic, geo, proof signals
  explain.rs       reasons, preference coverage, learning gaps
```

The existing `text.rs` should be drained gradually. It remains the compatibility
path until each layer is proven by tests.

## Proof Loop

Search improvements must move through a small proof loop instead of accumulating
unverified code:

```text
chain audit
  -> pinned baseline benchmark
  -> failure bucket classification
  -> one generic/config-driven change
  -> same benchmark again
  -> keep / revert / shadow-only decision
```

The chain audit is mandatory before each new milestone slice. It should answer:

- Which local commits or touched files are part of the current search chain?
- Which milestone does each change support?
- Did any experimental path become default product behavior without a passing
  parity or quality gate?
- Are there two paths doing the same job, such as two resolvers, two fact
  mappings, or two proof contracts?
- Did any code add product vocabulary that belongs in `app/config/dag/` or DAG
  facts?
- Does the UI/search proof contract still point to the same evidence shape?

Each loop must classify failures as `data_gap`, `intent_gap`, `proof_gap`,
`ranking_gap`, `embedding_gap`, or `architecture_gap` before changing code.
`architecture_gap` means the behavior may work locally, but the layer boundary
is wrong, duplicated, unmeasurable, or not aligned with DAG/config ownership.

The benchmark artifact should record the proof-loop decision:

- `keep`: metric improved or architecture simplified with no quality loss.
- `revert`: behavior failed or made quality worse.
- `shadow_only`: useful signal, but not approved for product ranking/proof yet.
- `needs_more_data`: the right next step is DAG/source enrichment, not runtime
  code.

## Scoring Contract

The ranker should score every candidate with explicit components:

```text
CandidateScore {
  hard_constraint_score,
  lexical_score,
  semantic_score,
  geo_score,
  proof_score,
  confidence_score,
  freshness_score,
  negative_preference_penalty,
  relaxation_penalty,
}
```

Semantic recall may widen candidates and provide a soft score, but it must never
create proof. Geo distance must come from coordinates. Legal, review,
maintenance, water, builder, and price claims must come from serving facts.

## Latency Targets

Early product target:

- warm p95 search: `<200ms`
- warm p50 search: `<75ms`
- no result path should block on enrichment or network

Long-term local target:

- warm p95 search: `<100ms`
- p95 without semantic recall: `<50ms`
- semantic recall should be timeout-bounded and optional

Initial layer budget:

| Layer | Target |
|---|---:|
| intent parse | 1-5ms |
| entity/place resolution | 1-5ms |
| structured recall | 1-5ms |
| Tantivy recall | 5-20ms |
| query embedding | 10-50ms when warm |
| vector scan | 1-15ms at current scale |
| geo scoring | 1-10ms |
| fact/proof scoring | 5-25ms |
| ranking + response assembly | 5-20ms |

Async strategy:

- Run Tantivy recall, semantic recall, and place resolution as independent
  futures once intent is parsed.
- Keep structured filters and final ranking synchronous unless profiling proves
  they are expensive.
- Timeout semantic recall before it can dominate the request. Return lexical +
  structured + geo results when semantic is late.
- Cache repeated query embeddings and place resolutions.

## Milestones

### M0: Honest Baseline

Goal: know whether each miss is caused by missing data, intent parsing, proof
surfacing, ranking, or embeddings.

Deliverables:

- Reusable query bank under `data/validation/query_bank/`.
- Serving-bundle profile from promoted Parquet.
- Benchmark report split by `data_backed` and `data_gap`.
- Baseline latency report for current search endpoint.

Quality gates:

- Query bank loads through `case_files`.
- No duplicate query IDs.
- Current scoreable benchmark remains reproducible.
- Chain audit maps existing search commits/files to roadmap milestones and
  identifies any duplicate or experimental paths before new code lands.
- Every failed benchmark case is assignable to one primary bucket:
  `data_gap`, `intent_gap`, `proof_gap`, `ranking_gap`, `embedding_gap`, or
  `architecture_gap`.

Measurement:

```bash
python3.10 -m pipeline.benchmark_search_quality \
  --base-url http://127.0.0.1:4011 \
  --spec data/validation/search_quality_queries_v1.json \
  --output tmp/search_quality_benchmark_v1_data_backed.json \
  --markdown-output tmp/search_quality_benchmark_v1_data_backed.md
```

Current baseline:

- scoreable quality: `55/76`, `72.4%`
- overall including data gaps: `70/99`, `70.7%`
- result-count checks: `19/19`
- safety: `19/19`

### M1: Search Engine Boundaries

Goal: stop adding behavior into one large ranker path.

Deliverables:

- `SearchEngine` facade with an explicit request pipeline.
- `RecallSet` and `CandidateScore` structs.
- Layer timing captured in the search response under an internal/debug-only
  field or benchmark output.
- Existing `TextSearch` path preserved behind the facade.

Quality gates:

- No behavior regression on the query bank.
- Existing API response shape remains stable.
- Layer timings appear for every benchmark case.

Latency gates:

- p95 no worse than current baseline by more than 10%.
- p95 stays under 200ms on the benchmark suite.

Status:

- Implemented `SearchEngine` as a compatibility facade over the existing
  `TextSearch` ranker.
- Added explicit `RecallSet` and `CandidateScore` boundaries for recall
  attribution and candidate-level scoring diagnostics.
- Added layer timings, recall-channel attribution, and top candidate score
  diagnostics to `/api/search?debug=true`; normal search responses keep the
  public response shape stable.
- Updated the benchmark to require diagnostics, measure full endpoint latency,
  and enforce an endpoint p95 gate.
- Bounded semantic recall and broad unstructured local recall so measured
  candidate sets and ranking inputs stay aligned.
- Skipped semantic recall for high-confidence structured searches when local
  recall is already narrow, while keeping FastEmbed active for unstructured
  soft buyer-language queries.
- Added benchmark runtime proof for serving bundle version, semantic embedder
  model, semantic index model, and semantic document count.

M1 benchmark:

- scoreable quality: `69/90`, `76.7%`
- overall including data gaps: `89/118`, `75.4%`
- result-count checks: `19/19`
- safety: `19/19`
- serving bundle: `2026-07-21-generated-context-waterford-brigade-semantic-20260722053602`
- semantic embedder: `fastembed-all-minilm-l6-v2`
- semantic index: `fastembed-all-minilm-l6-v2`, `9930` documents
- endpoint p50: `147.55ms`
- endpoint p95: `190.44ms`
- engine p50: `138.92ms`
- engine p95: `176.46ms`
- Tantivy p95: `16.01ms`
- semantic recall p95: `88.32ms`
- ranking p95: `85.48ms`

### M2: Intent and Place Resolution

Goal: parse buyer language into stable structured dimensions before ranking.

Deliverables:

- Intent parser covers long-form budget phrases, typos, legal/review/builder
  phrases, and negative preferences.
- Entity resolver diagnostics map selected areas, exact society/builder names
  from ranked local results, and metro/mall/school/hospital/tech-park/landmark
  place-family aliases. A full serving-entity alias index belongs in the
  lexical/resolver milestone, not a request-time scan over the bundle.
- Resolved entities are included in internal search diagnostics.

Quality gates:

- Intent failures in the data-backed query bank fall by at least 50%.
- `DB01`, `DB02`, `DB03`, `DB09`, `DB12`, and `DB14` improve without adding
  query-specific hardcoding.
- New unit tests cover typo and 2-3 line query inputs.

Latency gates:

- intent + resolution p95 `<10ms`.

Status:

- Added punctuation-tolerant budget parsing for buyer phrasing such as `2.5Cr.`
  and typo/colloquial handling such as `witefield`, `undr`, and `gud reviews`.
- Expanded schema-backed buyer language for legal safety, review receipts,
  builder track record, commute, family, monsoon drainage, approach-road risk,
  and multi-line proof-oriented queries.
- Added `entity_resolution` diagnostics behind `/api/search?debug=true`; normal
  search responses still hide diagnostics.
- Resolver diagnostics now report selected area aliases, exact society/builder
  names from ranked local results, and generic place-family aliases such as
  `metro` and `hospital`.
- Place-family aliases, ignored resolver names, and minimum entity-name length
  live in `app/config/dag/search_intent.json`; Rust only loads the config and
  applies generic matching/de-duplication.
- Avoided request-time scans over all serving entities after review found noisy
  matches such as one-letter builder names. Full alias/entity resolution should
  be backed by a startup index in a later milestone.
- Kept FastEmbed active for thin soft-intent queries, but skip semantic recall
  when structured local recall is already sufficient or hard filters already
  define the candidate set. This is a latency/proof-safety tradeoff, not proof
  that semantic recall improved every structured query.

M2 benchmark:

- scoreable quality: `84/90`, `93.3%`
- overall including data gaps: `107/118`, `90.7%`
- result-count checks: `19/19`
- safety: `19/19`
- intent checks: `40/41`
- serving bundle: `2026-07-21-generated-context-waterford-brigade-semantic-20260722053602`
- semantic embedder: `fastembed-all-minilm-l6-v2`
- semantic index: `fastembed-all-minilm-l6-v2`, `9930` documents
- endpoint p50: `71.01ms`
- endpoint p95: `173.31ms`
- engine p50: `62.98ms`
- engine p95: `163.04ms`
- intent parse p95: `4.13ms`
- entity resolution p95: `0.26ms`
- Tantivy p95: `15.26ms`
- semantic recall p95: `95.76ms`
- ranking p95: `84.4ms`

Remaining failures:

- `DB08` still misses the legal-safety intent in a named-society premium query.
- `DB07`, `DB08`, `DB09`, `DB10`, and `DB11` still need stronger proof
  surfacing for listing price, RERA/builder records, 4BHK listing details, and
  review-snippet/community evidence.
- Data-gap cases still do not surface missing enrichment keys in the response;
  that belongs to the gap/evidence flywheel work, not M2 parsing.

### M3: Tantivy as Lexical Recall Backbone

Goal: make lexical recall fast, inspectable, and strong for names, aliases, and
fact text.

Deliverables:

- Tantivy indexes entities, aliases, property titles, society names,
  `answers_preferences`, display templates, and selected source snippets.
- Recall output records channel attribution: structured, Tantivy, semantic.
- Benchmark report shows whether each expected candidate was recalled by
  Tantivy, semantic, both, or neither.

Quality gates:

- Named-society and exact-place queries recall expected entities through
  Tantivy.
- Semantic-only proof remains forbidden.
- Recall stays at or above current baseline.

Latency gates:

- Tantivy recall p95 `<20ms`.
- Total search p95 `<200ms`.

### M4: Coordinate and Distance Search

Goal: answer "near X" and "not too far from Y" with deterministic distance
facts, not embedding guesses.

Deliverables:

- Parquet facts for coordinates on societies/projects and place entities:
  `geo.latitude`, `geo.longitude`, `geo.source`, `geo.confidence`.
- Derived distance facts for common place families:
  `distance_to_nearest_metro_km`, `distance_to_nearest_hospital_km`,
  `distance_to_nearest_school_km`, and named landmark distances where useful.
- `geo.rs` computes Haversine distance for request-specific place queries.
- Search explanations include coordinate-backed distance reasons.

Quality gates:

- Add query-bank cases for "near Forum Shantiniketan", "near metro", "close to
  hospitals", and "Whitefield but not cut off from offices".
- Distance proof reasons appear only when both endpoints have coordinates.
- Missing coordinates become data gaps, not fake proximity claims.

Latency gates:

- geo scoring p95 `<10ms` for current bundle.
- If entity count grows enough, add an R-tree or grid index before geo p95
  exceeds 20ms.

### M5: Proof Matching and Explanation

Goal: make the ranker surface the facts that actually caused the match.

Deliverables:

- `facts.rs` maps preferences to eligible serving fact keys through config.
- Result reasons include RERA, listing, review, builder, geo, and timeline facts
  when those facts support the query.
- Learning gaps include missing fact keys and candidate entities to enrich.

Quality gates:

- Proof failures in the data-backed query bank fall by at least 50%.
- `DB02`, `DB07`, `DB08`, `DB09`, `DB10`, `DB11`, and `DB12` improve.
- Safety remains perfect: semantic recall never creates proof.

Latency gates:

- fact/proof scoring p95 `<25ms`.

### M6: Constraint Relaxation

Goal: avoid empty or misleading results for over-constrained queries.

Deliverables:

- Deterministic relaxation order:
  budget tolerance, nearby area expansion, place radius expansion, timeline
  relaxation, then optional BHK relaxation only when user wording allows it.
- Response explains which constraint was relaxed.
- Relaxed candidates carry a penalty.

Quality gates:

- Add over-constrained query-bank cases.
- Zero-result broad buyer queries should go to zero only when unsupported
  inventory types are requested.
- Relaxation never hides the broken constraint.

Latency gates:

- relaxed search p95 `<250ms`.

### M7: Data-Gap Flywheel

Goal: every search improves future DAG priorities.

Deliverables:

- Search logs structured missing evidence:
  fact keys, entities, query category, and top candidate context.
- DAG priority builder consumes search gaps.
- Data-gap cases graduate to data-backed only after Parquet coverage crosses a
  defined threshold.
- Approach-road imagery gets a first-class DAG path and no longer depends on
  bootstrap validation JSON:
  `approach_road_images_weekly -> canonical_road_nodes -> approach_road_graph_facts`.
  This is separate from `external_images_weekly`, which is for project/gallery
  media. Approach-road images should carry Google Street View/Maps provenance,
  frame coordinates, heading, capture metadata when available, coverage quality,
  and source freshness before derived approach-road facts are trusted in search.

Quality gates:

- Gap sentinel cases emit expected missing fact keys.
- Tanker, maintenance, BBMP/OC, waterlogging, and builder-negative gaps map to
  explicit DAG work items.
- Approach-road query cases are marked data-backed only when the raw
  `approach_road_images_weekly` Parquet has rows for the target society or road
  segment; bootstrap-only frames stay a data gap.

Latency gates:

- gap logging is asynchronous or buffered and adds `<2ms` to request path.

### M8: Scale and Swap Boundaries

Goal: keep the embedded Rust design until scale proves a layer needs replacing.

Deliverables:

- Per-layer traits make recall backends swappable.
- Benchmarks run at synthetic sizes: 1k, 10k, 100k properties/entities.
- Decision thresholds documented for replacing a layer:
  Tantivy stays embedded unless lexical p95 or index size becomes operationally
  painful; vector scan stays local until semantic p95 exceeds budget; geo stays
  local until distance scoring needs a spatial index.

Quality gates:

- No product behavior depends on a specific backend implementation.
- Explanations remain identical across backend swaps.

Latency gates:

- 10k-property p95 `<200ms`.
- 100k-property p95 target defined before expanding inventory that far.

### M9: Search/UI Evidence Convergence

Goal: search results and property detail pages should use the same proof
contract for RERA, project milestones, pricing, nearby places, and other
DAG-backed evidence.

Deliverables:

- Result reasons carry stable proof focus handles: `surface_id`, `layer_id`,
  `fact_key`, source handle, matched label, value/distance, requested
  constraint, and reason.
- Property detail UI can open/focus the same evidence without re-parsing the
  query or adding fact-key-specific UI branches.
- RERA/project proof used in property pages is scoreable/searchable through the
  same serving facts and config ownership.

Quality gates:

- Search-to-detail handoff works for at least RERA/legal, price/listing,
  nearby-place, and review/community proof classes.
- Direct property visits still show normal default evidence; search focus is
  additive and never hides existing facts.
- No one-off frontend or Rust branches such as "if hospital", "if RERA", or
  "if Google review" for product semantics.

### M10: Curated Review and External Evidence Design

Goal: design how Google/community/RERA-derived evidence can support both search
and UI without polluting either surface with noisy or weak claims.

This milestone stays design-first until the proof contract and quality gates are
reviewed. Google review curation is intentionally not a simple ingestion task:
reviews must be scoped, deduplicated, confidence-rated, source-backed, and
separated from stronger RERA/listing facts.

Deliverables:

- Evidence taxonomy for review-derived claims: maintenance, noise, water,
  approach road, builder conduct, amenities, safety, and sentiment.
- Confidence/source policy that prevents reviews from overriding stronger RERA,
  listing, transaction, or verified project facts.
- Storage contract for curated snippets/themes as DAG facts, including source
  URL/place id, quote/snippet handle, freshness, confidence, and surface
  eligibility.
- Search usage policy: review evidence can support context and proof only when
  scoped and confidence-qualified; it cannot become a hidden ranking shortcut.

Quality gates:

- Curated review facts remain distinguishable from authoritative project facts.
- Search and UI consume the same structured evidence object.
- No request-time Google calls or LLM calls in `/api/search`.
- Benchmark cases prove review evidence helps supported review/community
  queries without hurting legal/RERA/price proof precision.

## Test Strategy

Use three complementary test layers:

1. Unit tests for intent, resolver, geo math, fact matching, and ranking math.
2. Contract tests for "semantic is recall, not proof", constraint relaxation,
   missing evidence, and explanation shape.
3. Benchmark suites from the query bank for realistic buyer journeys.

Each milestone should improve at least one measured metric without regressing:

- scoreable benchmark pass rate
- recall pass rate
- proof pass rate
- safety pass rate
- p50/p95 total latency
- p95 latency by layer
- number of data-gap cases that graduate to data-backed
- number of architecture gaps removed from the chain audit

## Immediate Next Step

Run the next proof-loop session before adding more search code:

1. Audit the actual local chain on the active branch: ontology-scaling,
   source-truth/lineage fixes, search facade/timing, intent parser changes,
   semantic recall behavior, and proof/reason surfacing.
2. Pin the current promoted bundle and run the benchmark once as the baseline.
3. Classify remaining failures. If recall/name/place misses dominate, continue
   with M3. If recall is healthy but reasons are missing, take a narrow M5 proof
   surfacing slice before broad M3 work. If missing data dominates, stop runtime
   work and feed M7/DAG enrichment.
4. Make one generic/config-driven change, rerun the same benchmark, and record
   `keep`, `revert`, `shadow_only`, or `needs_more_data`.
