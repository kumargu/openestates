# Search, data, and UI contract audit — 21 September 2026

**Verdict: the layers are partly aligned, but they do not yet share one complete evidence contract.** The signed journey and named-place receipt path work in the existing contracts. Real catalog probes still expose wrong measurement semantics, unopenable receipts, displayable-but-unsearchable inventory, and map facts outside the DAG.

This is an audit, not an implementation change. No production behavior, source records, catalog pointer, or frozen expectations were changed.

## Scope and baseline

- Fetched `origin/main`, created the isolated `audit/search-data-ui-20260921` worktree, and ran `git pull --ff-only origin main`. Audited commit: `2f0f4e27c4339531191467da5a137e8538ae4616`.
- The original `/Users/gulshan.kumar/openestates` checkout had 21 branch commits outside main, four modified tracked files, and one untracked seed file. Those changes were preserved and excluded from this main-code audit.
- Chain audit: `18603071` introduced contextual journeys; `f41e7475` tightened exact receipt handoff; `afc4432b` updated interaction checks; `b3c10f8d` removed obsolete documentation. The receipt fixes cover the tested named-place path, while category-derived receipts and older consumers remain inconsistent.
- Data was the **existing local promoted catalog**, not a verified production deployment or a rebuild from this main commit. Pointer: `/Users/gulshan.kumar/openestates/data/lake/manifests/catalog/dev.json`, revision 13. Bundle: `catalog-71-38d60ec3-cc6e-40d2-bc07-cf30a45b7298`, format 12.
- Read promoted entities, facts, and search metadata directly using the repository's Parquet readers: 2,259 entities, 25,751 facts, 24,409 metadata rows. Loaded 71 societies and 160 property configurations. Inspected all 160 detail responses and one normal scene for each of the 71 societies.
- Ran 16 exploratory queries through the engine and current journey handler; resolved all 422 emitted proof references. These are probes, not a replacement frozen benchmark or a statistical quality score. Repeated references across queries are counted separately.
- Local probe search events were diverted to an in-memory channel to avoid polluting the lake's search-demand data. Ordinary rebuildable hydration caches were allowed.

Baseline logs and reproductions: [/tmp/openestates-audit-20260921](/tmp/openestates-audit-20260921). Structured findings: [summary.json](/tmp/openestates-audit-20260921/summary.json). Raw Parquet-derived records/API outputs: [live-records.json](/tmp/openestates-audit-20260921/live-records.json), [live-details.json](/tmp/openestates-audit-20260921/live-details.json), [live-scenes.json](/tmp/openestates-audit-20260921/live-scenes.json).

## Verified findings

### 1. P1 — Listed area is relabeled as carpet area and used to satisfy carpet constraints

Classification: `architecture_gap` / `intent_gap`.

