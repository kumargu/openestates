# Domain and evidence migration

Base: audited main `2f0f4e27`. Worktree: `openestates-contracts`; unrelated OSM work remains in the original checkout. The two September 21 audit reports are retained alongside this note.

## Chain audit and baseline

`18603071` introduced journeys; `f41e7475` tightened exact receipt handoff; `afc4432b` updated interaction checks. The remaining old paths crossed measurement, proof and client boundaries. Classifications: area projection and inventory admission are architecture gaps; missing inventory observations are data gaps; category/numeric receipts are proof gaps; the retired benchmark envelope is an architecture gap.

The audited Parquet inspection is preserved at `/tmp/openestates-audit-20260921/live-records.json`, with the pointer, bundle, counts and eligibility findings recorded in `search_data_ui_audit.md`. No observations are reconstructed from URLs or timestamps in this migration.

New baseline: `/tmp/openestates-contracts-baseline.log`. Extending the existing source → Parquet → bundle → production-router scenario reproduced three super built-up listings satisfying a carpet-only constraint. This regression catches a real bug; it does not change a frozen expectation.

## Checkpoint 1: measurement and durable witnesses

Producers: sourced listing aggregates and serving facts. Consumers: property projection, numeric/category evaluation, journey proof projection and proof resolution.

- Preserve measurement basis, units, scope, range and observation reference. Stop copying listed area into both carpet and super built-up fields.
- Delete runtime-field numeric evidence. Numeric evaluation uses sourced inventory measurements or eligible serving observations, retaining its typed constraint and witness, including an unsatisfied bound.
- Category-distance predicates reference their durable source observation directly.
- Remove UI destinations from signed proof identity. Destination projection remains a separate presentation concern during the next checkpoint.
- Delete seed-polygon receipt synthesis and its dedicated tests. Remove unused graph property/society projections and their private fixtures. Keep serving, geometry and parser coverage.
- Remove the orphan Area Tracker UI, client and endpoint. Restoration requires separate product work over promoted facts and the durable search event stream.

Validation: `/tmp/openestates-contracts-tests.log`: materializer vertical contract, 15 controlled semantic contracts and 22 journey API contracts pass. The pinned live promotion gate remains separate. `/tmp/openestates-contracts-check.log`: `cargo check` passes. The vertical contract opens all emitted references for BHK, numeric and spatial queries and asserts super built-up cannot establish carpet eligibility.

## Remaining checkpoints

Inventory admission/reporting; public DTO schema generation and boundary validation; proof outcomes independent of UI destinations; snapshot-pinned context reads; bounded selected-home hydration; migrate the benchmark; real API browser journeys; consolidate obsolete tests and documentation; full lint/build/integration/hardcoding gates and screenshots.
