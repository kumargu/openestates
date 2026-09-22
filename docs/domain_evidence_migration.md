# Domain and evidence migration

Base: audited main `2f0f4e27`. Worktree: `openestates-contracts`; unrelated OSM work remains in the original checkout. The two September 21 audit reports are retained alongside this note.

## Chain audit and baseline

`18603071` introduced journeys; `f41e7475` tightened exact receipt handoff; `afc4432b` updated interaction checks. The remaining old paths crossed measurement, proof and client boundaries. Classifications: area projection and inventory admission are architecture gaps; missing inventory observations are data gaps; category/numeric receipts are proof gaps; the retired benchmark envelope is an architecture gap.

The audited Parquet inspection is preserved at `/tmp/openestates-audit-20260921/live-records.json`, with the pointer, bundle, counts and eligibility findings recorded in `search_data_ui_audit.md`. No observations are reconstructed from URLs or timestamps in this migration.

New baseline: `/tmp/openestates-contracts-baseline.log`. Extending the existing source → Parquet → bundle → production-router scenario reproduced three super built-up listings satisfying a carpet-only constraint. This regression catches a real bug; it does not change a frozen expectation.

## Checkpoint 1: measurement and durable witnesses

Producers: sourced listing aggregates and serving facts. Consumers: property projection, numeric/category evaluation, journey proof projection and proof resolution.

- Preserve measurement basis, units, scope, range and observation reference. Stop copying listed area into both carpet and super built-up fields.
- Delete runtime-field numeric evidence. Numeric evaluation uses sourced inventory measurements or eligible serving observations, retaining its typed constraint and witness, including an unsatisfied bound.
- Category-distance predicates reference their durable source observation directly.
- Remove UI destinations from signed proof identity. Destination projection remains a separate presentation concern during the next checkpoint.
- Delete seed-polygon receipt synthesis and its dedicated tests. Remove unused graph property/society projections and their private fixtures. Keep serving, geometry and parser coverage.
- Remove the orphan Area Tracker UI, client and endpoint. Restoration requires separate product work over promoted facts and the durable search event stream.

Validation: `/tmp/openestates-contracts-tests.log`: materializer vertical contract, 15 controlled semantic contracts and 22 journey API contracts pass. The pinned live promotion gate remains separate. `/tmp/openestates-contracts-check.log`: `cargo check` passes. The vertical contract opens all emitted references for BHK, numeric and spatial queries and asserts super built-up cannot establish carpet eligibility.

## Remaining checkpoints

Inventory admission/reporting; public DTO schema generation and boundary validation; proof outcomes independent of UI destinations; snapshot-pinned context reads; bounded selected-home hydration; migrate the benchmark; real API browser journeys; consolidate obsolete tests and documentation; full lint/build/integration/hardcoding gates and screenshots.

## Checkpoint 2: public boundaries and admission

Producers: typed runtime attributes, RERA registry normalization and search evaluation. Consumers: generated wire decoders, benchmark, selected-home summaries and detail proof views.

- Rust DTOs generate JSON Schema and TypeScript; frontend decoding rejects malformed journey/detail/summary/proof responses before caching them.
- The benchmark consumes only the current revision envelope and authoritative ordered IDs. Its proof checks resolve signed receipts; client-authored focus is retired.
- Signed proof issuance and resolution share one validator over the immutable snapshot. Missing evidence cannot acquire a signed reference. Presentation destinations moved out of search proof code and into `surfaces/proof_focus.rs` with UI config under `app/config/ui/`.
- Shared property-card projection replaces graph-enriched card construction. Unknown inventory attributes are omitted from serialization; bounded summary responses expose availability and preserve requested order.
- The vertical test's former numeric loop was vacuous. Requiring a nonempty result exposed missing RERA source observation identity in the materializer (`data_gap`). The fix carries the actual serialized registry record's digest, captured timestamp and asset lineage through normalization; no inferred source observation, entity join or crawler fallback is introduced.
- The same materialized router can serve a loopback fixture for browser journeys via `OPENESTATES_CONTRACT_SERVER_ADDR`. Its ordinary contract runs both negative area-basis cases and an explicit carpet case.

Interaction research: reviewed the ThreeUI source tree (`MengTo/threeui`, main). No directly relevant receipt disclosure was found. The generic receipt uses native disclosure: closed at rest, native pointer/keyboard focus and touch activation, no animation under either normal or reduced-motion preferences. Decorative demo motion was not borrowed. A receipt remains usable without a map or specialized section.

