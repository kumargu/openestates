# Inventory Truth preview (#148)

A fixture-driven search → property → evidence → saved-home journey. It reuses the
real `PropertySceneCard`, photo viewer, `PropertySearchStrip`, brand and design
tokens. Its entry point is separate from the production app. The scenario picker
switches alternative data states; those scenarios are **not** ten real homes.

## Run

From `frontend`, after `npm ci`, use two terminals:

```sh
npm run dev:inventory-api
npm run dev:inventory
```

Open http://127.0.0.1:5191/ or
http://127.0.0.1:5191/property/four-ads?context=inventory-preview&qf=q148.
Use the scenario selector for all ten states. Save/unsave uses a separate,
versioned preview storage key and never changes production saved homes.

The Rust service binds **loopback only**, port 4019. The launch command compiles
the isolated crate, materializes a fresh temporary Parquet snapshot, prints its
path, then serves it. Snapshots remain available for inspection after shutdown.
The media proxy uses the existing local API on port 4000 for the captured
Waterford exterior; without that media service, shared image-failure behavior
omits the photograph. No stock photo is substituted for the home.

## Data contract

`backend/prototypes/inventory-truth/fixtures/scenarios.json` is the explicitly
synthetic input. Offline materialization produces:

```
snapshot/
  homes.parquet          typed home identity and physical measurements
  observations.parquet   typed INR, source/ad identity, states and predecessor IDs
  signals.parquet        receipt-bound identity agreements/disagreements
  policy.json            versioned admission policy
  manifest.json          SHA-256 file hashes and immutable snapshot identity
```

All prices come from integer Parquet columns. There are no price arrays in React.
Rust loads and validates the whole snapshot at startup, then assembles catalog
and detail from the same projection. Handlers only retrieve in-memory views.

- `GET /api/inventory/homes`: summaries, contract version, immutable snapshot ID.
- `GET /api/inventory/homes/{id}?snapshot={id}`: same summary, advertisements,
  likely candidates, separate comparable homes and reliable registrations.
- Mismatched snapshot: 409; missing home: 404; missing snapshot parameter: 400.

An exact-home active observation can contribute its price; a likely match,
inactive observation, comparable or registration cannot. Repeated observations
count as one advertisement, keyed by **provider + advertisement ID**. A price
change needs an explicit predecessor from that same advertisement. Neither
observation dates nor ingestion dates establish state, precedence or freshness.
Source links are absent in these invented observations rather than fake live URLs.

The loader rejects duplicate/current contradictions, conflicting confirmed
measurements, dangling bindings/history, cross-ad history, cycles, orphaned
history, malformed schemas and snapshot tampering. The materializer refuses to
overwrite an existing snapshot. Its manifest is deliberately not a production
serving manifest, and the policy is listed as `contract_only` in the DAG manifest.

## Validation and regeneration

```sh
npm run contracts:inventory
npm run build:inventory
npm run lint
npm test
npm run test:inventory-browser
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --manifest-path ../backend/prototypes/inventory-truth/Cargo.toml
git diff --check
```

Browser tests start the two preview servers if needed. Install Playwright Chromium
or set `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` to an installed Chrome executable.
The browser suite uses the real Parquet API; only its failure tests intercept
responses. Screenshots are written to `frontend/test-results/inventory`.

The Rust DTOs generate JSON schemas and TypeScript. Both API responses are
validated against those schemas and the requested snapshot/home identity before
entering UI state. No production wire DTO is changed.

## Deliberate boundary

This is a design and aggregation contract, not production identity resolution.
Bindings, current markers, comparable eligibility and registration reliability
are explicit mock assertions. A fixed carried search sentence exercises layout;
this preview does not claim that a live search engine proved the hospital match.
Saved homes retain IDs only; no invented “since saved” deltas are shown.

#144 still owns collectors, source adjudication, real canonical-home matching,
production DAG assets, serving promotion and persistent change events. Before
production adoption, replace the mock inputs with those promoted products and
prove materializer parity. Keep the Rust aggregation and shared UI contract;
delete this preview host/fixture adapter at that cutover.

See [design decisions and screenshots](../../../../docs/inventory-truth-design.md).
