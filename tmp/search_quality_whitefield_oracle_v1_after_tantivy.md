# Buyer-Language Search Quality Benchmark

Generated: 2026-07-23T18:16:50Z
Base URL: `http://127.0.0.1:4012`

## Summary

- Cases: 34 (25 pass, 9 fail)
- Scoreable quality checks: 0/0 (0.0%) across data-backed cases
- Overall checks including data-gap sentinels: 205/217 (94.5%)

### By Layer

| Layer | Passed | Failed |
|---|---:|---:|
| intent | 75 | 6 |
| latency | 34 | 0 |
| ranking | 14 | 4 |
| recall | 15 | 1 |
| result_count | 33 | 1 |
| safety | 34 | 0 |

### Runtime

- Serving bundle: `2026-07-23T14:32:19Z`
- Semantic embedder: `openestates-domain-hash-v1`
- Semantic index model: `openestates-domain-hash-v1`
- Semantic index documents: 53
- Semantic index empty: False

### By Mode

| Mode | Passed | Failed |
|---|---:|---:|
| whitefield_oracle | 205 | 12 |

### Latency

- Total p50: 48.43ms
- Total p95: 79.48ms
- Endpoint p50: 51.04ms
- Endpoint p95: 84.48ms

| Layer | p50 ms | p95 ms |
|---|---:|---:|
| constraint_relaxation | 0.89 | 0.89 |
| entity_resolution | 0.09 | 0.21 |
| geo_recall | 0.0 | 0.25 |
| geo_resolve | 0.45 | 0.6 |
| intent_parse | 28.96 | 36.41 |
| ranking | 9.89 | 48.68 |
| semantic_recall | 0.0 | 0.0 |
| structured_recall | 0.2 | 1.39 |
| tantivy_recall | 5.1 | 21.18 |
| total | 48.43 | 79.48 |

## Failed Cases

### WF-O-001 (whitefield_oracle, listing_price)

godrej splendour 3bhk whitefield under 2.1 cr

- `intent.positive_preferences`: missing positive preferences ['listing_evidence']; got []
- Top result: 3 BHK in Godrej Splendour (score=0.93, semantic=None)
- Total latency: 38.6ms

### WF-O-007 (whitefield_oracle, specific_project)

godrej united 5bhk whitefield

- `result_count.min_results`: expected at least 1 results, got 0
- `ranking.top_title_any`: expected one of ['Godrej United'] in top 3, got []
- Total latency: 32.18ms

### WF-O-017 (whitefield_oracle, near_school)

apartment near gopalan national school hoodi

- `recall.result_title_any`: expected one of ['Godrej Air', 'Godrej United', 'Brigade Lakefront - Crimson'] in top 10, got ['3 BHK in Vaswani Starlight', '4 BHK in Vaswani Starlight', '3 BHK in Sumadhura Capitol Residences', '4 BHK in Sumadhura Capitol Residences', '1 BHK in PRESTIGE WATERFORD', '3 BHK in PRESTIGE WATERFORD', '4 BHK in PRESTIGE WATERFORD', '2 BHK in Candeur Landmark', '3 BHK in Candeur Landmark', '1 BHK in Godrej Splendour']
- Top result: 3 BHK in Vaswani Starlight (score=0.17, semantic=None)
- Total latency: 55.69ms

### WF-O-018 (whitefield_oracle, near_school)

whitefield home near deens academy

- `intent.positive_preferences`: missing positive preferences ['family_friendly']; got []
- `ranking.top_title_any`: expected one of ['PRESTIGE WATERFORD'] in top 3, got ['3 BHK in Vaswani Starlight', '4 BHK in Vaswani Starlight', '3 BHK in Sumadhura Capitol Residences']
- Top result: 3 BHK in Vaswani Starlight (score=0.13, semantic=None)
- Total latency: 35.19ms

### WF-O-023 (whitefield_oracle, rera_builder)

rera approved godrej properties whitefield 3bhk

- `intent.positive_preferences`: missing positive preferences ['legal_safety', 'builder_trust']; got []
- Top result: 3 BHK in Godrej Splendour (score=0.67, semantic=None)
- Total latency: 53.54ms

### WF-O-025 (whitefield_oracle, review_quality)

most reviewed assetz marq whitefield

- `intent.positive_preferences`: missing positive preferences ['review_quality']; got []
- Top result: 2 BHK in Assetz Marq (score=0.93, semantic=None)
- Total latency: 79.48ms

### WF-O-028 (whitefield_oracle, rera_scale)

large rera project over 1000 units godrej splendour

