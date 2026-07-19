# DAG config package

**Read order (minimize context):**

1. `manifest.json` — what exists, version, agent routing
2. **One file per task** — do not load the whole tree

| Task | Read only |
|------|-----------|
| New fact / theme leaf | `concern_taxonomy.json` |
| Entity types / edges | `ontology.json` |
| Source conflict rules | `resolution_policies.json` |
| Crawl schedule / skip | `crawl_policies/*.json` (Phase 1+) |
| Asset graph | `asset_registry.json` (Phase 1+) |

Runtime (search/UI) reads **serving bundle Parquet**, not these files.

Legacy registries (`data/search/fact_schema_registry.json`, `data/product/livability_theme_registry.json`) remain until Phase 2 merge.
