# Semantic Search and Curated Review Record

Updated: 2026-08-04

This is the single engineering record for the semantic-router experiment, its removal, and the deterministic replacement work.

## Current decision

FastEmbed, ontology routing, vector recall, and semantic scores were removed. The experiments did not demonstrate enough incremental buyer-search value to justify their dependencies, artifact lifecycle, request-path complexity, or parallel diagnostics contract.

The useful work was retained:

- deterministic DAG-fact ranking and proof;
- curated review facts and exact-excerpt receipts;
- configured intent vocabulary and place categories;
- structured, lexical, entity, geo, and serving-fact recall;
- fact-first benchmark cases and hardcoding audits.

Production serving pointers and review facts were not changed. Search remains local and deterministic.

## Current search boundary

```text
anchored buyer query
  -> deterministic constraint and relation parsing
  -> serving entity/place/category resolution
  -> structured + Tantivy + geo/fact recall
  -> DAG fact scoring and proof
```

Contextless vague text may abstain. Area, BHK, budget, exclusions, hard constraints, candidate membership, ranking evidence, and receipts are controlled by deterministic parsing and serving facts.

The following paths were removed:

- property-vector candidate ranking and semantic score tie-breaking;
- `semantic_score` in Rust and frontend contracts;
- semantic debug/shadow payloads and `/api/search?debug=` behavior;
- compiled-intent and semantic benchmark banks that had become a parallel product contract.

Internal metrics remain available to Rust tests without being serialized to buyers. The normal `/api/search` response has no debug mode.

## Multi-anchor proximity replacement

The deterministic parser now preserves up to four configured relation clauses independently. Named places and coordinate-backed areas resolve through serving entities; generic place families resolve through configured `nearby_*` fact families.

For the discovery query:

`3BHK in Whitefield near Manipal Hospital Whitefield and near International Tech Park Bengaluru (ITPB)`

the benchmark does not prescribe a society. It requires the top result to satisfy the hard BHK/area intent and carry both hospital and tech-park reasons and proof focuses.

For a partially resolvable query such as:

`3bhk near Whitefield close to kids school and near my wife office in Marathahalli`

Whitefield and configured school evidence remain usable. The personal office clause is not mapped to an arbitrary office or a similarly named business; it records an internal `geo.proximity_anchor` gap until a named serving entity or user coordinate is available.

Overlapping category phrases use longest-token matching, so `tech park` does not also request the generic public-park fact family.

## Query preservation audit

The deletion audit found 470 query occurrences across removed experiment files,
representing 115 unique strings. Generated reports and model comparisons remain
deleted, but the source query banks were reviewed separately instead of being
discarded with those outputs.

Fourteen credible, society-agnostic queries were recovered into the active
fact-first bank:

- ten deterministic multi-constraint controls covering lifecycle + reviews,
  lifecycle + 3BHK + listing proof, reviews + school access, 3BHK + listing
  proof + zero RERA complaints, and metro + school access;
- four 3BHK + brochure/land-document queries retained as explicit
  `intent_gap` cases because deterministic search currently preserves the BHK
  constraint but does not prove the required document clause.

The 50-case active bank therefore keeps difficult queries even when they expose
a miss. In particular, the metro + school paraphrase that currently produces a
school-only top result remains as a proof gap; it was not deleted or weakened.
Single-fact classification paraphrases and unanchored router relationship/hard-
negative experiments were not promoted into the buyer-quality bank. Their
supported intent classes remain covered by fact-first cases and focused Rust
parser/ranking tests.

The new multi-anchor case remains society-agnostic. It does not prescribe
Prestige Waterford or any other result id; the top result must prove both named
anchors.

## Curated fixture status

The router-only curated fixture and generated artifacts were removed with the
experiment because no deterministic test consumed them after the router path
was deleted. Review-derived production facts still rank and prove through the
ordinary deterministic fact contract.

## Verification recorded

- Semantic-removal before/after benchmark parity: exact for deterministic intent, ordered results, reasons, proof focuses, and gaps.
- Pinned-bundle fact-first HTTP benchmark after query recovery: `243/262`
  checks overall (`92.7%`) and `231/246` scoreable checks (`93.9%`) across 50
  cases. Endpoint p95 was `54.23 ms`; the generic named multi-anchor case is
  `6/6`.
- Ten recovered deterministic multi-constraint controls passed `36/38` checks.
  The retained metro + school paraphrase accounts for the two misses.
- The benchmark runner now fails if the live `/api/health` serving-bundle
  version differs from `required_serving_bundle_version`; the recorded run used
  materialization `909a8bd0-3af0-42af-ae26-ba493f54174a`.
- Rust library tests: `476` passed in focused/full runs; complete locked Cargo
  suite passed across library, binaries, integration contracts, and doc tests.
- Python benchmark evaluator tests: `5` passed.
- Search hardcoding audit: zero blocked production-search alias findings before multi-anchor work; rerun required at final verification.
- Search hardcoding audit after multi-anchor work: zero blocked production-search alias findings.
- Existing 35 fact-first cases retain exact ordered-result/reason/proof parity
  except `FF-S-003`, where the incorrect public-park proof triggered by the
  phrase `tech park` was deliberately removed; ordered ids and requested
  tech-park proof remain unchanged.

No test writes production serving pointers or promotes fixture data.

## Production identities checked before implementation

| Item | Identity |
|---|---|
| Production catalog release | `1092cf4a-ade6-4bc2-ad9c-7749eced7a59` |
| Production serving materialization | `909a8bd0-3af0-42af-ae26-ba493f54174a` |
| Read-only Google source pool inspected for fixture review | `2831535e-36fd-4545-a357-82b7a22a962b` |

None of these pointers was mutated.