- `intent.positive_preferences`: missing positive preferences ['legal_safety']; got []
- Top result: 1 BHK in Godrej Splendour (score=1.0, semantic=None)
- Total latency: 76.61ms

### WF-O-029 (whitefield_oracle, rera_scale)

whitefield project with 13 towers 2bhk

- `ranking.top_title_any`: expected one of ['SBR ONE RESIDENCE'] in top 3, got ['2 BHK in Assetz Marq Building 3 - Tower 6', '2 BHK in Eden at Brigade Cornerstone Utopia', '2 BHK in Paradise At Brigade Cornerstone Utopia']
- Top result: 2 BHK in Assetz Marq Building 3 - Tower 6 (score=0.38, semantic=None)
- Total latency: 48.65ms

### WF-O-033 (whitefield_oracle, specific_project)

brigade lakefront 4bhk near seetharam palya

- `intent.positive_preferences`: missing positive preferences ['commute']; got []
- `ranking.top_title_any`: expected one of ['Brigade Lakefront - Crimson'] in top 3, got ['4 BHK in Assetz Marq', '4 BHK in Godrej United', '4 BHK in Godrej Woodscapes']
- Top result: 4 BHK in Assetz Marq (score=0.07, semantic=None)
- Total latency: 37.96ms

## All Cases

| Case | Mode | Category | Status | Results | Failed Checks |
|---|---|---|---|---:|---|
| WF-O-001 | whitefield_oracle | listing_price | FAIL | 3 | intent.positive_preferences |
| WF-O-002 | whitefield_oracle | listing_price | PASS | 2 |  |
| WF-O-003 | whitefield_oracle | budget_family | PASS | 1 |  |
| WF-O-004 | whitefield_oracle | budget_reviews | PASS | 3 |  |
| WF-O-005 | whitefield_oracle | specific_project | PASS | 5 |  |
| WF-O-006 | whitefield_oracle | specific_project | PASS | 1 |  |
| WF-O-007 | whitefield_oracle | specific_project | FAIL | 0 | result_count.min_results, ranking.top_title_any |
| WF-O-008 | whitefield_oracle | specific_project | PASS | 16 |  |
| WF-O-009 | whitefield_oracle | specific_project | PASS | 5 |  |
| WF-O-010 | whitefield_oracle | specific_project | PASS | 13 |  |
| WF-O-011 | whitefield_oracle | near_metro | PASS | 38 |  |
| WF-O-012 | whitefield_oracle | near_metro | PASS | 14 |  |
| WF-O-013 | whitefield_oracle | near_metro | PASS | 5 |  |
| WF-O-014 | whitefield_oracle | near_metro_tech | PASS | 38 |  |
| WF-O-015 | whitefield_oracle | near_metro | PASS | 11 |  |
| WF-O-016 | whitefield_oracle | near_school | PASS | 38 |  |
| WF-O-017 | whitefield_oracle | near_school | FAIL | 24 | recall.result_title_any |
| WF-O-018 | whitefield_oracle | near_school | FAIL | 38 | intent.positive_preferences, ranking.top_title_any |
| WF-O-019 | whitefield_oracle | near_hospital | PASS | 13 |  |
| WF-O-020 | whitefield_oracle | near_hospital_metro | PASS | 38 |  |
| WF-O-021 | whitefield_oracle | near_tech_park | PASS | 13 |  |
| WF-O-022 | whitefield_oracle | near_tech_park | PASS | 16 |  |
| WF-O-023 | whitefield_oracle | rera_builder | FAIL | 13 | intent.positive_preferences |
| WF-O-024 | whitefield_oracle | review_quality | PASS | 13 |  |
| WF-O-025 | whitefield_oracle | review_quality | FAIL | 38 | intent.positive_preferences |
| WF-O-026 | whitefield_oracle | home_state | PASS | 11 |  |
| WF-O-027 | whitefield_oracle | home_state | PASS | 13 |  |
| WF-O-028 | whitefield_oracle | rera_scale | FAIL | 16 | intent.positive_preferences |
| WF-O-029 | whitefield_oracle | rera_scale | FAIL | 11 | ranking.top_title_any |
| WF-O-030 | whitefield_oracle | access_belt_budget | PASS | 2 |  |
| WF-O-031 | whitefield_oracle | access_belt_budget | PASS | 6 |  |
| WF-O-032 | whitefield_oracle | access_belt_project | PASS | 11 |  |
| WF-O-033 | whitefield_oracle | specific_project | FAIL | 6 | intent.positive_preferences, ranking.top_title_any |
| WF-O-034 | whitefield_oracle | access_belt_project | PASS | 16 |  |
