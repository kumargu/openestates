# days/day19.md

# Day 19: Society Intelligence — One Unforgettable Vertical Slice

**Hero query:** "family-friendly society in Whitefield"
**Scope:** 6-8 Whitefield societies. Real photos. Real Reddit evidence. Real Google reviews. Ranked with explanations.

## 1. Goal

Ship one search experience that makes someone say "this is not like any property platform I've used." A user types "family-friendly society in Whitefield" and gets back ranked societies — not listings — with real photos, resident quotes, honest tradeoffs, life-fit reasons, and visible evidence depth.

## 2. Product Reason

People search for **societies** (apartment complexes), not individual units. The society IS the decision. Yet no platform:
- ranks societies against each other with evidence
- shows what residents actually say (from Reddit/Google)
- explains why one society beat another
- admits tradeoffs honestly
- labels "best for families" vs "best for value" vs "best for calm living"

This is the moat. One vertical slice, done extraordinarily well.

## 3. Deliverables (in priority order)

### 3.1 Define the Society Result Card (design-first)

Before any pipeline work, define exactly what one society card shows:

```
┌─────────────────────────────────────────────────┐
│ [Real Photo]                         #1 · 87/100│
│                                                 │
│ Prestige Lakeside Habitat                       │
│ Prestige Group · Whitefield · 2018 · 3426 units │
│                                                 │
│ 🏷️ BEST FOR FAMILIES                            │
│                                                 │
│ "Best for families who want a full-service      │
│  township with strong school access and can     │
│  tolerate peak-hour exit traffic."              │
│                                                 │
│ ┌─────────────────────────────────────┐         │
│ │ Family-friendly  ████████░░  85     │         │
│ │ Maintenance      █████████░  90     │         │
│ │ School access    ████████░░  82     │         │
│ │ Calm environment ███████░░░  75     │         │
│ └─────────────────────────────────────┘         │
│                                                 │
│ ★ 4.2 Google (156 reviews) · 8 Reddit threads   │
│                                                 │
│ What residents say:                             │
│ "Well-maintained and good for families, but     │
│  exit traffic is the daily pain point."         │
│                                                 │
│ ✓ Lake-facing towers  ✓ Large clubhouse         │
│ ⚠ Peak-hour traffic   ⚠ Higher maintenance fees │
│                                                 │
│ Why this beat #2: Stronger resident sentiment   │
│ on maintenance quality and school proximity     │
│ than Brigade Metropolis.                        │
│                                                 │
│ Confidence: Moderate · Last enriched: 2 days ago│
└─────────────────────────────────────────────────┘
```

Key elements:
- **Life-fit narrative** (1-2 sentences explaining who this society is best for)
- **"Best for" ribbon** (families / value / calm / greenery / premium)
- **Dimension score bars** (query-relevant dimensions)
- **Resident voice** (synthesized from Reddit + Google)
- **Signals** (green) and **Cautions** (amber)
- **Why-this-beat-next** (competitive ranking explanation)
- **Evidence depth + freshness**

### 3.2 Curate Whitefield Society Universe

Start from existing `societies.json` (already has 4+ Whitefield societies), then expand:

Use Claude API to identify the canonical top societies in Whitefield for families. Cross-reference with seed data. Target: **6-8 societies** with real names, real builders, real details.

**Script:** `pipeline/society_discovery.py`

```python
# Input: area + query intent
# Output: enriched society list merged with seed data
# Method: Claude identifies societies → validate names are real → merge with existing seed
```

Do NOT rely purely on AI — use it to expand the seed list, then validate. If a society name can't be confirmed via Google, drop it.

### 3.3 Reddit Intelligence for Each Society

**Script:** `pipeline/reddit_enrichment.py`

For each society + area-level:
1. Reddit search API: `"{society_name}" site:reddit.com/r/bangalore`
2. Also: `"best society Whitefield" site:reddit.com`
3. Extract: thread titles, top 5 comments per thread, URLs, dates
4. Claude synthesis per society:
   - Sentiment (positive/negative/mixed)
   - Recurring positives (list)
   - Recurring complaints (list)
   - Best resident quote (verbatim or close paraphrase)
   - Family suitability signal
   - Confidence (based on thread count)

Cache: `data/intelligence/whitefield/{society_slug}/reddit.json`

**Important discipline:**
- Separate society-specific evidence from area-level evidence
- Don't let one angry thread dominate
- Mark confidence: 1-2 threads = low, 3-5 = moderate, 6+ = good

### 3.4 Google Reviews + Real Photos

**Script:** `pipeline/google_enrichment.py`

For each society:
1. Google Places text search → find place
2. Extract: rating, review count, top 5 reviews
3. Download 2-3 real photos per society → `frontend/public/societies/{slug}/`
4. Fallback if no API key: use Google Image search results (manual or SerpAPI)

Cache: `data/intelligence/whitefield/{society_slug}/google.json`

Photos stored at: `frontend/public/societies/{slug}/1.jpg`, `2.jpg`, `3.jpg`

### 3.5 Society Scoring + Ranking

**Script:** `pipeline/society_scorer.py`

For the hero query "family-friendly society in Whitefield":

