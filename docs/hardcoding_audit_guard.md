# DAG Convergence Hardcoding Audit Guard

Status: warning-only review guard. Do not promote to CI blocking until buddy review confirms the baseline is stable and false positives are low.

## Review Command

```bash
python3 scripts/audit_dag_convergence.py --max-findings 0
```

Compatibility command:

```bash
python3 scripts/audit_search_hardcoding.py --max-findings 0
```

Both commands run the broad DAG convergence audit. The search command remains only so older review notes and local habits keep working.

## Covered Categories

- search vocabulary
- source labels
- map layer names
- recommendation branch names
- evidence section names
- Area Tracker product terms
- warning/red-flag terms

## Approved Locations

- `app/config/dag`
- tests and fixtures
- rendering primitives where labels, icons, and accessibility text are structural
- docs

Findings in approved locations are still reported so reviewers can see vocabulary ownership, but they are not runtime hardcoding regressions by themselves.

## Expected False Positives

- API contract types in Rust and TypeScript that mirror backend response fields.
- Source adapter and pipeline code that parses third-party source schemas before converting them into serving facts.
- Structural rendering code that displays API-provided labels or supports generic icons/states.
- Tests, fixtures, validation query banks, and docs that intentionally mention buyer-facing terms.
- Policy-shaped constants in runtime code that are generic mechanics but match names such as `limit`, `score`, or `fallback`.

## Review Notes

Use the summary by classification and top runtime hotspots first. A stable M7 baseline is acceptable with warnings as long as new product semantics are in DAG config or serving facts, and any runtime findings are structural or documented debt.
