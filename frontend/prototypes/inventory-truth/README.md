# Inventory Truth preview (#148)

The primary preview now runs inside the **actual PropertyPage and WorkspaceFrame**:
the atlas, photos, reviews, Save, Note and sidebar are the existing components.
An explicit dev-only adapter binds one example unit to the archived Waterford
property context. A developer-only `?scenario=` URL selects alternative evidence states;
these are **not** ten real homes. Neither unit identity nor price is inferred
from the society's name, measurements or project price range.

## Run

From `frontend`, after `npm ci`, use two terminals:

```sh
npm run dev:inventory-api
npm run dev:inventory
```

Open http://127.0.0.1:5192/property/discovered-prestige-waterford-3bhk.
Use `?scenario=uncertain`, `?scenario=reduction`, or another scenario ID to test
different states. There is no development toolbar on the property page; the
price itself is marked as an example. This uses the normal property Save
and Note controls on the preview origin; they refer to the fixture property, not
to a newly promoted canonical unit. They do not modify storage on another origin.

The integrated mode is not a production feature flag. A normal production build
does not contain the mock API client, schemas, adapter or preview styles. Before
production wiring, the property-bound identity view must come from #144's promoted
unit contract. The society's old price and size are replaced in the preview
identity, and its old price is not emitted in page metadata or JSON-LD.

Google rendering requires the existing `VITE_GOOGLE_MAPS_API_KEY` in local Vite
configuration. Without a key the real map-unavailable state remains; prices,
photos, reviews and notes still work. Do not commit credentials. The captured
context is an archived renderer fixture, not a production evidence-admission claim.

The earlier isolated search/saved-unit scenario harness remains available with
`npm run dev:inventory-scenarios` at http://127.0.0.1:5191/. It exercises the
future canonical-unit search/save contract with separate versioned storage.
It is no longer the primary property-design preview. Both hosts share the same
API, receipt renderer and scenario definitions.

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

Browser tests start the API and both preview servers if needed. Install Playwright Chromium
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
that harness does not claim that a live search engine proved the hospital match.
Saved homes retain IDs only; no invented “since saved” deltas are shown.

#144 still owns collectors, source adjudication, real canonical-home matching,
production DAG assets, serving promotion and persistent change events. Before
production adoption, replace the mock inputs with those promoted products and
prove materializer parity. Keep the Rust aggregation and shared UI contract;
delete this preview host/fixture adapter at that cutover.

See [design decisions and screenshots](../../../../docs/inventory-truth-design.md).