Checkpoint 2 verification:
- Rust library: 712 passed. Controlled semantics: 15 passed; journey: 22 passed; serving: 3 passed; source-materialized vertical: passed. Clippy has no warnings.
- Frontend: 306 passed before replacing two source-text checks with the real browser assertions; lint and production build pass. Vite reports the existing large map chunk.
- Existing browser journeys: 36 passed. Materialized desktop/mobile journeys: 2 passed, including search → generic receipt → EMI plan → RERA document disclosure → back. No application API is mocked. Bounded summary reads preserve requested order; all related detail resources reject a retired snapshot.
- Benchmark and hardcoding checks: 22 Python checks passed; production search audit: zero findings. The frozen query bank is unchanged.
- The live-bundle browser check is now an explicit `test:promotion-browser` gate requiring `SEARCH_LIVE_API`; required CI suites do not silently skip it.

Coverage mapping: obsolete graph-only inventory assertions now check withheld inventory and browseable societies; coordinate fixtures share one real observation identity across latitude and longitude, matching the production materializer. Quarantine coverage includes the new society browse representative. The deleted JSX/source-name tests are replaced by rendered official-link and bounded-network assertions in `e2e/contracts/journey.spec.ts`.

UI critic: the generic receipt stays closed at rest, has a keyboard-focusable native summary, and exposes its claim and source only on request. No extra heading, badge, motion or tutorial copy was added. Screenshots capture [desktop rest](verification/domain-evidence/desktop-receipt-resting.png), [desktop open](verification/domain-evidence/desktop-receipt-open.png), [mobile rest](verification/domain-evidence/mobile-receipt-resting.png), and [mobile open](verification/domain-evidence/mobile-receipt-open.png). External map rendering is intentionally unavailable in this contract; the screenshot does not assert map geometry or imagery. The receipt is usable in that state. React review confirms cancellation guards, snapshot dependencies, and native disclosure semantics.

Still required before completion: remove remaining graph enrichment and name-recovery paths; replace layout-addressed context resources; finish dedicated domain DTO ownership and capability admission; validate identity aliases against representative source records; rerun all gates after those replacements. This checkpoint does not claim those migrations are complete.

## Checkpoint 3: retire graph and synthetic map fallbacks

Producers: promoted facts and explicit serving graph edges. Consumers: detail, evidence panels, recommendation branches and scene projection. Deleted the alternate property-map constructor, graph-only detail enrichment, graph confidence implementation, source-less Google/community fallbacks, and name-based related-society recovery. Builder portfolio membership now follows `built_by` edges. The surviving map projection uses typed, receipt-backed scene geometry.

The identity/fallback research gate compared 30 societies across raw OSM, RERA, Google and listing inputs. Artifact: `/tmp/openestates-contracts-source-research/identity-source-comparison.json`. It preserves raw IDs, names, coordinates, polygons, phases, missing fields and ambiguous candidates. Findings: 23 exact OSM/RERA names, four source qualifiers, three project/phase differences; 15 missing RERA coordinates; two societies with distinct Google IDs; residential, construction and building polygons all occur. Names and phase suffixes cannot establish runtime identity. Missing joins must remain missing until sourced relationships exist.

Coverage mapping: deleted tests exercised removed graph-only constructors and fabricated Google URLs; serving-based evidence assertions, typed geometry tests and additive focus tests remain. Canonical-society scene fixtures now put coordinates and proximity facts on the same explicit entity. Recommendation coverage supplies builder edges instead of matching promoter strings. Validation: `/tmp/openestates-contracts-cleanup-contracts.log`: 691 library, 15 controlled semantics, 22 journey, two recommendation and one materializer-to-router tests pass. The separately pinned live promotion test remains explicitly ignored in this ordinary run.

## Checkpoint 4: source receipts and bounded public attributes

Producers: serving observations and admitted inventory. Consumers: source panels, shared summary cards, generated detail/catalog/evidence schemas and RERA navigation. Every source-panel item now carries observation references for its immutable snapshot; multi-source attributions retain their individual references. Rows without valid subject-bound observations are withheld. Removed the Street View availability receipt constructor, which invented confidence and observation time; visual frames remain visual context. Removed the unconsumed evidence-batch endpoint. RERA navigation pins its first detail read to the carried journey snapshot.

