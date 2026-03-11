# OpenEstates 100-Day Plan

## Vision
Transform OpenEstates from a prototype into a production-ready, trust-first property platform where sellers list transparently, buyers discover confidently, and data enrichment never stops.

---

## Sprint 1 (Days 1-10): Seller-Buyer Flow Polish & Pipeline Hardening
**Theme: Make what exists actually work end-to-end**

### Goals
- [ ] Audit and fix seller registration → listing → bid flow (clicks must work)
- [ ] Fix bid acceptance/rejection → listing status update flow
- [ ] Harden pipeline with checkpoint/resume, never-crash error handling
- [ ] Remove UI clutter — audit every component for value
- [ ] Add frontend tests (build check + type safety)

### Deliverables
- Seller can register, list, see bids, accept/reject — zero dead ends
- Buyer can search, view, bid — zero broken clicks
- Pipeline runs continuously with checkpoint recovery
- UI audit doc: what stays, what goes

---

## Sprint 2 (Days 11-20): Data Pipeline Robustness & Always-On Crawling
**Theme: The pipeline never stops, never crashes, always moves forward**

### Goals
- [ ] Implement robust retry + exponential backoff for all skills
- [ ] Add checkpoint/resume to enrichment engine
- [ ] Background crawling scheduler (slow, respectful, non-stop)
- [ ] Pipeline health dashboard (what ran, what failed, what's stale)
- [ ] Fix all known pipeline failure modes

### Deliverables
- `python3 -m pipeline.enrich --continuous` runs indefinitely
- Failures logged, checkpointed, retried automatically
- New data appears in knowledge graph without manual intervention

---

## Sprint 3 (Days 21-30): Data Model Review & Storage Alignment
**Theme: Clean data model for seller+buyer world, storage that scales**

### Goals
- [ ] Review data model: Property ↔ Seller ↔ Bid ↔ Society relationships
- [ ] Align disk storage, in-memory cache, and S3-ready layout
- [ ] Ensure marketplace data (sellers, bids) follows same atomic write pattern
- [ ] Add data integrity checks at startup
- [ ] Consider SQLite for marketplace state (append-only bids)

### Deliverables
- Data model diagram (entities, relationships, storage locations)
- Storage migration plan if needed
- Zero data corruption scenarios

---

## Sprint 4 (Days 31-40): UI/UX Overhaul — Inspired by Modern AI Platforms
**Theme: Calm, premium, transparent — like Yutori meets Robinhood**

### Goals
- [ ] Redesign search results with better information hierarchy
- [ ] Add "Property Watch" concept (persistent search agents, inspired by Yutori Scouts)
- [ ] Clean typography hierarchy (max 3 sizes per view)
- [ ] Remove decorative elements, keep functional ones
- [ ] Mobile-first responsive pass on all pages

### Deliverables
- Search results feel premium and explainable
- Property detail page tells a story, not just data
- Mobile experience is first-class

---

## Sprint 5 (Days 41-50): Search Intelligence & Ranking Quality
**Theme: Search results that genuinely help users decide**

### Goals
- [ ] Improve intent parsing (handle complex multi-preference queries)
- [ ] Better scoring: weight recent facts higher, penalize stale data
- [ ] Add "confidence meter" to search results
- [ ] Search analytics: track what users search, what they click
- [ ] A/B test different ranking strategies

### Deliverables
- Search accuracy measurably improved
- Users understand WHY results are ranked the way they are

---

## Sprint 6 (Days 51-60): Transparency Deep Dive
**Theme: Every number is traceable, every score is explainable**

### Goals
- [ ] Add fact provenance UI (click any score → see sources)
- [ ] "Data freshness" indicators on every property
- [ ] Price history visualization
- [ ] Society comparison tool improvements
- [ ] Trust badges system (verified vs unverified data)

### Deliverables
- Users can trace any claim to its source
- Stale data is visually distinct from fresh data

---

## Sprint 7 (Days 61-70): Marketplace Maturity
**Theme: Real transactions, real trust**

### Goals
- [ ] Seller verification flow (RERA linking, identity)
- [ ] Bid negotiation (counter-offers)
- [ ] Property listing quality scoring
- [ ] Notification system (bid received, accepted, new match)
- [ ] Listing expiry and renewal

### Deliverables
- Marketplace feels like a real transaction platform
- Sellers get value from listing
- Buyers get value from bidding

---

## Sprint 8 (Days 71-80): Performance & Scale Prep
**Theme: Fast now, ready for 10x**

### Goals
- [ ] Profile and optimize Rust hot paths
- [ ] Implement proper caching strategy (LRU + TTL)
- [ ] Pre-compute popular search results
- [ ] Optimize frontend bundle size
- [ ] Load testing with realistic data volumes

### Deliverables
- Search < 50ms warm, < 200ms cold consistently
- Frontend loads in < 2s on 3G
- System handles 1000 concurrent users

---

## Sprint 9 (Days 81-90): Data Quality & Coverage
**Theme: More data, better data, fresher data**

### Goals
- [ ] Expand to 500+ societies in Bangalore
- [ ] Add new data sources (Google Maps, 99acres, MagicBricks)
- [ ] Image quality assessment and curation
- [ ] Automated data quality scoring
- [ ] Stale data re-enrichment scheduler

### Deliverables
- 500+ societies with rich facts
- Every society has at least 5 fact dimensions
- Data freshness < 30 days for active societies

---

## Sprint 10 (Days 91-100): Polish, Test, Launch-Ready
**Theme: Ship-quality product**

### Goals
- [ ] End-to-end journey tests (buyer and seller flows)
- [ ] Error handling audit (no unhandled errors anywhere)
- [ ] SEO and meta tags
- [ ] Analytics integration
- [ ] Landing page that converts
- [ ] Documentation for deployment

### Deliverables
- Product is demo-ready for investors
- Zero known bugs in core flows
- One-command deployment

---

## Current Status
- **Sprint 1**: IN PROGRESS
- **Knowledge nodes**: 229 (126 properties, 55 societies, 16 areas, 32 builders)
- **API endpoints**: 29
- **Frontend pages**: 6
