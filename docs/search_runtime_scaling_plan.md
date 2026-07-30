# Search Runtime Scaling Invariants

- The live search server serves from one promoted serving bundle snapshot.
- The request path must not read Parquet or object-store data.
- Current ranking, recall, proof, diagnostics, and response shape define correctness.
- Runtime concurrency changes must preserve ordered result IDs and explanations for the same query, bundle, and config.
- Caches are optimization only; cache keys must include bundle, scoring, and engine identity.
- DAG runs build immutable new bundle versions outside the live request path.
- Reload must build a complete new snapshot before swapping it into traffic.

## Runtime Roles

- `openestates-api` serves search from the current immutable runtime snapshot.
- `openestates-run-assets` builds new immutable serving bundle versions outside the request path.
- Promotion updates the serving-bundle `current.json` pointer, then the API admin reload builds and atomically swaps a full new snapshot.
- Existing `/api/admin/asset-runs` behavior is retained for local/admin operation, but production scheduling should run DAG work externally or behind a one-run lease.