The materialized contract exposed a second area bug: summary cards copied generic listed area into `carpet_area_sqft` even though detail was correct. Baseline: `/tmp/openestates-contracts-summary-basis-baseline.log`. The shared projection now preserves carpet meaning, and batch/detail checks assert the same basis. The vertical contract verifies every emitted source-panel reference exists in the loaded evidence index.

Public `PropertyAttributes` and `SocietySummary` explicitly project supported attributes instead of embedding storage records. Removed unused internal scores, enrichment placeholders, derived area profiles and pipeline source-reference fields from detail. Catalog and evidence now have generated boundary validators. Existing cache behavior coverage uses the shared valid card fixture; story fixtures no longer exercise deleted similar-property fields.

Checkpoint 4 verification: 692 library tests and the materialized vertical contract pass; controlled semantics (15), journey API (22) and recommendation scenarios (2) remain green. Frontend: 304 tests, type checking, lint and production build pass. Real materialized desktop/mobile journeys: 2 passed, no skipped tests. Search hardcoding audit reports no findings. Existing bundle-size and macOS linker-size warnings remain. Baseline failure was corrected without changing the frozen bank.

## Checkpoint 5: explicit identity and bounded hydration

The source comparison rules out implicit identity joins. Runtime societies now carry canonical serving IDs; projected inventory IDs derive from those IDs rather than display names. Direct property records require an explicit `in_society` edge. Removed society-name alias generation from fact, graph, recall, recommendation and RERA receipt lookup, plus the display-title/BHK-slug parser. Display names remain searchable vocabulary, never joins. Explicit canonical spatial provider bindings remain intact.

Fixture migration retains all product assertions and supplies missing explicit memberships. The same-name society scenario now asserts independent IDs, stable across source row reordering and renaming. Runtime area references use serving edges; source labels remain display labels. Deletion mapping: only the private slug parser and inferred-name-alias unit tests were retired; canonical recall, explicit-edge recall, geometry and frozen semantics remain required.

Gold materialization now rewrites aliases only from explicit `SourceEntitySeed` mappings; identical display names no longer merge or reject independent canonical entities. The existing gold tests preserve subject rewrites while using an explicit mapping whose display spelling is unrelated. Runtime and promotion consume the same canonical IDs. Removed the unused RERA alias mutator and the impossible name-collision quarantine branch.

The real browser comparison extension caught a bounded-hydration race: saving a second home pruned it against the previous selection's response. Deleted that catalog-era pruning effect and bind context reconciliation to the completed request's ID set and snapshot. Aborted responses cannot overwrite newer hydration state. Comparison groups by canonical society ID and displays measurement ranges with their configured basis. Removed the two unreferenced comparison-selection helpers (repository-wide reference check found definitions only).

Interaction note: the comparison change uses the existing home links and checkboxes, with basis text in the existing summary line. ThreeUI's inspected pattern inventory has no relevant typed-measurement comparison pattern; no new pattern or motion is borrowed. Existing hover, keyboard focus, touch and reduced-motion behavior remain. This is a semantic correction, with screenshots and a real save → compare → return journey as its UI gate.

Expected fixture ordering change: canonical inventory IDs replace name-derived slugs, changing equal-score lexical tie-breaks. The browser asserts authoritative ordering survives navigation and that the selected API price survives detail → plan; it no longer assumes a particular society occupies the first slot. The frozen query bank is unchanged.

Removed the unused `/api/societies/search` and society-slug endpoint after checking frontend, scripts, tests and route references. Their ranking/default-confidence implementation and private tests are retired; the production journey contracts remain the search gate. AppState no longer carries an empty mutable knowledge graph. Offline graph materialization and durable search-event ingestion remain.

Verification: materialized browser journeys pass on desktop and mobile, including saving two homes, comparing measurement bases, and returning to the authoritative ordered results. The focused library, controlled semantics, journey API and vertical contracts pass (`/tmp/openestates-contracts-identity-final.log`). Frontend lint and production build pass. The existing large map-chunk warning remains.

## Checkpoint 6: domain context and snapshot transitions

Producers: promoted contextual facts, explicit source identities and the spatial evidence index. Consumers: `/api/properties/{id}/context`, bounded `/api/properties/context/batch`, detail context, and frontend scene projectors.

