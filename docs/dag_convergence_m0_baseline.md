# DAG Convergence M0 Baseline

Status: M0 audit harness added, warning-only baseline captured on 2026-07-28.

## Audit Commands

```bash
python3 scripts/audit_dag_convergence.py --max-findings 0
python3 scripts/audit_search_hardcoding.py
```

## Hardcoding Inventory Baseline

`scripts/audit_dag_convergence.py` derives 1,633 product terms from DAG/product config, scans Rust, TypeScript, Python pipeline code, config, docs, and tests, and classifies findings as:

- `product_semantic`: runtime code that likely needs migration or justification.
- `api_contract`: serialized Rust/frontend API shapes and frontend API types that may be structural but need compatibility review.
- `known_debt`: already identified transitional debt or convergence docs.
- `test_fixture`: tests, validation fixtures, and dev fixtures.
- `structural`: config files, loaders, protocol types, and review tooling.

Current warning-only counts:

| Classification | Findings |
| --- | ---: |
| `product_semantic` | 8,771 |
| `api_contract` | 264 |
| `known_debt` | 780 |
| `test_fixture` | 4,187 |
| `structural` | 6,956 |
| Total | 20,958 |

Current term-category counts:

| Term Category | Findings |
| --- | ---: |
| policy keys | 7,010 |
| source labels | 3,101 |
| search vocabulary | 3,098 |
| evidence section names | 2,297 |
| recommendation branch names | 1,888 |
| map layer names | 1,607 |
| search guardrails | 1,324 |
| resolution policies | 454 |
| policy constants | 149 |
| warning/red-flag terms | 24 |
| proof labels | 6 |

Top runtime hotspots from the baseline:

| File | Findings |
| --- | ---: |
| `backend/src/routes/properties.rs` | 612 |
| `backend/src/search/text.rs` | 382 |
| `pipeline/skills/fetch_rera.py` | 351 |
| `pipeline/collect_asset_sources.py` | 331 |
| `backend/src/assets/google.rs` | 252 |
| `backend/src/data_loader.rs` | 205 |
| `pipeline/skills/fetch_google_review_links.py` | 203 |
| `backend/src/routes/property_map.rs` | 192 |
| `app/config/product/evidence_sections.json` | 187 |
| `backend/src/assets/project_enrichment.rs` | 172 |

Legacy search-only audit baseline:

| Command | Config Terms | Findings |
| --- | ---: | ---: |
| `python3 scripts/audit_search_hardcoding.py` | 83 | 764 |

## Behavior Baselines

These are the current behavior owners to preserve while later milestones move semantics into DAG config and serving facts.

| Surface | Current Baseline Coverage |
| --- | --- |
| Detail evidence | `backend/src/routes/properties.rs` tests `source_panels_*`, `property_evidence_*`, and `property_evidence_sections_are_backend_shaped_for_dynamic_ui`; frontend `frontend/tests/property-detail-ui.test.ts`. |
| Map layers | `backend/src/routes/property_map.rs` tests `map_context_*`; frontend `frontend/tests/surface-scene.smoke.test.ts` and `frontend/tests/property-detail-ui.test.ts`. |
| Recommendations | `backend/src/recommendations/*` unit coverage and `frontend/tests/property-detail-ui.test.ts` test `recommendation scenes are stable and wrap after exhaustion`. |
| Area Tracker | `backend/src/routes/areas.rs` test `area_tracker_combines_inventory_and_search_demand`; route still derives demand and labels in runtime code, which is M6 debt. |
| Source labels | `backend/src/dag_config/resolution.rs` tests source tier/visibility; `backend/src/routes/properties.rs` source panel tests; `backend/tests/source_input_provider_contract.rs`. |
| Search | `backend/tests/search_quality_contract.rs`, `backend/tests/search_semantic_quality_contract.rs`, `backend/tests/search_efficiency_contract.rs`, `backend/tests/search_quality.rs`, `data/validation/query_bank/*.json`, and `tests/fuzzy_search_quality.py`. |

## M0 Review Notes

- The baseline intentionally includes config/docs/test findings so reviewers can see where vocabulary lives without treating all matches as runtime debt.
- Whole runtime files are not classified as `known_debt`; existing hotspots remain visible as `product_semantic` so new hardcoding in active files is not silently approved.
- The audit includes a second heuristic pass for policy-shaped constants such as caps, limits, thresholds, weights, radius values, fallbacks, recall knobs, and array literals.
- API type/model files are classified as `api_contract`, not `structural`, so serialized product semantics and closed literal unions remain review-visible.
- `app/config/product/evidence_sections.json` remains marked `known_debt` because M1 moves detail evidence sections into DAG-owned config.
- Pipeline fetchers and asset code have many product terms because source adapters still normalize source-specific records; later milestones should distinguish source parsing mechanics from buyer-facing product semantics.
- `backend/src/routes/properties.rs`, `backend/src/routes/property_map.rs`, `backend/src/search/text.rs`, and `frontend/src/lib/nearbyPlateProjection.ts` are the highest-priority runtime areas for M1-M5 review.
- Post-M0 M8 review found high-severity harness gaps around whole-file `known_debt` classification and missing numeric/scoring policy coverage; both are addressed in the current script baseline.

## Buddy Review Gate

Before starting M1, buddy review should inspect:

- Audit classification rules in `scripts/audit_dag_convergence.py`.
- The top runtime hotspot list for missed or misclassified files.
- Whether each behavior baseline above has enough test coverage to catch parity regressions.
- Whether any M1 implementation starts from new product hardcoding rather than moving existing semantics to DAG config.
