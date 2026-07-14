# Backend Benchmark Gates

OpenEstates backend changes should improve one or more measured product outcomes without regressing the others. These gates are intentionally small enough to run during normal development.

## Success Criteria

| Dimension | Gate |
|---|---|
| Authoritative richness | Selected RERA projects produce at least 25 current facts each and cover registration, status, completion, units, land area, litigation, project cost, and portal URL when the source exposes them. |
| Correctness | Every durable fact has a typed value, canonical entity ID, source type, source URL, skill ID, learned time, run ID, and input hash. Newer valid facts win; malformed or older facts cannot hide valid evidence. |
| Search proof | Proof-required constraints use authoritative facts. Search and property detail must expose the same current evidence and freshness timestamp. |
| Freshness | Dynamic support sources can refresh daily. RERA registry/detail snapshots refresh monthly or on demand. A cached refresh must not perform network calls. |
| Crawl efficiency | Detail crawling is scoped to selected entities, never the full registry. Three-project cold RERA collection should finish within 120 seconds; warm collection within 1 second. One failed optional source must not block independent assets. |
| Storage | Durable payloads are Parquet; JSON is limited to manifests and control-plane requests. Three-project RERA details should add less than 100 KB raw storage. A full local benchmark lake should remain below 25 MB until image/document payloads are introduced. |
| DAG runtime | A full local materialization from cached source inputs should finish within 20 seconds on the development machine. |
| Query latency | Local search p95 below 20 ms and property detail p95 below 10 ms over 50 sequential requests with a loaded serving bundle. |
| Operability | A promoted local or S3-shaped snapshot must load without recrawling. Failed assets and exact reasons remain visible in the run manifest. |

## Baseline: 2026-07-14

Three societies: Prestige Raintree Park, Prestige Park Grove, and Prestige Lavender Fields.

| Metric | Result |
|---|---:|
| Cached RERA listing rows | 9,761 |
| Detailed RERA facts | 38, 38, 37 per canonical society |
| Cold collector time | 76.19 s |
| Warm collector time | 0.46 s |
| Raw detail Parquet size | 14,983 bytes |
| Silver RERA current facts | 73,972 |
| Full DAG materialization time | 14.8 s for successful branches |
| Full lake size | 13 MB |
| Search latency | p50 7.79 ms, p95 7.96 ms |
| Property detail latency | p50 4.39 ms, p95 4.75 ms |
| API smoke suite | 51/51 passed |

Reddit returned HTTP 403 during this baseline. The failure was recorded, while RERA, Google, KG, and serving assets completed.
