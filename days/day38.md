# Day 38: Property Page Conviction — Transparency Score, Price Gauge, Social Sharing

## Goal
Add conviction features: composite Transparency Score, enhanced price position gauge, and social sharing buttons.

## Product Reason
Scattered scores exist but no single trust indicator. Buyers need a glanceable "how transparent is this listing?" signal, clear price positioning, and ability to share with family/friends.

## Deliverables

### 1. Transparency Score (sidebar top)
- Composite 0-100 score from: document_completeness (30%), society_quality (25%), builder_quality (25%), RERA status (20%)
- Visual: circular progress indicator, color-coded
- 4 contributing factors as mini progress bars
- Backend: new `scoring/transparency.rs` with compute function, add to PropertyDetail response

### 2. Enhanced Price Position Widget (sidebar)
- Replace thin bar with spectrum visualization
- Area price range (min-max) with median and property markers
- Color gradient green→amber→red
- One-line value verdict: "12% below median — strong value"
- Backend: add area_price_range_low/high to response

### 3. Social Sharing Buttons
- Copy Link, WhatsApp, X/Twitter
- Below Save button in sidebar
- Pure frontend — navigator.clipboard + URL schemes
- New component: `ShareButtons.tsx`

### 4. Minor Polish
- Reorder RERA tile below "Why this property" section

## Constraints
- No new dependencies (npm or Cargo)
- Transparency score deterministic from existing data (no LLM)
- Share URLs use canonical https://openestates.in/property/{id}
- Mobile responsive

## Success Criteria
1. Transparency Score visible in sidebar with breakdown
2. Price gauge shows spectrum with markers
3. Share buttons work (copy, WhatsApp, X)
4. All responsive at 375px
5. cargo check + npm run build pass
