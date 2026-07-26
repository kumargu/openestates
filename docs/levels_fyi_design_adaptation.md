# Levels.fyi design ideas for OpenEstates

## Why this reference fits

Levels.fyi makes compensation feel like an asset market: it uses fair peer
groups, distributions, shared scales, and drill-down evidence instead of a
single headline number. OpenEstates can use the same visual grammar for homes
without copying Levels.fyi's product or visual identity.

The transfer is:

> Turn a complex decision into a fair comparison, show the shape behind the
> summary, and let every conclusion open into receipts.

For OpenEstates, the durable truth still flows:

```text
crawl/source input -> normalize -> DAG asset -> serving bundle -> Rust API -> UI
```

Levels.fyi is inspiration for visualization and explanation, not a new source
of facts or a reason to bypass the DAG.

## Design principles to adapt

### 1. Distribution over one average

A median price hides the useful part of a market. When enough comparable facts
exist, show:

- p25-p75 as the typical band
- median as the primary marker
- p10-p90 as floor and stretch
- sample size, freshness, and peer-set definition beside the chart

Use this for price per square foot, total price, all-in cost, commute, and other
dimensions where a range is more honest than a point estimate.

### 2. Fair cuts are part of the interface

Every aggregate should state its comparison set. A useful caption looks like:

`3BHK · 1,400-2,000 sqft · ready/near-ready · last 6 months · n=31`

The cut can include BHK, area, possession state, size range, time window, and
minimum observations. If the cut is weak, reduce the claim or omit the chart.
Do not manufacture a confident range from thin evidence.

### 3. Shared scales reveal tradeoffs

Compare homes on one axis instead of placing isolated cards side by side.
Useful shared scales include:

- expected all-in cost and uncertainty range
- office commute
- price versus comparable market band
- proof strength
- maintenance or monthly burden

The point is not to crown a universal winner. It is to make the cost of each
tradeoff visible.

### 4. Maps encode value and evidence separately

For Area Tracker:

- color can represent median price, price movement, or proof strength
- bubble size can represent society coverage, listing density, or fact count
- city scale can use regions; closer zoom can reveal society bubbles

Never let size imply quality when it actually means coverage. Legends and
captions must name both encodings.

### 5. Headline price becomes lived cost

The property ask is analogous to base compensation: important but incomplete.
Plan and compare surfaces can decompose:

- base price
- registration and stamp duty
- parking and club charges
- likely fit-out
- EMI interest
- maintenance and annual ownership costs
- commute time or commute-adjusted burden

Derived values must name their assumptions and link back to source facts.

### 6. Aggregates drill into receipts

Every market band, proof score, and comparison claim should open into the
underlying evidence: RERA, listing proof, Google, transaction or comparable
facts, civic sources, and future validated contributions. Show source,
freshness, confidence, and scope.

### 7. One claim, one chart

Lead with a decision-relevant observation, then let one visualization prove it.
Examples:

- "Whitefield is not uniformly expensive; project choice explains the spread."
- "Same budget, different commute and proof."
- "This home's ask is below the middle of comparable ready 3BHKs."

Avoid dashboards made of unrelated metrics. Each visual needs a clear buyer
question.

### 8. Corrections are a trust feature

Prefer visible language such as `Updated after RERA refresh` or
`Comparable set changed` over silently changing a number. Provenance and
versioned serving bundles make corrections explainable.

## Levels.fyi pattern to OpenEstates surface

| Levels.fyi pattern | OpenEstates adaptation | Primary surface |
| --- | --- | --- |
| Compensation distribution | Asking-price or all-in-cost band | Area, society, property |
| p10 / median / p90 controls | Floor / typical / stretch market view | Area Tracker |
| Bubble size = observations | Bubble size = society or fact coverage | Area Tracker |
| Same title, different pay | Same BHK and budget, different outcomes | Shortlist compare |
| Base / equity / bonus stack | Price / fees / interest / maintenance stack | Plan |
| Cost-of-living adjustment | Commute- or monthly-burden adjustment | Plan and compare |
| Company peer set | Society, builder, and possession peer set | Property and society |
| Drill to compensation rows | Drill to sourced facts and receipts | Evidence panels |
| Data Explorer | Intent query to deterministic local cut and chart | Search and Area Tracker |
| Place-first Atlas | Society-first market map with transit context | Area Tracker, later |

## Recommended product sequence

1. **Price bands first.** Add p25-median-p75 and sample/freshness captions where
   comparable data is already credible.
2. **Property market position.** Place a home on the same area/society price
   band and explain the major drivers.
3. **Shared-scale shortlist comparison.** Compare all-in cost, commute, and
   proof without repeating isolated cards.
4. **Area Tracker dual encoding.** Use price and evidence coverage as separate,
   explicit map dimensions.
5. **Lived monthly cost.** Extend Plan with ownership and commute burden.
6. **Atlas-like place experience later.** Add richer geographic atmosphere only
   after society-level coverage and lineages are dense.

## Guardrails

- Adapt the decision grammar; do not clone Levels.fyi branding or page layouts.
- Use calm OpenEstates typography, spacing, and evidence language.
- Do not show a distribution without a valid peer set and minimum sample.
- Do not use a live LLM to produce request-path charts.
- Do not turn missing evidence into a fake midpoint or default score.
- Keep each signal on one primary surface, with drill-down rather than
  repetition.
- Treat maps as context, not decoration; facts remain the spine.
- Mark mock values as illustrative until serving-bundle facts back them.

## Original concept artifacts

The ideas were explored in the Cursor canvases
`levels-fyi-viz-ideas.canvas.tsx` and
`levels-fyi-openestates-mocks.canvas.tsx`. This document is the stable product
record to use when those temporary visual artifacts are unavailable.
