# Search Ontology Refactor

Search runtime cleanup is strict-parity work. The goal is to move buyer vocabulary and fact-key ownership into DAG config while keeping ranking, response shape, proof keys, and missing-evidence behavior stable.

## Invariant

Production search runtime may contain generic mechanics, but not product vocabulary branches. `backend/src/search/**` and `backend/src/routes/search.rs` should load, validate, index, score, and explain configured records; they should not own closed product lists such as hospital aliases, metro aliases, listing fact-key families, or preference prefix semantics.

## Parity Gates

- Read `app/config/dag/manifest.json` and the relevant config file before editing.
- Start with a chain audit: map touched search/config files and local commits to
  the current roadmap milestone, then flag duplicate or accidentally-default
  experimental paths before adding more code.
- Run `python3 scripts/audit_search_hardcoding.py --mode production-search`.
- Run focused tests for touched logic.
- Run search contracts:
  - `cargo test --manifest-path backend/Cargo.toml --test search_quality_contract`
  - `cargo test --manifest-path backend/Cargo.toml --test search_semantic_quality_contract`
  - `cargo test --manifest-path backend/Cargo.toml --test search_efficiency_contract`
- Run the relevant buyer-language benchmark before and after behavior changes,
  using the same serving bundle and spec.

Any ordered-result, proof-key, missing-gap, or guardrail diff is a regression unless the task explicitly approves a behavior change.
If a change is structurally useful but not quality-proven, keep it behind
shadow/diagnostic behavior and record the proof-loop decision before continuing.

## Removal Queue

- Keep nearby place category aliases in `app/config/dag/nearby_place_categories.json`.
- Keep BHK-scoped listing fact-key derivation in `app/config/dag/fact_registry.json`.
- Keep legacy `preferences: Vec<String>` only as API compatibility; expand it through schema helpers.
- Narrow hardcoding audit enforcement to production search runtime, with tests allowed to keep explicit buyer phrases as fixtures.
