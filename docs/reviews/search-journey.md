# Search journey — PR #131

## Buyer story

Exact search results come first, followed by at most three backend-owned
collections, then a two-chapter product story and the footer. Search, accepted
edits, resume, property proof, workspace, and comparison retain the same signed journey.
The browser does not infer geography, broaden constraints, or rank homes.

Discovery behavior lives in `app/config/product/discovery_home.json`; journey
copy lives in `app/config/ui/search-journey.json`.
The no-search landing now opens with area, metro, schools, open-area/project-scale,
and resident-rating lenses. Price remains a card fact, not a discovery category.
Search and frontend contracts ship together, without a rolling-deploy fallback.

## Interaction review — September 20

The user’s Airbnb reference informed the clear end of the catalog and quiet,
full-width background change into the product section. The requested ThreeUI-like
interaction language is expressed with lift, depth and neighbour de-emphasis;
no ThreeUI source implementation is claimed or copied. Native horizontal
scrolling, proximity snap, and explicit arrows are retained for home rails.

- Rest: exact and broader rails share a container, heading scale, and image-first
  card layout. A one-page shelf uses its actual home count so empty reserved
  columns do not waste the no-sidebar landing width. Longer shelves retain
  measured fixed-width pages; resizing or opening the sidebar recomputes capacity.
  Each exact card retains its match reason and proof destination.
- Hover/focus: the active card lifts and grows visually while its fixed rail slot
  stays unchanged; neighbouring cards soften slightly. Facts remain readable
  below the photograph and keyboard focus receives the same state.
- Touch: native scrolling and direct single-tap links, with 44 px save/control
  targets. The mobile browser project now enables actual touch emulation.
- Reduced motion: immediate visibility, no image/lift transitions, and
  non-animated arrow scrolling.
- Product section: a quiet `About us` marker names the transition without an
  explanatory banner. A soft neutral-to-sage background shift begins at the
  boundary and the marker sits closer to the first chapter. Two alternating
  media-and-copy chapters retain the earlier side-note rhythm. The competing
  virtual-visit promo and its duplicate action were removed.
- Scroll entry: home shelves and each product chapter share one
  `IntersectionObserver` and reveal once with a restrained opacity/lift. Motion
  is immediate when reduced motion is requested.
- Intentionally omitted: autoplay, parallax, blur, glints, timer-driven stories,
  expanding card previews, and extra headings explaining the transition.

The React review keeps one shared reveal observer, parallel search/discovery
requests, lazy images, and backend-owned order. No new dependency was added.

## UI critic — searched landing

### Issues found and fixed

- Exact headings were weaker than broader headings → one shared type scale.
- Search rails and the product chapter had different left edges → one 1320 px
  maximum container with consistent responsive gutters.
- The nested virtual-visit promo competed with its chapter heading and action →
  remove that chapter and its landing-only eligibility request.
- Dark expanding result cards obscured ratings and changed geometry → reuse
  the normal browse layout; delete the spatial/preview variant and its styles.
- Mobile focus changed image ratio → remove the rule and assert stable media
  dimensions across interaction in the existing browser journey.
- Compressed horizontal product cards weakened the original visual rhythm →
  retain two spacious alternating rows with integrated side copy and remove
  the old marketing heading in favour of one quiet `About us` marker.
- Price bands and support lines repeated what collection headings already said
  → keep price in the typed payload/cards, remove it from contextual titles,
  and let each landing rail use one heading.
- The catalog-to-About handoff had two large spacing blocks → tighten both sides
  and fade the page into a muted sage section background before the marker.
- Property evidence ended as an internal-taxonomy accordion → keep one open
  Market trail chapter, collapse repeated rows by source, and align the official
  record action with its header.

### Remaining verification limits

These are controlled layout captures, not live inventory evidence. The existing
fixture uses a product illustration as a property image, lacks several property
photos, and duplicates one home in a synthetic alternative branch. Do not infer
production imagery, society deduplication, or DAG coverage from these images.
The original illustrations are retained unchanged, including their baked-in text.
Fresh live FCP/CLS measurements are still required before claiming the paint
and layout-shift targets. Against the 71-society bundle, discovery is 26,857
bytes and the three fallback collections for the controlled `3BHK` request are
15,611 bytes, both uncompressed.

### Fresh captures