```python
# Query-aware dimension weights:
FAMILY_QUERY_WEIGHTS = {
    "family_friendly": 0.30,
    "maintenance_quality": 0.20,
    "school_access": 0.20,
    "calm_environment": 0.15,
    "builder_trust": 0.10,
    "value": 0.05,
}

# Each dimension scored 0-100 from:
# - Reddit sentiment signals
# - Google review rating
# - Seed data (area profile, society metadata)
# - Builder reputation heuristic

# Output per society:
{
    "society_id": "prestige_lakeside_habitat",
    "overall_score": 87,
    "rank": 1,
    "best_for_label": "Best for families",
    "life_fit_reason": "Best for families who want a full-service township with strong school access and can tolerate peak-hour exit traffic.",
    "dimension_scores": {...},
    "why_above_next": "Stronger resident sentiment on maintenance quality and school proximity than Brigade Metropolis.",
    "signals": [...],
    "cautions": [...],
    "resident_quote": "...",
    "confidence": 0.78,
    "evidence": {"reddit_threads": 8, "google_reviews": 156}
}
```

Write the full scored + ranked output to: `data/intelligence/whitefield/_ranked_results.json`

### 3.6 Rust API Endpoint

Add to Axum backend:

```
GET /api/societies/search?q=family+friendly+whitefield
```

Reads from `data/intelligence/whitefield/_ranked_results.json` (pre-computed by pipeline).

Response shape matches the scored output. Include:
- `query_interpreted` (intent, area, dimensions used)
- `results` (ranked society list with all card data)
- `area_context` (from area_profiles.json)
- `enrichment_status` (thread count, review count, freshness)

### 3.7 Frontend: Society Search Results Page

New route: `/search` or evolve existing `/results`

Layout:
- **Top:** Search bar with NL input + interpreted intent chips below
- **Main:** Ranked society cards (as defined in 3.1)
- **Sidebar/top bar:** Area context (Whitefield: metro status, traffic reality, price trend)
- **Bottom:** Evidence transparency bar ("Ranked using 23 Reddit threads and 200+ Google reviews · Last updated 2 days ago")

Card interactions:
- Click card → expand inline drawer with:
  - More photos
  - Full resident voice section
  - Detailed dimension breakdown
  - More evidence sources

### 3.8 Disk Cache Structure

```
data/intelligence/
  whitefield/
    _area_context.json
    _ranked_results.json          # final scored + ranked output
    prestige_lakeside_habitat/
      reddit.json
      google.json
    brigade_metropolis/
      reddit.json
      google.json
    ...
frontend/public/societies/
  prestige_lakeside_habitat/
    1.jpg
    2.jpg
  brigade_metropolis/
    1.jpg
    2.jpg
  ...
```

## 4. Technical Guidance

### Pipeline execution order:
```bash
# Step 1: Discover/curate societies
python3 pipeline/society_discovery.py "family-friendly society in Whitefield"

# Step 2: Fetch Reddit intelligence
python3 pipeline/reddit_enrichment.py whitefield

# Step 3: Fetch Google reviews + photos
python3 pipeline/google_enrichment.py whitefield

# Step 4: Score and rank
python3 pipeline/society_scorer.py whitefield

# Result: data/intelligence/whitefield/_ranked_results.json is ready
# API serves it, frontend renders it
```

### API keys needed in `.env`:
- `ANTHROPIC_API_KEY` — for Claude synthesis (already exists)
- `GOOGLE_PLACES_API_KEY` — for Google reviews + photos (new, may need setup)
- Reddit public JSON API needs no key

### On photos:
- If Google Places API isn't available, use Google Image search as fallback
- Minimum 2 photos per society, aim for 3
- Photos should show the actual complex (exterior, amenities, entrance) not stock images
- Compress to reasonable size for web

### On AI synthesis:
- Use Claude Haiku for cost-efficiency on Reddit thread synthesis (many calls)
- Use Claude Sonnet for the life-fit narrative and ranking explanations (fewer, higher quality)

## 5. Constraints

- **Whitefield only.** No multi-area generalization.
- **6-8 societies max.** Quality over quantity.
- **Real data only.** No synthetic evidence. Every quote must trace to a real source.
- **Real photos only.** Placeholder images are not acceptable.
- **Pipeline runs offline.** API reads from cache, does not fetch live.
- **Keep existing seed data intact.** Intelligence layer is additive.

## 6. Success Criteria

- [ ] `_ranked_results.json` exists with 6-8 scored Whitefield societies
- [ ] Each society has 2-3 real photos in `frontend/public/societies/`
- [ ] Each society has Reddit-sourced evidence (threads, quotes, sentiment)
- [ ] Each society has a life-fit narrative and "best for" label
- [ ] Rankings include "why this beat the next one" explanations
- [ ] API endpoint returns the full ranked result set
- [ ] Frontend renders society cards that look genuinely different from any property portal
- [ ] A non-technical person looking at the results page would say "I've never seen search results like this"

## 7. Product Decisions

- **Society is the search unit.** Fundamental shift from property listings.
- **Life-fit narratives over raw scores.** The card tells you who this society is FOR, not just how it scored.
- **Honest tradeoffs are required.** Every society must admit something. This is the transparency promise.
- **"Best for" ribbons create a decision surface.** The page helps you decide, not just browse.
- **Evidence is the product.** Reddit threads and Google reviews aren't hidden metadata — they're the core value prop.
- **Competitive ranking explanations.** "Why this beat #2" is the killer feature for trust.
