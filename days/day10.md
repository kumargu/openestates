# days/day10.md

# OpenEstates v2

## Day 10 – Property Detail Conviction + Theme-Based Compare Workspace

Before starting today, read:

- CLAUDE.md

- LEARNING.md

- docs/openestates_v2_surfaces_and_data.md

- latest shipped Day 8 summary

- latest customer journey review JSON

- days/day09.md

- any notes from Day 9 implementation, especially: 
  natural-language search behavior

- results-page ranking explanations

- shortlist save flow

- homepage area-card improvements

Day 9 was about making discovery feel intelligent.

Day 10 is about making the product feel like a decision platform.

The two highest-leverage surfaces now are:

- the property detail page, where a user decides whether this listing deserves conviction

- the shortlist / compare page, where a user decides between 2–4 properties

This day should make OpenEstates feel closer to:

- Hinge-style matching: “why this matches you”

- Robinhood-style market visibility: visible interest, trend, and value context

- Levels.fyi-style comparison: structured side-by-side decision workspace

Do not try to add too many new systems today.
Focus on making these two surfaces feel substantially more differentiated than a normal property portal.

## 1. Goal

The goal of Day 10 is to deepen the customer journey from discovery → conviction.

By the end of Day 10, OpenEstates should support this believable flow:

- user searches or browses

- user lands on results with match framing

- user opens a property detail page that feels like an asset page, not a brochure

- user saves 2–4 properties

- user opens shortlist and sees a theme-based compare workspace

- user understands not just which property is better, but what tradeoff each option represents

The product should start to answer:

- Why is this property a strong match?

- Is it fair value or premium?

- What hidden negatives should I care about?

- How does it compare against the others I am considering?

- Which one is better for value, commute, openness, or society quality?

This is the real Day 10 objective.

## 2. Product Reason

OpenEstates will not win by being a nicer listing site.

It will win if it helps users reach conviction faster through:

- visible reasoning

- honest tradeoffs

- structured comparison

- and market legibility

### Why the property detail page matters

This is where transparency must become real.
A good property detail page should make the user feel:

- “I understand this listing”

- “I know where it is strong”

- “I know what to be careful about”

- “I know how it compares to the market”

Without that, OpenEstates is still just a portal with better copy.

### Why the compare workspace matters

Most people compare homes mentally, through tabs, screenshots, WhatsApp, and spreadsheets.

OpenEstates should reduce that chaos.

A strong shortlist page should not just show saved homes.
It should become a decision workspace that compares properties across the themes that actually matter:

- value

- commute

- society quality

- greenery / openness

- risk

- resale strength

- market activity

That is where the product becomes truly useful.

## 3. Deliverables

By the end of Day 10, the implementation should produce the following.

### 3.1 Property detail page becomes a true decision surface

The property detail page must include these visible sections:

- Property Summary

- Why this property for you

- Price vs Area Median

- Market Activity

- Tradeoffs to Know

- Society / Livability

- Area Signals

- Save / Compare actions

These sections should be explicit and readable, not implied by raw data.

### 3.2 “Why this property for you” becomes stronger and more Hinge-like

This section should reflect match framing, not just generic listing description.

Examples:

- “Strong match for metro access and value-conscious buyers.”

- “Better fit if society quality matters more than lowest price.”

- “Premium choice with stronger sunlight and society reputation.”

- “Good value option, but commute tradeoff is higher.”

This section should feel like contextual matching, not static badges.

### 3.3 Robinhood-style market visibility begins

Introduce a mocked but structured Market Activity section.

At minimum include:

- area trend direction

- days on market

- saves or interest level

- mock offer / bid visibility

- simple value framing relative to market

Examples:

- “High interest area”

- “Saved by 14 users this week”

- “2 mock offers in last 7 days”

- “Listed 38 days ago”

- “Area prices up 6% in last 6 months”

This is not for hype.
It is for transparent market context.

### 3.4 Shortlist page becomes a compare workspace

The shortlist page must move beyond empty state or a plain saved-card list.

It should include:

- compact shortlisted property headers/cards

- a Quick Compare section

- a Decision Themes comparison section

- a Best for... summary row or block

- visible remove action per property

This should feel closer to Levels.fyi comparison than a favorites page.

### 3.5 Greenery / open-space becomes a first-class theme

Because greenery and open space are now an important buying theme, include them explicitly in at least one of:

- property detail page