- [Flexible discovery rail, desktop](search-journey/discovery-flex-desktop.png)
- [Flexible discovery rail, mobile](search-journey/discovery-flex-mobile.png)
- [Desktop search](search-journey/search-desktop.png)
- [Desktop hover](search-journey/hover-desktop.png)
- [Desktop rail-to-product transition](search-journey/boundary-desktop.png)
- [Desktop product section](search-journey/product-desktop.png)
- [Tablet product section](search-journey/product-tablet.png)
- [Mobile search](search-journey/search-mobile.png)
- [Mobile keyboard focus](search-journey/focus-mobile.png)
- [Mobile rail-to-product transition](search-journey/boundary-mobile.png)
- [Mobile product section](search-journey/product-mobile.png)
- [Reduced motion, desktop](search-journey/reduced-motion-desktop.png)
- [Reduced motion, mobile](search-journey/reduced-motion-mobile.png)
- [Property Market trail](property-market-trail.png)

## Cleanup

Removed 28 old review/generated PNG captures. Tracked versions remain recoverable
from Git; generated versions can be recreated. Product assets are untouched.
Deleted unused heading selectors, obsolete journey copy, spatial/preview card
code, and the rolling-backend-deploy fallback and its one obsolete unit test.
The frozen query bank and current evidence, ordering, pagination, and journey
contracts are preserved. The browser journey was extended, not duplicated.

## Verification

Before this visual cleanup, the implementation had passed Rust library (722),
journey API (19), conversational semantics (15), and efficiency (11) tests,
plus Clippy with warnings denied. Those are prior-run results; this UI-only pass
does not change Rust. Earlier controlled payload measurements were 7,375 bytes
for discovery and 845 bytes for contextual collections, not a live-catalog budget
measurement.

Current cleanup checks:

- Frontend tests: 311 passed; lint, TypeScript/Vite build, and dist budgets passed.
- Focused product journey: desktop and mobile passed with two-row ordering,
  one-time reveal, reduced motion, and context links.
- Browser journey: all 32 desktop/mobile cases passed. The product contract now
  asserts two rows, alternating copy, reveal completion, and reduced motion
  without adding a parallel suite.
- Checks cover fixed rail slots with transform-only visual growth, aligned
  rail/product containers, tablet overflow, reduced motion, single-tap property
  navigation, and preserved search context. No page errors in the focused cases.
- Hardcoding audit: 376 warning-only findings, zero blocked aliases. The 13 new
  warnings are expected buyer-copy assertions in the updated fixture/browser
  journey; no production search branch or alias was added.
- Git whitespace check passed.

Browser transport is mocked; this is not a live-DAG-to-browser test.
Local Node is 25; CI uses the repository’s required Node 22.

## Astra completion — baseline and chain audit, September 21

Worktree/branch verified; all inherited changes retained. Commit chain:
`46c0bd93` signed journey/proof → `08539375` buyer handoffs → `09a89c8d`
resume performance → `28ef6704` conditional journey → `a46f9ab8` discovery
shelves → `da300aa2` continuous landing. Uncommitted work adds collections,
compact browse cards, two About rows and Market trail. No second journey model
is needed. Duplicate shelf filter/sort engines exist in discovery and collections;
collection topology traversal also duplicates the canonical compiler.

Checkpoint 1 baseline artifacts: `/tmp/pr131-astra/baseline-contract.log`
(19/19 journey API tests), `baseline-audit.log` (zero blocked aliases),
`baseline-live-summary.json` and individual response files (IDs, predicates,
proof keys, memberships and bytes), `baseline-profile.log`. Rebuilt debug API,
restarted against the existing lake, verified the requested bundle in health and
responses. Broad Whitefield: 375,244 bytes; 2.832 s cold / 0.025 s warm.
Whitefield+BHK and the two-area OR search reach the unchanged 3 s timeout.
`3BHK` has zero exact results but returns mixed-BHK fallback shelves: a confirmed
architecture bug, not evidence of eligible inventory. Baseline stale unit-test
expectation still requires three About chapters despite the accepted two.

Checkpoint 2 audit: existing start/revise/resume contracts pass (including stable
IDs, branch edits, clarification, rejected edits, idempotency, catalog rebase and
portable AST resume). Live chained verification is carried into checkpoint 3
because its initial request times out. No typed-journey replacement is warranted.

