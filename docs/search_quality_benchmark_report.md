# Search Quality Benchmark

Generated: 2026-07-15T14:43:12.229815+00:00
Backend: `http://127.0.0.1:4002`

## Summary

| Surface | Gate | PASS | WARN | FAIL |
|---|---:|---:|---:|---:|
| 10-society product proof | WARN | 1 | 9 | 0 |
| Search quality cases | FAIL | 8 | 0 | 2 |

| Latency | p50 | p95 | max |
|---|---:|---:|---:|
| Search | 122.32 ms | 184.24 ms | 184.24 ms |
| Detail | 8.05 ms | 8.63 ms | 8.76 ms |

Latency gate: **WARN**

## Product Proof

| Society | Segment | Status | Source items | Sources | Key failures |
|---|---|---:|---:|---|---|
| Prestige Raintree Park | enriched_prestige | WARN | 19 | Google, Llm, Rera | - |
| Prestige Park Grove | enriched_prestige | WARN | 16 | Google, Llm, Rera | - |
| Prestige Lavender Fields | enriched_prestige | WARN | 23 | Google, Rera | - |
| Prestige Lakeside Habitat | rera_legacy | PASS | 30 | BuilderOfficial, Computed, Google, Rera | - |
| Brigade Woods | rera_legacy | WARN | 28 | Computed, Google, Rera | - |
| K Raheja Vivarea | central_premium | WARN | 21 | Google, Rera | - |
| Sobha Neopolis | east_premium | WARN | 20 | Google, Llm, Rera | - |
| The Prestige City | sarjapur_township | WARN | 21 | Google, Llm, Rera | - |
| Century Ethos | north_premium | WARN | 27 | Computed, Google, Rera | - |
| Vaswani Starlight | whitefield_resale | WARN | 22 | Google, Rera | - |

## Search Cases

| Case | Query | Status | Top result | Graph % | Key failures |
|---|---|---:|---|---:|---|
| green_large_whitefield | `3bhk with greenery in whitefield above 10 acres` | PASS | 3 BHK in Prestige Raintree Park | 100 | - |
| metro_green_whitefield | `green 3bhk whitefield near metro` | PASS | 3 BHK in Sumadhura Capitol Residences | 100 | - |
| premium_koramangala | `premium 3bhk koramangala` | FAIL | 3 BHK in Sobha Infina | 100 | detail_fact_key |
| ready_whitefield | `ready to move 3bhk whitefield` | PASS | 3 BHK in Assetz Marq 3.0 | 100 | - |
| reliable_builder_whitefield | `reliable builder 3bhk whitefield under 3cr` | PASS | 3 BHK in Alembic Urban Forest | 100 | - |
| avoid_waterlogging_whitefield | `3bhk whitefield avoid waterlogging and traffic` | PASS | 3 BHK in Sobha Windsor | 100 | - |
| sold_out_status_conflict | `prestige park grove 3bhk sold out` | FAIL | 3 BHK in Prestige Park Grove | 0 | detail_fact_key, detail_fact_key, detail_fact_key |
| family_sarjapur_ready | `family friendly ready to move 3bhk sarjapur road` | PASS | 3 BHK in The Prestige City | 100 | - |
| north_large_clubhouse | `large premium township with clubhouse and pool in north bangalore` | PASS | 4 BHK in Sumadhura Epitome 1 | 100 | - |
| reviews_metro_whitefield | `3bhk whitefield with good reviews and metro access` | PASS | 3 BHK in Sumadhura Capitol Residences | 100 | - |

## Failure Details

### premium_koramangala

- `detail_fact_key`: detail should expose source-backed fact rera_number

### sold_out_status_conflict

- `detail_fact_key`: detail should expose source-backed fact market_project_status
- `detail_fact_key`: detail should expose source-backed fact official_project_url
- `detail_fact_key`: detail should expose source-backed fact project_maps_url

## Product Reading

- PASS means the current local product can explain the result with sourced evidence.
- WARN means the user-facing path works but data is sparse or support evidence is weak.
- FAIL means the current engine is likely misleading, stale, or too legacy-driven for that query.