[data_loader.rs:291](../backend/src/data_loader.rs#L291) copies the listing's generic representative square footage into `carpet_area_sqft`; [line 338](../backend/src/data_loader.rs#L338) copies the same number into `super_builtup_sqft`. [parse_listing_pricing:1484](../backend/src/data_loader.rs#L1484) reads `area_sqft` and drops `area_type`. The registry explicitly calls this runtime field “Carpet area” ([fact_registry.json:3022](../app/config/dag/fact_registry.json#L3022)), and [text.rs:1473](../backend/src/search/text.rs#L1473) uses it for numeric eligibility.

The source distinction is present in Parquet. Of the 144 inventory-bound configurations, 77 say `mixed listed area`, 32 `super built-up`, 15 `built-up`, and only 20 `carpet`. The first three categories are all projected as carpet area.

Reproduction: `3BHK carpet area above 1500 sqft` returns, among others:

| Configuration | Source area type | Value accepted as carpet |
|---|---|---:|
| Mantri Tranquil 3BHK | mixed listed area | 1,745 sqft |
| Purva Westend 3BHK | mixed listed area | 1,783 sqft |
| Sumadhura Capitol Residences 3BHK | built-up | 1,635 sqft |
| SNN Clermont 3BHK | super built-up | 2,526 sqft |

Buyer impact: search can claim a minimum usable/carpet area without evidence for that measurement. Detail and comparison consume the same mislabeled fields, so agreement between them does not establish correctness.

Smallest coherent fix: retain the typed area basis through materialization, inventory, and API projection; bind carpet predicates only to carpet evidence. Keep generic advertised area distinct. Add one materializer-to-search-to-detail scenario with different carpet and sale areas, including mixed/unknown area.

### 2. P1 — Category proximity reasons issue proof tokens that cannot resolve immediately

Classification: `proof_gap` / `architecture_gap`.

[geo.rs:1035](../backend/src/search/geo.rs#L1035) creates an ephemeral `serving_distance_fact` derivation with no target entity when satisfying a category such as “near schools.” [journey.rs:868](../backend/src/search/journey.rs#L868) accepts that in-memory derivation for token issuance. The token carries its reference but not the derivation. On resolution, [proof.rs:289](../backend/src/search/proof.rs#L289) looks in the bundle and then tries to recompute; [line 348](../backend/src/search/proof.rs#L348) requires a target entity, which this path did not preserve.

Reproduction on the same unchanged snapshot:

- `quiet 3BHK near schools under 2.5Cr`: 17 `near schools` card reasons fail with `proof_evidence_missing`.
- `3BHK near metro`: 10 `near metro` card reasons fail with the same error.
- All 27 failed reasons have `showOnCard: true`. Example: `discovered-mantri-tranquil-3bhk`, “near schools.”

The source fact exists with an observation; this is not simply absent source coverage. The missing link is the reproducible derivation identity. Current counts: 395/422 references resolve; 27 fail. The separately pinned named-place contract passes, so that result must not be generalized to category proximity.

Smallest coherent fix: preserve a durable target and derivation, or bind directly to an eligible typed source fact with a resolvable derivation contract. Token issuance and resolution must accept the same evidence forms. Extend the existing API contract to open every emitted category reason and prove the focused fact remains visible.

### 3. P1 — Normal BHK/budget searches produce a false “receipt no longer available” state

Classification: `proof_gap`.

[primaryProofFocus](../frontend/src/lib/proof-focus.ts#L42) selects the first reason even when all reasons have `showOnCard: false`. Inventory proofs use keys such as `listing_3bhk`, which have no destination in [proof_destination_for_fact_key](../backend/src/search/proof.rs#L438). They resolve successfully but have no focus target. [PropertyPage.tsx:201](../frontend/src/pages/PropertyPage.tsx#L201) interprets that as an unavailable receipt.

Reproduction with the actual `3BHK` API result: the first SNN Raj Greenbay card receives a proof token; resolution succeeds; `resolvedProofFocus()` returns `undefined`; the page sets `proofUnavailable` and renders “This search receipt is no longer available.” The receipt is neither missing nor expired. Across the probe, 370 successfully resolved references had no destination, all inventory references.

Smallest coherent fix: distinguish valid evidence without a focus destination from missing evidence. Either provide an inventory proof destination or omit the focus navigation token when none is supported. Keep this distinct from `showOnCard`, which controls copy visibility. Add a plain BHK/budget search-to-detail journey using the real contract shape.

### 4. P1 — Six societies are displayable but lose all their configurations under BHK-constrained search

Classification: `data_gap` / promotion `architecture_gap`.

There are 160 displayable configurations but 144 `InventoryOption` bindings. The missing 16 belong to Arvind Bel Air, Brigade Komarla Heights, Godrej Eternity, Mahaveer Ranches, Prestige Song of the South, and SNN Raj Etternia.

Direct Parquet inspection shows their expected listing values and source URLs, with confidence around 0.7, but without `SourceObservation` identities. They are not absent entities or merely missing display metadata. [InventoryOption::from_serving_observation](../backend/src/search/evaluation.rs#L25) correctly refuses to prove a BHK/budget predicate without the observation. The display projection accepts the same listing rows without this binding; [serving_eligibility.json](../app/config/dag/serving_eligibility.json) does not gate promotion on inventory proof coverage.

Reproduction: `Arvind Bel Air` returns two configurations, while `3BHK in Arvind Bel Air` returns zero. `3BHK in Brigade Komarla Heights` also returns zero despite a displayed 3BHK configuration.

Smallest coherent fix: rebuild the affected source/materialized records with valid original lineage and require parity between advertised inventory and searchable inventory at promotion. Preserve search's fail-closed behavior; do not invent observations or treat a source URL alone as a receipt identity. This finding applies to the observed local bundle; the isolated main source was not used to regenerate it.

### 5. P1 — Buyer map scenes manufacture receipt records from non-DAG seed geometry

Classification: `architecture_gap` / `proof_gap`.

[map_overlays.rs:102](../backend/src/routes/map_overlays.rs#L102) loads local `data/seed/map/*.geojson`. After building a DAG scene, [routes/surfaces.rs:222](../backend/src/routes/surfaces.rs#L222) clips those seed polygons on the request path and merges them into empty configured layers. [surfaces.rs:606](../backend/src/surfaces.rs#L606) creates relations and receipts with `confidence: 1.0`, `learned_at: Utc::now()`, no source URL, and no durable feature entity ID.

Observed: 51 of 71 normal society scenes contain these fallback features, totaling 243 feature occurrences. Example: Amrutha Platinum Towers' scene includes Sheelavathana Kere and Nallurahalli Lake as `:context-` features with confidence 1.0 and `entityId: null`.

Buyer impact: detail can present a nearby lake with a receipt-like claim that search cannot trace through the same serving evidence. Changing a local seed file can change the scene without changing the bundle version. The defect is the provenance/contract boundary; this audit does not assert that the underlying OSM polygons are geographically false.

Smallest coherent fix: promote the geometry and society relationships through DAG assets and typed evidence. If retained solely as a basemap visual, prevent it from acquiring buyer-fact receipts, proof confidence, or search-equivalent claims.

### 6. P2 — Numeric eligibility discards its evidence before journey proof projection

Classification: `proof_gap` / `architecture_gap`.

[text.rs:3046](../backend/src/search/text.rs#L3046) calls `match_hard_constraints()` but turns success into `BooleanEvaluation::satisfied(Vec::new())`, dropping the evidence. [journey.rs:837](../backend/src/search/journey.rs#L837) binds verified reasons for BHK, budget, and spatial predicates only; numeric `Evidence` predicates fall through. Runtime area checks additionally create an `EvidenceMatch` without an evidence identity ([text.rs:1473](../backend/src/search/text.rs#L1473)).

Reproduction: `3BHK above 10 acres` returns 10 results, but the first result's reasons contain only `3 BHK`. `3BHK carpet area above 1500 sqft` also exposes only BHK proof on the first result. The numeric condition changes eligibility without surviving as the same typed receipt used by the journey/detail handoff.

Smallest coherent fix: carry the numeric evaluator's exact `VerifiedMatch` and evidence bindings through Boolean evaluation, predicate binding, reasons, and proof resolution. Do not reconstruct it from display text. Add numeric constraints to the existing receipt contract, including a failed bound and a missing measurement.

### 7. P2 — The Python benchmark still consumes the retired search/proof API

Classification: `architecture_gap` in validation.

[benchmark_search_quality.py:1298](../pipeline/benchmark_search_quality.py#L1298) reads top-level `resultSets` or legacy `results`. The current API nests results under `active.results`. `call_search()` returns the envelope unchanged. [collect_proof_handoffs:298](../pipeline/benchmark_search_quality.py#L298) also expects `proofFocuses` and sends client-authored `focus` JSON, whereas the API now emits `reasons[].proofToken` and resolves signed proofs.

Reproduction: passing the captured successful `3BHK` response, containing 32 result cards, to `flattened_results()` returns `[]`. Positive checks can report false failures, while absence checks can become vacuous. Unwrapping the result list alone would still leave the obsolete proof transport.

Smallest coherent fix: migrate the runner to the current journey and proof contracts, remove the superseded adapter, and run an existing compatible query-bank suite end to end. Add a boundary check that refuses unsupported response versions/shapes instead of interpreting them as no matches. Do not change frozen expectations to accommodate this failure.

### 8. P2 — Area Tracker is disconnected from both active UI and search-demand events

Classification: `architecture_gap`.

On audited main, `AreaTrackerSection` has no importing/rendering component. The backend endpoint remains registered, but [area_tracker](../backend/src/routes/areas.rs#L83) reads `state.knowledge.search_log`. [data_loader.rs:51](../backend/src/data_loader.rs#L51) starts that graph empty; [spawn_search_log_worker](../backend/src/state.rs#L706) writes events to the lake and never updates or reloads that graph log.

Area profiles are reconstructed from inventory with empty context/trend fields, and `last_updated` is set to runtime construction time ([data_loader.rs:691](../backend/src/data_loader.rs#L691)). This is not crawl freshness. The local endpoint returns 30 markets/160 configurations and zero demand events; the disconnected producer/consumer path is established from code, not inferred from the probe's deliberately suppressed event writes.

Smallest coherent fix: expose Area Tracker from the active UI using the same snapshot contract, and materialize/read the intended demand and evidence summaries from the actual event/data source. Keep processing timestamps separate from source observations. Remove the orphan frontend recomputation path when replacing it.

## What passed, and what those passes establish

| Check | Result | Limit |
|---|---|---|
| Frontend tests | 307 passed | Existing assertions do not cover the real failures above |
| Rust controlled semantics + journey API + serving bundle contracts | 40 passed, 1 initially ignored | Controlled fixtures, except the separately run live test |
| Pinned live proof-handoff contract | 1 passed | Uses pinned `catalog-71-299c3135-23aa-4945-8f26-2cf2fc775eaa`, not the current pointer |
| Desktop/mobile journey browser suite | 36 passed, 2 skipped | Live API scenarios require a server on that pinned bundle; fixture journeys passed |
| Frontend TypeScript/Vite production build | Passed | Explicit non-deployable `https://*.audit.invalid` build origins supplied; bundle-size warning remains |
| Frontend lint | Passed | No source edits |
| Production search hardcoding gate | Passed, zero findings | Narrow runtime scope; broader audit lists review candidates outside it |
| Current-bundle observational probe | Completed | Intentionally records failures rather than asserting global correctness |
| `git diff --check` | Passed | Documentation-only final change |

The first frontend run lacked worktree dependencies; the corrected run passed. The first production build correctly required explicit origins; the configured build passed. Neither setup issue is reported as a product defect.

Runtime loads the promoted bundle rather than silently loading legacy knowledge JSON. Search uses an immutable snapshot, and the loader sorts property IDs before the ranking comparator's final ordinal tie-break. The frontend's `orderedLandingSearchResults` follows backend `orderedResultIds`; existing browser order/resume checks pass. No new ranking tie bug was established. This is not exhaustive proof of every ordering combination.

The UI Critic pass identifies the false unavailable-receipt copy and unsupported measurement/provenance claims above as substantive product issues. Existing fixture screenshots/test artifacts are under `frontend/test-results/search-journey`. They do not constitute screenshots of every live-data failure.

## Recommended checkpoints

1. Repair typed area semantics and the inventory promotion/binding gap; preserve unknown evidence instead of weakening eligibility.
2. Unify numeric and category predicate evidence with token issuance/resolution; distinguish valid receipts without destinations from unavailable receipts.
3. Remove seed geometry as a second buyer-fact source, then reconnect Area Tracker to the real data and event contracts.
4. Repair the Python runner and extend the existing controlled and pinned-live suites with these generic scenarios. Prove each checkpoint through materializer → bundle → search → receipt → UI, without replacing the frozen bank.

The temporary Rust probe is preserved at [/tmp/openestates-audit-20260921/audit_current_bundle.rs](/tmp/openestates-audit-20260921/audit_current_bundle.rs). It is an investigative harness, not a new permanent regression test. No remediation or deployment is included in this audit.
