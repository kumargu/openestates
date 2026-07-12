# OpenEstates — Vision

## Mission

Transform OpenEstates from a prototype into a production-ready, trust-first property platform where buyers discover confidently, sellers list transparently, and the system gets smarter with every interaction.

## Core Thesis

**Discovery brings traffic. Connection is the revenue event. Reduce friction to zero.**

The product earns trust by being transparent — every ranking is explainable, every fact is sourced, every score is traceable. Users stay because the system genuinely helps them make better decisions.

## Data Thesis (added Day 49)

**RERA is the root of trust. Everything else is enrichment.**

The knowledge graph is rooted in government-verified RERA data (confidence: 1.0). Every society, builder, and project starts as a RERA node with legal proof of existence. Skills enrich these nodes with Reddit sentiment, Google reviews, images, and LLM-scored dimensions. Seller inventory that isn't RERA-registered still enters the graph — same enrichment, lower trust floor, clearly labeled.

Two roots, explicit trust:
- **RERA root** → government-verified, trust badge, full enrichment chain
- **Seller root** → self-reported, "Unverified" tag, enriched but no legal proof

---

## Current State (Day 49)

| Metric | Value |
|--------|-------|
| Knowledge nodes | 229 (126 properties, 55 societies, 16 areas, 32 builders) |
| API endpoints | 32+ |
| Frontend pages | 7 (Home, Results, Property, Society, Shortlist, Seller Dashboard, Registration) |
| Intelligence skills | 8 (search_reddit, fetch_rera, fetch_google_reviews, learn_society, fetch_images, score_society, embed_entity, identify_gaps) |
| Images | 240 across 48 societies, 135 properties |
| Stack | Rust+Axum backend, React frontend, Python pipeline |
| Sellers | 5 seed sellers, registration flow in progress |

**What works:** NL search, local knowledge graph search, scoring with explanations, shortlist + decision sheet, seller dashboard, interest flow.

**What's missing:** RERA-rooted data foundation, trust badges, marketplace analytics, performance, CI/CD, SEO.

---

## Sprint 1: Buyer Experience & Foundation (Days 31–44)

**Make the discovery experience shareable, trustworthy, and mobile-ready.**

Before building the seller side, the buyer experience must be strong enough to generate demand. No one lists on an empty platform.

### Deliverables

- **Mobile-first responsive pass** — all 5 pages work well on mobile (property search is overwhelmingly mobile in India)
- **Shareable property pages** — OG meta tags, clean URLs, social preview cards
- **SEO basics** — meta tags, structured data for properties, sitemap
- **Click flow audit** — zero dead ends, every element leads somewhere useful
- **UI cleanup** — remove clutter that competes with primary actions
- **Lightweight seller claim** — "Is this your property? Claim it" on property pages (interest capture, not full registration)
- **CI gate** — `cargo check` + `npm run build` must pass on every change
- **Property page conviction** — strengthen transparency widgets, market context, comparison prompts

### Not in this sprint

- Full seller registration journey (Sprint 2)
- RERA data foundation (Sprint 3)
- Performance optimization (Sprint 5)

---

## Sprint 2: Seller-Buyer Connection (Days 45–58)

**Connect buyers and sellers. Every click matters.**

With a solid buyer experience, now build the supply side.

### Seller Registration (Hinge-style journey)

- 7 steps: basic info → property prompt → details → pricing → documents → photos → society
- Skippable and resumable at any time
- Profile completeness shown as percentage (like Hinge profile strength)
- Higher-completeness profiles rank higher in search results

### Property Prompts (the matching signal)

- Sellers write NL descriptions: "East facing corner flat with sunrise views", "Walking distance to Prestige Forum mall"
- Prompts are matched against buyer search queries
- Shown prominently on property cards and detail pages

### Connection Flow

- 1-click "I'm Interested" on property cards and detail pages
- No unnecessary intermediate pages between discovery and connection
- Interest stored as events (no chat/messaging yet)
- Seller dashboard: see listings, buyer interest, resume incomplete profiles

### Trust & Verification

- Document verification status display
- Progressive trust indicators (what's provided vs what's missing)
- Animated seller landing page with value propositions

### Data

- Dummy seller data: 10 sellers with varying completeness (30%–100%) in `data/sellers/`
- Sellers linked to existing properties via `seller_id`

