# OpenEstates v2 — Product Surfaces, Transparency Widgets, and Data Strategy

**Created:** Day 5
**Purpose:** Implementation reference for Day 6 onward. This document defines what the product actually looks like before any web code is written.

---

## 1. Why We Reset Before Building More

Days 1–4 were a terminal-first AI prototype. They produced useful learning: a scoring model, property schemas, Reddit market intelligence, and structured signal principles. But the product direction is now web-first, transparency-first.

Building more code without defining the exact surfaces, widgets, and data shapes would mean building the wrong thing. Day 5 exists to make the product buildable.

---

## 2. The 4 Core Product Pages

### 2.1 Homepage / Search

**User's job-to-be-done:** Start a search without having to fill in a form. Describe what they want in natural language, or browse by context.

**What the page shows:**
- Prominent natural language search input ("Describe what you're looking for")
- 3–4 curated area cards with brief market summary (Whitefield, Sarjapur, HSR, Bellandur, Electronic City)
- No filter dropdowns on first load — filters emerge after search

**What makes it feel transparent:**
- Each area card shows a single honest data point: median price/sqft, trend direction (up/flat/down), and one key externality signal ("Airport noise zone", "Waterlogging reported", "Good schools nearby")
- Search input placeholder uses real user language, not jargon

**What should NOT be here yet:**
- Login/auth walls
- Saved searches
- Push notifications
- Recommendations before any search context exists

---

### 2.2 Results Page

**User's job-to-be-done:** Understand which properties are worth looking at and why — without having to open each one to find out.

**What the page shows:**
- Ranked property cards (8–12 per page)
- Each card: photo, title, price, BHK, area, price/sqft, possession status
- Each card has a **"Why this property"** summary (1–2 lines, derived from context match)
- Each card has 2–3 **transparency badges** (e.g. "Below area median", "Ready to move", "Low litigation risk")
- Sidebar or top filter strip to refine: area, BHK, budget range, possession status

**What makes it feel transparent:**
- The ranking has a visible reason — not just a sorted list
- Badges are data-derived, not marketing copy
- Users can see at a glance what a card is strong on and what it is not

**What should NOT be here yet:**
- Chat or AI conversation on results page
- Complex saved search flows
- Map view (comes later)

---

### 2.3 Property Detail Page

**User's job-to-be-done:** Reach conviction — understand this property deeply enough to decide whether to shortlist, compare, or dismiss.

**What the page shows:**
- Photo gallery
- Core facts: price, BHK, sqft, price/sqft, floor, facing, possession status, area
- **Transparency widget section** (see Section 3 below)
- Society / livability summary
- Area signals (externalities, infrastructure)
- Market context (price trend, demand signal)
- Seller posture (urgency level, negotiation style — visible but anonymized)
- Shortlist / Compare CTA buttons

**What makes it feel transparent:**
- Every important data point is explained, not just listed
- The property page feels like an asset page — showing context, risk, and opportunity — not a brochure
- User can see the score breakdown: what this property ranks well on and where it falls short

**What should NOT be here yet:**
- Contact seller / unlock contact flow
- Mortgage calculator
- Legal document upload

---

### 2.4 Shortlist / Compare Page

**User's job-to-be-done:** Reduce a set of 3–6 shortlisted properties to a final 1–2 worth acting on. See tradeoffs side by side.

**What the page shows:**
- Grid of shortlisted properties (2–4 columns)
- Per-row comparison across key dimensions: price, price/sqft vs median, BHK, area, possession, society score, environment score, litigation risk, builder risk
- Each cell is color-coded: green (strong), yellow (neutral), red (weak)
- A **"Tradeoffs summary"** block per property: what you gain, what you give up
- Remove from shortlist button per card

**What makes it feel transparent:**
- Side-by-side comparison removes the cognitive load of remembering across pages
- The tradeoffs block names the compromise explicitly — not buried in numbers
- Color coding makes the winner visible without hiding the weaknesses

**What should NOT be here yet:**
- Negotiation or offer flows
- Shared shortlists / collaboration

---