The context DTO contains bounded typed facts, canonical targets, geometry, source observations and optional resolved proof. It contains no surface IDs, layers, camera controls, component names or DOM targets. Context collection bindings extend the existing fact registry. UI configuration controls layers, labels, caps, styles and navigation; Rust no longer loads that presentation file. A source URL join requires an unambiguous promoted provider identity and is indexed at snapshot hydration. Names and row positions cannot bind targets. Geometry is withheld without durable inputs, and numeric context no longer comes from parsing a nearby display string.

Deleted the old single/list/batch surface routes, scene constructor, backend map projector, display-text parser, ordinal linked-entity fallback, backend proof-destination mapper and obsolete surface-request settings. Existing polygon/line geometry safety assertions remain in the domain module. Exact proof identity and additive resource assertions remain in the journey API contracts. Presentation cap/focus coverage moved to the frontend projector: the exact sixth receipt expands a five-item selection while retaining all five defaults.

A second materialized snapshot exercises real browser transitions. The fixture control listener is compiled only in the integration test; production exposes no test endpoint. A stale detail response clears displayed data and offers one `Search again` action preserving the query. Both snapshots pass the same source-to-Parquet-to-router assertions before serving. Browser reads remain subject to ordinary API rate limits; the explicit batch assertion retries transient 429 responses without mocking or bypassing the limiter.

Archived Waterford inputs are explicitly renderer fixtures, not evidence-admission or promotion fixtures. Their active transport uses the new wire DTO; unsupported archived source-panel items were removed. Original scene geometry remains only for camera, polygon and presentation regression tests. The real materialized browser journey is the required API semantics gate.

Interaction review: the context cutover preserves existing layer controls; no new motion is introduced. The stale state uses an existing page-state layout and a native link with normal hover, keyboard focus and touch behavior; reduced motion adds no animation. ThreeUI has no relevant stale-snapshot or typed evidence pattern in the inspected inventory. UI critic: the mobile stale state has one message and one next action, with no duplicate facts or implementation terminology. Screenshots include the resting receipt, opened receipt, comparison and changed-snapshot state.

Checkpoint 6 verification: 667 library tests, 15 controlled semantics, 22 journey API and the real materialized vertical contract pass. Frontend: 305 tests, lint/type checks and production build pass. All 36 existing search-browser journeys and both real desktop/mobile journeys pass. Clippy has no warnings; the production-search hardcoding gate reports zero findings. The broader review-aid scan retains pre-existing findings outside that gate. Frozen buyer-query expectations remain unchanged.

## Checkpoint 7: catalog identity witnesses

The named-society journey regression reproduced a satisfied identity with no receipt (`/tmp/openestates-contracts-identity-witness-baseline.log`): `proof_gap` / `architecture_gap`. Producers are promoted entity and relationship records; consumers are the generic identity evaluator, compiled predicate bindings, reason issuance, proof resolution and the generic frontend receipt. Catalog records are evidence for structural identity; they do not pretend to be source observations or acquire invented timestamps/confidence.

Removed recall-membership and display-name eligibility fallbacks, their unused exclusion helpers and the private recall-membership Boolean helper. Explicit self identity and configured memberships retain canonical targets and content-addressed relationship references. Missing or ambiguous exclusive assignments cannot establish a negative. Resolution uses the same evaluator and the identified immutable catalog. Configured broad-region terms resolve to catalog entities before execution; resolved spans replace unresolved parser placeholders.

Named geography can intentionally broaden browsing. Exact matches now retain the original branch's identity witness; nearby results cannot claim that exact identity. Existing unit fixtures now supply explicit entity, inventory, locality and topology bindings. Their result expectations and the frozen query bank remain unchanged. The source-materialized context assertion now requires a nonempty receipt collection.

Interaction note: identity receipts use the existing native disclosure and one configured relation label. The catalog target is shown only inside the opened receipt. Existing rest, hover, keyboard focus, touch and reduced-motion behavior is preserved. No additional layout, heading or motion is borrowed from ThreeUI.

Verification: 667 library tests pass. The source-materialized vertical contract, 15 frozen semantic contracts and 24 journey contracts pass (`/tmp/openestates-contracts-identity-witness-api-final.log`). The live promotion case remains a separate gate. Clippy, generated schemas, frontend type checking and lint pass; production-search hardcoding gate reports zero findings. Full UI/browser verification follows the remaining cutover work.

