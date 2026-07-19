# Bootstrap config (`app/config/bootstrap/`)

**Import policies only.** Tells the pipeline how to infer nodes and edges from RERA, geocoding, etc.

| File | Meaning |
|------|---------|
| `policy.json` | Where bootstrap output goes (lake Parquet), source priority, confidence caps |
| `edge_inference.json` | Rules for `served_by_road`, `in_area`, `built_by` — methods + min_confidence |

Never put `society:…`, `road:…`, or specific edges in this folder.

Output: `data/lake/gold/.../entities.parquet`, `edges.parquet`, `facts.parquet`.
