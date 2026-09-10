# Issue 118 consolidation record

Issue 118 converged the search runtime and development catalog around one
typed, evidence-carrying serving snapshot. Historical experiment diaries and
temporary materialization IDs were removed because they described retired
execution paths rather than the current architecture.

## Current contracts

- `manifests/catalog/dev.json` is the only catalog commit point.
- `data/catalog/bootstrap_roster.json` is immutable first-cutover input.
- Catalog `add`, `remove`, `rebuild`, and `undo` stage and validate immutable
  artifacts before one compare-and-swap pointer update.
- Serving bundles contain facts, relations, inventory, spatial indexes, and
  Tantivy recall. Global topology and proximity are derived offline during
  bundle assembly.
- Search executes typed predicates with durable evidence; recall alone never
  establishes eligibility or a buyer-facing reason.
- Chained revisions carry a compact authenticated `IntentAst` token. The
  backend supports initial, refine, expand, replace, exclude, correct, and
  rephrase. Browser-owned Undo activates a stored parent token; Fresh starts a
  new initial search.

## Verification baseline

The frozen query bank and
`backend/tests/search_conversational_semantics_contract.rs` remain the semantic
authority. Catalog lifecycle contracts cover atomic activation, corrupt
artifacts, stale CAS, retention, and undo. Search revision contracts cover
authenticated compact tokens, result fingerprints, deterministic duplicate
submissions, limits, zero-result preservation, relative numeric corrections,
selected-property consequences, and rebinding portable intent after a catalog
generation changes. Search-event contracts verify that evidence gaps are
written asynchronously to immutable lake keys rather than mutable graph state.

The migration catalog target is 71 societies and 159 property configurations.
Any future change must record a new before/after benchmark artifact rather than
adding another execution diary here.
