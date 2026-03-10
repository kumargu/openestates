# days/day18.md

# OpenEstates v2

## Day 18 – Rethink the Journey: Fewer Clicks, Stronger Results, and a Shortlist-First Flow

## 1. Goal

Redesign the browse-to-decision journey so users can move from discovery to shortlist with fewer clicks and more confidence.

Target flow: search → scan results → shortlist directly → compare → deep dive only when needed.

## 2. Product Reason

Too much importance placed on the dedicated property page too early. The better page-role split:
- **Homepage** = intent capture and trust
- **Results** = shortlist decision surface (enough info to save directly)
- **Shortlist** = comparison and conviction workspace
- **Property page** = optional deep dive

## 3. Deliverables

### 3.1 Results cards become shortlist-decision surface
Each card shows: real image, title, society+area, price, price/sqft, BHK+sqft, match label, one-line "why this property", 2-3 tags, compact signals, Save CTA, Quick view CTA.

### 3.2 Real property images on results
Replace placeholders with real images. Stable aspect ratio, graceful fallback.

### 3.3 Quick-view or inline expansion on results
Expandable card / drawer / modal showing: larger image, price vs median, tradeoffs, society summary, area summary, shortlist action, optional full-page link.

### 3.4 Property page becomes optional, not mandatory
Keep /property/:id for deep-link and deep-dive. Stop relying on it as main step before shortlisting.

### 3.5 Shortlist becomes enriched comparison workspace
Show more context per shortlisted property: stronger price context, key tradeoffs, society/area summary, risk cues, market activity.

### 3.6 Strengthen results action hierarchy
Priority: Save to shortlist > Quick view > Open full page.

### 3.7 Reduce click cost
Landing→results: 1 action. Results→save: 1 action. Results→richer detail: 1 expand (no route change).

### 3.8 Journey rethink note
`docs/day18_journey_rethink_note.md`

## 4. Technical Guidance

- Results card two-layer: Layer A (visible) + Layer B (quick-view expansion)
- Quick-view: prefer inline expansion or side drawer
- Add match_label, why_this_property, quick_signals, quick_view fields to PropertyCard
- Save state must feel immediate
- Shortlist shows more detail than results

## 5. Constraints

Do NOT build: ranking engine rewrite, auth, server-side shortlist, map view, AI chat, broad visual rebrand.

## 6. Success Criteria

- Results cards show real images
- Results cards support direct shortlisting
- Quick-view works without route change
- Shortlist is visibly more enriched than results
- Property page still works but isn't required
- Journey feels like: search → shortlist → compare