---

## Sprint 3: RERA Data Foundation & Trust Model (Days 59–72)

**Root the graph in government truth. Make trust visible.**

Strategic reprioritization (Day 49): The original Sprint 3 focused on search intelligence, but better search requires better data. RERA gives us 9,469 verified projects for free — this is the foundation that makes everything after it more powerful.

### RERA-Rooted Seeding

- **`seed_from_rera` skill** — scrape Karnataka RERA registry, seed 50-100 societies spread across Bengaluru (East, South, North, West corridors)
- Prove the pipeline end-to-end on a small set: RERA scrape → KG nodes → enrichment → search → trust badges
- Each project becomes a society node with confidence 1.0 facts: registration number, builder, units, completion date, litigation status, complaints, escrow
- Builder nodes extracted and deduplicated (cross-reference all projects per promoter)
- Area nodes linked from RERA district/taluk data
- Edges: society→builder (BUILT_BY), society→area (LOCATED_IN), builder→society (HAS_PROJECT)
- `pipeline/seed.py` entry point — runs seeding before enrichment

### RERA Date-Based Property Classification (Skill-Driven, Not Filters)

RERA gives us `start_date`, `completion_date`, and `original_completion_date` for every project. The `seed_from_rera` skill computes these classifications and stores them as **self-describing SourcedFacts** — they are NOT UI filter dropdowns. The search bar is the only interface. Natural text drives discovery.

The skill produces these facts per society node:

```
SourcedFact {
  key: "project_status",
  value: {type: "Text", data: "ready_to_move"},
  confidence: 1.0,
  source: {type: "Rera", skill_id: "seed_from_rera"},
  display_template: "Ready to Move — delivered {completion_date}",
  answers_preferences: [
    "ready to move", "ready possession", "completed project",
    "move in now", "immediate possession", "ready flat"
  ],
  scoring_hint: {direction: "TextMatch", weight: 3.0}
}
```

Classification logic (computed from RERA dates):
- **ready_to_move** → `completion_date` in the past, project delivered
- **under_construction** → `completion_date` in the future, on track
- **new_launch** → `start_date` within last 12 months
- **delayed** → `completion_date` pushed past `original_completion_date` (transparency signal)
- **upcoming** → `start_date` in the future (pre-launch)

Similarly for builder delivery track record:

```
SourcedFact {
  key: "builder_delivery_rate",
  value: {type: "Numeric", data: 0.75},
  confidence: 1.0,
  source: {type: "Rera", skill_id: "seed_from_rera"},
  display_template: "Builder delivers on time: {value}% of projects",
  answers_preferences: [
    "reliable builder", "on time delivery", "trusted builder",
    "good builder", "no delays"
  ],
  scoring_hint: {direction: "HigherIsBetter", weight: 2.5}
}
```

**How search uses this (no filters, just natural text):**

| User types | Intent extracted | Facts matched via `answers_preferences` |
|---|---|---|
| "ready to move 3BHK Whitefield" | area: Whitefield, prefs: [ready to move, 3bhk] | `project_status.answers_preferences` contains "ready to move" → boost |
| "new launch near Sarjapur" | area: Sarjapur, prefs: [new launch] | `project_status` = "new_launch" matched |
| "reliable builder Whitefield" | area: Whitefield, prefs: [reliable builder] | `builder_delivery_rate.answers_preferences` contains "reliable builder" → rank by rate |
| "no delays safe investment" | prefs: [no delays, safe] | `builder_delivery_rate` high + `project_status` != "delayed" → boost |

This is the core product principle: **minimal filters, maximum intelligence.** The search bar understands intent. Skills produce facts that declare what they answer. The graph matches them. Zero hardcoded filter logic.

### `root_source` on All Nodes

- Every KG node gets a `root_source` field: `"rera"`, `"seller"`, or `"discovered"`
- RERA-rooted nodes: government-verified, full trust chain
- Seller-rooted nodes: self-reported, enrichable but no legal proof
- Discovered nodes from offline crawlers/enrichment: verification pending, queued for RERA cross-check

### Trust Badges UI