## Checkpoint 8: evaluator-backed capability admission

The existing capability test reproduced low-confidence and nonnumeric rows advertising a numeric preference (`/tmp/openestates-contracts-capability-baseline.log`, `architecture_gap`). Admission now calls the same positive/negative evaluators used by ranking and requires a valid subject-bound observation. Required negative preferences fail closed when their evidence index is absent. Source confidence thresholds moved from Rust source-name branches into the existing scoring policy without changing their values.

Producers: promoted fact/metadata pairs and scoring config. Consumers: capability checks, required-preference evaluation and serving validation. Deleted the parallel observation-only admission path and source-specific threshold branches. Promotion reports unavailable entity/fact/preference bindings separately; raw society facts remain usable even when a claimed search capability is unavailable. UI destinations are not admission requirements.

Verification: 667 library tests, 15 frozen semantic contracts, 24 journey contracts and the two-snapshot materialized vertical contract pass (`/tmp/openestates-contracts-capability-check.log`). Clippy passes for all targets. Frozen expectations remain unchanged.

## Checkpoint 9: retire synthetic area and tag models

Producers: admitted property attributes and contextual facts. Consumers: catalog, detail, recommendation input and frontend context projection. Removed orphan area routes, area profile loading/state, search areaContext and the synthetic transparency_tags field. Comparable median prices now require the same canonical area, measurement basis and unit. Deleted assertions covered only retired fabricated tags; canonical identity and financial contracts remain.

Replaced the retired smoke script with real HTTP journey, bounded detail/context/batch, receipt, resume, revision and stale-snapshot assertions. Restored configured geographical spreading in the frontend context projector while preserving additive exact-proof focus. The existing presenter contract now covers that behavior.

Verification: 667 library, 15 frozen semantic, 24 journey, two recommendation and the two-snapshot materializer contract pass. Frontend 306 tests, type checking, lint and build pass. Real API smoke passes with bounded hydration and resolved identity receipts (`/tmp/openestates-contracts-api-smoke.log`). Generated schemas are refreshed.

## Checkpoint 10: materialized context targets

The existing source-to-Parquet-to-router contract now asserts a canonical school target, coordinates and geometry receipts. It reproduced the observed null target (`/tmp/openestates-contracts-context-binding-baseline.log`). Direct Parquet inspection found the subject relation, place URL identity and observed coordinates; this was an architecture/proof-binding gap, not missing source data. Runtime alias hydration made one provider identity appear ambiguous.

The serving builder now materializes content-addressed context-target edges from the existing fact-registry bindings, actual observations and canonical provider relationships. Ambiguous canonical targets remain unbound. Rebuilds replace these derivations. The runtime consumes those edges; the request-time source-URL and linked-fact joins are deleted. The same materializer prepares journey fixtures. Both materialized snapshots and all 24 journey API contracts pass (`/tmp/openestates-contracts-context-binding-check.log`).

## Final contract and fixture reconciliation

The agreed removal of name-derived identity requires two identifier-only changes in the pinned live scenario: Godrej Air remains first (`discovered-rera-c1af3dd6c1581e3e-3bhk`), Brigade Lakefront Crimson remains second (`discovered-rera-ccfa353d22484c7b-3bhk`). These canonical society IDs were read directly from the immutable pinned entities Parquet. Query, BHK, ordering, school and proof expectations are unchanged; no alias lookup or name recovery was added to runtime. The original pinned bundle remains immutable. The separately invoked live contract passes with no ignored cases.

The efficiency fixture now supplies explicit society/area records and membership edges and compiles the same query before evaluating candidates. It preserves its 12-result, pruning and latency assertions. This replaces its retired assumption that recall alone proves area membership. All 12 efficiency contracts and all three serving-bundle contracts pass.

Source-panel text is no longer parsed to infer distances for ordering. Observed collections sort by evidence confidence and text; typed context owns spatial distance. The existing nearby test preserves both values and their source attribution with the new deterministic order. Structured livability signals now require subject-bound observations, matching community evidence admission. Archived Waterford source-panel rows without receipts remain withheld; the renderer fallback test checks official-record navigation, reviews and photos. Actual fact/receipt completeness is covered by the required materialized API journey.

Engineering guidance now separates domain/presentation ownership, disallows embedded fallbacks, retains the source-research gate, and treats observation timestamps only as provenance. Area Tracker restoration remains separate product work.
