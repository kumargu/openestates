# PR #131 follow-up: complete the search receipt handoff

## Scope and chain audit

Base: `5ea307c0`, the merge of PR #131 (`1860307`). This follow-up repairs
search → receipt → detail focus, restores contract coverage, and consolidates
portable-plan validation. The market-only property detail layout is intentionally
unchanged, as requested. No crawler, ranking, eligibility, or source-selection
policy changes are included.

The review traced `search/proof.rs` → `routes/surfaces.rs` → `surfaces.rs` →
`surfaceSceneProjection.ts`. The miss is a `proof_gap`: the named place, its
coordinates, nearby facts, and search metadata are present in the pinned bundle;
scene matching rejected the receipt's observation IDs. The duplicated
`portable_root_is_valid` implementations are an `architecture_gap` and now use
one method on the shared Boolean expression.

Baseline: 48 contract tests passed at `/tmp/proof-handoff-baseline.log`, despite
the live failure. Updating the existing fixture to use subject-scoped observations
reproduced the failure at `/tmp/proof-handoff-red.log` before changing runtime code.

## Evidence and implementation

Inspected the pinned bundle's entities, facts, and search metadata, plus raw Google
nearby records and society OSM/RERA context for 25 representative societies.
The research artifact is `/tmp/proof-handoff-source-research.json`.
22 sampled nearby records also have a place-scoped observation of the same source
record; three do not. Each observation correctly retains its own subject identity.
Other homes have nearby rows from a different source record than the coordinates
used by search. Their receipts must not be silently substituted.

Derived spatial receipts now project their verified derivation, target geometry,
source observation, and distance directly into the configured scene layer. The
receipt ID is the derivation ID. Existing features remain intact; a focused feature
is added beyond the default cap when necessary. Direct observation receipts retain
the existing exact-identity checks. The browser gives the focused receipt priority
when merging multiple observations of one place, retaining all feature handles
without borrowing a different observation's distance or source URL.

## Contract coverage

- The existing controlled API fixture now mirrors production's subject-scoped
  observations. Its source handoff assertion failed before the fix.
- The `proof_handoff_live` suite in the unified query bank pins the real bundle,
  named-place query, leading homes, fact key, and target label. Its API contract
  checks the applied feature, exact derived distance/receipt ID, and additive focus.
- The existing live browser journey now uses that bank and requires the matched
  place, visible “Matched your search”, and source link after two edits, followed
  by successful return/resume. HTTP 200 alone is insufficient.
- Restored the deleted 10,000-property named-place efficiency contract unchanged.
- Replaced unused transcript-shaped `expected_active_query` fields with exact
  `expected_buyer_brief` assertions over the typed plan. Parent queries, edits,
  operation/outcome expectations, and result assertions remain intact. This is a
  display-contract migration to the typed brief introduced in PR #131, not a
  restoration of the removed text-reconstruction path. The fixture now includes
  all localities used by the three/eight-branch scenarios. Repeated equivalent
  labels are suppressed in the brief without changing executable predicates.
- Native touch tests wait for a visible, stable hit target before the gesture;
  scroll controls, native scrolling, resize, and reduced-motion checks remain.

## Interaction note and UI Critic

This repairs the existing selected-place drawer pattern; no new interaction is
borrowed. ThreeUI access was checked and returned a login page, so no source or
states could be inspected or copied. Resting direct visits retain their default
scene. Search visits open the existing matched-place selection. Hover, keyboard
focus, touch controls, and reduced-motion behavior remain the existing controls.
No decorative motion, tutorial copy, duplicate detail section, or new chrome is
introduced. Source labels remain attached to the selected receipt. The original
search context and return path are preserved.

## Validation

- 49 Rust search contracts passed (15 semantics, 12 efficiency, 22 API).
- The pinned live-bundle API contract passed separately; its optional lake input
  is required explicitly rather than silently skipped during a live run.
- 26 surface tests and 307 frontend tests passed.
- All 36 controlled browser cases passed; both strengthened live browser journeys
  passed on desktop/mobile. The native scroll case also passed three consecutive
  runs per viewport.
- Clippy with warnings denied, frontend lint, production build, dist verification,
  the hardcoding gate (zero findings), 21 audit/benchmark Python tests, and
  `git diff --check` passed.
- CI now runs the three Rust search contract suites, including the restored test.

The initial browser attempt used the wrong local CORS origin and was rerun with
`OPENESTATES_ALLOWED_ORIGINS=http://localhost:5174`. The test also now waits for a
completed proof response rather than reading a canceled StrictMode effect request.
Neither adjustment changes the browser's proof assertions.

UI Critic: the matched place and source are visible and actionable on both
viewports; a direct visit has no search-focus overlay. No new copy/chrome was
added. Google imagery was unavailable locally; screenshots verify live evidence
and interaction states, not the 3D renderer.

| State | Desktop | Mobile |
| --- | --- | --- |
| Direct visit | [Rest](proof-handoff/rest-desktop.png) | [Rest](proof-handoff/rest-mobile.png) |
| Search receipt | [Focused](proof-handoff/focused-desktop.png) | [Focused](proof-handoff/focused-mobile.png) |

Reproduce the live contract with:

```bash
OPENESTATES_TEST_LAKE_ROOT=/path/to/data/lake \
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --manifest-path backend/Cargo.toml \
  --test search_revision_api_contract pinned_live_bundle -- --ignored
SEARCH_LIVE_API=http://127.0.0.1:4013 npm --prefix frontend run test:search-browser
```
