# App config (Git source of truth)

**Config = schemas & policies. Data = `data/lake/` Parquet.**

## Start here

1. `manifest.json` — root index  
2. `coverage.json` — **full audit**: what's included, what's in lake, what's still in Rust  

## Packages

| Folder | Primitives defined |
|--------|-------------------|
| `dag/` | **nodes**, **edges**, **leaves**, pipeline, enrichment, UI surfaces |
| `bootstrap/` | Import policies, edge inference rules |
| `lake/` | Parquet layout, S3 sync |
| `product/` | Landing copy, evidence panel layout |
| `runtime/` | Env vars, local paths |

Each folder has its own `README.md` with a per-file glossary.

## JSON `_comment` fields

Top-level `"_comment"` keys in JSON files are for humans/agents — ignored by serde loaders.

## Regenerate

```bash
python3.10 pipeline/tools/build_dag_json.py
```