- shortlist compare themes

- area signals

A user should be able to understand whether a property/area feels:

- greener / more open

- dense / built-up

- neutral / average

This can be mocked from seed data if needed, but it must become visible in the product.

### 3.6 Reuse and preserve the Day 9 discovery flow

Do not break:

- homepage search

- results navigation

- save flow

- shortlist state

Day 10 must deepen the journey, not reset it.

## 4. Technical Guidance

### 4.1 Property detail page — target structure

Implement or refine the property detail page so it follows this structure:

#### A. Property Summary

Show:

- title

- society name

- area

- price

- BHK

- carpet area

- price/sqft

- possession status

- quick tags

This is the anchor section.

#### B. Why This Property For You

This section should now be stronger than a plain badge list.

Show:

- one short explanation paragraph or 2-line summary

- 4–6 labeled components, such as: 
  Value

- Commute & Access

- Society Quality

- Greenery / Open Space

- Document Trust

- Risk Profile

These can be labels, bars, or score bands such as:

- Strong

- Good

- Mixed

- Weak

Do not overbuild the scoring engine.
Simple deterministic computation from existing property fields and search context is enough.

#### C. Price vs Area Median

Show:

- price

- price/sqft

- area median price/sqft

- % above or below median

- short verdict label: 
  Good value

- Near market

- Premium pricing

This should visually and textually explain value.

#### D. Market Activity

This is the first Robinhood-style block.

Show mocked or structured fields such as:

- days on market

- interest level

- saves this week

- mock bid / offer count

- area transaction trend

If some of these fields do not exist yet, add lightweight mocked or seed-derived fields.

Keep it tasteful and calm.

Do not make it feel like hype or urgency manipulation.

#### E. Society / Livability

Show:

- society quality

- builder reputation

- maintenance sentiment

- common positives

- common complaints

If already implemented, refine it to be more scannable and comparison-friendly.

#### F. Area Signals

Show:

- metro access

- traffic

- waterlogging risk

- noise

- greenery / openness

- infrastructure highlights

Important: explicitly add greenery / openness as a visible signal.

#### G. Tradeoffs to Know

This section must be honest and concise.

Show:

- 2–3 strengths

- 1–2 cautions

Examples:

- “Strong value relative to Whitefield median”

- “Good metro access and strong docs”

- “Tradeoff: heavier traffic than HSR options”

- “Tradeoff: premium pricing vs other Mahadevapura listings”

This is one of the strongest trust-building sections in the whole product.

### 4.2 Shortlist compare workspace — required structure

The shortlist page should now become a real comparison tool.

Implement it in three layers.

#### Layer A — Quick Compare

A side-by-side comparison of core facts:

- price

- price/sqft

- BHK

- carpet area

- area

- possession

- society name / builder

This is the simple scan layer.

#### Layer B — Decision Themes

This is the most important part of Day 10.

Compare each shortlisted property across these themes:

- Value

- Commute & Access

- Society Quality

- Greenery & Open Space

- Risk Signals

- Resale Strength

- Market Activity

Each theme should show, per property:

- a label or score band

- a one-line explanation

Examples:

- “Below area median, strong value”

- “Metro access is good, but traffic caution”

- “Premium society, higher maintenance”

- “Greener feel, lower density”

- “Low litigation risk, medium noise”

This should not be a raw score dump.

It should feel interpretable.

#### Layer C — Best For Summary

Add a small summary block such as:

- Best for value

- Best for commute

- Best for greenery

- Best for society quality

This helps users orient quickly.

Do not over-automate it.
Simple rule-based assignment is fine for now.

### 4.3 Match + market data model additions

If needed, extend frontend types and backend response shapes carefully.

Suggested additions:

#### Property detail response additions

TypeScripttype PropertyDetailResponse = {
  property: ...
  society: ...
  area: ...
  market_activity?: {
    interest_level: "high" | "moderate" | "low"
    saves_last_7d?: number
    offers_last_7d?: number
    days_on_market?: number
    area_trend_summary?: string
  }
  compare_themes?: {
    value?: ThemeResult
    commute?: ThemeResult
    society?: ThemeResult
    greenery?: ThemeResult
    risk?: ThemeResult
    resale?: ThemeResult
  }
}

#### Theme result shape

TypeScripttype ThemeResult = {
  label: "strong" | "good" | "mixed" | "weak"
  summary: string
}
You may compute these frontend-side for Day 10 if that is faster and cleaner.

