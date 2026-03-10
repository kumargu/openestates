# days/day17.md

# OpenEstates v2

## Day 17 – Homepage Sponsored Launches Module with Premium Placement, Real Images, and Clean Builder Discovery

## 1. Goal

Add a premium homepage sponsored-launches module that showcases a small number of curated upcoming builder launches using real images and structured project metadata, while preserving the calm, sleek OpenEstates brand.

By the end of Day 17:
- A new homepage section for Upcoming Launches
- Placed below hero/search, above area cards
- 2–4 curated launch cards using real project images
- Initial dummy launch records sourced from Prestige and Godrej reference pages
- Clear but subtle sponsored labeling
- Liquid-glass / transparent visual treatment that feels premium
- Mobile-friendly layout
- Documentation for monetization surface evolution

## 2. Product Reason

OpenEstates will need monetization surfaces later. Display ads in real estate are dangerous — they make products feel like cluttered classifieds. Day 17 answers: **Can we create a revenue surface that still looks elegant, useful, and native?**

The answer is yes only if:
- The section is clearly curated
- Placed below hero, not above
- Uses real images
- Visually restrained
- Secondary to search
- Helps discovery rather than interrupting it

### Homepage order (locked):
```
Header / nav
Hero with natural-language search
Upcoming Launches (sponsored module)   ← NEW
Area cards / Explore Bengaluru
Transparency explainer blocks
Browse properties CTA
```

## 3. Deliverables

### 3.1 Upcoming Launches homepage section
- Titled "Upcoming launches" with subtitle like "Selected new projects from trusted builders"
- 2–4 cards on desktop, 1–2 visible on mobile
- Must feel editorial and curated, not ad-like

### 3.2 Homepage placement
- Below hero/search, above area cards
- Must not overpower hero headline or search

### 3.3 Real images for launch cards
- Use real project images, not placeholders
- Store in `frontend/public/launches/`
- Graceful degradation if image fails

### 3.4 Curated launch dataset
- File: `data/seed/upcoming_launches.json`
- Fields: id, builder_name, project_name, micro_market, city, launch_stage, starting_price_label, project_type_label, hero_image, image_alt, primary_highlight, secondary_highlight, source_url, sponsored

### 3.5 Subtle sponsored framing
- "Sponsored" or "Builder spotlight" labels
- Subtle pill or section-level label
- No loud ad language

### 3.6 Liquid-glass visual treatment
- Translucent/glass-like panel, soft border, subtle blur
- Rounded corners, restrained shadow, elegant spacing

### 3.7 Mobile responsiveness
- Horizontal snap-scroll or stacked single-column
- Compact and elegant, not overwhelming

### 3.8 Monetization note
- File: `docs/day17_sponsored_launches_note.md`

## 4. Technical Guidance

### Types
```typescript
export type UpcomingLaunchCard = {
  id: string;
  builder_name: string;
  project_name: string;
  micro_market: string;
  city: string;
  launch_stage: string;
  starting_price_label: string;
  project_type_label: string;
  hero_image: string;
  image_alt: string;
  primary_highlight: string;
  secondary_highlight?: string;
  source_url: string;
  sponsored: boolean;
};
```

### Component structure
```
frontend/src/components/
  UpcomingLaunchesSection.tsx
  UpcomingLaunchCard.tsx
```

### Data: manual curation only (no crawler)

### Image assets
```
frontend/public/launches/
  prestige_01.jpg
  godrej_01.jpg
  ...
```

## 5. Constraints

Do NOT build:
- Broad launch crawling
- Ad bidding / inventory system
- Backend monetization APIs
- Sponsored placements inside results grid
- Auto-rotating carousel
- Full launch-detail pages

## 6. Success Criteria

- Homepage includes Upcoming Launches module
- Placed below hero, above area cards
- Real curated images
- Premium, restrained visual treatment
- Sponsorship visible but subtle
- Cards don't feel like generic ads
- Mobile rendering elegant and compact
- Homepage search/trust hierarchy preserved
- Backed by structured launch dataset
- Monetization note documented
