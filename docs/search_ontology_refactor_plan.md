# Search Ontology Refactor

Search runtime cleanup is architecture-first work. The goal is to move buyer
vocabulary and fact-key ownership into DAG config and DAG-backed serving facts
while keeping the runtime generic. Parity is required for mechanical refactors,
but it is not required when the current behavior is only working because of
hidden locality, landmark, or brand aliases.

## Reset: 2026-08-01

The latest cleanup intentionally broke the old "win by alias" path:

- `search_intent.json` now keeps only broad Bengaluru regions and generic place
  families.
- Named localities, roads, societies, schools, hospitals, tech parks, malls,
  metro stations, aliases, coordinates, and distance facts must come from the
  DAG/serving bundle.
- The fuzzy area alias resolver was removed because it turned unrelated words
  into area filters, such as `easy` resolving to `East Bengaluru`.
- Benchmarks may drop after this cleanup. That is acceptable until the
  serving-backed place resolver and proof path are measured.

This reset changes the plan from "preserve old results" to "preserve the
architecture, then prove quality with a pinned bundle."

## Invariant

Production search runtime may contain generic mechanics, but not product vocabulary branches. `backend/src/search/**` and `backend/src/routes/search.rs` should load, validate, index, score, and explain configured records; they should not own closed product lists such as hospital aliases, metro aliases, listing fact-key families, or preference prefix semantics.

Place ontology must not become a filter list. Config may define dimensions,
place families, scoring policies, source priorities, and buyer-facing labels.
It must not contain specific localities or landmarks as parser shortcuts.

Intent extraction may identify:

- structural slots: BHK, budget, numeric constraints, broad region;
- generic place family: school, hospital, metro, tech park, mall, landmark;
- relation: near, within, far from, avoid;
- unresolved mention text for a serving entity/place resolver.

Intent extraction must not decide that `Hoodi`, `Bagmane`, or a school name
means a specific area. That is the serving resolver's job.

## Parity Gates

- Read `app/config/dag/manifest.json` and the relevant config file before editing.
- Start with a chain audit: map touched search/config files and local commits to
  the current roadmap milestone, then flag duplicate or accidentally-default
  experimental paths before adding more code.
- Run `python3 scripts/audit_search_hardcoding.py --mode production-search`.
- Run focused tests for touched logic.
- Run search contracts:
  - `cargo test --manifest-path backend/Cargo.toml --test search_quality_contract`
  - `cargo test --manifest-path backend/Cargo.toml --test search_quality`
  - `cargo test --manifest-path backend/Cargo.toml --test search_efficiency_contract`
- Run the relevant buyer-language benchmark before and after behavior changes,
  using the same serving bundle and spec.

Any ordered-result, proof-key, missing-gap, or guardrail diff is a regression
for parity work unless the task explicitly approves a behavior change.
If a change is structurally useful but not quality-proven, keep it behind
shadow/diagnostic behavior and record the proof-loop decision before continuing.

For architecture cleanup, the gate is different: document the expected quality
drop, add a regression that prevents the old hardcoding from returning, and
open a follow-up proof-loop task to recover quality through DAG facts or generic
runtime machinery.

## Current State

Completed locally:

- Removed locality and landmark aliases from search intent config.
- Removed fuzzy area alias resolution from runtime search.
- Added/updated tests that prove named localities do not resolve through parser
  config.
- Kept generic ontology phrases and fact-key expansion in config.
- Removed the fastText residual intent classifier, its runtime dependency,
  training binary, model loading, and shadow diagnostics. Configured
  deterministic intent plus serving-backed entity and capability indexes are
  now the only production search path.
- Compiled structured constraints into backend-owned result sets. Top-level OR
  branches retain their order and exact eligible counts; the frontend renders
  those sets without inventing new price, BHK, or area groups.
- Kept BHK, named-society, area, and other hard constraints intact during
  fallback. Budget-only expansion is bounded by the configured 10% and 25%
  tiers and carries an explicit buyer tradeoff label.
- Made missing soft evidence additive. Only preferences explicitly marked
  `required` in the fact registry can gate eligibility.
- Stopped named-entity scopes before configured budget operators, BHK clauses,
  and evidence constraints so natural buyer queries do not turn whole suffixes
  into entity names.
- Reduced the buyer API to `query`, backend-owned `resultSets`, `totalMatches`,
  optional `areaContext`, and `state`; parser, diagnostics, enrichment gaps,
  and relaxation machinery remain internal.

Verification checkpoint — 2026-08-22:

- Rust library: 620/620 passed.
- Search/DAG contracts: asset executor 12/12, search efficiency 11/11,
  search quality 4/4, search quality contract 5/5, serving runtime 5/5.
- Frontend: 145/145 tests passed; TypeScript and production Vite build passed.
- Fresh-server smoke test: 50/50 passed against promoted bundle
  `clean-serving-v7-2026-08-16` (86 runtime properties).
- Production search hardcoding audit passed with no blocked config aliases. Its
  only two warnings are the existing comment-only `office` examples in
  `backend/src/search/geo.rs`.
- `cargo check`, Rust formatting, and diff whitespace checks passed. The macOS
  linker still emits its existing large `__eh_frame` advisory during tests.

Known follow-up audit findings:

- `nearby_place_categories.json` and `ui_surfaces.json` contain brand/name
  markers. `cult` is intentional because the current fitness layer is curated
  to Cult gyms. Other markers such as transit line names or office-park names
  need review before they become search/runtime behavior.
- Several tests and docs still use Whitefield/Sarjapur/Bagmane examples. That
  is acceptable for historical fixtures and data-specific benchmark specs, but
  parser/runtime tests should prefer broad regions or generic named-place
  fixtures.
- Recommendation recall still has an `area_alias_bhk` channel. With the current
  broad-region config it is low-risk, but M4 should rename/review it so it
  cannot become locality alias matching again.

## Next Proof Loop

Detailed execution plan:
`docs/search_fact_grounded_proof_loop_plan.md`.

Use the current promoted bundle as the first proof-loop baseline:

- bundle version: `clean-serving-v7-2026-08-16`
- materialization: `fe16ca1a-a301-4306-ae10-06cc27f792e2`
- scope: 697 entities, 13,310 facts, 3,353 graph edges, 12,969 search
  metadata rows

Run one small loop at a time:

1. Profile the bundle and list scoreable fact families.
2. Build a query suite from facts that actually exist in that bundle.
3. Split cases into:
   - broad ontology queries;
   - named-place queries;
   - RERA/legal queries;
   - Google/review queries;
   - data-gap sentinels.
4. Run baseline search quality and classify failures before changing code.
5. Fix only one dominant layer per loop: intent, resolver, proof, ranking,
   embedding, or data.
6. Keep the change only if the benchmark decision is `keep`; otherwise revert
   or mark `shadow_only`.

## Removal Queue

- Review nearby place category aliases in
  `app/config/dag/nearby_place_categories.json`. Keep generic families and
  intentional curation policies such as Cult-only fitness; remove accidental
  place/entity shortcuts from runtime search behavior.
- Keep BHK-scoped listing fact-key derivation in `app/config/dag/fact_registry.json`.
- Keep legacy `preferences: Vec<String>` only as API compatibility; expand it through schema helpers.
- Narrow hardcoding audit enforcement to production search runtime, with tests allowed to keep explicit buyer phrases as fixtures.
