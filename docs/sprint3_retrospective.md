# Sprint 3 Retrospective (Days 59-72)

## Sprint Theme: RERA Data Foundation & Trust Model

Sprint 3 inserted a foundational data layer between Sprint 2 (seller-buyer connection) and the originally planned Sprint 3 (search quality). The thesis: government-sourced RERA data as the trust anchor makes every other feature more credible.

## What was delivered

**RERA-Seeded Knowledge Graph (Days 59-63)**
- `seed_from_rera.py` skill: scrapes Karnataka RERA portal, produces structured society/builder/area/property nodes
- 70 society nodes, 44 builder nodes, 19 area nodes in `data/knowledge/nodes/`
- 155 properties in seed data with RERA registration IDs
- Per-entity JSON files with atomic writes (S3-ready layout)

**Trust Badges & Confidence Scoring (Days 64-68)**
- 6 frontend trust components: TrustBadge, ConfidenceMeter, BuilderTrustBadge, DataFreshnessBadge, ProjectStatusTag, ComparePanel trust columns
- Confidence scoring engine: weighted combination of fact_count, source_diversity, rera_registration, data_freshness, graph_driven_pct
- Trust badges appear on property detail, results cards, and compare views
- "Verification pending" tags for freshly discovered data

**Graph-Driven Search (Days 65-68)**
- `answers_preferences` matching: facts declare which user preferences they satisfy
- `scoring_hint` driven ranking: facts carry their own scoring direction and weight
- Cross-node scoring: builder trust facts flow to properties via BuiltBy edges
- Area-level fact aggregation for neighborhood-level search signals
- 9 graph_area_match tests, 6 confidence tests

**Canonical Builder Resolution (Days 69-71)**
- Orphan builder detection and canonical_builder fact generation
- 5 builder duplicates resolved (e.g., "Prestige Group" variants -> single canonical)
- Query-time resolution: enrichment route follows canonical_builder edges
- 14 Python fuzzy match tests, 2 Rust canonical resolution tests

**Confidence Calibration & Polish (Day 71)**
- `fact_source_quality` scoring for detail route (replaces hardcoded graph_driven_pct=0.0)
- Freshness capping: bulk-created nodes with identical timestamps capped at 0.5 freshness
- RERA portal URL linked in Data Provenance sidebar
- Badge deduplication: header badges removed, Data Provenance sidebar is authoritative

**Pipeline Skills (Days 59-63)**
- `seed_from_rera.py` -- RERA portal scraping and knowledge graph seeding
- `compute_builder_delivery_rate.py` -- builder on-time delivery scoring
- `fetch_market_pricing.py` -- area-level pricing data

## Key metrics

| Metric | Sprint 2 End | Sprint 3 End | Delta |
|--------|-------------|-------------|-------|
| Rust backend tests | 10 | 65 | +55 |
| Knowledge graph societies | ~12 | 70 | +58 |
| Knowledge graph builders | 0 | 44 | +44 |
| Knowledge graph areas | ~5 | 19 | +14 |
| Seed properties | ~20 | 155 | +135 |
| Frontend trust components | 0 | 6 | +6 |
| Sprint commits | 3 batched | 4 | -- |
| Files changed (data commit) | -- | 269 (+40,050 lines) | -- |

## Architecture decisions

**1. fact_source_quality over graph_driven_pct for detail pages**
The search route uses `graph_driven_pct` (how much of the score came from graph vs legacy fallback). But on a detail page there is no search score -- so we compute quality from the average confidence of the society's facts. This separates "search relevance" from "data quality."

**2. Freshness capping for bulk-created nodes**
A node with 50 facts all sharing the same `learned_at` timestamp is freshly seeded, not freshly enriched. Capping freshness at 0.5 for these nodes prevents seed data from inflating confidence. After real enrichment adds facts with different timestamps, the cap lifts.

**3. Self-describing SourcedFacts**
Every fact carries `display_template`, `answers_preferences`, and `scoring_hint`. Adding a new knowledge dimension (e.g., "ev_charging") requires zero Rust code changes -- just a skill that produces the fact. This was validated across trust badges, confidence scoring, and graph-driven search.

**4. Canonical builder pattern**
Instead of merging duplicate builder nodes (destructive), orphan builders get a `canonical_builder` fact pointing to the canonical node. Query-time resolution follows the pointer. This preserves provenance while deduplicating at read time.

**5. Per-entity JSON files**
Each knowledge graph node is a separate JSON file at `data/knowledge/nodes/{type}/{slug}.json`. Adding a fact to one society does not rewrite the entire graph. This layout maps directly to S3 prefixes.

## What worked

- **RERA as government-truth anchor** -- having a verifiable external source transformed the credibility of every downstream feature. Trust badges backed by RERA registration IDs are fundamentally different from trust badges backed by scraped data.
- **Trust badges as visible transparency** -- the 6 trust components make the transparency promise tangible. Users can see confidence levels, data sources, and freshness at a glance.
- **Test growth from 10 to 65** -- Sprint 3 added 55 tests covering graph search, confidence scoring, intent parsing, builder resolution, and area matching. This is the first sprint where the test suite became a real safety net.
- **Self-describing facts proved out** -- the pattern of facts declaring their own display, preference matching, and scoring worked across multiple features without hardcoded Rust match arms.

## What to improve

- **Commit cadence** -- Sprint 3 had only 4 commits for 14 days of work. Large batched commits make it harder to bisect, review, and recover. Sprint 4 should target daily or every-2-day commits.
- **Integration tests** -- all 65 tests are unit tests. There are no end-to-end tests that verify a search query flows through intent parsing, graph scoring, and confidence calculation as a complete pipeline.
- **Render verification** -- trust badges were built and tested in isolation but never verified rendering in a live browser during the sprint. The frontend build passes but visual correctness is unverified.
- **Frontend test coverage** -- still zero frontend tests. Trust components (ConfidenceMeter, TrustBadge, BuilderTrustBadge) are good candidates for component tests.
- **Data freshness monitoring** -- no automated check for stale knowledge graph data. If RERA data changes, we have no detection mechanism.

## Sprint 4 handoff

**Clean state:**
- All code committed, builds pass, 65 tests pass
- Knowledge graph populated: 70 societies, 44 builders, 19 areas, 155 properties
- Trust model complete: confidence scoring, trust badges, data provenance
- Graph-driven search operational with self-describing fact scoring

**Known gaps for Sprint 4:**
- Search quality validation: no user-facing A/B or quality metrics
- Embedding-based semantic search: wired but not integrated with graph scoring
- Society detail pages: data is richer but display is basic
- Pipeline scheduling: skills run manually, no automated enrichment cadence
- Frontend tests: zero coverage
