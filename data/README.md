# Data directory

**`data/` is for enriched artifacts and local rebuildable caches — not application config.**

| Path | Contents | Sync to S3? | Runtime input? |
|------|----------|-------------|----------------|
| `lake/` | Parquet + manifests (raw → silver → gold → serving) | **Yes** (entire lake root) | **Yes** (promoted serving bundle only) |
| `cache/` | Skill results, tantivy hydration, rebuildable indexes | **No** | Hydration only |
| `validation/` | Eval baselines, QA reports | Optional | No |
| `seed/` | Legacy flat JSON import input | No | No (bootstrap import only) |
| `search/`, `product/` | Legacy registries — migrating to `app/config/dag/` | No | Until loaders switch |

## Lake layout

See `app/config/lake/layout.json` for zone definitions and key patterns.

```
data/lake/
  raw/           # crawl snapshots
  silver/        # normalized facts per skill/asset
  gold/          # kg_society_view: entities, edges, facts, fact_annotations
  serving/       # search_bundle versioned folders + tantivy
  manifests/     # current.json pointers, materialization records
```

## S3

Set `OPENESTATES_LAKE_URL=s3://bucket/lake` — same logical keys as local.

See `app/config/lake/sync.json` for full sync policy.

## Config

All schemas, policies, leaf definitions → **`app/config/`** (Git).