Measured architecture gaps for checkpoint 3: collection planning takes 3.268 s
for Whitefield+BHK, in addition to 0.827 s compilation / 0.860 s evaluation.
Tantivy diagnostic stored-text processing accounts for most evaluation time.
Fallback shelves ignore accepted constraints; broadening drops negated geography
and mutates negated budgets; collection adjacency fabricates zero distances;
“other” membership depends on capped nearby results. Investigate missing inventory
and named-place proof directly in Parquet (`parquet-research.json`) before any
runtime eligibility changes. Honest missing-proof results must stay empty.

Checkpoint 3a: discovery shelf filtering/sorting now has one immutable hydration
owner; request handling selects from uncapped ranked IDs and deduplicates after
eligibility. Configuration parses/validates once through `OnceLock`. Four focused
library tests pass, including the explicitly updated two-chapter product decision.
The unchanged journey contract is rerun before altering collection semantics.

Checkpoint 2 correction: direct Parquet inspection found canonical Waterford
listing observations with valid lineage and matching typed BHK/price values.
The bundle's `proof_snapshot_identity` differs from its catalog version. Inventory
hydration, journey proof issuance and proof resolution incorrectly used the latter.
All three now use the manifest proof identity. The existing API contract gained a
separate-catalog/proof-identity case covering initial search, edit, resume and detail
proof; 20 tests passed. This was a verified architecture/proof gap, not absent data.

Checkpoint 3 completed: one config-driven collection planner preserves positive
ceilings versus exclusions, required spatial proof, BHK and excluded geography.
Nearby and other membership use canonical distance-checked topology independently
of display caps; branch eligibility remains local before shared rails merge.
Fallback selection intersects the authoritative non-geographic evaluation with
hydrated shelf rankings. Missing topology omits geographic claims; unresolved
required intent remains unresolved. Replaced the duplicated shelf engines and
zero-distance adjacency traversal. A populated extension of the existing journey
fixture now proves three ordered rails, society deduplication, distant adjacency
rejection, required-school evidence, BHK/exclusions, edits/resume, multi-branch
merging and non-empty missing-topology fallback. This also caught an intent gap:
negated numeric ceilings previously lost polarity in parsing. Budget slots now
carry configured exclusion polarity into the existing Boolean AST.

Verification: 22 journey API, 15 conversational semantics and 11 efficiency
contracts pass (`/tmp/pr131-astra/search-checkpoint.log`). No frozen expectation
was weakened. Streaming Tantivy diagnostic matching preserved recall while cutting
Whitefield+BHK recall from 783 ms to 16 ms. Reusing each destination distance within
canonical traversal cut compilation 837→334 ms; evaluation 860→104 ms; collection
planning 3268→397 ms. Broad Whitefield's 32 ordered exact IDs remain identical.
Profiles: `baseline-profile.log`, `collections-profile.log`, `distance-profile.log`.
All measured in debug mode; request timeout remains 3000 ms.

## Completion interaction note

Re-read the current landing, tile, rail styles and `.claude/skills/ui-critic.md`.
ThreeUI access was attempted at `https://threeui.com/`; the network returned
“Web Page Blocked” (`/tmp/pr131-astra/threeui.html`). No source or state was
available to inspect and no ThreeUI implementation is claimed or copied.
Retain the already accepted native scrolling/fixed-slot card pattern. Rest uses
available container width, hover/focus changes transforms only, touch scrolls
natively with single-tap links, and reduced motion uses immediate visibility and
non-animated scrolling. Arrows track actual scroll boundaries; ResizeObserver owns
capacity changes and only discrete boundary/index state reaches React. No autoplay,
new motion system, detail preview, price headings or removed chapter returns.

## Astra checkpoints 4–6 — compact handoffs and cleanup

Exact results now flatten the same compact browse-card contract used by contextual
rails, retaining ordered IDs, structured reasons, signed proof references and home
state. Full detail payloads, internal scores/signals, legacy snake-case adapters
and unused discovery envelope fields are removed. Hidden card labels still retain
proof navigation: omitting a duplicate BHK/price caption must not discard its receipt.
`active.collections` is always an array; typed price bands remain in the API, never
in headings. Both exact and contextual cards use `BrowsePropertyTile`.

Navigation projects exact homes followed by explicitly contextual homes from the
accepted response. Contextual homes carry their collection title and no match
proof. This works with zero exact matches through detail, workspace and compare.
The latest live two-edit test caught an actual history bug: acceptance saved the
history index before pushing the revision URL. Departure now checkpoints the
accepted page's index, so Back returns to the latest revision instead of the prior
budget-only search. The existing navigation unit contract covers this regression.

