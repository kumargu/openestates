# Lake config (`app/config/lake/`)

Describes **where Parquet lives** and **how to sync** with S3. Not the data itself.

| File | Meaning |
|------|---------|
| `layout.json` | Zones (raw/silver/gold/serving/manifests), LakeKey patterns, table columns |
| `sync.json` | `OPENESTATES_LAKE_URL`, S3 examples, never-sync paths (`data/cache/`) |

Local default lake root: `data/lake/`

Same logical keys on disk and S3 — see `backend/src/assets/paths.rs`.
