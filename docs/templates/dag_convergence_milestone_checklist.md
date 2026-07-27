# DAG Convergence Milestone Checklist

Use this checklist for every M0-M8 diff before requesting buddy review.

Milestone:
Owner:
Date:

## Boundary Checks

- [ ] Config/code boundary checked
- [ ] Product semantics added to Rust/TypeScript are structural, justified, or moved to DAG config / serving facts
- [ ] API compatibility checked with affected response fields, frontend API types, and contract/snapshot tests recorded
- [ ] No request-path config/file IO added; DAG config is loaded at startup, baked into the serving bundle, or behind explicit cache refresh

## Latency, Cache, And Computation

- [ ] Affected endpoint(s) listed: property detail, recommendations, surfaces/map, search, Area Tracker, or none
- [ ] Before/after p95 or p99 recorded with command, corpus/bundle size, and decision; if not measurable, reason recorded
- [ ] Frontend request count for changed views recorded, including added/removed calls, batching/dedup, cancellation, and N+1 risk
- [ ] Recommendation cache key inputs checked: property id, engine version, policy version/hash, serving-bundle version
- [ ] Policy-version and serving-bundle invalidation tests added or updated where cacheable behavior changed
- [ ] Computation placement checked: repeated config parsing avoided, derived structures prebuilt, per-request loops bounded, intentional recomputation justified

## Storage And Serving Efficiency

- [ ] Serving bundle metrics recorded when serving facts/API metadata changed: entity rows, fact rows, edge rows, search metadata rows, bundle bytes
- [ ] Tantivy/semantic index size and document fanout checked when search/recommendation recall changed
- [ ] Duplicate fact/source audit performed or explicitly not applicable
- [ ] Source metadata shape checked for bounded fields and avoided bulky repeated provenance
- [ ] Startup load time and estimated request-path memory impact recorded when serving shape changed

## Tests

- [ ] Backend tests added or updated
- [ ] Frontend tests added or updated
- [ ] Config-only regression covered, if this milestone promises config-only extension
- [ ] Manual/browser verification recorded, if buyer UI changed

## Algorithm Parity

- [ ] Search top-k, parsed slots, proof reasons, matched fact keys, and source/proof labels compared before/after for representative queries
- [ ] Recommendation branch IDs, lenses/channels, order, and fallback behavior compared before/after when recommendation logic/config changed
- [ ] Conflict-resolution winners, source caps, and proof-quality thresholds compared before/after when source/resolution policy changed
- [ ] Any ranking, recall, proof, or recommendation deltas documented with reason and test fixture update

## Cache Dependency Matrix

- [ ] Search cache inputs and invalidation triggers listed, if touched
- [ ] Recommendation cache inputs and invalidation triggers listed, if touched
- [ ] Detail/surface/Area Tracker cache inputs and invalidation triggers listed, if touched
- [ ] Tests prove stale entries are bypassed after policy, engine, serving-bundle, or source-display changes

## Review Gate

- [ ] `python3 scripts/audit_dag_convergence.py --max-findings 0` reviewed
- [ ] Audit scale delta recorded: config-derived term count, total findings, runtime hotspots, and justified increases
- [ ] Diff audited for new hardcoded product semantics
- [ ] Buddy review completed
- [ ] High-severity buddy findings fixed or explicitly deferred with owner and reason