## 3. Transparency Widgets

These are the first 6 concrete widgets. Each is defined precisely enough for a designer or engineer to build it.

---

### Widget 1: Price vs Area Median

**Where it appears:** Results card (badge), Property detail page (full widget)

**What it shows:**
- Asking price
- Computed price/sqft for this listing
- Area median price/sqft (from seed dataset or taxonomy)
- Whether the listing is above or below median, expressed as a percentage and a label: "12% below area median" or "8% above area median"

**Why it matters:** Buyers cannot easily know if a price is fair without reference. This converts raw price into a legible signal.

**Data fields required:**
- `listing.price`
- `listing.carpet_area_sqft`
- `area_stats.median_price_per_sqft` (keyed by area)
- `area_stats.sample_size` (shown as footnote for honesty)

---

### Widget 2: Why This Property For You

**Where it appears:** Results card (1–2 line summary), Property detail page (expandable breakdown)

**What it shows:**
- A 1–2 sentence plain-language explanation of why this property appears in results
- Expandable score breakdown: budget fit, area fit, society quality, environment, possession match, builder risk — each shown as a bar or label (Strong / OK / Weak)

**Why it matters:** Context-based ranking is only trustworthy if the user understands the reason. This widget converts the ranking engine's output into legible explanation.

**Data fields required:**
- `match.score` (overall)
- `match.breakdown` (per component: budget, area, bhk, metro, society, environment, doc_safety, builder_risk, possession)
- `user_context.search_signals` (to reference in explanation text)

---

### Widget 3: Society / Livability Summary

**Where it appears:** Property detail page

**What it shows:**
- Society quality score (0–10 scale, not raw float)
- Maintenance cost (monthly INR)
- Builder quality score
- 2–3 qualitative tags derived from score thresholds: "Well maintained", "Premium society", "Average upkeep", "High maintenance fees"
- If litigation risk > 0.3: a visible "Legal notice" badge in amber

**Why it matters:** "Society quality" as a float is meaningless to buyers. Translated into labels and tags it becomes a decision input.

**Data fields required:**
- `listing.society_quality_score`
- `listing.builder_quality_score`
- `listing.maintenance_cost`
- `listing.litigation_risk`
- `listing.document_completeness_score`

---

### Widget 4: Area Signals

**Where it appears:** Property detail page, area cards on homepage

**What it shows:**
- Positive signals: "Metro within 10 min", "Schools nearby", "Good connectivity"
- Negative/caution signals: "Waterlogging reported", "Highway-facing noise", "Airport noise zone", "Under-construction density high"
- Source note: "Based on community reports and area data"

**Why it matters:** Area externalities (noise, flooding, proximity risks) are among the top decision drivers from Reddit research but are almost never shown on listing portals. This widget makes them visible.

**Data fields required:**
- `listing.metro_distance_minutes`
- `listing.noise_score`
- `listing.facing`
- `area_profile.externality_tags[]` (list of known area signals, curated)
- `area_profile.infrastructure_tags[]`

---

### Widget 5: Market Activity Signal

**Where it appears:** Property detail page, results card (small badge)

**What it shows:**
- Price trend for the area: "Prices up 8% in last 6 months", "Stable", "Cooling"
- Demand signal: "High demand area", "Moderate", "Slow moving"
- Days on market (if known): "Listed 45 days ago"

**Why it matters:** Buyers want to know if they are buying into a market with momentum or one that is stalling. This reduces the fear of overpaying or underselling to themselves.

**Data fields required:**
- `area_stats.price_trend_6m` (percentage change)
- `area_stats.demand_label` ("high" / "moderate" / "low")
- `listing.days_on_market` (optional, shown only if known)

---

### Widget 6: Tradeoffs to Know

**Where it appears:** Property detail page (bottom section), Shortlist compare page (per property column)

**What it shows:**
- 2–3 things this property is strong on: "Below area median price", "Ready to move", "Strong society"
- 1–2 honest cautions: "Noisier than average for this area", "Under-construction — 18 months out", "Litigation risk: check docs before proceeding"

