# Recommendation engine proof loop

## Decision

OpenEstates should use a deterministic, evidence-backed cold-start recommender until it owns durable interaction history. The current engine recommends alternatives to a property by combining configured eligibility, local recall, DAG facts, and branch-specific scoring. It does not infer a user profile from browser-local state and does not call an LLM or network service on the request path.

No new recommendation crate is justified yet. The repository already has the useful primitives:

- `rstar` for spatial candidate generation
- the serving graph for shared, source-backed relationships
- Tantivy for optional lexical recall
- the scoring policy and DAG facts for ranking and explanations

Session-based libraries such as Serenade and sequential recommenders such as `sbr-rs` become relevant only after OpenEstates records trustworthy impressions, clicks, shortlist actions, compares, and outcomes. A small graph random-walk implementation such as `pixie-rust` would add mechanics without solving the current evidence and interaction-data gaps.

## Contract

The controlled scenario bank is `data/validation/recommendation_scenario_bank.json`. Its ten current scenarios cover cold start, branch specialists, missing evidence, explicit tradeoffs, society diversity, graph recall, spatial recall, deterministic ties, empty inventory, and configured property-type compatibility.

The live-bundle contract is `backend/tests/recommendation_live_audit.rs`. It runs every promoted property through the production route and verifies:

- same BHK
- no self recommendation
- no anchor-society repetition
- at most one result per target society
- unique targets
- at least one candidate-generating recall receipt per result
- bounded branch count and p95 latency
- at least 90% society-coordinate coverage

Exact live result IDs are deliberately not frozen because the promoted DAG bundle should improve. Trust invariants are frozen; coverage is reported so honest abstention remains possible.

## Retained engine design

Eligibility is controlled by `app/config/dag/scoring_policy.json`:

- same BHK and listing type
- configured property-type compatibility groups rather than raw source-string equality
- exclude the anchor society
- one recommendation per society

Recall channels are also configured. `same_area`, `shared_graph_neighbor`, and `spatial_nearby` can generate candidates. Price band and builder identity only boost candidates that already have stronger recall evidence. Lexical recall is disabled because it produced no useful live candidates. The engine preserves configured branch priority, uses deterministic property-ID tie breaks, and fills unused branches only from the already-qualified candidate pool.

The property page preserves that backend order and renders only qualified branch results. It no longer re-ranks branches by Google review count or fills empty slots from the unqualified global property list. The unused `AlternativePaths` component, which duplicated the old client-side ranking behavior, was removed.

## Measured experiments

All measurements use 86 listable properties from promoted bundle `clean-serving-v7-2026-08-16`.

| Experiment | Result | Decision |
|---|---:|---|
| Original engine | 246 results; 69.1% same-BHK; 21 anchors repeated a society; 51 anchors had cross-BHK results; 57.7% fallback | Rejected |
| Strict evidence recall, 8 km spatial radius | 204 results; 100% same-BHK; no repeated society; 10 empty and 24 thin anchors | Retained as safe baseline |
| Configured residential property-type aliases | 221 results; 2.57/anchor; 100% same-BHK; no repeated society; 7 empty and 18 thin anchors; 51.1 ms p95 | Retained |
| Global 12 km spatial radius | 228 results; 3 empty and 15 thin anchors; 64.2 ms p95; 26 pairs beyond 8 km, up to 11.1 km | Rejected |

The 12 km experiment improved coverage but admitted too many cross-area alternatives for anchors that already had good nearby choices. The engine keeps the 8 km boundary and abstains when proof is weak.

## Remaining gaps

The gaps were checked directly in the promoted Parquet, not inferred from API output.

- `data_gap`: `SHRIRAM HEBBAL ONE` and `Sobha 25 Richmond` have no `geo.latitude` or `geo.longitude` facts. They account for two empty anchors.
- `inventory_gap`: the live bundle has only five 1-BHK and four 5-BHK properties. Five empty anchors are isolated members of these scarce cohorts outside the 8 km boundary.
- `data_gap`: the live graph has no `near_transit` edges. Its 37 `served_by_road` edges each point to a unique project-specific road node, so shared-access graph recall correctly produces zero live hits. The controlled graph scenario proves the runtime behavior independently.

These should be fixed in coordinate enrichment, shared-place/road canonicalization, and inventory coverage—not by weakening runtime eligibility.

## Personalization boundary

Search selections and shortlist actions are not yet valid recommender inputs. The backend shortlist route is a stub and the frontend shortlist lives in `localStorage`; neither provides durable ordered events, impressions, negative feedback, identity, or consent.

Before adding collaborative or session-based ranking, materialize an append-only event contract with at least:

- anonymous/session and optional user identity
- search and recommendation impression IDs
- ordered candidate IDs and policy/bundle versions
- click, detail-open, shortlist add/remove, compare, contact, and explicit dismiss events
- event time and source surface

Once event quality and volume are measurable, evaluate session kNN first. Keep learned scores as an additive, explainable boost over evidence-qualified candidates rather than allowing behavior data to bypass DAG-backed eligibility and proof.

## Reproducing the proof loop

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --test recommendation_scenarios_contract -- --nocapture
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --test recommendation_live_audit -- --nocapture
```

For each future change: record the baseline, classify the miss, change one generic mechanism or config row, rerun both contracts, and retain the change only when it improves a stated metric or removes a verified architecture defect without weakening trust invariants.
