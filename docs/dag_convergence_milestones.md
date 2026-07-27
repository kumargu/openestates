# Config/DAG Convergence Milestones

This tracks the milestone plan for moving buyer signals, intents, map layers, detail sections, recommendation lenses, warnings, and source labels behind `app/config/dag` plus serving facts.

## Review Gate

No milestone should proceed without:

- using `docs/templates/dag_convergence_milestone_checklist.md`;
- running the M0 audit command;
- preserving API compatibility or covering the API change with frontend/contract tests;
- recording latency, cache, computation placement, storage/serving size, and frontend request-count impact for performance-sensitive changes;
- comparing algorithm parity for search ranking, proof reasons, recommendation branches, and conflict-resolution behavior when those surfaces are touched;
- completing buddy review.

## Milestone Status

| Milestone | Status | Notes |
| --- | --- | --- |
| M0 Baseline Audit And Review Harness | In progress | Warning-only audit harness and baseline doc added. Buddy review still required before M1. |
| M1 Detail Evidence Sections Into DAG | Not started | Move `app/config/product/evidence_sections.json` into DAG-owned config/loading path. |
| M2 Source Display Policy | Not started | Centralize source visibility/labels/provenance in DAG config. |
| M3 API-Shaped Map And Detail Metadata | Not started | Make map/detail layers API-shaped and config-driven. |
| M4 Recommendation Recall Policy In Config | Not started | Move recall branches/limits/fallbacks/tie-breakers into `scoring_policy.json`. |
| M5 Search Intent Leftovers | Not started | Keep parser structural; move vocabulary, aliases, units, templates, conflict keys to config. |
| M6 Area Tracker DAG/Serving-Fact Backed | Not started | Replace runtime buyer-semantics derivations with serving facts and API metadata. |
| M7 Hardcoding Audit Guard | Not started | Expand warning audit toward stable false-positive baseline, then consider CI. |
| M8 Five-Agent Gap Review | Not started | Run after M0 and again after M7; triage high-severity findings. |

## M8 Review Agents

Run read-only high-effort reviews after M0 and M7:

- Rust/API Correctness Agent
- Ranking/Algorithm Agent
- Hardcoding Boundary Agent
- Latency And Caching Agent
- Storage And Serving Efficiency Agent

High-severity findings must be fixed or explicitly deferred with owner and reason.