- `✓` RERA Verified (confidence 1.0, government source)
- `●` Skill-enriched (confidence 0.5-0.9, Reddit/Google/LLM)
- `○` Self-reported (confidence 0.3-0.5, seller-claimed)
- `⚠` System advisory (verification pending, stale data)
- Badges shown on property cards, detail pages, search results
- Data freshness indicators — when was this last enriched?
- Confidence meter on search results

### Data Impact

- From ~50 hand-curated societies → 50-100 RERA-verified societies (Sprint 3), expanding to 300+ in Sprint 4
- Prove the full pipeline: RERA seed → skill enrichment → graph-first scoring → trust badges in UI
- Builder graph with cross-project trust signals (12 projects, 0 revocations = strong signal)
- Every existing skill (search_reddit, learn_society, score_society, etc.) works unchanged on RERA-seeded nodes
- RERA dates → skill-produced facts with `answers_preferences` — search understands "ready to move", "new launch", "reliable builder" naturally, no filter UI needed
- Builder delivery track record as a SourcedFact — "reliable builder" in search matches builders with high on-time delivery rates

### Not in this sprint

- Multi-preference search parsing (Sprint 4)
- Property Watch / Scouts (Sprint 4)
- Performance optimization (Sprint 5)

---

## Sprint 4: Data Cleanup & RERA Expansion (Days 73–86)

**Clean house. Validate end-to-end. Scale to 100 properties.**

Sprint 3 proved the RERA pipeline on 50-100 societies. Sprint 4 validates the full stack on 10 properties first (enrichment, embeddings, search, seller matching), then scales to 100 RERA-rooted societies with rich data.

### Phase 0: Validation Gate (Days 73–75)

Before scaling, prove every layer works on 10 hand-picked properties:

**Enrichment verification (all 10):**
- RERA facts present (registration, builder, units, dates, project_status)
- Market pricing facts present (per-BHK configs, price/sqft, appreciation)
- Reddit/Google review facts present (where available)
- Images fetched and linked
- Trust badges rendering correctly (RERA verified vs enriched vs pending)
- Fact provenance chain intact (every fact → source → skill_id → timestamp)

**Embeddings & search (all 10):**
- Entity embeddings generated and stored
- NL search returns these properties for relevant queries
- Search ranking uses graph-driven scoring (not legacy fallback)
- "Why this matches" explanations reference real facts

**Seller matching test (3–4 of the 10):**
- Create/assign 3-4 dummy sellers to random properties from the 10
- Seller property prompt matches against buyer search queries
- Interest flow works: buyer → "I'm Interested" → seller dashboard shows it
- Seller dashboard displays linked property with trust badges + enrichment data
- Matching algorithm correctly ranks seller-listed properties alongside RERA-rooted ones

**Gate criteria:** All 10 properties pass enrichment checks, search returns them correctly, and seller matching works for the 3-4 test cases. Only then proceed to Phase 1.

### Phase 1: Data Cleanup (Days 76–78)

- **Remove hand-curated seed data** — `data/seed/properties.json`, `data/seed/societies.json` become RERA-generated, not manually written
- **Migrate existing 48 societies** — match to RERA entries where possible, tag unmatched as `root_source: "legacy"`
- **Delete legacy scoring fallback** — if `graph_driven_pct` is 95%+, remove hardcoded preference maps from `search.rs` and `text.rs`
- **Remove seed-JSON bootstrap** — backend loads KG directly, no more `bootstrap_from_seed()`
- **Audit knowledge graph** — remove stale/duplicate facts, re-run skills on RERA-rooted nodes

### Phase 2: Scale to 100 (Days 79–83)

- Expand RERA seeding to **100 societies** across Bengaluru (not 300 — keep it tight, find data quality issues early)
- Run full enrichment pipeline on all 100: RERA → market pricing → Reddit → images → embeddings
- Automated data quality scoring — flag nodes with low fact counts or stale enrichment
- Verify local search works against RERA-rooted data and queues enrichment gaps instead of making request-time LLM calls

### Phase 3: Market Pricing Enrichment (Days 84–86)

