# Product config (`app/config/product/`)

Buyer-facing landing copy. **No entity instances, no fact values.**

| File | Meaning |
|------|---------|
| `discovery_home.json` | Landing promise, rotating quotes, curated shelves with `search_query` |

Loaded by Rust at startup (`discovery.rs`). Property evidence sections are owned by `app/config/dag/evidence_sections.json`.

Street View resolves at runtime only from an explicit OSM entrance or exact OSM
approach corridor. It is visualization coverage, not durable entrance truth.
