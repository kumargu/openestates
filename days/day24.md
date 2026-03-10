# Day 24: Move Theme & Tradeoff Computation to Backend — Complete the Single Source of Truth

## 1. The Problem

Day 23 made the backend the single authority for all property data and eliminated frontend fallbacks. But one major gap remains: **theme computation still happens client-side**.

When a user opens a property detail page, the frontend calls `generateMatchSummary()`, `generateTradeoffs()`, and `computeMarketActivity()` from `compare.ts` and `market.ts`. These functions read raw scores from the API response and compute theme labels ("strong", "good", "mixed", "weak") with summaries. The ShortlistPage calls `computeThemes()` and `computeBestFor()` for compare workspace.

This means:
- The backend returns raw scores, the frontend applies judgment — violating "backend owns all computation"
- Two consumers (PropertyPage, ShortlistPage) each run their own theme computation paths
- The backend's knowledge graph has **pre-scored facts** from Claude Skills (e.g., `score_maintenance_quality`, `score_family_friendly`, `overall_score`) that are completely ignored by the frontend's hardcoded scoring
- Theme computation can't improve without a frontend deploy — it should improve when skills enrich new data

**Day 24 completes the architecture: the backend computes themes using KG facts first, and the frontend deletes its scoring code entirely.**

---

## 2. Current State (After Day 23)

### What the backend returns for `/api/properties/:id`:
```json
{
  "property": { /* 30+ fields including raw scores */ },
  "society": { /* enriched from KG */ },
  "area": { /* enriched from KG */ }
}
```

### What the frontend computes locally:
| Function | File | Used By | Computes |
|----------|------|---------|----------|
| `generateMatchSummary()` | compare.ts | PropertyPage | headline + 6 component badges |
| `generateTradeoffs()` | compare.ts | PropertyPage | strengths[] + cautions[] |
| `computeThemes()` | compare.ts | ShortlistPage | 7 theme scores (value, commute, society, greenery, risk, resale, market) |
| `computeBestFor()` | compare.ts | ShortlistPage | "Best for X" labels across shortlisted properties |
| `computeMarketActivity()` | market.ts | PropertyPage | interest label, days on market, area trend |
| `priceVsMedian()` | market.ts | PropertyPage | % diff + verdict |
| `interestLabel()` | market.ts | PropertyPage | display string |
| `daysOnMarketLabel()` | market.ts | PropertyPage | display string |

### What KG facts exist but are unused:
From `score_society` skill:
- `score_maintenance_quality` (0-100)
- `score_family_friendly` (0-100)
- `score_connectivity` (0-100)
- `score_value_for_money` (0-100)
- `score_amenities` (0-100)
- `score_safety` (0-100)
- `overall_score` (0-100)
- `one_line_verdict` (text)
- `top_signals` (tags)
- `top_cautions` (tags)

These are real AI-judged scores from Claude/Gemini reading Reddit threads and reviews. They're far better than the hardcoded threshold logic in compare.ts.

---

## 3. The Fix — Design

### 3.1 KG-facts-first theme computation

The key principle: **don't re-port the frontend's hardcoded logic to Rust. Instead, check KG facts first.**

```
compute_themes(property, society, area, graph):
  For each theme dimension:
    1. Check KG for pre-scored facts → if found, use directly
    2. Fall back to seed data + hardcoded thresholds (same logic as current compare.ts)
    3. The fallback shrinks to nothing as skills enrich more entities
```

### 3.2 Progressive enrichment

```
Society with NO KG facts:
  → themes computed from seed data scores (hardcoded thresholds)
  → identical to current frontend behavior

Society WITH KG facts (from score_society skill):
  → themes computed from AI-judged scores + explanations
  → better quality: "Family friendly scored 25/100 because of child safety incidents on Reddit"
  → this is the goal state for all entities
```

### 3.3 Backend serves computed themes in API response

```json
{
  "property": { ... },
  "society": { ... },
  "area": { ... },
  "themes": {
    "value": { "label": "strong", "summary": "12% below Whitefield median" },
    "commute": { "label": "good", "summary": "18 min to metro, moderate traffic" },
    "society": { "label": "strong", "summary": "Premium society, well-maintained" },
    "greenery": { "label": "good", "summary": "Good green cover and open space" },
    "risk": { "label": "strong", "summary": "Low risk across all dimensions" },
    "resale": { "label": "good", "summary": "Good resale potential" },
    "market": { "label": "mixed", "summary": "Moderate interest, 45 days on market" }
  },
  "tradeoffs": {
    "headline": "Strong match for strong value and good metro access.",
    "strengths": ["Strong value relative to Whitefield median", "Good metro access"],
    "cautions": ["Heavier traffic in this corridor"],
    "components": [
      { "label": "Value", "level": "strong" },
      { "label": "Commute & Access", "level": "good" },
      ...
    ]
  },
  "market_activity": {
    "interest_level": "moderate",
    "saves_last_7d": null,
    "offers_last_7d": null,
    "days_on_market": 45,
    "days_on_market_label": "On market for a while",
    "interest_label": "Moderate interest",
    "area_trend_summary": "Prices trending up",
    "price_vs_median": { "pct_diff": -12, "verdict": "Good value", "verdict_class": "positive" }
  }
}
```