Another live miss exposed an architecture gap: the signed portable AST omitted
unresolved requirements that existed in compiled geography scope. Resume could
therefore broaden an unknown-place search. The portable branch now carries that
state, compilation preserves it, and the buyer brief includes the unresolved
request. Existing API coverage requires identical intent and empty results after
resume; newly broken catalog bindings still trigger the existing recovery flow.

Collections now require sourced candidate membership before claiming “Other
areas”. Direct Parquet inspection found SUMADHURA EDEN GARDEN's listing locality
said Whitefield, but its only non-proximity edge was `built_by`; it had no market
membership or occupied geo cell. It may enter a compatible configured shelf, but
cannot support a geographic claim. The populated fixture includes unlocated homes
and still requires all three non-empty geographic strategies without using them.

Rail capacity comes from one ResizeObserver. Passive native scroll events schedule
one animation frame and update React only for changed index/boundary state. Arrow
availability follows swipe/trackpad positions and resizing. Revised rails remount
under the accepted revision, and reduced motion reveals immediately and scrolls
without animation. Desktop checks include the saved sidebar appearing; mobile
checks issue a real Chrome touch scroll gesture. Card growth remains transform-only.

Deleted replacements: `LivingEvidenceTile` → shared browse tile; duplicated
request-time shelf engines → immutable hydrated rankings; duplicate adjacency
traversal → canonical geo-cell expansion; legacy frontend reason inference and
wire adapters → backend reason projection; trailing spacer cards → actual scroll
boundaries. Removed the temporary profiling example after recording measurements.
Area Tracker, property surfaces, imagery, frozen query bank and conversational
semantics contracts remain. No remote push or new test framework.

## UI critic — rebuilt live landing/search

Blockers found and fixed: unlocated candidates described as “Other areas”; detail
Back reopening the previous revision. No remaining blocker in reviewed states.

Passes: fixed slots use the available width; hover/focus leaves neighboring slots
stable; native touch and boundary arrows agree; no clipping on desktop/mobile;
contextual homes have their own heading; two About rows, quiet marker, sage
transition, illustrations and Market trail remain. The card surface presents name,
configuration, size, price and rating once. Internal pipeline language stays out of
the buyer UI. Placeholder images reflect missing catalog media and are not evidence.

Live captures use the optimized production frontend and the rebuilt local Rust
API. The production HTTPS API origin is proxied to localhost in Playwright; payloads
are real, unmodified bundle responses, not fixture data. Older unprefixed captures
in this directory are inherited mocked-transport evidence. Fresh `live-*` captures
are the acceptance evidence for this pass.

## Astra checkpoint 7 — final verification

All seven checkpoints are implemented. Final gates: 845 Rust tests across 32
test/doc-test runs; 22 journey API and 3 serving-bundle tests rerun after the final
structural cleanup; Clippy with `--all-targets -- -D warnings`; 307 frontend tests;
lint, TypeScript/Vite production build and dist budgets; 38 desktop/mobile browser
checks (36 controlled transport, 2 real API); hardcoding audit (zero blocked aliases);
`git diff --check`. The frozen query bank was not changed.

Broad-suite corrections: the initial invocation used the worktree’s empty lake;
rerunning with `OPENESTATES_LAKE_URL` targeting the required lake passes. The
serving-edge assertion now compares to the manifest proof snapshot identity, as
the production writer does. No assertion was relaxed. Native debug-linker unwind
size warnings and Vite’s existing large-map-chunk warning remain; both builds and
the repository’s explicit dist budgets pass. Local Node is 25, while CI pins 22.

Final isolated query pass used a freshly restarted debug API, with pacing to avoid
benchmark traffic consuming the normal rate limiter. Every query returned HTTP 200.
An earlier overlapping browser/query run received 429s; rate limits were not changed.
The original request timeout stays 3000 ms. Cold means first request after restart,
warm means the repeated request.

| Query | Exact homes | Cold / warm ms | Envelope bytes | Collection bytes |
|---|---:|---:|---:|---:|
| `homes in Whitefield` | 32 | 862 / 14 | 37,769 | 14,864 |
| `3BHK in Whitefield` | 24 | 855 / 29 | 75,591 | 14,989 |
| `3BHK` | 32 | 107 / 47 | 82,916 | 4,242 |
| `2BHK in Whitefield under 2 Cr or 3BHK in Sarjapur Road under 2.5 Cr` | 20 | 1360 / 58 | 100,468 | 9,618 |
| `quiet 3BHK near schools under 2.5 Cr` | 32 | 882 / 70 | 207,366 | 6,309 |
| `3BHK near ITPL with schools nearby under 3 Cr` | 0 | 166 / 18 | 7,467 | 2 |
| `3BHK not in Whitefield` | 31 | 131 / 54 | 79,126 | 2 |
| `3BHK in Zzyzx Gardens` | 0 | 74 / 40 | 4,848 | 2 |
| `3BHK under 1 lakh` | 0 | 70 / 40 | 5,968 | 2 |

