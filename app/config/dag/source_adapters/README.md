# Source adapters (pending)

One JSON file per external source. Declares:

- `adapter_id`, `source_type`, `trust_tier`
- which skills/assets consume it
- field mapping to canonical `fact_key`s from `concern_taxonomy.json`
- refresh cadence reference into `asset_registry.json`

Current adapters:
- `reddit_theme.json` — derived resident concern signals.
- `opencity_environment.json` — public environmental datasets such as groundwater, drains, flood locations, and lakes/wetlands; joins must happen offline during DAG materialization.

Examples to add: `rera.json`, `google_places.json`, `openstreetmap_roads.json`.

No society names or road names in this directory — only adapter contracts.
