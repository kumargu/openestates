# Source adapters (pending)

One JSON file per external source. Declares:

- `adapter_id`, `source_type`, `trust_tier`
- which skills/assets consume it
- field mapping to canonical `fact_key`s from `concern_taxonomy.json`
- refresh cadence reference into `asset_registry.json`

Examples to add: `rera.json`, `google_places.json`, `reddit_theme.json`, `legacy_seed.json`, `openstreetmap_roads.json`.

No society names or road names in this directory — only adapter contracts.