**Why it matters:** No property is perfect. Showing tradeoffs explicitly builds trust and reduces the fear of missing something. It also helps users compare without opening each property individually.

**Data fields required:**
- `match.breakdown` (to identify strong and weak dimensions)
- `listing.possession_status`
- `listing.noise_score`
- `listing.litigation_risk`
- `area_stats.median_price_per_sqft`

---

## 4. Listing / Property Schema

This is the data model required to render the full web UI. It is product-facing and practical — not an ORM model.

```json
{
  "id": "prop_0001",
  "title": "3BHK in Prestige Lakeside Habitat, Whitefield",
  "area": "Whitefield",
  "society_name": "Prestige Lakeside Habitat",
  "builder": "Prestige Group",

  "price": 12500000,
  "carpet_area_sqft": 1450,
  "price_per_sqft": 8621,
  "bhk": 3,
  "floor": 7,
  "total_floors": 14,
  "facing": "East",

  "possession_status": "ready",
  "possession_date": null,

  "metro_distance_minutes": 12,
  "maintenance_cost_monthly": 8000,

  "society_quality_score": 0.78,
  "builder_quality_score": 0.85,
  "document_completeness_score": 0.92,
  "litigation_risk": 0.08,
  "noise_score": 0.35,
  "sunlight_score": 0.72,

  "days_on_market": 38,

  "images": ["url1", "url2", "url3"],

  "area_profile": {
    "externality_tags": ["highway_adjacent", "metro_nearby"],
    "infrastructure_tags": ["schools_nearby", "mall_within_2km"],
    "price_trend_6m": 0.06,
    "median_price_per_sqft": 9200,
    "demand_label": "high",
    "sample_size": 34
  },

  "seller_posture": {
    "urgency_label": "medium",
    "negotiation_style": "flexible",
    "price_flexibility_pct": 5
  },

  "match": {
    "score": null,
    "breakdown": null,
    "explanation": null
  }
}
```

**Notes:**
- `match` fields are populated at query time by the ranking engine — they are not stored on the listing
- `area_profile` is joined from a separate area stats table/file — not duplicated per listing
- `seller_posture` is anonymized — no seller identity exposed to buyer
- Scores are floats internally, but all UI display converts to labels and bands

---

## 5. Area / Society Signal Schema

Area-level data is stored separately and joined at query time.

```json
{
  "area": "Whitefield",
  "median_price_per_sqft": 9200,
  "price_trend_6m_pct": 6.0,
  "demand_label": "high",
  "sample_size": 34,
  "last_updated": "2026-03-01",

  "externality_tags": [
    "airport_noise_zone",
    "waterlogging_risk",
    "highway_adjacent",
    "under_construction_density_high"
  ],
  "infrastructure_tags": [
    "metro_nearby",
    "schools_nearby",
    "it_corridor_adjacent",
    "mall_within_2km"
  ],

  "reddit_signals": {
    "decision_drivers": ["builder_trust", "environment_sensitivity", "area_preference"],
    "recurring_concerns": ["sewage plant proximity", "traffic congestion", "builder delays"],
    "sentiment_label": "mixed",
    "last_updated": "2026-03-07"
  }
}
```

---

## 6. Data Strategy

### 6.1 Posture

We want a **credible localhost product feel** with 15–25 real or realistic properties. We do not need scale. We need quality and variety enough for the transparency widgets to feel real.

### 6.2 What Can Be Mocked / Synthetic

- Buyer profiles and search contexts (used for engine testing only)
- Seller posture (urgency, negotiation style)
- Score and match breakdown (generated by engine at query time)
- Market trend directions (can be hand-set per area for now)

### 6.3 What Should Be Curated (Seed Dataset)

15–25 properties across 4–5 Bangalore areas (Whitefield, Sarjapur, HSR, Bellandur, Electronic City) with realistic:
- Pricing (from area_prices data already in taxonomy)
- Society names (real societies)
- Builder names
- Possession status and floor details
- Area profiles with honest externality tags

