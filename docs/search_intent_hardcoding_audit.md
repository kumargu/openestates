# Search Intent Hardcoding Audit

Date: 2026-07-28

Scope: search parsing, geo resolution, search ranking proof handoff, map/detail nearby evidence projection, and product vocabulary in Rust/TypeScript.

## Summary

The repo already has substantial config-backed search semantics in `app/config/dag/fact_registry.json`, `search_intent.json`, `scoring_policy.json`, and `ui_surfaces.json`. This pass moved BHK, budget, relation, and geo distance query vocabulary into `search_intent.json`, introduced `backend/src/search/parser.rs` as the parser boundary, and uses `winnow` for deterministic numeric token parsing while leaving tokenization, phrase matching, and slot orchestration as structural Rust code.

## Findings

| Location | Classification | Hotspot | Recommended owner |
| --- | --- | --- | --- |
| `backend/src/search/intent.rs::parse_intent` | structural | Orchestrates area, slot, preference, and guardrail extraction. Allowed in code as parser plumbing. | Rust search |
| `backend/src/search/parser.rs` | structural | Normalizes tokens and parses typed slot shapes for BHK, money, distance, and relation intent. Vocabulary comes from config. | Rust search |
| `app/config/dag/search_intent.json::parser` | product_semantic | Owns BHK unit aliases, number words, money units/operators, distance units/operators, relation aliases. Required and validated at load time. | Product/search config |
| `backend/src/search/schema.rs::detect_hard_constraints` | mixed | Generic numeric constraint detection loops over `fact_registry.json`, but still encodes min/max context mechanics in Rust. Structural operators are allowed; buyer-facing dimensions belong in `fact_registry.json`. | Rust search + fact registry |
| `backend/src/search/geo.rs::GeoSearchIndex::query` | structural | Resolves named places and applies configured distance limit. Place-family aliases and generic stopwords are config-backed. | Rust search |
| `app/config/dag/scoring_policy.json::search_ranking` | product_semantic | Owns named-place weights, stopwords, distance scoring thresholds, and geo fact keys. | Search ranking config |
| `frontend/src/lib/search.ts` | structural/rendering | Formats backend match reasons and suppresses generic filter text. It still contains generic display heuristics, not search intent parsing. | Frontend rendering |
| `frontend/src/lib/nearbyPlateProjection.ts` | mixed | Plate radius/scale constants and proof focus are rendering mechanics. Layer-specific scale defaults for metro/tech/red_flags should eventually move to `ui_surfaces.json`. | UI surfaces config + frontend |
| `frontend/tests/property-detail-ui.test.ts` | test_fixture | Manipal/Aster/tech park fixtures are allowed in tests. | Test fixtures |
| `frontend/tests/surface-scene.smoke.test.ts` | test_fixture | Nearby layer examples are allowed in tests. | Test fixtures |

## Review Command

Run the broad DAG convergence audit:

```bash
python3 scripts/audit_dag_convergence.py --max-findings 0
```

The compatibility command below delegates to the same broad audit:

```bash
python3 scripts/audit_search_hardcoding.py --max-findings 0
```

The command is warning-only. It derives scan terms from DAG config and scans Rust, TypeScript, Python, config, docs, and tests for buyer/product vocabulary outside approved config, test, and rendering locations. It should stay non-blocking until false positives are reviewed.

Historical search-only baseline from this pass, before M7 broadened the compatibility command:

```text
Config-derived terms: 78
Findings: 759
```

## Implemented In This Pass

- Added required `parser` vocabulary in `app/config/dag/search_intent.json`.
- Added backend config structs and validation for parser vocabulary.
- Added `winnow` and a typed slot parser for BHK, budget, distance, and relation intent.
- Moved relation alias distance requirements, such as distance-only `within`, into parser config.
- Routed public `SearchIntent` BHK/budget fields through the parser.
- Routed geo distance limits through the same parser and gated named-place geo resolution on relation intent.
- Added regression coverage for whitespace, optional hyphen, number words, distance units, budget formats, Manipal/tech-park queries, and non-proximity place mentions.
- Added a warning-only hardcoding audit command that scans parser production code and excludes inline Rust test modules as fixtures.
- Verified the backend library suite and frontend proof-focus/map projection tests after the parser and geo handoff changes.

## Remaining Follow-Up

- `backend/src/search/intent.rs::apply_bhk_fact_key_derivations` still hardcodes listing fact-key template behavior and should move to `fact_registry.json` or `search_intent.json`.
- `backend/src/search/schema.rs` still contains some numeric constraint direction/context mechanics around age constraints; dimensions are config-backed, but context vocabulary can move further into config.
- `frontend/src/lib/search.ts` still contains display-suppression vocabulary for result reasons; this should move to response metadata or UI config.
- The audit command still reports many known findings and should gain a reviewed baseline before becoming a CI gate.
