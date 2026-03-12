# Day 40: Cross-Page Quality Bar — Helmet SEO, Skeletons, Sharing

## Goal
Bring every page to the same quality standard: Helmet SEO on 3 remaining pages, loading skeletons on 2 detail pages, sharing on SocietyDetailPage.

## Product Reason
Consistency builds trust. PropertyPage has rich SEO + skeleton + sharing, but other pages fall back to defaults. Users sharing results or shortlists get generic previews.

## Deliverables

### 1. Helmet SEO on ResultsPageA
- Dynamic title: "[query] — Property Search | OpenEstates"
- Description from search summary, OG tags

### 2. Helmet SEO on ShortlistPage
- Static: "Compare Saved Homes | OpenEstates"

### 3. Helmet SEO on SocietySearchPage
- Dynamic title: "[query] — Society Rankings | OpenEstates"

### 4. Loading skeleton for PropertyPage
- Replace PageState spinner with layout-matching skeleton
- Hero placeholder + title + price + two-column sections

### 5. Loading skeleton for SocietyDetailPage
- Same pattern: hero + title + dimension bars + sidebar placeholders

### 6. ShareButtons on SocietyDetailPage
- Generalize ShareButtons to accept `path` prop (not just propertyId)
- Add to SocietyDetailPage sidebar

## Constraints
- No new dependencies, no backend changes
- ShareButtons backward-compatible with PropertyPage
- Reuse existing skeleton CSS classes

## Success Criteria
1. Every page has unique title + meta description
2. Detail pages show skeletons during load
3. SocietyDetailPage has sharing buttons
4. No regressions
5. `npm run build` passes
