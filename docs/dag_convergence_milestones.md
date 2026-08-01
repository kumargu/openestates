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
| M4 Recommendation Recall Policy In Config | Partially started | Review `area_alias_bhk` and move recall branches/limits/fallbacks/tie-breakers into `scoring_policy.json` without reintroducing locality aliases. |
| M5 Search Intent Leftovers | Partially started | Locality/landmark parser aliases and fuzzy area aliasing removed. Remaining work: named-place resolver/proof loop, stale tests/docs, and non-parser config markers. |
| M6 Area Tracker DAG/Serving-Fact Backed | Not started | Replace runtime buyer-semantics derivations with serving facts and API metadata. |
| M7 Hardcoding Audit Guard | Not started | Expand warning audit toward stable false-positive baseline, then consider CI. |
| M8 Five-Agent Gap Review | Not started | Run after M0 and again after M7; triage high-severity findings. |

## 2026-08-01 Reset Notes

Recent search work intentionally moved out of strict parity mode to remove
hidden locality/landmark filters. The plan should treat this as architectural
cleanup, then recover quality through measured proof loops against the pinned
Bangalore bundle.

Immediate ordering:

1. Finish M5 reset documentation and benchmark harness around
   `bangalore-catalog-60-coherent-2026-08-01`.
2. Audit non-parser config markers: keep intentional Cult-only fitness
   curation, review transit-line/office-hub markers.
3. Run the first Bangalore proof loop and classify failures before writing more
   search code.
4. Resume M4 only after the search proof loop tells us whether recommendation
   recall is actually hurting product quality.

## M8 Review Agents

Run read-only high-effort reviews after M0 and M7:

- Rust/API Correctness Agent
- Ranking/Algorithm Agent
- Hardcoding Boundary Agent
- Latency And Caching Agent
- Storage And Serving Efficiency Agent

High-severity findings must be fixed or explicitly deferred with owner and reason.
