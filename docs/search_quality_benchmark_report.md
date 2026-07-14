# Search Quality Benchmark

Generated: 2026-07-14T15:25:40.335607+00:00
Backend: `http://127.0.0.1:4000`

## Summary

| Surface | Gate | PASS | WARN | FAIL |
|---|---:|---:|---:|---:|
| 10-society product proof | WARN | 3 | 7 | 0 |
| Search quality cases | FAIL | 5 | 1 | 4 |

| Latency | p50 | p95 | max |
|---|---:|---:|---:|
| Search | 23.36 ms | 87.09 ms | 87.09 ms |
| Detail | 7.86 ms | 8.98 ms | 9.39 ms |

## Product Proof

| Society | Segment | Status | Source items | Sources | Key failures |
|---|---|---:|---:|---|---|
| Prestige Raintree Park | enriched_prestige | PASS | 32 | BuilderOfficial, Computed, Google, Llm, Rera | - |
| Prestige Park Grove | enriched_prestige | PASS | 29 | BuilderOfficial, Computed, Google, Llm, Rera | - |
| Prestige Lavender Fields | enriched_prestige | PASS | 34 | BuilderOfficial, Computed, Google, Rera | - |
| Prestige Lakeside Habitat | rera_legacy | WARN | 21 | Google, Llm, Rera | - |
| Brigade Woods | rera_legacy | WARN | 19 | Google, Llm, Rera | - |
| K Raheja Vivarea | central_premium | WARN | 23 | Google, Rera | - |
| Sobha Neopolis | east_premium | WARN | 22 | Google, Llm, Rera | - |
| The Prestige City | sarjapur_township | WARN | 22 | Google, Llm, Rera | - |
| Century Ethos | north_premium | WARN | 22 | Google, Rera | - |
| Vaswani Starlight | whitefield_resale | WARN | 22 | Google, Rera | - |

## Search Cases

| Case | Query | Status | Top result | Graph % | Key failures |
|---|---|---:|---|---:|---|
| green_large_whitefield | `3bhk with greenery in whitefield above 10 acres` | PASS | 3 BHK in Prestige Raintree Park | 100 | - |
| metro_green_whitefield | `green 3bhk whitefield near metro` | FAIL | 3 BHK in Prestige Raintree Park | 100 | required_reason_any |
| premium_koramangala | `premium 3bhk koramangala` | FAIL | 3 BHK in Radiance Platinum | 0 | top_graph_driven_pct, legacy_reason_budget, detail_fact_key, detail_fact_key |
| ready_whitefield | `ready to move 3bhk whitefield` | PASS | 3 BHK in Assetz Marq 3.0 | 100 | - |
| reliable_builder_whitefield | `reliable builder 3bhk whitefield under 3cr` | PASS | 3 BHK in Sarang By Sumadhura Phase 2 | 100 | - |
| avoid_waterlogging_whitefield | `3bhk whitefield avoid waterlogging and traffic` | WARN | 3 BHK in Sobha Windsor | 0 | - |
| sold_out_status_conflict | `prestige park grove 3bhk sold out` | PASS | 3 BHK in Prestige Park Grove | 0 | - |
| family_sarjapur_ready | `family friendly ready to move 3bhk sarjapur road` | FAIL | 3 BHK in Arvind Sarjapur Road | 0 | top_graph_driven_pct |
| north_large_clubhouse | `large premium township with clubhouse and pool in north bangalore` | PASS | 3 BHK in L&T Elara Celestia | 50 | - |
| reviews_metro_whitefield | `3bhk whitefield with good reviews and metro access` | FAIL | 3 BHK in Prestige Raintree Park | 100 | required_reason_any |

## Failure Details

### metro_green_whitefield

- `required_reason_any`: top result should include at least one expected reason

### premium_koramangala

- `top_graph_driven_pct`: expected top graph-driven pct >= 50, got 0
- `legacy_reason_budget`: expected <= 1 legacy reasons in top 3, got 3
- `detail_fact_key`: detail should expose source-backed fact market_project_status
- `detail_fact_key`: detail should expose source-backed fact google_reviews_url

### family_sarjapur_ready

- `top_graph_driven_pct`: expected top graph-driven pct >= 40, got 0

### reviews_metro_whitefield

- `required_reason_any`: top result should include at least one expected reason

## Product Reading

- PASS means the current local product can explain the result with sourced evidence.
- WARN means the user-facing path works but data is sparse or support evidence is weak.
- FAIL means the current engine is likely misleading, stale, or too legacy-driven for that query.