Do not overbuild the backend unless needed.

### 4.4 Seed-data support for greenery and market activity

If the current seed dataset does not support the new UI needs, extend it lightly.

Add only what is necessary.

Examples of acceptable new fields:

- greenery_score

- open_space_score

- resale_strength_score

- interest_level

- saves_last_7d

- offers_last_7d

These may be mocked or seeded manually.

Keep them structured and modest.

Do not redesign the full dataset.

### 4.5 Save / compare interaction expectations

The save flow should remain simple and local.

Use existing shortlist state if already built.

If not, continue using:

- local state

- localStorage

Expected behavior:

- save from results page

- save from property detail page

- shortlist updates immediately

- compare page can render 2–4 properties side by side

No auth.

No server persistence.

No collaboration.

### 4.6 UI guidance

The UI must stay:

- calm

- premium

- legible

- high-signal

Avoid:

- heavy tables without hierarchy

- overly dense financial-dashboard energy

- alarm-style urgency language

- too many scores in one place

Use structure and spacing to make high-stakes comparison feel calmer, not more chaotic.

The compare page should feel like:

“I can finally think clearly.”

### 4.7 Suggested file touch points

These are indicative, not mandatory.

Plain textfrontend/
  src/
    pages/
      PropertyPage.tsx
      ShortlistPage.tsx

    components/
      MatchSummaryCard.tsx
      PriceVsMedianWidget.tsx
      MarketActivityWidget.tsx
      TradeoffsWidget.tsx
      CompareTable.tsx
      CompareThemeSection.tsx
      BestForSummary.tsx

    lib/
      types.ts
      compare.ts
      market.ts
If you already have similar components, refine them instead of creating duplicates.

## 5. Constraints

Do not build today:

- full ranking engine rewrite

- full AI explanation layer

- real bid engine

- real transactional workflow

- authentication

- server-side shortlist persistence

- collaborative compare

- Google review integration

- broad data enrichment pipeline

- map view

- heavy backend refactor unless absolutely necessary

Day 10 must stay focused on:

- richer property detail reasoning

- compare workspace

- theme-based comparison

- mock market visibility

- greenery/open-space as a visible theme

Do not let this day expand into “final product” scope.

## 6. Success Criteria

Day 10 is successful if all of the following are true:

- the property detail page clearly shows: 
  why this property

- value vs area median

- market activity

- tradeoffs

- area signals

- society/livability

- the product now visibly expresses Hinge-style match framing

- the product now visibly expresses Robinhood-style market visibility in a calm way

- shortlist page supports comparing 2–4 saved properties

- compare view includes: 
  raw facts

- decision themes

- best-for summaries

- greenery / open-space appears as a real decision theme

- the save → shortlist → compare flow works end-to-end

- the UI feels more like a decision platform than a listing portal

If these conditions are met, Day 10 will mark the first point where OpenEstates feels meaningfully differentiated.

## 7. Product Decisions (what changed and why)

### Decision 1: Day 10 focuses on conviction, not more discovery breadth

Day 9 was about making discovery feel intelligent.

Day 10 deliberately shifts toward conviction surfaces:

- property detail

- compare workspace

Why:

- discovery is no longer the biggest gap

- the next big question is whether OpenEstates helps users decide, not just browse

- conviction is where transparency becomes real

### Decision 2: Introduce mocked market activity now, before real transaction systems

We are intentionally adding a light market activity layer now, even though real offers/bids are not implemented.

Why:

- Robinhood-style market legibility is a core product direction

- visible market context is useful even in mocked form

- this helps users understand that a property sits inside a market, not just inside a listing page

This is explicitly a mock visibility layer, not a real offer engine.

### Decision 3: Greenery / open-space becomes a first-class comparison theme

Greenery and openness are now important buying themes and should no longer remain implicit.

Why:

- this is becoming a real buyer preference pattern

- normal portals do not make it legible

- OpenEstates should surface this as a decision-relevant theme alongside commute, value, and risk

### Decision 4: Shortlist evolves toward a Levels.fyi-style decision workspace

The shortlist is no longer just a saved-items page.

Why:

- saved lists alone do not reduce decision friction

- side-by-side theme comparison is a stronger and more defensible product surface

- this is one of the clearest ways OpenEstates can become a serious decision tool

This is still v1 of the compare workspace, not the final version.