---

## 4. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Create `backend/src/scoring/` module with theme computation | 1.5 hours |
| **B** | Extend PropertyDetailResponse with themes, tradeoffs, market_activity | 30 min |
| **C** | Update frontend to render API-provided themes, delete compare.ts and market.ts | 1 hour |
| **D** | Verify consistency end-to-end | 30 min |

---

## 5. Implementation Plan

### Phase A: Backend Scoring Module (1.5 hours)

**Goal:** Create a Rust module that computes themes, tradeoffs, and market activity — checking KG facts first.

#### A.1 Create `backend/src/scoring/mod.rs`

New module with public types and the main computation entry point:

```rust
pub mod themes;

// Re-export the main types and functions
pub use themes::{
    CompareThemes, MarketActivityResponse, ThemeLabel, ThemeResult,
    TradeoffsResponse, compute_themes, compute_tradeoffs, compute_market_activity,
};
```

#### A.2 Create `backend/src/scoring/themes.rs`

This file contains ALL theme computation. The structure mirrors `compare.ts` but checks KG first.

```rust
use serde::Serialize;
use crate::knowledge::KnowledgeGraph;
use crate::models::{AreaProfile, Property, Society};
use crate::routes::enrichment::{kg_numeric, kg_text, kg_tags, society_node_id};

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ThemeLabel { Strong, Good, Mixed, Weak }

#[derive(Serialize, Clone)]
pub struct ThemeResult {
    pub label: ThemeLabel,
    pub summary: String,
}

#[derive(Serialize, Clone)]
pub struct CompareThemes {
    pub value: ThemeResult,
    pub commute: ThemeResult,
    pub society: ThemeResult,
    pub greenery: ThemeResult,
    pub risk: ThemeResult,
    pub resale: ThemeResult,
    pub market: ThemeResult,
}

#[derive(Serialize, Clone)]
pub struct TradeoffsResponse {
    pub headline: String,
    pub strengths: Vec<String>,
    pub cautions: Vec<String>,
    pub components: Vec<ThemeComponent>,
}

#[derive(Serialize, Clone)]
pub struct ThemeComponent {
    pub label: String,
    pub level: ThemeLabel,
}

#[derive(Serialize, Clone)]
pub struct PriceVsMedian {
    pub pct_diff: i32,
    pub verdict: String,
    pub verdict_class: String, // "positive", "neutral", "warning"
}

#[derive(Serialize, Clone)]
pub struct MarketActivityResponse {
    pub interest_level: String,
    pub saves_last_7d: Option<u32>,
    pub offers_last_7d: Option<u32>,
    pub days_on_market: u32,
    pub days_on_market_label: String,
    pub interest_label: String,
    pub area_trend_summary: String,
    pub price_vs_median: Option<PriceVsMedian>,
}
```

#### A.3 Theme computation functions

Each function follows the same pattern: **check KG → fallback to seed data**.

**Value theme:**
```rust
fn compute_value(p: &Property, area: Option<&AreaProfile>, graph: &KnowledgeGraph) -> ThemeResult {
    // KG-first: check for score_value_for_money from score_society skill
    let node_id = society_node_id(&p.society_id);
    if let Some(score) = kg_numeric(graph, &node_id, "score_value_for_money") {
        let normalized = score / 100.0; // skill scores are 0-100
        return ThemeResult {
            label: score_to_label(normalized),
            summary: kg_text(graph, &node_id, "value_reasoning")
                .unwrap_or_else(|| fallback_value_summary(p, area)),
        };
    }

    // Fallback: same logic as current compare.ts
    let Some(area) = area else {
        return ThemeResult { label: ThemeLabel::Mixed, summary: "Area data unavailable".into() };
    };
    let ratio = p.price_per_sqft as f64 / area.median_price_per_sqft as f64;
    let pct_diff = ((1.0 - ratio) * 100.0).round() as i32;
    // ... same threshold logic as compare.ts ...
}
```

