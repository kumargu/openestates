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
