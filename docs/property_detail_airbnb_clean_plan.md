# Property Detail Airbnb-Clean Plan

## Goal

Rebuild the property detail page from a clean baseline with an Airbnb-like rhythm:
photos, one-line decision read, the existing map, Google reviews, recommendations, and a compact micro-market tracker. Keep the page calm by moving proof-heavy detail into focused popups.

## Page Shape

```text
PROPERTY DETAIL
────────────────────────────────────────────────────────────

Title / society / area                         Save · Note · Share
₹2.2Cr · 2 BHK · 1,165 sqft · Delivered · ★4.2 Google

[photo mosaic]                                  [Show all photos]


ONE-LINE READ
────────────────────────────────────────────────────────────
Delivered · ₹18.9k/sqft · Map checked · ★4.2 Google, maintenance mixed


MAP
────────────────────────────────────────────────────────────
[existing Around This Home map and note plumbing stays unchanged]

[Approach road]       [Market trend]
popup                 popup


GOOGLE REVIEWS
────────────────────────────────────────────────────────────
★ 4.2 · 128 Google reviews

Overall rating       Location       Maintenance       Noise/traffic       Value
simple row/bars      score/theme    score/theme       theme               theme

2-3 review/theme snippets

[Read on Google]     [Show review evidence]


MORE HOMES NEARBY
────────────────────────────────────────────────────────────
More homes nearby                                      <  1 / 2  >
[Airbnb-sized card] [Airbnb-sized card] [Airbnb-sized card] [Airbnb-sized card]


MICRO MARKET TRACKER
────────────────────────────────────────────────────────────
Explore prices around Whitefield
Hoodi        Varthur        Kadugodi        Budigere        KR Puram

compact price-band rows from the existing area tracker style
```

## Visible By Default

- Title, area, price, BHK, size, status, and Google rating.
- Photo mosaic with a single "Show all photos" affordance.
- One decision line only. Do not create a fact grid here.
- Existing map as the primary product surface.
- Two popup buttons below the map: approach road and market trend.
- Airbnb-style Google reviews section near the bottom.
- Airbnb-sized horizontal recommendations carousel.
- Compact micro-market tracker scoped around the current home area.

## Popup Surfaces

- Show all photos.
- Approach road.
- Market trend.
- Review evidence.

All popups should use the same calm overlay pattern already used by approach road: one title, one close button, focused content, no nested accordions.

## Removed For This Pass

- Financial plan CTA on the detail page. Users can use the sidebar/navigation for planning.
- RERA section and RERA button. Rebuild RERA later as a separate pass.
- Home / Project / Legal / Price tab dumps.
- Inline `EvidenceStack`.
- Nested folds or disclosure chains.
- Multi-row key-read/fact-table cards above the map.

## Recommendations

Use an Airbnb-like carousel, not large banner cards.

```text
More homes nearby                                      <  1 / 2  >
┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐
│ square img │ │ square img │ │ square img │ │ square img │
├────────────┤ ├────────────┤ ├────────────┤ ├────────────┤
│ title      │ │ title      │ │ title      │ │ title      │
│ price ★    │ │ price ★    │ │ price ★    │ │ price ★    │
└────────────┘ └────────────┘ └────────────┘ └────────────┘
```

## Micro Market Tracker

Reuse the existing landing-page area tracker language and price-band component, but change the scope.

- Landing page: macro Bengaluru markets like Marathahalli, Jayanagar, JP Nagar, Whitefield.
- Detail page: nearby micro-markets around the selected home.
- Example: a Whitefield home should show Hoodi, Varthur, Kadugodi, Budigere, KR Puram if those markets have enough priced listings.

## Implementation Notes

- Start from the clean pre-redesign property detail code.
- Preserve existing map internals and map-to-note behavior.
- Preserve notebook note anchors where they already make sense.
- Keep UI density low: if a section needs more than one short line, it probably belongs in a popup.
- After each implementation commit, run build/lint/tests and have the review buddy inspect code and UI quality.