**Society theme — KG-first is most impactful here:**
```rust
fn compute_society(p: &Property, society: Option<&Society>, graph: &KnowledgeGraph) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: overall_score from score_society skill
    if let Some(score) = kg_numeric(graph, &node_id, "overall_score") {
        let normalized = score / 100.0;
        let summary = kg_text(graph, &node_id, "one_line_verdict")
            .unwrap_or_else(|| format!("Society scored {}/100", score as u32));
        return ThemeResult { label: score_to_label(normalized), summary };
    }

    // Fallback: seed data society_quality_score + maintenance_sentiment
    let score = p.society_quality_score;
    let label = score_to_label(score);
    // ... same logic as compare.ts ...
}
```

**Tradeoffs — use KG top_signals and top_cautions when available:**
```rust
pub fn compute_tradeoffs(
    p: &Property,
    area: Option<&AreaProfile>,
    society: Option<&Society>,
    graph: &KnowledgeGraph,
) -> TradeoffsResponse {
    let node_id = society_node_id(&p.society_id);

    // KG-first: top_signals and top_cautions from score_society skill
    let kg_signals = kg_tags(graph, &node_id, "top_signals");
    let kg_cautions = kg_tags(graph, &node_id, "top_cautions");

    let mut strengths = kg_signals.unwrap_or_default();
    let mut cautions = kg_cautions.unwrap_or_default();

    // Supplement with seed-data-derived tradeoffs if KG didn't provide enough
    if strengths.len() < 2 {
        // ... same threshold checks as compare.ts generateTradeoffs() ...
    }
    if cautions.len() < 1 {
        // ... same threshold checks ...
    }

    // Build headline from themes
    let themes = compute_themes(p, area, society, graph);
    let headline = build_headline(&themes);

    // Build components
    let components = vec![
        ThemeComponent { label: "Value".into(), level: themes.value.label.clone() },
        ThemeComponent { label: "Commute & Access".into(), level: themes.commute.label.clone() },
        ThemeComponent { label: "Society Quality".into(), level: themes.society.label.clone() },
        ThemeComponent { label: "Greenery / Open Space".into(), level: themes.greenery.label.clone() },
        ThemeComponent { label: "Document Trust".into(), level: score_to_label(p.document_completeness_score) },
        ThemeComponent { label: "Risk Profile".into(), level: invert_risk_label(p) },
    ];

    TradeoffsResponse {
        headline,
        strengths: strengths.into_iter().take(3).collect(),
        cautions: cautions.into_iter().take(2).collect(),
        components,
    }
}
```

**Market activity — mostly pass-through, add display labels:**
```rust
pub fn compute_market_activity(
    p: &Property,
    area: Option<&AreaProfile>,
) -> MarketActivityResponse {
    let interest_level = p.interest_level.clone().unwrap_or_else(|| "moderate".into());
    let days_on_market_label = match p.days_on_market {
        d if d <= 14 => "Recently listed".into(),
        d if d <= 30 => "Listed this month".into(),
        d if d <= 60 => "On market for a while".into(),
        _ => "Long on market — may negotiate".into(),
    };
    let interest_label = match interest_level.as_str() {
        "high" => "High interest area".into(),
        "moderate" => "Moderate interest".into(),
        _ => "Limited interest".into(),
    };

    let price_vs_median = area.map(|a| {
        let pct_diff = (((p.price_per_sqft as f64 - a.median_price_per_sqft as f64)
            / a.median_price_per_sqft as f64) * 100.0).round() as i32;
        let (verdict, verdict_class) = if pct_diff <= -10 {
            ("Good value".into(), "positive".into())
        } else if pct_diff <= 5 {
            ("Near market".into(), "neutral".into())
        } else {
            ("Premium pricing".into(), "warning".into())
        };
        PriceVsMedian { pct_diff, verdict, verdict_class }
    });

    MarketActivityResponse {
        interest_level,
        saves_last_7d: p.saves_last_7d,
        offers_last_7d: p.offers_last_7d,
        days_on_market: p.days_on_market,
        days_on_market_label,
        interest_label,
        area_trend_summary: area.map(|a| a.trend_summary.clone()).unwrap_or_else(|| "Trend data unavailable".into()),
        price_vs_median,
    }
}
```

**Helper:**
```rust
fn score_to_label(score: f64) -> ThemeLabel {
    if score >= 0.8 { ThemeLabel::Strong }
    else if score >= 0.6 { ThemeLabel::Good }
    else if score >= 0.4 { ThemeLabel::Mixed }
    else { ThemeLabel::Weak }
}
```

**PAUSE. Verify: `cargo check` passes. The scoring module compiles. No wiring yet — just the logic.**

---

