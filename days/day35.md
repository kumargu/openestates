# Day 35: SEO Structured Data (JSON-LD) + UI Declutter

## Goal
Make property and society pages machine-readable for Google (JSON-LD) and remove visual clutter competing with primary actions.

## Product Reason
Google cannot index what it cannot understand. Property pages have meta tags but no structured data — Google shows generic snippets instead of rich results with price, location, images. JSON-LD is the highest-leverage SEO move remaining.

## Deliverables

### 1. JSON-LD on PropertyPage
- `RealEstateListing` schema with: name, description, url, image, price (INR), address, floorSize, numberOfRooms
- Helper function `buildPropertyJsonLd()` rendered inside Helmet
- File: `frontend/src/pages/PropertyPage.tsx`

### 2. JSON-LD on SocietyDetailPage
- `ApartmentComplex` schema with: name, description, image, address, numberOfUnits, yearBuilt
- Helper function `buildSocietyJsonLd()` rendered inside Helmet
- File: `frontend/src/pages/SocietyDetailPage.tsx`

### 3. UI Declutter
- ResultsPageA: "Quick view" label hides on mobile (icon-only below 600px)
- PropertyPage: replace hardcoded `#eee` with CSS var in sidebar bar chart
- index.css: add `.sr-only` utility, `.card-a-detail-btn-label` responsive hiding

## Constraints
- No new dependencies, no backend changes
- JSON-LD additive — don't break existing Helmet meta tags
- Keep JSON-LD builders in page files, not a shared lib
- Desktop layouts unchanged

## Success Criteria
1. PropertyPage renders valid JSON-LD with price, location, image, rooms
2. SocietyDetailPage renders valid JSON-LD
3. "Quick view" label hides on mobile
4. No hardcoded colors in sidebar bar chart
5. `npm run build` passes
