# Day 9 – Make Discovery Feel Intelligent: Ranking Reasons, Real Search, and First Compare Flow

## Goal
Make OpenEstates feel meaningfully different from a normal property portal:
1. Search must feel real (NL parsing → filtered results with interpreted query display)
2. Results must explain themselves ("why this property for you")
3. Shortlist must begin to feel like a comparison tool (save + compare preview)
4. Homepage area cards should show concrete, decision-relevant signals

## Priorities

### A — Real natural-language search flow
- Simple NL parser: extract area, BHK, budget, preferences (metro, society, noise, docs, sunlight, value)
- Homepage search → /results with query params
- Results page shows: original query, interpreted chips, ranking summary

### B — "Why this property" on results cards
- Short explanation line per card based on property signals + search context
- Lightweight match label (Strong match, Good match, Value pick, Premium match)

### C — Save + compare interaction
- Save button on results cards and property detail page
- localStorage-based shortlist
- Shortlist page: saved cards + comparison table (price, price/sqft, area, society quality, doc score)

### D — Improved homepage area cards
- Each card shows concrete signal (pricing + one livability/access signal)

## Backend
- Add query params to /api/properties: area, bhk, budget_max
- Keep scope minimal — most work is frontend

## Not building today
- Full AI search, OpenFang, database, auth, real persistence, full compare matrix, bidding, live reviews
