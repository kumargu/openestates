# OpenEstates — 100-Day Vision

## Mission

Transform OpenEstates from a prototype into a production-ready, trust-first property platform where buyers discover confidently, sellers list transparently, and the system gets smarter with every interaction.

## Core Thesis

**Discovery brings traffic. Connection is the revenue event. Reduce friction to zero.**

The product earns trust by being transparent — every ranking is explainable, every fact is sourced, every score is traceable. Users stay because the system genuinely helps them make better decisions.

---

## Current State (Day 30)

| Metric | Value |
|--------|-------|
| Knowledge nodes | 229 (126 properties, 55 societies, 16 areas, 32 builders) |
| API endpoints | 32 |
| Frontend pages | 5 (Home, Results, Property, Society, Shortlist) |
| Intelligence skills | 3 (score_society, identify_gaps, rank_for_intent) |
| Images | 240 across 48 societies, 135 properties |
| Stack | Rust+Axum backend, React frontend, Python pipeline |

**What works:** NL search, property discovery, knowledge graph, live discovery via Gemini, scoring with explanations, shortlist + compare.

**What's missing:** Seller side, marketplace flow, pipeline resilience, mobile polish, CI/CD, SEO.

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
- **Pipeline basics** — retry with backoff for skills, basic checkpoint/resume
- **Property page conviction** — strengthen transparency widgets, market context, comparison prompts

### Not in this sprint

- Full seller registration journey (Sprint 2)
- Bid/negotiation flows (Sprint 3+)
- Performance optimization (Sprint 4)

---

## Sprint 2: Seller-Buyer Connection (Days 45–58)

**Connect buyers and sellers. Every click matters.**

With a solid buyer experience, now build the supply side.

### Seller Registration (Hinge-style journey)

- 7 steps: basic info → property prompt → details → pricing → documents → photos → society
- Skippable and resumable at any time
- Profile completeness shown as percentage (like Hinge profile strength)
- "Verified" badge for sellers with Khata + EC + RERA
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

## Sprint 3: Search Intelligence & Marketplace (Days 59–72)

**Search that genuinely helps. The marketplace starts converting.**

### Search Improvements

- Complex multi-preference intent parsing
- Better scoring: weight recent facts higher, penalize stale data
- Confidence meter on search results
- Prompt-based matching: buyer queries matched against seller property prompts
- "Why this matches" explanations that reference seller prompts
- Property Watch — persistent search agents (Yutori Scouts concept)

### Marketplace

- Buyer interest analytics: who expressed interest, response rates
- Connection notifications (stored events, not push)
- Property listing quality scoring
- Search analytics: what users search, what they click

### Transparency

- Fact provenance UI (click any score → see sources)
- Data freshness indicators
- Price history visualization
- Trust badges system (verified vs unverified data)

---

## Sprint 4: Performance & Data Expansion (Days 73–86)

**Fast now, ready for 10x. More data, better data.**

### Performance

- Profile and optimize Rust hot paths
- Proper caching strategy (LRU + TTL)
- Pre-compute popular search results
- Optimize frontend bundle size
- Target: search < 50ms warm, < 200ms cold
- Target: frontend loads < 2s on 3G

### Data Expansion

- Expand to 500+ societies in Bangalore
- Add new data sources (Google Maps, 99acres, MagicBricks)
- Image quality assessment and curation
- Automated data quality scoring
- Stale data re-enrichment scheduler

---

## Sprint 5: Launch-Ready Polish (Days 87–100)

**Ship-quality product.**

- End-to-end journey tests (buyer and seller flows)
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
- **Transparency is the product**: Every ranking explainable, every fact sourced.
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
