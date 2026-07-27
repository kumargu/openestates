# Product config (`app/config/product/`)

Buyer-facing landing copy. **No entity instances, no fact values.**

| File | Meaning |
|------|---------|
| `discovery_home.json` | Landing promise, rotating quotes, curated shelves with `search_query` |
| `evidence_sections.json` | Deprecated compatibility copy. Source of truth moved to `app/config/dag/evidence_sections.json`. |

Loaded by Rust at startup (`discovery.rs`). Property evidence sections are loaded from DAG config.

`approach_road_visuals` (Street View frames) is **lake data**, not config — see `app/config/coverage.json`.
