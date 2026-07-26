# Product config (`app/config/product/`)

Buyer-facing copy and UI section layout. **No entity instances, no fact values.**

| File | Meaning |
|------|---------|
| `discovery_home.json` | Landing promise, rotating quotes, curated shelves with `search_query` |
| `evidence_sections.json` | Property page panels: approach road, waterlogging, surroundings, lifecycle — presentation variant + fact keys |

Loaded by Rust at startup (`discovery.rs`, `properties.rs`).

`approach_road_visuals` (Street View frames) is **lake data**, not config — see `app/config/coverage.json`.