- **`fetch_market_pricing` skill** — for each RERA-rooted society, query Gemini with grounded search for per-BHK pricing
- Pass RERA context (builder, units, completion date, project type) to Gemini for more accurate results
- Facts produced per society: `pricing_{bhk}` (sqft range, price range, price/sqft), `price_per_sqft`, `configurations`, `market_status`, `price_appreciation`, `comparable_projects`, `pricing_insight`
- Each pricing fact has `answers_preferences` for search matching (e.g., "3bhk price", "under 2 crore", "affordable")
- Comparable projects output feeds back into discovery — tells us what to seed next
- Cached 14 days (market prices change); re-enrichable on demand

### Not in this sprint

- Search intelligence improvements (Sprint 5)
- Seller→Society matching at scale (Sprint 5) — only 3-4 test cases in Phase 0
- Performance optimization (Sprint 6)

---

## Sprint 5: Search Intelligence & Marketplace (Days 87–100)

**Make search genuinely smart. Connect the dots between buyers and sellers.**

### Seller→Society Matching (needs Sprint 2 + Sprint 3 complete)

- When a seller registers a property, fuzzy-match their society name to RERA-seeded nodes
- **Match found** → property inherits society's RERA trust, badge: "RERA-Verified Society"
- **No match** → standalone property node, badge: "Seller-Listed · Not RERA Verified"
- "Verified" badge for sellers with Khata + EC + RERA documents
- Same graph, same skills, different trust floor

### Transparency Surfaces

- Fact provenance UI — click any score → see sources, skill, timestamp
- Builder trust page — all RERA projects, revocations, complaints across projects
- Builder profile pages powered by RERA cross-references (delivery track record)

### Search Intelligence

- Complex multi-preference intent parsing
- Better scoring: weight RERA-verified facts higher, penalize stale/unverified data
- Prompt-based matching: buyer queries matched against seller property prompts
- "Why this matches" explanations that reference seller prompts + RERA data
- Property Watch — persistent search agents (Yutori Scouts concept)

### Marketplace

- Buyer interest analytics: who expressed interest, response rates
- Connection notifications (stored events, not push)
- Property listing quality scoring (completeness + verification = higher rank)
- Search analytics: what users search, what they click

---

## Sprint 6: Performance & Scale (Days 101–114)

**Fast now, ready for 10x.**

### Performance

- Profile and optimize Rust hot paths (2,000+ societies in memory)
- Proper caching strategy (LRU + TTL)
- Pre-compute popular search results
- Optimize frontend bundle size
- Target: search < 50ms warm, < 200ms cold
- Target: frontend loads < 2s on 3G

### Scale

- Image quality assessment and curation
- Pre-warm enrichment for high-traffic areas

---

## Sprint 7: Launch-Ready Polish (Days 115–128)

**Ship-quality product.**

- End-to-end journey tests (buyer and seller flows, RERA-rooted and seller-rooted paths)
- Error handling audit
- Analytics integration (basic event tracking)
- Landing page that converts
- Society comparison tool improvements
- Clean typography hierarchy (max 3 sizes per view)
- Remove decorative elements, keep functional ones
- Documentation for deployment
- Zero known bugs, one-command deployment

---

## Design Principles

- **Theme**: Calm, premium, transparent. Don't redesign — evolve.
- **Mobile-first**: Property search in India is overwhelmingly mobile.
- **Inspiration**: Hinge (journey, matching), Robinhood (clean data), Yutori (scouts).
- **AI is supportive**: Intent extraction, summarization, explanation. Not the center.
- **Skills are the intelligence layer**: LLM judgment packaged as typed, cacheable, auditable units. Adding a new knowledge dimension = writing a new skill, zero Rust changes.
- **RERA is the root of trust**: Government data seeds the graph. Everything else enriches it. Seller data without RERA backing gets lower confidence, clearly labeled.
- **Transparency is the product**: Every ranking explainable, every fact sourced, every trust level visible.
- **Context > Filters**: NL search, soft preferences, tradeoff sensitivity.
- **Demand before supply**: Make the buyer experience excellent before building seller tools.

## Architecture

| Layer | Tech |
|-------|------|
| Frontend | React (Vite, port 5173) |
| Backend | Rust + Axum (port 4000) |
| Pipeline | Python (data collection, enrichment, AI skills) |
| Storage | S3-ready local FS → `data/` |
| Knowledge Graph | Per-entity JSON files, self-describing SourcedFacts |
| Skills | Python modules producing SourcedFacts (the intelligence layer) |