### Phase B: Wire Into API Response (30 min)

**Goal:** The property detail endpoint returns pre-computed themes, tradeoffs, and market activity.

#### B.1 Extend PropertyDetail response struct

In `backend/src/routes/properties.rs`:

```rust
use crate::scoring::{CompareThemes, TradeoffsResponse, MarketActivityResponse};

#[derive(Serialize)]
pub struct PropertyDetail {
    pub property: Property,
    pub society: Option<Society>,
    pub area: Option<AreaProfile>,
    pub themes: CompareThemes,             // NEW
    pub tradeoffs: TradeoffsResponse,      // NEW
    pub market_activity: MarketActivityResponse,  // NEW
}
```

#### B.2 Compute in get_property handler

```rust
pub async fn get_property(...) -> ... {
    // ... existing property/society/area loading and enrichment ...

    let themes = scoring::compute_themes(&property, area.as_ref(), society.as_ref(), &graph);
    let tradeoffs = scoring::compute_tradeoffs(&property, area.as_ref(), society.as_ref(), &graph);
    let market_activity = scoring::compute_market_activity(&property, area.as_ref());

    Ok(Json(PropertyDetail {
        property,
        society,
        area,
        themes,
        tradeoffs,
        market_activity,
    }))
}
```

#### B.3 Add themes to shortlist compare endpoint

If the shortlist/compare endpoint exists, it should also use `compute_themes()` for each property so the ShortlistPage gets server-computed themes. If there's no dedicated compare endpoint, the frontend can call `/api/properties/:id` for each shortlisted property and use the themes from the response.

**PAUSE. Verify: `cargo check` passes. `GET /api/properties/:id` returns the new fields. Themes match what the frontend was computing.**

---

### Phase C: Simplify Frontend (1 hour)

**Goal:** Frontend renders API-provided themes. Delete compare.ts and market.ts.

#### C.1 Update TypeScript types

In `frontend/src/lib/types.ts`, extend `PropertyDetailResponse`:

```typescript
export type PropertyDetailResponse = {
  property: { ... };   // existing
  society: { ... } | null;  // existing
  area: { ... } | null;  // existing
  themes: CompareThemes;  // NEW — from backend
  tradeoffs: {            // NEW — from backend
    headline: string;
    strengths: string[];
    cautions: string[];
    components: { label: string; level: ThemeLabel }[];
  };
  market_activity: {      // NEW — from backend
    interest_level: string;
    saves_last_7d: number | null;
    offers_last_7d: number | null;
    days_on_market: number;
    days_on_market_label: string;
    interest_label: string;
    area_trend_summary: string;
    price_vs_median: {
      pct_diff: number;
      verdict: string;
      verdict_class: string;
    } | null;
  };
};
```

#### C.2 Update PropertyPage.tsx

Remove imports from compare.ts and market.ts. Use API-provided data directly:

```tsx
// REMOVE these:
// import { generateMatchSummary, generateTradeoffs } from "../lib/compare.ts";
// import { computeMarketActivity, interestLabel, daysOnMarketLabel, priceVsMedian } from "../lib/market.ts";

// REPLACE with:
const { property: p, society, area, themes, tradeoffs, market_activity } = data;

// "Why this property" section — use tradeoffs directly:
// OLD: const match = generateMatchSummary(p, area);
// NEW: use data.tradeoffs.headline and data.tradeoffs.components

// "Market activity" section — use market_activity directly:
// OLD: const market = computeMarketActivity(p, area);
// NEW: use data.market_activity fields

// "Price vs median" section:
// OLD: const pvm = area ? priceVsMedian(p.price_per_sqft, area.median_price_per_sqft) : null;
// NEW: use data.market_activity.price_vs_median

// "Tradeoffs to know" section:
// OLD: const tradeoffs = generateTradeoffs(p, area);
// NEW: use data.tradeoffs.strengths and data.tradeoffs.cautions
```

#### C.3 Update ShortlistPage.tsx

```tsx
// REMOVE:
// import { computeThemes, computeBestFor } from "../lib/compare.ts";

// For each shortlisted property, themes now come from the API response.
// Fetch full property details for each shortlisted ID → use response.themes
// computeBestFor() logic moves server-side or is reimplemented inline
// using the API-provided themes (it's just finding the max per dimension).
```

For `computeBestFor()`, since it's a simple comparison across shortlisted properties, it can remain as a **thin frontend utility** that operates on API-provided themes — no scoring, just finding the best per dimension. Move it to a small helper in ShortlistPage or a new `shortlist-utils.ts` (NOT in compare.ts).

#### C.4 Delete frontend scoring files