**How to build it:** Manually curate 15–25 JSON records using dm8.in or 99acres as reference for realistic data. No scraping yet — just manual JSON authoring with realistic values. One afternoon of work.

### 6.4 dm8.in — Reference and Candidate Source

Treat dm8.in as a **reference listing page** for now:
- Use it to calibrate realistic prices, sqft, society names, builder names
- Do not scrape it broadly yet
- Later: targeted selective extraction using Python pipeline, with AI normalization

### 6.5 What to Attempt Later (Not Now)

- Broad crawling of 99acres or MagicBricks — too noisy, legal ambiguity
- Review summarization pipeline — useful but not Day 6 work
- AI enrichment of listing text — useful but requires listings first
- Real-time price feeds — not needed until product is stable

### 6.6 Data File Layout

```
data/
  seed/
    properties.json          # 15-25 curated properties
    area_profiles.json       # area stats for 5 areas
  reddit/
    taxonomy.json            # existing, keep
    reports/archive/         # existing, keep
  synthetic/
    synthetic_market.json    # keep for engine testing
    synthetic_market_truth.json
    truth_model_weights.json
```

---

## 7. What to Mock vs Curate vs Extract Later

| Data | Now | Later |
|---|---|---|
| Property listings | Manual curation (15–25) | Selective extraction from dm8.in |
| Area profiles | Hand-authored (5 areas) | Enriched from Reddit + crawl |
| Price/sqft stats | From taxonomy (already have) | Updated from periodic crawl |
| Externality tags | Curated from Reddit taxonomy | AI extraction from reviews |
| Buyer/seller profiles | Synthetic (engine testing only) | Real accounts later |
| Match scores | Computed at query time | Same |
| Reddit signals | Already collected | Expand to more subreddits |

---

## 8. Old Prototype Work — What to Retain or Remove

| File / Module | Decision | Reason |
|---|---|---|
| `app/` (TUI) | **Deleted** | Dead surface — v2 is web-first |
| `agents/coach.py` | **Deleted** | AI coach was terminal-first product surface |
| `agents/change_narrator.py` | **Deleted** | TUI-specific narrative output |
| `agents/openfang_client.py` | **Deleted** | OpenFang integration not needed for v2 |
| `graph/` (context graph) | **Deleted** | Built for TUI session state; v2 state lives in structured user context objects |
| `simulation/conversation_simulator.py` | **Deleted** | TUI simulation, no v2 use |
| `src/` (click CLI) | **Deleted** | Deprecated — v2 has a web frontend |
| `agents/schemas.py` | **Keep** | SignalUpdate schema is reusable in pipeline |
| `agents/signal_extractor.py` | **Keep** | Signal extraction logic reusable in data pipeline |
| `simulation/market_generator.py` | **Keep** | Useful for generating test buyer/property sets |
| `simulation/truth_model.py` | **Keep** | 12-component scoring model; will evolve into Rust ranking engine |
| `engine/scoring.py` | **Keep** | Reference for ranking logic |
| `research/` | **Keep** | Reddit pipeline is directly useful for area intelligence |
| `data/reddit/` | **Keep** | Taxonomy and market signals already collected |
| `data/synthetic_market*` | **Keep** | Engine testing data |

---

## 9. Recommended Next Implementation Sequence

**Day 6:** Curate seed dataset — 15–25 properties in `data/seed/properties.json` and 5 area profiles in `data/seed/area_profiles.json`

**Day 7:** Initialize Rust + Axum backend — 4 placeholder routes: `GET /`, `GET /results`, `GET /property/:id`, `GET /shortlist`. Serve seed data as JSON.

**Day 8:** Initialize React frontend — 4 page shells wired to backend routes. No styling yet — just data flowing end to end.

**Day 9:** Results page — property cards with core fields + transparency badges from real seed data.

**Day 10:** Property detail page — full transparency widget section using seed data.

**Day 11:** Shortlist / compare page.

**Day 12:** Wire ranking engine (from `truth_model.py` logic, ported to Rust) — results page shows ranked order with match scores.

---

*This document is the implementation contract for OpenEstates v2. Update it when product direction changes, not by patching code.*