### Knowledge Graph Update Model (redefined Day 49)

The KG must support the full lifecycle: skills create nodes, add facts, update facts, and add edges. Not just append facts to pre-existing nodes.

**Node lifecycle:**

```
  CREATE (new in Sprint 3)
  ─────────────────────────
  Skills can create nodes via API. Every node has:
    - id: "{type}:{slug}"
    - node_type: Society | Builder | Area | Property
    - root_source: "rera" | "seller" | "discovered"   ← NEW
    - name: display name
    - facts: []  (empty, skills fill these)
    - created_at, updated_at

  API: POST /api/knowledge/nodes
  Body: { id, node_type, name, root_source }

  Python: graph_client.create_node(id, node_type, name, root_source)
```

**Fact updates (upsert, not just append):**

```
  CURRENT (broken):
    node.facts.push(fact)    ← duplicates accumulate
    get_fact(key) → highest version  ← works but wastes disk

  NEW (upsert by key):
    If fact with same key exists:
      Replace it (keep old in history if version > 1)
      Increment version automatically
    Else:
      Append new fact

  API: POST /api/knowledge/nodes/{id}/facts  (same endpoint, new behavior)

  Rule: One active fact per key per node.
  History: Available via GET /api/knowledge/nodes/{id}/facts/{key}/history
```

**Edge management:**

```
  CURRENT: edges created at bootstrap only, stored in one edges.json

  NEW: Skills can add edges via API
  API: POST /api/knowledge/edges
  Body: { from_id, to_id, relation, source }

  Deduplicated: same from+to+relation = skip (idempotent)

  Python: graph_client.add_edge(from_id, to_id, relation)
```

**Startup (no more seed-JSON bootstrap):**

```
  CURRENT:
    1. Load seed JSON → bootstrap nodes if no KG exists
    2. Load KG from disk

  NEW:
    1. Load KG from data/knowledge/nodes/**/*.json  (always)
    2. If empty → run pipeline/seed.py (RERA seeding)
    3. No hand-curated JSON as source of truth

  The KG IS the source of truth. Seed JSON becomes a migration artifact.
```

**Full API surface (updated):**

```
  Nodes:
    POST   /api/knowledge/nodes              ← CREATE node (new)
    GET    /api/knowledge/nodes?type=society  ← list
    GET    /api/knowledge/nodes/{id}          ← detail
    DELETE /api/knowledge/nodes/{id}          ← remove (new, admin only)

  Facts:
    POST   /api/knowledge/nodes/{id}/facts          ← upsert facts (changed)
    GET    /api/knowledge/nodes/{id}/facts/{key}/history  ← version history (new)

  Edges:
    POST   /api/knowledge/edges              ← create edge (new)
    GET    /api/knowledge/nodes/{id}/neighbors
    GET    /api/knowledge/path?from=...&to=...

  Graph:
    GET    /api/knowledge/stats
    GET    /api/knowledge/coverage?type=society
    GET    /api/knowledge/enrichment/queue
    GET    /api/knowledge/search-log
```

**Skill → Graph flow (updated):**

```
  Python skill produces SkillResult:
    new_nodes:  [{ id, type, name, root_source }]   ← CREATE nodes
    facts:      [SourcedFact, ...]                   ← UPSERT to node
    edges:      [{ from, to, relation }]             ← ADD edges

  graph_client.push_skill_result(node_id, result):
    1. For each new_node → POST /api/knowledge/nodes
    2. For each fact    → POST /api/knowledge/nodes/{id}/facts  (upsert)
    3. For each edge    → POST /api/knowledge/edges

  Idempotent: re-running a skill with same input produces same graph state
```

### Key Paths

- Seller data: `data/sellers/{seller_id}.json`
- Properties get `seller_id` field linking to seller
- Connection/interest events: `data/interests/{buyer-seller-property}.json`
- Profile completeness: computed from filled fields, not stored
- New Rust types: `backend/src/models/`
- New React pages: `frontend/src/pages/`
- Seller components: `frontend/src/components/seller/`

## What We Don't Build (yet)

- Payment flows
- Chat/messaging between buyer-seller
- Full document verification (just track uploads)
- Push notifications (just store events)
- Bid/counter-offer negotiation engine
- Heavy agent orchestration