Broad Whitefield retains all 32 ordered result IDs while dropping from 375,244 to
37,769 bytes (90% smaller). Discovery is 20,008 bytes. Largest measured collections
are 14,989 bytes, below 80 KB. Full IDs, predicates, proof keys and rail membership
are preserved in [astra-validation.json](search-journey/astra-validation.json).
Compilation/evaluation/planning/serialization profiles remain in
`/tmp/pr131-astra/{baseline,collections,distance}-profile.log`.

Expected versus actual: BHK/budget/branch searches return compatible exact homes
and bounded contextual alternatives; exclusions stay excluded. Quiet/schools and
metro are configured preferences rather than fabricated required evidence. ITPL,
the intentionally unmapped place, and the impossible budget return zero exact
homes and no invented collections. All three geographic strategies are non-empty
in the controlled contract; the live bundle uses only the strategies its topology
and available inventory support.

ITPL is a `data_gap`: no serving entity named ITPL and no corresponding entity
metadata were found. The text exists in listing locality, addresses, reviews and
media captions, which is insufficient for a named-place proximity claim. Unlocated
societies likewise need DAG membership enrichment; no aliases or instance-specific
production exceptions were added.

The live UI journey verifies ordered API cards, per-card reason labels, signed proof
links and collection IDs, then performs Whitefield → budget → metro → proof → Back
→ resume and compares the complete accepted intent and ordered IDs. Zero-exact
contextual detail/workspace/compare handoffs have a separate case in the same
existing browser suite. Race, retry, clarification, rejected edits, undo, storage
loss and copied-link coverage remain.

`live-*-journey-*` screenshots come from the real-API browser journey using the Vite
dev server. Other `live-*` screenshots and performance measurements use the optimized
production frontend with real local API responses. `mock-touch-scroll-mobile.png`
is explicitly controlled fixture evidence. ThreeUI source access remained blocked.


| Optimized warm page | Desktop FCP | Mobile FCP | CLS (both) |
|---|---:|---:|---:|
| Landing | 348 ms | 348 ms | 0 |
| Searched landing | 380 ms | 360 ms | 0 |

Measured on local Chrome without CPU/network throttling, after reload with a warm
API/cache. Layout-shift observation includes data arrival and 1.5 seconds after
cards appear. These are local measurements, not field performance guarantees.

Fresh captures: [desktop search](search-journey/live-search-desktop.png),
[mobile search](search-journey/live-search-mobile.png),
[hover](search-journey/live-hover-desktop.png),
[focus](search-journey/live-focus-mobile.png),
[touch](search-journey/live-touch-mobile.png),
[scroll boundary](search-journey/live-rail-boundary-desktop.png),
[About transition](search-journey/live-about-boundary-desktop.png),
[reduced motion](search-journey/live-reduced-motion-mobile.png),
[live proof](search-journey/live-proof-journey-desktop.png).

Commands: `OPENESTATES_LAKE_URL=file://<lake-root>
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo test --manifest-path backend/Cargo.toml`;
`CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo clippy --manifest-path
backend/Cargo.toml --all-targets -- -D warnings`; frontend `test`, `lint`, production
`build`, `verify:dist`; `SEARCH_LIVE_API=http://localhost:4000 npm --prefix frontend
run test:search-browser`; `python3 scripts/audit_search_hardcoding.py`.

After measuring the optimized preview, normal local browsing is restored: rebuilt
API on 4000 and Vite dev on 5173, using the live API with fixtures disabled. All
work was prepared on `codex/pr-131-contextual-rails`. Publishing to PR #131 was
subsequently authorized by the owner.


## Publishing preparation

The final PR snapshot is consolidated under `kumargu` using the account's GitHub
noreply identity for both author and committer. Published notes omit machine-specific
paths, and local browser configuration uses `localhost` instead of numeric host
addresses. A staged-content audit found no private or public IP literals in the PR
additions. Existing validation evidence above describes the completed implementation;
remote CI validates the published commit separately.
