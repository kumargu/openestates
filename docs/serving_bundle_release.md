# Catalog and serving bundle lifecycle

The development catalog has one commit point:

```text
manifests/catalog/dev.json
```

It points to the current immutable roster and format-12 serving bundle, and
retains the immediately previous generation for `undo`.

## Data flow

```text
scoped society DAG → immutable society gold snapshot
active society snapshots → offline global topology/proximity derivation → one serving bundle
validated bundle → CAS update of dev.json
```

Society collection is isolated. Adding or replacing a society does not rerun
the other society DAGs. Bundle assembly still reads every active snapshot so
the API and search process load one compact Parquet bundle and one Tantivy
index, never per-society files.

The immutable roster referenced by `manifests/catalog/dev.json` is authoritative.
`data/catalog/bootstrap_roster.json` is used only for the first `rebuild` when
no catalog pointer exists; commands never rewrite it. Each active generation
stores a roster that pins the exact gold snapshots used by that bundle.

## Commands

```bash
cd backend
cargo run --bin openestates-catalog -- add <society-seed.json>
cargo run --bin openestates-catalog -- remove <society-id>
cargo run --bin openestates-catalog -- rebuild
cargo run --bin openestates-catalog -- undo
```

- `add` is an upsert and atomically replaces a matching RERA/runtime identity.
- `remove` omits the society from the next generation.
- `rebuild` recollects every seed in the authoritative roster, then rebuilds
  global topology and proximity while assembling the new bundle. It does not
  use old society gold or serving output.
- `undo` swaps current and previous generations.

Every mutating command builds and validates first, then compare-and-swaps the
pointer. A corrupt snapshot, structural error, empty projected property set,
or CAS conflict leaves `dev` unchanged.

## Validation boundary

Activation blocks on corrupt artifacts or schema, duplicate canonical
identities, dangling or contradictory relations/evidence, an explicit
property entity linked to zero or multiple societies, an empty catalog, or a
failed operation. Missing optional enrichment and serving quarantines are
warnings. Geographic search continues to fail closed when topology evidence
is unavailable.

The bundle validator verifies hashes, row counts, typed Parquet schemas,
Tantivy artifacts, projected properties, eligibility, evidence relations, and
every local media reference. There is no frontend manifest mutation and no
separate create/validate/promote workflow.

After a successful pointer swap, cleanup retains only current and previous
bundles, rosters, and referenced gold/topology snapshots. It also removes the
retired release, environment, serving-materialization, and asset-pointer data.

## Runtime

The Rust API reads `manifests/catalog/dev.json`, loads its one format-12 bundle,
hydrates one local Tantivy index, and keeps the serving state in memory. There
is no alternate runtime pointer or materialization override.

Project media stays in immutable lake keys under
`media/images/sha256/{prefix}/{sha256}.{extension}` and is streamed by the
backend. Cache output is rebuildable and never source truth.