```
rm frontend/src/lib/compare.ts
rm frontend/src/lib/market.ts
```

#### C.5 Verify no remaining imports

Search for any remaining imports from the deleted files and fix them.

**PAUSE. Verify: `npm run build` succeeds with zero errors. No imports from deleted files. PropertyPage renders all sections from API data.**

---

### Phase D: Consistency Verification (30 min)

#### D.1 Theme consistency test

1. Load `/api/properties/:id` for a property
2. Verify `themes`, `tradeoffs`, `market_activity` fields are present
3. Compare theme labels to what the old frontend would have computed — they should match for seed-data-only properties
4. For KG-enriched properties — themes should reflect KG facts (possibly different/better than old hardcoded logic)

#### D.2 Shortlist compare test

1. Add 2-3 properties to shortlist
2. Open shortlist compare page
3. Verify themes render correctly from API data
4. Verify "Best for X" labels work

#### D.3 No frontend computation test

1. Search for `scoreToLabel`, `computeValue`, `computeCommute` in frontend code
2. Verify: zero matches. All scoring is gone from frontend.
3. Verify: `compare.ts` and `market.ts` don't exist

#### D.4 KG-first test (if enriched data available)

1. Find a society that has been scored by `score_society` skill (has KG facts)
2. Load its property detail
3. Verify: society theme uses KG's `overall_score` + `one_line_verdict`, not hardcoded thresholds
4. Verify: tradeoffs include KG's `top_signals` and `top_cautions`

#### D.5 Error state regression

1. Stop the backend
2. Load PropertyPage → should show clean error state, no theme computation attempted
3. No console errors about missing theme functions

**PAUSE. All tests pass. Theme computation lives in the backend. Frontend is a pure renderer.**

---

## 6. Files Changed

### New
- `backend/src/scoring/mod.rs` — Scoring module entry point
- `backend/src/scoring/themes.rs` — Theme, tradeoff, market activity computation (KG-first)

### Modified (Backend)
- `backend/src/main.rs` — add `mod scoring;`
- `backend/src/routes/properties.rs` — extend PropertyDetail with themes/tradeoffs/market_activity, compute in handler
- `backend/src/routes/enrichment.rs` — possibly make some KG helpers pub(crate) if not already

### Modified (Frontend)
- `frontend/src/lib/types.ts` — add themes, tradeoffs, market_activity to PropertyDetailResponse
- `frontend/src/pages/PropertyPage.tsx` — render from API data, remove compare.ts/market.ts imports
- `frontend/src/pages/ShortlistPage.tsx` — render from API data, remove compare.ts import

### Deleted
- `frontend/src/lib/compare.ts` — all logic moved to backend scoring module
- `frontend/src/lib/market.ts` — all logic moved to backend scoring module

---

## 7. What NOT to Build Today

- New scoring dimensions beyond the existing 7 (value, commute, society, greenery, risk, resale, market)
- AI-powered scoring at request time (skills run in batch; themes use pre-computed KG facts)
- Embedding-based search integration (Day 25)
- New KG fact types or skills
- Database changes
- Frontend redesign — same UI, different data source
- Caching of computed themes (they're cheap to compute inline)

---

## 8. Success Criteria

- [ ] `backend/src/scoring/` module exists with theme, tradeoff, and market activity computation
- [ ] KG-facts-first: when KG has `overall_score`, `top_signals`, etc., they're used over hardcoded thresholds
- [ ] Fallback: when KG facts are absent, output matches what `compare.ts` was computing
- [ ] `PropertyDetailResponse` includes `themes`, `tradeoffs`, `market_activity`
- [ ] `frontend/src/lib/compare.ts` — DELETED
- [ ] `frontend/src/lib/market.ts` — DELETED
- [ ] PropertyPage renders entirely from API-provided data
- [ ] ShortlistPage renders themes from API-provided data
- [ ] No `scoreToLabel`, `computeValue`, `computeCommute`, etc. in frontend code
- [ ] `cargo check` + `cargo test` pass
- [ ] `npm run build` succeeds with zero errors
- [ ] Same property shows identical themes whether viewed from detail page or shortlist compare

---

## 9. The Principle

After today, the data flow is complete:

```
Skills → KG facts → Backend scoring (KG-first) → API response → Frontend renders
```

No side channels. No client-side judgment. No split computation paths. The frontend is a pure rendering layer that trusts the backend completely.

When a skill enriches a new society with better scores, the next API call serves better themes — with zero frontend changes. The system gets smarter by enrichment alone.

This is what "single source of truth" looks like at every layer: **skills produce facts, the graph stores them, the backend computes from them, the frontend shows them.**
